# jackfield-tui — tasks

Order follows the dependency chain in `design.md`: house rules and vendored
contract first, then the model that must satisfy that contract, then the clients
that produce it, then the engine that holds it, then the screen that shows it.
Per `AGENTS.md`, the test comes before the code in every implementation task, and
each task names how it is verified.

## 1. Workspace and house rules

- [x] 1.1 Create the Cargo workspace: root `Cargo.toml` with members
      `jackfield-nmos`, `jackfield-engine`, `jackfield-tui`, `jackfield`,
      edition 2024, shared `[workspace.package]` metadata (Apache-2.0, repository
      pointing at the public mirror); verify `cargo metadata` lists all four
      members and `cargo build` succeeds on the empty crates
- [x] 1.2 Add `[workspace.lints]` denying panics per `AGENTS.md`
      (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `todo`,
      `unimplemented`, `correctness`), inherited by every crate with
      `[lints] workspace = true`; verify by adding a temporary `.unwrap()` in a
      non-test function and confirming `cargo clippy --all-targets` fails on it,
      then removing it
- [x] 1.3 Add `rustfmt.toml`, `rust-toolchain.toml` pinning the toolchain, and a
      `Makefile` with `fmt`, `lint`, `test` targets; verify `make lint` and
      `make test` both run clean on the empty workspace
- [x] 1.4 Add `.gitignore` entries for `target/`; verify `git status` is clean
      after a full `cargo build`

## 2. Vendored contract and architecture decisions

- [x] 2.1 Vendor the IS-04 `v1.3.x` JSON Schemas into
      `schemas/is-04/v1.3/` byte-for-byte, with a `PROVENANCE.md` recording the
      upstream repository, tag, and commit, and the upstream Apache-2.0 licence
      alongside; verify every `$ref` in the vendored set resolves to a vendored
      file (a test walks the directory and fails on a dangling reference)
- [x] 2.2 Vendor the IS-05 `v1.1.x` JSON Schemas into `schemas/is-05/v1.1/` on
      the same terms; verify with the same reference-resolution test
- [x] 2.3 Vendor the published resource examples used by the round-trip tests
      into `schemas/examples/`; verify the test harness can enumerate them and
      that the set is non-empty for every resource type this change consumes
- [x] 2.4 Write `docs/adr/0001-nmos-types-by-hand-schemas-as-validators.md`
      (design D1, D2); verify it states the draft-04 constraint, the rejected
      generator alternative, and the two-directional validation rule
- [x] 2.5 Write `docs/adr/0002-peer-to-peer-mdns-discovery.md` (design D3);
      verify it records why no registry client and why no `avahi-browse`
- [x] 2.6 Write `docs/adr/0003-crate-split-engine-owns-state.md` (design D4);
      verify it records the channel seam and what each crate may not know
- [x] 2.7 Write `docs/adr/0004-connection-vocabulary-and-graph.md` (design D10);
      verify it records why Transmitting is not "connected", why the graph is
      network-wide, and why unresolved edges are surfaced rather than dropped
- [x] 2.8 Write `docs/adr/0005-two-tier-fetch.md` (design D9); verify it records
      that IS-04 carries connection state, that this hardware leaves the pairing
      identifiers null, and why the transport pass has a periodic floor
- [x] 2.9 Keep `CONTEXT.md` current as terms settle; verify every term used in
      the specs and in public type names appears there, and that no forbidden
      synonym (`active` for a Sender, `device` for a Node, `input`/`output`)
      appears in the crate's public API

## 3. Resource model (`jackfield-nmos`)

- [x] 3.1 Write the schema-validation test harness first: load a vendored schema
      with the `jsonschema` crate, assert draft-04 support, and prove the harness
      fails on a deliberately wrong document; verify the harness's own negative
      test passes
- [x] 3.2 Model the core resource fields shared by every resource (`id`,
      `version`, `label`, `description`, `tags`) as a `ResourceCore`; verify
      every vendored example of every resource type parses and re-serializes to
      a document validating against `resource_core.json`
- [x] 3.3 Model `Node`, `Device`, `Sender`, `Receiver`, `Source`; verify each
      vendored example parses into its type and each serialization validates
      against its schema
