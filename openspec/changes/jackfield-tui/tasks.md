# jackfield-tui — tasks

Order follows the dependency chain in `design.md`: house rules and vendored
contract first, then the model that must satisfy that contract, then the clients
that produce it, then the engine that holds it, then the screen that shows it.
Per `AGENTS.md`, the test comes before the code in every implementation task, and
each task names how it is verified.

## 1. Workspace and house rules

- [ ] 1.1 Create the Cargo workspace: root `Cargo.toml` with members
      `jackfield-nmos`, `jackfield-engine`, `jackfield-tui`, `jackfield`,
      edition 2024, shared `[workspace.package]` metadata (Apache-2.0, repository
      pointing at the public mirror); verify `cargo metadata` lists all four
      members and `cargo build` succeeds on the empty crates
- [ ] 1.2 Add `[workspace.lints]` denying panics per `AGENTS.md`
      (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `todo`,
      `unimplemented`, `correctness`), inherited by every crate with
      `[lints] workspace = true`; verify by adding a temporary `.unwrap()` in a
      non-test function and confirming `cargo clippy --all-targets` fails on it,
      then removing it
- [ ] 1.3 Add `rustfmt.toml`, `rust-toolchain.toml` pinning the toolchain, and a
      `Makefile` with `fmt`, `lint`, `test` targets; verify `make lint` and
      `make test` both run clean on the empty workspace
- [ ] 1.4 Add `.gitignore` entries for `target/`; verify `git status` is clean
      after a full `cargo build`

## 2. Vendored contract and architecture decisions

- [ ] 2.1 Vendor the IS-04 `v1.3.x` JSON Schemas into
      `schemas/is-04/v1.3/` byte-for-byte, with a `PROVENANCE.md` recording the
      upstream repository, tag, and commit, and the upstream Apache-2.0 licence
      alongside; verify every `$ref` in the vendored set resolves to a vendored
      file (a test walks the directory and fails on a dangling reference)
- [ ] 2.2 Vendor the IS-05 `v1.1.x` JSON Schemas into `schemas/is-05/v1.1/` on
      the same terms; verify with the same reference-resolution test
- [ ] 2.3 Vendor the published resource examples used by the round-trip tests
      into `schemas/examples/`; verify the test harness can enumerate them and
      that the set is non-empty for every resource type this change consumes
- [ ] 2.4 Write `docs/adr/0001-nmos-types-by-hand-schemas-as-validators.md`
      (design D1, D2); verify it states the draft-04 constraint, the rejected
      generator alternative, and the two-directional validation rule
- [ ] 2.5 Write `docs/adr/0002-peer-to-peer-mdns-discovery.md` (design D3);
      verify it records why no registry client and why no `avahi-browse`
- [ ] 2.6 Write `docs/adr/0003-crate-split-engine-owns-state.md` (design D4);
      verify it records the channel seam and what each crate may not know

## 3. Resource model (`jackfield-nmos`)

- [ ] 3.1 Write the schema-validation test harness first: load a vendored schema
      with the `jsonschema` crate, assert draft-04 support, and prove the harness
      fails on a deliberately wrong document; verify the harness's own negative
      test passes
- [ ] 3.2 Model the core resource fields shared by every resource (`id`,
      `version`, `label`, `description`, `tags`) as a `ResourceCore`; verify
      every vendored example of every resource type parses and re-serializes to
      a document validating against `resource_core.json`
- [ ] 3.3 Model `Node`, `Device`, `Sender`, `Receiver`, `Source`; verify each
      vendored example parses into its type and each serialization validates
      against its schema
- [ ] 3.4 Model `Flow` as a Rust enum over the specification's variants (video
      raw and coded, audio raw and coded, data, SDI ancillary data, mux); verify
      every vendored flow example parses into the correct variant and the wrong
      variant is never selected
- [ ] 3.5 Model `Receiver` capability variants (video, audio, data, mux) as an
      enum on the same terms; verify with the vendored receiver examples
