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

The endpoints this change reads are settled and were confirmed against hardware
on the bench: `/x-nmos/node/v1.3` for the resource tree, and
`/x-nmos/connection/v1.1/single/{senders|receivers}/{id}/active` for connection
state.

## What the bench showed

A Blackmagic 2110 IP Video Converter 3x3G, observed at `10.77.1.90:8090`, settles
the shape of the interface more sharply than argument could:

- It advertises `_nmos-node._tcp` on three interfaces at once, each resolving to
  the same address and port — deduplication is not a hypothetical requirement.
- It offers `api_ver=v1.0,v1.1,v1.2,v1.3`, `api_proto=http`, `api_auth=false`.
- One Node hosts **three Devices** — `SDI 1`, `SDI 2`, `SDI 3`, one per physical
  port — with three Senders and three Receivers each: nine and nine.
- **Labels repeat.** All three Senders on `SDI 1` are labelled exactly `SDI 1`.
  They are told apart only by the media type of the Flow each carries:
  `video/raw`, `audio/L24`, `video/smpte291`. Receivers on the same box are
  labelled better (`SDI 1/Audio`), so label quality cannot be relied on either
  way.
- Receiver capabilities and Sender flows need not agree: these Receivers
  advertise `audio/L24` while two of the Senders emit `audio/L16`. Nothing in
  this read-only change acts on that, but it is a real mismatch a controller must
  face once it starts making connections.
- **IS-04 already carries connection state.** Each Sender and Receiver has a
  `subscription` object, and its `active` flag agrees exactly with IS-05's
  `master_enable`: the three `SDI 1` Senders report `{"active": true}`, the rest
  `false`. State therefore costs six requests per Node, not twenty-four.
- **But this device leaves the pairing empty.** `subscription.receiver_id` is
  `null` on an active Sender, and `subscription.sender_id` is `null` on every
  Receiver. The identifiers IS-04 provides for pairing are simply not populated,
  so who-feeds-whom cannot be answered from the resource tree on this hardware and
  must be recovered from the transport parameters in IS-05.
- Its advertisement carries the IS-04 peer-to-peer version counters —
  `ver_slf`, `ver_dvc`, `ver_snd=17`, `ver_rcv`, `ver_flw=4`, `ver_src` — which
  is the mechanism the specification provides for noticing change without
  polling.

## What the bench confirmed once it was built

The finished controller was run against the same Blackmagic converter, from a
host on the plant, on 2026-09-04. Everything above held:

- The Node appears **once**, though it advertises on three interfaces.
- Its three Devices are grouped correctly, each with three Senders and three
  Receivers.
- The three Senders labelled `SDI 1` are told apart by media type, exactly as
  the interface depends on.
- Only `SDI 1` is live, and its three Senders' destinations —
  `239.255.0.190:16384`, `239.255.1.190:16386`, `239.255.2.190:16388` — read
  from the Connection API match what the device reports directly.
- **IS-04 `active` agrees with IS-05 `master_enable` on all eighteen
  resources**, with no exceptions. This is the observation D9 rests on, and it
  now rests on the finished code rather than on a probe script.
- Each of those three Senders is transmitting with nothing taking its stream,
  which the interface says in those words.

The read-only contract was checked the way `tasks.md` asks: every Sender's and
Receiver's state was read before and after the session and found identical. No
device changed state.

One presentational consequence worth naming. An Idle Sender on this converter
still carries a configured `destination_ip`, but with `rtp_enabled` false; the
controller reports it as having no destination. That is deliberate — a Sender
with RTP off is not putting anything on that address, and treating the address
as live would let a Receiver be paired to a silent Sender — but it does mean the
screen does not show what an Idle Sender *would* send to.

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
browser. The obvious shortcut — shelling out to `avahi-browse` and parsing its
output — makes the controller depend on a system daemon, on that daemon's output
format, and on a subprocess round trip per scan. None of that survives contact
with a controller that must keep a live picture of the network.

Peer-to-peer discovery, with no IS-04 registry, matches how a small plant is
actually wired. A registry client is a later
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

### D8. A Node is what it says it is, not where it answers

