## Context

The repository is empty; there is no Rust yet, so this change also sets the shape
everything later grows into. See `proposal.md` for motivation and
`specs/` for the behaviour being contracted.

Two constraints come from outside and are not negotiable here. `AGENTS.md`
forbids panics in production code and requires tests before code, edge cases
beyond the happy path, and a regression test per bug. The repository is
Apache-2.0-licensed and mirrored publicly, so vendored third-party material
must carry its provenance and its own licence.

One terminology point, because the informal wording of the request and the
specification disagree. IS-04 defines a strict containment: a **Node** is one
logical host, and it holds **Devices**; a Device holds **Senders** and
**Receivers**; a Sender carries a **Flow**, which originates from a **Source**.
What mDNS advertises as `_nmos-node._tcp` is a Node, not a Device. This design
and the specs use the specification's meanings throughout — "device" is never
used loosely for "the box on the network".

The working reference for behaviour is `nmosctl.py` at the repository root: 182
lines of stdlib Python that shell out to `avahi-browse`, read
`/x-nmos/node/v1.3`, and read `/x-nmos/connection/v1.1/single`. It is the proof
the endpoints and the flow work; it is not a structure worth porting.

## Goals / Non-Goals

**Goals:**

- A crate split where the domain does not know that a terminal exists, so the
  engine is testable without a screen and a second front end is possible later.
- A resource model that reads like Rust rather than like generated JSON Schema,
  while still being provably faithful to the published contract.
- Per-Node failure isolation: one dead box on the network degrades one row.
- An interface that never blocks on the network.

**Non-Goals:**

- Any write to a device. No IS-05 `staged` PATCH, no activation, no SDP handling
  beyond noting that a transport file exists.
- A registry client (IS-04 Registration and Query APIs).
- Abstracting over transports or API versions beyond what is needed to read the
  two versions named below. Premature generality here would cost more than the
  duplication it saves.

## Decisions

### D1. Hand-written Rust types, with the AMWA schemas as test-time validators

The published AMWA schemas are JSON Schema **draft-04**, built out of `allOf`
composition over `resource_core.json`, with polymorphism expressed structurally
(`flow_video_raw`, `flow_audio_coded`, `receiver_mux`, and so on). Rust type
generators target draft-07 and 2020-12; draft-04 `allOf` inheritance either fails
outright or produces flattened types with stuttering names and no enums where the
domain plainly has enums.

So the types are written by hand — small, named after the domain, with `Flow` and
`Receiver` polymorphism expressed as real Rust enums — and the schemas are used
where they genuinely pay: as validators in the test suite. The `jsonschema` crate
supports draft-04 natively, so the published schemas can be applied verbatim,
with no conversion step and no drift between what we validate against and what
AMWA published.

This gives both properties that matter: the model is pleasant to use, and a
divergence from the specification fails `cargo test` rather than a live plant.
Two directions are checked — every published example must parse into our types,
and everything our types serialize must validate against the schema.

*Alternatives considered.* Generating with `typify` after converting draft-04 to
2020-12: adds a conversion step and a generator to the build, and yields types we
would end up wrapping by hand anyway. An existing NMOS crate from crates.io: none
was found covering the controller side of IS-04 plus IS-05, and taking one would
trade a known cost for an unknown maintenance risk on the load-bearing layer.

### D2. Schemas are vendored at pinned versions, not fetched

The schema files are copied into the repository from `AMWA-TV/is-04` at `v1.3.x`
and `AMWA-TV/is-05` at `v1.1.x`, kept byte-for-byte, with their upstream commit
recorded alongside them and their Apache-2.0 licence included. Tests must not
reach the network, builds must be reproducible offline, and a public mirror must
be able to show exactly which revision of the contract the code was checked
against.

### D3. mDNS is spoken in-process; no registry

Discovery browses `_nmos-node._tcp` using `mdns-sd`, a pure-Rust responder and
browser. `nmosctl.py` shells out to `avahi-browse`, which makes it depend on a
system daemon, on that daemon's output format, and on a `subprocess` round trip
per scan. None of that survives contact with a controller that must keep a live
picture of the network.

Peer-to-peer discovery, with no IS-04 registry, matches how the reference script
works and how a small plant is actually wired. A registry client is a later
change; nothing here forecloses it, because discovery is expressed as a stream of
appear and depart events, and a registry is simply another producer of those.

### D4. Four crates; the engine owns the truth, the interface owns the screen

- `jackfield-nmos` — the resource model and the IS-04 and IS-05 HTTP clients.
  Knows the protocol; knows nothing about state or discovery.
- `jackfield-engine` — discovery, the in-memory inventory, and the fetching that
  fills it. Knows the domain; knows nothing about a terminal.
