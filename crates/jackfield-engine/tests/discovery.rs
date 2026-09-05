//! Discovery, driven by fabricated advertisements.
//!
//! No network is involved in any of this. mDNS is where interoperability goes
//! wrong in practice, so the layer is kept behind an interface the tests can
//! drive, and the real check against equipment is a bench run — see the
//! interoperability risk in the change's design.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use jackfield_engine::{
    Advertisement, Collection, Discovery, DiscoveryEvent, Endpoint, FabricatedDiscovery,
    VersionCounters,
};
use nmos::Protocol;

/// The TXT records the bench converter actually publishes.
fn bench_txt() -> BTreeMap<String, String> {
    [
        ("api_proto", "http"),
        ("api_ver", "v1.0,v1.1,v1.2,v1.3"),
        ("api_auth", "false"),
        ("ver_slf", "3"),
        ("ver_dvc", "5"),
        ("ver_snd", "17"),
        ("ver_rcv", "9"),
        ("ver_flw", "4"),
        ("ver_src", "4"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn advertisement(
    instance: &str,
    addresses: &[IpAddr],
    txt: BTreeMap<String, String>,
) -> Advertisement {
    Advertisement::try_from_parts(
        instance,
        Some("converter.local.".to_owned()),
        addresses,
        8090,
        &txt,
    )
    .expect("these parts make a usable advertisement")
}

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

// --- events -----------------------------------------------------------------

#[tokio::test]
async fn a_fabricated_advertisement_produces_an_appearance_without_any_network() {
    let discovery = FabricatedDiscovery::new();
    let mut events = discovery.subscribe().events;

    discovery.advertise(advertisement(
        "converter",
        &[v4(10, 77, 1, 90)],
        bench_txt(),
    ));

    match events.recv().await.expect("an event arrives") {
        DiscoveryEvent::Appeared(node) => {
            assert_eq!(node.instance, "converter");
            assert_eq!(node.endpoints.len(), 1);
        }
        other => panic!("expected an appearance, got {other:?}"),
    }
}

#[tokio::test]
async fn a_goodbye_produces_a_departure_and_a_return_produces_a_fresh_appearance() {
    let discovery = FabricatedDiscovery::new();
    let mut events = discovery.subscribe().events;

    discovery.advertise(advertisement(
        "converter",
        &[v4(10, 77, 1, 90)],
        bench_txt(),
    ));
    discovery.withdraw("converter");
    discovery.advertise(advertisement(
        "converter",
        &[v4(10, 77, 1, 90)],
        bench_txt(),
    ));

    assert!(matches!(
        events.recv().await.unwrap(),
        DiscoveryEvent::Appeared(_)
    ));
    match events.recv().await.unwrap() {
        DiscoveryEvent::Departed { instance } => assert_eq!(instance, "converter"),
        other => panic!("expected a departure, got {other:?}"),
    }
    assert!(matches!(
        events.recv().await.unwrap(),
        DiscoveryEvent::Appeared(_)
    ));
}

#[tokio::test]
async fn re_advertising_unchanged_says_nothing() {
    let discovery = FabricatedDiscovery::new();
    let mut events = discovery.subscribe().events;

    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], bench_txt());
    discovery.advertise(advert.clone());
    discovery.advertise(advert);

    assert!(matches!(
        events.recv().await.unwrap(),
        DiscoveryEvent::Appeared(_)
    ));
    assert!(
        events.try_recv().is_err(),
        "an unchanged re-advertisement must not wake the rest of the system"
    );
}

#[tokio::test]
async fn a_counter_moving_reports_that_the_collection_changed() {
    let discovery = FabricatedDiscovery::new();
    let mut events = discovery.subscribe().events;

    discovery.advertise(advertisement(
        "converter",
        &[v4(10, 77, 1, 90)],
        bench_txt(),
    ));
    let mut moved = bench_txt();
    moved.insert("ver_snd".to_owned(), "18".to_owned());
    discovery.advertise(advertisement("converter", &[v4(10, 77, 1, 90)], moved));

    assert!(matches!(
        events.recv().await.unwrap(),
        DiscoveryEvent::Appeared(_)
    ));
    match events.recv().await.unwrap() {
        DiscoveryEvent::Changed {
            advertisement,
            changed,
        } => {
            assert_eq!(changed, vec![Collection::Senders]);
            assert_eq!(advertisement.counters.get(Collection::Senders), Some(18));
        }
        other => panic!("expected a change, got {other:?}"),
    }
}

// --- TXT records ------------------------------------------------------------

#[test]
fn the_advertised_versions_are_read() {
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], bench_txt());
    let versions: Vec<String> = advert.versions.iter().map(ToString::to_string).collect();
    assert_eq!(versions, ["v1.0", "v1.1", "v1.2", "v1.3"]);
}

