//! The inventory: what the controller holds, and what it says about the gaps.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use jackfield_engine::{Inventory, Media, NodeKey, NodeState, OrphanKind, Pairing, Transport};
use jackfield_nmos::{
    ReceiverLeg, ReceiverTransport, Reception, SenderLeg, SenderTransport, StreamAddress,
    Transmission,
};
use support::{
    TreeBuilder, advertisement, bench_tree, device, flow, id, receiver, sender, source, subscribed,
    transmitting,
};

fn multicast(last: u8, port: u16) -> StreamAddress {
    StreamAddress {
        address: format!("239.255.0.{last}"),
        port,
    }
}

fn sender_transport(streams: &[StreamAddress]) -> SenderTransport {
    SenderTransport {
        receiver_id: None,
        legs: streams
            .iter()
            .map(|stream| SenderLeg {
                destination_ip: Some(stream.address.clone()),
                destination_port: Some(stream.port),
                source_ip: None,
                rtp_enabled: true,
            })
            .collect(),
    }
}

fn receiver_transport(streams: &[StreamAddress]) -> ReceiverTransport {
    ReceiverTransport {
        sender_id: None,
        legs: streams
            .iter()
            .map(|stream| ReceiverLeg {
                multicast_ip: Some(stream.address.clone()),
                source_ip: None,
                destination_port: Some(stream.port),
                rtp_enabled: true,
            })
            .collect(),
        has_transport_file: true,
    }
}

// --- state transitions ------------------------------------------------------

#[test]
fn a_discovered_node_is_listed_as_loading_rather_than_hidden() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));

    let node = inventory.node(&key).expect("the Node is held");
    assert_eq!(node.state, NodeState::Loading);
    assert_eq!(node.endpoints.len(), 1);
    assert_eq!(inventory.len(), 1);
}

#[test]
fn a_node_that_fails_keeps_its_reason_and_does_not_poison_the_others() {
    let mut inventory = Inventory::new();
    let healthy = inventory.observe(&advertisement("healthy", [10, 77, 1, 90], 8090));
    let broken = inventory.observe(&advertisement("broken", [10, 77, 1, 91], 8090));

    inventory.set_tree(&healthy, bench_tree());
    inventory.set_failure(&broken, "connection refused");

    assert!(inventory.node(&healthy).unwrap().state.is_ready());
    assert_eq!(
        inventory.node(&broken).unwrap().state.failure(),
        Some("connection refused")
    );
    assert_eq!(inventory.node(&healthy).unwrap().devices().len(), 3);
}

#[test]
fn a_failed_node_becomes_browsable_when_it_starts_answering() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_failure(&key, "timed out");
    inventory.set_tree(&key, bench_tree());

    assert!(inventory.node(&key).unwrap().state.is_ready());
    assert_eq!(inventory.node(&key).unwrap().state.failure(), None);
}

#[test]
fn a_node_surviving_on_its_second_endpoint_is_not_reported_as_failed() {
    let mut inventory = Inventory::new();
    inventory.observe(&advertisement("media-1", [10, 77, 1, 90], 8090));
    inventory.observe(&advertisement("media-2", [10, 78, 1, 90], 8090));
    inventory.identify("media-1", &id(1));
    let key = inventory.identify("media-2", &id(1));
    inventory.set_tree(&key, bench_tree());

    inventory.depart("media-1");

    let node = inventory.node(&key).expect("still there");
    assert!(
        node.state.is_ready(),
        "losing one route is not losing the box"
    );
    assert_eq!(inventory.len(), 1);
}

#[test]
fn departing_the_last_route_removes_the_node() {
    let mut inventory = Inventory::new();
    inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.depart("converter");
    assert!(inventory.is_empty());
}

