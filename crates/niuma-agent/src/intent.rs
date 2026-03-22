//! Intent parser for classifying user input.
//!
//! This module provides the [`IntentParser`] which classifies user input
//! into structured intents and execution strategies using an LLM.

use std::sync::Arc;

use niuma_core::{Confidence, MissingInfo, Session, UserIntent};
use niuma_llm::{ChatCompletionRequest, LLMProvider, Message};
use serde::Deserialize;
use tracing::{debug, instrument};

use super::{Error, Result};

/// The intent parser for classifying user input.
#[derive(Debug)]
pub struct IntentParser {
    llm: Arc<dyn LLMProvider>,
}

impl IntentParser {
    /// Creates a new intent parser with the given LLM provider.
    #[must_use]
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Classifies the user's input into an intent and execution strategy.
    ///
    /// # Arguments
    ///
    /// * `user_input` - The raw user input to classify
    ///
    /// # Returns
    ///
    /// An [`IntentClassification`] containing the classified intent,
    /// execution strategy, and confidence level.
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails or the response cannot be parsed.
    #[instrument(skip(self))]
    pub async fn classify(&self, user_input: &str) -> Result<IntentClassification> {
        debug!(input = user_input, "Classifying user intent");

        let system_prompt = Self::classification_system_prompt();
        let user_prompt = Self::build_classification_prompt(user_input);

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![Message::system(system_prompt), Message::user(user_prompt)],
        )
        .with_max_tokens(512);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        debug!(raw_response = content, "Received classification response");

        let parsed = Self::parse_classification(&content)?;
        debug!(
            intent = ?parsed.intent,
            confidence = ?parsed.confidence,
            "Intent classified"
        );

        Ok(parsed)
    }

    /// Classifies the user's input using the current session context.
    ///
    /// This method is useful when the LLM needs to consider previous
    /// interactions in the session.
    ///
    /// # Arguments
    ///
    /// * `user_input` - The raw user input to classify
    /// * `session` - The current session with context
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails or the response cannot be parsed.
    #[instrument(skip(self, session))]
    pub async fn classify_with_session(
        &self,
        user_input: &str,
        session: &Session,
    ) -> Result<IntentClassification> {
        let context = Self::build_session_context(session);
        let system_prompt = format!(
            "{}\n\n## Current Session Context\n{}",
            Self::classification_system_prompt(),
            context
        );

        let user_prompt = Self::build_classification_prompt(user_input);

        let request = ChatCompletionRequest::new(
            self.llm.default_model(),
            vec![Message::system(system_prompt), Message::user(user_prompt)],
        )
        .with_max_tokens(512);

        let response = self.llm.complete(&request).await?;
        let content = Self::extract_content(&response)?;

        let parsed = Self::parse_classification(&content)?;
        debug!(
            intent = ?parsed.intent,
            confidence = ?parsed.confidence,
            "Intent classified with session context"
        );

        Ok(parsed)
    }

    fn classification_system_prompt() -> &'static str {
        r#"You are an intent classifier for a task-processing agent.

Given a user's input, classify it into one of the following intents:

1. **ExecuteNow**: The user wants to execute a task immediately.
   - Contains: goal (what to do)
   - Example: "帮我查天气" (Check the weather for me)

2. **CreateScheduledTask**: The user wants to create a scheduled task with a specific schedule.
   - Contains: goal (what to do), schedule (cron expression or natural language)
   - Example: "每天早上9点提醒我开会" (Remind me of meetings every morning at 9)

3. **SaveAsScheduledTask**: The user wants to save an executed task as a scheduled task.
   - Contains: name (task name), schedule (when to run)
   - Example: "Save this as a daily task"

4. **Other**: Anything that doesn't fit the above categories.
   - Contains: description of what the user wants

Also assess your confidence:
- **High (>0.8)**: Clear intent, sufficient information
- **Medium (0.5-0.8)**: Mostly clear intent, minor ambiguity
- **Low (<0.5)**: Unclear intent, missing critical information

If confidence is not High, include a list of MissingInfo describing what needs clarification.

