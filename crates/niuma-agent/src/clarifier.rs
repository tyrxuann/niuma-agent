//! Clarifier for Socrates-style dialogue.
//!
//! This module provides the [`Clarifier`] which guides users through
//! clarifying questions to extract complete execution plans.

use std::sync::Arc;

use niuma_core::{ClarifyResult, ClarifyState, Confidence, ExecutionPlan, MissingInfo, Session};
use niuma_llm::{ChatCompletionRequest, LLMProvider, Message};
use tracing::{debug, instrument};

use super::{Error, Result};

/// A clarifier that guides users through Socrates-style dialogue.
///
/// The clarifier helps extract complete execution plans from partial
/// or ambiguous user input through a series of clarifying questions.
#[derive(Debug)]
pub struct Clarifier {
    llm: Arc<dyn LLMProvider>,
}

impl Clarifier {
    /// Creates a new clarifier with the given LLM provider.
    #[must_use]
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Generates the next clarifying question based on missing information.
    ///
    /// # Arguments
    ///
    /// * `missing` - The list of missing information to ask about
    ///
    /// # Returns
    ///
    /// A natural language question to ask the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails.
    #[instrument(skip(self))]
    pub async fn next_question(&self, missing: &[MissingInfo]) -> Result<String> {
        if missing.is_empty() {
            return Err(Error::Clarifier(
                "No missing information to ask about".to_string(),
            ));
        }

        debug!(
            missing_count = missing.len(),
            fields = ?missing.iter().map(|m| m.field.clone()).collect::<Vec<_>>(),
            "Generating clarifying questions"
        );

        let prompt = Self::build_question_prompt(missing);

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![
                Message::system(Self::clarifier_system_prompt()),
                Message::user(prompt),
            ],
        )
        .with_max_tokens(256);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        debug!(question = %content, "Generated clarifying question");
        Ok(content)
    }

    /// Processes a user's answer to a clarification question.
    ///
    /// # Arguments
    ///
    /// * `answer` - The user's answer
    /// * `ctx` - The clarification context to update
    ///
    /// # Returns
    ///
    /// A [`ClarifyResult`] indicating whether clarification is complete,
    /// needs more questions, or encountered an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails.
    #[instrument(skip(self, ctx))]
    pub async fn process(&self, answer: &str, ctx: &mut ClarifyContext) -> Result<ClarifyResult> {
        let current_missing = ctx.pending_missing();

        if current_missing.is_empty() {
            return Ok(ClarifyResult::Complete {
                gathered: ctx.gathered.clone(),
            });
        }

        debug!(
            answer = answer,
            pending = current_missing
                .first()
                .map(|m| m.field.as_str())
                .unwrap_or("none"),
            "Processing clarification answer"
        );

        let field = &current_missing[0].field;
        ctx.gathered.insert(field.clone(), answer.to_string());
        ctx.answered.push(field.clone());

        let remaining = ctx.pending_missing();
        if remaining.is_empty() {
            debug!("All clarification questions answered");
            Ok(ClarifyResult::Complete {
                gathered: ctx.gathered.clone(),
            })
        } else {
            let next_question = self.next_question(&remaining).await?;
            ctx.current_question = Some(next_question.clone());
            Ok(ClarifyResult::NeedsMore {
                question: next_question,
                remaining: remaining.len(),
            })
        }
    }

    /// Distills a session into an execution plan.
    ///
    /// This method extracts the correct execution path from a session,
    /// filtering out failed attempts, trial-and-error paths, and
    /// clarification dialogue.
    ///
    /// **Distillation rules:**
    /// | Kept | Filtered Out |
    /// |------|--------------|
    /// | Successful tool calls | Failed attempts |
    /// | Confirmed decisions | Trial-and-error paths |
    /// | Required parameters | Clarification dialogue |
    ///
    /// # Arguments
    ///
    /// * `session` - The session containing the dialogue and events
    ///
    /// # Returns
    ///
    /// An [`ExecutionPlan`] with the distilled steps.
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails or plan parsing fails.
    #[instrument(skip(self, session))]
    pub async fn distill(&self, session: &Session) -> Result<ExecutionPlan> {
        debug!(
            session_id = %session.id,
            event_count = session.events.len(),
            "Distilling execution plan from session"
        );

        if session.events.is_empty() {
            return Err(Error::Clarifier(
                "Empty session cannot be distilled".to_string(),
            ));
        }

        let prompt = Self::build_distillation_prompt(session);

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![
                Message::system(Self::distillation_system_prompt()),
                Message::user(prompt),
            ],
        )
        .with_max_tokens(1024);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        let plan = Self::parse_distilled_plan(&content)?;
        debug!(step_count = plan.steps.len(), "Distilled execution plan");

        Ok(plan)
    }

    /// Evaluates the result of a step and determines if it's confident enough to proceed.
    ///
    /// # Arguments
    ///
    /// * `step_id` - The ID of the step
    /// * `result` - The result from the step execution
    ///
    /// # Returns
    ///
    /// A tuple of (confidence, message) where message provides feedback
    /// if confidence is low.
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails.
    #[instrument(skip(self))]
    pub async fn evaluate_result(
        &self,
        step_id: &str,
        result: &serde_json::Value,
    ) -> Result<(Confidence, Option<String>)> {
        let prompt = format!(
            r#"Evaluate the result of step '{}':
Result: {}

Determine if this result is correct and complete:
- Is the data format correct?
- Are there any obvious errors?
- Is this sufficient to proceed to the next step?

Respond in JSON format:
{{
  "confidence": "high|medium|low",
  "reasoning": "brief explanation",
  "feedback": "optional message to user if confidence is not high"
}}"#,
            step_id,
            serde_json::to_string_pretty(result).unwrap_or_default()
        );

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![
                Message::system(Self::evaluation_system_prompt()),
                Message::user(prompt),
            ],
        )
        .with_max_tokens(256);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        Self::parse_evaluation(&content)
    }

    fn clarifier_system_prompt() -> &'static str {
        r#"You are a helpful clarification assistant for a task-processing agent.

