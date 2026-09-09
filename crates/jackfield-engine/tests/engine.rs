//! The engine loop, driven by a fabricated fetcher and fabricated
//! advertisements. No network, no timing luck.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use jackfield_engine::{
    Command, Connector, Engine, EngineConfig, EngineHandle, FabricatedDiscovery, Fetcher,
    KnownNode, NodeKey, NodeState, Requested, SenderView, Snapshot, StreamSource,
};
use nmos::{
    ApiVersion, CollectionData, NodeCollection, ReceiverTransport, ResourceId, ResourceTree,
    SenderTransport, StreamAddress,
};
use support::{TreeBuilder, advertisement, bench_tree, device, receiver, sender, transmitting};
use tokio::sync::Mutex;

/// What a fabricated fetcher was asked for.
#[derive(Debug, Default)]
struct Record {
    trees: Vec<String>,
    collections: Vec<(String, NodeCollection)>,
    sender_transports: Vec<ResourceId>,
    receiver_transports: Vec<ResourceId>,
}

/// A fetcher that answers from a script.
struct Fabricated {
    record: Arc<Mutex<Record>>,
    /// Bases that never answer, so a stalled Node can be simulated exactly.
    stalls: Vec<String>,
    /// Bases that fail, and why.
    failures: BTreeMap<String, String>,
    tree: ResourceTree,
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    /// Held while a fetch is in flight, so the test can hold every fetch open.
    gate: Arc<tokio::sync::Semaphore>,
    gated: bool,
}

impl Fabricated {
    fn new(tree: ResourceTree) -> Self {
        Self {
            record: Arc::new(Mutex::new(Record::default())),
            stalls: Vec::new(),
            failures: BTreeMap::new(),
            tree,
            in_flight: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            gate: Arc::new(tokio::sync::Semaphore::new(0)),
            gated: false,
        }
    }

    async fn enter(&self, base: &str) {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        if self.gated {
            // Never granted: the test wants every fetch held open.
            let _ = self.gate.acquire().await;
        }
        if self.stalls.iter().any(|stalled| base.contains(stalled)) {
            std::future::pending::<()>().await;
        }
    }

    fn leave(&self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }

    fn failure_for(&self, base: &str) -> Option<String> {
        self.failures
            .iter()
            .find(|(pattern, _)| base.contains(pattern.as_str()))
            .map(|(_, reason)| reason.clone())
    }
}

impl Fetcher for Fabricated {
    async fn fetch_tree(
        &self,
        base: String,
        _versions: Vec<ApiVersion>,
    ) -> Result<ResourceTree, String> {
        self.record.lock().await.trees.push(base.clone());
        self.enter(&base).await;
        let answer = match self.failure_for(&base) {
            Some(reason) => Err(reason),
            None => Ok(self.tree.clone()),
        };
        self.leave();
        answer
    }

    async fn fetch_collection(
        &self,
        base: String,
        _versions: Vec<ApiVersion>,
        collection: NodeCollection,
    ) -> Result<CollectionData, String> {
        self.record
            .lock()
            .await
            .collections
            .push((base.clone(), collection));
        self.enter(&base).await;
        let answer = match collection {
            NodeCollection::Node => CollectionData::Node(Box::new(self.tree.node.clone())),
            NodeCollection::Devices => CollectionData::Devices(self.tree.devices.clone()),
            NodeCollection::Senders => CollectionData::Senders(self.tree.senders.clone()),
            NodeCollection::Receivers => CollectionData::Receivers(self.tree.receivers.clone()),
            NodeCollection::Flows => CollectionData::Flows(self.tree.flows.clone()),
            NodeCollection::Sources => CollectionData::Sources(self.tree.sources.clone()),
        };
        self.leave();
        Ok(answer)
    }

    async fn fetch_sender_transport(
        &self,
        _base: String,
        id: ResourceId,
    ) -> Result<SenderTransport, String> {
        self.record.lock().await.sender_transports.push(id);
        Ok(SenderTransport {
            receiver_id: None,
            legs: Vec::new(),
        })
    }

