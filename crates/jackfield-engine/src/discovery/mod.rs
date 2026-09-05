//! Finding NMOS Nodes on the local network, without a registry.
//!
//! Discovery is expressed as a stream of appear, change and depart events. That
//! shape is deliberate: a registry client, when one arrives, is simply another
//! producer of the same events, and the tests can drive the stream with
//! fabricated advertisements and no network at all. See
//! `docs/adr/0002-peer-to-peer-mdns-discovery.md`.

mod counters;
mod endpoint;
mod fabricated;
pub mod mdns;
mod txt;

use std::collections::BTreeSet;
use std::net::IpAddr;

pub use counters::VersionCounters;
pub use endpoint::Endpoint;
pub use fabricated::FabricatedDiscovery;
use nmos::ApiVersion;
pub use nmos::NodeCollection as Collection;

/// The DNS-SD service type an NMOS Node advertises itself under.
pub const NODE_SERVICE_TYPE: &str = "_nmos-node._tcp.local.";

/// What a Node said about itself over mDNS.
///
/// This is everything known before anything is fetched. Note what it is not: an
/// identity. A Node is identified by the identifier it reports for itself, and
/// an advertisement is only where to go and ask — see
/// `docs/adr/0004-connection-vocabulary-and-graph.md` for the neighbouring
/// decision, and `identity.rs` for this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    /// The DNS-SD instance name, which is how a departure is matched to an
    /// arrival. Unique on the network, but chosen by the vendor and not the
    /// Node's own identifier.
    pub instance: String,

    /// The hostname the responder resolved to, when there was one. The specs
    /// fall back to this for a Node reporting no label.
    pub hostname: Option<String>,

    /// Every address and port this advertisement resolved to. A Node advertised
    /// on three interfaces at one address has one endpoint here, not three.
    pub endpoints: BTreeSet<Endpoint>,

    /// The API versions the Node says it speaks, lowest first.
    pub versions: Vec<ApiVersion>,

    /// Whether the Node requires authorization. Such a Node is reported as
    /// unsupported rather than attempted: IS-10 is out of scope and a 401 is a
    /// worse diagnostic than an honest refusal.
    pub requires_authorization: bool,

    /// The per-collection version counters the Node publishes.
    pub counters: VersionCounters,
}

impl Advertisement {
    /// Build an advertisement from what a responder resolved to, or return
    /// `None` if it cannot be made sense of.
    ///
    /// An advertisement with no address, no port, or no instance name is not a
    /// Node the controller can do anything with. It is dropped rather than
    /// carried as a half-thing, and dropping it must never stop discovery — a
    /// single bad responder on a plant is normal.
    #[must_use]
    pub fn try_from_parts<S: AsRef<str>>(
        instance: &str,
        hostname: Option<String>,
        addresses: &[IpAddr],
        port: u16,
        txt: &std::collections::BTreeMap<String, S>,
    ) -> Option<Self> {
        if instance.is_empty() || addresses.is_empty() || port == 0 {
            return None;
        }

        let protocol = txt::protocol(txt);
        let endpoints: BTreeSet<Endpoint> = addresses
            .iter()
            .map(|address| Endpoint {
                address: *address,
                port,
                protocol,
            })
            .collect();

        Some(Self {
            instance: instance.to_owned(),
            hostname,
            endpoints,
            versions: txt::versions(txt),
            requires_authorization: txt::requires_authorization(txt),
            counters: VersionCounters::from_txt(txt),
        })
    }

    /// The endpoint to make requests to.
    ///
    /// IPv4 is preferred where a Node offers both, IPv6 being untested against
    /// the equipment this change targets. Both are kept, so an IPv6-only Node
    /// still works.
    #[must_use]
    pub fn preferred_endpoint(&self) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.address.is_ipv4())
            .or_else(|| self.endpoints.iter().next())
    }
}

