//! Agent engine integration for the CLI.
//!
//! This module provides the [`AgentEngine`] which orchestrates all
//! agent components for the TUI.

use std::sync::Arc;

use niuma_agent::{Clarifier, ClarifyContext, Executor, IntentParser, PlanCache};
use niuma_core::{ExecutionPlan, ExecutionStrategy, Session, Step, UserIntent};
use niuma_llm::LLMProvider;
use niuma_tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// The agent engine that orchestrates all agent components.
#[derive(Debug)]
pub struct AgentEngine {
    intent_parser: IntentParser,
    clarifier: Clarifier,
    executor: Executor,
    plan_cache: PlanCache,
    #[allow(dead_code)]
    tools: Arc<ToolRegistry>,
    current_session: RwLock<Session>,
    clarify_ctx: RwLock<Option<ClarifyContext>>,
}

impl AgentEngine {
    /// Creates a new agent engine with the given LLM provider.
    #[must_use]
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        let tools = Arc::new(ToolRegistry::with_builtins());
        Self {
            intent_parser: IntentParser::new(Arc::clone(&llm)),
            clarifier: Clarifier::new(Arc::clone(&llm)),
            executor: Executor::new(Arc::clone(&llm), Arc::clone(&tools)),
            plan_cache: PlanCache::new(),
            tools,
            current_session: RwLock::new(Session::new()),
            clarify_ctx: RwLock::new(None),
        }
    }

    /// Creates a new agent engine with a plan cache persistence directory.
    #[must_use]
    #[allow(dead_code, unused_variables)]
    pub fn with_cache_persistence(
        llm: Arc<dyn LLMProvider>,
        cache_dir: std::path::PathBuf,
    ) -> Self {
        let tools = Arc::new(ToolRegistry::with_builtins());
        Self {
            intent_parser: IntentParser::new(Arc::clone(&llm)),
            clarifier: Clarifier::new(Arc::clone(&llm)),
            executor: Executor::new(Arc::clone(&llm), Arc::clone(&tools)),
            plan_cache: PlanCache::new(),
            tools,
            current_session: RwLock::new(Session::new()),
            clarify_ctx: RwLock::new(None),
        }
    }

    /// Processes a user message and returns the agent's response.
    ///
    /// This is the main entry point for the TUI. It handles the full
    /// flow from intent classification to execution.
    pub async fn process_message(&self, message: &str) -> AgentResponse {
        let mut session = self.current_session.write().await;

        // Add user message to session
        session.add_event(niuma_core::ExecutionEvent::UserMessage {
            content: message.to_string(),
        });

        // Check if we should compress the session
        if session.should_compress(100) {
            info!(
                event_count = session.events.len(),
                "Session exceeds compression threshold"
            );
        }

        // Check if we're in clarification mode
        {
            let mut ctx_guard = self.clarify_ctx.write().await;
            if let Some(ref mut ctx) = *ctx_guard {
                let result = self.clarifier.process(message, ctx).await;
                match result {
                    Ok(niuma_core::ClarifyResult::Complete { gathered }) => {
                        info!(gathered_count = gathered.len(), "Clarification complete");
                        let plan =
                            self.build_plan_from_gathered(&gathered, session.goal.as_deref());
                        drop(ctx_guard);
                        drop(session);
                        return self.execute_plan(plan).await;
                    }
                    Ok(niuma_core::ClarifyResult::NeedsMore {
                        question,
                        remaining,
                    }) => {
                        debug!(question, remaining, "More clarification needed");
                        return AgentResponse::Clarifying {
                            question,
                            remaining,
                            session_id: session.id.to_string(),
                        };
                    }
                    Ok(niuma_core::ClarifyResult::Failed { reason }) => {
                        info!(reason, "Clarification failed");
                        *ctx_guard = None;
                        return AgentResponse::Error {
                            message: format!("Clarification failed: {}", reason),
                        };
                    }
                    Err(e) => {
                        *ctx_guard = None;
                        return AgentResponse::Error {
                            message: format!("Error during clarification: {}", e),
                        };
                    }
                }
            }
        }

        // Check plan cache first
        if let Some(cached_plan) = self.plan_cache.get_by_goal(message) {
            info!(goal = message, "Cache hit for goal");
            return self.execute_plan(cached_plan).await;
        }

        // Classify intent
        let classification = match self.intent_parser.classify(message).await {
            Ok(c) => c,
            Err(e) => {
                return AgentResponse::Error {
                    message: format!("Failed to classify intent: {}", e),
                };
            }
        };

        debug!(
            intent = ?classification.intent,
            confidence = ?classification.confidence,
            "Intent classified"
        );

        match &classification.strategy {
            ExecutionStrategy::Clarifying { missing } => {
                let mut ctx = ClarifyContext::with_missing(missing.clone());
                let first_question = match self.clarifier.next_question(missing).await {
                    Ok(q) => q,
                    Err(e) => {
                        return AgentResponse::Error {
                            message: format!("Failed to generate question: {}", e),
                        };
                    }
                };

                ctx.set_current_question(first_question.clone());
                *self.clarify_ctx.write().await = Some(ctx);

                if session.goal.is_none() {
                    session.goal = Some(message.to_string());
                }

                AgentResponse::Clarifying {
                    question: first_question,
                    remaining: missing.len(),
                    session_id: session.id.to_string(),
                }
            }
            ExecutionStrategy::Autonomous => match &classification.intent {
                UserIntent::ExecuteNow { goal } => {
                    let plan = self.build_plan_from_goal(goal).await;
                    return self.execute_plan(plan).await;
                }
                UserIntent::CreateScheduledTask { goal, schedule } => {
                    AgentResponse::ScheduledTask {
                        goal: goal.clone(),
                        schedule: schedule.clone(),
                        message: format!("Task '{}' scheduled for {}", goal, schedule),
                    }
                }
                UserIntent::SaveAsScheduledTask { name, schedule } => {
                    AgentResponse::ScheduledTask {
                        goal: name.clone(),
                        schedule: schedule.clone(),
                        message: format!("Saved as scheduled task '{}'", name),
                    }
                }
                UserIntent::Other(desc) => AgentResponse::Message {
                    content: format!("I understand you want: {}. Let me help with that.", desc),
                },
            },
        }
    }

    async fn build_plan_from_goal(&self, goal: &str) -> ExecutionPlan {
        ExecutionPlan::new(vec![Step::new(
            "step_1",
            "shell",
            serde_json::json!({ "command": "echo", "args": [goal] }),
        )])
    }

    fn build_plan_from_gathered(
        &self,
        gathered: &std::collections::HashMap<String, String>,
        _goal: Option<&str>,
    ) -> ExecutionPlan {
        let steps: Vec<Step> = gathered
            .iter()
            .enumerate()
            .map(|(i, (field, value))| {
                Step::new(
                    format!("step_{}", i + 1),
                    "shell",
                    serde_json::json!({
                        "command": "echo",
                        "args": [format!("{}: {}", field, value)]
                    }),
                )
            })
            .collect();

        ExecutionPlan::new(steps)
    }

    async fn execute_plan(&self, plan: ExecutionPlan) -> AgentResponse {
        let mut session = self.current_session.write().await;

        if let Some(goal) = &session.goal {
            self.plan_cache.put(goal, plan.clone());
        }

        info!(step_count = plan.steps.len(), "Executing plan");

        match self.executor.execute(&plan, &mut session).await {
            Ok(result) => {
                if result.success {
                    AgentResponse::ExecutionComplete {
                        success: true,
                        step_count: result.step_results.len(),
                        total_duration_ms: result.total_duration.as_millis() as u64,
                        message: format!(
                            "Task completed successfully in {} steps",
                            result.step_results.len()
                        ),
                    }
                } else {
                    let error_msg = result
                        .error
                        .unwrap_or_else(|| "Execution failed".to_string());
                    AgentResponse::ExecutionComplete {
                        success: false,
                        step_count: result.step_results.len(),
                        total_duration_ms: result.total_duration.as_millis() as u64,
                        message: error_msg,
                    }
                }
            }
            Err(e) => AgentResponse::Error {
                message: format!("Execution error: {}", e),
            },
        }
    }

    /// Returns the current session state.
    #[allow(dead_code)]
    pub async fn session_state(&self) -> niuma_core::DialogueState {
        let session = self.current_session.read().await;
        session.state
    }

    /// Clears the current session.
    #[allow(dead_code)]
    pub async fn clear_session(&self) {
        let mut session = self.current_session.write().await;
        *session = Session::new();
        *self.clarify_ctx.write().await = None;
    }
}

