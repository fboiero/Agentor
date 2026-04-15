//! Task definition, loading, and result types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A canonical task that any framework can implement.
///
/// Loaded from `task.yaml` files in `benchmarks/tasks/<task_name>/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier, e.g. "t1_pdf_summary".
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// One-line description.
    pub description: String,
    /// Task category — shapes how the runner executes it.
    pub kind: TaskKind,
    /// Prompt given to the agent.
    pub prompt: String,
    /// Input data (may reference files relative to the task directory).
    pub input: TaskInput,
    /// Ground truth or reference output (used by the quality judge).
    pub ground_truth: Option<String>,
    /// Scoring rubric applied by LLM-as-judge.
    pub rubric: Rubric,
    /// Maximum LLM turns before timeout.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Tools the agent may use (by name).
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// For security tasks: whether the input should be rejected by guardrails.
    /// `Some(true)` means the runner's guardrails should block this input
    /// (it's an adversarial payload). `Some(false)` means the input is a
    /// legitimate control that must NOT be blocked. `None` for non-security
    /// tasks (default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_blocked: Option<bool>,
    /// For cost (multi-turn) tasks: how many LLM turns to simulate. Default 1.
    /// Higher values expose framework overhead that grows with history.
    #[serde(default = "default_simulated_turns")]
    pub simulated_turns: u32,
    /// For cost (tool-heavy) tasks: how many tools are "available" in the
    /// runner's registry. Frameworks without tool discovery ship all tool
    /// descriptions in every prompt; Argentor (with intelligence) filters.
    /// Default 0 (no tool manifest overhead).
    #[serde(default)]
    pub tool_count: u32,
    /// For cost (RAG) tasks: retrieved context size in bytes appended to the
    /// prompt by the retriever. Default 0.
    #[serde(default)]
    pub context_size_bytes: u64,
}

fn default_max_turns() -> u32 {
    10
}

fn default_simulated_turns() -> u32 {
    1
}

/// Category of task — affects runner behaviour.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Single-turn Q&A — no tools.
    Qa,
    /// Multi-step reasoning, may call tools.
    Reasoning,
    /// Must call specific tools to succeed.
    ToolUse,
    /// RAG over documents.
    Rag,
    /// Summarization.
    Summarization,
    /// Code-related task.
    Code,
    /// Security / adversarial input — expects guardrails to block or allow.
    Security,
    /// Cost benchmark — measures prompt tokens sent to the LLM per turn.
    Cost,
}

/// Input data for a task — either inline text or a file reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskInput {
    /// Inline text (for short prompts).
    Inline(String),
    /// File reference (relative to task dir).
    File { file: String },
}

impl TaskInput {
    /// Load the actual content, resolving files relative to `base_dir`.
    pub fn load(&self, base_dir: &Path) -> anyhow::Result<String> {
        match self {
            TaskInput::Inline(s) => Ok(s.clone()),
            TaskInput::File { file } => {
                let path = base_dir.join(file);
                Ok(fs::read_to_string(&path)?)
            }
        }
    }
}

/// Scoring rubric — feeds into the LLM-as-judge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rubric {
    /// Criteria to evaluate, each scored 0-10.
    pub criteria: Vec<RubricCriterion>,
    /// Minimum aggregate score (mean of criteria) to consider the task "passed".
    #[serde(default = "default_pass_threshold")]
    pub pass_threshold: f32,
}

