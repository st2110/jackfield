## Purpose

The terminal application an engineer sits in front of: a list of the NMOS Nodes
found on the network, and, for the selected one, the Devices it hosts with their
Senders and Receivers and whether each is currently streaming.

## Requirements

### Requirement: The main screen lists discovered Nodes

The main screen SHALL present, in a pane on the left, every Node currently known
on the network, identified by the label the Node reports and by the address it
was reached at. A Node reporting no label SHALL be identified by its hostname,
and failing that by its address, rather than by a blank row. Nodes SHALL appear as
they are discovered, without the operator asking for a rescan.

The order of the list SHALL be deterministic and SHALL NOT change under the
operator's selection: a Node arriving, departing, or being re-read MUST NOT make
the highlighted row jump to a different Node.

#### Scenario: Nodes appear as they are found

- **WHEN** the application starts on a network with Nodes on it
- **THEN** each Node appears in the left pane as discovery finds it

#### Scenario: Empty network

- **WHEN** no Node has been discovered yet
- **THEN** the left pane says so plainly, rather than showing an empty frame with
  no explanation

#### Scenario: Node without a label

- **WHEN** a discovered Node reports an empty label
- **THEN** it is listed under its hostname, or its address if it has no hostname

#### Scenario: Selection survives the list changing

- **WHEN** a Node is selected and another Node appears above it in the order
- **THEN** the selection stays on the same Node, not on the same row position

#### Scenario: A Node that has not been fetched yet

- **WHEN** a Node has been discovered but its resource tree has not arrived
- **THEN** it is listed with its address and shown as still loading, rather than
  being hidden until it is complete

#### Scenario: A Node goes away

- **WHEN** a listed Node departs the network
- **THEN** the list stops presenting it as present, and the operator's selection
  is left somewhere sensible rather than pointing at nothing

### Requirement: Selecting a Node shows its Devices, Senders and Receivers

Entering a Node SHALL present the resources that Node hosts grouped by Device:
each Device, and under it the Senders and Receivers belonging to it. Senders and
Receivers SHALL be distinguishable from one another, and each SHALL carry the
label the Node reports for it. The grouping SHALL be kept whatever the number of
Devices — Senders and Receivers are never flattened into per-Node lists.

#### Scenario: Node with one Device

- **WHEN** the operator enters a Node hosting one Device with two Senders and two
  Receivers
- **THEN** the detail pane shows that Device with its two Senders and two
  Receivers under it

#### Scenario: One Device per physical port

- **WHEN** the operator enters a Node hosting three Devices of three Senders and
  three Receivers each
- **THEN** each Device is shown with its own three Senders and three Receivers,
  and no resource appears outside the Device that owns it

#### Scenario: Node with several Devices

- **WHEN** the operator enters a Node hosting more than one Device
- **THEN** each Device is shown with its own Senders and Receivers, and it is
  unambiguous which resource belongs to which Device

#### Scenario: Device with nothing on it

- **WHEN** a Device exposes neither Senders nor Receivers
- **THEN** the Device is still shown, marked as having no resources

### Requirement: Resources are told apart by media type, not by label alone

Node labels are not reliably unique: a Device commonly presents several Senders
carrying the same label, distinguished only by the media they carry. Every Sender
SHALL therefore be shown with the media type of the Flow it sends, and every
Receiver with the media types it accepts. Two resources on the same Device MUST
NOT be presented identically.

#### Scenario: Senders sharing a label

- **WHEN** a Device exposes three Senders all labelled `SDI 1`, carrying
  `video/raw`, `audio/L24` and `video/smpte291`
- **THEN** all three are distinguishable on screen, each showing its media type

#### Scenario: Receiver capabilities are shown

- **WHEN** a Receiver accepts `audio/L24`
- **THEN** that is shown on the Receiver's row

#### Scenario: Media type unavailable

- **WHEN** a Sender's Flow could not be resolved
- **THEN** the row says the media type is unknown rather than showing a blank
  that reads as "no media"

### Requirement: Every Sender and Receiver shows its own connection state

A Sender SHALL be shown as Transmitting or Idle, and a Receiver as Subscribed or
Unsubscribed. A Sender MUST NOT be described as connected: whether anything takes
its stream is a separate fact, shown separately. Where a state has not been read,
it SHALL be shown as unknown and MUST NOT be shown as Idle or Unsubscribed.

#### Scenario: Transmitting Sender

- **WHEN** a Sender is Transmitting
- **THEN** it is shown as Transmitting, with its destination once that has been
  read

#### Scenario: Unsubscribed Receiver

- **WHEN** a Receiver takes nothing
- **THEN** it is shown as Unsubscribed

#### Scenario: State not yet read

- **WHEN** a Node's connection state could not be read
- **THEN** its Senders and Receivers are shown as unknown, visibly distinct from
  Idle and Unsubscribed

#### Scenario: Destination not yet read

- **WHEN** a Sender is Transmitting but its transport parameters have not been
  read yet
- **THEN** the row shows it as Transmitting and its destination as pending,
  rather than showing a blank that reads as "nowhere"

