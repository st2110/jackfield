//! Which Node is which.
//!
//! A Node is identified by the identifier it reports for itself, not by where
//! it answers. The tempting simplification — endpoint as identity — survives
//! exactly as long as every Node has one address, and ST 2110 equipment is
//! multi-homed by design: two media NICs for ST 2022-7 seamless protection,
//! often a separate management port. A controller that listed such a box twice
//! would be worse than useless, because the operator could not tell which of
//! the two rows was real.
//!
//! But the identifier only arrives with the Node's own record, and something
//! has to key the Node before then. So identity is two-stage: the advertisement
//! keys it provisionally, and the first successful fetch settles it. See
//! `docs/adr/0004-connection-vocabulary-and-graph.md` for the neighbouring
//! decision on connection state.

use std::collections::BTreeMap;
use std::fmt;

use nmos::ResourceId;

use crate::discovery::Advertisement;

/// How a Node is keyed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKey {
    /// Before the Node's own record has been read, its DNS-SD instance name is
    /// the best available key. It is not an identity: two instance names may
    /// turn out to be one Node.
    Provisional(String),

    /// Once the Node has reported its identifier, that is what it is.
    Settled(ResourceId),
}

impl NodeKey {
    /// Whether this key is the Node's own identifier.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        matches!(self, NodeKey::Settled(_))
    }
}

impl fmt::Display for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKey::Provisional(instance) => write!(f, "{instance} (not yet identified)"),
            NodeKey::Settled(id) => f.write_str(id.as_str()),
        }
    }
}

/// Maps advertisements to the Node they belong to.
///
/// Every advertisement starts provisional. When one is identified, it is
/// re-keyed, and any other advertisement already known to carry that identifier
/// collapses into the same Node — which is how a box advertising at two
/// addresses becomes one row rather than two.
#[derive(Debug, Default)]
pub struct Identities {
    /// Instance name -> the key that instance currently resolves to.
    by_instance: BTreeMap<String, NodeKey>,
}

/// What happened when an advertisement was identified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identified {
    /// The advertisement was keyed provisionally and now has its identifier.
    Settled {
        /// The key it had before.
        was: NodeKey,
        /// The key it has now.
        now: NodeKey,
    },

    /// The advertisement turned out to belong to a Node already known under a
    /// different instance name. The two are one Node, and its endpoints are the
    /// union of theirs.
    Merged {
        /// The key it had before.
        was: NodeKey,
        /// The Node it joins.
        now: NodeKey,
    },

    /// Nothing changed: this advertisement was already settled under that
    /// identifier.
    Unchanged(NodeKey),
}

impl Identities {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an advertisement, keying it provisionally if it is new.
    pub fn observe(&mut self, advertisement: &Advertisement) -> NodeKey {
        self.by_instance
            .entry(advertisement.instance.clone())
            .or_insert_with(|| NodeKey::Provisional(advertisement.instance.clone()))
            .clone()
    }

    /// Record that an instance reported an identifier.
    pub fn identify(&mut self, instance: &str, id: &ResourceId) -> Identified {
        let settled = NodeKey::Settled(id.clone());
        let was = self
            .by_instance
            .get(instance)
            .cloned()
            .unwrap_or_else(|| NodeKey::Provisional(instance.to_owned()));

        if was == settled {
            return Identified::Unchanged(settled);
        }

        // Does another instance already answer to this identifier? Then this is
        // the same Node reached by a second route, not a second Node.
        let already_known = self
            .by_instance
            .iter()
            .any(|(other, key)| other != instance && key == &settled);

        self.by_instance
            .insert(instance.to_owned(), settled.clone());

        if already_known {
            Identified::Merged { was, now: settled }
        } else {
            Identified::Settled { was, now: settled }
        }
    }

    /// Forget an instance, returning the key it held.
    ///
    /// A Node reachable under another instance name keeps its key: losing one
    /// route to a box is not losing the box.
    pub fn forget(&mut self, instance: &str) -> Option<NodeKey> {
        self.by_instance.remove(instance)
    }

    /// The key an instance currently resolves to.
    #[must_use]
    pub fn key_for(&self, instance: &str) -> Option<&NodeKey> {
        self.by_instance.get(instance)
    }

    /// Every instance name that resolves to a key.
    #[must_use]
    pub fn instances_for(&self, key: &NodeKey) -> Vec<&str> {
        self.by_instance
            .iter()
            .filter(|(_, candidate)| *candidate == key)
            .map(|(instance, _)| instance.as_str())
            .collect()
    }

    /// How many distinct Nodes are known.
    #[must_use]
    pub fn len(&self) -> usize {
        let mut keys: Vec<&NodeKey> = self.by_instance.values().collect();
        keys.sort();
        keys.dedup();
        keys.len()
    }

    /// Whether nothing is known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_instance.is_empty()
    }
}
