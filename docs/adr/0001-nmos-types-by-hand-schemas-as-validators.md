# NMOS types are written by hand; the AMWA schemas are validators

> This decision moved with the code: the types and the vendored schemas now live
> in the published `nmos` crate, which carries the same two-directional test
> suite. Kept here because it is why the crate looks the way it does.

AMWA publishes the NMOS contract as JSON Schema **draft-04**, built from `allOf`
composition over `resource_core.json` with polymorphism expressed structurally
(`flow_video_raw`, `receiver_mux`, and so on). Rust type generators target
draft-07 and 2020-12, so on draft-04 they either fail outright or flatten the
composition into stuttering types with no enums where the domain plainly has
them. We therefore write the resource types by hand — small, named after the
domain, with `Flow` and `Receiver` polymorphism as real Rust enums — and spend
the published schemas where they actually pay: as validators in the test suite,
applied verbatim through the `jsonschema` crate, which supports draft-04
natively.

The validation runs in **both directions**, and that is the point of the
decision rather than a detail of it: every published example must parse into our
types, and everything our types serialize must validate against the schema
that governs it. One direction alone would let the model drift on the side it
does not check.

The schemas are vendored, not fetched — copied byte-for-byte from `AMWA-TV/is-04`
and `AMWA-TV/is-05` at recorded tags, with their Apache-2.0 licence and upstream
commit alongside them in `schemas/PROVENANCE.md`. Tests must not reach the
network, builds must be reproducible offline, and a public mirror must be able to
show exactly which revision of the contract the code was checked against.

## Considered options

- **Generate with `typify` after converting draft-04 to 2020-12.** Adds a
  conversion step and a generator to the build, and still yields types we would
  wrap by hand.
- **Take an existing NMOS crate from crates.io.** None was found covering the
  controller side of IS-04 plus IS-05; adopting one would trade a known cost for
  an unknown maintenance risk on the load-bearing layer.

## Consequences

Coverage is bounded by what we consume: a field we never model is a field the
round-trip test never exercises. This is accepted, and is why the parsers ignore
unknown fields rather than rejecting them. A divergence from the specification
fails `cargo test` rather than a live plant.
