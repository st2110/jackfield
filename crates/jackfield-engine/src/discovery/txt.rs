//! Reading the DNS-SD TXT records of an NMOS advertisement.
//!
//! Every field here is optional in practice, whatever the specification says,
//! so each falls back to the IS-04 default rather than taking the whole
//! advertisement down with it. A responder on a plant that publishes nonsense
//! in one record is a Node that still answers on its API.

use std::collections::BTreeMap;

use nmos::{ApiVersion, Protocol};

/// The version IS-04 says to assume when a Node advertises none.
const DEFAULT_VERSION: ApiVersion = ApiVersion::new(1, 0);

/// The scheme the Node's API is reached over.
pub(crate) fn protocol<S: AsRef<str>>(txt: &BTreeMap<String, S>) -> Protocol {
    match txt.get("api_proto").map(AsRef::as_ref) {
        Some("https") => Protocol::Https,
        // IS-04's default, and what anything unreadable falls back to: a Node
        // whose `api_proto` is garbage is far more likely to be plain HTTP than
        // to be unreachable.
        _ => Protocol::Http,
    }
}

/// The API versions the Node says it speaks, lowest first, deduplicated.
pub(crate) fn versions<S: AsRef<str>>(txt: &BTreeMap<String, S>) -> Vec<ApiVersion> {
    let Some(raw) = txt.get("api_ver") else {
        return vec![DEFAULT_VERSION];
    };

    let mut versions: Vec<ApiVersion> = raw
        .as_ref()
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();

    if versions.is_empty() {
        return vec![DEFAULT_VERSION];
    }

    versions.sort_unstable();
    versions.dedup();
    versions
}

/// Whether the Node's API requires authorization.
pub(crate) fn requires_authorization<S: AsRef<str>>(txt: &BTreeMap<String, S>) -> bool {
    // Only a literal `true` means yes. Anything else — absent, `false`, or a
    // value nobody can read — is treated as no, which is IS-04's default and
    // fails towards attempting the Node rather than writing it off.
    txt.get("api_auth").map(AsRef::as_ref) == Some("true")
}
