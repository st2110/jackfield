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
use nmos::{ApiVersion, ConnectionApiClient, ResourceId, StreamAddress};

/// The IS-05 version writes are addressed at, matching the reads in
/// `fetcher.rs`.
const CONNECTION_VERSION: ApiVersion = ApiVersion::new(1, 1);

/// What a Receiver is told to take.
///
/// IS-05 expects the media description to travel from Sender to Receiver as an
/// SDP, which the controller passes through without reading a word of it. Not
/// every Sender publishes one — the specification's own answer for equipment
/// that does not is to make the connection from transport parameters alone —
/// so the addresses are the fallback, and there is deliberately no third case
/// where a Receiver is enabled with nothing to point it at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamSource {
    /// The Sender's transport file, handed over unchanged, with the addresses
    /// the Sender reports for the same stream.
    ///
    /// Both, because a Node holds whatever was staged against it until
    /// something replaces it, and IS-05 makes the parameters win where the two
    /// disagree: a take that named only the file would be judged against the
    /// addresses of the take before it. The addresses may be empty, for a
    /// Sender that publishes a file and no destination.
    TransportFile {
        /// The SDP, unread.
        data: String,
        /// Where the Sender says the stream goes, one entry per leg.
        streams: Vec<StreamAddress>,
    },
    /// Where the Sender's stream goes, one entry per leg. Two means ST 2022-7,
    /// and the order is the Sender's own: primary leg first.
    Streams(Vec<StreamAddress>),
}

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
    fn subscribe(
        &self,
        base: String,
        receiver: ResourceId,
        sender: ResourceId,
        source: StreamSource,
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

    /// Read where a Sender is sending, at the moment of the take.
    ///
    /// Read again rather than taken from the inventory: what the transport pass
    /// learned may be a minute old, and stale addresses staged alongside a fresh
    /// transport file are the one disagreement IS-05 resolves in favour of the
    /// stale half.
    fn fetch_streams(
        &self,
        base: String,
        sender: ResourceId,
    ) -> impl Future<Output = Result<Vec<StreamAddress>, String>> + Send;
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
        source: StreamSource,
    ) -> Result<(), String> {
        let (transport_file, transport_params) = match source {
            StreamSource::TransportFile { data, streams } if streams.is_empty() => {
                (Some(TransportFile::sdp(Some(data))), None)
            }
            StreamSource::TransportFile { data, streams } => {
                (Some(TransportFile::sdp(Some(data))), Some(legs(&streams)?))
            }
            StreamSource::Streams(streams) => (None, Some(legs(&streams)?)),
        };

        let patch = ReceiverStagedPatch::<ReceiverRtpParams> {
            // IS-05 requires the Sender to be named when the transport file or
            // the parameters change, so that the Node can report what it is
            // taking rather than only that it is taking something.
            sender_id: Param::Set(sender),
            master_enable: Some(true),
            activation: now(),
            transport_file,
            transport_params,
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

    async fn fetch_streams(
        &self,
        base: String,
        sender: ResourceId,
    ) -> Result<Vec<StreamAddress>, String> {
        let transport = self
            .connection_api
            .fetch_sender_transport(&base, CONNECTION_VERSION, &sender)
            .await
            .map_err(|e| e.to_string())?;
        Ok(transport
            .legs
            .iter()
            .filter_map(nmos::SenderLeg::destination)
            .collect())
    }
}

/// One Receiver leg per Sender leg, joining the group the Sender sends to.
///
/// `rtp_enabled` is set explicitly on every leg. IS-05 asks a controller to
/// state the parameters a connection depends on rather than assume them, and on
/// a redundant Receiver the second leg is exactly the one somebody else may
/// have left disabled.
fn legs(streams: &[StreamAddress]) -> Result<Vec<ReceiverRtpParams>, String> {
    streams
        .iter()
        .map(|stream| {
            let multicast_ip = stream.address.parse().map_err(|_| {
                format!(
                    "the Sender reports an address this controller cannot use: {}",
                    stream.address
                )
            })?;
            Ok(ReceiverRtpParams {
                multicast_ip: Param::Set(multicast_ip),
                destination_port: Param::Set(stream.port),
                rtp_enabled: Param::Set(true),
                ..ReceiverRtpParams::default()
            })
        })
        .collect()
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
