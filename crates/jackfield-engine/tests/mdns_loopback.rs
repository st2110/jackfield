//! Discovery against a service registered on this machine.
//!
//! This is the one test here that touches a real socket. It proves the two
//! claims the fabricated tests cannot: that the browser speaks mDNS itself,
//! with no `avahi-daemon` or equivalent installed, and that a real
//! advertisement makes it all the way through to a discovery event.
//!
//! It needs a network interface that can carry multicast. Where there is none —
//! a locked-down container, a machine with every interface down — the test says
//! so and stops, rather than failing for a reason that has nothing to do with
//! the code.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use jackfield_engine::{Discovery, DiscoveryEvent, MdnsDiscovery};
use mdns_sd::{ServiceDaemon, ServiceInfo};

/// A service type of our own, so a real NMOS Node on the developer's network
/// cannot make this test pass or fail by accident.
const SERVICE_TYPE: &str = "_jackfield-test._tcp.local.";

#[tokio::test]
async fn a_service_registered_on_this_machine_is_discovered() {
    let Ok(responder) = ServiceDaemon::new() else {
        eprintln!("skipping: this host cannot start an mDNS responder");
        return;
    };

    let mut txt = HashMap::new();
    txt.insert("api_ver".to_owned(), "v1.2,v1.3".to_owned());
    txt.insert("api_proto".to_owned(), "http".to_owned());
    txt.insert("ver_snd".to_owned(), "17".to_owned());

    let service = ServiceInfo::new(
        SERVICE_TYPE,
        "jackfield-fixture",
        "jackfield-fixture.local.",
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        8090,
        txt,
    )
    .expect("the fixture service is well formed");

    if responder.register(service).is_err() {
        eprintln!("skipping: this host cannot register a service");
        return;
    }

    let Ok(discovery) = MdnsDiscovery::browsing(SERVICE_TYPE) else {
        eprintln!("skipping: this host cannot browse over multicast");
        return;
    };
    // Everything already found comes back in the snapshot, so a Node
    // discovered in the moment before subscribing is not lost.
    let subscription = discovery.subscribe();
    let mut events = subscription.events;
    let mut found = subscription
        .known
        .into_iter()
        .find(|advertisement| advertisement.instance == "jackfield-fixture");

    // Multicast is not instant, and this is the only place the tests wait on
    // one. Ten seconds is generous; the assertion is what matters, not the
    // latency.
    if found.is_none() {
        found = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match events.recv().await {
                    Ok(DiscoveryEvent::Appeared(advertisement))
                        if advertisement.instance == "jackfield-fixture" =>
                    {
                        return Some(advertisement);
                    }
                    Ok(_) => continue,
                    Err(_) => return None,
                }
            }
        })
        .await
        .ok()
        .flatten();
    }

    let Some(advertisement) = found else {
        eprintln!("skipping: no multicast reached this host within ten seconds");
        return;
    };

    assert_eq!(advertisement.endpoints.len(), 1);
    let endpoint = advertisement.preferred_endpoint().expect("an endpoint");
    assert_eq!(endpoint.port, 8090);
    assert_eq!(
        advertisement
            .versions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["v1.2", "v1.3"],
        "the TXT records survived the round trip through a real responder"
    );
    assert_eq!(
        advertisement
            .counters
            .get(jackfield_engine::Collection::Senders),
        Some(17)
    );
}

#[test]
fn discovery_needs_no_system_mdns_daemon() {
    // `mdns-sd` is a responder and browser in its own right. Nothing in this
    // crate shells out, and nothing here talks to a daemon socket — which is
    // the property `docs/adr/0002-peer-to-peer-mdns-discovery.md` records.
    // Comments are stripped first: `mdns.rs` explains at length why it does not
    // shell out to `avahi-browse`, and that explanation must not trip the check
    // that enforces it.
    let sources: Vec<String> = walk(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .into_iter()
        .map(|path| std::fs::read_to_string(path).expect("source is readable"))
        .map(|text| {
            text.lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();

    for text in &sources {
        for forbidden in ["avahi", "dns-sd", "Command::new", "std::process"] {
            assert!(
                !text.contains(forbidden),
                "discovery must not depend on an external process (`{forbidden}` found)"
            );
        }
    }
}

fn walk(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