#[test]
fn a_multi_homed_node_occupies_one_row_with_both_endpoints() {
    let mut inventory = Inventory::new();
    inventory.observe(&advertisement("media-1", [10, 77, 1, 90], 8090));
    inventory.observe(&advertisement("media-2", [10, 78, 1, 90], 8090));
    assert_eq!(
        inventory.len(),
        2,
        "before identification they cannot be told apart"
    );

    inventory.identify("media-1", &id(1));
    let key = inventory.identify("media-2", &id(1));

    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory.node(&key).unwrap().endpoints.len(), 2);
    assert!(matches!(key, NodeKey::Settled(_)));
}

#[test]
fn a_refresh_replaces_the_tree_so_a_removed_sender_disappears() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));

    let two = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .sender(sender(100, "SDI 1", 10, None))
        .sender(sender(101, "SDI 1", 10, None))
        .build();
    inventory.set_tree(&key, two);
    assert_eq!(inventory.node(&key).unwrap().devices()[0].senders.len(), 2);

    let one = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .sender(sender(100, "SDI 1", 10, None))
        .build();
    inventory.set_tree(&key, one);
    assert_eq!(inventory.node(&key).unwrap().devices()[0].senders.len(), 1);
}

// --- naming -----------------------------------------------------------------

#[test]
fn a_node_without_a_label_falls_back_to_its_hostname_then_its_address() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));

    // Loading, so only the hostname is known. The trailing dot of a DNS name
    // is not something to show an operator.
    assert_eq!(
        inventory.node(&key).unwrap().display_name(),
        "converter.local"
    );

    let unlabelled = TreeBuilder::new(1, "")
        .device(device(10, "SDI 1", 1))
        .build();
    inventory.set_tree(&key, unlabelled);
    assert_eq!(
        inventory.node(&key).unwrap().display_name(),
        "converter.local"
    );

    let labelled = TreeBuilder::new(1, "Converter").build();
    inventory.set_tree(&key, labelled);
    assert_eq!(inventory.node(&key).unwrap().display_name(), "Converter");
}

#[test]
fn a_node_with_neither_label_nor_hostname_falls_back_to_its_address() {
    use std::collections::BTreeMap;
    use std::net::{IpAddr, Ipv4Addr};

    let mut inventory = Inventory::new();
    let nameless = jackfield_engine::Advertisement::try_from_parts(
        "converter",
        None,
        &[IpAddr::V4(Ipv4Addr::new(10, 77, 1, 90))],
        8090,
        &BTreeMap::<String, String>::new(),
    )
    .expect("usable");
    let key = inventory.observe(&nameless);
    inventory.set_tree(&key, TreeBuilder::new(1, "").build());

    assert_eq!(
        inventory.node(&key).unwrap().display_name(),
        "http://10.77.1.90:8090",
        "a blank row cannot be pointed at or reported"
    );
}

// --- reference resolution ---------------------------------------------------

#[test]
fn senders_and_receivers_are_attributed_to_the_device_that_owns_them() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, bench_tree());

    let devices = inventory.node(&key).unwrap().devices();
    assert_eq!(devices.len(), 3, "one Device per physical port");
    for device in devices {
        assert_eq!(device.senders.len(), 3);
        assert_eq!(device.receivers.len(), 3);
        for sender in &device.senders {
            assert_eq!(sender.sender.device_id, device.device.core.id);
        }
    }
}

#[test]
fn a_devices_media_types_tell_its_identically_labelled_senders_apart() {
    // The bench converter presents three Senders on one Device, all labelled
    // `SDI 1`. Without the media type the rows are indistinguishable.
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, bench_tree());

    let device = &inventory.node(&key).unwrap().devices()[0];
    let labels: Vec<&str> = device
        .senders
        .iter()
        .map(|s| s.sender.core.label.as_str())
        .collect();
    assert_eq!(labels, ["SDI 1", "SDI 1", "SDI 1"]);

    let media: Vec<String> = device.senders.iter().map(|s| s.media.to_string()).collect();
    assert_eq!(media, ["video/raw", "audio/L24", "video/smpte291"]);
}

