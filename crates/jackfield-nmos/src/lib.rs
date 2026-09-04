//! The NMOS contract: the resource model of IS-04 and the read-only clients for
//! the Node API and the Connection API.
//!
//! This crate knows the protocol and nothing else. It holds no state, performs
//! no discovery, and has no opinion about how anything is displayed.

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
