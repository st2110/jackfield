## Purpose

Turns a discovered network address into the controller's picture of what is out
there: a Node, the Devices it hosts, the Senders and Receivers those Devices
expose, and whether each of them is currently active.

## ADDED Requirements

### Requirement: A Node's resource tree is fetched over IS-04

For each discovered Node, the system SHALL fetch the Node API resource
collections — the Node's own record, its Devices, Senders, Receivers, Flows, and
Sources — and SHALL present them as a single coherent picture of that Node.

#### Scenario: Full tree is read

- **WHEN** inventory runs against a Node exposing one Device, two Senders, and
  two Receivers
- **THEN** the inventory holds that Node, its Device, and all four resources

#### Scenario: Node exposing nothing

- **WHEN** a Node exposes no Devices at all
- **THEN** the inventory holds the Node with an empty Device list, and this is
  not treated as an error

### Requirement: Resource relationships are resolved

The system SHALL resolve the references between resources so that each Sender and
Receiver is attributed to the Device that owns it, and each Sender is associated
with the Flow it carries. A reference that points at a resource the Node did not
return SHALL be reported as unresolved rather than silently dropped or treated as
a failure of the whole Node.

#### Scenario: Sender attributed to its Device

- **WHEN** a Sender declares a `device_id` matching a Device the Node returned
- **THEN** the inventory presents that Sender under that Device

#### Scenario: Dangling reference

- **WHEN** a Sender declares a `flow_id` that appears in no Flow the Node
  returned
- **THEN** the inventory records the Sender with its Flow unresolved, and the
  rest of the Node's tree is still usable

### Requirement: Connection state comes from the resource tree

A Sender SHALL be reported as Transmitting or Idle, and a Receiver as Subscribed
or Unsubscribed, from the subscription each resource carries in the Node API.
Reading connection state MUST NOT require a request per resource, because the
resource tree already carries it.

Transmitting says only that the Sender is putting its Flow on the network. It
MUST NOT be presented as meaning that anything is receiving it.

#### Scenario: Transmitting Sender

- **WHEN** a Sender's resource reports its subscription as active
- **THEN** the inventory reports that Sender as Transmitting

#### Scenario: Idle Sender

- **WHEN** a Sender's resource reports its subscription as inactive
- **THEN** the inventory reports that Sender as Idle

#### Scenario: Unsubscribed Receiver

- **WHEN** a Receiver's resource reports its subscription as inactive
- **THEN** the inventory reports that Receiver as Unsubscribed

#### Scenario: State is available without the Connection API

- **WHEN** a Node's resource tree has been read but its Connection API has not
- **THEN** every Sender and Receiver already carries Transmitting, Idle,
  Subscribed or Unsubscribed

### Requirement: Transport details are read separately from state

The transport parameters of a Sender and Receiver — the addresses and ports a
stream actually uses — SHALL be read from the Connection API in a pass separate
from the resource tree, since they cost one request per resource. That pass SHALL
run when a Node's Sender or Receiver version counter changes, and SHALL in any
case run no less often than once a minute per Node, so that a Node which does not
maintain its counters still converges.

Until that pass has run for a resource, its transport parameters SHALL be
reported as not yet read — distinct from a resource that has none.

#### Scenario: Counter change triggers the pass

- **WHEN** a Node's Sender version counter changes
- **THEN** the transport parameters of that Node's Senders are read again

#### Scenario: Node that never updates its counters

- **WHEN** a Node's connection state changes but its counters never move
- **THEN** the periodic pass still picks the change up within its interval

#### Scenario: A quiet network is left alone

- **WHEN** no counter changes for several minutes
- **THEN** the only Connection API traffic is the periodic pass, not a request
  per resource per cycle

#### Scenario: Transport parameters not yet read

- **WHEN** a Node's resource tree is known but its transport pass has not run
- **THEN** its Senders and Receivers report their transport parameters as not yet
  read, and their Transmitting or Subscribed state is still shown

#### Scenario: Node without a reachable Connection API

- **WHEN** a Node exposes the Node API but its Connection API cannot be reached
- **THEN** transport parameters are reported as unknown, connection state from
  the resource tree remains valid, and the Node stays usable

### Requirement: Connections are resolved across the whole network

The system SHALL derive, over every Node it knows, which Senders feed which
Receivers. A pairing SHALL be established from the identifier a Receiver reports
for its Sender; where the Receiver reports none — as happens when it was
connected by transport file — the pairing SHALL be established by matching the
stream's address and port against known Senders.

A Sender SHALL carry the set of Receivers taking its stream, which may be empty
even while it is Transmitting, and may hold Receivers on other Nodes.

#### Scenario: Receiver naming its Sender

- **WHEN** a Subscribed Receiver reports the identifier of a Sender on another
  Node that the system knows
- **THEN** the pairing is recorded on both the Receiver and that Sender

#### Scenario: Pairing by stream address

- **WHEN** a Subscribed Receiver reports no Sender identifier, and its stream
  address and port match exactly one known Sender
- **THEN** the pairing is recorded

#### Scenario: Transmitting into the void

- **WHEN** a Sender is Transmitting and no known Receiver takes its stream
- **THEN** it is reported as Transmitting with an empty set of Receivers, and is
  never reported as connected

#### Scenario: One Sender feeding several Receivers

- **WHEN** three Receivers across two Nodes take one Sender's stream
- **THEN** that Sender carries all three, each attributed to its own Node

### Requirement: Connections that cannot be resolved are reported as such

