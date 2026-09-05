//! Fabricated resources, so the inventory can be exercised with no network.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use jackfield_engine::Advertisement;
use nmos::{
    ApiVersion, Control, Device, Flow, FlowCore, InterlaceMode, MediaCaps, MediaType, Node,
    NodeApi, Rate, Receiver, ReceiverCaps, ReceiverSubscription, Reception, ResourceCore,
    ResourceId, ResourceTree, Sender, SenderSubscription, Source, SourceCore, Transmission,
    VideoCore,
};

/// A well-formed identifier built from a small number, so tests can name
/// resources readably.
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

pub fn advertisement(instance: &str, address: [u8; 4], port: u16) -> Advertisement {
    Advertisement::try_from_parts(
        instance,
        Some(format!("{instance}.local.")),
        &[IpAddr::V4(Ipv4Addr::from(address))],
        port,
        &BTreeMap::<String, String>::new(),
    )
    .expect("usable")
}

pub fn node(n: u16, label: &str) -> Node {
    Node {
        core: core(n, label),
        href: "http://10.77.1.90:8090/".to_owned(),
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

pub fn device(n: u16, label: &str, node_id: u16) -> Device {
    Device {
        core: core(n, label),
        kind: "urn:x-nmos:device:generic".to_owned(),
        node_id: id(node_id),
        senders: Vec::new(),
        receivers: Vec::new(),
        controls: vec![Control {
            href: "http://10.77.1.90:8090/x-nmos/connection/v1.1/".to_owned(),
            kind: "urn:x-nmos:control:sr-ctrl/v1.1".to_owned(),
            authorization: false,
        }],
    }
}

/// The same Device, advertising its Connection API somewhere else. A Node may
/// run one per Device, so two Devices on one Node need not share a base URL.
pub fn controlled_at(mut device: Device, href: &str) -> Device {
    device.controls = vec![Control {
        href: href.to_owned(),
        kind: "urn:x-nmos:control:sr-ctrl/v1.1".to_owned(),
        authorization: false,
    }];
    device
}

pub fn sender(n: u16, label: &str, device_id: u16, flow_id: Option<u16>) -> Sender {
    Sender {
        core: core(n, label),
        caps: BTreeMap::new(),
        flow_id: flow_id.map(id),
        transport: "urn:x-nmos:transport:rtp.mcast".to_owned(),
        device_id: id(device_id),
        manifest_href: None,
        interface_bindings: Vec::new(),
        subscription: SenderSubscription::new(Transmission::Idle, None),
    }
}

pub fn transmitting(mut sender: Sender) -> Sender {
    sender.subscription = SenderSubscription::new(Transmission::Transmitting, None);
    sender
}

pub fn receiver(n: u16, label: &str, device_id: u16, media: &str) -> Receiver {
    Receiver {
        core: core(n, label),
        device_id: id(device_id),
        transport: "urn:x-nmos:transport:rtp.mcast".to_owned(),
        interface_bindings: Vec::new(),
        subscription: ReceiverSubscription::new(Reception::Unsubscribed, None),
        caps: ReceiverCaps::Video {
            caps: MediaCaps {
                media_types: vec![media.parse::<MediaType>().expect("a media type")],
                event_types: Vec::new(),
            },
        },
    }
}

pub fn subscribed(mut receiver: Receiver, sender_id: Option<u16>) -> Receiver {
    receiver.subscription = ReceiverSubscription::new(Reception::Subscribed, sender_id.map(id));
    receiver
}

pub fn flow(n: u16, label: &str, device_id: u16, source_id: u16, media: &str) -> Flow {
    let flow_core = FlowCore {
        core: core(n, label),
        source_id: id(source_id),
        device_id: id(device_id),
        parents: Vec::new(),
        grain_rate: None,
    };
    let media_type: MediaType = media.parse().expect("a media type");

    if media_type.as_str().starts_with("audio/") {
        Flow::AudioRaw {
            core: flow_core,
            sample_rate: Rate {
                numerator: 48_000,
                denominator: 1,
            },
            media_type,
            bit_depth: Some(24),
        }
    } else if media_type.as_str() == "video/smpte291" {
        Flow::SdiAncData {
            core: flow_core,
            media_type,
            did_sdid: Vec::new(),
        }
    } else {
        Flow::VideoRaw {
            core: flow_core,
            video: VideoCore {
                frame_width: 1920,
                frame_height: 1080,
                interlace_mode: InterlaceMode::Progressive,
                colorspace: "BT709".to_owned(),
                transfer_characteristic: None,
            },
            media_type,
            components: Vec::new(),
        }
    }
}

pub fn source(n: u16, label: &str, device_id: u16) -> Source {
    Source::Video {
        core: SourceCore {
            core: core(n, label),
            grain_rate: None,
            caps: BTreeMap::new(),
            device_id: id(device_id),
            parents: Vec::new(),
            clock_name: None,
        },
    }
}

/// A resource tree assembled from fabricated parts.
pub struct TreeBuilder {
    tree: ResourceTree,
}

impl TreeBuilder {
    pub fn new(node_n: u16, label: &str) -> Self {
        Self {
            tree: ResourceTree {
                version: ApiVersion::new(1, 3),
                node: node(node_n, label),
                devices: Vec::new(),
                senders: Vec::new(),
                receivers: Vec::new(),
                flows: Vec::new(),
                sources: Vec::new(),
            },
        }
    }

    pub fn device(mut self, device: Device) -> Self {
        self.tree.devices.push(device);
        self
    }

    pub fn sender(mut self, sender: Sender) -> Self {
        self.tree.senders.push(sender);
        self
    }

    pub fn receiver(mut self, receiver: Receiver) -> Self {
        self.tree.receivers.push(receiver);
        self
    }

    pub fn flow(mut self, flow: Flow) -> Self {
        self.tree.flows.push(flow);
        self
    }

    pub fn source(mut self, source: Source) -> Self {
        self.tree.sources.push(source);
        self
    }

    pub fn build(self) -> ResourceTree {
        self.tree
    }
}

/// The bench converter's shape: one Node, three Devices, three Senders and
/// three Receivers each, the Senders on a Device sharing one label.
pub fn bench_tree() -> ResourceTree {
    let mut builder = TreeBuilder::new(1, "Converter");
    let media = ["video/raw", "audio/L24", "video/smpte291"];

    for port in 0..3u16 {
        let device_n = 10 + port;
        let label = format!("SDI {}", port + 1);
        builder = builder.device(device(device_n, &label, 1));

        for (index, media_type) in media.iter().enumerate() {
            let index = u16::try_from(index).expect("three fits in a u16");
            let sender_n = 100 + port * 10 + index;
            let flow_n = 200 + port * 10 + index;
            let source_n = 300 + port * 10 + index;
            let receiver_n = 400 + port * 10 + index;

            let one = sender(sender_n, &label, device_n, Some(flow_n));
            let one = if port == 0 { transmitting(one) } else { one };

            builder = builder
                .sender(one)
                .flow(flow(flow_n, &label, device_n, source_n, media_type))
                .source(source(source_n, &label, device_n))
                .receiver(receiver(
                    receiver_n,
                    &format!("{label}/{media_type}"),
                    device_n,
                    media_type,
                ));
        }
    }

    builder.build()
}
