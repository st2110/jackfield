//! The `jackfield` binary: arguments, logging, and handing off to the run loop.

// Panics are forbidden in production code (AGENTS.md); tests are the one place
// where they are the clearest way to assert.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

use std::fs::File;

use anyhow::{Context, Result};
use clap::Parser;
use jackfield::Options;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let options = Options::parse();
    install_logging(&options)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("cannot start the async runtime")?;

    runtime.block_on(async {
        if options.once {
            let report = jackfield::run_headless(&options).await?;
            print!("{report}");
            return Ok(());
        }
        jackfield::run(&options).await.map(|_| ())
    })
}

/// Send logs somewhere that is not the screen.
///
/// The interface owns the terminal; a log line written over the alternate
/// screen corrupts what an operator is reading. With no `--log-file` the logs
/// go to stderr, which is where they belong for `--once` and for a run whose
/// output is being captured.
fn install_logging(options: &Options) -> Result<()> {
    let filter = EnvFilter::try_new(&options.log_level)
        .with_context(|| format!("`{}` is not a log filter", options.log_level))?;

    match &options.log_file {
        Some(path) => {
            let file = File::create(path)
                .with_context(|| format!("cannot write logs to {}", path.display()))?;
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(file)
                .try_init()
                .map_err(|e| anyhow::anyhow!("cannot install logging: {e}"))?;
        }
        None => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .try_init()
                .map_err(|e| anyhow::anyhow!("cannot install logging: {e}"))?;
        }
    }
    Ok(())
}
