//! Giving the terminal back.
//!
//! Restoration hangs on `Drop`, which Rust runs on a normal exit and on a panic
//! unwind alike. What that guarantee needs from this project is two things a
//! test can actually check: that the release profile does not turn a panic into
//! an abort, which would skip every `Drop`; and that a failed acquisition does
//! not leave raw mode on for a shell that did nothing to deserve it.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

#[test]
fn the_release_profile_does_not_abort_on_panic() {
    // `panic = "abort"` would skip the guard's `Drop` and leave an operator's
    // terminal in raw mode. See `crates/jackfield-tui/src/terminal.rs`.
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
            .expect("the workspace manifest is readable");

    // Comments stripped: the profile explains why it does *not* set this, and
    // the explanation must not trip the check that enforces it.
    let settings: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !settings.contains(r#"panic = "abort""#),
        "the release profile aborts on panic, so the terminal guard would never run"
    );
}

#[test]
fn drop_runs_while_unwinding() {
    // The mechanism the guard relies on, stated as a test so that a future
    // change to the profile or to the guard's shape has something to fail.
    use std::sync::atomic::{AtomicBool, Ordering};
    static RESTORED: AtomicBool = AtomicBool::new(false);

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            RESTORED.store(true, Ordering::SeqCst);
        }
    }

    let outcome = std::panic::catch_unwind(|| {
        let _guard = Guard;
        panic!("something went wrong deep inside the application");
    });

    assert!(outcome.is_err());
    assert!(
        RESTORED.load(Ordering::SeqCst),
        "the guard did not run while unwinding"
    );
}

#[test]
fn taking_a_terminal_that_is_not_one_fails_rather_than_panicking() {
    // Under `cargo test` stdin is not a terminal, so this exercises the failure
    // path: it must return, and it must not leave raw mode enabled behind it.
    let taken = jackfield_tui::TerminalGuard::take();

    if let Ok(guard) = taken {
        // Some CI runners do give a terminal. Dropping it restores whatever it
        // took, which is the whole point.
        drop(guard);
        return;
    }

    let still_raw = ratatui::crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    assert!(
        !still_raw,
        "a failed acquisition left the terminal in raw mode"
    );
}
