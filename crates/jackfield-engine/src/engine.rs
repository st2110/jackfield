//! The loop that keeps the inventory following the network.
//!
//! Everything the interface sees comes through here, and it sees it as a
//! snapshot on a channel; everything the interface wants goes back as a
//! command on another. That seam is what keeps the domain testable headless
//! and leaves room for a second front end — see
//! `docs/adr/0003-crate-split-engine-owns-state.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use nmos::{
    ApiVersion, CollectionData, NodeCollection, ReceiverTransport, ResourceId, ResourceTree,
    SenderTransport,
};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::discovery::{Advertisement, Discovery, DiscoveryEvent};
use crate::identity::NodeKey;
use crate::inventory::{Inventory, KnownNode};

/// How many Nodes are read at once.
///
/// Requests within a Node are sequential — small equipment answers one at a
/// time — and this bounds how many Nodes that happens for. Discovering a large
/// plant must not flood it.
pub const FETCH_CONCURRENCY: usize = 8;

/// How often the transport pass runs for a Node regardless of its counters.
///
/// The counter mechanism is only as good as the vendor's implementation of it,
/// and this project has deliberately not verified that by writing to live
/// hardware. The floor converges anyway, at the cost of latency on such Nodes.
pub const TRANSPORT_FLOOR: Duration = Duration::from_secs(60);

/// What the interface asks the engine to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Re-read one Node in full, whatever its counters say.
    Refresh(NodeKey),
    /// Re-read every Node in full.
    RefreshAll,
    /// Stop.
    Stop,
}

/// The inventory as of one moment.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// Every Node known, in a stable order.
    pub nodes: Vec<KnownNode>,
    /// Increments with every published snapshot, so a subscriber can tell two
    /// identical-looking ones apart.
    pub generation: u64,
}

/// What the engine needs from the network.
///
/// A trait so the engine can be driven by fabricated answers in a test and by
/// HTTP in the binary. Nothing here writes to a device.
pub trait Fetcher: Send + Sync + 'static {
    /// Read a Node's whole tree.
    fn fetch_tree(
        &self,
        base: String,
        versions: Vec<ApiVersion>,
    ) -> impl Future<Output = Result<ResourceTree, String>> + Send;

    /// Read one collection of a Node's tree.
    fn fetch_collection(
        &self,
        base: String,
        versions: Vec<ApiVersion>,
        collection: NodeCollection,
    ) -> impl Future<Output = Result<CollectionData, String>> + Send;

    /// Read where a Sender's stream goes.
    fn fetch_sender_transport(
        &self,
        base: String,
        id: ResourceId,
    ) -> impl Future<Output = Result<SenderTransport, String>> + Send;

    /// Read which stream a Receiver takes.
    fn fetch_receiver_transport(
        &self,
        base: String,
        id: ResourceId,
    ) -> impl Future<Output = Result<ReceiverTransport, String>> + Send;
}

/// How to reach the engine.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    commands: mpsc::Sender<Command>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
}

impl EngineHandle {
    /// Ask the engine to do something.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine has stopped.
    pub async fn send(&self, command: Command) -> Result<(), &'static str> {
        self.commands
            .send(command)
            .await
            .map_err(|_| "the engine has stopped")
    }

    /// The current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshots.borrow())
    }

    /// A view that can be awaited for the next snapshot.
    #[must_use]
    pub fn snapshots(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshots.clone()
    }
}

/// One unit of work the engine has outstanding.
#[derive(Debug)]
enum Done {
    Tree(NodeKey, Box<Result<ResourceTree, String>>),
    Collection(NodeKey, Box<Result<CollectionData, String>>),
    SenderTransport(ResourceId, Box<Result<SenderTransport, String>>),
    ReceiverTransport(ResourceId, Box<Result<ReceiverTransport, String>>),
}

/// What the engine still has to do.
#[derive(Debug, Default)]
struct Pending {
    trees: BTreeSet<NodeKey>,
    collections: BTreeSet<(NodeKey, NodeCollection)>,
    transports: BTreeSet<NodeKey>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.trees.is_empty() && self.collections.is_empty() && self.transports.is_empty()
    }
}

/// How the engine behaves. Separated out so tests can shorten the floor.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    /// How many Nodes are read at once.
    pub concurrency: usize,
    /// How often the transport pass runs regardless of counters.
    pub transport_floor: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            concurrency: FETCH_CONCURRENCY,
            transport_floor: TRANSPORT_FLOOR,
        }
    }
}

