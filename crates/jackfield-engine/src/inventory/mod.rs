//! The controller's picture of what is out there.
//!
//! Held in memory only, for the lifetime of the process: nothing is written to
//! disk, and nothing from a previous run is read on startup. On a plant of
//! realistic size rediscovery costs seconds, and a cache that can be stale is
//! worse than no cache when the thing it describes is on air.

mod graph;
mod resolve;
mod view;

use std::collections::{BTreeMap, BTreeSet};

use nmos::{CollectionData, ReceiverTransport, ResourceId, ResourceTree, SenderTransport};

use crate::discovery::{Advertisement, Endpoint, VersionCounters};
use crate::identity::{Identities, NodeKey};

pub use view::{
    DeviceView, KnownNode, Media, NodeContents, NodeState, Orphan, OrphanKind, Pairing,
    ReceiverView, ResourceRef, SenderView, Transport,
};

/// Everything the controller knows about the network.
///
/// Nodes are keyed by the identifier they report, with their endpoints as a
/// set. Before that identifier has been read the DNS-SD instance name serves,
/// because nothing better is known — see `identity.rs`.
#[derive(Debug, Default)]
pub struct Inventory {
    identities: Identities,
    nodes: BTreeMap<NodeKey, KnownNode>,
    /// The last tree read from each Node, kept so a counter-driven re-read of
    /// one collection can replace that collection alone.
    trees: BTreeMap<NodeKey, ResourceTree>,
    counters: BTreeMap<NodeKey, VersionCounters>,
    /// Transport parameters, held per Node so a re-read of the tree does not
    /// throw away the slower pass's work.
    sender_transports: BTreeMap<ResourceId, Result<SenderTransport, String>>,
    receiver_transports: BTreeMap<ResourceId, Result<ReceiverTransport, String>>,
}

impl Inventory {
    /// An empty inventory. Every run starts here.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what a Node advertised, returning the key it is held under.
    pub fn observe(&mut self, advertisement: &Advertisement) -> NodeKey {
        let key = self.identities.observe(advertisement);
        self.counters.insert(key.clone(), advertisement.counters);

        let node = self.nodes.entry(key.clone()).or_insert_with(|| KnownNode {
            key: key.clone(),
            instances: BTreeSet::new(),
            endpoints: BTreeSet::new(),
            hostname: None,
            requires_authorization: false,
            state: NodeState::Loading,
        });

        node.instances.insert(advertisement.instance.clone());
        node.endpoints
            .extend(advertisement.endpoints.iter().copied());
        if node.hostname.is_none() {
            node.hostname.clone_from(&advertisement.hostname);
        }
        node.requires_authorization |= advertisement.requires_authorization;

        key
    }

    /// Record that a Node reported its identifier, re-keying it.
    ///
    /// A Node already known under another instance name collapses into this
    /// one: its endpoints join, and it occupies one row rather than two.
    pub fn identify(&mut self, instance: &str, id: &ResourceId) -> NodeKey {
        let outcome = self.identities.identify(instance, id);
        let settled = NodeKey::Settled(id.clone());

        let was = match outcome {
            crate::identity::Identified::Unchanged(key) => return key,
            crate::identity::Identified::Settled { was, .. }
            | crate::identity::Identified::Merged { was, .. } => was,
        };

        if let Some(node) = self.nodes.remove(&was) {
            let counters = self.counters.remove(&was);
            let merged = self
                .nodes
                .entry(settled.clone())
                .or_insert_with(|| KnownNode {
                    key: settled.clone(),
                    instances: BTreeSet::new(),
                    endpoints: BTreeSet::new(),
                    hostname: None,
                    requires_authorization: false,
                    state: NodeState::Loading,
                });
            merged.instances.extend(node.instances);
            merged.endpoints.extend(node.endpoints);
            if merged.hostname.is_none() {
                merged.hostname = node.hostname;
            }
            merged.requires_authorization |= node.requires_authorization;
            if !merged.state.is_ready() {
                merged.state = node.state;
            }
            merged.key = settled.clone();
            if let Some(tree) = self.trees.remove(&was) {
                self.trees.entry(settled.clone()).or_insert(tree);
            }
            if let Some(counters) = counters {
                self.counters.entry(settled.clone()).or_insert(counters);
            }
        }

        settled
    }

