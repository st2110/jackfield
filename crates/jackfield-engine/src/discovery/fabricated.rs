//! A discovery source the tests drive by hand.
//!
//! It exists so that everything interesting about discovery — what counts as a
//! change, what a departure means, what happens to a hostile TXT record — is
//! tested without a network, a daemon, or a timing assumption. The mDNS browser
//! feeds exactly the same machinery.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Mutex;

use tokio::sync::broadcast;

use super::{Advertisement, Discovery, DiscoveryEvent, Subscription, Tracker};

/// How many events may queue before a slow subscriber starts losing them.
const CAPACITY: usize = 256;

/// A discovery source driven by test code rather than by the network.
#[derive(Debug)]
pub struct FabricatedDiscovery {
    events: broadcast::Sender<DiscoveryEvent>,
    tracker: Mutex<Tracker>,
}

impl Default for FabricatedDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl FabricatedDiscovery {
    /// A source with nothing advertised.
    #[must_use]
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(CAPACITY);
        Self {
            events,
            tracker: Mutex::new(Tracker::default()),
        }
    }

    /// Advertise a Node.
    pub fn advertise(&self, advertisement: Advertisement) {
        let event = match self.tracker.lock() {
            Ok(mut tracker) => tracker.observe(advertisement),
            // A poisoned lock means a test panicked while holding it; there is
            // nothing useful to report and nothing to gain by panicking again.
            Err(_) => None,
        };
        if let Some(event) = event {
            let _ = self.events.send(event);
        }
    }

    /// Advertise from raw parts, which may or may not make sense.
    ///
    /// Parts that cannot be understood are ignored, exactly as a malformed
    /// responder on the network would be — and, as there, ignoring one must not
    /// stop the others.
    pub fn advertise_raw<S: AsRef<str>>(
        &self,
        instance: &str,
        hostname: Option<String>,
        addresses: &[IpAddr],
        port: u16,
        txt: &BTreeMap<String, S>,
    ) {
        if let Some(advertisement) =
            Advertisement::try_from_parts(instance, hostname, addresses, port, txt)
        {
            self.advertise(advertisement);
        }
    }

    /// Withdraw a Node's advertisement.
    pub fn withdraw(&self, instance: &str) {
        let event = match self.tracker.lock() {
            Ok(mut tracker) => tracker.withdraw(instance),
            Err(_) => None,
        };
        if let Some(event) = event {
            let _ = self.events.send(event);
        }
    }
}

impl Discovery for FabricatedDiscovery {
    fn subscribe(&self) -> Subscription {
        // The lock is held across both halves so nothing can be advertised
        // between taking the snapshot and joining the stream.
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