/// Runs the inventory.
pub struct Engine<D, F> {
    discovery: D,
    fetcher: Arc<F>,
    config: EngineConfig,
}

impl<D, F> Engine<D, F>
where
    D: Discovery + Send + 'static,
    F: Fetcher,
{
    /// An engine over a discovery source and a fetcher.
    pub fn new(discovery: D, fetcher: F, config: EngineConfig) -> Self {
        Self {
            discovery,
            fetcher: Arc::new(fetcher),
            config,
        }
    }

    /// Start the engine on the current runtime, returning how to reach it.
    #[must_use]
    pub fn spawn(self) -> EngineHandle {
        let (commands, command_rx) = mpsc::channel(64);
        let (snapshots, snapshot_rx) = watch::channel(Arc::new(Snapshot::default()));
        tokio::spawn(self.run(command_rx, snapshots));
        EngineHandle {
            commands,
            snapshots: snapshot_rx,
        }
    }

    async fn run(
        self,
        mut commands: mpsc::Receiver<Command>,
        snapshots: watch::Sender<Arc<Snapshot>>,
    ) {
        let subscription = self.discovery.subscribe();
        let mut events = subscription.events;

        let mut inventory = Inventory::new();
        let mut pending = Pending::default();
        let mut running: JoinSet<Done> = JoinSet::new();
        let mut generation = 0u64;
        let mut last_transport: BTreeMap<NodeKey, tokio::time::Instant> = BTreeMap::new();

        for advertisement in subscription.known {
            observe(&mut inventory, &mut pending, &advertisement);
        }

        let mut floor = tokio::time::interval(self.config.transport_floor);
        floor.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        floor.tick().await;

        publish(&snapshots, &mut inventory, &mut generation);

        loop {
            self.start_work(&mut inventory, &mut pending, &mut running);

            tokio::select! {
                event = events.recv() => match event {
                    Ok(event) => {
                        apply_event(&mut inventory, &mut pending, event);
                        publish(&snapshots, &mut inventory, &mut generation);
                    }
                    // Lagged: the engine fell behind the network. Nothing is
                    // lost that a re-read will not recover, so ask for one.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        refresh_all(&inventory, &mut pending);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },

                command = commands.recv() => match command {
                    Some(Command::Refresh(key)) => {
                        pending.trees.insert(key);
                    }
                    Some(Command::RefreshAll) => refresh_all(&inventory, &mut pending),
                    Some(Command::Stop) | None => break,
                },

                Some(done) = running.join_next(), if !running.is_empty() => {
                    match done {
                        Ok(done) => {
                            apply_done(&mut inventory, &mut pending, &mut last_transport, done);
                            publish(&snapshots, &mut inventory, &mut generation);
                        }
                        // A fetch task panicked. Nothing here may abort over
                        // it: the other Nodes are still browsable.
                        Err(error) => tracing::error!(%error, "a fetch task ended badly"),
                    }
                }

                _ = floor.tick() => {
                    let now = tokio::time::Instant::now();
                    for node in inventory.nodes() {
                        let due = last_transport
                            .get(&node.key)
                            .is_none_or(|last| now.duration_since(*last) >= self.config.transport_floor);
                        if due && node.state.is_ready() {
                            pending.transports.insert(node.key.clone());
                        }
                    }
                }
            }
        }
    }

    /// Start as much outstanding work as the bound allows.
    fn start_work(
        &self,
        inventory: &mut Inventory,
        pending: &mut Pending,
        running: &mut JoinSet<Done>,
    ) {
        while running.len() < self.config.concurrency && !pending.is_empty() {
            if let Some(key) = pending.trees.iter().next().cloned() {
                pending.trees.remove(&key);
                pending.collections.retain(|(node, _)| node != &key);
                let Some(base) = inventory.endpoint_for(&key).map(ToString::to_string) else {
                    continue;
                };
                let versions = advertised_versions(inventory, &key);
                let fetcher = Arc::clone(&self.fetcher);
                running.spawn(async move {
                    let result = fetcher.fetch_tree(base, versions).await;
                    Done::Tree(key, Box::new(result))
                });
                continue;
            }

            if let Some((key, collection)) = pending.collections.iter().next().cloned() {
                pending.collections.remove(&(key.clone(), collection));
                let Some(base) = inventory.endpoint_for(&key).map(ToString::to_string) else {
                    continue;
                };
                let versions = advertised_versions(inventory, &key);
                let fetcher = Arc::clone(&self.fetcher);
                running.spawn(async move {
                    let result = fetcher.fetch_collection(base, versions, collection).await;
                    Done::Collection(key, Box::new(result))
                });
                continue;
            }

            if let Some(key) = pending.transports.iter().next().cloned() {
                pending.transports.remove(&key);
                let Some(base) = inventory.connection_base(&key) else {
                    continue;
                };
                let (senders, receivers) = inventory.resources_of(&key);
                // One task per Node, reading its resources in sequence: the
                // bound is on Nodes, not on requests, because a small device
                // answers one at a time.
                for id in senders {
                    let fetcher = Arc::clone(&self.fetcher);
                    let base = base.clone();
                    running.spawn(async move {
                        let result = fetcher.fetch_sender_transport(base, id.clone()).await;
                        Done::SenderTransport(id, Box::new(result))
                    });
                }
                for id in receivers {
                    let fetcher = Arc::clone(&self.fetcher);
                    let base = base.clone();
                    running.spawn(async move {
                        let result = fetcher.fetch_receiver_transport(base, id.clone()).await;
                        Done::ReceiverTransport(id, Box::new(result))
                    });
                }
                continue;
            }

            break;
        }
    }
}

/// The versions a Node advertised, falling back to everything this controller
/// speaks when nothing is known — which is the case before its own record has
/// been read.
fn advertised_versions(inventory: &Inventory, key: &NodeKey) -> Vec<ApiVersion> {
    let known = inventory.versions_of(key);
    if known.is_empty() {
        nmos::SUPPORTED_VERSIONS.to_vec()
    } else {
        known
    }
}

fn observe(inventory: &mut Inventory, pending: &mut Pending, advertisement: &Advertisement) {
    let key = inventory.observe(advertisement);
    if !inventory.has_tree(&key) {
        pending.trees.insert(key);
    }
}

fn apply_event(inventory: &mut Inventory, pending: &mut Pending, event: DiscoveryEvent) {
    match event {
        DiscoveryEvent::Appeared(advertisement) => observe(inventory, pending, &advertisement),
        DiscoveryEvent::Changed {
            advertisement,
            changed,
        } => {
            let key = inventory.observe(&advertisement);
            if !inventory.has_tree(&key) {
                pending.trees.insert(key);
                return;
            }
            // Only what moved is re-read. An unchanged Node is left alone.
            for collection in changed {
                pending.collections.insert((key.clone(), collection));
                if collection == NodeCollection::Senders || collection == NodeCollection::Receivers
                {
                    pending.transports.insert(key.clone());
                }
            }
        }
        DiscoveryEvent::Departed { instance } => inventory.depart(&instance),
    }
}

fn apply_done(
    inventory: &mut Inventory,
    pending: &mut Pending,
    last_transport: &mut BTreeMap<NodeKey, tokio::time::Instant>,
    done: Done,
) {
    match done {
        Done::Tree(key, result) => match *result {
            Ok(tree) => {
                let id = tree.node.core.id.clone();
                let instance = inventory
                    .node(&key)
                    .and_then(|node| node.instances.iter().next().cloned());
                inventory.set_tree(&key, tree);
                // Now that the Node has told us who it is, re-key it.
                let key = match instance {
                    Some(instance) => inventory.identify(&instance, &id),
                    None => key,
                };
                pending.transports.insert(key);
            }
            Err(reason) => inventory.set_failure(&key, reason),
        },
        Done::Collection(key, result) => match *result {
            Ok(data) => inventory.update_collection(&key, data),
            Err(reason) => inventory.set_failure(&key, reason),
        },
        Done::SenderTransport(id, result) => {
            inventory.set_sender_transport(&id, *result);
        }
        Done::ReceiverTransport(id, result) => {
            inventory.set_receiver_transport(&id, *result);
        }
    }

    for node in inventory.nodes() {
        last_transport
            .entry(node.key.clone())
            .or_insert_with(tokio::time::Instant::now);
    }
}

fn refresh_all(inventory: &Inventory, pending: &mut Pending) {
    for node in inventory.nodes() {
        pending.trees.insert(node.key.clone());
    }
}

fn publish(
    snapshots: &watch::Sender<Arc<Snapshot>>,
    inventory: &mut Inventory,
    generation: &mut u64,
) {
    inventory.resolve_connections();
    *generation = generation.wrapping_add(1);
    let snapshot = Snapshot {
        nodes: inventory.nodes().cloned().collect(),
        generation: *generation,
    };
    // Ignored: a snapshot nobody is holding is a snapshot nobody needs.
    let _ = snapshots.send(Arc::new(snapshot));
}
