//! What the operator is looking at.
//!
//! The interface holds a selection and a snapshot and nothing else: the
//! authoritative copy of everything lives in the engine, and this crate cannot
//! reach the network to ask. See
//! `docs/adr/0003-crate-split-engine-owns-state.md`.

use std::collections::BTreeSet;
use std::sync::Arc;

use jackfield_engine::{KnownNode, NodeKey, Snapshot};

/// Which pane the operator is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The list of Nodes.
    Nodes,
    /// One Node's Devices, Senders and Receivers.
    Node,
}

/// The interface's own state.
#[derive(Debug, Clone)]
pub struct App {
    snapshot: Arc<Snapshot>,
    /// The Node the operator has selected, held by key rather than by row so
    /// that a Node appearing above it does not move the selection.
    selected: Option<NodeKey>,
    screen: Screen,
    /// Senders whose Receiver list the operator has opened.
    expanded: BTreeSet<jackfield_nmos::ResourceId>,
    /// Which row of the detail pane is highlighted.
    detail_row: usize,
    /// How far the Node list has scrolled.
    scroll: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// An interface with nothing discovered yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Snapshot::default()),
            selected: None,
            screen: Screen::Nodes,
            expanded: BTreeSet::new(),
            detail_row: 0,
            scroll: 0,
        }
    }

    /// Take a new snapshot from the engine.
    ///
    /// The selection is kept on the same Node, not on the same row: a Node
    /// arriving, departing, or being re-read must not make the highlighted row
    /// jump to a different Node.
    pub fn apply(&mut self, snapshot: Arc<Snapshot>) {
        self.snapshot = snapshot;

        let still_there = self
            .selected
            .as_ref()
            .is_some_and(|key| self.snapshot.nodes.iter().any(|node| &node.key == key));

        if !still_there {
            // The selected Node went away. Land somewhere sensible rather than
            // on nothing.
            self.selected = self.snapshot.nodes.first().map(|node| node.key.clone());
            if self.selected.is_none() {
                self.screen = Screen::Nodes;
            }
        }
        if self.selected.is_none() {
            self.selected = self.snapshot.nodes.first().map(|node| node.key.clone());
        }
    }

    /// Every Node, in the order they are shown.
    ///
    /// Deterministic: the engine keys Nodes in a `BTreeMap`, so the order is
    /// the key order and does not depend on when anything was discovered.
    #[must_use]
    pub fn nodes(&self) -> &[KnownNode] {
        &self.snapshot.nodes
    }

    /// Which Node is selected.
    #[must_use]
    pub fn selected(&self) -> Option<&KnownNode> {
        let key = self.selected.as_ref()?;
        self.snapshot.nodes.iter().find(|node| &node.key == key)
    }

    /// The index of the selected Node in the list.
    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        let key = self.selected.as_ref()?;
        self.snapshot.nodes.iter().position(|node| &node.key == key)
    }

    /// Which pane is showing.
    #[must_use]
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// How far the list has scrolled.
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Whether a Sender's Receiver list is open.
    #[must_use]
    pub fn is_expanded(&self, sender: &jackfield_nmos::ResourceId) -> bool {
        self.expanded.contains(sender)
    }

    /// Move the selection down the Node list.
    pub fn select_next(&mut self) {
        self.move_selection(1);
    }

    /// Move the selection up the Node list.
    pub fn select_previous(&mut self) {
        self.move_selection(-1);
    }

    fn move_selection(&mut self, by: isize) {
        if self.snapshot.nodes.is_empty() {
            return;
        }
        let last = self.snapshot.nodes.len().saturating_sub(1);
        let current = self.selected_index().unwrap_or(0);
        let next = current.saturating_add_signed(by).min(last);
        self.selected = self.snapshot.nodes.get(next).map(|node| node.key.clone());
    }

    /// Keep the selected Node visible in a list `height` rows tall.
    pub fn scroll_into_view(&mut self, height: usize) {
        let Some(index) = self.selected_index() else {
            self.scroll = 0;
            return;
        };
        if height == 0 {
            return;
        }
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + height {
            self.scroll = index.saturating_sub(height.saturating_sub(1));
        }
    }

    /// Enter the selected Node.
    pub fn enter(&mut self) {
        if self.selected().is_some() {
            self.screen = Screen::Node;
            self.detail_row = 0;
        }
    }

    /// Return to the Node list, keeping the same Node selected.
    pub fn back(&mut self) {
        self.screen = Screen::Nodes;
    }

    /// Open or close a Sender's Receiver list.
    pub fn toggle_expanded(&mut self, sender: &jackfield_nmos::ResourceId) {
        if !self.expanded.remove(sender) {
            self.expanded.insert(sender.clone());
        }
    }

    /// Move the highlight down the detail pane.
    pub fn detail_next(&mut self) {
        self.detail_row = self.detail_row.saturating_add(1);
    }

    /// Move the highlight up the detail pane.
    pub fn detail_previous(&mut self) {
        self.detail_row = self.detail_row.saturating_sub(1);
    }

    /// Which row of the detail pane is highlighted.
    #[must_use]
    pub fn detail_row(&self) -> usize {
        self.detail_row
    }
}
