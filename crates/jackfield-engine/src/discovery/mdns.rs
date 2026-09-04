//! Browsing `_nmos-node._tcp` over multicast DNS, in this process.
//!
//! The obvious shortcut — shelling out to `avahi-browse` and parsing its output
//! — makes a controller that must hold a live picture of the network depend on
//! a system daemon, on that daemon's output format, and on a subprocess round
//! trip per scan. Speaking mDNS ourselves also means the tool works on a host
//! with no mDNS daemon installed at all. See
//! `docs/adr/0002-peer-to-peer-mdns-discovery.md`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use mdns_sd::{ServiceDaemon, ServiceEvent};
use thiserror::Error;
use tokio::sync::broadcast;

use super::{Advertisement, Discovery, DiscoveryEvent, NODE_SERVICE_TYPE, Subscription, Tracker};

/// How many events may queue before a slow subscriber starts losing them.
const CAPACITY: usize = 256;

/// Why discovery could not be started.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DiscoveryError {
    /// The mDNS responder could not be started, usually because the multicast
    /// socket could not be bound.
    #[error("cannot start mdns discovery: {reason}")]
    Start {
        /// Why.
        reason: String,
    },

    /// The service type could not be browsed.
    #[error("cannot browse {service_type}: {reason}")]
    Browse {
        /// What was being browsed.
        service_type: String,
        /// Why it failed.
        reason: String,
    },
}

/// Discovers NMOS Nodes by listening to what they say about themselves.
///
/// Runs for as long as it is held, reporting Nodes as they are found rather
/// than only in response to a scan, and never blocking the caller.
pub struct MdnsDiscovery {
    events: broadcast::Sender<DiscoveryEvent>,
    tracker: Arc<Mutex<Tracker>>,
    // Held so the daemon lives as long as discovery does; dropping it stops the
    // browse.
    _daemon: ServiceDaemon,
}

impl std::fmt::Debug for MdnsDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `ServiceDaemon` is not `Debug`, and its internals would say nothing
        // useful anyway.
        f.debug_struct("MdnsDiscovery")
            .field("subscribers", &self.events.receiver_count())
            .finish_non_exhaustive()
    }
}

impl MdnsDiscovery {
    /// Start browsing for NMOS Nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS responder cannot be started or the service
    /// type cannot be browsed — both of which mean the host cannot do
    /// multicast, and are worth telling the operator about rather than
    /// retrying silently.
    pub fn start() -> Result<Self, DiscoveryError> {
        Self::browsing(NODE_SERVICE_TYPE)
    }

    /// Start browsing for a given service type.
    ///
    /// # Errors
    ///
    /// As [`MdnsDiscovery::start`].
    pub fn browsing(service_type: &str) -> Result<Self, DiscoveryError> {
        let daemon = ServiceDaemon::new().map_err(|e| DiscoveryError::Start {
            reason: e.to_string(),
        })?;
        let receiver = daemon
            .browse(service_type)
            .map_err(|e| DiscoveryError::Browse {
                service_type: service_type.to_owned(),
                reason: e.to_string(),
            })?;

        let (events, _) = broadcast::channel(CAPACITY);
        let publish = events.clone();
        let tracker = Arc::new(Mutex::new(Tracker::default()));
        let observing = Arc::clone(&tracker);

        // The mDNS daemon speaks over a blocking channel, so it is drained on a
        // thread and forwarded into the runtime. That is what keeps a slow or
        // silent network from stalling anything else.
        std::thread::Builder::new()
            .name("jackfield-mdns".to_owned())
            .spawn(move || forward(receiver.into_iter(), &observing, &publish))
            .map_err(|e| DiscoveryError::Start {
                reason: e.to_string(),
            })?;

        Ok(Self {
            events,
            tracker,
            _daemon: daemon,
        })
    }
}

impl Discovery for MdnsDiscovery {
    fn subscribe(&self) -> Subscription {
        // The lock is held across both halves so no advertisement can slip
        // between the snapshot and the stream.
        match self.tracker.lock() {
            Ok(tracker) => Subscription {
                known: tracker.known(),
                events: self.events.subscribe(),
            },
            Err(_) => Subscription {
                known: Vec::new(),
                events: self.events.subscribe(),
            },
        }
    }
}

/// Drain the mDNS daemon's events and publish what they amount to.
///
/// Note what happens when nobody is subscribed: nothing. An earlier version
/// treated a failed send as a reason to stop, which quietly killed discovery
/// for the whole run if the very first Node was found in the moment between
/// starting the browse and the first subscriber arriving — and on a plant where
/// a Node answers immediately, that moment is the common case. The loop belongs
/// to the network, not to its audience, so it ends only when the daemon does.
fn forward(
    events: impl Iterator<Item = ServiceEvent>,
    tracker: &Mutex<Tracker>,
    publish: &broadcast::Sender<DiscoveryEvent>,
) {
    publish_all(
        events.filter_map(|event| interpret(tracker, event)),
        publish,
    );
}