#[test]
fn an_absent_api_proto_falls_back_to_the_specifications_default() {
    // Dropping the advertisement would lose a Node that is perfectly reachable.
    let mut txt = bench_txt();
    txt.remove("api_proto");
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);

    assert_eq!(
        advert.endpoints.iter().next().unwrap().protocol,
        Protocol::Http
    );
}

#[test]
fn an_https_node_is_addressed_over_https() {
    let mut txt = bench_txt();
    txt.insert("api_proto".to_owned(), "https".to_owned());
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);
    assert_eq!(
        advert.endpoints.iter().next().unwrap().protocol,
        Protocol::Https
    );
}

#[test]
fn a_node_requiring_authorization_is_marked() {
    let mut txt = bench_txt();
    txt.insert("api_auth".to_owned(), "true".to_owned());
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);
    assert!(advert.requires_authorization);

    let default = advertisement("converter", &[v4(10, 77, 1, 90)], bench_txt());
    assert!(!default.requires_authorization);
}

#[test]
fn an_absent_api_ver_falls_back_to_the_specifications_default() {
    let mut txt = bench_txt();
    txt.remove("api_ver");
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);
    // IS-04 says v1.0 when nothing is advertised.
    assert_eq!(
        advert
            .versions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["v1.0"]
    );
}

#[test]
fn unparseable_versions_are_dropped_and_the_rest_kept() {
    let mut txt = bench_txt();
    txt.insert("api_ver".to_owned(), "v1.3,nonsense,,v1.2".to_owned());
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);
    assert_eq!(
        advert
            .versions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["v1.2", "v1.3"]
    );
}

// --- version counters -------------------------------------------------------

#[test]
fn every_counter_is_reported() {
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], bench_txt());
    assert_eq!(advert.counters.get(Collection::Node), Some(3));
    assert_eq!(advert.counters.get(Collection::Devices), Some(5));
    assert_eq!(advert.counters.get(Collection::Senders), Some(17));
    assert_eq!(advert.counters.get(Collection::Receivers), Some(9));
    assert_eq!(advert.counters.get(Collection::Flows), Some(4));
    assert_eq!(advert.counters.get(Collection::Sources), Some(4));
}

#[test]
fn an_absent_counter_is_absent_and_not_zero() {
    // Zero is a legitimate value. Reporting an absent counter as zero would
    // falsely imply the collection had been observed.
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], BTreeMap::new());
    for collection in Collection::ALL {
        assert_eq!(advert.counters.get(collection), None, "{collection:?}");
    }
    assert!(advert.counters.is_empty());
}

#[test]
fn a_counter_of_zero_is_a_value() {
    let mut txt = bench_txt();
    txt.insert("ver_snd".to_owned(), "0".to_owned());
    let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);
    assert_eq!(advert.counters.get(Collection::Senders), Some(0));
}

#[test]
fn only_the_collections_whose_counters_moved_are_reported_as_changed() {
    let before = advertisement("converter", &[v4(10, 77, 1, 90)], bench_txt()).counters;
    let mut txt = bench_txt();
    txt.insert("ver_rcv".to_owned(), "10".to_owned());
    let after = advertisement("converter", &[v4(10, 77, 1, 90)], txt).counters;

    assert_eq!(after.changed_since(&before), vec![Collection::Receivers]);
    assert!(after.changed_since(&after).is_empty());
}

#[test]
fn a_counter_appearing_for_the_first_time_is_a_change() {
    let mut without = bench_txt();
    without.remove("ver_snd");
    let before = advertisement("c", &[v4(10, 77, 1, 90)], without).counters;
    let after = advertisement("c", &[v4(10, 77, 1, 90)], bench_txt()).counters;
    assert_eq!(after.changed_since(&before), vec![Collection::Senders]);
}