#[test]
fn a_dangling_flow_reference_is_unresolved_and_the_rest_stays_usable() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));

    let tree = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .sender(sender(100, "SDI 1", 10, Some(999)))
        .sender(sender(101, "SDI 1", 10, Some(200)))
        .flow(flow(200, "SDI 1", 10, 300, "video/raw"))
        .source(source(300, "SDI 1", 10))
        .build();
    inventory.set_tree(&key, tree);

    let device = &inventory.node(&key).unwrap().devices()[0];
    assert!(matches!(device.senders[0].media, Media::Unresolved { .. }));
    assert_eq!(device.senders[0].media.to_string(), "unknown media");
    assert!(matches!(device.senders[1].media, Media::Known { .. }));
}

#[test]
fn a_sender_with_no_flow_says_so_rather_than_showing_a_blank() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    let tree = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .sender(sender(100, "SDI 1", 10, None))
        .build();
    inventory.set_tree(&key, tree);

    let device = &inventory.node(&key).unwrap().devices()[0];
    assert_eq!(device.senders[0].media, Media::None);
    assert_eq!(device.senders[0].media.to_string(), "no flow");
}

#[test]
fn a_resource_whose_device_is_missing_is_reported_not_dropped() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    let tree = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .sender(sender(100, "orphan", 999, None))
        .build();
    inventory.set_tree(&key, tree);

    let NodeState::Ready(contents) = &inventory.node(&key).unwrap().state else {
        panic!("expected a ready Node");
    };
    assert_eq!(contents.orphans.len(), 1);
    assert_eq!(contents.orphans[0].kind, OrphanKind::Sender);
    assert!(contents.devices[0].senders.is_empty());
}

#[test]
fn a_device_with_nothing_on_it_is_still_shown() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    let tree = TreeBuilder::new(1, "Converter")
        .device(device(10, "SDI 1", 1))
        .build();
    inventory.set_tree(&key, tree);

    let devices = inventory.node(&key).unwrap().devices();
    assert_eq!(devices.len(), 1);
    assert!(devices[0].is_empty());
}

#[test]
fn a_node_exposing_no_devices_is_not_an_error() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, TreeBuilder::new(1, "Converter").build());

    assert!(inventory.node(&key).unwrap().state.is_ready());
    assert!(inventory.node(&key).unwrap().devices().is_empty());
}

// --- connection state -------------------------------------------------------

#[test]
fn connection_state_is_available_from_the_resource_tree_alone() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, bench_tree());

    let devices = inventory.node(&key).unwrap().devices();
    // Only SDI 1 is live on the bench.
    for sender in &devices[0].senders {
        assert_eq!(sender.transmission, Transmission::Transmitting);
        assert!(
            sender.transport.is_pending(),
            "no Connection API has been read"
        );
    }
    for sender in &devices[1].senders {
        assert_eq!(sender.transmission, Transmission::Idle);
    }
    for receiver in &devices[0].receivers {
        assert_eq!(receiver.reception, Reception::Unsubscribed);
    }
}

#[test]
fn a_transmitting_sender_whose_transport_is_unread_is_pending_not_absent() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, bench_tree());
    inventory.resolve_connections();

    let sender = &inventory.node(&key).unwrap().devices()[0].senders[0];
    assert_eq!(sender.transmission, Transmission::Transmitting);
    assert!(sender.transport.is_pending());
    assert!(
        !sender.is_transmitting_into_the_void(),
        "a Sender whose takers are unknown is not a Sender with none"
    );
}

// --- the connection graph ---------------------------------------------------

