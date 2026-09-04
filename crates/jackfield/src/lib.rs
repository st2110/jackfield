//! Wiring discovery, the inventory and the screen together.
//!
//! The logic lives here rather than in `main.rs` so that it can be tested: an
//! end-to-end run against fixture Nodes is the only place the three crates meet,
//! and it is worth having a test for.

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

mod options;
mod run;

pub use options::Options;
pub use run::{Outcome, describe, drive, run, run_headless, spawn_engine, spawn_engine_with, stop};
