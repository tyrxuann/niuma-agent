//! Session and execution types for niuma agent.
//!
//! This module provides the core types for managing agent sessions,
//! dialogue states, and execution events.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Error, Result};

/// User intent derived from input classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UserIntent {
    /// Execute the task immediately.
    ExecuteNow {
        /// The goal or task to execute.
        goal: String,
    },
    /// Create a scheduled task.
    CreateScheduledTask {
        /// The goal or task to schedule.
        goal: String,
        /// The schedule (cron expression or natural language).
        schedule: String,
    },
    /// Save an executed task as a scheduled task.
    SaveAsScheduledTask {
        /// Name for the scheduled task.
        name: String,
        /// The schedule (cron expression or natural language).
        schedule: String,
    },
    /// Any other intent that doesn't fit the above categories.
    Other(String),
}

/// State of the clarification process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClarifyState {
    /// Not currently clarifying.
    #[default]
    Idle,
    /// Waiting for user answer.
    AwaitingAnswer,
    /// Clarification is complete.
    Complete,
    /// Clarification failed or was cancelled.
    Failed,
}

/// Result of a clarification step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClarifyResult {
    /// Clarification is complete with gathered information.
    Complete {
        /// All gathered information keyed by field name.
        gathered: std::collections::HashMap<String, String>,
    },
    /// More clarification is needed.
    NeedsMore {
        /// The next question to ask.
        question: String,
        /// Number of remaining questions.
        remaining: usize,
    },
    /// Clarification failed.
    Failed {
        /// Why clarification failed.
        reason: String,
    },
}

/// Execution strategy determined by the intent classifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecutionStrategy {
    /// Proceed autonomously without user input.
    Autonomous,
    /// Ask clarifying questions before proceeding.
    Clarifying {
        /// Information that is missing and needs to be clarified.
        missing: Vec<MissingInfo>,
    },
}

/// Confidence level for intent classification and execution decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// High confidence (> 0.8). The agent can proceed autonomously.
    High,
    /// Medium confidence (0.5 - 0.8). The agent may proceed but should be cautious.
    Medium,
    /// Low confidence (< 0.5). The agent should ask for clarification.
    Low,
}

impl Confidence {
    /// Creates a confidence level from a raw score between 0.0 and 1.0.
    #[must_use]
    pub fn from_score(score: f64) -> Self {
        if score > 0.8 {
            Self::High
        } else if score >= 0.5 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    /// Returns the score range as a tuple of (min, max) for this confidence level.
    #[must_use]
    pub fn range(&self) -> (f64, f64) {
        match self {
            Self::High => (0.8, 1.0),
            Self::Medium => (0.5, 0.8),
            Self::Low => (0.0, 0.5),
        }
    }

    /// Returns true if this confidence level is high enough to proceed autonomously.
    #[must_use]
    pub fn can_proceed_autonomously(&self) -> bool {
        matches!(self, Self::High)
    }
}

/// Information about missing or uncertain data that needs to be clarified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingInfo {
    /// The name of the field or parameter that is missing.
    pub field: String,
    /// The reason why this information is needed.
    pub reason: String,
    /// The question to ask the user to obtain this information.
    pub question: String,
}

impl MissingInfo {
    /// Creates a new missing info entry.
    #[must_use]
    pub fn new(
        field: impl Into<String>,
        reason: impl Into<String>,
        question: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
            question: question.into(),
        }
    }
}

/// A single step in an execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Unique identifier for this step.
    pub id: String,
    /// The name of the tool to execute.
    pub tool: String,
    /// The arguments to pass to the tool (as a JSON value).
    pub args: serde_json::Value,
}

impl Step {
    /// Creates a new step.
    #[must_use]
    pub fn new(id: impl Into<String>, tool: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            tool: tool.into(),
            args,
        }
    }
}

/// The result of executing a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// The ID of the step that was executed.
    pub step_id: String,
    /// Whether the step succeeded.
    pub success: bool,
    /// The output from the tool (if successful).
    pub output: Option<serde_json::Value>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Confidence in the result correctness.
    pub confidence: Confidence,
    /// How long the step took to execute.
    pub duration: Duration,
}

impl StepResult {
    /// Creates a successful step result.
    #[must_use]
    pub fn success(
        step_id: impl Into<String>,
        output: serde_json::Value,
        confidence: Confidence,
        duration: Duration,
    ) -> Self {
        Self {
            step_id: step_id.into(),
            success: true,
            output: Some(output),
            error: None,
            confidence,
            duration,
        }
    }