#[test]
fn a_receiver_naming_its_sender_on_another_node_pairs_on_both_ends() {
    let mut inventory = Inventory::new();
    let sending = inventory.observe(&advertisement("source-box", [10, 77, 1, 90], 8090));
    let taking = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));

    inventory.set_tree(
        &sending,
        TreeBuilder::new(1, "Source")
            .device(device(10, "SDI 1", 1))
            .sender(transmitting(sender(100, "SDI 1", 10, None)))
            .build(),
    );
    inventory.set_tree(
        &taking,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(
                receiver(400, "SDI 1", 20, "video/raw"),
                Some(100),
            ))
            .build(),
    );
    inventory.resolve_connections();

    let receiver = &inventory.node(&taking).unwrap().devices()[0].receivers[0];
    match &receiver.pairing {
        Pairing::Resolved(sender) => {
            assert_eq!(sender.node_label, "Source");
            assert_eq!(sender.id, id(100));
        }
        other => panic!("expected a resolved pairing, got {other:?}"),
    }

    let sender = &inventory.node(&sending).unwrap().devices()[0].senders[0];
    assert_eq!(sender.taken_by.len(), 1);
    assert_eq!(sender.taken_by[0].node_label, "Sink");
}

#[test]
fn a_receiver_naming_nobody_is_paired_by_its_stream_address() {
    // The bench case: neither end reports an identifier, so the multicast group
    // is the only evidence there is.
    let mut inventory = Inventory::new();
    let sending = inventory.observe(&advertisement("source-box", [10, 77, 1, 90], 8090));
    let taking = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));

    inventory.set_tree(
        &sending,
        TreeBuilder::new(1, "Source")
            .device(device(10, "SDI 1", 1))
            .sender(transmitting(sender(100, "SDI 1", 10, None)))
            .build(),
    );
    inventory.set_tree(
        &taking,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(receiver(400, "SDI 1", 20, "video/raw"), None))
            .build(),
    );

    let stream = multicast(190, 5004);
    inventory.set_sender_transport(
        &id(100),
        Ok(sender_transport(std::slice::from_ref(&stream))),
    );
    inventory.set_receiver_transport(&id(400), Ok(receiver_transport(&[stream])));
    inventory.resolve_connections();

    let receiver = &inventory.node(&taking).unwrap().devices()[0].receivers[0];
    assert!(
        matches!(receiver.pairing, Pairing::Resolved(_)),
        "got {:?}",
        receiver.pairing
    );

    let sender = &inventory.node(&sending).unwrap().devices()[0].senders[0];
    assert_eq!(sender.taken_by.len(), 1);
}

#[test]
fn one_sender_feeding_three_receivers_across_two_nodes_carries_all_three() {
    let mut inventory = Inventory::new();
    let sending = inventory.observe(&advertisement("source-box", [10, 77, 1, 90], 8090));
    let first = inventory.observe(&advertisement("sink-a", [10, 77, 1, 91], 8090));
    let second = inventory.observe(&advertisement("sink-b", [10, 77, 1, 92], 8090));

    inventory.set_tree(
        &sending,
        TreeBuilder::new(1, "Source")
            .device(device(10, "SDI 1", 1))
            .sender(transmitting(sender(100, "SDI 1", 10, None)))
            .build(),
    );
    inventory.set_tree(
        &first,
        TreeBuilder::new(2, "Sink A")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(receiver(400, "one", 20, "video/raw"), Some(100)))
            .receiver(subscribed(receiver(401, "two", 20, "video/raw"), Some(100)))
            .build(),
    );
    inventory.set_tree(
        &second,
        TreeBuilder::new(3, "Sink B")
            .device(device(30, "SDI 1", 3))
            .receiver(subscribed(
                receiver(402, "three", 30, "video/raw"),
                Some(100),
            ))
            .build(),
    );
    inventory.resolve_connections();

    let sender = &inventory.node(&sending).unwrap().devices()[0].senders[0];
    assert_eq!(sender.taken_by.len(), 3);
    let nodes: Vec<&str> = sender
        .taken_by
        .iter()
        .map(|r| r.node_label.as_str())
        .collect();
    assert!(nodes.contains(&"Sink A") && nodes.contains(&"Sink B"));
}