#[test]
fn a_node_publishing_no_counters_is_reported_as_wholly_changed() {
    // Nothing can be trusted to be current, so everything is re-read. This is
    // the case the periodic floor exists for.
    let before = advertisement("c", &[v4(10, 77, 1, 90)], bench_txt()).counters;
    let after = advertisement("c", &[v4(10, 77, 1, 90)], BTreeMap::new()).counters;
    assert_eq!(after.changed_since(&before), Collection::ALL.to_vec());
}

// --- address families -------------------------------------------------------

#[test]
fn both_address_families_are_kept_and_ipv4_is_preferred() {
    let ipv6 = IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1));
    let advert = advertisement("converter", &[ipv6, v4(10, 77, 1, 90)], bench_txt());

    assert_eq!(advert.endpoints.len(), 2);
    let preferred = advert.preferred_endpoint().expect("an endpoint");
    assert_eq!(preferred.address, v4(10, 77, 1, 90));
}

#[test]
fn an_ipv6_only_node_is_discovered_and_used() {
    let ipv6 = IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1));
    let advert = advertisement("converter", &[ipv6], bench_txt());
    assert_eq!(
        advert.preferred_endpoint().expect("an endpoint").address,
        ipv6
    );
}

#[test]
fn one_node_advertised_on_three_interfaces_has_one_endpoint() {
    // The bench converter does exactly this: three interfaces, one address.
    let advert = advertisement(
        "converter",
        &[v4(10, 77, 1, 90), v4(10, 77, 1, 90), v4(10, 77, 1, 90)],
        bench_txt(),
    );
    assert_eq!(advert.endpoints.len(), 1);
}

#[test]
fn an_endpoint_prints_as_a_url_an_operator_can_paste() {
    let endpoint = Endpoint {
        address: v4(10, 77, 1, 90),
        port: 8090,
        protocol: Protocol::Http,
    };
    assert_eq!(endpoint.to_string(), "http://10.77.1.90:8090");

    let ipv6 = Endpoint {
        address: IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1)),
        port: 8090,
        protocol: Protocol::Https,
    };
    assert_eq!(ipv6.to_string(), "https://[fd00::1]:8090");
}

// --- malformed and hostile --------------------------------------------------

#[test]
fn an_advertisement_with_no_address_is_ignored() {
    assert!(Advertisement::try_from_parts("converter", None, &[], 8090, &bench_txt()).is_none());
}

#[test]
fn an_advertisement_with_no_port_is_ignored() {
    assert!(
        Advertisement::try_from_parts("converter", None, &[v4(10, 77, 1, 90)], 0, &bench_txt())
            .is_none()
    );
}

#[test]
fn an_advertisement_with_no_instance_name_is_ignored() {
    assert!(
        Advertisement::try_from_parts("", None, &[v4(10, 77, 1, 90)], 8090, &bench_txt()).is_none()
    );
}

#[test]
fn absurd_txt_values_do_not_stop_discovery() {
    let hostile = [
        ("api_ver", "v".repeat(100_000)),
        ("api_ver", "v1.99999999999999999999999".to_owned()),
        ("api_proto", "\u{0}\u{1}\u{2}".to_owned()),
        ("api_auth", "maybe".to_owned()),
        ("ver_snd", "-1".to_owned()),
        ("ver_snd", "99999999999999999999999999".to_owned()),
        ("ver_snd", "seventeen".to_owned()),
    ];

    for (key, value) in hostile {
        let mut txt = bench_txt();
        txt.insert(key.to_owned(), value);
        let advert = advertisement("converter", &[v4(10, 77, 1, 90)], txt);
        // The advertisement survives; the unusable field falls back.
        assert_eq!(advert.instance, "converter");
        assert!(!advert.endpoints.is_empty());
    }
}

#[tokio::test]
async fn one_bad_responder_does_not_stop_the_others() {
    let discovery = FabricatedDiscovery::new();
    let mut events = discovery.subscribe().events;

    discovery.advertise_raw("broken", None, &[], 0, &bench_txt());
    discovery.advertise(advertisement(
        "converter",
        &[v4(10, 77, 1, 90)],
        bench_txt(),
    ));

    match events.recv().await.expect("the good one still arrives") {
        DiscoveryEvent::Appeared(node) => assert_eq!(node.instance, "converter"),
        other => panic!("expected the healthy Node, got {other:?}"),
    }
}

#[test]
fn a_counter_set_is_comparable_against_an_empty_one() {
    let empty = VersionCounters::default();
    assert!(empty.changed_since(&empty).is_empty());
    assert!(empty.is_empty());
}
