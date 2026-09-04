# Connection state and transport parameters are fetched on two different clocks

**IS-04 already carries connection state.** Every Sender and Receiver in the Node
API has a `subscription` object whose `active` flag agrees exactly with IS-05's
`master_enable` — verified on the bench, where the three transmitting Senders
reported `{"active": true}` and the rest `false`. Reading state therefore costs
**six requests per Node** (the six resource collections), not one request per
resource.

**But the pairing identifiers are not populated.** On that same hardware
`subscription.receiver_id` is `null` on a transmitting Sender and
`subscription.sender_id` is `null` on every Receiver. Who-feeds-whom cannot be
answered from the resource tree, and must be recovered from the transport
parameters in IS-05 — one request per Sender and per Receiver, eighteen on the
bench converter.

So there are two tiers. State comes from the resource tree, re-read only for the
collections whose version counters moved. Transport parameters are a second,
slower pass over the Connection API, triggered by the same counters and **floored
at once a minute per Node**. The floor exists because the counter mechanism is
only as good as the vendor's implementation of it, real equipment is uneven about
it, and we have deliberately chosen not to verify it by writing to live hardware.

The eager alternative — reading the Connection API per resource every cycle —
means hundreds of requests per cycle against equipment that is on air, on a plant
of only a few dozen boxes.

## Consequences

Connection state and the connection graph refresh on different clocks and can
briefly disagree. The interface therefore reports transport parameters as
*pending* where the second pass has not yet run, rather than as absent — a
Transmitting Sender whose destination is unread must not render as a Sender going
nowhere.