/// Publish every event, whether or not anyone is listening.
fn publish_all(
    events: impl Iterator<Item = DiscoveryEvent>,
    publish: &broadcast::Sender<DiscoveryEvent>,
) {
    for event in events {
        // Deliberately ignored. See `forward`.
        let _ = publish.send(event);
    }
}

/// Turn one mDNS event into a discovery event, or into nothing.
///
/// A responder that resolves to no address, publishes no port, or carries a TXT
/// record nobody can read is ignored. One bad responder must never stop
/// discovery, so nothing here can fail loudly.
fn interpret(tracker: &Mutex<Tracker>, event: ServiceEvent) -> Option<DiscoveryEvent> {
    match event {
        ServiceEvent::ServiceResolved(service) => {
            let addresses: Vec<std::net::IpAddr> = service
                .addresses
                .iter()
                .map(mdns_sd::ScopedIp::to_ip_addr)
                .collect();

            let txt: BTreeMap<String, String> = service
                .txt_properties
                .iter()
                .map(|property| (property.key().to_owned(), property.val_str().to_owned()))
                .collect();

            let hostname = Some(service.host.clone()).filter(|host| !host.is_empty());
            let advertisement = Advertisement::try_from_parts(
                instance_name(&service.fullname, &service.ty_domain),
                hostname,
                &addresses,
                service.port,
                &txt,
            );

            match advertisement {
                Some(advertisement) => tracker.lock().ok()?.observe(advertisement),
                None => {
                    tracing::debug!(
                        fullname = %service.fullname,
                        "ignoring an advertisement that resolves to nothing usable"
                    );
                    None
                }
            }
        }
        ServiceEvent::ServiceRemoved(service_type, fullname) => tracker
            .lock()
            .ok()?
            .withdraw(instance_name(&fullname, &service_type)),
        // Searching started or stopped, or a name was seen but not yet
        // resolved: nothing the rest of the system can act on.
        _ => None,
    }
}

/// The instance part of a DNS-SD full name, given the service type it was
/// found under.
///
/// `converter._nmos-node._tcp.local.` under `_nmos-node._tcp.local.` is the
/// converter. Keeping the whole fullname would work equally well as a key, but
/// the instance is what an operator recognises in a log line.
///
/// The service type is taken from the event rather than assumed, because an
/// instance name may itself contain dots and because the browsed type is not
/// always `_nmos-node._tcp` — the tests browse one of their own.
fn instance_name<'a>(fullname: &'a str, service_type: &str) -> &'a str {
    fullname
        .strip_suffix(&format!(".{}", service_type.trim_start_matches('.')))
        .unwrap_or(fullname)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_is_taken_out_of_the_full_name() {
        let nmos = "_nmos-node._tcp.local.";
        assert_eq!(
            instance_name("converter._nmos-node._tcp.local.", nmos),
            "converter"
        );
        // An instance name may contain dots of its own.
        assert_eq!(instance_name("a.b._nmos-node._tcp.local.", nmos), "a.b");
    }

    /// Regression: the service type was once hard-coded here, so a full name
    /// found under any other type came back whole and matched nothing.
    #[test]
    fn the_service_type_comes_from_the_event_not_from_an_assumption() {
        assert_eq!(
            instance_name(
                "fixture._jackfield-test._tcp.local.",
                "_jackfield-test._tcp.local."
            ),
            "fixture"
        );
    }

    /// Regression: discovery must survive a Node found before anyone is
    /// listening. This once stopped the browse for the life of the process,
    /// because a failed send was read as "nobody wants this any more" rather
    /// than "nobody is here yet".
    #[test]
    fn an_event_published_before_anyone_subscribes_does_not_stop_discovery() {
        use std::net::{IpAddr, Ipv4Addr};

        fn appearance(instance: &str) -> DiscoveryEvent {
            let advertisement = Advertisement::try_from_parts(
                instance,
                None,
                &[IpAddr::V4(Ipv4Addr::new(10, 77, 1, 90))],
                8090,
                &std::collections::BTreeMap::<String, String>::new(),
            )
            .expect("usable");
            DiscoveryEvent::Appeared(advertisement)
        }

        let (publish, _) = broadcast::channel(16);

        // No subscriber exists yet, so this send has nowhere to go.
        publish_all(std::iter::once(appearance("early")), &publish);

        // A subscriber arriving afterwards must still be served.
        let mut late = publish.subscribe();
        publish_all(std::iter::once(appearance("later")), &publish);

        match late.try_recv() {
            Ok(DiscoveryEvent::Appeared(advertisement)) => {
                assert_eq!(advertisement.instance, "later");
            }
            other => panic!("discovery stopped after an unheard event: {other:?}"),
        }
    }

    #[test]
    fn a_full_name_of_another_shape_is_kept_whole() {
        let nmos = "_nmos-node._tcp.local.";
        assert_eq!(
            instance_name("something-else.local.", nmos),
            "something-else.local."
        );
        assert_eq!(instance_name("", nmos), "");
    }
}
