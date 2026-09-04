//! What the operator can ask for on the command line.

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;

/// An NMOS controller for SMPTE ST 2110 networks.
///
/// Discovers the Nodes on the local network over mDNS and shows what each one
/// exposes and what is streaming where. This version reads only: it never
/// changes a device.
#[derive(Debug, Clone, Parser)]
#[command(name = "jackfield", version, about, long_about = None)]
pub struct Options {
    /// Where to write logs.
    ///
    /// Logs never go to the screen: the interface owns it, and a log line
    /// written over the alternate screen corrupts what an operator is reading.
    #[arg(long, value_name = "PATH")]
    pub log_file: Option<PathBuf>,

    /// How much to log. Accepts anything `RUST_LOG` does.
    #[arg(long, default_value = "warn", value_name = "FILTER")]
    pub log_level: String,

    /// How long one HTTP request to a Node may take, in seconds.
    #[arg(long, default_value_t = 5, value_name = "SECONDS")]
    pub request_timeout: u64,

    /// How long establishing a connection to a Node may take, in seconds.
    #[arg(long, default_value_t = 2, value_name = "SECONDS")]
    pub connect_timeout: u64,

    /// How many Nodes to read at once.
    #[arg(long, default_value_t = jackfield_engine::FETCH_CONCURRENCY, value_name = "N")]
    pub concurrency: usize,

    /// Discover and print what is found, then exit, without taking the terminal.
    ///
    /// For a machine, a script, or a terminal that is not one.
    #[arg(long)]
    pub once: bool,

    /// How long to discover for in `--once` mode, in seconds.
    #[arg(long, default_value_t = 5, value_name = "SECONDS")]
    pub discover_for: u64,
}

impl Options {
    /// How long one request may take.
    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout)
    }

    /// How long connecting may take.
    #[must_use]
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout)
    }

    /// How long to discover for before printing.
    #[must_use]
    pub fn discover_for(&self) -> Duration {
        Duration::from_secs(self.discover_for)
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            log_file: None,
            log_level: "warn".to_owned(),
            request_timeout: 5,
            connect_timeout: 2,
            concurrency: jackfield_engine::FETCH_CONCURRENCY,
            once: false,
            discover_for: 5,
        }
    }
}
