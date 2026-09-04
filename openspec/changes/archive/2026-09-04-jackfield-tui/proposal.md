# jackfield-tui

## Why

Operating an ST 2110 plant today means either a vendor controller — expensive,
appliance-bound, opaque — or a handful of `curl` calls against each box in turn,
printing flat text and holding no state between invocations. Neither lets an
engineer sit in front of a rack and answer the first question that matters:
*what is on this network, and what is it connected to?*

Jackfield answers that question in a terminal. This first change builds the
foundation everything else needs — find the Nodes, read their resource tree, show
it — without yet touching a single device's configuration.

## What Changes

- New Cargo workspace with a crate split that keeps the domain independent of the
  screen: an engine crate owning discovery and in-memory state, a `ratatui` crate
  owning presentation only, a shared NMOS model crate, and a thin binary.
- mDNS discovery of `_nmos-node._tcp` advertisements, in-process via `mdns-sd`.
  No `avahi-browse` subprocess, no dependency on a running `avahi-daemon`.
- An IS-04 Node API client that walks a discovered Node's resource tree: `self`,
  `devices`, `senders`, `receivers`, `flows`, `sources`.
- An IS-05 Connection API client that reads `/active` for each Sender and
  Receiver, so the UI can show whether a stream is running and where it points.
  **Read-only in this change** — no `staged` PATCH, no activation.
- A two-pane terminal UI: discovered Nodes on the left, the selected Node's
  Devices with their Senders and Receivers on the right, each carrying its
  active/inactive state.
- Hand-written Rust types for the NMOS resource model, with the official AMWA
  JSON Schemas vendored into the repository and used as **test-time validators**
  rather than as input to a code generator. See `docs/adr/`.
- All state lives in memory for the lifetime of the process. Nothing is written
  to disk — no cache, no config file, no session.

**Non-goals** (deliberately out of this change):

- Making or breaking connections (IS-05 `staged` PATCH, `activate_immediate`,
  SDP transport files). The point of the program is to start 2110 broadcast; this
  change stops one step short of it, at read-only visibility.
- IS-04 Registry and Query API. Discovery here is peer-to-peer over mDNS.
- Persistence, authentication (IS-10), HTTPS, and IS-07/IS-08.
- Any use of `sapsan/crates/nmos`, which implements the Node side, not the
  controller side.

## Capabilities

### New Capabilities

- `node-discovery`: finding NMOS Nodes on the local network over mDNS, tracking
  their appearance and disappearance, and exposing them as discovery events.
- `node-inventory`: fetching a Node's IS-04 resource tree and its IS-05 active
  connection state, and holding the result as the in-memory model of what the
  controller currently knows about the network.
- `tui-browser`: the terminal application — a Node list, a Node detail view of
  Devices with their Senders and Receivers, connection state per resource, and
  the keyboard model for moving between them.

### Modified Capabilities

None — this is the first change in an empty repository.

## Impact

- **New code**: a Cargo workspace where there is currently no Rust at all.
  Crates: `jackfield-nmos` (model and HTTP clients), `jackfield-engine`
  (discovery, state), `jackfield-tui` (ratatui), `jackfield` (binary).
- **New dependencies**: `mdns-sd`, `reqwest`, `serde`/`serde_json`, `tokio`,
  `ratatui`, `crossterm`, `thiserror`, `anyhow`, `uuid`, `tracing`; for tests
  `jsonschema`, `proptest`, `wiremock`, `insta`.
- **Vendored third-party files**: AMWA JSON Schemas from `AMWA-TV/is-04`
  (`v1.3.x`) and `AMWA-TV/is-05` (`v1.1.x`), under their Apache-2.0 licence,
  kept verbatim with provenance recorded.
- **External systems**: reads from NMOS Nodes over plain HTTP on the local
  network; joins the mDNS multicast group. Writes to nothing.
- **Documentation**: `docs/adr/` gains the technical decisions behind schema
  handling, discovery transport, and the crate split.
