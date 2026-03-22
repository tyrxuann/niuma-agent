//! Main application state and logic.
//!
//! This module contains the core App struct that manages the application state,
//! including the current view, chat messages, tasks, and logs.

use std::time::Instant;

use crate::event::InputEvent;

/// Available views in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Chat view - main conversation interface.
    #[default]
    Chat,
    /// Tasks view - scheduled tasks management.
    Tasks,
    /// Logs view - execution logs.
    Logs,
}

impl View {
    /// Returns the display name of the view.
    #[must_use]
    #[expect(dead_code, reason = "Public API for future use in sidebar rendering")]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn display_name(&self) -> &'static str {
        match self {
            View::Chat => "Chat",
            View::Tasks => "Tasks",
            View::Logs => "Logs",
        }
    }

    /// Cycles to the next view.
    #[must_use]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn next(&self) -> Self {
        match self {
            View::Chat => View::Tasks,
            View::Tasks => View::Logs,
            View::Logs => View::Chat,
        }
    }
}

/// A single chat message.
#[derive(Debug, Clone)]
pub struct Message {
    /// Who sent the message.
    pub sender: MessageSender,
    /// The message content.
    pub content: String,
}

/// Message sender type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSender {
    /// Message from the agent.
    Agent,
    /// Message from the user.
    User,
}

impl MessageSender {
    /// Returns the display prefix for the sender.
    #[must_use]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn prefix(&self) -> &'static str {
        match self {
            MessageSender::Agent => "Agent",
            MessageSender::User => "User",
        }
    }
}

/// A scheduled task.
#[derive(Debug, Clone)]
pub struct Task {
    /// Task identifier.
    #[expect(dead_code, reason = "Public API for future task management features")]
    pub id: usize,
    /// Task name.
    pub name: String,
    /// Cron schedule expression.
    pub schedule: String,
    /// Whether the task is enabled.
    pub enabled: bool,
}

/// A log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Timestamp of the log entry.
    #[expect(dead_code, reason = "Public API for future timestamp display")]
    pub timestamp: Instant,
    /// Log level.
    pub level: LogLevel,
    /// Log message.
    pub message: String,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Information message.
    Info,
    /// Warning message.
    #[expect(dead_code, reason = "Public API for warning log entries")]
    Warn,
    /// Error message.
    #[expect(dead_code, reason = "Public API for error log entries")]
    Error,
    /// Debug message.
    Debug,
}

impl LogLevel {
    /// Returns the display string for the log level.
    #[must_use]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn display(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }
}

/// Application state.
#[derive(Debug)]
pub struct App {
    /// Currently active view.
    pub current_view: View,
    /// Chat messages.
    pub messages: Vec<Message>,
    /// Current input buffer.
    pub input: String,
    /// Scheduled tasks.
    pub tasks: Vec<Task>,
    /// Log entries.
    pub logs: Vec<LogEntry>,
    /// Whether the application should quit.
    pub should_quit: bool,
    /// Connection status (for display).
    pub connected: bool,
    /// Application version.
    pub version: &'static str,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Creates a new application instance with default state.
    #[must_use]
    pub fn new() -> Self {
        let messages = vec![
            Message {
                sender: MessageSender::Agent,
                content: "Hi! What can I help you with?".to_string(),
            },
            Message {
                sender: MessageSender::User,
                content: "Export data from XX site".to_string(),
            },
            Message {
                sender: MessageSender::Agent,
                content: "Sure! What's the URL?".to_string(),
            },
        ];

        let tasks = vec![
            Task {
                id: 1,
                name: "Daily Data Export".to_string(),
                schedule: "0 9 * * *".to_string(),
                enabled: true,
            },
            Task {
                id: 2,
                name: "Weekly Report".to_string(),
                schedule: "0 9 * * 1".to_string(),
                enabled: true,
            },
            Task {
                id: 3,
                name: "Monthly Backup".to_string(),
                schedule: "0 0 1 * *".to_string(),
                enabled: false,
            },
        ];

        let logs = vec![
            LogEntry {
                timestamp: Instant::now(),
                level: LogLevel::Info,
                message: "Application started".to_string(),
            },
            LogEntry {
                timestamp: Instant::now(),
                level: LogLevel::Debug,
                message: "Loading configuration...".to_string(),
            },
        ];

        Self {
            current_view: View::default(),
            messages,
            input: String::new(),
            tasks,
            logs,
            should_quit: false,
            connected: true,
            version: "v0.1.0",
        }
    }

    /// Handles an input event.
    pub fn handle_event(&mut self, event: InputEvent) {
        match event {
            InputEvent::SwitchToChat => self.current_view = View::Chat,
            InputEvent::SwitchToTasks => self.current_view = View::Tasks,
            InputEvent::SwitchToLogs => self.current_view = View::Logs,
            InputEvent::CycleView => self.current_view = self.current_view.next(),
            InputEvent::Clear => self.clear_current_view(),
            InputEvent::Quit => self.should_quit = true,
            InputEvent::Char(c) => {
                // Only handle text input in chat view
                if self.current_view == View::Chat {
                    self.input.push(c);
                }
            }
            InputEvent::Backspace => {
                if self.current_view == View::Chat {
                    self.input.pop();
                }
            }
            InputEvent::Enter => {
                if self.current_view == View::Chat && !self.input.is_empty() {
                    self.send_message();
                }
            }
            InputEvent::NoOp => {}
        }
    }

    /// Clears the content of the current view.
    fn clear_current_view(&mut self) {
        match self.current_view {
            View::Chat => {
                self.messages.clear();
                self.input.clear();
            }
            View::Tasks => self.tasks.clear(),
            View::Logs => self.logs.clear(),
        }
    }

    /// Sends the current input as a user message.
    fn send_message(&mut self) {
        let content = std::mem::take(&mut self.input);
        self.messages.push(Message {
            sender: MessageSender::User,
            content,
        });
    }

    /// Returns the number of enabled tasks.
    #[must_use]
    pub fn active_task_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.enabled).count()
    }
}