When asking clarifying questions:
1. Be specific and focused on one piece of information at a time
2. Use simple, clear language
3. Provide context for why you need the information
4. Ask one question per turn when possible

Keep your responses brief and conversational."#
    }

    fn distillation_system_prompt() -> &'static str {
        r#"You are a plan distillation assistant. Given a conversation session that may include
failed attempts, trial-and-error, and clarification dialogue, extract the correct execution path.

## Distillation Rules

**KEEP:**
- Successful tool calls
- Confirmed decisions
- Required parameters
- Final working approach

**FILTER OUT:**
- Failed attempts and errors
- Trial-and-error explorations
- Clarification questions and answers
- Backtracking and retries

## Output Format

Return a JSON execution plan:
{
  "steps": [
    {
      "id": "step_1",
      "tool": "tool_name",
      "args": { "param1": "value1" }
    }
  ],
  "estimated_duration_seconds": 60
}

Only include the final confirmed path. If a step required multiple attempts,
only include the successful one.

Each step should have:
- A unique ID
- The tool name
- The arguments as a JSON object
- Required parameters only (not exploration values)"#
    }

    fn evaluation_system_prompt() -> &'static str {
        r#"You are evaluating the result of an agent step execution.

Assess the confidence level:
- **High**: Result is clearly correct, complete, and ready for the next step
- **Medium**: Result is mostly correct but with minor concerns
- **Low**: Result has issues, is incomplete, or indicates failure

