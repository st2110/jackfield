//! Drawing the screen.
//!
//! Everything here is legible without colour. A monochrome terminal, a
//! screenshot in a ticket, a session over a serial console — all of them have
//! to show connection state, and colour alone would show none of it.

use jackfield_engine::{
    DeviceView, KnownNode, NodeState, Pairing, ReceiverView, SenderView, Transport,
};
use jackfield_nmos::{MediaType, Reception, Transmission};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::{App, Screen};

/// The keys the interface answers to, shown on screen so an operator need not
/// know them already.
pub const KEY_HINTS: &str = " up/down move  enter open  esc back  space expand  r refresh  q quit ";

/// Draw the whole screen.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let Some(body) = rows.first().copied() else {
        return;
    };
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(body);

    if let Some(left) = panes.first().copied() {
        app.scroll_into_view(usize::from(left.height.saturating_sub(2)));
        draw_nodes(frame, app, left);
    }
    if let Some(right) = panes.get(1).copied() {
        draw_detail(frame, app, right);
    }
    if let Some(footer) = rows.get(1).copied() {
        frame.render_widget(Paragraph::new(KEY_HINTS), footer);
    }
}

fn draw_nodes(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Nodes ");

    if app.nodes().is_empty() {
        // An empty frame with no explanation tells an operator nothing.
        let message = Paragraph::new("No NMOS Nodes found yet.\nStill listening.")
            .block(block)
            .wrap(Wrap { trim: true });
        frame.render_widget(message, area);
        return;
    }

    let selected = app.selected_index();
    let height = usize::from(area.height.saturating_sub(2));
    let items: Vec<ListItem<'_>> = app
        .nodes()
        .iter()
        .enumerate()
        .skip(app.scroll())
        .take(height.max(1))
        .map(|(index, node)| {
            let marker = if Some(index) == selected { "> " } else { "  " };
            let style = if Some(index) == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::raw(node_line(node)),
            ]))
            .style(style)
        })
        .collect();

    frame.render_widget(List::new(items).block(block), area);
}

/// One Node's row.
fn node_line(node: &KnownNode) -> String {
    let address = node
        .preferred_endpoint()
        .map_or_else(|| "no address".to_owned(), ToString::to_string);

    let state = match &node.state {
        NodeState::Loading => "loading".to_owned(),
        NodeState::Failed { reason } => format!("failed: {reason}"),
        NodeState::Ready(contents) => {
            let devices = contents.devices.len();
            format!("{devices} device{}", if devices == 1 { "" } else { "s" })
        }
    };

    // State before address: on a narrow pane the address is what an operator
    // can most afford to lose, and `loading` or `failed` is what they cannot.
    format!("{} — {state} — {address}", node.display_name())
}

