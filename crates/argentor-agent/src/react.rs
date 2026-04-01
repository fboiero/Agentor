//! ReAct (Reasoning + Acting) engine for structured agent reasoning.
//!
//! Implements the Think -> Act -> Observe -> Reflect cycle, producing structured
//! reasoning traces that can be inspected, logged, and used for debugging.
//!
//! The engine is **stateless** — all state lives in [`ReActTrace`]. This makes it
//! easy to serialize, replay, and inspect reasoning chains.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────┐     ┌─────────┐     ┌──────────┐     ┌──────────┐
//! │  Think  │ --> │   Act   │ --> │ Observe  │ --> │ Reflect  │ --┐
//! └─────────┘     └─────────┘     └──────────┘     └──────────┘   │
//!      ^                                                          │
//!      └──────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::warn;

/// A single step in the ReAct reasoning trace.
///
/// Each step captures the full Think -> Act -> Observe -> Reflect cycle,
/// including timing and confidence information for observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActStep {
    /// Sequential step number (1-based).
    pub step_number: u32,
    /// What the agent is thinking at this step.
    pub thought: String,
    /// The action the agent decided to take.
    pub action: ReActAction,
    /// The result observed after executing the action.
    pub observation: String,
    /// Optional self-reflection on whether the approach is working.
    pub reflection: Option<String>,
    /// Confidence level for this step (0.0 to 1.0).
    pub confidence: f32,
    /// Time taken for this step in milliseconds.
    pub duration_ms: u64,
}

/// Actions the agent can take in a ReAct step.
///
/// Terminal actions ([`ReActAction::Respond`] and [`ReActAction::Clarify`])
/// signal the end of the reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReActAction {
    /// Use a specific tool with the given arguments.
    ToolUse {
        /// Name of the tool to invoke.
        tool_name: String,
        /// JSON arguments to pass to the tool.
        arguments: serde_json::Value,
    },
    /// Respond to the user (terminal action).
    Respond {
        /// The final response content.
        content: String,
    },
    /// Ask the user for clarification (terminal action).
    Clarify {
        /// The clarification question.
        question: String,
    },
    /// Decompose the current task into sub-tasks.
    Decompose {
        /// List of sub-task descriptions.
        subtasks: Vec<String>,
    },
    /// No external action needed — pure reasoning step.
    Think,
}

impl ReActAction {
    /// Returns `true` if this action terminates the reasoning chain.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ReActAction::Respond { .. } | ReActAction::Clarify { .. }
        )
    }
}

impl fmt::Display for ReActAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReActAction::ToolUse { tool_name, .. } => write!(f, "ToolUse({tool_name})"),
            ReActAction::Respond { .. } => write!(f, "Respond"),
            ReActAction::Clarify { .. } => write!(f, "Clarify"),
            ReActAction::Decompose { subtasks } => {
                write!(f, "Decompose({} subtasks)", subtasks.len())
            }
            ReActAction::Think => write!(f, "Think"),
        }
    }
}

/// Configuration for the ReAct engine.
///
/// Controls the behavior of the reasoning loop including maximum steps,
/// reflection, and confidence thresholds.
#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// Maximum number of reasoning steps before stopping.
    pub max_steps: u32,
    /// Whether to include self-reflection after each step.
    pub enable_reflection: bool,
    /// Minimum confidence threshold to proceed (0.0 to 1.0).
    /// Steps below this threshold will trigger early termination.
    pub min_confidence: f32,
    /// Whether to emit structured traces for observability.
    pub emit_traces: bool,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            enable_reflection: true,
            min_confidence: 0.3,
            emit_traces: true,
        }
    }
}

/// The full reasoning trace produced by a ReAct reasoning process.
///
/// Contains the complete history of steps, the original task, and the final
/// outcome. Designed for serialization, logging, and post-hoc analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActTrace {
    /// The original task or query that triggered the reasoning.
    pub task: String,
    /// All reasoning steps executed.
    pub steps: Vec<ReActStep>,
    /// The final outcome of the reasoning process.
    pub outcome: ReActOutcome,
    /// Total wall-clock time in milliseconds.
    pub total_duration_ms: u64,
}

