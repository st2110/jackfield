//! Taking the terminal, and giving it back.
//!
//! The guard restores raw mode, the alternate screen and the cursor from its
//! `Drop`, which runs on a normal exit and on a panic unwind. That is why the
//! release profile does not set `panic = "abort"`: a controller that leaves an
//! operator's terminal in raw mode has cost them their session.

use std::io::{self, Stdout, Write};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Owns the terminal for as long as the application runs.
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    /// Take the terminal.
    ///
    /// # Errors
    ///
    /// Returns an error if raw mode cannot be entered or the alternate screen
    /// cannot be opened — which means this is not an interactive terminal, and
    /// is worth saying plainly rather than drawing into nothing.
    pub fn take() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        // If opening the alternate screen fails, raw mode is already on and has
        // to come back off before returning, or the caller's shell is ruined by
        // an error it did nothing to cause.
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }

    /// The terminal to draw on.
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Put the terminal back as it was found.
    ///
    /// Called from `Drop`, and safe to call more than once.
    fn restore(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        self.terminal.backend_mut().flush()?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Nothing here may fail loudly: this runs while unwinding, and a panic
        // during a panic aborts the process without restoring anything.
        if let Err(error) = self.restore() {
            // The terminal is already in an unknown state; a line on stderr is
            // the most that can honestly be done.
            let _ = writeln!(io::stderr(), "could not restore the terminal: {error}");
        }
    }
}
