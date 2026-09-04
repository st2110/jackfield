## Purpose

The terminal application an engineer sits in front of: a list of the NMOS Nodes
found on the network, and, for the selected one, the Devices it hosts with their
Senders and Receivers and whether each is currently streaming.

## ADDED Requirements

### Requirement: The main screen lists discovered Nodes

The main screen SHALL present, in a pane on the left, every Node currently known
on the network, identified by the label the Node reports and by the address it
was reached at. Nodes SHALL appear as they are discovered, without the operator
asking for a rescan.

#### Scenario: Nodes appear as they are found

- **WHEN** the application starts on a network with Nodes on it
- **THEN** each Node appears in the left pane as discovery finds it

#### Scenario: Empty network

- **WHEN** no Node has been discovered yet
- **THEN** the left pane says so plainly, rather than showing an empty frame with
  no explanation

#### Scenario: A Node that has not been fetched yet

- **WHEN** a Node has been discovered but its resource tree has not arrived
- **THEN** it is listed with its address and shown as still loading, rather than
  being hidden until it is complete

#### Scenario: A Node goes away

- **WHEN** a listed Node departs the network
- **THEN** the list stops presenting it as present, and the operator's selection
  is left somewhere sensible rather than pointing at nothing

### Requirement: Selecting a Node shows its Devices, Senders and Receivers

Entering a Node SHALL present the resources that Node hosts: its Devices, and
under each Device the Senders and Receivers belonging to it. Senders and
Receivers SHALL be distinguishable from one another, and each SHALL carry the
label the Node reports for it.

#### Scenario: Node with one Device

- **WHEN** the operator enters a Node hosting one Device with two Senders and two
  Receivers
- **THEN** the detail pane shows that Device with its two Senders and two
  Receivers under it

#### Scenario: Node with several Devices

- **WHEN** the operator enters a Node hosting more than one Device
- **THEN** each Device is shown with its own Senders and Receivers, and it is
  unambiguous which resource belongs to which Device

#### Scenario: Device with nothing on it

- **WHEN** a Device exposes neither Senders nor Receivers
- **THEN** the Device is still shown, marked as having no resources

### Requirement: Every Sender and Receiver shows whether it is connected

Each Sender and Receiver SHALL show its current connection state: active or
inactive. Where the state is not known — because the Node's Connection API could
not be read — the resource SHALL be shown as unknown, and MUST NOT be presented
as inactive.

#### Scenario: Active Sender

- **WHEN** a Sender is active
- **THEN** it is shown as active, together with where it is sending

#### Scenario: Inactive Receiver

- **WHEN** a Receiver is not active
- **THEN** it is shown as inactive

#### Scenario: Unreadable connection state

- **WHEN** the Connection API for a Node could not be read
- **THEN** its Senders and Receivers are shown as unknown, visibly distinct from
  inactive

#### Scenario: State does not rely on colour alone

- **WHEN** connection state is displayed
- **THEN** it is legible without colour, so the screen survives a monochrome
  terminal

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
