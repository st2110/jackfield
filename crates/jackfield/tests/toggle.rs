//! What a keystroke on a highlighted resource asks the engine for.
//!
//! The decision is tested without a running engine, because the decision is
//! where the rules are: a Sender is toggled between Transmitting and Idle, a
//! Receiver between Subscribed and Unsubscribed, and a Receiver with nothing to
//! take produces no command at all rather than a guess.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jackfield::toggle_command;
use jackfield_engine::{
    Command, DeviceView, KnownNode, Media, NodeContents, NodeKey, NodeState, Pairing, ReceiverView,
    SenderView, Snapshot, Transport,
};
use jackfield_tui::App;
use nmos::{
    Device, MediaType, Node, NodeApi, Receiver, ReceiverCaps, ReceiverSubscription, Reception,
    ResourceCore, ResourceId, Sender, SenderSubscription, Transmission,
};

fn id(n: u16) -> ResourceId {
    format!("{n:08x}-0000-4000-8000-000000000000")
        .parse()
        .unwrap()
}

fn core(n: u16, label: &str) -> ResourceCore {
    ResourceCore {
        id: id(n),
        version: "1441700172:0".parse().unwrap(),
        label: label.to_owned(),
        description: String::new(),
        tags: BTreeMap::new(),
    }
}

fn sender_view(transmission: Transmission) -> SenderView {
    SenderView {
        sender: Sender {
            core: core(100, "SDI 1"),
            caps: BTreeMap::new(),
            flow_id: None,
            transport: "urn:x-nmos:transport:rtp.mcast".to_owned(),
            device_id: id(10),
            manifest_href: None,
            interface_bindings: Vec::new(),
            subscription: SenderSubscription::new(transmission, None),
        },
        transmission,
        media: Media::None,
        transport: Transport::Pending,
        taken_by: Vec::new(),
        requested: None,
    }
}

fn receiver_view(reception: Reception) -> ReceiverView {
    ReceiverView {
        receiver: Receiver {
            core: core(400, "SDI 1 in"),
            device_id: id(10),
            transport: "urn:x-nmos:transport:rtp.mcast".to_owned(),
            interface_bindings: Vec::new(),
            subscription: ReceiverSubscription::new(reception, None),
            caps: ReceiverCaps::Video {
                caps: nmos::MediaCaps {
                    media_types: vec!["video/raw".parse::<MediaType>().unwrap()],
                    event_types: Vec::new(),
                },
            },
        },
        reception,
        accepts: Vec::new(),
        transport: Transport::Pending,
        pairing: Pairing::None,
        requested: None,
    }
}

/// One Node with one Device, holding one Sender and one Receiver.
fn snapshot(transmission: Transmission, reception: Reception) -> Arc<Snapshot> {
    let contents = NodeContents {
        node: Node {
            core: core(1, "Converter"),
            href: "http://10.77.1.90:8090/".to_owned(),
            hostname: None,
            api: NodeApi {
                versions: vec!["v1.3".to_owned()],
                endpoints: Vec::new(),
            },
            caps: BTreeMap::new(),
            services: Vec::new(),
            clocks: Vec::new(),
            interfaces: Vec::new(),
        },
        devices: vec![DeviceView {
            device: Device {
                core: core(10, "SDI 1"),
                kind: "urn:x-nmos:device:generic".to_owned(),
                node_id: id(1),
                senders: Vec::new(),
                receivers: Vec::new(),
                controls: Vec::new(),
            },
            senders: vec![sender_view(transmission)],
            receivers: vec![receiver_view(reception)],
        }],
        orphans: Vec::new(),
    };

    Arc::new(Snapshot {
        nodes: vec![KnownNode {
            key: NodeKey::Settled(id(1)),
            instances: BTreeSet::new(),
            endpoints: BTreeSet::new(),
            hostname: None,
            requires_authorization: false,
            state: NodeState::Ready(Box::new(contents)),
        }],
        generation: 1,
    })
}

/// An interface looking at that Node's resources, highlighting row `row`.
fn app(transmission: Transmission, reception: Reception, row: usize) -> App {
    let mut app = App::new();
    app.apply(snapshot(transmission, reception));
    app.enter();
    for _ in 0..row {
        app.detail_next();
    }
    app
}

const SENDER_ROW: usize = 0;
const RECEIVER_ROW: usize = 1;

#[test]
fn an_idle_sender_is_asked_to_transmit() {
    let app = app(Transmission::Idle, Reception::Unsubscribed, SENDER_ROW);

    assert_eq!(
        toggle_command(&app),
        Some(Command::StartTransmitting {
            node: NodeKey::Settled(id(1)),
            sender: id(100),
        })
    );
}

#[test]
fn a_transmitting_sender_is_asked_to_stop() {
    let app = app(
        Transmission::Transmitting,
        Reception::Unsubscribed,
        SENDER_ROW,
    );

    assert_eq!(
        toggle_command(&app),
        Some(Command::StopTransmitting {
            node: NodeKey::Settled(id(1)),
            sender: id(100),
        })
    );
}

#[test]
fn a_subscribed_receiver_is_asked_to_take_nothing() {
    let app = app(Transmission::Idle, Reception::Subscribed, RECEIVER_ROW);

    assert_eq!(
        toggle_command(&app),
        Some(Command::Unsubscribe {
            node: NodeKey::Settled(id(1)),
            receiver: id(400),
        })
    );
}

#[test]
fn an_unsubscribed_receiver_with_no_marked_sender_asks_for_nothing() {
    // Connecting a Receiver to whatever happened to be nearby is not a guess a
    // controller may make. The operator names the source first.
    let app = app(Transmission::Idle, Reception::Unsubscribed, RECEIVER_ROW);

    assert_eq!(toggle_command(&app), None);
}

#[test]
fn an_unsubscribed_receiver_takes_the_marked_sender() {
    let mut app = app(
        Transmission::Transmitting,
        Reception::Unsubscribed,
        SENDER_ROW,
    );
    app.mark();
    app.detail_next();

    assert_eq!(
        toggle_command(&app),
        Some(Command::Subscribe {
            node: NodeKey::Settled(id(1)),
            receiver: id(400),
            from: NodeKey::Settled(id(1)),
            sender: id(100),
        })
    );
}

#[test]
fn marking_a_receiver_marks_nothing() {
    // Only a Sender is a source. Marking a Receiver must not leave the next
    // subscription pointing at one.
    let mut app = app(Transmission::Idle, Reception::Subscribed, RECEIVER_ROW);
    app.mark();

    assert_eq!(app.marked(), None);
}

#[test]
fn nothing_highlighted_asks_for_nothing() {
    let app = App::new();

    assert_eq!(toggle_command(&app), None);
}
