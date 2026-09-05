//! Fabricated snapshots, so the screen can be exercised with no engine and no
//! network.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use jackfield_engine::{
    DeviceView, Endpoint, KnownNode, Media, NodeContents, NodeKey, NodeState, Orphan, OrphanKind,
    Pairing, ReceiverView, ResourceRef, SenderView, Snapshot, Transport,
};
use nmos::{
    Control, Device, MediaCaps, MediaType, Node, NodeApi, Protocol, Receiver, ReceiverCaps,
    ReceiverSubscription, Reception, ResourceCore, ResourceId, Sender, SenderSubscription,
    StreamAddress, Transmission,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

pub fn id(n: u16) -> ResourceId {
    format!("{n:08x}-0000-4000-8000-000000000000")
        .parse()
        .expect("a well-formed identifier")
}

fn core(n: u16, label: &str) -> ResourceCore {
    ResourceCore {
        id: id(n),
        version: "1441700172:0".parse().expect("a version"),
        label: label.to_owned(),
        description: String::new(),
        tags: BTreeMap::new(),
    }
}

pub fn endpoint(address: [u8; 4], port: u16) -> Endpoint {
    Endpoint {
        address: IpAddr::V4(Ipv4Addr::from(address)),
        port,
        protocol: Protocol::Http,
    }
}

pub fn node_record(n: u16, label: &str) -> Node {
    Node {
        core: core(n, label),
        href: String::new(),
        hostname: Some("converter.local.".to_owned()),
        api: NodeApi {
            versions: vec!["v1.3".to_owned()],
            endpoints: Vec::new(),
        },
        caps: BTreeMap::new(),
        services: Vec::new(),
        clocks: Vec::new(),
        interfaces: Vec::new(),
    }
}

pub fn device_record(n: u16, label: &str) -> Device {
    Device {
        core: core(n, label),
        kind: "urn:x-nmos:device:generic".to_owned(),
        node_id: id(1),
        senders: Vec::new(),
        receivers: Vec::new(),
        controls: vec![Control {
            href: "http://10.77.1.90:8090/x-nmos/connection/v1.1/".to_owned(),
            kind: "urn:x-nmos:control:sr-ctrl/v1.1".to_owned(),
            authorization: false,
        }],
    }
}

pub fn sender_view(n: u16, label: &str, media: &str, transmission: Transmission) -> SenderView {
    SenderView {
        sender: Sender {
            core: core(n, label),
            caps: BTreeMap::new(),
            flow_id: Some(id(n + 100)),
            transport: "urn:x-nmos:transport:rtp.mcast".to_owned(),
            device_id: id(10),
            manifest_href: None,
            interface_bindings: Vec::new(),
            subscription: SenderSubscription::new(transmission, None),
        },
        transmission,
        media: Media::Known {
            media_type: media.parse::<MediaType>().expect("a media type"),
            format: nmos::Format::Video,
        },
        transport: Transport::Pending,
        taken_by: Vec::new(),
    }
}

pub fn receiver_view(n: u16, label: &str, media: &str, reception: Reception) -> ReceiverView {
    ReceiverView {
        receiver: Receiver {
            core: core(n, label),
            device_id: id(10),
            transport: "urn:x-nmos:transport:rtp.mcast".to_owned(),
            interface_bindings: Vec::new(),
            subscription: ReceiverSubscription::new(reception, None),
            caps: ReceiverCaps::Video {
                caps: MediaCaps {
                    media_types: vec![media.parse().expect("a media type")],
                    event_types: Vec::new(),
                },
            },
        },
        reception,
        accepts: vec![media.parse().expect("a media type")],
        transport: Transport::Pending,
        pairing: Pairing::None,
    }
}

pub fn resource_ref(node: &str, device: &str, n: u16, label: &str) -> ResourceRef {
    ResourceRef {
        node: NodeKey::Settled(id(1)),
        node_label: node.to_owned(),
        device_label: device.to_owned(),
        id: id(n),
        label: label.to_owned(),
    }
}

pub fn stream(last: u8, port: u16) -> StreamAddress {
    StreamAddress {
        address: format!("239.255.0.{last}"),
        port,
    }
}

/// A Node in some state.
pub fn known(key: u16, label: &str, address: [u8; 4], state: NodeState) -> KnownNode {
    let mut endpoints = BTreeSet::new();
    endpoints.insert(endpoint(address, 8090));
    let mut instances = BTreeSet::new();
    instances.insert(label.to_owned());

    KnownNode {
        key: NodeKey::Settled(id(key)),
        instances,
        endpoints,
        hostname: Some("converter.local.".to_owned()),
        requires_authorization: false,
        state,
    }
}

pub fn ready(node_label: &str, devices: Vec<DeviceView>) -> NodeState {
    NodeState::Ready(Box::new(NodeContents {
        node: node_record(1, node_label),
        devices,
        orphans: Vec::new(),
    }))
}

pub fn ready_with_orphan(node_label: &str, devices: Vec<DeviceView>) -> NodeState {
    NodeState::Ready(Box::new(NodeContents {
        node: node_record(1, node_label),
        devices,
        orphans: vec![Orphan {
            id: id(900),
            label: "stray".to_owned(),
            device_id: id(999),
            kind: OrphanKind::Sender,
        }],
    }))
}

pub fn device_view(
    label: &str,
    senders: Vec<SenderView>,
    receivers: Vec<ReceiverView>,
) -> DeviceView {
    DeviceView {
        device: device_record(10, label),
        senders,
        receivers,
    }
}

pub fn snapshot(nodes: Vec<KnownNode>) -> Arc<Snapshot> {
    Arc::new(Snapshot {
        nodes,
        generation: 1,
    })
}

/// Render an app into a buffer and return it as lines of plain text.
///
/// Text, not styles: everything the interface must say has to survive a
/// monochrome terminal, and a test that inspected colours would not notice if
/// it stopped doing so.
pub fn render(app: &mut jackfield_tui::App, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("a test terminal");
    terminal
        .draw(|frame| jackfield_tui::draw(frame, app))
        .expect("the screen draws");

    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| {
                    buffer
                        .cell((x, y))
                        .map_or(" ", ratatui::buffer::Cell::symbol)
                })
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// The whole screen as one string, for `contains` assertions.
pub fn screen(app: &mut jackfield_tui::App, width: u16, height: u16) -> String {
    render(app, width, height).join("\n")
}

/// A Node shaped like the bench converter: three Devices, each with three
/// Senders sharing one label and three Receivers. Forty-odd rows, which is more
/// than a terminal shows at once — the case navigation exists for.
pub fn bench_node() -> KnownNode {
    let media = ["video/raw", "audio/L24", "video/smpte291"];
    let devices: Vec<DeviceView> = (0..3u16)
        .map(|port| {
            let label = format!("SDI {}", port + 1);
            let senders: Vec<SenderView> = media
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    let index = u16::try_from(index).unwrap_or_default();
                    let transmission = if port == 0 {
                        Transmission::Transmitting
                    } else {
                        Transmission::Idle
                    };
                    sender_view(100 + port * 10 + index, &label, kind, transmission)
                })
                .collect();
            let receivers: Vec<ReceiverView> = media
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    let index = u16::try_from(index).unwrap_or_default();
                    receiver_view(
                        400 + port * 10 + index,
                        &format!("{label}/{kind}"),
                        kind,
                        Reception::Unsubscribed,
                    )
                })
                .collect();
            device_view(&label, senders, receivers)
        })
        .collect();

    let mut node = known(
        1,
        "core-ml-2110-bm",
        [10, 77, 1, 90],
        ready("core-ml-2110-bm", devices),
    );
    node.hostname = Some("core-ml-2110-bm.local.".to_owned());
    node
}