- `jackfield-tui` — `ratatui` and `crossterm`. Renders a snapshot and turns
  keystrokes into commands; contains no protocol knowledge and no network calls.
- `jackfield` — the binary: argument parsing, logging, wiring the three together.

The seam between engine and interface is a pair of channels: the engine publishes
inventory snapshots, the interface sends commands (refresh this Node, and later,
connect this pair). The interface never calls the network and never holds the
authoritative copy of anything, which is what makes the engine testable headless
and the interface testable against a fabricated snapshot.

*Alternative considered.* A single crate with modules. Rejected because the
compiler stops enforcing the boundary the moment it is convenient to cross it,
and the boundary is the point.

### D5. Async everywhere, with the terminal driven from the same runtime

`tokio` runs discovery and the per-Node fetches. Terminal input is read on a
blocking thread and forwarded into the runtime as events, which is the shape
`crossterm` supports without fighting it. The interface loop then waits on
exactly two things — a terminal event or an engine snapshot — so a slow network
cannot stall a keystroke and a keystroke cannot stall a fetch.

Nodes are fetched concurrently, one task per Node, each with its own timeout and
its own result. This is what makes per-Node failure isolation fall out of the
structure rather than needing to be defended by hand.

### D6. Version negotiation over the small common subset

The Node advertises the API versions it speaks in its `api_ver` TXT record. The
system picks the highest it understands from `v1.3` then `v1.2` for IS-04, and
`v1.1` then `v1.0` for IS-05. The fields this change actually reads — labels,
identifiers, containment, `master_enable`, transport parameters — are unchanged
across those pairs, so one set of types serves both and the negotiation is a URL
prefix rather than a second model. A Node offering neither is surfaced as
unsupported, naming what it offered.

Supporting only `v1.3` would have been simpler and would have made the tool
useless on the many deployed devices that speak `v1.2`.

### D7. HTTP, and the shape of failure

`reqwest` with `rustls`, with a per-request timeout and a connect timeout. Only
`http` is exercised by this change; TLS is compiled in rather than retrofitted
later, since `api_proto=https` is a Node's choice, not ours. A Node advertising
`api_auth=true` is reported as unsupported rather than attempted, because IS-10
authorization is out of scope and a 401 is a worse diagnostic than an honest
refusal.

Errors are `thiserror` enums in the library crates and `anyhow` at the binary,
per the house rule. A Node failure is a value stored against that Node in the
inventory — not a log line, not a lost row — because the specs require the
operator to see it on screen.

### D8. Decisions that outlive this change go to `docs/adr/`

Three of the above are architectural and will be questioned again: the schema
strategy (D1, D2), discovery without a registry (D3), and the crate split (D4).
Each gets an ADR, which is the durable record; this document is the record of
*this change* and will be archived with it.

## Risks / Trade-offs

- **Hand-written types drift from the specification** → The schema validation
  tests are the mitigation, and they are the reason the schemas are vendored at
  all. A field we never model is a field the round-trip test never exercises, so
  the coverage is bounded by what we consume — accepted, and the reason the
  parsers ignore unknown fields rather than rejecting them.
- **`mdns-sd` behaves differently from `avahi` on a real plant** → mDNS is where
  interoperability goes wrong in practice, across switches doing IGMP snooping
  and devices with idiosyncratic responders. The discovery layer is kept behind
  an interface the tests can drive with fabricated advertisements, and the real
  check is a bench run against actual hardware before this is called done.
- **Concurrent fetches against a small device flood it** → Per-Node concurrency
  is bounded and requests within a Node are sequential; a device answering one
  request at a time is the norm, not the exception.
- **Reading `/active` per Sender and Receiver is N+1 requests** → Acceptable at
  the scale of one plant, and honest: IS-05 has no bulk read of active state. If
  it hurts, the fix is caching against the Node's version counters, which is a
  later change and not worth pre-building.
- **Terminal state left broken if the process dies badly** → Restoration is tied
  to a guard whose `Drop` runs on normal exit and on panic unwind. A hard abort
  can still leave a terminal in raw mode; the house rule against panics in
  production code is what keeps that path closed.
- **No persistence means a restart is a cold start** → Deliberate, per the
  proposal. On a plant of realistic size, rediscovery costs seconds.

## Migration Plan

Not applicable: there is no existing system, no data to migrate, and nothing
deployed. The change is additive to an empty repository, and rollback is
reverting the commits.

## Open Questions

- Whether Senders and Receivers should be grouped under their Device on screen or
  presented as two flat lists per Node, when a Node hosts exactly one Device —
  which is the common case and where the extra level of nesting may cost more
  than it explains. The specs require only that the attribution be unambiguous;
  both satisfy them, and the answer is better taken from looking at the built
  screen against real hardware than from arguing it now.