- [x] 3.4 Model `Flow` as a Rust enum over the specification's variants (video
      raw and coded, audio raw and coded, data, SDI ancillary data, mux); verify
      every vendored flow example parses into the correct variant and the wrong
      variant is never selected
- [x] 3.5 Model `Receiver` capability variants (video, audio, data, mux) as an
      enum on the same terms; verify with the vendored receiver examples
- [x] 3.6 Make every resource tolerate unknown fields; verify a test that adds a
      vendor-specific field to each example still parses and that the field's
      presence changes nothing else (spec: `node-inventory`, unknown fields)
- [x] 3.7 Property test the identifier and version types against malformed input
      (empty, oversized, non-UUID, non-numeric version); verify `proptest` finds
      no input that panics — the parse returns an error for every rejection
- [x] 3.8 Model the `subscription` object on Senders and Receivers, and map it to
      the domain states `Transmitting`/`Idle` and `Subscribed`/`Unsubscribed`
      (design D10, `CONTEXT.md`); verify a Sender with `{"active": true,
      "receiver_id": null}` maps to Transmitting with no pairing, and that no
      public name in the crate calls a Sender "active" or "connected"

## 4. IS-04 Node API client (`jackfield-nmos`)

- [x] 4.1 Write the client's tests first against `wiremock`: a fixture Node
      serving `self`, `devices`, `senders`, `receivers`, `flows`, `sources`;
      verify the fixture is exercised by a failing test before the client exists
- [x] 4.2 Implement the version-negotiating base URL (`v1.3` then `v1.2`,
      design D6); verify a Node offering only `v1.2` is addressed at the `v1.2`
      prefix and a Node offering neither yields an unsupported-version error
      naming what was offered
- [x] 4.3 Implement fetching the six collections with per-request and connect
      timeouts; verify against the fixture that a complete tree is returned, and
      that a Node exposing no Devices returns an empty list rather than an error
      (spec: `node-inventory`, Node exposing nothing)
- [x] 4.4 Cover the failure modes with tests: connection refused, timeout, HTTP
      500, HTML instead of JSON, truncated JSON; verify each produces a distinct,
      named error and none panics (spec: `node-inventory`, unreachable Node)

## 5. IS-05 Connection API client (`jackfield-nmos`)

- [x] 5.1 Write tests first for reading `/single/senders/{id}/active` and
      `/single/receivers/{id}/active` from a `wiremock` fixture; verify the tests
      fail before the client exists
- [x] 5.2 Implement the read-only client with `v1.1` then `v1.0` negotiation;
      verify an active Sender yields its destination address and port, and an
      inactive one yields no destination
- [x] 5.3 Return transport parameters only — connection state comes from IS-04
      (design D9); verify a Node with no reachable Connection API yields transport
      parameters as unknown while its Transmitting/Subscribed states, already read
      from the resource tree, remain valid
      (spec: `node-inventory`, Node without a reachable Connection API)
- [x] 5.4 Record a Receiver's subscribed Sender when the active state names one;
      verify the pairing survives a round trip through the fixture
- [x] 5.5 Assert by test that the crate exposes no way to write to a device —
      no `staged` PATCH, no activation; verify the public surface contains no
      such function (this change is read-only by contract)

## 6. Discovery (`jackfield-engine`)

- [x] 6.1 Define discovery as a stream of appear and depart events behind an
      interface the tests can drive with fabricated advertisements; verify a
      fabricated advertisement produces an appear event without any network
- [x] 6.2 Implement the `mdns-sd` browser for `_nmos-node._tcp`; verify against a
      locally registered service that it is discovered, and that discovery works
      with no system mDNS daemon installed (spec: `node-discovery`)
- [x] 6.3 Parse the TXT records (`api_proto`, `api_ver`, `api_auth`) with IS-04
      defaults for absent records; verify `api_ver=v1.0,v1.1,v1.2,v1.3` yields all
      four, an absent `api_proto` yields the default rather than a dropped
      advertisement, and `api_auth=true` marks the Node as requiring authorization