Where a pairing cannot be established, the system SHALL report why, and MUST NOT
guess. A Receiver naming a Sender the system has not discovered SHALL be reported
as subscribed to an unknown Sender, naming the identifier it reported. A stream
address matching more than one Sender SHALL be reported as ambiguous, naming the
candidates — two Senders sharing a multicast address is a fault in the plant, and
the controller exists to surface faults, not to pick a plausible one.

#### Scenario: Sender not discovered

- **WHEN** a Subscribed Receiver names a Sender identifier belonging to no known
  Node
- **THEN** it is reported as subscribed to an unknown Sender, carrying that
  identifier

#### Scenario: Ambiguous stream address

- **WHEN** a Receiver's stream address and port match two known Senders
- **THEN** the pairing is reported as ambiguous with both candidates named, and
  neither is recorded as the pairing

#### Scenario: An unresolved pairing is not silence

- **WHEN** a pairing cannot be resolved
- **THEN** the Receiver is still reported as Subscribed, never as Unsubscribed

#### Scenario: Resolution recovers

- **WHEN** the Node hosting a previously unknown Sender is discovered and fetched
- **THEN** the pairing resolves, on both ends, without the operator intervening

### Requirement: State is held in memory only

The inventory SHALL exist only for the lifetime of the process. The system MUST
NOT write any part of it to disk — no cache, no session file, no configuration
written on exit — and MUST NOT read a previous run's state on startup.

#### Scenario: Nothing is written

- **WHEN** the application discovers Nodes, builds an inventory, and exits
- **THEN** no file has been created or modified anywhere on the filesystem

#### Scenario: Every run starts empty

- **WHEN** the application starts
- **THEN** the inventory is empty until discovery and fetching populate it

### Requirement: An unreachable Node does not poison the inventory

The system SHALL record a per-Node failure — refused connection, timeout,
malformed response, HTTP error status — against that Node alone, keeping the
reason available for display. Every other Node's inventory SHALL remain intact
and usable.

#### Scenario: One Node times out

- **WHEN** one of three discovered Nodes stops answering and its requests time
  out
- **THEN** that Node is marked as failed with the reason recorded, and the other
  two Nodes remain fully browsable

#### Scenario: Node answers with an HTTP error

- **WHEN** a Node answers the Node API with status 500
- **THEN** the failure and its status are recorded against that Node, and no
  other Node is affected

#### Scenario: Node answers with something that is not NMOS

- **WHEN** a Node's API port answers with HTML instead of the expected resources
- **THEN** the Node is marked as failed with a reason saying the response could
  not be understood, and the application keeps running

### Requirement: Responses are validated against the published schemas

The Rust representation of every NMOS resource the system consumes SHALL be
verified against the official AMWA JSON Schemas for the supported API versions.
This verification SHALL run as part of the test suite, over the specification's
own examples as well as over locally held samples, so that a divergence between
the model and the published contract fails the build rather than a live plant.

#### Scenario: Model accepts the specification's examples

- **WHEN** the test suite parses every published example of a resource the system
  consumes
- **THEN** each example parses successfully into the corresponding type

#### Scenario: Serialized form satisfies the schema

- **WHEN** the test suite serializes a resource the system has parsed
- **THEN** the result validates against that resource's published JSON Schema

### Requirement: Unknown fields and versions do not break parsing

The system SHALL accept resources carrying fields it does not know about, and
SHALL support every IS-04 version it claims to support. Encountering a Node that
speaks only a version the system does not support SHALL be reported as an
unsupported Node, not as a crash or a silent omission.

#### Scenario: Vendor extension field

- **WHEN** a Node returns a Sender carrying an additional vendor-specific field
- **THEN** the Sender parses successfully and the unknown field is ignored

#### Scenario: Unsupported API version

- **WHEN** a discovered Node advertises only an IS-04 version the system does not
  support
- **THEN** the Node is presented as unsupported, naming the versions it offered

### Requirement: The inventory follows the network without being asked

The system SHALL re-read a Node's collection when that collection's version
counter changes, so that what is held converges on what is there without the
operator asking. An explicit refresh SHALL also be available, re-reading a Node
in full regardless of its counters.

Only the collections whose counters moved SHALL be re-read; an unchanged Node
MUST NOT be re-fetched merely because time passed, beyond the periodic transport
pass.

#### Scenario: Sender activated at the Node

- **WHEN** a Sender is activated at a Node and its Sender counter changes
- **THEN** the inventory reports that Sender as Transmitting without the operator
  refreshing anything

#### Scenario: Only the changed collection is re-read

- **WHEN** a Node's Receiver counter changes and its other counters do not
- **THEN** only its Receivers are re-read

#### Scenario: Sender removed at the Node

- **WHEN** a Node that previously exposed two Senders now exposes one
- **THEN** the inventory holds one Sender for that Node

#### Scenario: Explicit refresh ignores counters

- **WHEN** the operator refreshes a Node whose counters have not moved
- **THEN** the Node is re-read in full anyway

#### Scenario: Refresh of a failed Node succeeds

- **WHEN** a Node previously marked as failed answers correctly on refresh
- **THEN** its failure is cleared and its tree becomes browsable

### Requirement: Fetching is bounded and isolated

The system SHALL bound how many Nodes it fetches at once, and SHALL keep requests
to a single Node sequential, so that discovering a large plant does not flood it.
A Node's slowness or failure MUST NOT delay another Node's fetch.

#### Scenario: More Nodes than the bound

- **WHEN** twenty Nodes are discovered at once and the bound is eight
- **THEN** at most eight are fetched concurrently and the rest follow as those
  complete

#### Scenario: One Node stalls

- **WHEN** one Node accepts connections but never answers
- **THEN** the other Nodes complete their fetches within their own timeouts
