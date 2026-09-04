//! What the inventory holds about one Node, in the shape the interface reads.
//!
//! References are resolved here rather than in the interface, because resolving
//! them is what tells two rows apart: the bench converter presents three
//! Senders all labelled `SDI 1`, distinguishable only by the media type of the
//! Flow each carries.

use std::collections::BTreeSet;
use std::fmt;

use jackfield_nmos::{
    Device, Format, MediaType, Node, Receiver, Reception, ResourceId, Sender, StreamAddress,
    Transmission,
};

use crate::discovery::Endpoint;
use crate::identity::NodeKey;

/// What is known about one Node.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownNode {
    /// How this Node is keyed.
    pub key: NodeKey,
    /// The DNS-SD instance names that reach it. More than one means the box is
    /// multi-homed, not that there are two boxes.
    pub instances: BTreeSet<String>,
    /// Every address its API answers on.
    pub endpoints: BTreeSet<Endpoint>,
    /// The hostname it resolved to, used as a label when it reports none.
    pub hostname: Option<String>,
    /// Whether it requires authorization, and so cannot be read.
    pub requires_authorization: bool,
    /// What has been read from it.
    pub state: NodeState,
}

impl KnownNode {
    /// What to call this Node on screen.
    ///
    /// Its own label, failing that its hostname, failing that its address —
    /// never a blank row, because a blank row cannot be pointed at or reported.
    #[must_use]
    pub fn display_name(&self) -> String {
        if let NodeState::Ready(contents) = &self.state {
            let label = contents.node.core.label.trim();
            if !label.is_empty() {
                return label.to_owned();
            }
        }
        if let Some(hostname) = self
            .hostname
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
        {
            return hostname.trim_end_matches('.').to_owned();
        }
        self.endpoints
            .iter()
            .next()
            .map_or_else(|| self.key.to_string(), ToString::to_string)
    }

    /// The address to make requests to.
    #[must_use]
    pub fn preferred_endpoint(&self) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.address.is_ipv4())
            .or_else(|| self.endpoints.iter().next())
    }

    /// The Devices this Node hosts, empty until its tree has been read.
    #[must_use]
    pub fn devices(&self) -> &[DeviceView] {
        match &self.state {
            NodeState::Ready(contents) => &contents.devices,
            _ => &[],
        }
    }
}

/// How far reading a Node has got.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    /// Discovered, but nothing read yet. Such a Node is listed under its
    /// address rather than hidden until it is complete.
    Loading,

    /// Read, and browsable.
    ///
    /// Boxed because a Node's whole resource tree dwarfs the other two
    /// variants, and most Nodes on a plant are in one of those for part of
    /// their life.
    Ready(Box<NodeContents>),

    /// Could not be read. The reason is kept because the specs require the
    /// operator to see it against that Node, in terms that point at what to fix.
    Failed {
        /// What went wrong.
        reason: String,
    },
}

impl NodeState {
    /// Whether this Node can be browsed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, NodeState::Ready(_))
    }

    /// Why this Node cannot be read, if it cannot.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        match self {
            NodeState::Failed { reason } => Some(reason),
            _ => None,
        }
    }
}

/// A Node's resources, with their references resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeContents {
    /// The Node's own record.
    pub node: Node,
    /// The Devices it hosts, each with the Senders and Receivers it owns.
    pub devices: Vec<DeviceView>,
    /// Resources whose Device could not be found. Reported rather than dropped.
    pub orphans: Vec<Orphan>,
}

/// A Device, with what it owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    /// The Device itself.
    pub device: Device,
    /// Its Senders.
    pub senders: Vec<SenderView>,
    /// Its Receivers.
    pub receivers: Vec<ReceiverView>,
}

impl DeviceView {
    /// Whether this Device exposes nothing. Such a Device is still shown,
    /// marked as having no resources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.senders.is_empty() && self.receivers.is_empty()
    }
}

/// A resource whose owning Device the Node did not return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    /// The resource's identifier.
    pub id: ResourceId,
    /// Its label.
    pub label: String,
    /// The Device it claims to belong to.
    pub device_id: ResourceId,
    /// Whether it is a Sender or a Receiver.
    pub kind: OrphanKind,
}

/// What sort of resource was orphaned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs, reason = "the two variants are their own documentation")]
pub enum OrphanKind {
    Sender,
    Receiver,
}