- [ ] 3.6 Make every resource tolerate unknown fields; verify a test that adds a
      vendor-specific field to each example still parses and that the field's
      presence changes nothing else (spec: `node-inventory`, unknown fields)
- [ ] 3.7 Property test the identifier and version types against malformed input
      (empty, oversized, non-UUID, non-numeric version); verify `proptest` finds
      no input that panics — the parse returns an error for every rejection

## 4. IS-04 Node API client (`jackfield-nmos`)

- [ ] 4.1 Write the client's tests first against `wiremock`: a fixture Node
      serving `self`, `devices`, `senders`, `receivers`, `flows`, `sources`;
      verify the fixture is exercised by a failing test before the client exists
- [ ] 4.2 Implement the version-negotiating base URL (`v1.3` then `v1.2`,
      design D6); verify a Node offering only `v1.2` is addressed at the `v1.2`
      prefix and a Node offering neither yields an unsupported-version error
      naming what was offered
- [ ] 4.3 Implement fetching the six collections with per-request and connect
      timeouts; verify against the fixture that a complete tree is returned, and
      that a Node exposing no Devices returns an empty list rather than an error
      (spec: `node-inventory`, Node exposing nothing)
- [ ] 4.4 Cover the failure modes with tests: connection refused, timeout, HTTP
      500, HTML instead of JSON, truncated JSON; verify each produces a distinct,
      named error and none panics (spec: `node-inventory`, unreachable Node)

## 5. IS-05 Connection API client (`jackfield-nmos`)

- [ ] 5.1 Write tests first for reading `/single/senders/{id}/active` and
      `/single/receivers/{id}/active` from a `wiremock` fixture; verify the tests
      fail before the client exists
- [ ] 5.2 Implement the read-only client with `v1.1` then `v1.0` negotiation;
      verify an active Sender yields its destination address and port, and an
      inactive one yields no destination
- [ ] 5.3 Derive connection state (active, inactive, unknown) from
      `master_enable` and the transport parameters; verify a Node with no
      reachable Connection API yields `unknown` and never `inactive`
      (spec: `node-inventory`, Node without a Connection API)
- [ ] 5.4 Record a Receiver's subscribed Sender when the active state names one;
      verify the pairing survives a round trip through the fixture
- [ ] 5.5 Assert by test that the crate exposes no way to write to a device —
      no `staged` PATCH, no activation; verify the public surface contains no
      such function (this change is read-only by contract)

## 6. Discovery (`jackfield-engine`)

- [ ] 6.1 Define discovery as a stream of appear and depart events behind an
      interface the tests can drive with fabricated advertisements; verify a
      fabricated advertisement produces an appear event without any network
- [ ] 6.2 Implement the `mdns-sd` browser for `_nmos-node._tcp`; verify against a
      locally registered service that it is discovered, and that discovery works
      with no system mDNS daemon installed (spec: `node-discovery`)
- [ ] 6.3 Parse the TXT records (`api_proto`, `api_ver`, `api_auth`) with IS-04
      defaults for absent records; verify `api_ver=v1.2,v1.3` yields both, an
      absent `api_proto` yields the default rather than a dropped advertisement,
      and `api_auth=true` marks the Node as requiring authorization
- [ ] 6.4 Deduplicate by address and port; verify one Node advertised on two
      interfaces appears once, and two Nodes on one host with different ports
      appear twice
- [ ] 6.5 Handle departure and re-appearance; verify a goodbye produces a depart
      event and a subsequent advertisement produces a fresh appear event
- [ ] 6.6 Test malformed and hostile advertisements: no address, no port,
      oversized TXT, invalid UTF-8 in TXT; verify each is ignored, discovery
      keeps running, and nothing panics

## 7. Inventory (`jackfield-engine`)

- [ ] 7.1 Model the inventory: Nodes, each either fetched, still loading, or
      failed with a reason; verify state transitions with unit tests over a
      fabricated fetcher, with no network