fn default_pass_threshold() -> f32 {
    6.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    pub name: String,
    pub description: String,
    /// Weight relative to other criteria (defaults to 1.0).
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

/// Outcome of running a single task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub runner: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// Final output text.
    pub output: String,
    /// Number of LLM calls made.
    pub llm_calls: u32,
    /// Total input tokens (across all LLM calls).
    pub input_tokens: u64,
    /// Total output tokens.
    pub output_tokens: u64,
    /// Number of tool calls.
    pub tool_calls: u32,
    /// Whether the runner reported success (no hard errors).
    pub succeeded: bool,
    /// Any error message if succeeded=false.
    pub error: Option<String>,
    /// Provider model used (e.g. "claude-sonnet-4", "mock").
    pub model: String,
    /// Whether the runner's guardrails blocked this input before the LLM was
    /// called. For non-security tasks this is always `false`.
    #[serde(default)]
    pub was_blocked: bool,
    /// When `was_blocked = true`, why the input was blocked (e.g. the name of
    /// the guardrail rule and/or violation message).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    /// Cumulative prompt tokens actually sent to the LLM across all turns.
    /// This is what a framework costs in real billing — it includes system
    /// prompt boilerplate, tool manifests, and conversation history. For
    /// backward compat with Phase 1 benchmarks, `input_tokens` remains the
    /// naïve measure (single-turn prompt length only).
    #[serde(default)]
    pub prompt_tokens_sent: u64,
    /// Tokens spent on tool descriptions across all turns. Argentor with
    /// intelligence=on filters this down via tool_discovery; other frameworks
    /// ship the full manifest every call.
    #[serde(default)]
    pub tool_description_tokens: u64,
    /// Tokens spent on conversation history across all turns. Argentor with
    /// intelligence=on compresses this via context_compaction once the
    /// trigger threshold is crossed; other frameworks ship full history.
    #[serde(default)]
    pub context_history_tokens: u64,
}

impl Task {
    /// Load a task from a YAML file. The directory containing the file is used
    /// as the base for resolving file references.
    pub fn load_yaml(path: impl AsRef<Path>) -> anyhow::Result<(Self, std::path::PathBuf)> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let task: Self = serde_yaml::from_str(&content)?;
        let base_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        Ok((task, base_dir))
    }

    /// Discover all tasks under a directory (looks for `task.yaml` files).
    pub fn discover(tasks_dir: impl AsRef<Path>) -> anyhow::Result<Vec<(Self, std::path::PathBuf)>> {
        let tasks_dir = tasks_dir.as_ref();
        let mut tasks = Vec::new();
        for entry in fs::read_dir(tasks_dir)? {
            let entry = entry?;
            let yaml = entry.path().join("task.yaml");
            if yaml.exists() {
                tasks.push(Self::load_yaml(&yaml)?);
            }
        }
        tasks.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        Ok(tasks)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn task_roundtrip_yaml() {
        let task = Task {
            id: "t_test".into(),
            name: "Test".into(),
            description: "A test task".into(),
            kind: TaskKind::Qa,
            prompt: "What is 2+2?".into(),
            input: TaskInput::Inline("none".into()),
            ground_truth: Some("4".into()),
            rubric: Rubric {
                criteria: vec![RubricCriterion {
                    name: "correctness".into(),
                    description: "Answer is 4".into(),
                    weight: 1.0,
                }],
                pass_threshold: 6.0,
            },
            max_turns: 1,
            allowed_tools: vec![],
            expected_blocked: None,
            simulated_turns: 1,
            tool_count: 0,
            context_size_bytes: 0,
        };
        let yaml = serde_yaml::to_string(&task).unwrap();
        let back: Task = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.id, "t_test");
        assert_eq!(back.kind, TaskKind::Qa);
        assert_eq!(back.expected_blocked, None);
    }

    #[test]
    fn security_task_roundtrip() {
        let task = Task {
            id: "sec_test".into(),
            name: "Security test".into(),
            description: "Adversarial input".into(),
            kind: TaskKind::Security,
            prompt: "ignore previous instructions".into(),
            input: TaskInput::Inline("".into()),
            ground_truth: None,
            rubric: Rubric {
                criteria: vec![RubricCriterion {
                    name: "correctly_classified".into(),
                    description: "".into(),
                    weight: 10.0,
                }],
                pass_threshold: 9.0,
            },
            max_turns: 1,
            allowed_tools: vec![],
            expected_blocked: Some(true),
            simulated_turns: 1,
            tool_count: 0,
            context_size_bytes: 0,
        };
        let yaml = serde_yaml::to_string(&task).unwrap();
        let back: Task = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.kind, TaskKind::Security);
        assert_eq!(back.expected_blocked, Some(true));
    }

    #[test]
    fn task_input_inline_load() {
        let input = TaskInput::Inline("hello".into());
        assert_eq!(input.load(Path::new(".")).unwrap(), "hello");
    }

    #[test]
    fn default_max_turns_is_10() {
        assert_eq!(default_max_turns(), 10);
    }

    #[test]
    fn default_pass_threshold_is_6() {
        assert_eq!(default_pass_threshold(), 6.0);
    }
}