- [x] 6.3a Parse the version counters (`ver_slf`, `ver_dvc`, `ver_snd`,
      `ver_rcv`, `ver_flw`, `ver_src`) and emit a change event when a counter
      moves; verify an absent counter is reported absent and not as zero, and
      that a re-advertisement with an unchanged counter emits nothing
      (spec: `node-discovery`, version counters)
- [x] 6.4 Key Nodes two-stage: endpoint provisionally, `self.id` once fetched
      (design D8); verify one Node advertised on three interfaces appears once,
      a Node answering at two addresses with one identifier collapses to one row
      without flickering as two, and two Nodes on one host with different ports
      stay distinct
- [x] 6.4a Accept IPv4 and IPv6 advertisements, holding both as endpoints and
      preferring IPv4 for requests; verify an IPv6-only Node is discovered and
      used, and a dual-stack Node is addressed over IPv4
      (spec: `node-discovery`, both address families)
- [x] 6.5 Handle departure and re-appearance; verify a goodbye produces a depart
      event and a subsequent advertisement produces a fresh appear event
- [x] 6.6 Test malformed and hostile advertisements: no address, no port,
      oversized TXT, invalid UTF-8 in TXT; verify each is ignored, discovery
      keeps running, and nothing panics

## 7. Inventory (`jackfield-engine`)

- [x] 7.1 Model the inventory: Nodes keyed by identifier with a set of endpoints,
      each either fetched, still loading, or failed with a reason; verify state
      transitions with unit tests over a fabricated fetcher, with no network, and
      that a Node surviving on its second endpoint is not reported as failed
- [x] 7.2 Resolve references — Senders and Receivers to their Device, Senders to
      their Flow; verify a dangling `flow_id` yields an unresolved marker while
      the rest of the tree stays usable (spec: `node-inventory`, dangling
      reference)
- [x] 7.3 Fetch Nodes concurrently with a bound of eight, requests within a Node
      sequential, each with its own timeout; verify at most eight run at once
      with twenty Nodes discovered, and that one stalled Node delays no other
      (spec: `node-inventory`, fetching is bounded and isolated)
- [x] 7.4 Implement refresh replacing a Node's tree; verify a Sender removed at
      the Node disappears on refresh, and a previously failed Node becomes
      browsable when it starts answering
- [x] 7.4a Drive re-reads from the version counters: re-read only the collections
      whose counter moved, and leave an unchanged Node alone; verify a moved
      Receiver counter re-reads only Receivers, that an activated Sender becomes
      Transmitting with no operator action, and that an explicit refresh re-reads
      in full regardless of counters
- [x] 7.4b Run the transport pass separately — on counter change, and no less
      than once a minute per Node; verify a Node whose counters never move still
      converges within the interval, that a quiet network generates no
      per-resource traffic between passes, and that resources not yet covered
      report their transport parameters as pending rather than absent
      (spec: `node-inventory`, transport details read separately)
- [x] 7.4c Build the network-wide connection graph: pair Receivers to Senders by
      reported identifier, falling back to matching stream address and port;
      verify a Sender taken by three Receivers across two Nodes carries all
      three, and a Transmitting Sender with no takers carries an empty set and is
      never reported as connected
- [x] 7.4d Represent unresolvable pairings explicitly — unknown Sender, and
      ambiguous with candidates named; verify a Receiver naming an undiscovered
      Sender stays Subscribed rather than becoming Unsubscribed, that two Senders
      sharing a multicast address yield an ambiguous pairing with neither chosen,
      and that discovering the missing Node resolves the edge on both ends
      (spec: `node-inventory`, connections that cannot be resolved)
- [x] 7.5 Publish inventory snapshots on a channel and accept commands on
      another (design D4); verify a command to refresh a Node reaches the
      fetcher and a resulting snapshot reaches the subscriber
- [x] 7.6 Assert by test that nothing is written to disk: run a full discovery
      and fetch cycle in a temporary directory and verify no file was created or
      modified (spec: `node-inventory`, state held in memory only)

## 8. Terminal interface (`jackfield-tui`)

- [x] 8.1 Establish rendering tests against `ratatui`'s test backend, driven by a
      fabricated snapshot with no engine and no network; verify a snapshot
      renders deterministically into a buffer the test can assert on
