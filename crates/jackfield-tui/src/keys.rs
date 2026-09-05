//! Turning keystrokes into what the operator meant.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a keystroke asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move the selection down.
    Next,
    /// Move the selection up.
    Previous,
    /// Enter the selected Node.
    Enter,
    /// Return to the Node list.
    Back,
    /// Open or close a Sender's Receiver list.
    Expand,
    /// Re-read the selected Node.
    Refresh,
    /// Change what the highlighted resource is doing: a Sender on or off air,
    /// a Receiver onto the marked Sender or off everything.
    Toggle,
    /// Mark the highlighted Sender as the source the next subscription takes.
    Mark,
    /// Leave.
    Quit,
}

/// What a keystroke means, if it means anything.
///
/// Only a press counts. Windows reports a release for every key, and a terminal
/// speaking the kitty keyboard protocol reports releases and repeats too; acting
/// on those would move the selection twice for one keypress.
#[must_use]
pub fn action_for(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Next),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Previous),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Some(Action::Enter),
        KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => Some(Action::Back),
        KeyCode::Char(' ') => Some(Action::Expand),
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Char('t') => Some(Action::Toggle),
        KeyCode::Char('m') => Some(Action::Mark),
        KeyCode::Char('q') => Some(Action::Quit),
        // Ctrl-C, because a terminal application that ignores it is a
        // terminal application people kill from another window.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        _ => None,
    }
}