fn draw_detail(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(node) = app.selected() else {
        frame.render_widget(
            Paragraph::new("Nothing selected.")
                .block(Block::default().borders(Borders::ALL).title(" Node ")),
            area,
        );
        return;
    };

    let title = format!(" {} ", node.display_name());
    let block = Block::default().borders(Borders::ALL).title(title);

    // The address always appears here, whatever the list pane had room for.
    let mut lines: Vec<Line<'_>> = node
        .endpoints
        .iter()
        .map(|endpoint| Line::raw(format!("Address {endpoint}")))
        .collect();
    if lines.is_empty() {
        lines.push(Line::raw("Address unknown"));
    }
    lines.push(Line::raw(""));

    let body: Vec<Line<'_>> = match &node.state {
        NodeState::Loading => vec![Line::raw("Loading this Node's resources.")],
        NodeState::Failed { reason } => vec![
            Line::raw(format!("This Node could not be read: {reason}")),
            Line::raw("Press r to try again."),
        ],
        NodeState::Ready(contents) => {
            if app.screen() == Screen::Nodes && contents.devices.is_empty() {
                vec![Line::raw("This Node exposes no Devices.")]
            } else {
                node_body(app, &contents.devices, &contents.orphans)
            }
        }
    };
    lines.extend(body);

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn node_body(
    app: &App,
    devices: &[DeviceView],
    orphans: &[jackfield_engine::Orphan],
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if devices.is_empty() {
        lines.push(Line::raw("This Node exposes no Devices."));
    }

    for device in devices {
        lines.push(Line::raw(format!("Device {}", device.device.core.label)));
        if device.is_empty() {
            lines.push(Line::raw("    no senders or receivers"));
        }
        for sender in &device.senders {
            // Several short lines rather than one long one: a detail pane on an
            // 80-column terminal is about seventy characters wide, and a row
            // that runs past it loses whichever fact happens to be last.
            lines.extend(sender_lines(sender).into_iter().map(Line::raw));
            if app.is_expanded(&sender.sender.core.id) {
                for taker in &sender.taken_by {
                    lines.push(Line::raw(format!("            {taker}")));
                }
            }
        }
        for receiver in &device.receivers {
            lines.extend(receiver_lines(receiver).into_iter().map(Line::raw));
        }
        lines.push(Line::raw(""));
    }

    for orphan in orphans {
        lines.push(Line::raw(format!(
            "Unattached {:?} {} — names a Device this Node did not return",
            orphan.kind, orphan.label
        )));
    }

    lines
}

/// One Sender's rows.
///
/// A Sender is never described as connected. It is Transmitting or Idle, and
/// who takes its stream is a separate fact stated separately — see
/// `docs/adr/0004-connection-vocabulary-and-graph.md`.
fn sender_lines(sender: &SenderView) -> Vec<String> {
    let state = match sender.transmission {
        Transmission::Transmitting => "transmitting",
        Transmission::Idle => "idle",
    };

    let mut lines = vec![format!(
        "    Sender {} [{}] {state}",
        sender.sender.core.label, sender.media
    )];

    lines.push(match &sender.transport {
        Transport::Pending => "        destination pending".to_owned(),
        Transport::Unavailable { reason } => format!("        destination unknown: {reason}"),
        Transport::Known { streams } if streams.is_empty() => "        no destination".to_owned(),
        Transport::Known { streams } => {
            let shown: Vec<String> = streams.iter().map(ToString::to_string).collect();
            format!("        to {}", shown.join(" + "))
        }
    });

    lines.push(if sender.transport.is_pending() {
        "        receivers not yet known".to_owned()
    } else if sender.taken_by.is_empty() {
        "        taken by nobody".to_owned()
    } else {
        format!(
            "        taken by {} receiver{}",
            sender.taken_by.len(),
            if sender.taken_by.len() == 1 { "" } else { "s" }
        )
    });

    lines
}

/// One Receiver's rows.
fn receiver_lines(receiver: &ReceiverView) -> Vec<String> {
    let state = match receiver.reception {
        Reception::Subscribed => "subscribed",
        Reception::Unsubscribed => "unsubscribed",
    };

    let accepts = if receiver.accepts.is_empty() {
        "accepts unknown".to_owned()
    } else {
        receiver
            .accepts
            .iter()
            .map(MediaType::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut lines = vec![format!(
        "    Receiver {} [{accepts}] {state}",
        receiver.receiver.core.label
    )];

    match &receiver.pairing {
        Pairing::None => {}
        Pairing::Pending => lines.push("        sender not yet known".to_owned()),
        Pairing::Resolved(sender) => lines.push(format!("        from {sender}")),
        Pairing::UnknownSender { sender_id } => {
            lines.push("        from an unknown sender".to_owned());
            lines.push(format!("            {sender_id}"));
        }
        Pairing::Ambiguous { candidates } => {
            lines.push("        ambiguous between".to_owned());
            for candidate in candidates {
                lines.push(format!("            {candidate}"));
            }
        }
        Pairing::Unmatched => {
            lines.push("        from a sender this controller has not found".to_owned());
        }
    }

    lines
}
