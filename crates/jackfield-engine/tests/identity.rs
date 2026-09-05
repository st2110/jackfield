//! A Node is what it says it is, not where it answers.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use jackfield_engine::{Advertisement, Identified, Identities, NodeKey};
use nmos::ResourceId;

const CONVERTER: &str = "3b8be755-08ff-452b-b217-c9151eb21193";
const OTHER: &str = "9126cc2f-4c26-4c9b-a6cd-93c4381c9be5";

fn advert(instance: &str, address: [u8; 4], port: u16) -> Advertisement {
    Advertisement::try_from_parts(
        instance,
        None,
        &[IpAddr::V4(Ipv4Addr::from(address))],
        port,
        &BTreeMap::<String, String>::new(),
    )
    .expect("usable")
}

fn id(text: &str) -> ResourceId {
    text.parse().expect("well formed")
}

#[test]
fn an_advertisement_is_keyed_provisionally_until_it_is_identified() {
    let mut identities = Identities::new();
    let key = identities.observe(&advert("converter", [10, 77, 1, 90], 8090));

    assert!(!key.is_settled());
    assert!(key.to_string().contains("not yet identified"));
}

#[test]
fn a_node_is_re_keyed_to_its_identifier_and_does_not_appear_twice() {
    let mut identities = Identities::new();
    identities.observe(&advert("converter", [10, 77, 1, 90], 8090));

    match identities.identify("converter", &id(CONVERTER)) {
        Identified::Settled { was, now } => {
            assert!(!was.is_settled());
            assert_eq!(now, NodeKey::Settled(id(CONVERTER)));
        }
        other => panic!("expected the key to settle, got {other:?}"),
    }

    assert_eq!(
        identities.len(),
        1,
        "re-keying must not leave the old key behind"
    );
}

#[test]
fn one_node_advertised_at_two_addresses_collapses_into_one() {
    // The case endpoint-as-identity gets wrong: ST 2022-7 equipment has two
    // media NICs, and often a management port besides.
    let mut identities = Identities::new();
    identities.observe(&advert("converter-media-1", [10, 77, 1, 90], 8090));
    identities.observe(&advert("converter-media-2", [10, 78, 1, 90], 8090));
    assert_eq!(
        identities.len(),
        2,
        "before identification they cannot be told apart"
    );

    identities.identify("converter-media-1", &id(CONVERTER));
    match identities.identify("converter-media-2", &id(CONVERTER)) {
        Identified::Merged { now, .. } => assert_eq!(now, NodeKey::Settled(id(CONVERTER))),
        other => panic!("expected a merge, got {other:?}"),
    }

    assert_eq!(identities.len(), 1, "one box, one row");
    let instances = identities.instances_for(&NodeKey::Settled(id(CONVERTER)));
    assert_eq!(instances, ["converter-media-1", "converter-media-2"]);
}

#[test]
fn two_nodes_on_one_host_stay_distinct() {
    // Same address, different ports, different identifiers: two Nodes.
    let mut identities = Identities::new();
    identities.observe(&advert("first", [10, 77, 1, 90], 8090));
    identities.observe(&advert("second", [10, 77, 1, 90], 8091));

    identities.identify("first", &id(CONVERTER));
    identities.identify("second", &id(OTHER));

    assert_eq!(identities.len(), 2);
}

#[test]
fn identifying_the_same_node_again_changes_nothing() {
    let mut identities = Identities::new();
    identities.observe(&advert("converter", [10, 77, 1, 90], 8090));
    identities.identify("converter", &id(CONVERTER));

    match identities.identify("converter", &id(CONVERTER)) {
        Identified::Unchanged(key) => assert_eq!(key, NodeKey::Settled(id(CONVERTER))),
        other => panic!("expected no change, got {other:?}"),
    }
    assert_eq!(identities.len(), 1);
}

#[test]
fn losing_one_route_to_a_node_does_not_lose_the_node() {
    let mut identities = Identities::new();
    identities.observe(&advert("converter-media-1", [10, 77, 1, 90], 8090));
    identities.observe(&advert("converter-media-2", [10, 78, 1, 90], 8090));
    identities.identify("converter-media-1", &id(CONVERTER));
    identities.identify("converter-media-2", &id(CONVERTER));

    identities.forget("converter-media-1");

    assert_eq!(
        identities.len(),
        1,
        "the box is still there on its other address"
    );
    assert_eq!(
        identities.instances_for(&NodeKey::Settled(id(CONVERTER))),
        ["converter-media-2"]
    );
}

#[test]
fn forgetting_the_last_route_forgets_the_node() {
    let mut identities = Identities::new();
    identities.observe(&advert("converter", [10, 77, 1, 90], 8090));
    identities.identify("converter", &id(CONVERTER));

    assert_eq!(
        identities.forget("converter"),
        Some(NodeKey::Settled(id(CONVERTER)))
    );
    assert!(identities.is_empty());
    assert_eq!(identities.forget("converter"), None);
}

#[test]
fn a_node_identified_before_it_was_observed_is_still_keyed() {
    // Ordering on a live network is not guaranteed; nothing here may depend on
    // having seen the advertisement first.
    let mut identities = Identities::new();
    match identities.identify("converter", &id(CONVERTER)) {
        Identified::Settled { now, .. } => assert_eq!(now, NodeKey::Settled(id(CONVERTER))),
        other => panic!("expected the key to settle, got {other:?}"),
    }
    assert_eq!(
        identities.key_for("converter"),
        Some(&NodeKey::Settled(id(CONVERTER)))
    );
}

#[test]
fn one_advertisement_on_three_interfaces_is_one_node() {
    // The bench converter advertises on three interfaces at one address, under
    // one instance name.
    let mut identities = Identities::new();
    let advertisement = advert("converter", [10, 77, 1, 90], 8090);
    identities.observe(&advertisement);
    identities.observe(&advertisement);
    identities.observe(&advertisement);
    assert_eq!(identities.len(), 1);
}
