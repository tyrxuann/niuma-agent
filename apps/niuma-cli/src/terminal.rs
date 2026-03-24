//! Terminal management module.
//!
//! Provides a RAII guard for terminal state management, ensuring proper
//! cleanup when the TUI application exits.

use std::io::{self, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::error::CliResult;

/// Terminal backend type.
pub type Backend = CrosstermBackend<Stdout>;

/// Terminal type with our backend.
pub type TuiTerminal = Terminal<Backend>;

/// RAII guard for terminal state.
///
/// This struct manages the terminal lifecycle:
/// - On creation: enters alternate screen, enables raw mode, enables mouse capture
/// - On drop: restores original terminal state
///
/// # Example
///
/// ```no_run
/// use niuma_cli::terminal::TerminalGuard;
///
/// fn main() -> anyhow::Result<()> {
///     let guard = TerminalGuard::new()?;
///     let terminal = guard.terminal();
///     // Use terminal...
///     Ok(())
/// } // guard is dropped, terminal is restored
/// ```
#[derive(Debug)]
pub struct TerminalGuard {
    terminal: TuiTerminal,
}

impl TerminalGuard {
    /// Creates a new terminal guard.
    ///
    /// This will:
    /// - Enter alternate screen
    /// - Enable raw mode
    /// - Enable mouse capture
    /// - Create a new terminal instance
    ///
    /// # Errors
    ///
    /// Returns an error if terminal setup fails.
    pub fn new() -> CliResult<Self> {
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        terminal::enable_raw_mode()?;

        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Returns a mutable reference to the terminal.
    #[must_use]
    pub fn terminal(&mut self) -> &mut TuiTerminal {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best effort cleanup - ignore errors during drop
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = terminal::disable_raw_mode();
    }
}
