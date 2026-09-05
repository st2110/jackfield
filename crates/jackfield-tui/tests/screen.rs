//! What the screen says.
//!
//! Every assertion here reads the rendered text, never a colour: the screen has
//! to survive a monochrome terminal, a screenshot pasted into a ticket, and a
//! session over a serial console.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use jackfield_engine::{NodeState, Pairing, Transport};
use jackfield_tui::App;
use nmos::{Reception, Transmission};
use support::{
    device_view, known, ready, ready_with_orphan, receiver_view, render, resource_ref, screen,
    sender_view, snapshot, stream,
};

const WIDE: u16 = 120;
const TALL: u16 = 40;

/// An app showing one Node in the given state.
fn showing(state: NodeState) -> App {
    let mut app = App::new();
    app.apply(snapshot(vec![known(
        1,
        "converter",
        [10, 77, 1, 90],
        state,
    )]));
    app.enter();
    app
}

// --- the node list ----------------------------------------------------------

#[test]
fn an_empty_network_says_so_rather_than_showing_a_blank_frame() {
    let mut app = App::new();
    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("No NMOS Nodes found yet"), "{text}");
}

#[test]
fn a_node_appears_with_its_label_and_address() {
    let mut app = showing(ready("Converter", Vec::new()));
    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("Converter"), "{text}");
    // The list row truncates the address before it truncates the state; the
    // detail pane always carries it in full.
    assert!(text.contains("Address http://10.77.1.90:8090"), "{text}");
}

#[test]
fn a_node_that_has_not_been_fetched_yet_is_shown_as_loading() {
    let mut app = showing(NodeState::Loading);
    let text = screen(&mut app, WIDE, TALL);
    assert!(
        text.contains("— loading —"),
        "the state survives a narrow pane:\n{text}"
    );
    assert!(
        text.contains("converter.local"),
        "a loading Node is listed, not hidden"
    );
}

#[test]
fn a_failed_node_shows_its_reason_and_the_screen_survives() {
    let mut app = showing(NodeState::Failed {
        reason: "connection refused".to_owned(),
    });
    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("connection refused"), "{text}");
}

#[test]
fn a_failure_is_confined_to_its_own_row() {
    let mut app = App::new();
    app.apply(snapshot(vec![
        known(
            1,
            "broken",
            [10, 77, 1, 91],
            NodeState::Failed {
                reason: "refused".to_owned(),
            },
        ),
        known(2, "healthy", [10, 77, 1, 90], ready("Healthy", Vec::new())),
    ]));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("refused"), "{text}");
    assert!(text.contains("Healthy"), "the healthy Node is still listed");
}

#[test]
fn the_selection_stays_on_the_same_node_when_another_appears_above_it() {
    let mut app = App::new();
    app.apply(snapshot(vec![known(
        5,
        "second",
        [10, 77, 1, 91],
        ready("Second", Vec::new()),
    )]));
    let selected = app.selected().expect("something is selected").key.clone();

    // A Node with a lower key sorts above it.
    app.apply(snapshot(vec![
        known(1, "first", [10, 77, 1, 90], ready("First", Vec::new())),
        known(5, "second", [10, 77, 1, 91], ready("Second", Vec::new())),
    ]));

    assert_eq!(app.selected().expect("still selected").key, selected);
    assert_eq!(
        app.selected_index(),
        Some(1),
        "the same Node, a different row"
    );
}

#[test]
fn the_selection_lands_somewhere_sensible_when_the_selected_node_departs() {
    let mut app = App::new();
    app.apply(snapshot(vec![
        known(1, "first", [10, 77, 1, 90], ready("First", Vec::new())),
        known(5, "second", [10, 77, 1, 91], ready("Second", Vec::new())),
    ]));
    app.select_next();
    let second = app.selected().expect("selected").key.clone();

    app.apply(snapshot(vec![known(
        1,
        "first",
        [10, 77, 1, 90],
        ready("First", Vec::new()),
    )]));

    let now = app.selected().expect("something is still selected");
    assert_ne!(now.key, second);
    assert_eq!(now.display_name(), "First");
}

#[test]
fn an_empty_snapshot_leaves_the_selection_pointing_at_nothing_without_panicking() {
    let mut app = showing(ready("Converter", Vec::new()));
    app.apply(snapshot(Vec::new()));
    assert!(app.selected().is_none());
    let _ = screen(&mut app, WIDE, TALL);
}

