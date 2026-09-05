//! The documents this controller puts on the wire.
//!
//! The engine tests prove which resource is written to and when. These prove
//! *what* is written, against a server that records it: IS-05 gives four
//! meanings to a field that is set, `null`, `auto` or missing, and the
//! difference between two of them is a Receiver that keeps claiming a
//! connection it no longer has.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::time::Duration;

use jackfield_engine::{Connector, NmosConnector, StreamSource};
use nmos::{ConnectionApiClient, ResourceId, StreamAddress};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SENDER: &str = "3b8be755-08ff-452b-b217-c9151eb21193";
const RECEIVER: &str = "1eb53d65-ac83-441c-86f6-9b27df30ef0c";

fn id(text: &str) -> ResourceId {
    text.parse().expect("a well-formed identifier")
}

fn connector() -> NmosConnector {
    NmosConnector::new(
        ConnectionApiClient::builder()
            .request_timeout(Duration::from_millis(500))
            .connect_timeout(Duration::from_millis(500))
            .build()
            .expect("a client builds"),
    )
}

/// What a Node answers to an accepted patch.
fn accepted(master_enable: bool, peer: &str, peer_id: Value) -> Value {
    json!({
        peer: peer_id,
        "master_enable": master_enable,
        "activation": {
            "mode": "activate_immediate",
            "requested_time": null,
            "activation_time": "1441700172:0"
        },
        "transport_params": [{}],
        "transport_file": {"data": null, "type": null}
    })
}

async fn staged_node(kind: &str, id: &str, response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!(
            "/x-nmos/connection/v1.1/single/{kind}/{id}/staged"
        )))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

async fn body(server: &MockServer) -> Value {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .last()
        .and_then(|request| serde_json::from_slice(&request.body).ok())
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn putting_a_sender_on_air_asks_for_an_immediate_activation() {
    let node = staged_node(
        "senders",
        SENDER,
        ResponseTemplate::new(200).set_body_json(accepted(true, "receiver_id", Value::Null)),
    )
    .await;

    connector()
        .set_transmitting(node.uri(), id(SENDER), true)
        .await
        .expect("the node accepts");

    assert_eq!(
        body(&node).await,
        json!({
            "master_enable": true,
            "activation": {"mode": "activate_immediate"}
        }),
        "nothing else is touched: an unmentioned transport parameter is one the \
         operator did not ask to change"
    );
}

#[tokio::test]
async fn taking_a_sender_off_air_is_the_same_request_the_other_way() {
    let node = staged_node(
        "senders",
        SENDER,
        ResponseTemplate::new(200).set_body_json(accepted(false, "receiver_id", Value::Null)),
    )
    .await;

    connector()
        .set_transmitting(node.uri(), id(SENDER), false)
        .await
        .expect("the node accepts");

    assert_eq!(body(&node).await["master_enable"], json!(false));
}

#[tokio::test]
async fn a_device_that_does_not_do_what_it_was_asked_is_reported_as_such() {
    // IS-05 has no locking, so another controller can stage against the same
    // resource. The reply is the only evidence of what took effect.
    let node = staged_node(
        "senders",
        SENDER,
        ResponseTemplate::new(200).set_body_json(accepted(false, "receiver_id", Value::Null)),
    )
    .await;

    let error = connector()
        .set_transmitting(node.uri(), id(SENDER), true)
        .await
        .expect_err("the node did not do it");

    assert!(error.contains("master_enable=false"), "{error}");
}

#[tokio::test]
async fn connecting_a_receiver_names_the_sender_and_carries_its_transport_file() {
    let node = staged_node(
        "receivers",
        RECEIVER,
        ResponseTemplate::new(200).set_body_json(accepted(true, "sender_id", json!(SENDER))),
    )
    .await;

    connector()
        .subscribe(
            node.uri(),
            id(RECEIVER),
            id(SENDER),
            StreamSource::TransportFile("v=0\r\n".to_owned()),
        )
        .await
        .expect("the node accepts");

    assert_eq!(
        body(&node).await,
        json!({
            "sender_id": SENDER,
            "master_enable": true,
            "activation": {"mode": "activate_immediate"},
            "transport_file": {"data": "v=0\r\n", "type": "application/sdp"}
        })
    );
}

#[tokio::test]
async fn disconnecting_a_receiver_sets_the_sender_to_null_rather_than_omitting_it() {
    // Absent means "leave it": the Node would keep the old identifier and keep
    // claiming a connection it no longer has. IS-05 requires the null.
    let node = staged_node(
        "receivers",
        RECEIVER,
        ResponseTemplate::new(200).set_body_json(accepted(false, "sender_id", Value::Null)),
    )
    .await;

    connector()
        .unsubscribe(node.uri(), id(RECEIVER))
        .await
        .expect("the node accepts");

    let sent = body(&node).await;
    assert_eq!(sent["sender_id"], Value::Null);
    assert!(
        sent.as_object()
            .expect("an object")
            .contains_key("sender_id"),
        "the key is present and null, not missing"
    );
    assert_eq!(sent["master_enable"], json!(false));
}

#[tokio::test]
async fn a_refusal_reaches_the_operator_in_the_devices_own_words() {
    let node = staged_node(
        "senders",
        SENDER,
        ResponseTemplate::new(400).set_body_json(json!({
            "code": 400,
            "error": "destination_ip is not a multicast address",
            "debug": null
        })),
    )
    .await;

    let error = connector()
        .set_transmitting(node.uri(), id(SENDER), true)
        .await
        .expect_err("the node refuses");

    assert!(
        error.contains("destination_ip is not a multicast address"),
        "{error}"
    );
}

#[tokio::test]
async fn a_sender_with_no_transport_file_yields_none_rather_than_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/x-nmos/connection/v1.1/single/senders/{SENDER}/transportfile"
        )))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    assert_eq!(
        connector()
            .fetch_transport_file(server.uri(), id(SENDER))
            .await
            .expect("404 is an answer"),
        None
    );
}

