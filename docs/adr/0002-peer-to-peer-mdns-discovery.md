# Discovery is peer-to-peer mDNS, spoken in-process

Nodes are found by browsing `_nmos-node._tcp` over multicast DNS from inside the
process, using the pure-Rust `mdns-sd` crate. There is **no registry client**:
IS-04 Registration and Query are not implemented, which matches how a small
plant is actually wired and matches the bench, where neither
`_nmos-register._tcp` nor `_nmos-query._tcp` answered at all.

Shelling out to `avahi-browse` and parsing its output was the obvious shortcut
and is rejected: it makes a controller that must hold a live picture of the
network depend on a system daemon, on that daemon's output format, and on a
subprocess round trip per scan. Speaking mDNS ourselves also means the tool works
on a host with no mDNS daemon installed at all.

## Consequences

Nothing here forecloses a registry. Discovery is expressed as a stream of appear
and depart events, and a registry is simply another producer of those.

The gap this leaves is real and is surfaced rather than hidden: mDNS is
link-local, so a Receiver fed from another network segment names a Sender we will
never see, and that shows on screen as an unresolved edge — see
[0004](./0004-connection-vocabulary-and-graph.md).