Common issues to detect:
- Empty or null results
- Error messages in the output
- Missing expected fields
- Malformed data
- Timeout or truncation indicators"#
    }

    fn build_question_prompt(missing: &[MissingInfo]) -> String {
        let mut prompt = "Please answer the following clarifying question:\n\n".to_string();

        for (i, info) in missing.iter().take(3).enumerate() {
            prompt.push_str(&format!("{}. {}\n", i + 1, info.question));
        }

        prompt.push_str("\nProvide your answer:");
        prompt
    }

    fn build_distillation_prompt(session: &Session) -> String {
        let mut prompt = "## Session Events\n\n".to_string();

        for event in &session.events {
            match event {
                niuma_core::ExecutionEvent::UserMessage { content } => {
                    prompt.push_str(&format!("[User] {}\n", content));
                }
                niuma_core::ExecutionEvent::AgentMessage { content } => {
                    prompt.push_str(&format!("[Agent] {}\n", content));
                }
                niuma_core::ExecutionEvent::ToolCall { tool, args, result } => {
                    let args_str = serde_json::to_string(args).unwrap_or_default();
                    if result.success {
                        let result_str = serde_json::to_string(&result.output).unwrap_or_default();
                        prompt.push_str(&format!(
                            "[Tool: {}] Called with {} -> Success: {}\n",
                            tool, args_str, result_str
                        ));
                    } else {
                        prompt.push_str(&format!(
                            "[Tool: {}] Called with {} -> Failed: {}\n",
                            tool,
                            args_str,
                            result.error.as_deref().unwrap_or("Unknown error")
                        ));
                    }
                }
                niuma_core::ExecutionEvent::Clarification { question, answer } => {
                    prompt.push_str(&format!("[Clarification] Q: {} A: {}\n", question, answer));
                }
                niuma_core::ExecutionEvent::Error { step, error } => {
                    prompt.push_str(&format!("[Error@{}] {}\n", step, error));
                }
                niuma_core::ExecutionEvent::StepStarted { step_id } => {
                    prompt.push_str(&format!("[Step Started] {}\n", step_id));
                }
                niuma_core::ExecutionEvent::StepCompleted { step_id, result } => {
                    let status = if result.success { "Success" } else { "Failed" };
                    prompt.push_str(&format!(
                        "[Step {}] {}: {:?}\n",
                        step_id, status, result.output
                    ));
                }
            }
        }

        if let Some(goal) = &session.goal {
            prompt.push_str(&format!("\n## Original Goal\n{}\n", goal));
        }

        prompt
    }

    fn extract_content(response: &niuma_llm::ChatCompletionResponse) -> Result<String> {
        response
            .choices
            .first()
            .and_then(|c| match &c.message.content {
                niuma_llm::Content::Text(text) => Some(text.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::Clarifier("No content in response".to_string()))
    }

    fn parse_distilled_plan(content: &str) -> Result<ExecutionPlan> {
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
        struct RawPlan {
            steps: Vec<RawStep>,
            #[serde(alias = "estimated_duration_seconds", default)]
            estimated_duration_seconds: Option<u64>,
        }

        #[derive(serde::Deserialize)]
        struct RawStep {
            #[serde(alias = "id", default)]
            id: Option<String>,
            #[serde(alias = "tool", default)]
            tool: Option<String>,
            #[serde(alias = "args", default)]
            args: Option<serde_json::Value>,
        }

        let raw: RawPlan = serde_json::from_str(json_str)
            .map_err(|e| Error::Clarifier(format!("Failed to parse distilled plan: {}", e)))?;

        let steps: Vec<niuma_core::Step> = raw
            .steps
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                niuma_core::Step::new(
                    s.id.unwrap_or_else(|| format!("step_{}", i + 1)),
                    s.tool.unwrap_or_default(),
                    s.args.unwrap_or(serde_json::json!({})),
                )
            })
            .collect();

        let duration = raw
            .estimated_duration_seconds
            .map(std::time::Duration::from_secs);

        Ok(ExecutionPlan {
            steps,
            estimated_duration: duration,
        })
    }

    fn parse_evaluation(content: &str) -> Result<(Confidence, Option<String>)> {
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
        struct EvalResult {
            #[serde(alias = "confidence", default)]
            confidence: Option<String>,
            #[serde(alias = "feedback", default)]
            feedback: Option<String>,
        }

        let parsed: EvalResult = serde_json::from_str(json_str)
            .map_err(|e| Error::Clarifier(format!("Failed to parse evaluation: {}", e)))?;

        let conf = match parsed.confidence.as_deref() {
            Some("high") => Confidence::High,
            Some("medium") => Confidence::Medium,
            Some("low") | None => Confidence::Low,
            _ => Confidence::Low,
        };

        Ok((conf, parsed.feedback))
    }
}