#[tokio::test]
async fn a_sender_without_an_sdp_is_taken_by_its_addresses_instead() {
    // IS-05's own answer for equipment that publishes no transport file: make
    // the connection from transport parameters. Two legs is ST 2022-7, and the
    // Sender's order is kept — element 0 is the primary leg.
    let node = staged_node(
        "receivers",
        RECEIVER,
        ResponseTemplate::new(200).set_body_json(accepted(true, "sender_id", json!(SENDER))),
    )
    .await;

    connector()
        .subscribe(
            node.uri(),
            id(RECEIVER),
            id(SENDER),
            StreamSource::Streams(vec![
                StreamAddress {
                    address: "239.10.10.10".to_owned(),
                    port: 5004,
                },
                StreamAddress {
                    address: "239.20.20.20".to_owned(),
                    port: 5006,
                },
            ]),
        )
        .await
        .expect("the node accepts");

    assert_eq!(
        body(&node).await["transport_params"],
        json!([
            {"multicast_ip": "239.10.10.10", "destination_port": 5004, "rtp_enabled": true},
            {"multicast_ip": "239.20.20.20", "destination_port": 5006, "rtp_enabled": true}
        ])
    );
    assert!(
        body(&node).await.get("transport_file").is_none(),
        "no file is invented for a Sender that has none"
    );
}

#[tokio::test]
async fn an_address_this_controller_cannot_read_is_refused_rather_than_sent() {
    // Equipment reports what it likes. A hostname where an address belongs
    // must not become a patch the Node has to reject.
    let node = staged_node(
        "receivers",
        RECEIVER,
        ResponseTemplate::new(200).set_body_json(accepted(true, "sender_id", json!(SENDER))),
    )
    .await;

    let error = connector()
        .subscribe(
            node.uri(),
            id(RECEIVER),
            id(SENDER),
            StreamSource::Streams(vec![StreamAddress {
                address: "not-an-address".to_owned(),
                port: 5004,
            }]),
        )
        .await
        .expect_err("nothing usable to send");

    assert!(error.contains("not-an-address"), "{error}");
    assert_eq!(
        node.received_requests().await.unwrap_or_default().len(),
        0,
        "nothing was sent"
    );
}
