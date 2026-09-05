//! The glossary in `CONTEXT.md` is only worth having if the code obeys it, so
//! the words it forbids are checked rather than reviewed.
//!
//! The check is deliberately narrow. It cannot see that a type called `Device`
//! is being used to mean a Node — that stays a review matter — but it can see
//! the words that are never right here, and those are the ones that creep in:
//! `active` for a Sender (see `docs/adr/0004-connection-vocabulary-and-graph.md`)
//! and `input`/`output` for a Receiver and Sender.

// This file is test code in its entirety.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

/// Words that must not appear in a public API name, and why.
const FORBIDDEN: &[(&str, &str)] = &[
    ("input", "a Receiver is a Receiver, not an input"),
    ("output", "a Sender is a Sender, not an output"),
    ("active", "a Sender is Transmitting or Idle, never active"),
    ("inactive", "a Sender is Idle; a Receiver is Unsubscribed"),
    ("connected", "only a pair is connected, never one end of it"),
    ("enabled", "not a word this domain uses"),
    ("disabled", "not a word this domain uses"),
];

/// Public API names that carry a forbidden word for a reason, each justified.
///
/// The IS-05 Connection API names an endpoint `active`; a type modelling that
/// endpoint's response is faithful to the protocol, not a description of a
/// Sender's state, and lives in the `nmos` crate where protocol names belong.
const ALLOWED: &[&str] = &[
    // IS-05's own transport parameter, `rtp_enabled`. It says whether RTP is
    // running on one leg of a transport, which is a protocol fact, not a
    // description of what a Sender is doing — that is `Transmission`.
    "rtp_enabled",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under `crates/*/src`, recursively.
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
    let crates = workspace_root().join("crates");
    for entry in std::fs::read_dir(&crates)
        .expect("crates/ is readable")
        .flatten()
    {
        walk(&entry.path().join("src"), &mut files);
    }
    files.sort();
    files
}

/// The identifier declared by a line introducing a public item, if any.
///
/// Crude on purpose: a full parser would be a dependency and a maintenance
/// burden for a check whose whole value is that it is impossible to argue with.
fn public_identifier(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = line.strip_prefix("pub ")?;
    // `pub(crate)` and friends are not public API.
    if rest.starts_with('(') {
        return None;
    }
    // Strip modifiers before the item keyword, so `pub async fn sender_is_active`
    // is seen as `sender_is_active` and not as `async`.
    let mut rest = rest;
    loop {
        let stripped = ["async ", "unsafe ", "extern ", "default "]
            .iter()
            .find_map(|kw| rest.strip_prefix(kw));
        match stripped {
            Some(next) => rest = next.trim_start(),
            None => break,
        }
    }
    let rest = [
        "fn ", "struct ", "enum ", "trait ", "const ", "static ", "type ", "mod ", "union ",
    ]
    .iter()
    .find_map(|kw| rest.strip_prefix(kw))
    .unwrap_or(rest);
    let name: &str = rest
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|s| !s.is_empty())?;
    Some(name)
}

/// Split an identifier into lowercase words, handling both `snake_case` and
/// `CamelCase`.
fn words(identifier: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in identifier.chars() {
        if ch == '_' {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            out.push(std::mem::take(&mut current));
            current.push(ch.to_ascii_lowercase());
        } else {
            current.push(ch.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[test]
fn no_public_api_name_uses_a_forbidden_word() {
    let mut problems = Vec::new();

    for path in source_files() {
        let text = std::fs::read_to_string(&path).expect("source is readable");
        for (number, line) in text.lines().enumerate() {
            let Some(identifier) = public_identifier(line) else {
                continue;
            };
            if ALLOWED.contains(&identifier) {
                continue;
            }
            let parts = words(identifier);
            for (word, why) in FORBIDDEN {
                if parts.iter().any(|p| p == word) {
                    problems.push(format!(
                        "{}:{}: `{identifier}` uses `{word}` — {why}",
                        path.display(),
                        number + 1
                    ));
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "public API names contradict CONTEXT.md:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_vocabulary_check_catches_a_forbidden_name() {
    // Two green assertions over an empty codebase would prove nothing.
    assert_eq!(
        public_identifier("pub fn sender_is_active() -> bool {"),
        Some("sender_is_active")
    );
    // Modifiers between `pub` and the item keyword once hid the name entirely,
    // which would have let `pub async fn sender_is_active` through unseen.
    assert_eq!(
        public_identifier("pub async fn sender_is_active() {"),
        Some("sender_is_active")
    );
    assert_eq!(
        public_identifier("pub unsafe fn active() {"),
        Some("active")
    );
    assert_eq!(
        public_identifier("pub const ACTIVE: u8 = 1;"),
        Some("ACTIVE")
    );
    assert!(words("sender_is_active").contains(&"active".to_string()));
    assert!(words("ActiveSender").contains(&"active".to_string()));
    assert!(words("VideoInput").contains(&"input".to_string()));
}

#[test]
fn the_vocabulary_check_ignores_what_is_not_public_api() {
    assert_eq!(public_identifier("fn active() {}"), None);
    assert_eq!(public_identifier("pub(crate) fn active() {}"), None);
    assert_eq!(public_identifier("// pub fn active"), None);
}

#[test]
fn every_forbidden_word_is_listed_in_context_md() {
    // The check and the glossary must not drift apart: a word forbidden here
    // has to be a word `CONTEXT.md` tells a reader to avoid.
    let context = std::fs::read_to_string(workspace_root().join("CONTEXT.md"))
        .expect("CONTEXT.md is readable");
    let avoided: String = context
        .lines()
        .filter(|l| l.starts_with("_Avoid_"))
        .collect::<Vec<_>>()
        .join(" ");

    for (word, _) in FORBIDDEN {
        assert!(
            avoided.contains(word),
            "`{word}` is forbidden by this test but not listed under any _Avoid_ in CONTEXT.md"
        );
    }
}
