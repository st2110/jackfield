# Four crates: the engine owns the truth, the interface owns the screen

> The protocol crate has since been published as `nmos` and is no longer in this
> workspace. The boundary below is unchanged; only its address is.

The workspace is split so that the compiler, not discipline, enforces the
boundary:

- `nmos` — the resource model and the IS-04 and IS-05 clients. Knows the
  protocol; **may not know** about state, discovery, or a screen. It lived here
  as `jackfield-nmos` until it was published on its own
  (<https://crates.io/crates/nmos>); the boundary this record draws is what made
  the split a move rather than an extraction.
- `jackfield-engine` — discovery, the in-memory inventory, and the fetching that
  fills it. Knows the domain; **may not know** that a terminal exists.
- `jackfield-tui` — `ratatui` and `crossterm`. Renders a snapshot and turns
  keystrokes into commands; **may not** contain protocol knowledge, and may not
  depend on an HTTP or mDNS crate at all.
- `jackfield` — the binary: arguments, logging, and wiring the three together.

The seam between engine and interface is a **pair of channels**: the engine
publishes inventory snapshots, the interface sends commands (refresh this Node;
later, connect this pair). The interface never calls the network and never holds
the authoritative copy of anything. That is what makes the engine testable
headless and the interface testable against a fabricated snapshot, and it is what
leaves room for a second front end — the web server named in `README.md` — without
rewriting the domain.

## Considered options

**A single crate with modules.** Rejected: the compiler stops enforcing the
boundary the moment it is convenient to cross it, and the boundary is the point.

## Consequences

The `jackfield-tui` crate carries a test asserting it depends on no HTTP or mDNS
crate, so the boundary is checked rather than merely intended.
