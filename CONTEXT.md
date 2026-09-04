# Jackfield

An NMOS controller for SMPTE ST 2110 networks. It finds the equipment on a
broadcast plant's network, shows what each box exposes and what is currently
streaming where, and patches senders to receivers.

The vocabulary below is the AMWA NMOS one. Where an operator's everyday phrasing
disagrees with the specification, the specification wins: this project talks to
devices from many vendors, and their own interfaces use these words.

## Language

### The resource model

**Node**:
One logical host on the network — the physical box, as it announces itself over
mDNS. Holds Devices.
_Avoid_: device, box, unit, machine, endpoint

**Device**:
A grouping inside a Node that owns Senders and Receivers. Typically one physical
port: a three-port converter presents three Devices.
_Avoid_: node, port, channel

**Sender**:
An egress from a Device: it carries a Flow onto the network. From the plant's
point of view this is a source of signal, but never call it a Source — that word
is taken.
_Avoid_: output, source, transmitter

**Receiver**:
An ingress to a Device: it consumes a stream from the network.
_Avoid_: input, destination, sink

**Flow**:
The media a Sender carries — video raw or coded, audio, data, SDI ancillary data,
mux. Its media type is what tells two identically labelled Senders apart.

**Source**:
The logical origin of a Flow, upstream of the Flow itself. Not a synonym for
Sender.

**Endpoint**:
One network address and port at which a Node's API can be reached. A Node may
have several; they are an attribute of the Node, not its identity.

### Connection

The word *connected* describes a pair — a Sender and the Receivers taking its
stream. Neither end is "connected" on its own, so each end has its own word.

**Transmitting**:
A Sender is putting its Flow on the network. Says nothing about whether anyone
is listening: a Sender can transmit into a multicast group no Receiver has
joined.
_Avoid_: active, enabled, connected, on

**Idle**:
A Sender that is not transmitting.
_Avoid_: inactive, disabled, off

**Subscribed**:
A Receiver is taking a stream, and which Sender it takes is known.
_Avoid_: active, connected

**Unsubscribed**:
A Receiver that is taking nothing.
_Avoid_: inactive, disconnected

**Unresolved edge**:
A connection the controller can see but cannot pin down: a Receiver naming a
Sender that has not been discovered, or a stream whose address matches more than
one Sender. Shown as such, never guessed at and never silently dropped.

**Pending**:
A fact the controller has not read yet but expects to — most often a
Transmitting Sender's destination, which arrives on the slower of the two fetch
clocks. Distinct from absent: a Sender whose destination is pending is not a
Sender going nowhere.
_Avoid_: none, empty, blank

**Unknown state**:
A Sender's or Receiver's connection state that has not been read yet, or could
not be read. Distinct from Idle and from Unsubscribed, which are positive
observations.

### Discovery

**Discovered**:
A Node whose mDNS advertisement has been seen. It has at least one Endpoint, and
nothing else is known about it until its resource tree is fetched.

**Departed**:
A Node that has withdrawn its advertisement or whose advertisement expired.
_Avoid_: offline, dead, lost

**Version counter**:
The per-collection counter a Node publishes in its mDNS advertisement,
incremented whenever that collection changes. What tells the controller to
re-read a collection without polling it.