// --- devices, senders and receivers -----------------------------------------

#[test]
fn devices_group_their_senders_and_receivers() {
    let mut app = showing(ready(
        "Converter",
        vec![
            device_view(
                "SDI 1",
                vec![sender_view(
                    100,
                    "SDI 1",
                    "video/raw",
                    Transmission::Transmitting,
                )],
                vec![receiver_view(
                    400,
                    "SDI 1/in",
                    "video/raw",
                    Reception::Unsubscribed,
                )],
            ),
            device_view(
                "SDI 2",
                vec![sender_view(110, "SDI 2", "video/raw", Transmission::Idle)],
                Vec::new(),
            ),
        ],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("Device SDI 1"), "{text}");
    assert!(text.contains("Device SDI 2"), "{text}");
    assert!(text.contains("Sender SDI 1"), "{text}");
    assert!(text.contains("Receiver SDI 1/in"), "{text}");
}

#[test]
fn grouping_holds_for_a_single_device_node_too() {
    let mut app = showing(ready(
        "Converter",
        vec![device_view(
            "SDI 1",
            vec![sender_view(100, "SDI 1", "video/raw", Transmission::Idle)],
            vec![receiver_view(
                400,
                "SDI 1/in",
                "video/raw",
                Reception::Unsubscribed,
            )],
        )],
    ));
    let text = screen(&mut app, WIDE, TALL);
    assert!(
        text.contains("Device SDI 1"),
        "Senders are never flattened out of their Device"
    );
}

#[test]
fn a_device_with_nothing_on_it_is_still_shown() {
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 3", Vec::new(), Vec::new())],
    ));
    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("Device SDI 3"), "{text}");
    assert!(text.contains("no senders or receivers"), "{text}");
}

#[test]
fn three_senders_sharing_a_label_are_told_apart_by_media_type() {
    // The bench converter's shape exactly.
    let mut app = showing(ready(
        "Converter",
        vec![device_view(
            "SDI 1",
            vec![
                sender_view(100, "SDI 1", "video/raw", Transmission::Transmitting),
                sender_view(101, "SDI 1", "audio/L24", Transmission::Transmitting),
                sender_view(102, "SDI 1", "video/smpte291", Transmission::Transmitting),
            ],
            Vec::new(),
        )],
    ));

    let text = screen(&mut app, WIDE, TALL);
    for media in ["video/raw", "audio/L24", "video/smpte291"] {
        assert!(text.contains(media), "{media} is missing from:\n{text}");
    }
}

#[test]
fn a_receiver_shows_the_media_types_it_accepts() {
    let mut app = showing(ready(
        "Converter",
        vec![device_view(
            "SDI 1",
            Vec::new(),
            vec![receiver_view(
                400,
                "SDI 1/audio",
                "audio/L24",
                Reception::Unsubscribed,
            )],
        )],
    ));
    assert!(screen(&mut app, WIDE, TALL).contains("audio/L24"));
}

#[test]
fn an_unresolved_flow_renders_as_unknown_rather_than_blank() {
    let mut sender = sender_view(100, "SDI 1", "video/raw", Transmission::Transmitting);
    sender.media = jackfield_engine::Media::Unresolved {
        flow_id: support::id(999),
    };
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", vec![sender], Vec::new())],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(
        text.contains("unknown media"),
        "a blank would read as `no media`:\n{text}"
    );
}

#[test]
fn an_unattached_resource_is_shown_not_hidden() {
    let mut app = showing(ready_with_orphan("Converter", Vec::new()));
    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("Unattached"), "{text}");
    assert!(text.contains("stray"), "{text}");
}

// --- connection state -------------------------------------------------------