    async fn fetch_receiver_transport(
        &self,
        _base: String,
        id: ResourceId,
    ) -> Result<ReceiverTransport, String> {
        self.record.lock().await.receiver_transports.push(id);
        Ok(ReceiverTransport {
            sender_id: None,
            legs: Vec::new(),
            has_transport_file: false,
        })
    }
}

/// Wait until a snapshot satisfies `ready`, or give up.
async fn until(handle: &EngineHandle, ready: impl Fn(&Snapshot) -> bool) -> Option<Arc<Snapshot>> {
    let mut snapshots = handle.snapshots();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            {
                let snapshot = handle.snapshot();
                if ready(&snapshot) {
                    return snapshot;
                }
            }
            if snapshots.changed().await.is_err() {
                return handle.snapshot();
            }
        }
    })
    .await
    .ok()
}

/// One thing a connector was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Transmitting {
        base: String,
        sender: ResourceId,
        on: bool,
    },
    Subscribe {
        base: String,
        receiver: ResourceId,
        sender: ResourceId,
        source: StreamSource,
    },
    Unsubscribe {
        base: String,
        receiver: ResourceId,
    },
    TransportFile {
        base: String,
        sender: ResourceId,
    },
    Streams {
        base: String,
        sender: ResourceId,
    },
}

/// A connector that records what it was asked to do, and can be told to refuse
/// or to never answer at all.
#[derive(Default, Clone)]
struct Connections {
    calls: Arc<Mutex<Vec<Call>>>,
    /// What the device says when it refuses.
    refusal: Option<String>,
    /// What the Sender's transport file contains.
    transport_file: Option<String>,
    /// Where the Sender's Connection API says the stream goes.
    streams: Vec<StreamAddress>,
    /// What the Sender's Connection API says instead of that.
    streams_refusal: Option<String>,
    /// Whether writes hang, so the in-flight state can be seen.
    stalled: bool,
}

impl Connections {
    async fn record(&self, call: Call) -> Result<(), String> {
        self.calls.lock().await.push(call);
        if self.stalled {
            std::future::pending::<()>().await;
        }
        match &self.refusal {
            Some(reason) => Err(reason.clone()),
            None => Ok(()),
        }
    }
}

impl Connector for Connections {
    async fn set_transmitting(
        &self,
        base: String,
        sender: ResourceId,
        on: bool,
    ) -> Result<(), String> {
        self.record(Call::Transmitting { base, sender, on }).await
    }

    async fn subscribe(
        &self,
        base: String,
        receiver: ResourceId,
        sender: ResourceId,
        source: StreamSource,
    ) -> Result<(), String> {
        self.record(Call::Subscribe {
            base,
            receiver,
            sender,
            source,
        })
        .await
    }

    async fn unsubscribe(&self, base: String, receiver: ResourceId) -> Result<(), String> {
        self.record(Call::Unsubscribe { base, receiver }).await
    }

    async fn fetch_transport_file(
        &self,
        base: String,
        sender: ResourceId,
    ) -> Result<Option<String>, String> {
        self.calls
            .lock()
            .await
            .push(Call::TransportFile { base, sender });
        Ok(self.transport_file.clone())
    }

    async fn fetch_streams(
        &self,
        base: String,
        sender: ResourceId,
    ) -> Result<Vec<StreamAddress>, String> {
        self.calls.lock().await.push(Call::Streams { base, sender });
        match &self.streams_refusal {
            Some(reason) => Err(reason.clone()),
            None => Ok(self.streams.clone()),
        }
    }
}

/// The first Sender of the first Device, which every write test acts on.
fn first_sender(snapshot: &Snapshot) -> &SenderView {
    &snapshot.nodes[0].devices()[0].senders[0]
}

/// What has been asked of one Receiver, wherever it is.
fn requested_receiver<'a>(snapshot: &'a Snapshot, id: &ResourceId) -> Option<&'a Requested> {
    snapshot
        .nodes
        .iter()
        .flat_map(KnownNode::devices)
        .flat_map(|device| &device.receivers)
        .find(|receiver| &receiver.receiver.core.id == id)
        .and_then(|receiver| receiver.requested.as_ref())
}

