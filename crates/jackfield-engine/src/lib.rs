//! The controller's picture of the network: discovery of NMOS Nodes, the
//! in-memory inventory built from them, and the connection graph derived across
//! every Node it knows.
//!
//! This crate owns the truth. It knows nothing about a terminal — see
//! `docs/adr/0003-crate-split-engine-owns-state.md` — which is what makes it
//! testable headless and leaves room for a second front end.

// Panics are forbidden in production code (AGENTS.md); tests are the one place
// where they are the clearest way to assert.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod discovery;
mod identity;
mod inventory;

pub use discovery::mdns::{DiscoveryError, MdnsDiscovery};
pub use discovery::{
    Advertisement, Collection, Discovery, DiscoveryEvent, Endpoint, FabricatedDiscovery,
    NODE_SERVICE_TYPE, Subscription, VersionCounters,
};
pub use identity::{Identified, Identities, NodeKey};
pub use inventory::{
    DeviceView, Inventory, KnownNode, Media, NodeContents, NodeState, Orphan, OrphanKind, Pairing,
    ReceiverView, ResourceRef, SenderView, Transport,
};
