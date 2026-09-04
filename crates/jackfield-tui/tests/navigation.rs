//! Moving around.
//!
//! Regression tests for a report from the bench: with one Node on the network,
//! every key except `q` appeared to do nothing. Each of these fails against the
//! interface as it was, and each names a distinct reason it failed.

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

/// The size of a terminal an engineer actually has.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 30;

fn looking_at_the_bench() -> App {
    let mut app = App::new();
    app.apply(snapshot(vec![bench_node()]));
    app
}

#[test]
fn entering_a_node_changes_what_the_screen_shows() {
    // Reported: pressing enter or right did nothing visible, so the interface
    // looked frozen. Whatever else it does, entering has to look like something.
    let mut app = looking_at_the_bench();
    let before = screen(&mut app, WIDTH, HEIGHT);

    app.enter();
    let after = screen(&mut app, WIDTH, HEIGHT);

    assert_ne!(before, after, "entering a Node changed nothing on screen");
}

#[test]
fn leaving_a_node_changes_what_the_screen_shows() {
    let mut app = looking_at_the_bench();
    app.enter();
    let inside = screen(&mut app, WIDTH, HEIGHT);

    app.back();
    let outside = screen(&mut app, WIDTH, HEIGHT);

    assert_ne!(inside, outside, "going back changed nothing on screen");
}

#[test]
fn the_focused_pane_is_marked_without_colour() {
    let mut app = looking_at_the_bench();
    let listing = screen(&mut app, WIDTH, HEIGHT);
    app.enter();
    let inside = screen(&mut app, WIDTH, HEIGHT);

    // Some mark has to move from one pane to the other, legibly, in text.
    assert!(
        listing.contains("Nodes ◂") || listing.contains("[Nodes]"),
        "the list pane does not say it has focus:\n{listing}"
    );
    assert!(
        !inside.contains("Nodes ◂") && !inside.contains("[Nodes]"),
        "the list pane still claims focus after entering a Node:\n{inside}"
    );
}

#[test]
fn moving_the_highlight_inside_a_node_changes_what_the_screen_shows() {
    // Reported: up and down did nothing. Inside a Node they move a highlight —
    // which was tracked but never drawn, so nothing moved on screen.
    let mut app = looking_at_the_bench();
    app.enter();
    let first = screen(&mut app, WIDTH, HEIGHT);

    app.detail_next();
    let second = screen(&mut app, WIDTH, HEIGHT);

    assert_ne!(
        first, second,
        "moving the highlight changed nothing on screen"
    );
}

#[test]
fn the_highlighted_row_is_marked_in_text() {
    let mut app = looking_at_the_bench();
    app.enter();

    let text = screen(&mut app, WIDTH, HEIGHT);
    let marked: Vec<&str> = text.lines().filter(|line| line.contains('▸')).collect();
    assert_eq!(
        marked.len(),
        1,
        "expected exactly one marked row, got {marked:?}"
    );
    assert!(
        marked[0].contains("Sender"),
        "the first selectable row is a Sender: {marked:?}"
    );
}

#[test]
fn the_highlight_walks_every_sender_and_receiver_of_every_device() {
    let mut app = looking_at_the_bench();
    app.enter();

    // Three Devices of three Senders and three Receivers each.
    assert_eq!(
        app.detail_len(),
        18,
        "every Sender and Receiver is selectable"
    );

    let mut seen = Vec::new();
    for _ in 0..app.detail_len() {
        let text = screen(&mut app, WIDTH, HEIGHT);
        let marked = text
            .lines()
            .find(|line| line.contains('▸'))
            .unwrap_or_default()
            .trim()
            .to_owned();
        assert!(!marked.is_empty(), "the highlight left the screen:\n{text}");
        seen.push(marked);
        app.detail_next();
    }

    assert_eq!(seen.len(), 18);
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        18,
        "the highlight visited the same row twice: {seen:?}"
    );
}

#[test]
fn the_highlight_does_not_run_off_either_end() {
    let mut app = looking_at_the_bench();
    app.enter();

    for _ in 0..100 {
        app.detail_next();
    }
    assert_eq!(app.detail_row(), app.detail_len() - 1);
    assert!(
        screen(&mut app, WIDTH, HEIGHT).contains('▸'),
        "the highlight fell off the end"
    );

    for _ in 0..100 {
        app.detail_previous();
    }
    assert_eq!(app.detail_row(), 0);
}

#[test]
fn the_detail_pane_scrolls_so_the_highlight_stays_visible() {
    // A bench Node is forty-odd rows; a terminal is thirty. Without scrolling
    // everything past the first Device is unreachable, which is exactly what
    // was reported.
    let mut app = looking_at_the_bench();
    app.enter();
    for _ in 0..app.detail_len() {
        app.detail_next();
    }

    let text = screen(&mut app, WIDTH, HEIGHT);
    assert!(
        text.contains('▸'),
        "the highlight scrolled out of view:\n{text}"
    );
    assert!(
        text.contains("SDI 3"),
        "the last Device is unreachable:\n{text}"
    );
}

#[test]
fn expanding_opens_the_row_under_the_highlight() {
    let mut app = looking_at_the_bench();
    app.enter();

    let before = screen(&mut app, WIDTH, HEIGHT);
    app.toggle_selected();
    let after = screen(&mut app, WIDTH, HEIGHT);
    assert_ne!(
        before, after,
        "expanding the highlighted row changed nothing"
    );

    app.toggle_selected();
    assert_eq!(
        screen(&mut app, WIDTH, HEIGHT),
        before,
        "expanding does not close again"
    );
}

#[test]
fn expanding_a_receiver_does_nothing_rather_than_expanding_a_sender() {
    // The highlight walks Senders and Receivers alike; only Senders have a list
    // to open, and pressing space on a Receiver must not open somebody else's.
    let mut app = looking_at_the_bench();
    app.enter();
    for _ in 0..3 {
        app.detail_next();
    }

    let before = screen(&mut app, WIDTH, HEIGHT);
    app.toggle_selected();
    assert_eq!(screen(&mut app, WIDTH, HEIGHT), before);
}

#[test]
fn the_node_list_still_moves_when_there_is_more_than_one_node() {
    let mut app = App::new();
    let mut second = bench_node();
    second.key = jackfield_engine::NodeKey::Settled(support::id(2));
    app.apply(snapshot(vec![bench_node(), second]));

    let first = screen(&mut app, WIDTH, HEIGHT);
    app.select_next();
    let moved = screen(&mut app, WIDTH, HEIGHT);
    assert_ne!(first, moved, "the list selection changed nothing on screen");
}