/// What a Sender carries, once its Flow reference has been followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Media {
    /// The Flow was found.
    Known {
        /// What it carries.
        media_type: MediaType,
        /// Its high-level format.
        format: Format,
    },

    /// The Sender names a Flow the Node did not return. Shown as unknown rather
    /// than blank: a blank reads as "no media", which is a different thing.
    Unresolved {
        /// The Flow identifier the Sender reported.
        flow_id: ResourceId,
    },

    /// The Sender has no Flow routed to it at all.
    None,
}

impl fmt::Display for Media {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Media::Known { media_type, .. } => f.write_str(media_type.as_str()),
            Media::Unresolved { .. } => f.write_str("unknown media"),
            Media::None => f.write_str("no flow"),
        }
    }
}

/// How far reading a resource's transport parameters has got.
///
/// The three states are distinct on purpose. A Transmitting Sender whose
/// destination is merely unread must not render as a Sender going nowhere —
/// see `docs/adr/0005-two-tier-fetch.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    /// The transport pass has not run for this resource yet.
    Pending,

    /// The transport pass ran and the Connection API could not be read.
    Unavailable {
        /// Why.
        reason: String,
    },

    /// The stream addresses, as the device reports them. Empty where the device
    /// says `auto`, or says nothing usable.
    Known {
        /// One address per leg. More than one means ST 2022-7.
        streams: Vec<StreamAddress>,
    },
}

impl Transport {
    /// The stream addresses, if they have been read.
    #[must_use]
    pub fn streams(&self) -> &[StreamAddress] {
        match self {
            Transport::Known { streams } => streams,
            _ => &[],
        }
    }

    /// Whether the transport pass has run for this resource.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        matches!(self, Transport::Pending)
    }
}

/// Where a resource lives, for naming the other end of a pairing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceRef {
    /// The Node hosting it.
    pub node: NodeKey,
    /// What that Node is called on screen.
    pub node_label: String,
    /// The Device owning it.
    pub device_label: String,
    /// The resource's own identifier.
    pub id: ResourceId,
    /// The resource's label.
    pub label: String,
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} / {} / {}",
            self.node_label, self.device_label, self.label
        )
    }
}

/// What a Receiver is taking, once the graph has been resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pairing {
    /// The Receiver takes nothing.
    None,

    /// The transport pass has not run, so nothing can be said yet.
    Pending,

    /// The Sender is known.
    Resolved(Box<ResourceRef>),

    /// The Receiver names a Sender belonging to no discovered Node. It is still
    /// Subscribed — rendering it as Unsubscribed would be a lie.
    UnknownSender {
        /// The identifier it reported.
        sender_id: ResourceId,
    },

    /// The stream matches more than one Sender. Two sources in one multicast
    /// group is a fault in the plant, and picking one destroys the evidence.
    Ambiguous {
        /// Every Sender the stream matches.
        candidates: Vec<ResourceRef>,
    },

    /// The Receiver is Subscribed but names no Sender, and its stream matches
    /// nothing known.
    Unmatched,
}

/// A Sender, with everything the interface needs to draw its row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderView {
    /// The Sender itself.
    pub sender: Sender,
    /// Whether it is putting its Flow on the network.
    pub transmission: Transmission,
    /// What it carries.
    pub media: Media,
    /// Where it sends, once that has been read.
    pub transport: Transport,
    /// Every Receiver known to take its stream. May be empty while
    /// Transmitting — that is the state an operator most needs to notice.
    pub taken_by: Vec<ResourceRef>,
}

impl SenderView {
    /// Whether this Sender is Transmitting with nobody listening.
    ///
    /// Distinct from a Sender whose takers are merely not yet known, which is
    /// what [`Transport::Pending`] says.
    #[must_use]
    pub fn is_transmitting_into_the_void(&self) -> bool {
        self.transmission.is_transmitting()
            && self.taken_by.is_empty()
            && matches!(self.transport, Transport::Known { .. })
    }
}

/// A Receiver, with everything the interface needs to draw its row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverView {
    /// The Receiver itself.
    pub receiver: Receiver,
    /// Whether it is taking a stream.
    pub reception: Reception,
    /// What it accepts.
    pub accepts: Vec<MediaType>,
    /// Which stream it takes, once that has been read.
    pub transport: Transport,
    /// Which Sender feeds it, so far as the controller can tell.
    pub pairing: Pairing,
}
