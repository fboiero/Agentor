//! External runner — spawns a subprocess (Python/Node/etc.) and parses its JSON output.
//!
//! # Contract
//!
//! External runners are invoked as:
//! ```bash
//! <command> --task <task-json-path> --task-dir <dir>
//! ```
//!
//! They must write a JSON `TaskResult` to stdout on success, or exit non-zero
//! with an error message on stderr on failure.

use super::{Runner, RunnerKind};
use crate::task::{Task, TaskResult};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct ExternalRunner {
    /// Command name (looked up in PATH) or absolute path.
    command: String,
    args: Vec<String>,
    kind: RunnerKind,
    name: String,
}

impl ExternalRunner {
    pub fn new(command: impl Into<String>, kind: RunnerKind, name: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            kind,
            name: name.into(),
        }
    }

    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
}

#[async_trait]
impl Runner for ExternalRunner {
    fn kind(&self) -> RunnerKind {
        self.kind
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, task: &Task, task_dir: &Path) -> anyhow::Result<TaskResult> {
        let started_at = Utc::now();

        // Serialize the task to a temp file so the external runner can read it
        let task_json = serde_json::to_string(task)?;
        let tmp = std::env::temp_dir().join(format!("argentor-bench-task-{}.json", task.id));
        std::fs::write(&tmp, task_json)?;

        let output = Command::new(&self.command)
            .args(&self.args)
            .arg("--task")
            .arg(&tmp)
            .arg("--task-dir")
            .arg(task_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let _ = std::fs::remove_file(&tmp);

        if !output.status.success() {
            return Ok(TaskResult {
                task_id: task.id.clone(),
                runner: self.name.clone(),
                started_at,
                ended_at: Utc::now(),
                output: String::new(),
                llm_calls: 0,
                input_tokens: 0,
                output_tokens: 0,
                tool_calls: 0,
                succeeded: false,
                error: Some(String::from_utf8_lossy(&output.stderr).to_string()),
                model: "external".into(),
                was_blocked: false,
                block_reason: None,
                prompt_tokens_sent: 0,
                tool_description_tokens: 0,
                context_history_tokens: 0,
            });
        }

        let result: TaskResult = serde_json::from_slice(&output.stdout)?;
        Ok(result)
    }
}
