//! The application loop.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use jackfield_engine::{
    Command, Connector, Discovery, Engine, EngineConfig, EngineHandle, Fetcher, MdnsDiscovery,
    NmosConnector, NmosFetcher, Snapshot,
};
use jackfield_tui::{Action, App, Screen, TerminalGuard, action_for};
use nmos::{ConnectionApiClient, NodeApiClient};

use crate::options::Options;

/// Why the application stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The operator quit.
    Quit,
    /// The engine stopped, so there is nothing left to show.
    EngineStopped,
}

/// Run the controller, taking the terminal.
///
/// # Errors
///
/// Returns an error if discovery cannot be started, an HTTP client cannot be
/// built, or the terminal cannot be taken.
pub async fn run(options: &Options) -> Result<Outcome> {
    let discovery = MdnsDiscovery::start().context("cannot start mdns discovery")?;
    let handle = spawn_engine(discovery, options)?;
    drive(handle).await
}

/// Discover for a while and describe what was found, without taking the
/// terminal.
///
/// # Errors
///
/// As [`run`], less the terminal.
pub async fn run_headless(options: &Options) -> Result<String> {
    let discovery = MdnsDiscovery::start().context("cannot start mdns discovery")?;
    let handle = spawn_engine(discovery, options)?;
    tokio::time::sleep(options.discover_for()).await;
    Ok(describe(&handle.snapshot()))
}

/// Start the engine over a discovery source.
///
/// # Errors
///
/// Returns an error if an HTTP client cannot be built.
pub fn spawn_engine<D>(discovery: D, options: &Options) -> Result<EngineHandle>
where
    D: Discovery + Send + 'static,
{
    let node_api = NodeApiClient::builder()
        .request_timeout(options.request_timeout())
        .connect_timeout(options.connect_timeout())
        .build()
        .context("cannot build the node api client")?;
    let connection_api = ConnectionApiClient::builder()
        .request_timeout(options.request_timeout())
        .connect_timeout(options.connect_timeout())
        .build()
        .context("cannot build the connection api client")?;

    let config = EngineConfig {
        concurrency: options.concurrency,
        ..EngineConfig::default()
    };
    Ok(Engine::new(
        discovery,
        NmosFetcher::new(node_api, connection_api.clone()),
        NmosConnector::new(connection_api),
        config,
    )
    .spawn())
}

/// Start an engine over a fetcher of the caller's choosing.
///
/// Used by the end-to-end test, which needs fixture Nodes rather than a plant.
pub fn spawn_engine_with<D, F, C>(
    discovery: D,
    fetcher: F,
    connector: C,
    options: &Options,
) -> EngineHandle
where
    D: Discovery + Send + 'static,
    F: Fetcher,
    C: Connector,
{
    let config = EngineConfig {
        concurrency: options.concurrency,
        ..EngineConfig::default()
    };
    Engine::new(discovery, fetcher, connector, config).spawn()
}

/// Draw, wait, act, repeat.
///
/// The loop waits on exactly two things: a keystroke or a snapshot. A Node that
/// is slow or unreachable cannot freeze the screen, because nothing here waits
/// on the network at all — the engine does that, on its own tasks.
///
/// # Errors
///
/// Returns an error if the terminal cannot be taken or drawn on.
pub async fn drive(handle: EngineHandle) -> Result<Outcome> {
    let mut guard = TerminalGuard::take().context("cannot take the terminal")?;
    let mut app = App::new();
    let mut keys = jackfield_tui::keys();
    let mut snapshots = handle.snapshots();

    app.apply(handle.snapshot());
    guard
        .terminal()
        .draw(|frame| jackfield_tui::draw(frame, &mut app))?;

    loop {
        tokio::select! {
            key = keys.recv() => {
                let Some(key) = key else {
                    // The keyboard reader ended; there is no way left to quit
                    // from inside, so leave rather than hang.
                    return Ok(Outcome::Quit);
                };
                if let Some(action) = action_for(key)
                    && act(&mut app, &handle, action).await == Some(Outcome::Quit)
                {
                    return Ok(Outcome::Quit);
                }
            }

            changed = snapshots.changed() => {
                if changed.is_err() {
                    return Ok(Outcome::EngineStopped);
                }
                let snapshot = Arc::clone(&snapshots.borrow_and_update());
                app.apply(snapshot);
            }
        }

        guard
            .terminal()
            .draw(|frame| jackfield_tui::draw(frame, &mut app))?;
    }
}

/// Do what a keystroke asked for.
async fn act(app: &mut App, handle: &EngineHandle, action: Action) -> Option<Outcome> {
    match action {
        Action::Next => {
            if app.screen() == Screen::Nodes {
                app.select_next();
            } else {
                app.detail_next();
            }
        }
        Action::Previous => {
            if app.screen() == Screen::Nodes {
                app.select_previous();
            } else {
                app.detail_previous();
            }
        }
        Action::Enter => app.enter(),
        Action::Back => app.back(),
        Action::Expand => app.toggle_selected(),
        Action::Refresh => {
            if let Some(node) = app.selected() {
                let _ = handle.send(Command::Refresh(node.key.clone())).await;
            }
        }
        Action::Mark => app.mark(),
        Action::Toggle => toggle(app, handle).await,
        Action::Quit => return Some(Outcome::Quit),
    }
    None
}