#[test]
fn a_sender_transmitting_into_the_void_says_so() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("source-box", [10, 77, 1, 90], 8090));
    inventory.set_tree(
        &key,
        TreeBuilder::new(1, "Source")
            .device(device(10, "SDI 1", 1))
            .sender(transmitting(sender(100, "SDI 1", 10, None)))
            .build(),
    );
    inventory.set_sender_transport(&id(100), Ok(sender_transport(&[multicast(190, 5004)])));
    inventory.resolve_connections();

    let sender = &inventory.node(&key).unwrap().devices()[0].senders[0];
    assert!(sender.taken_by.is_empty());
    assert!(sender.is_transmitting_into_the_void());
    assert_eq!(sender.transmission, Transmission::Transmitting);
}

// --- unresolvable pairings --------------------------------------------------

#[test]
fn a_receiver_naming_an_undiscovered_sender_stays_subscribed() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));
    inventory.set_tree(
        &key,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(
                receiver(400, "SDI 1", 20, "video/raw"),
                Some(999),
            ))
            .build(),
    );
    inventory.resolve_connections();

    let receiver = &inventory.node(&key).unwrap().devices()[0].receivers[0];
    assert_eq!(
        receiver.reception,
        Reception::Subscribed,
        "never Unsubscribed"
    );
    match &receiver.pairing {
        Pairing::UnknownSender { sender_id } => assert_eq!(*sender_id, id(999)),
        other => panic!("expected an unknown Sender, got {other:?}"),
    }
}

#[test]
fn discovering_the_missing_node_resolves_the_edge_on_both_ends() {
    let mut inventory = Inventory::new();
    let taking = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));
    inventory.set_tree(
        &taking,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(
                receiver(400, "SDI 1", 20, "video/raw"),
                Some(100),
            ))
            .build(),
    );
    inventory.resolve_connections();
    assert!(matches!(
        inventory.node(&taking).unwrap().devices()[0].receivers[0].pairing,
        Pairing::UnknownSender { .. }
    ));

    let sending = inventory.observe(&advertisement("source-box", [10, 77, 1, 90], 8090));
    inventory.set_tree(
        &sending,
        TreeBuilder::new(1, "Source")
            .device(device(10, "SDI 1", 1))
            .sender(transmitting(sender(100, "SDI 1", 10, None)))
            .build(),
    );
    inventory.resolve_connections();

    assert!(matches!(
        inventory.node(&taking).unwrap().devices()[0].receivers[0].pairing,
        Pairing::Resolved(_)
    ));
    assert_eq!(
        inventory.node(&sending).unwrap().devices()[0].senders[0]
            .taken_by
            .len(),
        1
    );
}

#[test]
fn two_senders_in_one_multicast_group_are_reported_as_ambiguous() {
    // A fault in the plant. Picking one would destroy the only evidence of it.
    let mut inventory = Inventory::new();
    let sending = inventory.observe(&advertisement("source-box", [10, 77, 1, 90], 8090));
    let taking = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));

    inventory.set_tree(
        &sending,
        TreeBuilder::new(1, "Source")
            .device(device(10, "SDI 1", 1))
            .sender(transmitting(sender(100, "first", 10, None)))
            .sender(transmitting(sender(101, "second", 10, None)))
            .build(),
    );
    inventory.set_tree(
        &taking,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(receiver(400, "SDI 1", 20, "video/raw"), None))
            .build(),
    );

    let stream = multicast(190, 5004);
    inventory.set_sender_transport(
        &id(100),
        Ok(sender_transport(std::slice::from_ref(&stream))),
    );
    inventory.set_sender_transport(
        &id(101),
        Ok(sender_transport(std::slice::from_ref(&stream))),
    );
    inventory.set_receiver_transport(&id(400), Ok(receiver_transport(&[stream])));
    inventory.resolve_connections();

    let receiver = &inventory.node(&taking).unwrap().devices()[0].receivers[0];
    match &receiver.pairing {
        Pairing::Ambiguous { candidates } => {
            assert_eq!(candidates.len(), 2);
            let labels: Vec<&str> = candidates.iter().map(|c| c.label.as_str()).collect();
            assert!(labels.contains(&"first") && labels.contains(&"second"));
        }
        other => panic!("expected an ambiguous pairing, got {other:?}"),
    }
    assert_eq!(receiver.reception, Reception::Subscribed);
}

