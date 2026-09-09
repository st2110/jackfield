//! Drawing the screen.
//!
//! Everything here is legible without colour. A monochrome terminal, a
//! screenshot in a ticket, a session over a serial console — all of them have
//! to show connection state, and colour alone would show none of it.

use jackfield_engine::{
    DeviceView, KnownNode, NodeState, Pairing, ReceiverView, SenderView, Transport,
};
use nmos::{MediaType, Reception, Transmission};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::{App, DetailTarget, Screen};

/// The keys the interface answers to, shown on screen so an operator need not
/// know them already.
pub const KEY_HINTS: &str = " up/down move  enter/right open  esc/left back  space expand  t on/off  m mark sender  r refresh  q quit ";

/// Marks the row the keyboard is on.
///
/// A mark, not a colour: the screen has to be legible on a monochrome terminal,
/// and "which row am I on" is the single most important thing on it.
const HIGHLIGHT: &str = "▸";

/// Marks which pane the keyboard is talking to.
const FOCUS: &str = "◂";

/// Draw the whole screen.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();

    // The mark costs a row of the panes, so it takes one only while it is
    // held: an operator who is not connecting anything gets the whole screen.
    let mark = marked_line(app);
    let head = usize::from(mark.is_some());
    let mut constraints = Vec::with_capacity(3);
    if mark.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1));
    constraints.push(Constraint::Length(1));

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    if let (Some(text), Some(top)) = (mark, rows.first().copied()) {
        // Bold rather than reversed: reversed is what the highlighted row
        // wears, and two rows wearing it would read as two highlights. Both
        // degrade to plain text on a terminal that has neither.
        frame.render_widget(
            Paragraph::new(text).style(Style::default().add_modifier(Modifier::BOLD)),
            top,
        );
    }

    let Some(body) = rows.get(head).copied() else {
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
    if let Some(footer) = rows.get(head.saturating_add(1)).copied() {
        frame.render_widget(Paragraph::new(KEY_HINTS), footer);
    }
}

/// The Marked Sender, said in one line, or nothing while nothing is marked.
///
/// At the top because the Receiver it will feed is somewhere else entirely: by
/// the time an operator presses `t`, the Sender they named is off screen, on
/// another Node, and the only thing that can tell them what they are about to
/// connect is a line that does not move.
fn marked_line(app: &App) -> Option<String> {
    // Asked of the mark itself, not of the Sender it found: a Sender that has
    // gone off the network leaves the mark standing, and an operator holding
    // one has to be told that rather than left with an empty top line.
    app.marked()?;

    Some(match app.marked_source() {
        Some(source) => format!(
            " Marked  {} / {} / {} [{}] ",
            source.node.display_name(),
            source.device.device.core.label,
            source.sender.sender.core.label,
            source.sender.media,
        ),
        None => " Marked  a Sender no longer on screen ".to_owned(),
    })
}

