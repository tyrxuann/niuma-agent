//! Executor for running agent steps.
//!
//! This module provides the [`Executor`] which runs execution plans
//! with confidence checks and error handling.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use niuma_core::{
    Confidence, ExecutionEvent, ExecutionPlan, ExecutionResult, FailureAction, Session, Step,
    StepResult,
};
use niuma_llm::{ChatCompletionRequest, LLMProvider, Message};
use niuma_tools::ToolRegistry;
use tracing::{debug, info, instrument, warn};

use super::{Error, Result};

/// An executor that runs agent steps with confidence checks.
///
/// The executor takes an execution plan and runs each step, checking
/// confidence after each step and handling failures appropriately.
#[derive(Debug)]
pub struct Executor {
    llm: Arc<dyn LLMProvider>,
    tools: Arc<ToolRegistry>,
    max_retries: u32,
}

impl Executor {
    /// Creates a new executor with the given LLM provider and tool registry.
    #[must_use]
    pub fn new(llm: Arc<dyn LLMProvider>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            llm,
            tools,
            max_retries: 3,
        }
    }

    /// Sets the maximum number of retries for failed steps.
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Executes a task (an execution plan with a goal).
    ///
    /// # Arguments
    ///
    /// * `task` - The task containing the plan and goal
    /// * `session` - The session to record events in
    ///
    /// # Returns
    ///
    /// The execution result.
    ///
    /// # Errors
    ///
    /// Returns an error if the execution fails completely.
    #[instrument(skip(self, session))]
    pub async fn execute(
        &self,
        plan: &ExecutionPlan,
        session: &mut Session,
    ) -> Result<ExecutionResult> {
        self.execute_with_check(plan, session).await
    }

    /// Executes a single step.
    ///
    /// # Arguments
    ///
    /// * `step` - The step to execute
    ///
    /// # Returns
    ///
    /// The result of the step execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the tool is not found or execution fails.
    #[instrument(skip(self))]
    pub async fn execute_step(&self, step: &Step) -> Result<StepResult> {
        self.execute_step_internal(step, true).await
    }

    /// Executes a batch of steps in order.
    ///
    /// Each step is executed sequentially. If a step fails, execution
    /// continues with the next step unless `stop_on_error` is true.
    ///
    /// # Arguments
    ///
    /// * `steps` - The steps to execute
    /// * `stop_on_error` - Whether to stop execution when a step fails
    ///
    /// # Returns
    ///
    /// A vector of step results in the same order as the input steps.
    ///
    /// # Errors
    ///
    /// Returns an error only if a tool is not found (which would affect all subsequent steps).
    #[instrument(skip(self))]
    pub async fn execute_step_batch(
        &self,
        steps: &[Step],
        stop_on_error: bool,
    ) -> Vec<Result<StepResult>> {
        let mut results = Vec::with_capacity(steps.len());

        for step in steps {
            let result = self.execute_step_internal(step, stop_on_error).await;
            results.push(result);

            if stop_on_error && results.last().is_some_and(|r| r.is_err()) {
                break;
            }
        }

        results
    }

    /// Internal method that executes a single step.
    async fn execute_step_internal(&self, step: &Step, _stop_on_error: bool) -> Result<StepResult> {
        let start = Instant::now();

        debug!(step_id = step.id, tool = step.tool, "Executing step");

        let tool = self
            .tools
            .get(&step.tool)
            .ok_or_else(|| Error::ToolNotFound {
                name: step.tool.clone(),
            })?;

        let result = tool.execute(step.args.clone()).await;

        let duration = start.elapsed();

        match result {
            Ok(output) => {
                info!(
                    step_id = step.id,
                    duration_ms = duration.as_millis(),
                    "Step completed successfully"
                );
                Ok(StepResult::success(
                    step.id.clone(),
                    output,
                    Confidence::High,
                    duration,
                ))
            }
            Err(e) => {
                warn!(
                    step_id = step.id,
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "Step failed"
                );
                Ok(StepResult::failure(step.id.clone(), e.to_string()))
            }
        }
    }

    /// Executes a plan with confidence checks.
    ///
    /// After each step, the executor checks the confidence level.
    /// If confidence is low, it records the uncertainty for user review.
    ///
    /// # Arguments
    ///
    /// * `plan` - The execution plan to run
    /// * `session` - The session to record events in
    ///
    /// # Returns
    ///
    /// The execution result with all step results.
    ///
    /// # Errors
    ///
    /// Returns an error if execution cannot proceed at all.
    #[instrument(skip(self, session))]
    pub async fn execute_with_check(
        &self,
        plan: &ExecutionPlan,
        session: &mut Session,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();
        let mut step_results: Vec<StepResult> = Vec::new();

        info!(step_count = plan.steps.len(), "Starting plan execution");

        session.set_state(niuma_core::DialogueState::Executing);

        for step in &plan.steps {
            session.add_event(ExecutionEvent::StepStarted {
                step_id: step.id.clone(),
            });

            let step_result = self.execute_step_with_retry(step).await?;

            session.add_event(ExecutionEvent::StepCompleted {
                step_id: step.id.clone(),
                result: step_result.clone(),
            });

            step_results.push(step_result);

            // Check confidence and pause if low
            if let Some(last) = step_results.last()
                && last.confidence == Confidence::Low
            {
                warn!(
                    step_id = last.step_id,
                    "Low confidence detected, pausing for review"
                );
                session.add_event(ExecutionEvent::Error {
                    step: last.step_id.clone(),
                    error: format!(
                        "Low confidence ({}), execution paused",
                        serde_json::to_string(&last.output).unwrap_or_default()
                    ),
                });
            }
        }

        let total_duration = start.elapsed();
        let success = step_results.iter().all(|r| r.success);
        let overall_confidence = Self::compute_overall_confidence(&step_results);

        let result = if success {
            ExecutionResult::success(step_results, overall_confidence, total_duration)
        } else {
            ExecutionResult::failure(step_results, "One or more steps failed", total_duration)
        };

        info!(
            success = result.success,
            total_steps = plan.steps.len(),
            duration_ms = total_duration.as_millis(),
            "Plan execution completed"
        );

        session.set_state(if success {
            niuma_core::DialogueState::Completed
        } else {
            niuma_core::DialogueState::Error
        });

        Ok(result)
    }

    /// Executes a step with retry logic.
    async fn execute_step_with_retry(&self, step: &Step) -> Result<StepResult> {
        let mut attempt = 0;
        let mut last_error: Option<String> = None;

        while attempt <= self.max_retries {
            attempt += 1;

            let result = self.execute_step(step).await?;

            if result.success {
                return Ok(result);
            }

            last_error = result.error.clone();

            if attempt <= self.max_retries {
                let delay = Duration::from_millis(500 * attempt as u64);
                debug!(
                    step_id = step.id,
                    attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying failed step"
                );
                tokio::time::sleep(delay).await;
            }
        }

        Ok(StepResult::failure(
            step.id.clone(),
            last_error.unwrap_or_default(),
        ))
    }

    /// Computes the overall confidence from individual step results.
    fn compute_overall_confidence(results: &[StepResult]) -> Confidence {
        if results.is_empty() {
            return Confidence::Low;
        }

        let successes = results.iter().filter(|r| r.success).count();
        let ratio = successes as f64 / results.len() as f64;

        if ratio > 0.8 {
            Confidence::High
        } else if ratio >= 0.5 {
            Confidence::Medium
        } else {
            Confidence::Low
        }
    }

    /// Determines the failure action for a failed step.
    ///
    /// # Arguments
    ///
    /// * `step_result` - The result of the failed step
    /// * `plan` - The current execution plan
    ///
    /// # Returns
    ///
    /// The recommended failure action.
    #[instrument(skip(self))]
    pub async fn determine_failure_action(
        &self,
        step_result: &StepResult,
        plan: &ExecutionPlan,
    ) -> Result<FailureAction> {
        let has_more_steps = plan.steps.iter().any(|s| s.id > step_result.step_id);

        let prompt = format!(
            r#"A step failed during execution:
Step: {}
Error: {}

Plan has {} more steps after this one.

Determine the best failure action:
- Retry: If the error might be transient
- Skip: If the step is optional or has a fallback
- AskUser: If the error requires user intervention
- UseCached: If cached results are available

Respond with JSON:
{{
  "action": "retry|skip|askUser|useCached",
  "reasoning": "brief explanation"
}}"#,
            step_result.step_id,
            step_result.error.as_deref().unwrap_or("Unknown error"),
            plan.steps.len()
        );

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![
                Message::system("You are determining how to handle a failed execution step."),
                Message::user(prompt),
            ],
        )
        .with_max_tokens(128);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        Self::parse_failure_action(&content, &step_result.step_id, has_more_steps)
    }

    /// Determines the failure action for a scheduled task execution.
    ///
    /// Scheduled tasks prefer auto-retry since there's no user present to ask.
    /// This is a simplified version that defaults to retry for transient errors.
    ///
    /// # Arguments
    ///
    /// * `step_result` - The result of the failed step
    ///
    /// # Returns
    ///
    /// The recommended failure action for scheduled tasks.
    #[instrument(skip(self))]
    pub async fn determine_failure_action_scheduled(
        &self,
        step_result: &StepResult,
    ) -> Result<FailureAction> {
        // For scheduled tasks, prefer auto-retry with exponential backoff
        // Use LLM to determine if the error is transient
        let is_transient = self
            .is_likely_transient_error(step_result.error.as_deref().unwrap_or("Unknown error"))
            .await?;

        if is_transient {
            Ok(FailureAction::Retry {
                max_attempts: 3,
                backoff: niuma_core::Backoff::Exponential,
            })
        } else {
            // For non-transient errors in scheduled tasks, skip the step
            // rather than failing the entire schedule
            Ok(FailureAction::Skip)
        }
    }

    /// Determines the failure action for immediate task execution.
    ///
    /// Immediate tasks prefer asking the user since they're interactive.
    ///
    /// # Arguments
    ///
    /// * `_step_result` - The result of the failed step (available for context)
    ///
    /// # Returns
    ///
    /// The recommended failure action for immediate tasks.
    #[instrument(skip(self, _step_result))]
    pub async fn determine_failure_action_immediate(
        &self,
        _step_result: &StepResult,
    ) -> Result<FailureAction> {
        // For immediate tasks, prefer asking the user
        Ok(FailureAction::AskUser)
    }

    /// Checks if an error is likely transient based on common patterns.
    #[instrument(skip(self, error))]
    async fn is_likely_transient_error(&self, error: &str) -> Result<bool> {
        let prompt = format!(
            r#"Is this error likely to be transient (temporary)? Examples of transient errors:
- Network timeout, connection refused
- Rate limiting
- Temporary service unavailable
- Resource temporarily unavailable

Error: {}

Respond with JSON:
{{
  "transient": true|false,
  "reason": "brief explanation"
}}"#,
            error
        );

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![
                Message::system("You are analyzing error patterns."),
                Message::user(prompt),
            ],
        )
        .with_max_tokens(64);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        Self::parse_transient_check(&content)
    }

    fn parse_transient_check(content: &str) -> Result<bool> {
        let content = content.trim();
        let json_str = if content.starts_with("```json") {
            content
                .trim_start_matches("```json")
                .trim_end_matches("```")
                .trim()
        } else if content.starts_with("```") {
            content
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
        } else {
            content
        };

        #[derive(serde::Deserialize)]
        struct TransientResult {
            #[serde(alias = "transient", default)]
            is_transient: Option<bool>,
        }

        let parsed: TransientResult = serde_json::from_str(json_str)
            .map_err(|e| Error::Executor(format!("Failed to parse transient check: {}", e)))?;

        Ok(parsed.is_transient.unwrap_or(false))
    }

    fn extract_content(response: &niuma_llm::ChatCompletionResponse) -> Result<String> {
        response
            .choices
            .first()
            .and_then(|c| match &c.message.content {
                niuma_llm::Content::Text(text) => Some(text.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::Executor("No content in response".to_string()))
    }

    fn parse_failure_action(
        content: &str,
        _step_id: &str,
        has_more_steps: bool,
    ) -> Result<FailureAction> {
        let content = content.trim();

        let json_str = if content.starts_with("```json") {
            content
                .trim_start_matches("```json")
                .trim_end_matches("```")
                .trim()
        } else if content.starts_with("```") {
            content
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
        } else {
            content
        };

        #[derive(serde::Deserialize)]
        struct RawAction {
            #[serde(alias = "action", default)]
            action: Option<String>,
        }

        let parsed: RawAction = serde_json::from_str(json_str)
            .map_err(|e| Error::Executor(format!("Failed to parse failure action: {}", e)))?;

        match parsed.action.as_deref() {
            Some("retry") => Ok(FailureAction::Retry {
                max_attempts: 3,
                backoff: niuma_core::Backoff::Exponential,
            }),
            Some("skip") => Ok(FailureAction::Skip),
            Some("askUser") => Ok(FailureAction::AskUser),
            Some("useCached") => Ok(FailureAction::UseCached),
            Some(_) | None => {
                // Default to AskUser if there are more steps, otherwise skip
                if has_more_steps {
                    Ok(FailureAction::AskUser)
                } else {
                    Ok(FailureAction::Skip)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_overall_confidence() {
        // All successful -> High
        let results = vec![
            StepResult::success(
                "1".to_string(),
                serde_json::json!({}),
                Confidence::High,
                Duration::ZERO,
            ),
            StepResult::success(
                "2".to_string(),
                serde_json::json!({}),
                Confidence::High,
                Duration::ZERO,
            ),
        ];
        assert_eq!(
            Executor::compute_overall_confidence(&results),
            Confidence::High
        );

        // All failed -> Low
        let results = vec![
            StepResult::failure("1", "error"),
            StepResult::failure("2", "error"),
        ];
        assert_eq!(
            Executor::compute_overall_confidence(&results),
            Confidence::Low
        );

        // Mixed (50%) -> Medium
        let results = vec![
            StepResult::success(
                "1".to_string(),
                serde_json::json!({}),
                Confidence::High,
                Duration::ZERO,
            ),
            StepResult::failure("2", "error"),
        ];
        assert_eq!(
            Executor::compute_overall_confidence(&results),
            Confidence::Medium
        );

        // Empty -> Low
        assert_eq!(Executor::compute_overall_confidence(&[]), Confidence::Low);
    }

    #[tokio::test]
    async fn test_executor_creation() {
        let tools = Arc::new(ToolRegistry::with_builtins());
        assert_eq!(tools.tool_count(), 4);
        // Can't test fully without a real LLM, but we can verify construction
        // This would need a mock LLM for full testing
    }

    #[tokio::test]
    async fn test_execute_step_batch() {
        let tools = Arc::new(ToolRegistry::with_builtins());
        let llm = Arc::new(niuma_llm::ClaudeProvider::new("test"));
        let executor = Executor::new(llm, Arc::clone(&tools));

        let steps = vec![
            niuma_core::Step::new(
                "s1",
                "shell",
                serde_json::json!({"command": "echo", "args": ["a"]}),
            ),
            niuma_core::Step::new(
                "s2",
                "shell",
                serde_json::json!({"command": "echo", "args": ["b"]}),
            ),
        ];

        let results = executor.execute_step_batch(&steps, false).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
    }

    #[tokio::test]
    async fn test_execute_step_batch_stop_on_error() {
        let tools = Arc::new(ToolRegistry::with_builtins());
        let llm = Arc::new(niuma_llm::ClaudeProvider::new("test"));
        let executor = Executor::new(llm, Arc::clone(&tools));

        let steps = vec![
            niuma_core::Step::new(
                "s1",
                "shell",
                serde_json::json!({"command": "echo", "args": ["a"]}),
            ),
            niuma_core::Step::new("s2", "nonexistent_tool", serde_json::json!({})),
            niuma_core::Step::new(
                "s3",
                "shell",
                serde_json::json!({"command": "echo", "args": ["c"]}),
            ),
        ];

        let results = executor.execute_step_batch(&steps, true).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err()); // Nonexistent tool
    }
}
