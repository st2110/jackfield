//! Where a Node's API answers.

use std::fmt;
use std::net::IpAddr;

use nmos::Protocol;

/// One address and port at which a Node's API can be reached.
///
/// A Node may have several. They are an attribute of the Node, not its
/// identity: ST 2110 equipment is multi-homed by design — two media NICs for
/// ST 2022-7 seamless protection, often a separate management port — and a
/// controller that listed such a box twice would be worse than useless, because
/// the operator could not tell which of the two rows was real.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Endpoint {
    /// The address.
    pub address: IpAddr,
    /// The port.
    pub port: u16,
    /// Whether the API is reached over `http` or `https`.
    pub protocol: Protocol,
}

impl Endpoint {
    /// The base URL for requests to this endpoint.
    #[must_use]
    pub fn base_url(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.address {
            IpAddr::V4(address) => {
                write!(f, "{}://{address}:{}", self.protocol.as_str(), self.port)
            }
            // A bare IPv6 address in a URL has to be bracketed, or the colons
            // in it are read as the port separator.
            IpAddr::V6(address) => {
                write!(f, "{}://[{address}]:{}", self.protocol.as_str(), self.port)
            }
        }
    }
}