#[test]
fn a_sender_is_transmitting_or_idle_and_never_connected() {
    let mut app = showing(ready(
        "Converter",
        vec![device_view(
            "SDI 1",
            vec![
                sender_view(100, "live", "video/raw", Transmission::Transmitting),
                sender_view(101, "quiet", "video/raw", Transmission::Idle),
            ],
            Vec::new(),
        )],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("transmitting"), "{text}");
    assert!(text.contains("idle"), "{text}");
    assert!(
        !text.contains("connected"),
        "a Sender must never be described as connected:\n{text}"
    );
}

#[test]
fn a_receiver_is_subscribed_or_unsubscribed() {
    let mut app = showing(ready(
        "Converter",
        vec![device_view(
            "SDI 1",
            Vec::new(),
            vec![
                receiver_view(400, "taking", "video/raw", Reception::Subscribed),
                receiver_view(401, "quiet", "video/raw", Reception::Unsubscribed),
            ],
        )],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("subscribed"), "{text}");
    assert!(text.contains("unsubscribed"), "{text}");
}

#[test]
fn a_transmitting_sender_whose_transport_is_unread_shows_its_destination_as_pending() {
    let mut app = showing(ready(
        "Converter",
        vec![device_view(
            "SDI 1",
            vec![sender_view(
                100,
                "live",
                "video/raw",
                Transmission::Transmitting,
            )],
            Vec::new(),
        )],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(
        text.contains("destination pending"),
        "a blank would read as nowhere:\n{text}"
    );
    assert!(text.contains("receivers not yet known"), "{text}");
}

#[test]
fn a_sender_with_a_destination_shows_it() {
    let mut sender = sender_view(100, "live", "video/raw", Transmission::Transmitting);
    sender.transport = Transport::Known {
        streams: vec![stream(190, 5004)],
    };
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", vec![sender], Vec::new())],
    ));

    assert!(screen(&mut app, WIDE, TALL).contains("to 239.255.0.190:5004"));
}

#[test]
fn a_sender_transmitting_into_the_void_says_so_distinctly() {
    let mut sender = sender_view(100, "live", "video/raw", Transmission::Transmitting);
    sender.transport = Transport::Known {
        streams: vec![stream(190, 5004)],
    };
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", vec![sender], Vec::new())],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("taken by nobody"), "{text}");
    assert!(
        !text.contains("receivers not yet known"),
        "a Sender with no takers is not a Sender whose takers are unknown"
    );
}

#[test]
fn a_sender_shows_how_many_receivers_take_its_stream_and_expands_to_name_them() {
    let mut sender = sender_view(100, "live", "video/raw", Transmission::Transmitting);
    sender.transport = Transport::Known {
        streams: vec![stream(190, 5004)],
    };
    sender.taken_by = vec![
        resource_ref("Sink A", "SDI 1", 400, "one"),
        resource_ref("Sink A", "SDI 1", 401, "two"),
        resource_ref("Sink B", "SDI 1", 402, "three"),
    ];
    let id = sender.sender.core.id.clone();

    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", vec![sender], Vec::new())],
    ));

    let collapsed = screen(&mut app, WIDE, TALL);
    assert!(collapsed.contains("taken by 3 receivers"), "{collapsed}");
    assert!(
        !collapsed.contains("Sink B"),
        "the list is closed until it is opened"
    );

    app.toggle_expanded(&id);
    let expanded = screen(&mut app, WIDE, TALL);
    assert!(expanded.contains("Sink A / SDI 1 / one"), "{expanded}");
    assert!(expanded.contains("Sink B / SDI 1 / three"), "{expanded}");
}

// --- unresolved pairings ----------------------------------------------------

#[test]
fn a_receiver_subscribed_to_an_unknown_sender_is_not_shown_as_unsubscribed() {
    let mut receiver = receiver_view(400, "taking", "video/raw", Reception::Subscribed);
    receiver.pairing = Pairing::UnknownSender {
        sender_id: support::id(999),
    };
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", Vec::new(), vec![receiver])],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("subscribed"), "{text}");
    assert!(text.contains("from an unknown sender"), "{text}");
    assert!(
        !text.contains("unsubscribed"),
        "that would be a lie:\n{text}"
    );
}

#[test]
fn an_ambiguous_pairing_names_its_candidates() {
    let mut receiver = receiver_view(400, "taking", "video/raw", Reception::Subscribed);
    receiver.pairing = Pairing::Ambiguous {
        candidates: vec![
            resource_ref("Source", "SDI 1", 100, "first"),
            resource_ref("Source", "SDI 1", 101, "second"),
        ],
    };
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", Vec::new(), vec![receiver])],
    ));

    let text = screen(&mut app, WIDE, TALL);
    assert!(text.contains("ambiguous"), "{text}");
    assert!(text.contains("first"), "{text}");
    assert!(text.contains("second"), "{text}");
}

