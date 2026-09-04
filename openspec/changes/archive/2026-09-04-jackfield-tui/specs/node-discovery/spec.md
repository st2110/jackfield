## Purpose

Finds NMOS Nodes on the local network without a registry, by listening to the
mDNS advertisements Nodes make about themselves, and reports when a Node appears,
changes, or goes away.

## ADDED Requirements

### Requirement: Nodes are discovered over mDNS

The system SHALL discover NMOS Nodes by browsing the local network for the
`_nmos-node._tcp` DNS-SD service type, and SHALL do so in-process over multicast
DNS. The system MUST NOT require an external mDNS browser process or a running
system mDNS daemon.

#### Scenario: A Node advertising itself is found

- **WHEN** a host on the local network advertises `_nmos-node._tcp` with a
  resolvable hostname, address, and port
- **THEN** the system reports a discovered Node carrying that address and port

#### Scenario: No responders on the network

- **WHEN** discovery runs on a network where nothing advertises `_nmos-node._tcp`
- **THEN** the system reports zero Nodes and remains running, ready to report a
  Node that appears later

#### Scenario: Discovery does not depend on a system daemon

- **WHEN** the host has no `avahi-daemon`, `avahi-browse`, or equivalent
  installed
- **THEN** discovery still works, because the system speaks mDNS itself

### Requirement: Advertised API metadata is captured

The system SHALL read the DNS-SD TXT records of each advertisement and SHALL
capture the advertised API protocol (`api_proto`), the advertised API versions
(`api_ver`), and whether the API requires authorization (`api_auth`). When a
TXT record is absent, the system SHALL apply the default defined by IS-04 rather
than discarding the advertisement.

#### Scenario: Version list is parsed

- **WHEN** a Node advertises `api_ver=v1.2,v1.3`
- **THEN** the discovered Node records both versions as available

#### Scenario: Advertisement without optional TXT records

- **WHEN** a Node advertises with no `api_proto` record
- **THEN** the discovered Node is still reported, with the protocol taken from
  the IS-04 default rather than being dropped

#### Scenario: Node requiring authorization is marked

- **WHEN** a Node advertises `api_auth=true`
- **THEN** the discovered Node is marked as requiring authorization, so that the
  rest of the system can report it as unreachable rather than failing obscurely

### Requirement: A Node is identified by the identifier it reports

A Node's identity SHALL be the identifier the Node reports for itself. Until that
identifier has been read, an advertised endpoint SHALL serve as a provisional
identity, because nothing better is known. Once the identifier is known, every
endpoint bearing it SHALL collapse into that one Node, whose endpoints are a set.

An endpoint MUST NOT be treated as an identity after the identifier is known: a
Node advertised at two addresses is one Node, and 2110 equipment is routinely
multi-homed.

#### Scenario: Same Node advertised on several interfaces

- **WHEN** one Node is advertised over three network interfaces resolving to the
  same address and port
- **THEN** the system reports a single Node with one endpoint

#### Scenario: Node advertised at two different addresses

- **WHEN** a Node advertises itself at two different addresses and both report
  the same identifier
- **THEN** the system reports one Node holding both endpoints, not two Nodes

#### Scenario: One host advertising two Nodes

- **WHEN** a single host advertises two Nodes on the same address but different
  ports, reporting different identifiers
- **THEN** the system reports two distinct Nodes

#### Scenario: Not yet identified

- **WHEN** a Node has been advertised but its identifier has not yet been read
- **THEN** it is reported under its endpoint, and is re-keyed to its identifier
  once that is known, without appearing twice in the meantime

#### Scenario: One endpoint of a Node stops answering

- **WHEN** a Node holding two endpoints stops answering at one of them
- **THEN** the Node remains reachable through the other, and is not reported as
  failed

### Requirement: Both address families are accepted

The system SHALL accept advertisements carrying IPv4 or IPv6 addresses and SHALL
keep both in the Node's endpoint set. Where a Node offers both, an IPv4 endpoint
SHALL be preferred for requests, IPv6 being untested against the equipment this
change targets.

#### Scenario: Node advertised over IPv6 only

- **WHEN** a Node advertises only an IPv6 address
- **THEN** it is discovered and its IPv6 endpoint is used

#### Scenario: Node advertised over both families

- **WHEN** a Node advertises both an IPv4 and an IPv6 address
- **THEN** both are held as endpoints and the IPv4 one is used for requests

### Requirement: Version counters are captured

The system SHALL read the per-collection version counters a Node publishes in its
advertisement and SHALL report their current values with the Node. A counter that
is absent SHALL be reported as absent rather than as zero, since zero is a
legitimate value that would falsely imply the collection had been observed.

#### Scenario: Counters are reported

- **WHEN** a Node advertises counters for its resource collections
- **THEN** each counter's value is reported with the discovered Node

#### Scenario: Counter changes

- **WHEN** a Node re-advertises with a counter holding a different value from the
  one previously seen
- **THEN** the system reports that this collection has changed

#### Scenario: Node publishing no counters

- **WHEN** a Node advertises without any version counters
- **THEN** the Node is discovered normally and its counters are reported as
  absent

### Requirement: Departure is observable

The system SHALL report when a previously discovered Node withdraws its
advertisement or its advertisement expires, so that a Node that has been powered
off stops being presented as present.

#### Scenario: Node withdraws its advertisement

- **WHEN** a discovered Node sends a goodbye for its `_nmos-node._tcp` service
- **THEN** the system reports that Node as departed

#### Scenario: Node returns after departing

- **WHEN** a Node that was reported as departed advertises itself again
- **THEN** the system reports it as discovered again

### Requirement: Discovery is continuous and non-blocking

Discovery SHALL run continuously for as long as the application runs, reporting
Nodes as they are found rather than only in response to an explicit scan, and
SHALL NOT block the rest of the application while waiting for responses.

#### Scenario: Node appears after startup

- **WHEN** a Node is powered on several minutes after the application started
- **THEN** the system reports it without the operator restarting or triggering a
  rescan

#### Scenario: Slow network does not stall the application

- **WHEN** mDNS responses are slow or absent
- **THEN** the rest of the application continues to run and remains responsive

### Requirement: Malformed advertisements are tolerated

The system SHALL ignore an advertisement it cannot make sense of — one missing an
address or port, or carrying an unparseable TXT record — and SHALL continue
discovering other Nodes. A single bad responder MUST NOT stop discovery.

#### Scenario: Advertisement without a resolvable address

- **WHEN** a responder advertises the service type but resolves to no address
- **THEN** the system ignores it and keeps reporting other Nodes

#### Scenario: Hostile TXT record

- **WHEN** an advertisement carries a TXT record with unexpected encoding or
  absurd length
- **THEN** the system rejects that advertisement without panicking or aborting
  discovery