#### Scenario: State does not rely on colour alone

- **WHEN** connection state is displayed
- **THEN** it is legible without colour, so the screen survives a monochrome
  terminal

### Requirement: A Sender shows who is taking its stream

Each Sender SHALL show how many Receivers take its stream, counting Receivers on
every Node the controller knows, and SHALL allow that set to be expanded to see
which ones, each named with its Node and Device. A Sender that is Transmitting
with no Receivers SHALL say so, because that is the state an operator most needs
to notice.

#### Scenario: Sender with several Receivers

- **WHEN** a Transmitting Sender is taken by three Receivers across two Nodes
- **THEN** its row shows a count of three, and expanding it lists all three with
  their Node and Device

#### Scenario: Transmitting into the void

- **WHEN** a Sender is Transmitting and no known Receiver takes its stream
- **THEN** the row says so plainly, distinct from a Sender whose Receivers are
  merely not yet known

#### Scenario: Receiver shows its Sender

- **WHEN** a Receiver is Subscribed to a known Sender
- **THEN** its row names that Sender with the Node and Device it lives on

### Requirement: Unresolved connections are shown, not hidden

Where a pairing could not be resolved, the interface SHALL say which case it is.
A Receiver subscribed to a Sender the controller has not discovered SHALL be shown
as such, carrying the identifier it reported. A pairing that matches more than one
Sender SHALL be shown as ambiguous, naming the candidates, since that indicates a
misconfigured plant the operator must fix.

#### Scenario: Subscribed to an unknown Sender

- **WHEN** a Receiver names a Sender belonging to no discovered Node
- **THEN** the row shows it as Subscribed to an unknown Sender with that
  identifier, not as Unsubscribed

#### Scenario: Ambiguous pairing

- **WHEN** a Receiver's stream matches two Senders
- **THEN** the row shows the pairing as ambiguous and names both candidates

#### Scenario: Resolution is visible when it arrives

- **WHEN** the Node hosting a previously unknown Sender is discovered
- **THEN** the row stops showing the pairing as unknown and names the Sender

### Requirement: The keyboard drives the application

The application SHALL be operable entirely from the keyboard: moving through the
Node list, entering a Node, returning to the list, refreshing what is shown, and
quitting. The available keys SHALL be visible on screen rather than requiring the
operator to know them.

#### Scenario: Moving and entering

- **WHEN** the operator moves the selection through the Node list and enters a
  Node
- **THEN** the detail view for the selected Node is shown

#### Scenario: Going back

- **WHEN** the operator asks to go back from a Node's detail view
- **THEN** the Node list is shown again with the same Node still selected

#### Scenario: Quitting restores the terminal

- **WHEN** the operator quits
- **THEN** the application exits and leaves the terminal in the state it found
  it: normal screen, cursor visible, echo restored

#### Scenario: Refreshing a Node

- **WHEN** the operator asks to refresh the selected Node
- **THEN** that Node's tree is fetched again and the screen shows the new state

### Requirement: The interface stays responsive while the network is slow

The interface SHALL remain responsive to the keyboard while discovery and fetching
are in progress. A Node that is slow or unreachable MUST NOT freeze the screen or
delay the operator's ability to navigate and quit.

#### Scenario: Quitting during a slow fetch

- **WHEN** a Node's fetch is in progress and not completing
- **THEN** the operator can still move around and quit immediately

#### Scenario: Fetches do not block each other

- **WHEN** one discovered Node is unreachable and another answers promptly
- **THEN** the responsive Node's tree appears without waiting for the unreachable
  one

### Requirement: Failures are shown to the operator, not hidden

When a Node cannot be discovered, reached, or understood, the interface SHALL say
so against that Node, in terms that point at what to fix. The application MUST NOT
present a failed Node as an empty but healthy one, and MUST NOT exit because a
Node failed.

#### Scenario: Unreachable Node

- **WHEN** a discovered Node refuses connections
- **THEN** it is listed with its failure shown, and the application keeps running

#### Scenario: Node speaking an unsupported version

- **WHEN** a Node offers only an unsupported API version
- **THEN** the interface says so and names the versions the Node offered

#### Scenario: Failure does not empty the screen

- **WHEN** one Node fails while others are healthy
- **THEN** the healthy Nodes remain browsable and the failure is confined to its
  own row

### Requirement: The interface renders correctly across terminal sizes

The interface SHALL adapt to the terminal it is given, including narrow windows
and windows resized while running. Content that does not fit SHALL be truncated or
scrolled rather than corrupting the layout.

#### Scenario: Terminal resized while running

- **WHEN** the terminal window is resized
- **THEN** the interface redraws to the new size without corrupting the display

#### Scenario: Labels longer than the pane

- **WHEN** a Node reports a label longer than the pane is wide
- **THEN** the label is truncated to fit and the layout holds

#### Scenario: More Nodes than fit on screen

- **WHEN** more Nodes are discovered than the pane can show at once
- **THEN** the list scrolls and the selected Node stays visible
