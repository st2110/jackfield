//! The per-collection version counters a Node publishes in its advertisement.
//!
//! This is the mechanism IS-04 provides for noticing change without polling: a
//! Node increments a collection's counter when that collection changes, and a
//! controller re-reads only what moved. Real equipment is uneven about
//! maintaining them, which is why the transport pass has a periodic floor —
//! see `docs/adr/0005-two-tier-fetch.md`.

use std::collections::BTreeMap;
use std::fmt;

/// One of the six collections a Node API serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Collection {
    /// The Node's own record.
    Node,
    /// The Devices the Node hosts.
    Devices,
    /// The Senders those Devices expose.
    Senders,
    /// The Receivers those Devices expose.
    Receivers,
    /// The Flows the Senders carry.
    Flows,
    /// The Sources those Flows originate from.
    Sources,
}

impl Collection {
    /// Every collection, in the order a Node's tree is read.
    pub const ALL: [Collection; 6] = [
        Collection::Node,
        Collection::Devices,
        Collection::Senders,
        Collection::Receivers,
        Collection::Flows,
        Collection::Sources,
    ];

    /// The TXT record key this collection's counter is published under.
    #[must_use]
    pub fn counter_key(self) -> &'static str {
        match self {
            Collection::Node => "ver_slf",
            Collection::Devices => "ver_dvc",
            Collection::Senders => "ver_snd",
            Collection::Receivers => "ver_rcv",
            Collection::Flows => "ver_flw",
            Collection::Sources => "ver_src",
        }
    }

    /// The path segment this collection is read from.
    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Collection::Node => "self",
            Collection::Devices => "devices",
            Collection::Senders => "senders",
            Collection::Receivers => "receivers",
            Collection::Flows => "flows",
            Collection::Sources => "sources",
        }
    }
}

impl fmt::Display for Collection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.path())
    }
}

/// What a Node's advertisement says about how current each collection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VersionCounters {
    values: [Option<u64>; 6],
}

impl VersionCounters {
    /// Read the counters out of a set of TXT records.
    ///
    /// A counter that is absent, or that cannot be read as a number, is
    /// reported as absent — never as zero. Zero is a legitimate value, and
    /// reporting an absent counter as zero would falsely imply the collection
    /// had been observed.
    pub(crate) fn from_txt<S: AsRef<str>>(txt: &BTreeMap<String, S>) -> Self {
        let mut values = [None; 6];
        for (slot, collection) in Collection::ALL.into_iter().enumerate() {
            let parsed = txt
                .get(collection.counter_key())
                .and_then(|value| value.as_ref().parse::<u64>().ok());
            if let Some(value) = values.get_mut(slot) {
                *value = parsed;
            }
        }
        Self { values }
    }

    /// The counter for one collection, if the Node published it.
    #[must_use]
    pub fn get(&self, collection: Collection) -> Option<u64> {
        Collection::ALL
            .iter()
            .position(|c| *c == collection)
            .and_then(|slot| self.values.get(slot).copied().flatten())
    }

    /// Whether the Node published no counters at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.iter().all(Option::is_none)
    }

    /// Which collections have moved since `previous`.
    ///
    /// A counter that has appeared, or gone away, counts as a change: in both
    /// cases what is held can no longer be trusted to be current. A Node that
    /// publishes no counters at all is therefore always wholly changed, which
    /// is honest — it is exactly what such a Node tells us.
    #[must_use]
    pub fn changed_since(&self, previous: &Self) -> Vec<Collection> {
        if self.is_empty() && previous.is_empty() {
            return Vec::new();
        }
        Collection::ALL
            .into_iter()
            .filter(|collection| self.get(*collection) != previous.get(*collection))
            .collect()
    }
}