Identity is the Node's own identifier, with endpoints as a set attached to it.
Before the identifier has been read there is nothing else to key on, so the
endpoint serves provisionally and is re-keyed on first successful fetch.

The tempting simplification — endpoint as identity — survives exactly as long as
every Node has one address. ST 2110 equipment is multi-homed by design: two media
NICs for ST 2022-7 seamless protection, often a separate management port. A
controller that lists such a box twice is worse than useless, because the operator
cannot tell which of the two rows is real.

### D9. Two tiers of fetching: state is cheap, transport is not

State comes from the resource tree: six requests per Node, and the whole tree is
re-read only for the collections whose version counters moved. Transport
parameters come from the Connection API at one request per Sender and per
Receiver — eighteen on the bench converter — so they are a second, slower pass,
triggered by the same counters and floored at once a minute per Node for Nodes
whose counters cannot be trusted.

The consequence is that connection state and the connection graph refresh on
different clocks, and the interface says which parts are pending rather than
pretending they agree. This is a deliberate trade: on a plant of a few dozen boxes
the eager alternative is hundreds of requests per cycle against equipment that is
on air.

The floor exists because the counter mechanism is only as good as the vendor's
implementation of it, and we have chosen not to verify it by writing to live
hardware.

### D10. The connection graph is the product, and its gaps are shown

Whether a Sender is transmitting and whether anyone is listening are different
questions, and only the second is what an operator means by "connected". The
engine holds every Node, so it is the only place the second question can be
answered: it pairs Receivers to Senders by reported identifier, and — because the
bench hardware reports none — by matching stream address and port.

Where a pairing cannot be made, the state is named rather than hidden. A Receiver
pointing at an undiscovered Sender says so, carrying the identifier. A stream
matching two Senders is reported as ambiguous with both named: two sources in one
multicast group is a fault, and a controller that quietly picks one has destroyed
the only evidence of it. Silently dropping an unresolved edge would recreate
exactly the lie this decision exists to remove — a Receiver that is subscribed
would render as though it were not.

### D11. Group by Device, and identify every resource by its media type

Senders and Receivers are always grouped under the Device that owns them, never
flattened into two lists per Node. The bench settles it: a single converter
presents three Devices, and the Device is the physical port an engineer is
actually thinking about. Flattening nine Senders into one list throws away the
only structure that means anything to the person at the rack.

The same observation forces a second rule. Because labels repeat — three Senders
all called `SDI 1` — a row identified by label alone is unusable. Every Sender row
SHALL therefore carry the media type of the Flow it sends, and every Receiver row
the media types it accepts, resolved by the engine rather than assembled in the
interface. This is why `node-inventory` resolves the Sender-to-Flow reference at
all: it is not bookkeeping, it is the only thing that tells two rows apart.

### D12. Decisions that outlive this change go to `docs/adr/`

Five of the above are architectural and will be questioned again: the schema
strategy (D1, D2), discovery without a registry (D3), the crate split (D4), the
connection vocabulary and graph (D10), and the two-tier fetch (D9). Each gets an
ADR, which is the durable record; this document is the record of *this change* and
will be archived with it. The project's vocabulary lives in `CONTEXT.md` at the
repository root, which is the canonical glossary.

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
- **Vendors do not maintain their version counters** → The specification requires
  a Node to increment a collection's counter when it changes; real equipment is
  uneven about it, and we have deliberately not verified it by writing to live
  hardware. The periodic floor on the transport pass converges regardless, at the
  cost of latency on such Nodes. Counter handling is tested against fabricated
  advertisements, not against the bench.
- **The graph is only as complete as discovery** → In peer-to-peer mode a Receiver
  fed from another network segment names a Sender we will never see. This produces
  an unresolved edge, which is honest but not satisfying; it resolves when a
  registry client arrives.
- **Pairing by stream address can be wrong** → Two Senders in one multicast group
  make the match ambiguous, and a Receiver could in principle be joined to a group
  it is not actually being fed by. Ambiguity is reported rather than resolved, so
  the failure mode is an operator seeing a question rather than a wrong answer.
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

None. The question this design once carried — whether to group Senders and
Receivers under their Device or to flatten them per Node — was settled on the
bench and is recorded as D11 above.