- [ ] 7.2 Resolve references — Senders and Receivers to their Device, Senders to
      their Flow; verify a dangling `flow_id` yields an unresolved marker while
      the rest of the tree stays usable (spec: `node-inventory`, dangling
      reference)
- [ ] 7.3 Fetch Nodes concurrently, one task per Node with its own timeout;
      verify one unreachable Node does not delay a responsive one and that its
      failure is confined to its own entry
- [ ] 7.4 Implement refresh replacing a Node's tree; verify a Sender removed at
      the Node disappears on refresh, and a previously failed Node becomes
      browsable when it starts answering
- [ ] 7.5 Publish inventory snapshots on a channel and accept commands on
      another (design D4); verify a command to refresh a Node reaches the
      fetcher and a resulting snapshot reaches the subscriber
- [ ] 7.6 Assert by test that nothing is written to disk: run a full discovery
      and fetch cycle in a temporary directory and verify no file was created or
      modified (spec: `node-inventory`, state held in memory only)

## 8. Terminal interface (`jackfield-tui`)

- [ ] 8.1 Establish rendering tests against `ratatui`'s test backend, driven by a
      fabricated snapshot with no engine and no network; verify a snapshot
      renders deterministically into a buffer the test can assert on
- [ ] 8.2 Render the left pane: the Node list with label and address, including
      the loading, failed, and departed presentations; verify each state is
      distinguishable in the rendered buffer and the empty-network case states
      itself in words (spec: `tui-browser`, main screen)
- [ ] 8.3 Render the detail pane: Devices with their Senders and Receivers,
      Senders visually distinct from Receivers; verify a Node with several
      Devices renders unambiguous attribution and an empty Device still appears
- [ ] 8.4 Render connection state as active, inactive, or unknown; verify all
      three are distinguishable in a buffer with no colour applied
      (spec: `tui-browser`, state does not rely on colour alone)
- [ ] 8.5 Implement the keyboard model — move, enter, back, refresh, quit — and
      the on-screen key hints; verify each binding against the test backend,
      including that back restores the previous selection
- [ ] 8.6 Implement the terminal guard restoring raw mode, the alternate screen,
      and the cursor on `Drop`; verify restoration runs on normal exit and on
      panic unwind
- [ ] 8.7 Handle layout edges: resize, labels longer than the pane, more Nodes
      than fit; verify truncation and scrolling keep the layout intact and the
      selected Node visible
- [ ] 8.8 Verify by test that the crate depends on no HTTP or mDNS crate and
      performs no network call — the boundary from design D4 is enforced, not
      merely intended

## 9. Binary (`jackfield`)

- [ ] 9.1 Wire discovery, inventory, and interface together over the channels;
      verify the binary starts against a fixture Node and renders it
- [ ] 9.2 Add argument parsing and `tracing` logging to a file or stderr that
      does not corrupt the alternate screen; verify logs are readable after a run
      and the display is undisturbed during one
- [ ] 9.3 Ensure the event loop waits on terminal input and engine snapshots
      together (design D5); verify a keystroke is served while a Node fetch is
      deliberately stalled (spec: `tui-browser`, quitting during a slow fetch)
- [ ] 9.4 Add an end-to-end test: two fixture Nodes, one healthy and one
      refusing connections, driven through discovery to a rendered screen;
      verify the healthy Node is browsable and the failure is confined to its row

## 10. Verification

- [ ] 10.1 Run the full gate: `make fmt`, `make lint`, `make test`; verify no
      warnings and no failures
- [ ] 10.2 Run against real hardware on the bench, cross-checking the Node list
      and connection states against `nmosctl.py discover` and
      `nmosctl.py list <host>`; verify the two agree, and record any divergence
      as a regression test before fixing it (design: `mdns-sd` interoperability
      risk)
- [ ] 10.3 Confirm the read-only contract on the bench: no device changed state
      during the session; verify by reading each Sender's and Receiver's active
      state before and after and finding them identical