/// Change what the highlighted resource is doing.
///
/// A Sender goes on or off air. A Receiver that is taking something is taken
/// off it; one that is taking nothing is pointed at the marked Sender, and does
/// nothing at all if no Sender has been marked — connecting a Receiver to
/// whatever happened to be nearby is not a guess a controller may make.
async fn toggle(app: &App, handle: &EngineHandle) {
    let Some(command) = toggle_command(app) else {
        return;
    };
    // Ignored: an engine that has stopped is discovered by the loop that draws.
    let _ = handle.send(command).await;
}

/// Which command the highlight would produce, or none where there is nothing
/// to ask for.
///
/// Separated from sending it so that the decision — which is where the domain
/// rules live — can be tested without a running engine.
#[must_use]
pub fn toggle_command(app: &App) -> Option<Command> {
    let node = app.selected().map(|node| node.key.clone())?;

    if let Some(view) = app.selected_sender() {
        let transmitting = view.transmission.is_transmitting();
        let sender = view.sender.core.id.clone();
        return Some(if transmitting {
            Command::StopTransmitting { node, sender }
        } else {
            Command::StartTransmitting { node, sender }
        });
    }

    let receiver = app.selected_receiver()?;
    let subscribed = receiver.reception.is_subscribed();
    let receiver = receiver.receiver.core.id.clone();
    if subscribed {
        return Some(Command::Unsubscribe { node, receiver });
    }

    // A Receiver with nothing to take produces no command: connecting it to
    // whatever happened to be nearby is not a guess a controller may make.
    let (from, sender) = app.marked().cloned()?;
    Some(Command::Subscribe {
        node,
        receiver,
        from,
        sender,
    })
}

/// Describe a snapshot in plain text, for `--once`.
#[must_use]
pub fn describe(snapshot: &Snapshot) -> String {
    if snapshot.nodes.is_empty() {
        return "No NMOS Nodes found.\n".to_owned();
    }

    let mut out = String::new();
    for node in &snapshot.nodes {
        let address = node
            .preferred_endpoint()
            .map_or_else(|| "no address".to_owned(), ToString::to_string);
        out.push_str(&format!("{} — {address}\n", node.display_name()));

        if let Some(reason) = node.state.failure() {
            out.push_str(&format!("    could not be read: {reason}\n"));
            continue;
        }

        for device in node.devices() {
            out.push_str(&format!("    Device {}\n", device.device.core.label));
            for sender in &device.senders {
                out.push_str(&format!(
                    "        Sender {} [{}] {} {} {}\n",
                    sender.sender.core.label,
                    sender.media,
                    sender.transmission,
                    destination(&sender.transport),
                    takers(sender),
                ));
            }
            for receiver in &device.receivers {
                out.push_str(&format!(
                    "        Receiver {} {}{}\n",
                    receiver.receiver.core.label,
                    receiver.reception,
                    pairing(&receiver.pairing),
                ));
            }
        }
    }
    out
}

/// Where a Sender sends, in one short phrase.
fn destination(transport: &jackfield_engine::Transport) -> String {
    match transport {
        jackfield_engine::Transport::Pending => "(destination pending)".to_owned(),
        jackfield_engine::Transport::Unavailable { .. } => "(destination unknown)".to_owned(),
        jackfield_engine::Transport::Known { streams } if streams.is_empty() => {
            "(no destination)".to_owned()
        }
        jackfield_engine::Transport::Known { streams } => {
            let shown: Vec<String> = streams.iter().map(ToString::to_string).collect();
            format!("to {}", shown.join(" + "))
        }
    }
}

/// Who takes a Sender's stream, in one short phrase.
fn takers(sender: &jackfield_engine::SenderView) -> String {
    if sender.transport.is_pending() {
        "(receivers not yet known)".to_owned()
    } else if sender.taken_by.is_empty() {
        "taken by nobody".to_owned()
    } else {
        format!("taken by {}", sender.taken_by.len())
    }
}

/// Which Sender feeds a Receiver, in one short phrase.
fn pairing(pairing: &jackfield_engine::Pairing) -> String {
    match pairing {
        jackfield_engine::Pairing::None => String::new(),
        jackfield_engine::Pairing::Pending => " (sender not yet known)".to_owned(),
        jackfield_engine::Pairing::Resolved(sender) => format!(" from {sender}"),
        jackfield_engine::Pairing::UnknownSender { sender_id } => {
            format!(" from an unknown sender {sender_id}")
        }
        jackfield_engine::Pairing::Ambiguous { candidates } => {
            let named: Vec<String> = candidates.iter().map(ToString::to_string).collect();
            format!(" ambiguous between {}", named.join(" and "))
        }
        jackfield_engine::Pairing::Unmatched => {
            " from a sender this controller has not found".to_owned()
        }
    }
}

/// Send the engine home.
///
/// # Errors
///
/// Never fails loudly; an engine that has already stopped needs no telling.
pub async fn stop(handle: &EngineHandle) {
    let _ = handle.send(Command::Stop).await;
    // A moment for the engine's own loop to notice.
    tokio::time::sleep(Duration::from_millis(10)).await;
}
