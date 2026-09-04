//! Reading the keyboard without letting it stall anything.
//!
//! `crossterm` reads terminal input by blocking, so it is read on a thread of
//! its own and forwarded into the runtime as messages. The application's loop
//! then waits on exactly two things — a keystroke or a snapshot — which is what
//! keeps a slow network from swallowing a keypress and a keypress from
//! delaying a fetch.

use ratatui::crossterm::event::{self, Event};
use tokio::sync::mpsc;

/// How many keystrokes may queue before the oldest are dropped.
///
/// An operator leaning on a key while the screen is busy should not be able to
/// grow a queue without bound; losing the middle of a key repeat is harmless,
/// and the last one still arrives.
const CAPACITY: usize = 32;

/// Start reading the keyboard, returning where the keystrokes arrive.
///
/// The thread ends when the receiver is dropped, or when reading the terminal
/// fails — which happens on exit, and is not worth complaining about.
#[must_use]
pub fn keys() -> mpsc::Receiver<ratatui::crossterm::event::KeyEvent> {
    let (tx, rx) = mpsc::channel(CAPACITY);

    std::thread::Builder::new()
        .name("jackfield-input".to_owned())
        .spawn(move || {
            loop {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if tx.blocking_send(key).is_err() {
                            break;
                        }
                    }
                    // Resizes and mouse events redraw on the next tick; nothing
                    // here has to act on them.
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        })
        // A controller that cannot read the keyboard is still worth running: it
        // shows the network and is quit from another window. Saying so beats
        // refusing to start.
        .map_or_else(
            |error| {
                tracing::error!(%error, "cannot read the keyboard");
            },
            |_| (),
        );

    rx
}