/// Outcome of a ReAct reasoning process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReActOutcome {
    /// Reasoning completed successfully with a response.
    Success {
        /// The final response to the user.
        response: String,
    },
    /// The agent needs more information from the user.
    NeedsClarification {
        /// The clarification question.
        question: String,
    },
    /// The maximum number of steps was reached without a conclusion.
    MaxStepsReached {
        /// Partial progress or best-effort response.
        partial: String,
    },
    /// An error occurred during reasoning.
    Error {
        /// Description of the error.
        message: String,
    },
}

/// The ReAct engine that drives structured reasoning.
///
/// The engine is **stateless** — it provides methods for building prompts,
/// parsing LLM outputs into structured steps, and managing the reasoning
/// lifecycle. All mutable state lives in [`ReActTrace`].
pub struct ReActEngine {
    config: ReActConfig,
}

impl ReActEngine {
    /// Create a new ReAct engine with the given configuration.
    pub fn new(config: ReActConfig) -> Self {
        Self { config }
    }

    /// Create a new ReAct engine with default configuration.
    pub fn with_defaults() -> Self {
        Self {
            config: ReActConfig::default(),
        }
    }

    /// Return a reference to the engine's configuration.
    pub fn config(&self) -> &ReActConfig {
        &self.config
    }

    /// Build the ReAct system prompt that instructs the LLM to reason in structured steps.
    ///
    /// Appends ReAct-specific instructions to the provided base system prompt,
    /// including the list of available tools the agent may invoke.
    pub fn build_system_prompt(&self, base_prompt: &str, available_tools: &[String]) -> String {
        let tools_list = if available_tools.is_empty() {
            "No tools are currently available.".to_string()
        } else {
            format!("Available tools: {}", available_tools.join(", "))
        };

        let reflection_instruction = if self.config.enable_reflection {
            "\n\nAfter each observation, reflect on whether your approach is working \
             and whether you should adjust your strategy. Include a \"reflection\" field \
             in your JSON output."
        } else {
            ""
        };

        format!(
            "{base_prompt}\n\n\
             ## ReAct Reasoning Protocol\n\n\
             You must reason step by step. For each step, output a single JSON block \
             (and nothing else) with this structure:\n\n\
             ```json\n\
             {{\n  \
               \"thought\": \"What you are thinking and why\",\n  \
               \"action\": {{\"type\": \"tool_use\", \"tool_name\": \"name\", \"arguments\": {{}}}},\n  \
               \"confidence\": 0.8\n\
             }}\n\
             ```\n\n\
             ### Action types:\n\
             - `{{\"type\": \"tool_use\", \"tool_name\": \"...\", \"arguments\": {{...}}}}` — invoke a tool\n\
             - `{{\"type\": \"respond\", \"content\": \"...\"}}` — final response to the user\n\
             - `{{\"type\": \"clarify\", \"question\": \"...\"}}` — ask the user for more information\n\
             - `{{\"type\": \"decompose\", \"subtasks\": [\"...\", \"...\"]}}` — break into sub-tasks\n\
             - `{{\"type\": \"think\"}}` — pure reasoning, no external action\n\n\
             {tools_list}\n\n\
             Confidence is a float between 0.0 and 1.0 indicating how confident you are \
             in this step. If confidence drops below {min_conf}, stop and report what you have.{reflection}",
            min_conf = self.config.min_confidence,
            reflection = reflection_instruction,
        )
    }