- [x] 8.2 Render the left pane: the Node list with label and address, including
      the loading, failed, and departed presentations; verify each state is
      distinguishable in the rendered buffer and the empty-network case states
      itself in words (spec: `tui-browser`, main screen)
- [x] 8.3 Render the detail pane: Devices with their Senders and Receivers always
      grouped under the owning Device (design D8), Senders visually distinct from
      Receivers; verify a Node of three Devices renders unambiguous attribution,
      that grouping holds for a single-Device Node too, and that an empty Device
      still appears
- [x] 8.3a Show each Sender's Flow media type and each Receiver's accepted media
      types on its row; verify three Senders sharing the label `SDI 1` with
      `video/raw`, `audio/L24` and `video/smpte291` render distinguishably, and
      that an unresolved Flow renders as unknown rather than blank
      (spec: `tui-browser`, resources told apart by media type)
- [x] 8.4 Render connection state as Transmitting/Idle for Senders and
      Subscribed/Unsubscribed for Receivers, plus unknown; verify all states are
      distinguishable in a buffer with no colour applied, that no Sender is
      labelled "connected", and that a Transmitting Sender whose transport is not
      yet read shows its destination as pending rather than blank
      (spec: `tui-browser`, state does not rely on colour alone)
- [x] 8.4a Render a Sender's Receiver count and its expanded list, each entry
      naming the Node and Device; verify a Sender taken by three Receivers across
      two Nodes shows the count and expands correctly, and that a Transmitting
      Sender with no takers says so distinctly from one whose takers are not yet
      known (spec: `tui-browser`, a Sender shows who is taking its stream)
- [x] 8.4b Render unresolved pairings — subscribed to an unknown Sender, and
      ambiguous with candidates; verify neither renders as Unsubscribed and that
      resolution is visible once the missing Node appears
- [x] 8.4c Keep list order deterministic and stable under the selection; verify a
      Node appearing above the selected one leaves the selection on the same Node,
      and that a Node with an empty label falls back to hostname then address
- [x] 8.5 Implement the keyboard model — move, enter, back, refresh, quit — and
      the on-screen key hints; verify each binding against the test backend,
      including that back restores the previous selection
- [x] 8.6 Implement the terminal guard restoring raw mode, the alternate screen,
      and the cursor on `Drop`; verify restoration runs on normal exit and on
      panic unwind
- [x] 8.7 Handle layout edges: resize, labels longer than the pane, more Nodes
      than fit; verify truncation and scrolling keep the layout intact and the
      selected Node visible
- [x] 8.8 Verify by test that the crate depends on no HTTP or mDNS crate and
      performs no network call — the boundary from design D4 is enforced, not
      merely intended

## 9. Binary (`jackfield`)

- [x] 9.1 Wire discovery, inventory, and interface together over the channels;
      verify the binary starts against a fixture Node and renders it
- [x] 9.2 Add argument parsing and `tracing` logging to a file or stderr that
      does not corrupt the alternate screen; verify logs are readable after a run
      and the display is undisturbed during one
- [x] 9.3 Ensure the event loop waits on terminal input and engine snapshots
      together (design D5); verify a keystroke is served while a Node fetch is
      deliberately stalled (spec: `tui-browser`, quitting during a slow fetch)
- [x] 9.4 Add an end-to-end test: two fixture Nodes, one healthy and one
      refusing connections, driven through discovery to a rendered screen;
      verify the healthy Node is browsable and the failure is confined to its row

## 10. Verification

- [ ] 10.1 Run the full gate: `make fmt`, `make lint`, `make test`; verify no
      warnings and no failures
- [ ] 10.2 Run against the bench converter (Blackmagic 2110 IP Video Converter
      3x3G, three Devices of three Senders and three Receivers, advertised on
      three interfaces); verify the Node appears once, all three Devices are
      grouped correctly, the Senders sharing the label `SDI 1` are told apart by
      media type, and the active Senders match what the Connection API reports.
      Record any divergence as a regression test before fixing it (design:
      `mdns-sd` interoperability risk)
- [ ] 10.3 Confirm the read-only contract on the bench: no device changed state
      during the session; verify by reading each Sender's and Receiver's active
      state before and after and finding them identical