fn draw_nodes(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.screen() == Screen::Nodes;
    let title = if focused {
        format!(" Nodes {FOCUS} ")
    } else {
        " Nodes ".to_owned()
    };
    let block = Block::default().borders(Borders::ALL).title(title);

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
            let marker = if Some(index) == selected {
                if focused {
                    format!("{HIGHLIGHT} ")
                } else {
                    "> ".to_owned()
                }
            } else {
                "  ".to_owned()
            };
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

fn draw_detail(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let focused = app.screen() == Screen::Node;

    let Some(node) = app.selected() else {
        frame.render_widget(
            Paragraph::new("Nothing selected.")
                .block(Block::default().borders(Borders::ALL).title(" Node ")),
            area,
        );
        return;
    };

    let title = if focused {
        format!(" {} {FOCUS} ", node.display_name())
    } else {
        format!(" {} ", node.display_name())
    };
    let block = Block::default().borders(Borders::ALL).title(title);

    let rows = detail_rows(app);

    // Where the highlighted row is drawn, which only this side knows: a Sender
    // occupies three lines, and more when its Receivers are open.
    let height = usize::from(area.height.saturating_sub(2));
    let selected = app.detail_target();
    let highlighted = selected.as_ref().and_then(|target| {
        rows.iter()
            .position(|row| row.target.as_ref() == Some(target))
    });
    if let Some(line) = highlighted {
        app.scroll_detail_into_view(line, rows.len(), height);
    }
    let scroll = app.detail_scroll();

    let lines: Vec<Line<'_>> = rows
        .into_iter()
        .enumerate()
        .skip(scroll)
        .take(height.max(1))
        .map(|(index, row)| {
            let on_it = focused && Some(index) == highlighted;
            let marker = if on_it { HIGHLIGHT } else { " " };
            let style = if on_it {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(vec![Span::raw(marker.to_owned()), Span::raw(row.text)]).style(style)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// One line of the detail pane, and what the highlight would be on there.
struct DetailRow {
    text: String,
    target: Option<DetailTarget>,
}

impl DetailRow {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: None,
        }
    }

    fn at(text: impl Into<String>, target: DetailTarget) -> Self {
        Self {
            text: text.into(),
            target: Some(target),
        }
    }
}

/// Every line of the detail pane, in order.
fn detail_rows(app: &App) -> Vec<DetailRow> {
    let Some(node) = app.selected() else {
        return Vec::new();
    };

    let mut rows: Vec<DetailRow> = node
        .endpoints
        .iter()
        .map(|endpoint| DetailRow::plain(format!("Address {endpoint}")))
        .collect();
    if rows.is_empty() {
        rows.push(DetailRow::plain("Address unknown"));
    }
    rows.push(DetailRow::plain(""));

    match &node.state {
        NodeState::Loading => rows.push(DetailRow::plain("Loading this Node's resources.")),
        NodeState::Failed { reason } => {
            rows.push(DetailRow::plain(format!(
                "This Node could not be read: {reason}"
            )));
            rows.push(DetailRow::plain("Press r to try again."));
        }
        NodeState::Ready(contents) => {
            rows.extend(node_body(app, &contents.devices, &contents.orphans));
        }
    }

    rows
}

fn node_body(
    app: &App,
    devices: &[DeviceView],
    orphans: &[jackfield_engine::Orphan],
) -> Vec<DetailRow> {
    let mut rows = Vec::new();

    if devices.is_empty() {
        rows.push(DetailRow::plain("This Node exposes no Devices."));
    }

    for device in devices {
        rows.push(DetailRow::plain(format!(
            "Device {}",
            device.device.core.label
        )));
        if device.is_empty() {
            rows.push(DetailRow::plain("    no senders or receivers"));
        }

        for sender in &device.senders {
            // Several short lines rather than one long one: a detail pane on an
            // 80-column terminal is about seventy characters wide, and a row
            // that runs past it loses whichever fact happens to be last. The
            // first of them is what the highlight lands on.
            let target = DetailTarget::Sender(sender.sender.core.id.clone());
            let mut lines = sender_lines(sender).into_iter();
            if let Some(first) = lines.next() {
                rows.push(DetailRow::at(first, target));
            }
            rows.extend(lines.map(DetailRow::plain));

            if app.is_expanded(&sender.sender.core.id) {
                if sender.taken_by.is_empty() {
                    rows.push(DetailRow::plain("            (nothing takes this stream)"));
                }
                for taker in &sender.taken_by {
                    rows.push(DetailRow::plain(format!("            {taker}")));
                }
            }
        }

        for receiver in &device.receivers {
            let target = DetailTarget::Receiver(receiver.receiver.core.id.clone());
            let mut lines = receiver_lines(receiver).into_iter();
            if let Some(first) = lines.next() {
                rows.push(DetailRow::at(first, target));
            }
            rows.extend(lines.map(DetailRow::plain));
        }

        rows.push(DetailRow::plain(""));
    }

    for orphan in orphans {
        rows.push(DetailRow::plain(format!(
            "Unattached {:?} {} — names a Device this Node did not return",
            orphan.kind, orphan.label
        )));
    }

    rows
}

/// One Sender's rows.
///
/// Two lines, not one and not three. One runs past a detail pane on an
/// 80-column terminal and loses whichever fact happens to be last; three means
/// a single Device does not fit on screen, and an operator scrolls to see what
/// they are looking at. State and destination belong together — "transmitting
/// to nowhere" is one thought — and who is listening is the other.
///
/// A Sender is never described as connected. It is Transmitting or Idle, and
/// who takes its stream is a separate fact stated separately — see
/// `docs/adr/0004-connection-vocabulary-and-graph.md`.
fn sender_lines(sender: &SenderView) -> Vec<String> {
    let state = match sender.transmission {
        Transmission::Transmitting => "transmitting",
        Transmission::Idle => "idle",
    };

    let destination = match &sender.transport {
        Transport::Pending => "destination pending".to_owned(),
        Transport::Unavailable { reason } => format!("destination unknown: {reason}"),
        Transport::Known { streams } if streams.is_empty() => "no destination".to_owned(),
        Transport::Known { streams } => {
            let shown: Vec<String> = streams.iter().map(ToString::to_string).collect();
            format!("to {}", shown.join(" + "))
        }
    };

    let takers = if sender.transport.is_pending() {
        "receivers not yet known".to_owned()
    } else if sender.taken_by.is_empty() {
        "taken by nobody".to_owned()
    } else {
        format!(
            "taken by {} receiver{}",
            sender.taken_by.len(),
            if sender.taken_by.len() == 1 { "" } else { "s" }
        )
    };

    let mut lines = vec![
        format!(
            "    Sender {} [{}] {state}, {destination}",
            sender.sender.core.label, sender.media
        ),
        format!("        {takers}"),
    ];
    if let Some(requested) = &sender.requested {
        lines.push(format!("        -> {requested}"));
    }
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

    if let Some(requested) = &receiver.requested {
        lines.push(format!("        -> {requested}"));
    }

    lines
}
