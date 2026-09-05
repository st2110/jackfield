# The protocol crate moved out of this repository

`jackfield-nmos` is gone. The resource model of IS-04, the IS-05 connection
types and the clients for both now live in the `nmos` crate, in a repository of
their own, and this workspace depends on it like any other library.

The reason is not tidiness. The same NMOS documents were already modelled twice
inside one company: here, from the controller end, and in the ST 2110 node from
the serving end. The duplication was literal — `Format` implemented twice over
the same `urn:x-nmos:format:*` strings, and this repository's two-state
`AutoOr<T>` sitting beside the node's four-state `Param<T>`, which is the same
IS-05 transport parameter with the three states a PATCH actually needs.

Whichever product had owned that model, the other would have forked it. A
dependency that two products need belongs to neither of them.

Neither side was a superset of the other, and that is what made the move worth
making rather than merely defensible: this repository had the full resource set
including Senders, Flows and Sources, and no way to write; the node had the
whole staging and activation vocabulary, and only the resources it serves.
Together they cover the protocol. `DESIGN.md` in the other repository records
the shape that serves both ends and, just as importantly, what is refused entry
— identifier schemes derived from product concepts, storage, view-models.

## What stays here

The engine and the interface, and the opinions in them: peer-to-peer discovery,
the in-memory inventory, the connection graph, and a view-model with
`display_name()` and `DeviceView` in it. Those are a browser's judgements, not a
protocol's. Publishing them as a general NMOS engine would promise strangers a
stability we have no reason to offer.

## Consequences

The vendored AMWA schemas and the round-trip suite that applies them left with
the crate; they were only ever exercised by the code that moved.

The dependency is an ordinary version from crates.io (`nmos 1.1`), so a clone of
this repository alone builds. It was briefly a path to a sibling directory while
the crate was being cut, and that is over.

ADR-0001 and ADR-0003 describe decisions that now live elsewhere. They are left
in place, amended rather than deleted: they are the record of why the shapes are
what they are, and the crate inherited the shapes.
