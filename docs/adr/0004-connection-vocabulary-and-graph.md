# "Transmitting" is not "connected", and the graph is network-wide

Whether a Sender is putting its Flow on the network and whether anything is
listening are **different questions**, and only the second is what an operator
means by "connected". The bench proved the gap concretely: three Senders on a
Blackmagic converter reported `subscription.active = true` while nothing on the
network was taking their streams. So the vocabulary is fixed and the words are
not interchangeable — a Sender is **Transmitting** or **Idle**, a Receiver is
**Subscribed** or **Unsubscribed**, and no type, field, or screen label in this
project describes a Sender as "active" or "connected". `CONTEXT.md` is the
canonical glossary.

The graph is derived **across every Node the controller knows**, not per Node,
because that is the only place the second question can be answered: the Receiver
that takes a Sender's stream usually lives on a different box. Pairings are made
from the Sender identifier a Receiver reports, and — because the bench hardware
leaves those identifiers `null` — by matching the stream's address and port
against known Senders.

Where a pairing cannot be made, the state is **named rather than dropped**. A
Receiver pointing at an undiscovered Sender says so and carries the identifier it
reported; it stays Subscribed, because rendering it as Unsubscribed would be a
lie. A stream matching two Senders is reported as ambiguous with both named: two
sources in one multicast group is a fault in the plant, and a controller that
quietly picks one has destroyed the only evidence of it. Surfacing gaps is the
job; a plausible answer is worse than a visible question.

## Consequences

Pairing by stream address can be wrong — a Receiver could be joined to a group it
is not actually fed by. The failure mode is deliberately an operator seeing a
question rather than a wrong answer.