    /// Record that a Node withdrew one of its advertisements.
    ///
    /// A Node reachable under another instance name stays: losing one route to
    /// a box is not losing the box.
    pub fn depart(&mut self, instance: &str) {
        let Some(key) = self.identities.forget(instance) else {
            return;
        };
        if !self.identities.instances_for(&key).is_empty() {
            if let Some(node) = self.nodes.get_mut(&key) {
                node.instances.remove(instance);
            }
            return;
        }
        self.nodes.remove(&key);
        self.counters.remove(&key);
        self.trees.remove(&key);
    }

    /// Record a Node's resource tree, resolving its references.
    ///
    /// Replaces whatever was held: a Sender removed at the Node disappears
    /// here, which is what makes an explicit refresh meaningful.
    pub fn set_tree(&mut self, key: &NodeKey, tree: ResourceTree) {
        if !self.nodes.contains_key(key) {
            return;
        }
        self.trees.insert(key.clone(), tree);
        self.rebuild(key);
    }

    /// Replace one collection of a Node's tree, leaving the rest alone.
    ///
    /// This is what makes counter-driven refresh worth having: a Node whose
    /// Receivers changed is not re-read in full. A Node whose tree has never
    /// been read is left alone, because a collection on its own is not a Node.
    pub fn update_collection(&mut self, key: &NodeKey, data: CollectionData) {
        let Some(tree) = self.trees.get_mut(key) else {
            return;
        };
        match data {
            CollectionData::Node(node) => tree.node = *node,
            CollectionData::Devices(devices) => tree.devices = devices,
            CollectionData::Senders(senders) => tree.senders = senders,
            CollectionData::Receivers(receivers) => tree.receivers = receivers,
            CollectionData::Flows(flows) => tree.flows = flows,
            CollectionData::Sources(sources) => tree.sources = sources,
        }
        self.rebuild(key);
    }

    /// Whether a Node's tree has been read at all.
    #[must_use]
    pub fn has_tree(&self, key: &NodeKey) -> bool {
        self.trees.contains_key(key)
    }

    /// Re-derive one Node's contents from the tree currently held.
    fn rebuild(&mut self, key: &NodeKey) {
        let Some(tree) = self.trees.get(key) else {
            return;
        };
        let contents = resolve::contents(
            tree.clone(),
            &self.sender_transports,
            &self.receiver_transports,
        );
        if let Some(node) = self.nodes.get_mut(key) {
            node.state = NodeState::Ready(Box::new(contents));
        }
    }