fn key_of(snapshot: &Snapshot) -> NodeKey {
    snapshot.nodes[0].key.clone()
}

fn config() -> EngineConfig {
    EngineConfig {
        concurrency: 8,
        transport_floor: Duration::from_millis(120),
    }
}

#[tokio::test]
async fn a_discovered_node_is_fetched_and_published() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let record = Arc::clone(&fetcher.record);
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));

    let snapshot = until(&handle, |s| {
        s.nodes.iter().any(|node| node.state.is_ready())
    })
    .await
    .expect("the Node is read");

    assert_eq!(snapshot.nodes.len(), 1);
    assert_eq!(snapshot.nodes[0].devices().len(), 3);
    assert_eq!(record.lock().await.trees.len(), 1);
}

#[tokio::test]
async fn a_node_that_fails_is_kept_with_its_reason_and_the_others_are_fine() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let mut fetcher = Fabricated::new(bench_tree());
    fetcher
        .failures
        .insert("10.77.1.91".to_owned(), "connection refused".to_owned());
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    discovery.advertise(advertisement("healthy", [10, 77, 1, 90], 8090));
    discovery.advertise(advertisement("broken", [10, 77, 1, 91], 8090));

    let snapshot = until(&handle, |s| {
        s.nodes.len() == 2
            && s.nodes
                .iter()
                .all(|n| !matches!(n.state, NodeState::Loading))
    })
    .await
    .expect("both Nodes settle");

    let failed = snapshot
        .nodes
        .iter()
        .find(|n| n.state.failure().is_some())
        .expect("one failed");
    assert_eq!(failed.state.failure(), Some("connection refused"));
    assert!(
        snapshot.nodes.iter().any(|n| n.state.is_ready()),
        "the other is browsable"
    );
}

#[tokio::test]
async fn at_most_the_bound_are_fetched_at_once() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let mut fetcher = Fabricated::new(bench_tree());
    fetcher.gated = true;
    let peak = Arc::clone(&fetcher.peak);
    let in_flight = Arc::clone(&fetcher.in_flight);

    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        EngineConfig {
            concurrency: 8,
            ..config()
        },
    )
    .spawn();

    for n in 0..20u8 {
        discovery.advertise(advertisement(&format!("node-{n}"), [10, 77, 1, n], 8090));
    }

    // Every fetch is held open, so what is in flight settles at the bound.
    let settled = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if in_flight.load(Ordering::SeqCst) >= 8 {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(settled.is_ok(), "the engine never reached its bound");

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        peak.load(Ordering::SeqCst) <= 8,
        "{} Nodes were fetched at once, above the bound of eight",
        peak.load(Ordering::SeqCst)
    );
    drop(handle);
}

#[tokio::test]
async fn one_stalled_node_delays_no_other() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let mut fetcher = Fabricated::new(bench_tree());
    fetcher.stalls.push("10.77.1.99".to_owned());
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    discovery.advertise(advertisement("stalled", [10, 77, 1, 99], 8090));
    discovery.advertise(advertisement("healthy", [10, 77, 1, 90], 8090));

    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the healthy Node answers regardless");

    assert!(snapshot.nodes.iter().any(|n| n.state.is_ready()));
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|n| matches!(n.state, NodeState::Loading)),
        "the stalled Node is still shown, as loading"
    );
}

#[tokio::test]
async fn only_the_collection_whose_counter_moved_is_re_read() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let record = Arc::clone(&fetcher.record);
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    let mut txt: BTreeMap<String, String> = BTreeMap::new();
    txt.insert("ver_rcv".to_owned(), "9".to_owned());
    discovery.advertise(with_txt("converter", [10, 77, 1, 90], &txt));

    until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node is read");

    txt.insert("ver_rcv".to_owned(), "10".to_owned());
    discovery.advertise(with_txt("converter", [10, 77, 1, 90], &txt));

    let asked = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !record.lock().await.collections.is_empty() {
                return record.lock().await.collections.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the moved counter provoked a re-read");

    assert_eq!(
        asked.iter().map(|(_, c)| *c).collect::<Vec<_>>(),
        [NodeCollection::Receivers],
        "only Receivers moved, so only Receivers are re-read"
    );
    assert_eq!(
        record.lock().await.trees.len(),
        1,
        "the whole tree is not re-read"
    );
}

