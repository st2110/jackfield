# Writing is one verb, one activation mode, and a fifth state on the screen

IS-05 has a single write: stage a patch against `staged` and say when it takes
effect. Putting a Sender on air, taking it off, connecting a Receiver and
disconnecting it are that one request with four documents, so the controller has
one `Connector` and not four operations. It is separate from `Fetcher` because
the two differ where it matters: a failed read leaves a gap on the screen, a
failed write leaves equipment in a state nobody asked for.

**Only immediate activation.** IS-05 also offers `activate_scheduled_absolute`
and `activate_scheduled_relative`, which exist so that a salvo lands at one
instant across several devices. That needs a clock shared with the equipment.
Until this controller has one, asking for a scheduled activation would promise a
precision it cannot keep, so it asks for `activate_immediate` or nothing.

**The base is resolved per Device.** A Node may run one Connection API instance
per Device, and a Device may run none. A Device that advertises none is not
controllable over IS-05 and is reported as such, rather than having its
resources patched at a neighbour's address.

**Requested is a fifth word.** Connection state arrives on the slower of the two
clocks of ADR-0005, so a write can take up to a minute to appear in what the
Node reports. An interface showing only observations would answer the operator's
keystroke by snapping back to the state the device had a moment ago. So a
request is held against the resource, and a **read that started after the write**
is what clears it — a read already on the wire when the key was pressed
describes the device as it was, and may not settle anything. Refusals are held
the same way, carrying the message the device wrote, because IS-05 requires a
controller to show it.

**The reply is checked, not assumed.** IS-05 has no locking: two controllers can
stage against one resource at once, and the specification's answer is that a
client examines the result of its own PATCH. A device that answers
`master_enable: false` to a request for `true` has not done what was asked, and
saying otherwise would be the one lie this controller cannot afford.

## Consequences

Every write costs a re-read of the Node, which is one request, and buys the
confirmation. A salvo across many resources will want IS-05's `/bulk` endpoint
rather than one patch each; the `Connector` returns a document rather than a
unit so that adding it does not change the shape of anything above.
