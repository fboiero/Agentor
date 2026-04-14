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
}

fn default_max_turns() -> u32 {
    10
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
        };
        let yaml = serde_yaml::to_string(&task).unwrap();
        let back: Task = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.id, "t_test");
        assert_eq!(back.kind, TaskKind::Qa);
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