#[test]
fn a_resolved_pairing_names_the_sender_with_its_node_and_device() {
    let mut receiver = receiver_view(400, "taking", "video/raw", Reception::Subscribed);
    receiver.pairing = Pairing::Resolved(Box::new(resource_ref("Source", "SDI 1", 100, "live")));
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", Vec::new(), vec![receiver])],
    ));

    assert!(screen(&mut app, WIDE, TALL).contains("from Source / SDI 1 / live"));
}

#[test]
fn a_pairing_that_is_merely_unread_says_so() {
    let mut receiver = receiver_view(400, "taking", "video/raw", Reception::Subscribed);
    receiver.pairing = Pairing::Pending;
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", Vec::new(), vec![receiver])],
    ));

    assert!(screen(&mut app, WIDE, TALL).contains("sender not yet known"));
}

// --- keys and layout --------------------------------------------------------

#[test]
fn the_available_keys_are_shown_on_screen() {
    let mut app = App::new();
    let text = screen(&mut app, WIDE, TALL);
    for hint in ["move", "open", "back", "refresh", "quit"] {
        assert!(
            text.contains(hint),
            "`{hint}` is missing from the key hints:\n{text}"
        );
    }
}

#[test]
fn going_back_keeps_the_same_node_selected() {
    let mut app = App::new();
    app.apply(snapshot(vec![
        known(1, "first", [10, 77, 1, 90], ready("First", Vec::new())),
        known(5, "second", [10, 77, 1, 91], ready("Second", Vec::new())),
    ]));
    app.select_next();
    let chosen = app.selected().expect("selected").key.clone();

    app.enter();
    app.back();

    assert_eq!(app.selected().expect("still selected").key, chosen);
}

#[test]
fn the_selection_does_not_run_off_either_end_of_the_list() {
    let mut app = App::new();
    app.apply(snapshot(vec![
        known(1, "first", [10, 77, 1, 90], ready("First", Vec::new())),
        known(5, "second", [10, 77, 1, 91], ready("Second", Vec::new())),
    ]));

    for _ in 0..10 {
        app.select_next();
    }
    assert_eq!(app.selected_index(), Some(1));

    for _ in 0..10 {
        app.select_previous();
    }
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn a_label_longer_than_the_pane_does_not_break_the_layout() {
    let long = "A".repeat(400);
    let mut app = showing(ready(&long, Vec::new()));
    let lines = render(&mut app, 40, 12);

    assert_eq!(lines.len(), 12);
    for line in &lines {
        assert!(
            line.chars().count() <= 40,
            "a line ran past the pane: {}",
            line.len()
        );
    }
}

#[test]
fn a_narrow_terminal_still_renders() {
    let mut app = showing(ready("Converter", Vec::new()));
    let lines = render(&mut app, 20, 6);
    assert_eq!(lines.len(), 6);
}

#[test]
fn a_resize_redraws_at_the_new_size() {
    let mut app = showing(ready("Converter", Vec::new()));
    assert_eq!(render(&mut app, 80, 24).len(), 24);
    assert_eq!(render(&mut app, 120, 40).len(), 40);
    assert_eq!(render(&mut app, 40, 10).len(), 10);
}

#[test]
fn more_nodes_than_fit_scroll_and_the_selected_one_stays_visible() {
    let nodes: Vec<_> = (0..40u16)
        .map(|n| known(n, "node", [10, 77, 1, 90], ready("Node", Vec::new())))
        .collect();
    let mut app = App::new();
    app.apply(snapshot(nodes));

    for _ in 0..39 {
        app.select_next();
    }
    let text = screen(&mut app, WIDE, 12);
    // The list pane has focus, so the selected row carries the highlight mark.
    assert!(
        text.contains('▸'),
        "the selected Node scrolled out of view:\n{text}"
    );
}

#[test]
fn nothing_in_the_interface_relies_on_colour() {
    // Everything above asserts on text pulled from the buffer with styles
    // discarded, which is the property this test names out loud.
    let mut sender = sender_view(100, "live", "video/raw", Transmission::Transmitting);
    sender.transport = Transport::Known {
        streams: vec![stream(190, 5004)],
    };
    let mut app = showing(ready(
        "Converter",
        vec![device_view("SDI 1", vec![sender], Vec::new())],
    ));

    let text = screen(&mut app, WIDE, TALL);
    for word in ["transmitting", "taken by nobody", "239.255.0.190:5004"] {
        assert!(
            text.contains(word),
            "`{word}` is not legible without colour:\n{text}"
        );
    }
}
