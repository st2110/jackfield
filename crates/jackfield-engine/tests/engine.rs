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
    Command, Engine, EngineConfig, EngineHandle, FabricatedDiscovery, Fetcher, NodeState, Snapshot,
};
use nmos::{
    ApiVersion, CollectionData, NodeCollection, ReceiverTransport, ResourceId, ResourceTree,
    SenderTransport,
};
use support::{TreeBuilder, advertisement, bench_tree, device, sender};
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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node found before the engine started is read");
    assert_eq!(snapshot.nodes.len(), 1);
}

#[tokio::test]
async fn a_departed_node_stops_being_listed() {
    let discovery = Arc::new(FabricatedDiscovery::new());
    let fetcher = Fabricated::new(bench_tree());
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
    let handle = Engine::new(discovery_with(&discovery), fetcher, config()).spawn();

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
