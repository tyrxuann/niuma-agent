//! Event handling module.
//!
//! Provides keyboard event handling for the TUI application.

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::error::CliResult;

/// User input events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// Switch to chat view.
    SwitchToChat,
    /// Switch to tasks view.
    SwitchToTasks,
    /// Switch to logs view.
    SwitchToLogs,
    /// Cycle to next view.
    CycleView,
    /// Clear current view content.
    Clear,
    /// Quit the application.
    Quit,
    /// Character input for text entry.
    Char(char),
    /// Backspace key.
    Backspace,
    /// Enter key.
    Enter,
    /// No operation.
    NoOp,
}

/// Event handler for terminal input.
#[derive(Debug)]
pub struct EventHandler {
    /// Tick rate for polling.
    tick_rate: Duration,
}

impl EventHandler {
    /// Creates a new event handler with the specified tick rate.
    #[must_use]
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    /// Polls for the next input event.
    ///
    /// # Errors
    ///
    /// Returns an error if polling or reading events fails.
    pub fn next(&self) -> CliResult<InputEvent> {
        if event::poll(self.tick_rate)?
            && let Event::Key(key) = event::read()?
        {
            return Ok(Self::handle_key(key));
        }
        Ok(InputEvent::NoOp)
    }

    /// Handles a keyboard event and returns the corresponding input event.
    fn handle_key(key: KeyEvent) -> InputEvent {
        // Ignore key release events (repeated key events on some platforms)
        if key.kind == KeyEventKind::Release {
            return InputEvent::NoOp;
        }

        match key.code {
            KeyCode::Char('c' | '1') => InputEvent::SwitchToChat,
            KeyCode::Char('t' | '2') => InputEvent::SwitchToTasks,
            KeyCode::Char('l' | '3') => InputEvent::SwitchToLogs,
            KeyCode::Tab => InputEvent::CycleView,
            KeyCode::Char('L') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                InputEvent::Clear
            }
            KeyCode::Char('q') => InputEvent::Quit,
            KeyCode::Backspace => InputEvent::Backspace,
            KeyCode::Enter => InputEvent::Enter,
            KeyCode::Char(c) => InputEvent::Char(c),
            _ => InputEvent::NoOp,
        }
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(Duration::from_millis(100))
    }
}
