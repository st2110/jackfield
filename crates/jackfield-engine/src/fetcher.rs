//! The real fetcher: the engine's needs, met over HTTP.
//!
//! Errors become strings here, and that is deliberate. The engine stores a
//! failure against a Node so the interface can show it to an operator; what it
//! needs is a sentence that points at what to fix, not a type to match on. The
//! typed errors stay where they can still be acted on, in the `nmos` crate.

use nmos::{
    ApiVersion, CollectionData, ConnectionApiClient, NodeApiClient, NodeCollection,
    ReceiverTransport, ResourceId, ResourceTree, SenderTransport,
};

use crate::engine::Fetcher;

/// The IS-05 version to address a Connection API at.
///
/// The Device's control URN carries the version it speaks; until this project
/// reads connections per Device it addresses the version the bench equipment
/// advertises, and falls back nowhere — a Node whose Connection API answers
/// nothing here reports its transport as unavailable, and its connection state,
/// which came from IS-04, stays valid.
const CONNECTION_VERSION: ApiVersion = ApiVersion::new(1, 1);

/// Reads Nodes over HTTP.
#[derive(Debug, Clone)]
pub struct NmosFetcher {
    node_api: NodeApiClient,
    connection_api: ConnectionApiClient,
}

impl NmosFetcher {
    /// A fetcher over the given clients.
    #[must_use]
    pub fn new(node_api: NodeApiClient, connection_api: ConnectionApiClient) -> Self {
        Self {
            node_api,
            connection_api,
        }
    }
}

impl Fetcher for NmosFetcher {
    async fn fetch_tree(
        &self,
        base: String,
        versions: Vec<ApiVersion>,
    ) -> Result<ResourceTree, String> {
        self.node_api
            .fetch_tree(&base, &versions)
            .await
            .map_err(|e| e.to_string())
    }

    async fn fetch_collection(
        &self,
        base: String,
        versions: Vec<ApiVersion>,
        collection: NodeCollection,
    ) -> Result<CollectionData, String> {
        self.node_api
            .fetch_collection(&base, &versions, collection)
            .await
            .map_err(|e| e.to_string())
    }

    async fn fetch_sender_transport(
        &self,
        base: String,
        id: ResourceId,
    ) -> Result<SenderTransport, String> {
        self.connection_api
            .fetch_sender_transport(&base, CONNECTION_VERSION, &id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn fetch_receiver_transport(
        &self,
        base: String,
        id: ResourceId,
    ) -> Result<ReceiverTransport, String> {
        self.connection_api
            .fetch_receiver_transport(&base, CONNECTION_VERSION, &id)
            .await
            .map_err(|e| e.to_string())
    }
}
