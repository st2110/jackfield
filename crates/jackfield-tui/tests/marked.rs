//! What the screen says about the Marked Sender.
//!
//! Marking and connecting happen on two different Nodes — that is the whole
//! reason marking exists — so between the two keystrokes the operator walks
//! away from the Sender they named. Unless the mark is stated where they are
//! now, nothing on screen says whose stream the next `t` will take.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use jackfield_tui::App;
use nmos::{Reception, Transmission};
use support::{
    bench_node, device_view, known, ready, receiver_view, render, screen, sender_view, snapshot,
};

const WIDE: u16 = 120;
const TALL: u16 = 40;

/// The bench shape: a converter that transmits, and a box that receives.
fn two_nodes() -> App {
    let converter = known(
        1,
        "core-ml-2110-bm",
        [10, 77, 1, 90],
        ready(
            "core-ml-2110-bm",
            vec![device_view(
                "SDI 1",
                vec![sender_view(
                    100,
                    "SDI 1",
                    "video/raw",
                    Transmission::Transmitting,
                )],
                Vec::new(),
            )],
        ),
    );
    let box_ = known(
        2,
        "mcaster",
        [10, 77, 1, 178],
        ready(
            "mcaster",
            vec![device_view(
                "s64276",
                Vec::new(),
                vec![receiver_view(
                    200,
                    "video",
                    "video/raw",
                    Reception::Unsubscribed,
                )],
            )],
        ),
    );

    let mut app = App::new();
    app.apply(snapshot(vec![converter, box_]));
    app
}

/// Mark the converter's only Sender, then walk to the receiving Node.
fn marked_then_walked() -> App {
    let mut app = two_nodes();
    app.enter();
    app.mark();
    app.back();
    app.select_next();
    app.enter();
    app
}

#[test]
fn nothing_is_said_about_a_mark_before_one_is_made() {
    let mut app = two_nodes();
    app.enter();

    let text = screen(&mut app, WIDE, TALL);

    assert!(!text.contains("Marked"), "{text}");
}

#[test]
fn the_marked_sender_is_named_on_the_top_line() {
    let mut app = two_nodes();
    app.enter();
    app.mark();

    let lines = render(&mut app, WIDE, TALL);

    let top = &lines[0];
    assert!(top.contains("Marked"), "{top:?}");
    for fact in ["core-ml-2110-bm", "SDI 1", "video/raw"] {
        assert!(
            top.contains(fact),
            "the top line does not say {fact}: {top:?}"
        );
    }
}

#[test]
fn the_mark_is_still_named_on_the_node_the_receiver_lives_on() {
    let mut app = marked_then_walked();

    let lines = render(&mut app, WIDE, TALL);

    let top = &lines[0];
    assert!(
        top.contains("core-ml-2110-bm") && top.contains("video/raw"),
        "the Sender being connected is not named where it is being connected: {top:?}"
    );
    let text = lines.join("\n");
    assert!(
        text.contains("Receiver video"),
        "the Receiver being pointed at is not on screen:\n{text}"
    );
}

#[test]
fn a_mark_whose_sender_has_gone_says_so_rather_than_disappearing() {
    let mut app = marked_then_walked();
    // The converter drops off the network with the mark still held.
    let box_ = known(
        2,
        "mcaster",
        [10, 77, 1, 178],
        ready(
            "mcaster",
            vec![device_view("s64276", Vec::new(), Vec::new())],
        ),
    );
    app.apply(snapshot(vec![box_]));

    let lines = render(&mut app, WIDE, TALL);

    let top = &lines[0];
    assert!(top.contains("Marked"), "{top:?}");
    assert!(
        top.contains("no longer"),
        "a mark that cannot be shown must say so, not go quiet: {top:?}"
    );
}

#[test]
fn the_top_line_is_spent_on_a_mark_only_while_one_is_held() {
    let mut app = two_nodes();
    app.enter();

    let before = render(&mut app, WIDE, TALL);
    assert!(
        before[0].contains("Nodes"),
        "the panes start at the top: {:?}",
        before[0]
    );

    app.mark();
    let after = render(&mut app, WIDE, TALL);
    assert!(after[0].contains("Marked"), "{:?}", after[0]);
    assert!(
        after[1].contains("Nodes"),
        "the panes moved down by more than the one row the mark costs: {:?}",
        after[1]
    );
}

#[test]
fn the_mark_line_does_not_run_past_a_narrow_terminal() {
    let mut app = marked_then_walked();

    for width in [40u16, 60, 80, 120] {
        for line in render(&mut app, width, TALL) {
            assert!(
                line.chars().count() <= usize::from(width),
                "a line ran past a {width}-column terminal: {line}"
            );
        }
    }
}

#[test]
fn a_terminal_too_short_for_a_mark_line_still_draws() {
    let mut app = marked_then_walked();

    for height in [1u16, 2, 3] {
        let lines = render(&mut app, 80, height);
        assert_eq!(lines.len(), usize::from(height));
    }
}

#[test]
fn a_whole_device_still_fits_on_an_eighty_by_twenty_four_terminal_while_marked() {
    // The row the mark costs comes out of the pane, so the densest screen an
    // operator actually works on has to be measured with a mark held.
    let mut app = App::new();
    app.apply(snapshot(vec![bench_node()]));
    app.enter();
    app.mark();

    let text = screen(&mut app, 80, 24);

    assert!(text.contains("Marked"), "{text}");
    assert!(text.contains("Device SDI 1"), "{text}");
    for media in ["video/raw", "audio/L24", "video/smpte291"] {
        assert!(
            text.matches(media).count() >= 2,
            "the Sender and the Receiver carrying {media} do not both fit:\n{text}"
        );
    }
}