/// Context for the clarification process.
#[derive(Debug)]
pub struct ClarifyContext {
    /// Information gathered from clarification questions.
    gathered: std::collections::HashMap<String, String>,
    /// All missing information to clarify (the full list).
    all_missing: Vec<MissingInfo>,
    /// Questions that have been answered (field names).
    answered: Vec<String>,
    /// The current question being asked.
    current_question: Option<String>,
    /// State of the clarification process.
    state: ClarifyState,
}

impl Default for ClarifyContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ClarifyContext {
    /// Creates a new clarification context.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gathered: std::collections::HashMap::new(),
            all_missing: Vec::new(),
            answered: Vec::new(),
            current_question: None,
            state: ClarifyState::Idle,
        }
    }

    /// Creates a clarification context with initial missing information.
    #[must_use]
    pub fn with_missing(missing: Vec<MissingInfo>) -> Self {
        Self {
            gathered: std::collections::HashMap::new(),
            all_missing: missing,
            answered: Vec::new(),
            current_question: None,
            state: ClarifyState::AwaitingAnswer,
        }
    }

    /// Returns the list of missing information that still needs to be answered.
    #[must_use]
    pub fn pending_missing(&self) -> Vec<MissingInfo> {
        self.all_missing
            .iter()
            .filter(|m| !self.answered.contains(&m.field))
            .cloned()
            .collect()
    }

    /// Returns the current question being asked.
    #[must_use]
    pub fn current_question(&self) -> Option<&str> {
        self.current_question.as_deref()
    }

    /// Sets the current question to ask the user.
    pub fn set_current_question(&mut self, question: String) {
        self.current_question = Some(question);
    }

    /// Returns gathered information for a field.
    #[must_use]
    pub fn get(&self, field: &str) -> Option<&str> {
        self.gathered.get(field).map(String::as_str)
    }

    /// Returns all gathered information.
    #[must_use]
    pub fn gathered_all(&self) -> &std::collections::HashMap<String, String> {
        &self.gathered
    }

    /// Returns the number of answered questions.
    #[must_use]
    pub fn answered_count(&self) -> usize {
        self.answered.len()
    }

    /// Returns the current state.
    #[must_use]
    pub fn state(&self) -> ClarifyState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clarify_context_new() {
        let ctx = ClarifyContext::new();
        assert!(ctx.pending_missing().is_empty());
        assert!(ctx.current_question().is_none());
    }

    #[test]
    fn test_clarify_context_with_missing() {
        let missing = vec![
            MissingInfo::new("url", "Need URL", "What is the URL?"),
            MissingInfo::new("format", "Need format", "What format?"),
        ];
        let ctx = ClarifyContext::with_missing(missing);
        assert_eq!(ctx.pending_missing().len(), 2);
        assert!(matches!(ctx.state(), ClarifyState::AwaitingAnswer));
    }

    #[test]
    fn test_parse_distilled_plan() {
        let json = r#"```json
        {
            "steps": [
                {"id": "step_1", "tool": "http", "args": {"url": "https://example.com"}},
                {"id": "step_2", "tool": "shell", "args": {"command": "echo done"}}
            ],
            "estimated_duration_seconds": 30
        }
        ```"#;

        let plan = Clarifier::parse_distilled_plan(json);
        assert!(plan.is_ok());

        let plan = plan.unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].tool, "http");
        assert_eq!(plan.steps[1].tool, "shell");
    }

    #[test]
    fn test_parse_distilled_plan_no_markdown() {
        let json = r#"{
            "steps": [
                {"tool": "http", "args": {"url": "https://example.com"}}
            ]
        }"#;

        let plan = Clarifier::parse_distilled_plan(json);
        assert!(plan.is_ok());
        assert_eq!(plan.unwrap().steps.len(), 1);
    }
}
