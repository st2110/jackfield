//! The boundary from `docs/adr/0003-crate-split-engine-owns-state.md`, enforced
//! rather than merely intended.
//!
//! This crate renders a snapshot and turns keystrokes into commands. It does
//! not speak a protocol and it does not touch the network. A dependency that
//! let it do either would be the first step in dissolving the seam that makes
//! the engine testable headless and a second front end possible.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

/// Crates this one must never reach for.
const FORBIDDEN: &[&str] = &[
    "reqwest",
    "hyper",
    "mdns-sd",
    "mdns_sd",
    "trust-dns",
    "socket2",
    "tokio::net",
    "ureq",
    "curl",
];

fn manifest() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("the manifest is readable")
}

#[test]
fn this_crate_depends_on_no_http_or_mdns_crate() {
    let manifest = manifest();
    let dependencies = manifest
        .split("[dev-dependencies]")
        .next()
        .expect("a manifest has a first section");

    for forbidden in FORBIDDEN {
        assert!(
            !dependencies.contains(forbidden),
            "`{forbidden}` has appeared in this crate's dependencies"
        );
    }
}

#[test]
fn no_source_file_here_makes_a_network_call() {
    for path in source_files() {
        let text = std::fs::read_to_string(&path).expect("source is readable");
        let code: String = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for forbidden in FORBIDDEN {
            assert!(
                !code.contains(forbidden),
                "{}: `{forbidden}` is not this crate's business",
                path.display()
            );
        }
        for forbidden in ["TcpStream", "UdpSocket", "std::net::TcpListener"] {
            assert!(
                !code.contains(forbidden),
                "{}: `{forbidden}` is not this crate's business",
                path.display()
            );
        }
    }
}

#[test]
fn the_boundary_check_would_notice_a_breach() {
    // Two green assertions over a small crate prove nothing on their own.
    let pretend = "let client = reqwest::Client::new();";
    assert!(
        FORBIDDEN
            .iter()
            .any(|forbidden| pretend.contains(forbidden))
    );
}

fn source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files.sort();
    files
}