    /// Parse a ReAct step from raw LLM output.
    ///
    /// Attempts to extract a JSON block from the output. If the output does not
    /// contain valid JSON, falls back to treating the entire output as a `Think`
    /// action with the raw text as the thought.
    pub fn parse_step(&self, llm_output: &str, step_number: u32) -> Result<ReActStep, String> {
        let json_str = extract_json_block(llm_output);

        let parsed: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => {
                // Fallback: treat the entire output as a Think step
                warn!(
                    step = step_number,
                    "LLM output is not valid JSON, falling back to Think action"
                );
                return Ok(ReActStep {
                    step_number,
                    thought: llm_output.trim().to_string(),
                    action: ReActAction::Think,
                    observation: String::new(),
                    reflection: None,
                    confidence: 0.5,
                    duration_ms: 0,
                });
            }
        };

        let thought = parsed
            .get("thought")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let confidence = parsed
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .map(|f| f as f32)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let reflection = parsed
            .get("reflection")
            .and_then(|v| v.as_str())
            .map(String::from);

        let action = parse_action(&parsed)?;

        Ok(ReActStep {
            step_number,
            thought,
            action,
            observation: String::new(),
            reflection,
            confidence,
            duration_ms: 0,
        })
    }

    /// Determine if the reasoning loop should continue based on the current trace.
    ///
    /// Returns `false` (stop) if:
    /// - The maximum number of steps has been reached
    /// - The last step's action is terminal (Respond or Clarify)
    /// - The last step's confidence is below the minimum threshold
    pub fn should_continue(&self, trace: &ReActTrace) -> bool {
        // Check max steps
        if trace.steps.len() as u32 >= self.config.max_steps {
            return false;
        }

        // Check if the last step had a terminal action
        if let Some(last_step) = trace.steps.last() {
            if last_step.action.is_terminal() {
                return false;
            }

            // Check confidence threshold
            if last_step.confidence < self.config.min_confidence {
                return false;
            }
        }

        true
    }

    /// Generate a reflection prompt for the agent to evaluate its last step.
    ///
    /// Used when `enable_reflection` is `true` to ask the LLM to assess whether
    /// the current approach is working.
    pub fn reflection_prompt(&self, step: &ReActStep) -> String {
        format!(
            "Reflect on step {}: You thought '{}', acted with {}, and observed '{}'. \
             Was this the right approach? Should you adjust your strategy?",
            step.step_number, step.thought, step.action, step.observation
        )
    }

    /// Summarize a completed trace into a human-readable reasoning chain.
    ///
    /// Useful for logging, debugging, or injecting as context into subsequent
    /// conversations.
    pub fn summarize_trace(&self, trace: &ReActTrace) -> String {
        let mut lines = Vec::with_capacity(trace.steps.len() + 2);

        lines.push(format!("Task: {}", trace.task));
        lines.push(String::new());

        for step in &trace.steps {
            let mut step_line = format!(
                "Step {}: Thought: '{}', Action: {}, Observation: '{}'",
                step.step_number, step.thought, step.action, step.observation
            );

            if let Some(ref reflection) = step.reflection {
                step_line.push_str(&format!(", Reflection: '{reflection}'"));
            }

            step_line.push_str(&format!(
                " [confidence={:.2}, {}ms]",
                step.confidence, step.duration_ms
            ));

            lines.push(step_line);
        }

        lines.push(String::new());

        let outcome_line = match &trace.outcome {
            ReActOutcome::Success { response } => format!("Outcome: Success — {response}"),
            ReActOutcome::NeedsClarification { question } => {
                format!("Outcome: Needs clarification — {question}")
            }
            ReActOutcome::MaxStepsReached { partial } => {
                format!("Outcome: Max steps reached — {partial}")
            }
            ReActOutcome::Error { message } => format!("Outcome: Error — {message}"),
        };
        lines.push(outcome_line);
        lines.push(format!("Total time: {}ms", trace.total_duration_ms));

        lines.join("\n")
    }

    /// Create a new empty trace for the given task.
    pub fn new_trace(&self, task: impl Into<String>) -> ReActTrace {
        ReActTrace {
            task: task.into(),
            steps: Vec::new(),
            outcome: ReActOutcome::Error {
                message: "Trace not yet completed".to_string(),
            },
            total_duration_ms: 0,
        }
    }
}