    /// Creates a failed step result.
    #[must_use]
    pub fn failure(step_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            success: false,
            output: None,
            error: Some(error.into()),
            confidence: Confidence::Low,
            duration: Duration::ZERO,
        }
    }
}

/// The result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool call succeeded.
    pub success: bool,
    /// The output from the tool (if successful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    /// Creates a successful tool result.
    #[must_use]
    pub fn success(output: serde_json::Value) -> Self {
        Self {
            success: true,
            output: Some(output),
            error: None,
        }
    }

    /// Creates a failed tool result.
    #[must_use]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: None,
            error: Some(error.into()),
        }
    }
}

/// An execution plan containing steps to be executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// The ordered list of steps to execute.
    pub steps: Vec<Step>,
    /// Estimated time to complete the execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_duration: Option<Duration>,
}

impl ExecutionPlan {
    /// Creates a new execution plan.
    #[must_use]
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            estimated_duration: None,
        }
    }

    /// Creates a new execution plan with an estimated duration.
    #[must_use]
    pub fn with_duration(steps: Vec<Step>, duration: Duration) -> Self {
        Self {
            steps,
            estimated_duration: Some(duration),
        }
    }

    /// Returns the number of steps in the plan.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns true if the plan has no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// The overall result of executing a plan or task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether the execution completed successfully.
    pub success: bool,
    /// Results for each step in order.
    pub step_results: Vec<StepResult>,
    /// Overall confidence in the execution.
    pub confidence: Confidence,
    /// Total time spent executing.
    pub total_duration: Duration,
    /// Error message if the execution failed completely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ExecutionResult {
    /// Creates a successful execution result.
    #[must_use]
    pub fn success(
        step_results: Vec<StepResult>,
        confidence: Confidence,
        total_duration: Duration,
    ) -> Self {
        Self {
            success: true,
            step_results,
            confidence,
            total_duration,
            error: None,
        }
    }

    /// Creates a failed execution result.
    #[must_use]
    pub fn failure(
        step_results: Vec<StepResult>,
        error: impl Into<String>,
        total_duration: Duration,
    ) -> Self {
        Self {
            success: false,
            step_results,
            confidence: Confidence::Low,
            total_duration,
            error: Some(error.into()),
        }
    }
}

/// Backoff strategy for retrying failed operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backoff {
    /// No backoff - retry immediately.
    None,
    /// Linear backoff: delay increases linearly.
    Linear,
    /// Exponential backoff: delay doubles each time.
    #[default]
    Exponential,
    /// Fixed delay between retries.
    Fixed,
}

/// Action to take when a step fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FailureAction {
    /// Retry the step with the given parameters.
    Retry {
        /// Maximum number of retry attempts.
        max_attempts: u32,
        /// Backoff strategy between retries.
        backoff: Backoff,
    },
    /// Skip this step and continue.
    Skip,
    /// Use a cached result if available.
    UseCached,
    /// Fall back to an alternative step.
    Fallback {
        /// The ID of the fallback step.
        step_id: String,
    },
    /// Ask the user for guidance.
    AskUser,
}

impl FailureAction {
    /// Returns true if this action requires user input.
    #[must_use]
    pub fn requires_user_input(&self) -> bool {
        matches!(self, Self::AskUser)
    }
}

/// A task that can be scheduled and executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for the task.
    pub id: String,
    /// Human-readable name for the task.
    pub name: String,
    /// Cron expression for scheduling (e.g., "0 9 * * *" for daily at 9am).
    pub schedule: String,
    /// Whether the task is currently enabled.
    pub enabled: bool,
    /// Pre-confirmed steps to execute (no clarification needed).
    pub steps: Vec<Step>,
    /// Optional description of what the task does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Task {
    /// Creates a new task.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        schedule: impl Into<String>,
        steps: Vec<Step>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            schedule: schedule.into(),
            enabled: true,
            steps,
            description: None,
        }
    }

    /// Returns a builder for creating tasks with more options.
    #[must_use]
    pub fn builder() -> TaskBuilder {
        TaskBuilder::new()
    }
}

/// Builder for creating tasks.
#[derive(Debug)]
pub struct TaskBuilder {
    id: String,
    name: String,
    schedule: String,
    enabled: bool,
    steps: Vec<Step>,
    description: Option<String>,
}

