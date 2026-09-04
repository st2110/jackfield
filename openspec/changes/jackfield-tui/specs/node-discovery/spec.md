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

### Requirement: A Node is identified by its API endpoint

The system SHALL treat the combination of network address and port as the
identity of a discovered Node, and SHALL report each such endpoint at most once
regardless of how many interfaces or address families it is advertised on.

#### Scenario: Same Node advertised on several interfaces

- **WHEN** one Node is advertised over two network interfaces with the same
  address and port
- **THEN** the system reports it as a single Node, not two

#### Scenario: One host advertising two Nodes

- **WHEN** a single host advertises two Nodes on the same address but different
  ports
- **THEN** the system reports two distinct Nodes

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