/// Extract a JSON block from LLM output.
///
/// Handles both raw JSON and JSON wrapped in markdown code fences.
fn extract_json_block(output: &str) -> &str {
    let trimmed = output.trim();

    // Try to find JSON inside ```json ... ``` fences
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7; // skip "```json"
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }

    // Try to find JSON inside ``` ... ``` fences (no language tag)
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        if let Some(end) = trimmed[json_start..].find("```") {
            let candidate = trimmed[json_start..json_start + end].trim();
            // Only use it if it looks like JSON
            if candidate.starts_with('{') {
                return candidate;
            }
        }
    }

    // Try to find a top-level JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }

    trimmed
}

/// Parse a [`ReActAction`] from a parsed JSON value.
fn parse_action(parsed: &serde_json::Value) -> Result<ReActAction, String> {
    let action_value = match parsed.get("action") {
        Some(v) => v,
        None => return Ok(ReActAction::Think),
    };

    let action_type = action_value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("think");

    match action_type {
        "tool_use" => {
            let tool_name = action_value
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "tool_use action missing 'tool_name' field".to_string())?
                .to_string();

            let arguments = action_value
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            Ok(ReActAction::ToolUse {
                tool_name,
                arguments,
            })
        }
        "respond" => {
            let content = action_value
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            Ok(ReActAction::Respond { content })
        }
        "clarify" => {
            let question = action_value
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            Ok(ReActAction::Clarify { question })
        }
        "decompose" => {
            let subtasks = action_value
                .get("subtasks")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Ok(ReActAction::Decompose { subtasks })
        }
        "think" => Ok(ReActAction::Think),
        other => Err(format!("Unknown action type: '{other}'")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── 1. test_react_config_defaults ──────────────────────────────────

    #[test]
    fn test_react_config_defaults() {
        let config = ReActConfig::default();
        assert_eq!(config.max_steps, 10);
        assert!(config.enable_reflection);
        assert!((config.min_confidence - 0.3).abs() < f32::EPSILON);
        assert!(config.emit_traces);
    }

    // ── 2. test_react_step_serialization ───────────────────────────────

    #[test]
    fn test_react_step_serialization() {
        let step = ReActStep {
            step_number: 1,
            thought: "I need to search for the file".to_string(),
            action: ReActAction::Think,
            observation: "No observation yet".to_string(),
            reflection: Some("This seems right".to_string()),
            confidence: 0.85,
            duration_ms: 120,
        };

        let json = serde_json::to_string(&step).unwrap();
        let deserialized: ReActStep = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.step_number, 1);
        assert_eq!(deserialized.thought, "I need to search for the file");
        assert_eq!(deserialized.observation, "No observation yet");
        assert_eq!(
            deserialized.reflection,
            Some("This seems right".to_string())
        );
        assert!((deserialized.confidence - 0.85).abs() < f32::EPSILON);
        assert_eq!(deserialized.duration_ms, 120);
    }

    // ── 3. test_react_action_variants ──────────────────────────────────

    #[test]
    fn test_react_action_variants() {
        // ToolUse
        let action = ReActAction::ToolUse {
            tool_name: "search".to_string(),
            arguments: serde_json::json!({"query": "rust"}),
        };
        assert!(!action.is_terminal());
        assert!(format!("{action}").contains("search"));

        // Respond (terminal)
        let action = ReActAction::Respond {
            content: "Here is the answer".to_string(),
        };
        assert!(action.is_terminal());

        // Clarify (terminal)
        let action = ReActAction::Clarify {
            question: "What do you mean?".to_string(),
        };
        assert!(action.is_terminal());

        // Decompose
        let action = ReActAction::Decompose {
            subtasks: vec!["step 1".into(), "step 2".into()],
        };
        assert!(!action.is_terminal());
        assert!(format!("{action}").contains("2 subtasks"));

        // Think
        let action = ReActAction::Think;
        assert!(!action.is_terminal());
        assert_eq!(format!("{action}"), "Think");
    }

    // ── 4. test_parse_step_valid_json ──────────────────────────────────

    #[test]
    fn test_parse_step_valid_json() {
        let engine = ReActEngine::with_defaults();
        let input =
            r#"{"thought": "Let me analyze this", "action": {"type": "think"}, "confidence": 0.9}"#;

        let step = engine.parse_step(input, 1).unwrap();

        assert_eq!(step.step_number, 1);
        assert_eq!(step.thought, "Let me analyze this");
        assert!(matches!(step.action, ReActAction::Think));
        assert!((step.confidence - 0.9).abs() < f32::EPSILON);
        assert!(step.observation.is_empty());
    }

    // ── 5. test_parse_step_tool_use ────────────────────────────────────

    #[test]
    fn test_parse_step_tool_use() {
        let engine = ReActEngine::with_defaults();
        let input = r#"{
            "thought": "I should search for relevant documents",
            "action": {
                "type": "tool_use",
                "tool_name": "file_search",
                "arguments": {"path": "/src", "pattern": "*.rs"}
            },
            "confidence": 0.85
        }"#;

        let step = engine.parse_step(input, 3).unwrap();

        assert_eq!(step.step_number, 3);
        assert_eq!(step.thought, "I should search for relevant documents");
        match &step.action {
            ReActAction::ToolUse {
                tool_name,
                arguments,
            } => {
                assert_eq!(tool_name, "file_search");
                assert_eq!(arguments["path"], "/src");
                assert_eq!(arguments["pattern"], "*.rs");
            }
            other => panic!("Expected ToolUse, got {other:?}"),
        }
    }

    // ── 6. test_parse_step_respond ─────────────────────────────────────

    #[test]
    fn test_parse_step_respond() {
        let engine = ReActEngine::with_defaults();
        let input = r#"{
            "thought": "I have all the information needed",
            "action": {"type": "respond", "content": "The answer is 42."},
            "confidence": 0.95
        }"#;

        let step = engine.parse_step(input, 5).unwrap();

        assert!(step.action.is_terminal());
        match &step.action {
            ReActAction::Respond { content } => {
                assert_eq!(content, "The answer is 42.");
            }
            other => panic!("Expected Respond, got {other:?}"),
        }
    }

    // ── 7. test_parse_step_invalid_json_fallback ───────────────────────

    #[test]
    fn test_parse_step_invalid_json_fallback() {
        let engine = ReActEngine::with_defaults();
        let input = "I'm not sure what to do, let me think about this carefully.";

        let step = engine.parse_step(input, 2).unwrap();

        assert_eq!(step.step_number, 2);
        assert_eq!(
            step.thought,
            "I'm not sure what to do, let me think about this carefully."
        );
        assert!(matches!(step.action, ReActAction::Think));
        assert!((step.confidence - 0.5).abs() < f32::EPSILON);
    }

    // ── 8. test_should_continue_max_steps ──────────────────────────────

    #[test]
    fn test_should_continue_max_steps() {
        let config = ReActConfig {
            max_steps: 3,
            ..Default::default()
        };
        let engine = ReActEngine::new(config);

        let mut trace = engine.new_trace("test task");
        for i in 1..=3 {
            trace.steps.push(ReActStep {
                step_number: i,
                thought: format!("Step {i}"),
                action: ReActAction::Think,
                observation: String::new(),
                reflection: None,
                confidence: 0.8,
                duration_ms: 100,
            });
        }

        assert!(!engine.should_continue(&trace));
    }

    // ── 9. test_should_continue_terminal_action ────────────────────────

    #[test]
    fn test_should_continue_terminal_action() {
        let engine = ReActEngine::with_defaults();

        let mut trace = engine.new_trace("test task");
        trace.steps.push(ReActStep {
            step_number: 1,
            thought: "I know the answer".to_string(),
            action: ReActAction::Respond {
                content: "Done".to_string(),
            },
            observation: String::new(),
            reflection: None,
            confidence: 0.95,
            duration_ms: 50,
        });

        assert!(!engine.should_continue(&trace));
    }

    // ── 10. test_should_continue_low_confidence ────────────────────────

    #[test]
    fn test_should_continue_low_confidence() {
        let config = ReActConfig {
            min_confidence: 0.5,
            ..Default::default()
        };
        let engine = ReActEngine::new(config);

        let mut trace = engine.new_trace("test task");
        trace.steps.push(ReActStep {
            step_number: 1,
            thought: "I'm not sure about this".to_string(),
            action: ReActAction::Think,
            observation: String::new(),
            reflection: None,
            confidence: 0.2, // below 0.5 threshold
            duration_ms: 100,
        });

        assert!(!engine.should_continue(&trace));
    }

    // ── 11. test_build_system_prompt_includes_tools ────────────────────

    #[test]
    fn test_build_system_prompt_includes_tools() {
        let engine = ReActEngine::with_defaults();
        let tools = vec![
            "file_search".to_string(),
            "shell".to_string(),
            "http_fetch".to_string(),
        ];

        let prompt = engine.build_system_prompt("You are a helpful assistant.", &tools);

        assert!(prompt.contains("You are a helpful assistant."));
        assert!(prompt.contains("file_search"));
        assert!(prompt.contains("shell"));
        assert!(prompt.contains("http_fetch"));
        assert!(prompt.contains("ReAct Reasoning Protocol"));
        assert!(prompt.contains("tool_use"));
        assert!(prompt.contains("respond"));
        assert!(prompt.contains("clarify"));
        assert!(prompt.contains("decompose"));
        assert!(prompt.contains("think"));
        assert!(prompt.contains("confidence"));
    }

    // ── 12. test_summarize_trace ───────────────────────────────────────

    #[test]
    fn test_summarize_trace() {
        let engine = ReActEngine::with_defaults();

        let trace = ReActTrace {
            task: "Find the bug in main.rs".to_string(),
            steps: vec![
                ReActStep {
                    step_number: 1,
                    thought: "Need to read the file".to_string(),
                    action: ReActAction::ToolUse {
                        tool_name: "file_read".to_string(),
                        arguments: serde_json::json!({"path": "main.rs"}),
                    },
                    observation: "File contents loaded".to_string(),
                    reflection: Some("Good start".to_string()),
                    confidence: 0.8,
                    duration_ms: 150,
                },
                ReActStep {
                    step_number: 2,
                    thought: "Found an off-by-one error".to_string(),
                    action: ReActAction::Respond {
                        content: "The bug is on line 42".to_string(),
                    },
                    observation: String::new(),
                    reflection: None,
                    confidence: 0.95,
                    duration_ms: 80,
                },
            ],
            outcome: ReActOutcome::Success {
                response: "The bug is on line 42".to_string(),
            },
            total_duration_ms: 230,
        };

        let summary = engine.summarize_trace(&trace);

        assert!(summary.contains("Find the bug in main.rs"));
        assert!(summary.contains("Step 1"));
        assert!(summary.contains("Step 2"));
        assert!(summary.contains("file_read"));
        assert!(summary.contains("Good start"));
        assert!(summary.contains("Outcome: Success"));
        assert!(summary.contains("Total time: 230ms"));
    }

    // ── 13. test_reflection_prompt_format ──────────────────────────────

    #[test]
    fn test_reflection_prompt_format() {
        let engine = ReActEngine::with_defaults();

        let step = ReActStep {
            step_number: 3,
            thought: "Search for config files".to_string(),
            action: ReActAction::ToolUse {
                tool_name: "file_search".to_string(),
                arguments: serde_json::json!({"pattern": "*.toml"}),
            },
            observation: "Found 5 TOML files".to_string(),
            reflection: None,
            confidence: 0.7,
            duration_ms: 200,
        };

        let prompt = engine.reflection_prompt(&step);

        assert!(prompt.contains("step 3"));
        assert!(prompt.contains("Search for config files"));
        assert!(prompt.contains("file_search"));
        assert!(prompt.contains("Found 5 TOML files"));
        assert!(prompt.contains("right approach"));
        assert!(prompt.contains("adjust your strategy"));
    }

    // ── 14. test_react_trace_serialization ─────────────────────────────

    #[test]
    fn test_react_trace_serialization() {
        let trace = ReActTrace {
            task: "Deploy the application".to_string(),
            steps: vec![ReActStep {
                step_number: 1,
                thought: "Check deployment status".to_string(),
                action: ReActAction::ToolUse {
                    tool_name: "shell".to_string(),
                    arguments: serde_json::json!({"command": "kubectl get pods"}),
                },
                observation: "All pods running".to_string(),
                reflection: Some("System is healthy".to_string()),
                confidence: 0.9,
                duration_ms: 500,
            }],
            outcome: ReActOutcome::Success {
                response: "Deployment successful".to_string(),
            },
            total_duration_ms: 500,
        };

        let json = serde_json::to_string_pretty(&trace).unwrap();
        let deserialized: ReActTrace = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.task, "Deploy the application");
        assert_eq!(deserialized.steps.len(), 1);
        assert_eq!(deserialized.steps[0].step_number, 1);
        assert_eq!(deserialized.total_duration_ms, 500);

        match &deserialized.outcome {
            ReActOutcome::Success { response } => {
                assert_eq!(response, "Deployment successful");
            }
            other => panic!("Expected Success outcome, got {other:?}"),
        }
    }

    // ── Additional edge-case tests ─────────────────────────────────────

    #[test]
    fn test_parse_step_from_code_fence() {
        let engine = ReActEngine::with_defaults();
        let input = r#"Here is my reasoning:

```json
{"thought": "Inside a code fence", "action": {"type": "think"}, "confidence": 0.7}
```

That's my step."#;

        let step = engine.parse_step(input, 1).unwrap();
        assert_eq!(step.thought, "Inside a code fence");
        assert!(matches!(step.action, ReActAction::Think));
    }

    #[test]
    fn test_parse_step_clarify() {
        let engine = ReActEngine::with_defaults();
        let input = r#"{
            "thought": "I don't have enough info",
            "action": {"type": "clarify", "question": "Which file do you mean?"},
            "confidence": 0.4
        }"#;

        let step = engine.parse_step(input, 1).unwrap();
        assert!(step.action.is_terminal());
        match &step.action {
            ReActAction::Clarify { question } => {
                assert_eq!(question, "Which file do you mean?");
            }
            other => panic!("Expected Clarify, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_step_decompose() {
        let engine = ReActEngine::with_defaults();
        let input = r#"{
            "thought": "This is a complex task",
            "action": {"type": "decompose", "subtasks": ["read file", "analyze", "report"]},
            "confidence": 0.75
        }"#;

        let step = engine.parse_step(input, 1).unwrap();
        match &step.action {
            ReActAction::Decompose { subtasks } => {
                assert_eq!(subtasks.len(), 3);
                assert_eq!(subtasks[0], "read file");
                assert_eq!(subtasks[1], "analyze");
                assert_eq!(subtasks[2], "report");
            }
            other => panic!("Expected Decompose, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_step_missing_action_defaults_to_think() {
        let engine = ReActEngine::with_defaults();
        let input = r#"{"thought": "Just thinking", "confidence": 0.6}"#;

        let step = engine.parse_step(input, 1).unwrap();
        assert!(matches!(step.action, ReActAction::Think));
    }

    #[test]
    fn test_parse_step_unknown_action_type_errors() {
        let engine = ReActEngine::with_defaults();
        let input = r#"{"thought": "hmm", "action": {"type": "fly_to_moon"}, "confidence": 0.1}"#;

        let result = engine.parse_step(input, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("fly_to_moon"));
    }

    #[test]
    fn test_parse_step_tool_use_missing_tool_name_errors() {
        let engine = ReActEngine::with_defaults();
        let input =
            r#"{"thought": "use a tool", "action": {"type": "tool_use"}, "confidence": 0.5}"#;

        let result = engine.parse_step(input, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("tool_name"));
    }

    #[test]
    fn test_should_continue_empty_trace() {
        let engine = ReActEngine::with_defaults();
        let trace = engine.new_trace("test");
        // Empty trace — should continue
        assert!(engine.should_continue(&trace));
    }

    #[test]
    fn test_confidence_clamped_to_range() {
        let engine = ReActEngine::with_defaults();

        // Confidence above 1.0 should be clamped
        let input = r#"{"thought": "very sure", "action": {"type": "think"}, "confidence": 1.5}"#;
        let step = engine.parse_step(input, 1).unwrap();
        assert!((step.confidence - 1.0).abs() < f32::EPSILON);

        // Negative confidence should be clamped to 0.0
        let input = r#"{"thought": "not sure", "action": {"type": "think"}, "confidence": -0.5}"#;
        let step = engine.parse_step(input, 1).unwrap();
        assert!(step.confidence.abs() < f32::EPSILON);
    }

    #[test]
    fn test_build_system_prompt_no_tools() {
        let engine = ReActEngine::with_defaults();
        let prompt = engine.build_system_prompt("Base prompt.", &[]);
        assert!(prompt.contains("No tools are currently available"));
    }

    #[test]
    fn test_build_system_prompt_reflection_disabled() {
        let config = ReActConfig {
            enable_reflection: false,
            ..Default::default()
        };
        let engine = ReActEngine::new(config);
        let prompt = engine.build_system_prompt("Base.", &["tool1".to_string()]);
        assert!(!prompt.contains("reflect"));
    }

    #[test]
    fn test_new_trace_initial_state() {
        let engine = ReActEngine::with_defaults();
        let trace = engine.new_trace("my task");
        assert_eq!(trace.task, "my task");
        assert!(trace.steps.is_empty());
        assert_eq!(trace.total_duration_ms, 0);
        match &trace.outcome {
            ReActOutcome::Error { message } => {
                assert!(message.contains("not yet completed"));
            }
            other => panic!("Expected Error outcome, got {other:?}"),
        }
    }

    #[test]
    fn test_react_outcome_serialization_variants() {
        let outcomes = vec![
            ReActOutcome::Success {
                response: "done".to_string(),
            },
            ReActOutcome::NeedsClarification {
                question: "what?".to_string(),
            },
            ReActOutcome::MaxStepsReached {
                partial: "partial result".to_string(),
            },
            ReActOutcome::Error {
                message: "oops".to_string(),
            },
        ];

        for outcome in &outcomes {
            let json = serde_json::to_string(outcome).unwrap();
            let deserialized: ReActOutcome = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn test_summarize_trace_all_outcomes() {
        let engine = ReActEngine::with_defaults();

        let outcomes = vec![
            (
                ReActOutcome::NeedsClarification {
                    question: "Which one?".to_string(),
                },
                "Needs clarification",
            ),
            (
                ReActOutcome::MaxStepsReached {
                    partial: "Halfway done".to_string(),
                },
                "Max steps reached",
            ),
            (
                ReActOutcome::Error {
                    message: "Something broke".to_string(),
                },
                "Error",
            ),
        ];

        for (outcome, expected_text) in outcomes {
            let trace = ReActTrace {
                task: "test".to_string(),
                steps: vec![],
                outcome,
                total_duration_ms: 0,
            };

            let summary = engine.summarize_trace(&trace);
            assert!(
                summary.contains(expected_text),
                "Summary should contain '{expected_text}': {summary}"
            );
        }
    }
}