impl TaskBuilder {
    /// Creates a new task builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: String::new(),
            schedule: String::new(),
            enabled: true,
            steps: Vec::new(),
            description: None,
        }
    }

    /// Sets the task ID.
    #[must_use]
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Sets the task name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the cron schedule.
    #[must_use]
    pub fn schedule(mut self, schedule: impl Into<String>) -> Self {
        self.schedule = schedule.into();
        self
    }

    /// Sets whether the task is enabled.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the steps.
    #[must_use]
    pub fn steps(mut self, steps: Vec<Step>) -> Self {
        self.steps = steps;
        self
    }

    /// Sets the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Builds the task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task name or schedule is empty.
    pub fn build(self) -> Result<Task> {
        if self.name.is_empty() {
            return Err(Error::Generic("Task name cannot be empty".to_string()));
        }
        if self.schedule.is_empty() {
            return Err(Error::Generic("Task schedule cannot be empty".to_string()));
        }
        Ok(Task {
            id: self.id,
            name: self.name,
            schedule: self.schedule,
            enabled: self.enabled,
            steps: self.steps,
            description: self.description,
        })
    }
}

impl Default for TaskBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Events that occur during a session execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecutionEvent {
    /// A message from the user.
    UserMessage {
        /// The content of the message.
        content: String,
    },
    /// A message from the agent.
    AgentMessage {
        /// The content of the message.
        content: String,
    },
    /// A tool call was made.
    ToolCall {
        /// The name of the tool.
        tool: String,
        /// The arguments passed to the tool.
        args: serde_json::Value,
        /// The result of the tool call.
        result: ToolResult,
    },
    /// A clarification question was asked and answered.
    Clarification {
        /// The question asked.
        question: String,
        /// The answer provided.
        answer: String,
    },
    /// An error occurred during execution.
    Error {
        /// The step where the error occurred.
        step: String,
        /// The error message.
        error: String,
    },
    /// A step execution started.
    StepStarted {
        /// The step ID.
        step_id: String,
    },
    /// A step execution completed.
    StepCompleted {
        /// The step ID.
        step_id: String,
        /// The result of the step.
        result: StepResult,
    },
}

impl ExecutionEvent {
    /// Returns true if this event represents a successful tool call.
    #[must_use]
    pub fn is_successful_tool_call(&self) -> bool {
        matches!(
            self,
            Self::ToolCall {
                result: ToolResult { success: true, .. },
                ..
            }
        )
    }

    /// Returns true if this event represents a failed tool call.
    #[must_use]
    pub fn is_failed_tool_call(&self) -> bool {
        matches!(
            self,
            Self::ToolCall {
                result: ToolResult { success: false, .. },
                ..
            }
        )
    }

    /// Returns true if this event represents an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }
}

/// The current state of a dialogue session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DialogueState {
    /// Idle state, waiting for user input.
    #[default]
    Idle,
    /// Classifying user intent.
    Classifying,
    /// Clarifying with the user.
    Clarifying,
    /// Executing a task.
    Executing,
    /// Prompting the user to save a task.
    PromptingSave,
    /// Completed, task finished.
    Completed,
    /// Error state.
    Error,
}