#[tokio::test]
async fn an_unchanged_node_is_left_alone() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let record = Arc::clone(&fetcher.record);
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    let advert = advertisement("converter", [10, 77, 1, 90], 8090);
    discovery.advertise(advert.clone());
    until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready())).await;

    discovery.advertise(advert);
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(record.lock().await.trees.len(), 1);
    assert!(record.lock().await.collections.is_empty());
}

#[tokio::test]
async fn an_explicit_refresh_re_reads_in_full_whatever_the_counters_say() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let record = Arc::clone(&fetcher.record);
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));
    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("read once");

    handle
        .send(Command::Refresh(snapshot.nodes[0].key.clone()))
        .await
        .expect("sent");

    let read_twice = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if record.lock().await.trees.len() >= 2 {
                return true;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        read_twice.is_ok(),
        "an explicit refresh must re-read regardless"
    );
}

#[tokio::test]
async fn the_transport_pass_runs_even_when_no_counter_ever_moves() {
    // The floor. A Node whose counters are never maintained still converges.
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(
        TreeBuilder::new(1, "Converter")
            .device(device(10, "SDI 1", 1))
            .sender(sender(100, "SDI 1", 10, None))
            .build(),
    );
    let record = Arc::clone(&fetcher.record);
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        EngineConfig {
            concurrency: 8,
            transport_floor: Duration::from_millis(60),
        },
    )
    .spawn();

    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));

    let ran = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !record.lock().await.sender_transports.is_empty() {
                return true;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(ran.is_ok(), "the periodic pass never ran");
    drop(handle);
}

#[tokio::test]
async fn a_node_discovered_before_the_engine_started_is_not_missed() {
    // Discovery starts before the engine does; a Node found in that moment must
    // arrive in the subscription's snapshot rather than being lost.
    let discovery = Arc::new(FabricatedDiscovery::new());
    discovery.advertise(advertisement("early", [10, 77, 1, 90], 8090));

    let fetcher = Fabricated::new(bench_tree());
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node found before the engine started is read");
    assert_eq!(snapshot.nodes.len(), 1);
}

#[tokio::test]
async fn a_departed_node_stops_being_listed() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));
    until(&handle, |s| !s.nodes.is_empty())
        .await
        .expect("listed");

    discovery.withdraw("converter");
    let snapshot = until(&handle, |s| s.nodes.is_empty())
        .await
        .expect("delisted");
    assert!(snapshot.nodes.is_empty());
}

#[tokio::test]
async fn stopping_the_engine_ends_it() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let handle = Engine::new(
        discovery_with(&discovery),
        fetcher,
        Connections::default(),
        config(),
    )
    .spawn();

    handle.send(Command::Stop).await.expect("sent");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        handle.send(Command::RefreshAll).await.is_err(),
        "the engine has stopped"
    );
}

// --- helpers ----------------------------------------------------------------

/// The engine takes the discovery source by value; the test keeps a handle on
/// it to drive it, so a thin sharing wrapper stands between them.
fn discovery_with(discovery: &Arc<FabricatedDiscovery>) -> Shared {
    Shared(Arc::clone(discovery))
}

struct Shared(Arc<FabricatedDiscovery>);

impl jackfield_engine::Discovery for Shared {
    fn subscribe(&self) -> jackfield_engine::Subscription {
        self.0.subscribe()
    }
}

fn with_txt(
    instance: &str,
    address: [u8; 4],
    txt: &BTreeMap<String, String>,
) -> jackfield_engine::Advertisement {
    use std::net::{IpAddr, Ipv4Addr};
    jackfield_engine::Advertisement::try_from_parts(
        instance,
        Some(format!("{instance}.local.")),
        &[IpAddr::V4(Ipv4Addr::from(address))],
        8090,
        txt,
    )
    .expect("usable")
}

// --- changing a device ------------------------------------------------------