    /// Record that a Node could not be read.
    ///
    /// The failure is a value stored against that Node — not a log line, not a
    /// lost row — because the specs require the operator to see it.
    pub fn set_failure(&mut self, key: &NodeKey, reason: impl Into<String>) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.state = NodeState::Failed {
                reason: reason.into(),
            };
        }
    }

    /// Record what a Sender's Connection API said.
    pub fn set_sender_transport(
        &mut self,
        id: &ResourceId,
        transport: Result<SenderTransport, String>,
    ) {
        self.sender_transports.insert(id.clone(), transport);
        self.reapply_transports();
    }

    /// Record what a Receiver's Connection API said.
    pub fn set_receiver_transport(
        &mut self,
        id: &ResourceId,
        transport: Result<ReceiverTransport, String>,
    ) {
        self.receiver_transports.insert(id.clone(), transport);
        self.reapply_transports();
    }

    /// Push the transport pass's results into the views that show them.
    fn reapply_transports(&mut self) {
        for node in self.nodes.values_mut() {
            let NodeState::Ready(contents) = &mut node.state else {
                continue;
            };
            resolve::apply_transports(contents, &self.sender_transports, &self.receiver_transports);
        }
    }

    /// Work out which Senders feed which Receivers, across every Node known.
    ///
    /// This is the product. Whether a Sender is transmitting and whether
    /// anything is listening are different questions, and only here — where
    /// every Node is visible — can the second be answered. See
    /// `docs/adr/0004-connection-vocabulary-and-graph.md`.
    pub fn resolve_connections(&mut self) {
        graph::resolve(&mut self.nodes);
    }

    /// Every Node known, in a stable order.
    pub fn nodes(&self) -> impl Iterator<Item = &KnownNode> {
        self.nodes.values()
    }

    /// One Node, by key.
    #[must_use]
    pub fn node(&self, key: &NodeKey) -> Option<&KnownNode> {
        self.nodes.get(key)
    }

    /// The key an instance name currently resolves to.
    #[must_use]
    pub fn key_for(&self, instance: &str) -> Option<&NodeKey> {
        self.identities.key_for(instance)
    }

    /// The counters last advertised for a Node.
    #[must_use]
    pub fn counters(&self, key: &NodeKey) -> Option<&VersionCounters> {
        self.counters.get(key)
    }

    /// How many Nodes are known.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether nothing has been discovered yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every Sender and Receiver a Node exposes, for the transport pass.
    #[must_use]
    pub fn resources_of(&self, key: &NodeKey) -> (Vec<ResourceId>, Vec<ResourceId>) {
        let Some(tree) = self.trees.get(key) else {
            return (Vec::new(), Vec::new());
        };
        (
            tree.senders.iter().map(|s| s.core.id.clone()).collect(),
            tree.receivers.iter().map(|r| r.core.id.clone()).collect(),
        )
    }

    /// Where the Device owning a Sender or Receiver advertises its Connection
    /// API, if it advertises one.
    ///
    /// Resolved per Device, not per Node: IS-05 permits one instance of the
    /// Connection API per Device, and permits a Device to expose none at all.
    /// A resource whose Device advertises nothing is not controllable over
    /// IS-05, and answering with a neighbour's base would read — or write — the
    /// wrong box.
    #[must_use]
    pub fn connection_base(&self, key: &NodeKey, resource: &ResourceId) -> Option<String> {
        let tree = self.trees.get(key)?;
        let device_id = tree
            .senders
            .iter()
            .find(|sender| &sender.core.id == resource)
            .map(|sender| &sender.device_id)
            .or_else(|| {
                tree.receivers
                    .iter()
                    .find(|receiver| &receiver.core.id == resource)
                    .map(|receiver| &receiver.device_id)
            })?;

        let href = tree
            .devices
            .iter()
            .find(|device| &device.core.id == device_id)?
            .control_href(nmos::CONNECTION_CONTROL_URN)?;

        // The advertised href already ends in `/x-nmos/connection/v1.1/`; the
        // client builds that path itself, so only the origin is kept.
        let trimmed = href.trim_end_matches('/');
        Some(match trimmed.find("/x-nmos/connection") {
            Some(at) => trimmed.get(..at).unwrap_or(trimmed).to_owned(),
            None => trimmed.to_owned(),
        })
    }

    /// The API versions a Node's own record says it speaks.
    #[must_use]
    pub fn versions_of(&self, key: &NodeKey) -> Vec<nmos::ApiVersion> {
        self.trees
            .get(key)
            .map(|tree| {
                tree.node
                    .api
                    .versions
                    .iter()
                    .filter_map(|v| v.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The address to reach a Node at.
    #[must_use]
    pub fn endpoint_for(&self, key: &NodeKey) -> Option<&Endpoint> {
        self.nodes.get(key).and_then(KnownNode::preferred_endpoint)
    }
}