/// A session tracking the conversation and execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for the session.
    pub id: Uuid,
    /// Current state of the dialogue.
    pub state: DialogueState,
    /// Events that occurred during the session.
    pub events: Vec<ExecutionEvent>,
    /// User's original goal for this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Creates a new session with a random ID.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            state: DialogueState::Idle,
            events: Vec::new(),
            goal: None,
        }
    }

    /// Creates a session with a specific ID.
    #[must_use]
    pub fn with_id(id: Uuid) -> Self {
        Self {
            id,
            state: DialogueState::Idle,
            events: Vec::new(),
            goal: None,
        }
    }

    /// Creates a session with a goal.
    #[must_use]
    pub fn with_goal(goal: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            state: DialogueState::Idle,
            events: Vec::new(),
            goal: Some(goal.into()),
        }
    }

    /// Sets the session state.
    pub fn set_state(&mut self, state: DialogueState) {
        self.state = state;
    }

    /// Adds an event to the session.
    pub fn add_event(&mut self, event: ExecutionEvent) {
        self.events.push(event);
    }

    /// Returns all user messages in the session.
    #[must_use]
    pub fn user_messages(&self) -> Vec<&str> {
        self.events
            .iter()
            .filter_map(|e| {
                if let ExecutionEvent::UserMessage { content } = e {
                    Some(content.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns all agent messages in the session.
    #[must_use]
    pub fn agent_messages(&self) -> Vec<&str> {
        self.events
            .iter()
            .filter_map(|e| {
                if let ExecutionEvent::AgentMessage { content } = e {
                    Some(content.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns successful tool calls in the session.
    #[must_use]
    pub fn successful_tool_calls(&self) -> Vec<(&str, &serde_json::Value, &ToolResult)> {
        self.events
            .iter()
            .filter_map(|e| {
                if let ExecutionEvent::ToolCall { tool, args, result } = e {
                    if result.success {
                        Some((tool.as_str(), args, result))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the number of events in the session.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Clears all events from the session.
    pub fn clear_events(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_from_score() {
        assert_eq!(Confidence::from_score(0.9), Confidence::High);
        assert_eq!(Confidence::from_score(0.8), Confidence::Medium);
        assert_eq!(Confidence::from_score(0.5), Confidence::Medium);
        assert_eq!(Confidence::from_score(0.49), Confidence::Low);
        assert_eq!(Confidence::from_score(0.0), Confidence::Low);
    }

    #[test]
    fn test_confidence_can_proceed() {
        assert!(Confidence::High.can_proceed_autonomously());
        assert!(!Confidence::Medium.can_proceed_autonomously());
        assert!(!Confidence::Low.can_proceed_autonomously());
    }

    #[test]
    fn test_missing_info() {
        let info = MissingInfo::new("url", "Need URL to fetch data", "What is the URL?");
        assert_eq!(info.field, "url");
        assert_eq!(info.reason, "Need URL to fetch data");
        assert_eq!(info.question, "What is the URL?");
    }

    #[test]
    fn test_step() {
        let step = Step::new(
            "step1",
            "http",
            serde_json::json!({"url": "https://example.com"}),
        );
        assert_eq!(step.id, "step1");
        assert_eq!(step.tool, "http");
    }

    #[test]
    fn test_execution_plan() {
        let plan = ExecutionPlan::new(vec![
            Step::new("1", "http", serde_json::json!({})),
            Step::new("2", "shell", serde_json::json!({})),
        ]);
        assert_eq!(plan.len(), 2);
        assert!(!plan.is_empty());
    }

    #[test]
    fn test_tool_result() {
        let success = ToolResult::success(serde_json::json!({"data": "ok"}));
        assert!(success.success);
        assert!(success.output.is_some());

        let failure = ToolResult::failure("not found");
        assert!(!failure.success);
        assert!(failure.error.is_some());
    }

    #[test]
    fn test_failure_action() {
        assert!(
            !FailureAction::Retry {
                max_attempts: 3,
                backoff: Backoff::Linear
            }
            .requires_user_input()
        );
        assert!(FailureAction::AskUser.requires_user_input());
    }

    #[test]
    fn test_session() {
        let mut session = Session::new();
        session.add_event(ExecutionEvent::UserMessage {
            content: "Hello".to_string(),
        });
        session.add_event(ExecutionEvent::AgentMessage {
            content: "Hi there!".to_string(),
        });

        assert_eq!(session.event_count(), 2);
        assert_eq!(session.user_messages(), vec!["Hello"]);
        assert_eq!(session.agent_messages(), vec!["Hi there!"]);
    }

    #[test]
    fn test_session_successful_tool_calls() {
        let mut session = Session::new();
        session.add_event(ExecutionEvent::ToolCall {
            tool: "http".to_string(),
            args: serde_json::json!({}),
            result: ToolResult::success(serde_json::json!({"status": 200})),
        });
        session.add_event(ExecutionEvent::ToolCall {
            tool: "http".to_string(),
            args: serde_json::json!({}),
            result: ToolResult::failure("not found"),
        });

        let calls = session.successful_tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "http");
    }

    #[test]
    fn test_task_builder() {
        let task = Task::builder()
            .name("Daily Export")
            .schedule("0 9 * * *")
            .steps(vec![Step::new("1", "shell", serde_json::json!({}))])
            .build()
            .expect("build should succeed");

        assert_eq!(task.name, "Daily Export");
        assert_eq!(task.schedule, "0 9 * * *");
        assert!(task.enabled);
        assert_eq!(task.steps.len(), 1);
    }

    #[test]
    fn test_task_builder_empty_name() {
        let result = Task::builder().schedule("0 9 * * *").build();
        assert!(result.is_err());
    }
}
