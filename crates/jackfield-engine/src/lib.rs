//! The controller's picture of the network: discovery of NMOS Nodes, the
//! in-memory inventory built from them, and the connection graph derived across
//! every Node it knows.
//!
//! This crate owns the truth. It knows nothing about a terminal.

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