Output your classification as JSON with the following schema:
{
  "intent": "ExecuteNow|CreateScheduledTask|SaveAsScheduledTask|Other",
  "confidence": "high|medium|low",
  "confidence_score": 0.0-1.0,
  "goal": "description of what to do (for ExecuteNow/CreateScheduledTask)",
  "schedule": "schedule expression (for CreateScheduledTask/SaveAsScheduledTask)",
  "name": "task name (for SaveAsScheduledTask)",
  "other_description": "description (for Other intent)",
  "missing": [
    {
      "field": "field name",
      "reason": "why this is needed",
      "question": "question to ask user"
    }
  ]
}
"#
    }

    fn build_classification_prompt(user_input: &str) -> String {
        format!(
            r#"Classify the following user input:

User Input: "{}"
"#,
            user_input.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }

    fn build_session_context(session: &Session) -> String {
        if session.events.is_empty() {
            return "No previous context.".to_string();
        }

        let mut context = String::new();
        context.push_str("Recent conversation:\n");

        for event in session.events.iter().rev().take(10) {
            match event {
                niuma_core::ExecutionEvent::UserMessage { content } => {
                    context.push_str(&format!("User: {}\n", content));
                }
                niuma_core::ExecutionEvent::AgentMessage { content } => {
                    context.push_str(&format!("Agent: {}\n", content));
                }
                niuma_core::ExecutionEvent::ToolCall { tool, result, .. } => {
                    let status = if result.success {
                        "succeeded"
                    } else {
                        "failed"
                    };
                    context.push_str(&format!("Tool '{}' {}.\n", tool, status));
                }
                niuma_core::ExecutionEvent::Clarification { question, answer } => {
                    context.push_str(&format!("Clarification - Q: {} A: {}\n", question, answer));
                }
                _ => {}
            }
        }

        context
    }

    fn extract_content(response: &niuma_llm::ChatCompletionResponse) -> Result<String> {
        response
            .choices
            .first()
            .and_then(|c| match &c.message.content {
                niuma_llm::Content::Text(text) => Some(text.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::IntentParse("No content in response".to_string()))
    }

    fn parse_classification(content: &str) -> Result<IntentClassification> {
        let content = content.trim();

        // Try to extract JSON from the response (might be wrapped in markdown code blocks)
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

        let parsed: RawClassification = serde_json::from_str(json_str).map_err(|e| {
            Error::IntentParse(format!("Failed to parse classification JSON: {}", e))
        })?;

        let confidence = match parsed.confidence.as_deref() {
            Some("high") => Confidence::High,
            Some("medium") => Confidence::Medium,
            Some("low") | None => Confidence::Low,
            _ => Confidence::Low,
        };

        let intent = match parsed.intent.as_deref() {
            Some("ExecuteNow") | None => UserIntent::ExecuteNow {
                goal: parsed.goal.unwrap_or_default(),
            },
            Some("CreateScheduledTask") => UserIntent::CreateScheduledTask {
                goal: parsed.goal.unwrap_or_default(),
                schedule: parsed.schedule.unwrap_or_default(),
            },
            Some("SaveAsScheduledTask") => UserIntent::SaveAsScheduledTask {
                name: parsed.name.unwrap_or_default(),
                schedule: parsed.schedule.unwrap_or_default(),
            },
            Some("Other") | Some(_) => {
                UserIntent::Other(parsed.other_description.unwrap_or_default())
            }
        };

        let missing: Vec<MissingInfo> = parsed
            .missing
            .unwrap_or_default()
            .into_iter()
            .map(|m| MissingInfo::new(m.field, m.reason, m.question))
            .collect();

        let strategy = if confidence == Confidence::High || missing.is_empty() {
            niuma_core::ExecutionStrategy::Autonomous
        } else {
            niuma_core::ExecutionStrategy::Clarifying { missing }
        };

        Ok(IntentClassification {
            intent,
            strategy,
            confidence,
        })
    }
}

/// Result of intent classification.
#[derive(Debug, Clone)]
pub struct IntentClassification {
    /// The classified user intent.
    pub intent: UserIntent,
    /// The execution strategy determined by the classifier.
    pub strategy: niuma_core::ExecutionStrategy,
    /// The confidence level of the classification.
    pub confidence: Confidence,
}

#[derive(Debug, Deserialize)]
struct RawClassification {
    #[serde(alias = "intent", default)]
    intent: Option<String>,
    #[serde(alias = "confidence", default)]
    confidence: Option<String>,
    #[serde(alias = "confidence_score", default)]
    #[allow(dead_code)]
    confidence_score: Option<f64>,
    #[serde(alias = "goal", default)]
    goal: Option<String>,
    #[serde(alias = "schedule", default)]
    schedule: Option<String>,
    #[serde(alias = "name", default)]
    name: Option<String>,
    #[serde(alias = "other_description", alias = "description", default)]
    other_description: Option<String>,
    #[serde(alias = "missing", default)]
    missing: Option<Vec<RawMissingInfo>>,
}

#[derive(Debug, Deserialize)]
struct RawMissingInfo {
    #[serde(alias = "field", default)]
    field: String,
    #[serde(alias = "reason", default)]
    reason: String,
    #[serde(alias = "question", default)]
    question: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_classification_execute_now() {
        let json = r#"{
            "intent": "ExecuteNow",
            "confidence": "high",
            "confidence_score": 0.9,
            "goal": "Check the weather"
        }"#;

        let result = IntentParser::parse_classification(json);
        assert!(result.is_ok());

        let classification = result.unwrap();
        assert!(matches!(
            classification.intent,
            UserIntent::ExecuteNow { .. }
        ));
        assert!(matches!(
            classification.strategy,
            niuma_core::ExecutionStrategy::Autonomous
        ));
    }

    #[test]
    fn test_parse_classification_with_missing() {
        let json = r#"{
            "intent": "ExecuteNow",
            "confidence": "low",
            "confidence_score": 0.3,
            "goal": "Fetch data from a website",
            "missing": [
                {
                    "field": "url",
                    "reason": "Need URL to fetch data",
                    "question": "What is the URL?"
                }
            ]
        }"#;

        let result = IntentParser::parse_classification(json);
        assert!(result.is_ok());

        let classification = result.unwrap();
        assert!(matches!(classification.confidence, Confidence::Low));
        assert!(matches!(
            classification.strategy,
            niuma_core::ExecutionStrategy::Clarifying { .. }
        ));
    }

    #[test]
    fn test_parse_classification_create_scheduled() {
        let json = r#"{
            "intent": "CreateScheduledTask",
            "confidence": "high",
            "goal": "Export data daily",
            "schedule": "0 9 * * *"
        }"#;

        let result = IntentParser::parse_classification(json);
        assert!(result.is_ok());

        let classification = result.unwrap();
        assert!(matches!(
            classification.intent,
            UserIntent::CreateScheduledTask { .. }
        ));
    }

    #[test]
    fn test_parse_classification_invalid() {
        let json = "not valid json at all";
        let result = IntentParser::parse_classification(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_classification_markdown_wrap() {
        let json = r#"```json
        {
            "intent": "ExecuteNow",
            "confidence": "high",
            "goal": "Run a test"
        }
        ```"#;

        let result = IntentParser::parse_classification(json);
        assert!(result.is_ok());
    }
}