/// An engine over the bench tree with a discovered Node, ready to be written to.
async fn ready_engine(
    connections: Connections,
) -> (Arc<FabricatedDiscovery>, EngineHandle, Arc<Mutex<Record>>) {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let record = Arc::clone(&fetcher.record);
    let handle = Engine::new(discovery_with(&discovery), fetcher, connections, config()).spawn();
    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));
    until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node is read");
    (discovery, handle, record)
}

#[tokio::test]
async fn a_sender_is_put_on_air_at_the_connection_api_of_its_own_device() {
    let connections = Connections::default();
    let calls = Arc::clone(&connections.calls);
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let sender = first_sender(&snapshot).sender.core.id.clone();
    handle
        .send(Command::StartTransmitting {
            node: key_of(&snapshot),
            sender: sender.clone(),
        })
        .await
        .expect("the engine is running");

    let recorded = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(call) = calls.lock().await.first().cloned() {
                return call;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the write reaches the device");

    assert_eq!(
        recorded,
        Call::Transmitting {
            base: "http://10.77.1.90:8090".to_owned(),
            sender,
            on: true,
        }
    );
}

#[tokio::test]
async fn a_request_is_visible_before_the_device_answers() {
    // The two clocks of ADR-0005 mean an observation can lag a write by up to a
    // minute. Without this the operator's keystroke would appear to do nothing.
    let connections = Connections {
        stalled: true,
        ..Connections::default()
    };
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let sender = first_sender(&snapshot).sender.core.id.clone();
    handle
        .send(Command::StartTransmitting {
            node: key_of(&snapshot),
            sender,
        })
        .await
        .expect("the engine is running");

    let snapshot = until(&handle, |s| first_sender(s).requested.is_some())
        .await
        .expect("the request shows");

    assert_eq!(
        first_sender(&snapshot).requested,
        Some(Requested::Transmitting)
    );
}

#[tokio::test]
async fn a_refusal_is_held_against_the_resource_that_was_refused() {
    let connections = Connections {
        refusal: Some("destination_ip is not multicast".to_owned()),
        ..Connections::default()
    };
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let sender = first_sender(&snapshot).sender.core.id.clone();
    handle
        .send(Command::StartTransmitting {
            node: key_of(&snapshot),
            sender,
        })
        .await
        .expect("the engine is running");

    let snapshot = until(&handle, |s| {
        matches!(first_sender(s).requested, Some(Requested::Refused(_)))
    })
    .await
    .expect("the refusal shows");

    assert_eq!(
        first_sender(&snapshot).requested,
        Some(Requested::Refused(
            "destination_ip is not multicast".to_owned()
        ))
    );
}

#[tokio::test]
async fn a_write_the_node_confirms_stops_being_a_request() {
    // The engine reads the Node back after a write. Once that read lands it is
    // the observation that speaks, and the request has nothing left to say.
    let connections = Connections::default();
    let (_discovery, handle, record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let sender = first_sender(&snapshot).sender.core.id.clone();
    handle
        .send(Command::StartTransmitting {
            node: key_of(&snapshot),
            sender,
        })
        .await
        .expect("the engine is running");

    // Two trees read: the discovery one, and the confirming one.
    let confirmed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if record.lock().await.trees.len() >= 2 {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(confirmed.is_ok(), "the write was never confirmed by a read");

    let snapshot = until(&handle, |s| first_sender(s).requested.is_none())
        .await
        .expect("the request clears");
    assert_eq!(first_sender(&snapshot).requested, None);
}

#[tokio::test]
async fn a_device_with_no_connection_api_refuses_without_a_request_being_sent() {
    // Nothing is written into the dark: a Device that advertises no Connection
    // API is not controllable, and the operator is told so.
    let discovery = Arc::new(FabricatedDiscovery::new());
    let mut plain = device(10, "SDI 1", 1);
    plain.controls.clear();
    let tree = TreeBuilder::new(1, "Converter")
        .device(plain)
        .sender(sender(100, "SDI 1", 10, None))
        .build();
    let fetcher = Fabricated::new(tree);
    let connections = Connections::default();
    let calls = Arc::clone(&connections.calls);
    let handle = Engine::new(discovery_with(&discovery), fetcher, connections, config()).spawn();
    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));
    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node is read");

    let sender_id = first_sender(&snapshot).sender.core.id.clone();
    handle
        .send(Command::StartTransmitting {
            node: key_of(&snapshot),
            sender: sender_id,
        })
        .await
        .expect("the engine is running");

    let snapshot = until(&handle, |s| first_sender(s).requested.is_some())
        .await
        .expect("the refusal shows");

    assert!(matches!(
        first_sender(&snapshot).requested,
        Some(Requested::Refused(_))
    ));
    assert!(
        calls.lock().await.is_empty(),
        "nothing was sent to a device that cannot be controlled"
    );
}

#[tokio::test]
async fn subscribing_hands_the_receiver_the_senders_own_transport_file() {
    // IS-05 expects the media description to travel from Sender to Receiver.
    // The controller reads it and passes it through; it does not compose one.
    let connections = Connections {
        transport_file: Some("v=0\r\n".to_owned()),
        streams: vec![StreamAddress {
            address: "239.255.2.190".to_owned(),
            port: 16388,
        }],
        ..Connections::default()
    };
    let calls = Arc::clone(&connections.calls);
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let key = key_of(&snapshot);
    let sender = first_sender(&snapshot).sender.core.id.clone();
    let receiver = snapshot.nodes[0].devices()[0].receivers[0]
        .receiver
        .core
        .id
        .clone();

    handle
        .send(Command::Subscribe {
            node: key.clone(),
            receiver: receiver.clone(),
            from: key,
            sender: sender.clone(),
        })
        .await
        .expect("the engine is running");

    let recorded = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let calls = calls.lock().await.clone();
            if calls.len() >= 3 {
                return calls;
            }
            drop(calls);
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all three requests are made");

    assert_eq!(
        recorded[0],
        Call::TransportFile {
            base: "http://10.77.1.90:8090".to_owned(),
            sender: sender.clone(),
        },
        "the Sender's file is read first"
    );
    assert_eq!(
        recorded[1],
        Call::Streams {
            base: "http://10.77.1.90:8090".to_owned(),
            sender: sender.clone(),
        },
        "and where it is sending, now rather than from the last pass"
    );
    assert_eq!(
        recorded[2],
        Call::Subscribe {
            base: "http://10.77.1.90:8090".to_owned(),
            receiver,
            sender,
            source: StreamSource::TransportFile {
                data: "v=0\r\n".to_owned(),
                streams: vec![StreamAddress {
                    address: "239.255.2.190".to_owned(),
                    port: 16388,
                }],
            },
        }
    );
}

#[tokio::test]
async fn a_sender_that_will_not_say_where_it_sends_falls_back_to_the_last_pass() {
    // The Connection API can refuse the second read — a device under load, a
    // vendor that answers `/active` slowly — and a take that gave up there
    // would be a take lost to a detail the operator cannot see.
    let connections = Connections {
        transport_file: Some("v=0\r\n".to_owned()),
        streams_refusal: Some("the device is busy".to_owned()),
        ..Connections::default()
    };
    let calls = Arc::clone(&connections.calls);
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let key = key_of(&snapshot);
    let sender = first_sender(&snapshot).sender.core.id.clone();
    let receiver = snapshot.nodes[0].devices()[0].receivers[0]
        .receiver
        .core
        .id
        .clone();

    handle
        .send(Command::Subscribe {
            node: key.clone(),
            receiver: receiver.clone(),
            from: key,
            sender: sender.clone(),
        })
        .await
        .expect("the engine is running");

    let recorded = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let calls = calls.lock().await.clone();
            if calls
                .iter()
                .any(|call| matches!(call, Call::Subscribe { .. }))
            {
                return calls;
            }
            drop(calls);
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the take still reaches the device");

    assert_eq!(
        recorded.last(),
        Some(&Call::Subscribe {
            base: "http://10.77.1.90:8090".to_owned(),
            receiver,
            sender,
            source: StreamSource::TransportFile {
                data: "v=0\r\n".to_owned(),
                streams: Vec::new(),
            },
        }),
        "the file still travels, with whatever addresses were already known"
    );
}

#[tokio::test]
async fn unsubscribing_reaches_the_receivers_own_connection_api() {
    let connections = Connections::default();
    let calls = Arc::clone(&connections.calls);
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let receiver = snapshot.nodes[0].devices()[0].receivers[0]
        .receiver
        .core
        .id
        .clone();

    handle
        .send(Command::Unsubscribe {
            node: key_of(&snapshot),
            receiver: receiver.clone(),
        })
        .await
        .expect("the engine is running");

    let recorded = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(call) = calls.lock().await.first().cloned() {
                return call;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the write reaches the device");

    assert_eq!(
        recorded,
        Call::Unsubscribe {
            base: "http://10.77.1.90:8090".to_owned(),
            receiver,
        }
    );
}

// --- what a Receiver can take -----------------------------------------------

#[tokio::test]
async fn a_take_the_receiver_does_not_accept_is_refused_before_anything_is_written() {
    // The bench case: the Ancillary Sender of SDI 1 marked, and the Receiver
    // that takes video/raw pointed at it. A Node may refuse this in whatever
    // words it likes — or accept it and pass rubbish — and neither is a good
    // way for an operator to find out.
    let connections = Connections {
        transport_file: Some("v=0\r\n".to_owned()),
        ..Connections::default()
    };
    let calls = Arc::clone(&connections.calls);
    let (_discovery, handle, _record) = ready_engine(connections).await;

    let snapshot = handle.snapshot();
    let key = key_of(&snapshot);
    let device = &snapshot.nodes[0].devices()[0];
    // The bench Device carries video/raw, audio/L24 and video/smpte291, in
    // that order, on both sides.
    let ancillary = device.senders[2].sender.core.id.clone();
    let video = device.receivers[0].receiver.core.id.clone();

    handle
        .send(Command::Subscribe {
            node: key.clone(),
            receiver: video.clone(),
            from: key,
            sender: ancillary,
        })
        .await
        .expect("the engine is running");

    let snapshot = until(&handle, |s| {
        matches!(requested_receiver(s, &video), Some(Requested::Refused(_)))
    })
    .await
    .expect("the refusal shows");

    let Some(Requested::Refused(reason)) = requested_receiver(&snapshot, &video) else {
        panic!("the take was not refused");
    };
    assert!(
        reason.contains("video/smpte291") && reason.contains("video/raw"),
        "the refusal names neither end: {reason}"
    );
    assert!(
        calls.lock().await.is_empty(),
        "a refused take still went to the wire: {:?}",
        calls.lock().await
    );
}

#[tokio::test]
async fn a_sender_whose_flow_has_not_been_read_is_not_second_guessed() {
    // No Flow means no media type, which is ignorance, not evidence. Refusing
    // on it would make the controller the thing standing between an operator
    // and equipment that would have accepted the take.
    let discovery = Arc::new(FabricatedDiscovery::new());
    let tree = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .sender(transmitting(sender(100, "SDI 1", 10, None)))
        .receiver(receiver(400, "SDI 1", 10, "video/raw"))
        .build();
    let connections = Connections {
        transport_file: Some("v=0\r\n".to_owned()),
        ..Connections::default()
    };
    let calls = Arc::clone(&connections.calls);
    let handle = Engine::new(
        discovery_with(&discovery),
        Fabricated::new(tree),
        connections,
        config(),
    )
    .spawn();
    discovery.advertise(advertisement("converter", [10, 77, 1, 90], 8090));
    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node is read");

    let key = key_of(&snapshot);
    let device = &snapshot.nodes[0].devices()[0];
    let sender_id = device.senders[0].sender.core.id.clone();
    let receiver_id = device.receivers[0].receiver.core.id.clone();

    handle
        .send(Command::Subscribe {
            node: key.clone(),
            receiver: receiver_id,
            from: key,
            sender: sender_id,
        })
        .await
        .expect("the engine is running");

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !calls.lock().await.is_empty() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the take reaches the device");
}
