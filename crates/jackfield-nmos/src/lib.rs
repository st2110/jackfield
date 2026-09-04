//! The NMOS contract: the resource model of IS-04 and the read-only clients for
//! the Node API and the Connection API.
//!
//! This crate knows the protocol and nothing else. It holds no state, performs
//! no discovery, and has no opinion about how anything is displayed — see
//! `docs/adr/0003-crate-split-engine-owns-state.md`.
//!
//! The types here are written by hand rather than generated, and are held to
//! the published AMWA JSON Schemas by the test suite in both directions. The
//! reasoning is in `docs/adr/0001-nmos-types-by-hand-schemas-as-validators.md`.

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

mod connection;
mod connection_api;
mod device;
mod error;
mod flow;
mod media;
mod node;
mod node_api;
mod receiver;
mod resource;
mod sender;
mod source;
mod version;

pub use connection::{Reception, Transmission};
pub use connection_api::{
    CONNECTION_CONTROL_URN, ConnectionApiClient, ConnectionApiClientBuilder, ConnectionApiError,
    ReceiverLeg, ReceiverTransport, SUPPORTED_CONNECTION_VERSIONS, SenderLeg, SenderTransport,
    StreamAddress,
};
pub use device::{Control, Device};
pub use error::ParseError;
pub use flow::{Component, ComponentName, DidSdid, Flow, FlowCore, InterlaceMode, VideoCore};
pub use media::{Format, MediaType, Rate};
pub use node::{
    ApiEndpoint, AttachedNetworkDevice, Clock, Interface, Node, NodeApi, Protocol, Service,
};
pub use node_api::{
    NodeApiClient, NodeApiClientBuilder, NodeApiError, ResourceTree, SUPPORTED_VERSIONS,
};
pub use receiver::{MediaCaps, Receiver, ReceiverCaps, ReceiverSubscription};
pub use resource::{Capabilities, ResourceCore, ResourceId, Tags, Version};
pub use sender::{Sender, SenderSubscription};
pub use source::{Channel, Source, SourceCore};
pub use version::ApiVersion;
