//! How much of a Node fits on a terminal.
//!
//! Regression: the detail pane spent three lines on every Sender, so a single
//! Device did not fit on an 80×24 terminal and an operator had to scroll to see
//! what they were already looking at. On the bench that read as "everything is
//! already open and I cannot get anywhere".

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use jackfield_tui::App;
use support::{bench_node, screen, snapshot};

/// The terminal an engineer is most likely to have.
const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;

#[test]
fn a_whole_device_fits_on_an_eighty_by_twenty_four_terminal() {
    let mut app = App::new();
    app.apply(snapshot(vec![bench_node()]));
    app.enter();

    let text = screen(&mut app, WIDTH, HEIGHT);

    assert!(text.contains("Device SDI 1"), "{text}");
    for media in ["video/raw", "audio/L24", "video/smpte291"] {
        assert!(
            text.matches(media).count() >= 2,
            "the Sender and the Receiver carrying {media} do not both fit:\n{text}"
        );
    }
    assert!(
        text.contains("Device SDI 2"),
        "the next Device is not even in sight:\n{text}"
    );
}

#[test]
fn a_sender_takes_two_lines() {
    // One overflows the pane and loses whichever fact is last; three does not
    // fit. State and destination are one thought, listeners are the other.
    let mut app = App::new();
    app.apply(snapshot(vec![bench_node()]));
    app.enter();

    let lines = support::render(&mut app, 120, 40);
    let first = lines
        .iter()
        .position(|line| line.contains("Sender SDI 1 [video/raw]"))
        .expect("the first Sender is on screen");

    assert!(lines[first].contains("transmitting"), "{:?}", lines[first]);
    assert!(
        lines[first + 1].contains("receivers"),
        "{:?}",
        lines[first + 1]
    );
    assert!(
        lines[first + 2].contains("Sender SDI 1 [audio/L24]"),
        "a Sender spilled past two lines: {:?}",
        lines[first + 2]
    );
}

#[test]
fn no_line_runs_past_the_pane_on_a_narrow_terminal() {
    let mut app = App::new();
    app.apply(snapshot(vec![bench_node()]));
    app.enter();

    for width in [60u16, 80, 100, 120] {
        for line in support::render(&mut app, width, HEIGHT) {
            assert!(
                line.chars().count() <= usize::from(width),
                "a line ran past a {width}-column terminal: {line}"
            );
        }
    }
}
