//! Turning a Node's flat resource lists into the tree the interface reads.
//!
//! Two references are followed here. A Sender and a Receiver belong to a
//! Device, which is what the interface groups by; and a Sender carries a Flow,
//! whose media type is the only thing that tells two identically labelled
//! Senders apart. A reference that points at nothing is reported as
//! unresolved — never dropped, and never allowed to fail the whole Node.

use std::collections::BTreeMap;

use jackfield_nmos::{
    Flow, Receiver, ReceiverTransport, ResourceId, ResourceTree, Sender, SenderTransport,
};

use super::view::{
    DeviceView, Media, NodeContents, Orphan, OrphanKind, Pairing, ReceiverView, SenderView,
    Transport,
};

/// Build a Node's contents from what it returned.
pub(super) fn contents(
    tree: ResourceTree,
    sender_transports: &BTreeMap<ResourceId, Result<SenderTransport, String>>,
    receiver_transports: &BTreeMap<ResourceId, Result<ReceiverTransport, String>>,
) -> NodeContents {
    let flows: BTreeMap<&ResourceId, &Flow> =
        tree.flows.iter().map(|flow| (flow.id(), flow)).collect();

    let mut devices: Vec<DeviceView> = tree
        .devices
        .iter()
        .map(|device| DeviceView {
            device: device.clone(),
            senders: Vec::new(),
            receivers: Vec::new(),
        })
        .collect();

    let mut orphans = Vec::new();

    for sender in &tree.senders {
        let view = sender_view(sender, &flows, sender_transports);
        match devices
            .iter_mut()
            .find(|d| d.device.core.id == sender.device_id)
        {
            Some(device) => device.senders.push(view),
            None => orphans.push(Orphan {
                id: sender.core.id.clone(),
                label: sender.core.label.clone(),
                device_id: sender.device_id.clone(),
                kind: OrphanKind::Sender,
            }),
        }
    }

    for receiver in &tree.receivers {
        let view = receiver_view(receiver, receiver_transports);
        match devices
            .iter_mut()
            .find(|d| d.device.core.id == receiver.device_id)
        {
            Some(device) => device.receivers.push(view),
            None => orphans.push(Orphan {
                id: receiver.core.id.clone(),
                label: receiver.core.label.clone(),
                device_id: receiver.device_id.clone(),
                kind: OrphanKind::Receiver,
            }),
        }
    }

    NodeContents {
        node: tree.node,
        devices,
        orphans,
    }
}

fn sender_view(
    sender: &Sender,
    flows: &BTreeMap<&ResourceId, &Flow>,
    transports: &BTreeMap<ResourceId, Result<SenderTransport, String>>,
) -> SenderView {
    let media = match &sender.flow_id {
        None => Media::None,
        Some(flow_id) => match flows.get(flow_id) {
            Some(flow) => Media::Known {
                media_type: flow.media_type().clone(),
                format: flow.format(),
            },
            None => Media::Unresolved {
                flow_id: flow_id.clone(),
            },
        },
    };

    SenderView {
        sender: sender.clone(),
        transmission: sender.transmission(),
        media,
        transport: sender_transport(transports.get(&sender.core.id)),
        taken_by: Vec::new(),
    }
}

fn receiver_view(
    receiver: &Receiver,
    transports: &BTreeMap<ResourceId, Result<ReceiverTransport, String>>,
) -> ReceiverView {
    ReceiverView {
        receiver: receiver.clone(),
        reception: receiver.reception(),
        accepts: receiver.caps.media_types().to_vec(),
        transport: receiver_transport(transports.get(&receiver.core.id)),
        pairing: Pairing::Pending,
    }
}

fn sender_transport(held: Option<&Result<SenderTransport, String>>) -> Transport {
    match held {
        None => Transport::Pending,
        Some(Err(reason)) => Transport::Unavailable {
            reason: reason.clone(),
        },
        Some(Ok(transport)) => Transport::Known {
            streams: transport
                .legs
                .iter()
                .filter_map(|leg| leg.destination())
                .collect(),
        },
    }
}

fn receiver_transport(held: Option<&Result<ReceiverTransport, String>>) -> Transport {
    match held {
        None => Transport::Pending,
        Some(Err(reason)) => Transport::Unavailable {
            reason: reason.clone(),
        },
        Some(Ok(transport)) => Transport::Known {
            streams: transport
                .legs
                .iter()
                .filter_map(|leg| leg.stream())
                .collect(),
        },
    }
}

/// Push transport results into contents that were built before they arrived.
pub(super) fn apply_transports(
    contents: &mut NodeContents,
    sender_transports: &BTreeMap<ResourceId, Result<SenderTransport, String>>,
    receiver_transports: &BTreeMap<ResourceId, Result<ReceiverTransport, String>>,
) {
    for device in &mut contents.devices {
        for sender in &mut device.senders {
            sender.transport = sender_transport(sender_transports.get(&sender.sender.core.id));
        }
        for receiver in &mut device.receivers {
            receiver.transport =
                receiver_transport(receiver_transports.get(&receiver.receiver.core.id));
        }
    }
}
