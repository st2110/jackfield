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

### Requirement: Connection state is read from IS-05

For each Sender and Receiver, the system SHALL read its active connection state
from the Connection API and SHALL derive from it whether the resource is active,
and where it is sending to or receiving from.

#### Scenario: Active Sender

- **WHEN** a Sender's active state reports the master enable set, with a
  destination address and port
- **THEN** the inventory presents that Sender as active, carrying its destination

#### Scenario: Inactive Receiver

- **WHEN** a Receiver's active state reports the master enable clear
- **THEN** the inventory presents that Receiver as inactive

#### Scenario: Receiver subscribed to a Sender

- **WHEN** an active Receiver's state names the Sender it is subscribed to
- **THEN** the inventory records that subscription, so the pairing can be shown

#### Scenario: Node without a Connection API

- **WHEN** a Node exposes IS-04 but no reachable Connection API
- **THEN** the inventory presents its Senders and Receivers with their connection
  state reported as unknown, and the Node remains usable

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

### Requirement: The inventory can be refreshed

The system SHALL be able to re-fetch a Node's tree on demand and replace what it
holds for that Node with the newly observed state, including resources that have
appeared or disappeared since the previous fetch.

#### Scenario: Sender removed at the Node

- **WHEN** a Node that previously exposed two Senders now exposes one, and the
  inventory is refreshed
- **THEN** the inventory holds one Sender for that Node

#### Scenario: Refresh of a failed Node succeeds

- **WHEN** a Node previously marked as failed answers correctly on refresh
- **THEN** its failure is cleared and its tree becomes browsable
