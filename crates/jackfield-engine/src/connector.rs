//! Changing a device: the one thing this controller does that is not a read.
//!
//! IS-05 has a single verb — stage a patch against `staged` and say when it
//! takes effect — so putting a Sender on air, taking it off, connecting a
//! Receiver and disconnecting it are one request with four documents. The
//! trait is separate from [`Fetcher`](crate::Fetcher) because the two differ in
//! the way that matters: a failed read leaves a gap on the screen, a failed
//! write leaves equipment in a state nobody asked for.
//!
//! Errors are strings for the same reason they are in `fetcher.rs`: what
//! reaches the operator is the sentence the device wrote.

use std::future::Future;

use nmos::is05::{
    ActivationMode, ActivationPatch, Param, ReceiverRtpParams, ReceiverStagedPatch,
    SenderRtpParams, SenderStagedPatch, TransportFile,
};
use nmos::{ApiVersion, ConnectionApiClient, ResourceId};

/// The IS-05 version writes are addressed at, matching the reads in
/// `fetcher.rs`.
const CONNECTION_VERSION: ApiVersion = ApiVersion::new(1, 1);

/// What the engine needs in order to change a device.
pub trait Connector: Send + Sync + 'static {
    /// Put a Sender on air, or take it off.
    fn set_transmitting(
        &self,
        base: String,
        sender: ResourceId,
        on: bool,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// Point a Receiver at a Sender's stream.
    ///
    /// `transport_file` is the Sender's own SDP, read beforehand and passed
    /// through untouched: IS-05 expects the media description to travel from
    /// Sender to Receiver, and a controller that rewrote it would be inventing
    /// a description of a stream it has never seen.
    fn subscribe(
        &self,
        base: String,
        receiver: ResourceId,
        sender: ResourceId,
        transport_file: Option<String>,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// Take a Receiver off whatever it was taking.
    fn unsubscribe(
        &self,
        base: String,
        receiver: ResourceId,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// Read a Sender's transport file, to hand to a Receiver.
    fn fetch_transport_file(
        &self,
        base: String,
        sender: ResourceId,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send;
}

/// Writes to Connection APIs over HTTP.
#[derive(Debug, Clone)]
pub struct NmosConnector {
    connection_api: ConnectionApiClient,
}

impl NmosConnector {
    /// A connector over the given client.
    #[must_use]
    pub fn new(connection_api: ConnectionApiClient) -> Self {
        Self { connection_api }
    }
}

/// An immediate activation, which is the only kind this controller performs.
///
/// Scheduled activation exists in IS-05 for synchronised salvos across several
/// devices. It needs a clock shared with the equipment, and until this
/// controller has one, asking for it would be promising a precision it cannot
/// keep.
fn now() -> Option<ActivationPatch> {
    Some(ActivationPatch {
        mode: Some(ActivationMode::ActivateImmediate),
        ..ActivationPatch::default()
    })
}

impl Connector for NmosConnector {
    async fn set_transmitting(
        &self,
        base: String,
        sender: ResourceId,
        on: bool,
    ) -> Result<(), String> {
        // `rtp_enabled` is re-asserted per leg alongside `master_enable`, as
        // IS-05 asks of a controller: between the last read and this patch,
        // another controller may have changed either of them.
        let patch = SenderStagedPatch::<SenderRtpParams> {
            master_enable: Some(on),
            activation: now(),
            transport_params: None,
            ..SenderStagedPatch::default()
        };

        let staged = self
            .connection_api
            .patch_sender_staged(&base, CONNECTION_VERSION, &sender, &patch)
            .await
            .map_err(|e| e.to_string())?;

        confirm(staged.master_enable, on)
    }

    async fn subscribe(
        &self,
        base: String,
        receiver: ResourceId,
        sender: ResourceId,
        transport_file: Option<String>,
    ) -> Result<(), String> {
        let patch = ReceiverStagedPatch::<ReceiverRtpParams> {
            // IS-05 requires the Sender to be named when the transport file or
            // the parameters change, so that the Node can report what it is
            // taking rather than only that it is taking something.
            sender_id: Param::Set(sender),
            master_enable: Some(true),
            activation: now(),
            transport_file: transport_file.map(|data| TransportFile::sdp(Some(data))),
            transport_params: None,
        };

        let staged = self
            .connection_api
            .patch_receiver_staged(&base, CONNECTION_VERSION, &receiver, &patch)
            .await
            .map_err(|e| e.to_string())?;

        confirm(staged.master_enable, true)
    }

    async fn unsubscribe(&self, base: String, receiver: ResourceId) -> Result<(), String> {
        // `sender_id` is set to null, not left absent: absent would leave the
        // old identifier in place and the Node would keep claiming a
        // connection it no longer has. IS-05 requires the null.
        let patch = ReceiverStagedPatch::<ReceiverRtpParams> {
            sender_id: Param::Null,
            master_enable: Some(false),
            activation: now(),
            transport_file: None,
            transport_params: None,
        };

        let staged = self
            .connection_api
            .patch_receiver_staged(&base, CONNECTION_VERSION, &receiver, &patch)
            .await
            .map_err(|e| e.to_string())?;

        confirm(staged.master_enable, false)
    }

    async fn fetch_transport_file(
        &self,
        base: String,
        sender: ResourceId,
    ) -> Result<Option<String>, String> {
        self.connection_api
            .fetch_transport_file(&base, CONNECTION_VERSION, &sender)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Hold the device to what it answered.
///
/// IS-05 has no locking, so two controllers can stage against one resource at
/// once. The reply is the only evidence of what took effect, and a controller
/// that ignored it would report success for a change that never happened.
fn confirm(answered: bool, asked: bool) -> Result<(), String> {
    if answered == asked {
        return Ok(());
    }
    Err(format!(
        "the device answered master_enable={answered} to a request for {asked}; \
         something else is staging against it"
    ))
}
