//! The `jackfield` binary: wires discovery, the inventory, and the terminal
//! interface together.

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

fn main() {
    println!("jackfield");
}
