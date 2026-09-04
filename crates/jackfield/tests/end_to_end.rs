//! The three crates, together, against fixture Nodes.
//!
//! This is the only place discovery, the inventory and the screen all meet, so
//! it is where the shape of the whole thing is checked: two Nodes, one healthy
//! and one refusing connections, driven from an advertisement to a rendered
//! screen. The healthy one must be browsable and the failure must be confined
//! to its own row.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use jackfield::{Options, describe, spawn_engine};
use jackfield_engine::{
    Advertisement, Discovery, FabricatedDiscovery, NodeState, Snapshot, Subscription,
};
use serde_json::json;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A Node API answering the six collections with a converter-shaped tree.
async fn fixture_node() -> MockServer {
    let server = MockServer::start().await;
    let node_id = "3b8be755-08ff-452b-b217-c9151eb21193";
    let device_id = "9126cc2f-4c26-4c9b-a6cd-93c4381c9be5";
    let sender_id = "d7aa5a30-681d-4e72-92fb-f0ba0f6f4c3e";
    let flow_id = "5fbec3b1-1b0f-417d-9059-8b94a47197ed";
    let source_id = "02c46999-d532-4c52-905f-2e368a2af6cb";
    let receiver_id = "1eb53d65-ac83-441c-86f6-9b27df30ef0c";

    let core = |id: &str, label: &str| {
        json!({
            "id": id,
            "version": "1441700172:0",
            "label": label,
            "description": "",
            "tags": {}
        })
    };

    let mut node = core(node_id, "Bench Converter");
    node["href"] = json!(format!("{}/", server.uri()));
    node["hostname"] = json!("converter.local.");
    node["api"] = json!({"versions": ["v1.3"], "endpoints": []});
    node["caps"] = json!({});
    node["services"] = json!([]);
    node["clocks"] = json!([]);
    node["interfaces"] = json!([]);

    let mut device = core(device_id, "SDI 1");
    device["type"] = json!("urn:x-nmos:device:generic");
    device["node_id"] = json!(node_id);
    device["senders"] = json!([]);
    device["receivers"] = json!([]);
    device["controls"] = json!([{
        "href": format!("{}/x-nmos/connection/v1.1/", server.uri()),
        "type": "urn:x-nmos:control:sr-ctrl/v1.1"
    }]);

    let mut sender = core(sender_id, "SDI 1");
    sender["flow_id"] = json!(flow_id);
    sender["transport"] = json!("urn:x-nmos:transport:rtp.mcast");
    sender["device_id"] = json!(device_id);
    sender["manifest_href"] = json!(null);
    sender["interface_bindings"] = json!([]);
    sender["subscription"] = json!({"receiver_id": null, "active": true});

    let mut receiver = core(receiver_id, "SDI 1/in");
    receiver["device_id"] = json!(device_id);
    receiver["transport"] = json!("urn:x-nmos:transport:rtp.mcast");
    receiver["interface_bindings"] = json!([]);
    receiver["subscription"] = json!({"sender_id": null, "active": false});
    receiver["format"] = json!("urn:x-nmos:format:video");
    receiver["caps"] = json!({"media_types": ["video/raw"]});

    let mut flow = core(flow_id, "SDI 1");
    flow["source_id"] = json!(source_id);
    flow["device_id"] = json!(device_id);
    flow["parents"] = json!([]);
    flow["format"] = json!("urn:x-nmos:format:video");
    flow["media_type"] = json!("video/raw");
    flow["frame_width"] = json!(1920);
    flow["frame_height"] = json!(1080);
    flow["colorspace"] = json!("BT709");
    flow["components"] = json!([{"name": "Y", "width": 1920, "height": 1080, "bit_depth": 10}]);

    let mut source = core(source_id, "SDI 1");
    source["caps"] = json!({});
    source["device_id"] = json!(device_id);
    source["parents"] = json!([]);
    source["clock_name"] = json!(null);
    source["format"] = json!("urn:x-nmos:format:video");

    for (path, body) in [
        ("self", node),
        ("devices", json!([device])),
        ("senders", json!([sender])),
        ("receivers", json!([receiver])),
        ("flows", json!([flow])),
        ("sources", json!([source])),
    ] {
        Mock::given(method("GET"))
            .and(path_regex(format!("^/x-nmos/node/v1\\.[0-9]+/{path}$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }

    // The Connection API answers nothing, so transport stays unavailable and
    // connection state — which came from the resource tree — stays valid.
    server
}

fn advertisement_for(uri: &str) -> Advertisement {
    let address = uri
        .trim_start_matches("http://")
        .split(':')
        .next()
        .and_then(|host| host.parse::<Ipv4Addr>().ok())
        .unwrap_or(Ipv4Addr::LOCALHOST);
    let port = uri
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(80);

    let mut txt: BTreeMap<String, String> = BTreeMap::new();
    txt.insert("api_ver".to_owned(), "v1.3".to_owned());
    txt.insert("api_proto".to_owned(), "http".to_owned());

    Advertisement::try_from_parts(
        &format!("fixture-{port}"),
        Some("converter.local.".to_owned()),
        &[IpAddr::V4(address)],
        port,
        &txt,
    )
    .expect("usable")
}

struct Shared(Arc<FabricatedDiscovery>);

impl Discovery for Shared {
    fn subscribe(&self) -> Subscription {
        self.0.subscribe()
    }
}

async fn until(
    handle: &jackfield_engine::EngineHandle,
    ready: impl Fn(&Snapshot) -> bool,
) -> Option<Arc<Snapshot>> {
    let mut snapshots = handle.snapshots();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            {
                let snapshot = handle.snapshot();
                if ready(&snapshot) {
                    return snapshot;
                }
            }
            if snapshots.changed().await.is_err() {
                return handle.snapshot();
            }
        }
    })
    .await
    .ok()
}

#[tokio::test]
async fn two_nodes_one_healthy_and_one_refusing_are_both_shown() {
    let healthy = fixture_node().await;

    let discovery = Arc::new(FabricatedDiscovery::new());
    let options = Options {
        request_timeout: 2,
        connect_timeout: 1,
        ..Options::default()
    };
    let handle = spawn_engine(Shared(Arc::clone(&discovery)), &options).expect("the engine starts");

    discovery.advertise(advertisement_for(&healthy.uri()));
    // Port 1 on the loopback interface: nothing listens there.
    discovery.advertise(advertisement_for("http://127.0.0.1:1"));

    let snapshot = until(&handle, |s| {
        s.nodes.len() == 2
            && s.nodes
                .iter()
                .all(|n| !matches!(n.state, NodeState::Loading))
    })
    .await
    .expect("both Nodes settle");

    let browsable = snapshot
        .nodes
        .iter()
        .find(|n| n.state.is_ready())
        .expect("the healthy Node is browsable");
    assert_eq!(browsable.display_name(), "Bench Converter");
    assert_eq!(browsable.devices().len(), 1);
    assert_eq!(browsable.devices()[0].senders.len(), 1);
    assert_eq!(browsable.devices()[0].receivers.len(), 1);

    let failed = snapshot
        .nodes
        .iter()
        .find(|n| n.state.failure().is_some())
        .expect("the other failed");
    assert!(
        failed
            .state
            .failure()
            .is_some_and(|reason| !reason.is_empty()),
        "a failure without a reason tells an operator nothing"
    );
}

#[tokio::test]
async fn the_rendered_screen_shows_the_healthy_node_and_confines_the_failure() {
    let healthy = fixture_node().await;

    let discovery = Arc::new(FabricatedDiscovery::new());
    let options = Options {
        request_timeout: 2,
        connect_timeout: 1,
        ..Options::default()
    };
    let handle = spawn_engine(Shared(Arc::clone(&discovery)), &options).expect("the engine starts");

    discovery.advertise(advertisement_for(&healthy.uri()));
    discovery.advertise(advertisement_for("http://127.0.0.1:1"));

    let snapshot = until(&handle, |s| {
        s.nodes.len() == 2
            && s.nodes
                .iter()
                .all(|n| !matches!(n.state, NodeState::Loading))
    })
    .await
    .expect("both Nodes settle");

    let mut app = jackfield_tui::App::new();
    app.apply(snapshot);
    // Select the healthy Node, whichever row it landed on.
    while app.selected().is_some_and(|node| !node.state.is_ready()) {
        app.select_next();
    }
    app.enter();

    let backend = ratatui::backend::TestBackend::new(120, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("a test terminal");
    terminal
        .draw(|frame| jackfield_tui::draw(frame, &mut app))
        .expect("the screen draws");

    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| {
                    buffer
                        .cell((x, y))
                        .map_or(" ", ratatui::buffer::Cell::symbol)
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Device SDI 1"), "{text}");
    assert!(text.contains("video/raw"), "{text}");
    assert!(
        text.contains("transmitting"),
        "the Sender's state came from IS-04:\n{text}"
    );
    assert!(
        !text.contains("connected"),
        "a Sender must never be described as connected:\n{text}"
    );
}

#[tokio::test]
async fn the_headless_report_describes_what_was_found() {
    let healthy = fixture_node().await;
    let discovery = Arc::new(FabricatedDiscovery::new());
    let options = Options {
        request_timeout: 2,
        connect_timeout: 1,
        ..Options::default()
    };
    let handle = spawn_engine(Shared(Arc::clone(&discovery)), &options).expect("the engine starts");

    discovery.advertise(advertisement_for(&healthy.uri()));
    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the Node is read");

    let report = describe(&snapshot);
    assert!(report.contains("Bench Converter"), "{report}");
    assert!(report.contains("Device SDI 1"), "{report}");
    assert!(
        report.contains("Sender SDI 1 [video/raw] transmitting"),
        "{report}"
    );
    assert!(
        report.contains("Receiver SDI 1/in unsubscribed"),
        "{report}"
    );
}

#[tokio::test]
async fn an_empty_network_reports_that_it_is_empty() {
    let snapshot = Snapshot::default();
    assert_eq!(describe(&snapshot), "No NMOS Nodes found.\n");
}

#[tokio::test]
async fn the_interface_is_served_while_a_node_fetch_is_stalled() {
    // The property the event loop exists for: it waits on a keystroke or a
    // snapshot, never on the network. A Node that accepts the connection and
    // then says nothing must not be able to freeze anything.
    let stalled = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/x-nmos/node/v1\.[0-9]+/self$"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(300)))
        .mount(&stalled)
        .await;

    let healthy = fixture_node().await;

    let discovery = Arc::new(FabricatedDiscovery::new());
    let options = Options {
        request_timeout: 120,
        connect_timeout: 5,
        ..Options::default()
    };
    let handle = spawn_engine(Shared(Arc::clone(&discovery)), &options).expect("the engine starts");

    discovery.advertise(advertisement_for(&stalled.uri()));
    discovery.advertise(advertisement_for(&healthy.uri()));

    // A snapshot arrives, and the healthy Node is in it, while the other is
    // still hanging.
    let snapshot = until(&handle, |s| s.nodes.iter().any(|n| n.state.is_ready()))
        .await
        .expect("the healthy Node arrives regardless");
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|n| matches!(n.state, NodeState::Loading))
    );

    // And a command is still answered promptly.
    let sent = tokio::time::timeout(
        Duration::from_millis(500),
        handle.send(jackfield_engine::Command::RefreshAll),
    )
    .await;
    assert!(
        sent.is_ok_and(|r| r.is_ok()),
        "the engine ignored a command while a fetch hung"
    );
}

#[test]
fn nothing_in_the_run_loop_writes_to_the_screen_behind_the_interfaces_back() {
    // The interface owns the terminal. A stray `println!` during a run scribbles
    // over the alternate screen, and the operator sees a corrupted display with
    // no clue where it came from.
    let run = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/run.rs"),
    )
    .expect("run.rs is readable");

    let code: String = run
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for forbidden in ["println!", "print!", "eprintln!", "dbg!"] {
        assert!(!code.contains(forbidden), "`{forbidden}` in the run loop");
    }
}

#[test]
fn logs_are_written_where_they_were_asked_for() {
    // Not to the screen: `--log-file` is the only way to read them during a run
    // that has taken the terminal.
    let dir = std::env::temp_dir().join(format!("jackfield-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let path = dir.join("jackfield.log");

    {
        let file = std::fs::File::create(&path).expect("the log file is creatable");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(node = "converter", "cannot reach the node");
        });
    }

    let written = std::fs::read_to_string(&path).expect("the log file is readable");
    assert!(written.contains("cannot reach the node"), "{written}");
    assert!(
        written.contains("converter"),
        "structured fields survive: {written}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