#[test]
fn an_unsubscribed_receiver_has_no_pairing() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));
    inventory.set_tree(
        &key,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(receiver(400, "SDI 1", 20, "video/raw"))
            .build(),
    );
    inventory.resolve_connections();

    let receiver = &inventory.node(&key).unwrap().devices()[0].receivers[0];
    assert_eq!(receiver.pairing, Pairing::None);
    assert_eq!(receiver.reception, Reception::Unsubscribed);
}

#[test]
fn a_subscribed_receiver_whose_transport_is_unread_is_pending_not_unmatched() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));
    inventory.set_tree(
        &key,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(receiver(400, "SDI 1", 20, "video/raw"), None))
            .build(),
    );
    inventory.resolve_connections();

    let receiver = &inventory.node(&key).unwrap().devices()[0].receivers[0];
    assert_eq!(receiver.pairing, Pairing::Pending);
    assert_eq!(receiver.reception, Reception::Subscribed);
}

#[test]
fn a_receiver_whose_stream_matches_nothing_known_is_unmatched() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("sink-box", [10, 77, 1, 91], 8090));
    inventory.set_tree(
        &key,
        TreeBuilder::new(2, "Sink")
            .device(device(20, "SDI 1", 2))
            .receiver(subscribed(receiver(400, "SDI 1", 20, "video/raw"), None))
            .build(),
    );
    inventory.set_receiver_transport(&id(400), Ok(receiver_transport(&[multicast(9, 5004)])));
    inventory.resolve_connections();

    let receiver = &inventory.node(&key).unwrap().devices()[0].receivers[0];
    assert_eq!(receiver.pairing, Pairing::Unmatched);
    assert_eq!(receiver.reception, Reception::Subscribed);
}

#[test]
fn a_node_whose_connection_api_is_unreachable_keeps_its_state() {
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, bench_tree());
    inventory.set_sender_transport(&id(100), Err("connection refused".to_owned()));
    inventory.resolve_connections();

    let sender = &inventory.node(&key).unwrap().devices()[0].senders[0];
    assert_eq!(
        sender.transmission,
        Transmission::Transmitting,
        "state came from IS-04"
    );
    assert!(matches!(sender.transport, Transport::Unavailable { .. }));
}

#[test]
fn transport_results_survive_a_tree_being_re_read() {
    // The two tiers refresh on different clocks; a fresh tree must not throw
    // away the slower pass's work.
    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.set_tree(&key, bench_tree());
    inventory.set_sender_transport(&id(100), Ok(sender_transport(&[multicast(190, 5004)])));

    inventory.set_tree(&key, bench_tree());

    let sender = &inventory.node(&key).unwrap().devices()[0].senders[0];
    assert_eq!(sender.transport.streams(), [multicast(190, 5004)]);
}

// --- nothing is written -----------------------------------------------------

#[test]
fn a_full_cycle_writes_nothing_to_disk() {
    let scratch = std::env::temp_dir().join(format!("jackfield-inventory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch directory");

    let before = listing(&scratch);

    let mut inventory = Inventory::new();
    let key = inventory.observe(&advertisement("converter", [10, 77, 1, 90], 8090));
    inventory.identify("converter", &id(1));
    inventory.set_tree(&key, bench_tree());
    inventory.set_sender_transport(&id(100), Ok(sender_transport(&[multicast(190, 5004)])));
    inventory.resolve_connections();
    drop(inventory);

    assert_eq!(listing(&scratch), before, "the inventory wrote to disk");
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn every_run_starts_empty() {
    let inventory = Inventory::new();
    assert!(inventory.is_empty());
    assert_eq!(inventory.nodes().count(), 0);
}

fn listing(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
