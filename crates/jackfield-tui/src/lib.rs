//! The terminal interface: renders an inventory snapshot and turns keystrokes
//! into commands.
//!
//! This crate owns the screen. It performs no network calls and holds no
//! authoritative state; see `docs/adr/0003-crate-split-engine-owns-state.md`.

// Panics are forbidden in production code (AGENTS.md); tests are the one place
// where they are the clearest way to assert.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]
