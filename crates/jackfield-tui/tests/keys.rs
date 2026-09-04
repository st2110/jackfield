//! The keyboard drives the application.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use jackfield_tui::{Action, App, Screen, action_for};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use support::{known, ready, snapshot};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn every_binding_the_hints_promise_is_answered() {
    assert_eq!(action_for(key(KeyCode::Down)), Some(Action::Next));
    assert_eq!(action_for(key(KeyCode::Up)), Some(Action::Previous));
    assert_eq!(action_for(key(KeyCode::Enter)), Some(Action::Enter));
    assert_eq!(action_for(key(KeyCode::Esc)), Some(Action::Back));
    assert_eq!(action_for(key(KeyCode::Char(' '))), Some(Action::Expand));
    assert_eq!(action_for(key(KeyCode::Char('r'))), Some(Action::Refresh));
    assert_eq!(action_for(key(KeyCode::Char('q'))), Some(Action::Quit));
}

#[test]
fn the_vi_keys_work_too() {
    assert_eq!(action_for(key(KeyCode::Char('j'))), Some(Action::Next));
    assert_eq!(action_for(key(KeyCode::Char('k'))), Some(Action::Previous));
    assert_eq!(action_for(key(KeyCode::Char('l'))), Some(Action::Enter));
    assert_eq!(action_for(key(KeyCode::Char('h'))), Some(Action::Back));
}

#[test]
fn ctrl_c_quits() {
    // A terminal application that ignores it is one people kill from another
    // window, leaving the terminal in raw mode.
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(action_for(ctrl_c), Some(Action::Quit));

    // Without the modifier it is just a letter.
    assert_eq!(action_for(key(KeyCode::Char('c'))), None);
}

#[test]
fn a_key_that_means_nothing_means_nothing() {
    for code in [
        KeyCode::F(5),
        KeyCode::Tab,
        KeyCode::Char('z'),
        KeyCode::Backspace,
    ] {
        assert_eq!(action_for(key(code)), None, "{code:?}");
    }
}

#[test]
fn moving_and_entering_shows_the_selected_node() {
    let mut app = App::new();
    app.apply(snapshot(vec![
        known(1, "first", [10, 77, 1, 90], ready("First", Vec::new())),
        known(5, "second", [10, 77, 1, 91], ready("Second", Vec::new())),
    ]));

    assert_eq!(app.screen(), Screen::Nodes);
    app.select_next();
    app.enter();

    assert_eq!(app.screen(), Screen::Node);
    assert_eq!(app.selected().expect("selected").display_name(), "Second");
}

#[test]
fn entering_nothing_does_nothing() {
    let mut app = App::new();
    app.enter();
    assert_eq!(app.screen(), Screen::Nodes, "there is nothing to enter");
}

#[test]
fn expanding_a_sender_toggles() {
    let mut app = App::new();
    let sender = support::id(100);
    assert!(!app.is_expanded(&sender));
    app.toggle_expanded(&sender);
    assert!(app.is_expanded(&sender));
    app.toggle_expanded(&sender);
    assert!(!app.is_expanded(&sender));
}

#[test]
fn only_a_press_is_a_keystroke() {
    // Windows reports a release for every key, and a terminal speaking the
    // kitty keyboard protocol reports releases and repeats. Acting on those
    // would move the selection twice for one keypress.
    for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
        let event = KeyEvent::new_with_kind_and_state(
            KeyCode::Down,
            KeyModifiers::NONE,
            kind,
            KeyEventState::NONE,
        );
        assert_eq!(action_for(event), None, "{kind:?}");
    }

    let press = KeyEvent::new_with_kind_and_state(
        KeyCode::Down,
        KeyModifiers::NONE,
        KeyEventKind::Press,
        KeyEventState::NONE,
    );
    assert_eq!(action_for(press), Some(Action::Next));
}
