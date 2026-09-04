//! Which Senders feed which Receivers, across every Node the controller knows.
//!
//! This is the question an operator means by "connected", and it cannot be
//! answered one Node at a time: the Receiver taking a Sender's stream usually
//! lives on a different box. Pairings come from the Sender identifier a
//! Receiver reports, and — because much equipment reports none — from matching
//! the stream's address and port.
//!
//! Where a pairing cannot be made, it is named rather than dropped. A Receiver
//! pointing at an undiscovered Sender says so and stays Subscribed; a stream
//! matching two Senders is reported as ambiguous with both named, because two
//! sources in one multicast group is a fault and quietly picking one destroys
//! the only evidence of it.

use std::collections::BTreeMap;

use jackfield_nmos::{Reception, ResourceId, StreamAddress};

use super::view::{KnownNode, NodeState, Pairing, ResourceRef, Transport};
use crate::identity::NodeKey;

/// Fill in every pairing, on both ends.
pub(super) fn resolve(nodes: &mut BTreeMap<NodeKey, KnownNode>) {
    let senders = index_senders(nodes);

    // By identifier, and by the stream each Sender puts on the network.
    let by_id: BTreeMap<&ResourceId, &ResourceRef> = senders
        .iter()
        .map(|(reference, _)| (&reference.id, reference))
        .collect();

    let mut by_stream: BTreeMap<&StreamAddress, Vec<&ResourceRef>> = BTreeMap::new();
    for (reference, streams) in &senders {
        for stream in streams {
            by_stream.entry(stream).or_default().push(reference);
        }
    }

    let mut taken: BTreeMap<ResourceId, Vec<ResourceRef>> = BTreeMap::new();

    for node in nodes.values_mut() {
        let NodeState::Ready(contents) = &mut node.state else {
            continue;
        };
        let node_label = contents.node.core.label.clone();

        for device in &mut contents.devices {
            let device_label = device.device.core.label.clone();
            for receiver in &mut device.receivers {
                let pairing = pair(
                    receiver.reception,
                    receiver.receiver.subscription.sender_id.as_ref(),
                    &receiver.transport,
                    &by_id,
                    &by_stream,
                );

                if let Pairing::Resolved(sender) = &pairing {
                    taken
                        .entry(sender.id.clone())
                        .or_default()
                        .push(ResourceRef {
                            node: node.key.clone(),
                            node_label: node_label.clone(),
                            device_label: device_label.clone(),
                            id: receiver.receiver.core.id.clone(),
                            label: receiver.receiver.core.label.clone(),
                        });
                }

                receiver.pairing = pairing;
            }
        }
    }

    for node in nodes.values_mut() {
        let NodeState::Ready(contents) = &mut node.state else {
            continue;
        };
        for device in &mut contents.devices {
            for sender in &mut device.senders {
                let mut receivers = taken
                    .get(&sender.sender.core.id)
                    .cloned()
                    .unwrap_or_default();
                receivers.sort();
                sender.taken_by = receivers;
            }
        }
    }
}

/// Every Sender known, with the streams it puts on the network.
fn index_senders(nodes: &BTreeMap<NodeKey, KnownNode>) -> Vec<(ResourceRef, Vec<StreamAddress>)> {
    let mut senders = Vec::new();
    for node in nodes.values() {
        let NodeState::Ready(contents) = &node.state else {
            continue;
        };
        let node_label = contents.node.core.label.clone();
        for device in &contents.devices {
            for sender in &device.senders {
                senders.push((
                    ResourceRef {
                        node: node.key.clone(),
                        node_label: node_label.clone(),
                        device_label: device.device.core.label.clone(),
                        id: sender.sender.core.id.clone(),
                        label: sender.sender.core.label.clone(),
                    },
                    sender.transport.streams().to_vec(),
                ));
            }
        }
    }
    senders
}

/// Work out what one Receiver is taking.
fn pair(
    reception: Reception,
    sender_id: Option<&ResourceId>,
    transport: &Transport,
    by_id: &BTreeMap<&ResourceId, &ResourceRef>,
    by_stream: &BTreeMap<&StreamAddress, Vec<&ResourceRef>>,
) -> Pairing {
    if reception == Reception::Unsubscribed {
        return Pairing::None;
    }

    // The Receiver names its Sender: the best answer there is, whether or not
    // we have found that Sender.
    if let Some(sender_id) = sender_id {
        return match by_id.get(sender_id) {
            Some(reference) => Pairing::Resolved(Box::new((*reference).clone())),
            None => Pairing::UnknownSender {
                sender_id: sender_id.clone(),
            },
        };
    }

    // It names nobody, so the stream is all there is to go on — and until the
    // transport pass has run there is not even that.
    if transport.is_pending() {
        return Pairing::Pending;
    }

    let mut candidates: Vec<ResourceRef> = Vec::new();
    for stream in transport.streams() {
        if let Some(matches) = by_stream.get(stream) {
            for reference in matches {
                if !candidates.iter().any(|held| held.id == reference.id) {
                    candidates.push((*reference).clone());
                }
            }
        }
    }

    match candidates.len() {
        0 => Pairing::Unmatched,
        1 => match candidates.into_iter().next() {
            Some(only) => Pairing::Resolved(Box::new(only)),
            // Unreachable given the length check, and cheaper to answer than to
            // assert: a controller must not abort over an arithmetic surprise.
            None => Pairing::Unmatched,
        },
        _ => {
            candidates.sort();
            Pairing::Ambiguous { candidates }
        }
    }
}
