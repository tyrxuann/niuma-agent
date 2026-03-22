//! Main application state and logic.
//!
//! This module contains the core App struct that manages the application state,
//! including the current view, chat messages, tasks, and logs.

use std::{sync::Arc, time::Instant};

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
#[expect(
    dead_code,
    reason = "Part of public API for future log display features"
)]
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
    /// Pending user input to be processed by the agent.
    pending_input: Option<String>,
    /// Whether the agent is currently processing a message.
    pub is_processing: bool,
    /// Agent engine (set during initialization).
    agent: Option<Arc<super::agent::AgentEngine>>,
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
        let messages = vec![Message {
            sender: MessageSender::Agent,
            content: "Hi! What can I help you with?".to_string(),
        }];

        Self {
            current_view: View::default(),
            messages,
            input: String::new(),
            tasks: Vec::new(),
            logs: Vec::new(),
            should_quit: false,
            connected: true,
            version: "v0.1.0",
            pending_input: None,
            is_processing: false,
            agent: None,
        }
    }

    /// Creates a new application instance with an agent engine.
    #[must_use]
    pub fn with_agent(agent: Arc<super::agent::AgentEngine>) -> Self {
        let mut app = Self::new();
        app.agent = Some(agent);
        app
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
                // Only handle text input in chat view and when not processing
                if self.current_view == View::Chat && !self.is_processing {
                    self.input.push(c);
                }
            }
            InputEvent::Backspace => {
                if self.current_view == View::Chat && !self.is_processing {
                    self.input.pop();
                }
            }
            InputEvent::Enter => {
                if self.current_view == View::Chat && !self.input.is_empty() && !self.is_processing
                {
                    self.queue_message();
                }
            }
            InputEvent::NoOp => {}
        }
    }

    /// Queues the current input as a pending message to be processed.
    fn queue_message(&mut self) {
        let content = std::mem::take(&mut self.input);
        self.messages.push(Message {
            sender: MessageSender::User,
            content: content.clone(),
        });
        self.pending_input = Some(content);
        self.is_processing = true;
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

    /// Returns the number of enabled tasks.
    #[must_use]
    pub fn active_task_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.enabled).count()
    }

    /// Processes pending messages using the agent engine.
    /// This should be called in the main loop.
    pub async fn process_pending(&mut self) {
        let input = match self.pending_input.take() {
            Some(i) => i,
            None => return,
        };

        let agent = match &self.agent {
            Some(a) => Arc::clone(a),
            None => {
                // No agent configured, just add a mock response
                self.is_processing = false;
                self.messages.push(Message {
                    sender: MessageSender::Agent,
                    content: format!(
                        "Received: {}. Configure an LLM provider for full functionality.",
                        input
                    ),
                });
                return;
            }
        };

        let response = agent.process_message(&input).await;
        let display_text = response.display_text();

        self.is_processing = false;
        self.messages.push(Message {
            sender: MessageSender::Agent,
            content: display_text,
        });
    }
}