/// Something that happened on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    /// A Node that was not there before is there now.
    Appeared(Advertisement),

    /// A Node that was already known re-advertised itself differently.
    Changed {
        /// What it now says.
        advertisement: Advertisement,
        /// Which collections its counters say have moved. Empty when only the
        /// addresses changed.
        changed: Vec<Collection>,
    },

    /// A Node withdrew its advertisement, or its advertisement expired.
    Departed {
        /// The DNS-SD instance name that went away.
        instance: String,
    },
}

/// What a subscriber gets: everything already known, and everything from here.
///
/// The two arrive together on purpose. A subscriber that only got the stream
/// would miss every Node discovered before it arrived — and since discovery
/// starts before the interface does, on a plant where a Node answers at once
/// that is the common case, not an edge one.
#[derive(Debug)]
pub struct Subscription {
    /// The advertisements already known at the moment of subscribing.
    pub known: Vec<Advertisement>,
    /// Everything that happens from that moment on.
    pub events: tokio::sync::broadcast::Receiver<DiscoveryEvent>,
}

/// A source of discovery events.
///
/// The trait exists so the inventory can be driven by fabricated
/// advertisements in a test and by mDNS in the binary, and so that a registry
/// client — a later change — can be a third implementation without the
/// inventory noticing.
pub trait Discovery {
    /// Everything known now, and everything from now on.
    fn subscribe(&self) -> Subscription;
}

/// Turns a stream of raw advertisements into a stream of events.
///
/// Held separately from the mDNS browser so that the interesting logic — what
/// counts as a change, what counts as a return — is testable with no network,
/// and so a registry client could feed the same machinery.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    known: std::collections::BTreeMap<String, Advertisement>,
}

impl Tracker {
    /// Record an advertisement, returning the event it amounts to, if any.
    pub(crate) fn observe(&mut self, advertisement: Advertisement) -> Option<DiscoveryEvent> {
        match self.known.get(&advertisement.instance) {
            None => {
                self.known
                    .insert(advertisement.instance.clone(), advertisement.clone());
                Some(DiscoveryEvent::Appeared(advertisement))
            }
            Some(previous) if previous == &advertisement => None,
            Some(previous) => {
                let changed = advertisement.counters.changed_since(&previous.counters);
                self.known
                    .insert(advertisement.instance.clone(), advertisement.clone());
                Some(DiscoveryEvent::Changed {
                    advertisement,
                    changed,
                })
            }
        }
    }

    /// Everything currently advertised.
    pub(crate) fn known(&self) -> Vec<Advertisement> {
        self.known.values().cloned().collect()
    }

    /// Record a withdrawal, returning the event it amounts to, if any.
    pub(crate) fn withdraw(&mut self, instance: &str) -> Option<DiscoveryEvent> {
        self.known
            .remove(instance)
            .map(|_| DiscoveryEvent::Departed {
                instance: instance.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::net::Ipv4Addr;

    fn advert(instance: &str, txt: &BTreeMap<String, String>) -> Advertisement {
        Advertisement::try_from_parts(
            instance,
            None,
            &[IpAddr::V4(Ipv4Addr::new(10, 77, 1, 90))],
            8090,
            txt,
        )
        .expect("usable")
    }

    #[test]
    fn a_departure_for_something_never_seen_says_nothing() {
        let mut tracker = Tracker::default();
        assert!(tracker.withdraw("never-here").is_none());
    }

    #[test]
    fn a_return_after_a_departure_is_a_fresh_appearance() {
        let mut tracker = Tracker::default();
        let txt = BTreeMap::new();
        assert!(matches!(
            tracker.observe(advert("c", &txt)),
            Some(DiscoveryEvent::Appeared(_))
        ));
        assert!(tracker.withdraw("c").is_some());
        assert!(matches!(
            tracker.observe(advert("c", &txt)),
            Some(DiscoveryEvent::Appeared(_))
        ));
    }
}
