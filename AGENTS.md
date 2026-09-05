# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status

A Rust workspace of three crates — `jackfield-engine` (discovery, inventory,
fetching), `jackfield-tui` (the screen), and `jackfield` (the binary). The
protocol itself is not here: it is the published `nmos` crate
(<https://crates.io/crates/nmos>), and `docs/adr/0003-crate-split-engine-owns-state.md`
says where the boundary runs.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Context

- Jackfield is an NMOS controller for SMPTE ST 2110 networks: it discovers NMOS devices
  on the local network and configures them, running either as a TUI or as a web server.
- Part of the `streaming-workspace` multi-repo workspace (see `../CLAUDE.md` for the
  workspace-wide layout and the rule that each repo is committed separately).
- Default branch: `master`.
- Licensed under Apache-2.0 (see `LICENSE`).
- `CLAUDE.md` is a symlink to `AGENTS.md`, and `.claude` is a symlink to `.agents` —
  edit `AGENTS.md` and `.agents/`, not the symlinks.
- `.agents/` holds the committed agent config: skills and commands shared by everyone
  working on this repo. Its vendored skills are MIT-licensed (`rust-skills` by
  leonardomso, the openspec skills); keep their `license:` frontmatter intact.

## Rules

### Language: English only

This is an open source, international project. Everything written into the repository —
code comments, identifiers, documentation, commit messages, issue and MR descriptions,
log messages, error strings, UI copy — MUST be in English. No exceptions, even when the
conversation with the user happens in another language.

The only place other languages are allowed is localization data: locale files,
translation catalogs, and their test fixtures.

### Rust

When writing, reviewing, or refactoring Rust code — including dependency choices and
project structure — invoke the `rust-skills` skill first and follow its rules.

### Testing

- **Tests come first.** Write the test before the code it tests. A change that adds
  behaviour without a test that fails before it and passes after is not finished.
- **The happy path is not coverage.** Every test suite must also exercise the edges:
  empty and maximum-size input, zero and boundary values, malformed and hostile input,
  timeouts, disconnects, and concurrent access. Use `proptest` where the input space is
  large enough that hand-picked cases will miss things.
- **Every bug gets a regression test.** When diagnosing a problem, the first artifact is
  a test that reproduces it and fails. Fix the code only after that test is red. Never
  delete or weaken such a test; it is the proof the bug cannot come back.

### No panics in production code

`unwrap()`, `expect()`, `panic!()`, `todo!()`, and slice indexing that can go out of
bounds are forbidden outside tests — including in the binary entry points. A controller
that operates live 2110 devices must return an error, not abort. Propagate with `?`.

Enforce it with clippy rather than review:

```toml
[workspace.lints.clippy]
unwrap_used      = "deny"
expect_used      = "deny"
panic            = "deny"
indexing_slicing = "deny"
todo             = "deny"
unimplemented    = "deny"
correctness      = "deny"
```

Relax these in `#[cfg(test)]` code only. Do not add the per-crate
`unwrap_used = "allow"` override for binaries that `rust-skills` suggests.

### Working on tickets

Do not work on a ticket in the main checkout. For each ticket, create a dedicated
branch and a git worktree under `.agents/worktrees/`:

```bash
git worktree add -b <branch-name> .agents/worktrees/<branch-name>
```

Do the ticket's work inside that worktree, and remove it when the work is done.

### Mirrors

The project lives in two mirrors:

- A public one: <https://github.com/st2110/jackfield>. This is the only address
  that may be referenced anywhere in the repository.
- An internal corporate one. Its address, hostname, and the very fact of a specific
  internal location MUST NEVER appear in the sources — not in code, comments, docs,
  README, CI config, commit messages, issue links, or example URLs. When a link is
  needed, use the public mirror.

### Commits

- Keep the commit message body short: at most 3 lines of description. Say what changed
  and why; leave the details to the diff.
- If the work is done in the context of a ticket, put the ticket reference on the last
  line so it can be linked from the commit, e.g. a trailing `Refs #<ticket-id>` line.
- Commit messages are English only, like everything else in this repository.