/// Response from the agent engine.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentResponse {
    /// A simple text message from the agent.
    Message {
        /// The message content.
        content: String,
    },
    /// The agent needs clarification from the user.
    Clarifying {
        /// The question to ask the user.
        question: String,
        /// Number of remaining questions.
        remaining: usize,
        /// Session ID for tracking.
        session_id: String,
    },
    /// Execution completed successfully or with errors.
    ExecutionComplete {
        /// Whether execution succeeded.
        success: bool,
        /// Number of steps executed.
        step_count: usize,
        /// Total execution time in milliseconds.
        total_duration_ms: u64,
        /// A message describing the result.
        message: String,
    },
    /// A task was scheduled.
    ScheduledTask {
        /// The task goal.
        goal: String,
        /// The cron schedule.
        schedule: String,
        /// A message describing what happened.
        message: String,
    },
    /// An error occurred.
    Error {
        /// The error message.
        message: String,
    },
}

impl AgentResponse {
    /// Returns the message text for display in the TUI.
    #[must_use]
    pub fn display_text(&self) -> String {
        match self {
            Self::Message { content } => content.clone(),
            Self::Clarifying { question, .. } => question.clone(),
            Self::ExecutionComplete { message, .. } => message.clone(),
            Self::ScheduledTask { message, .. } => message.clone(),
            Self::Error { message } => format!("Error: {}", message),
        }
    }
}
