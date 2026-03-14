use super::LlmBackend;
use crate::config::ModelConfig;
use crate::llm::LlmResponse;
use crate::stream::StreamEvent;
use agentor_core::{AgentorError, AgentorResult, Message, Role};
use agentor_skills::SkillDescriptor;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Claude Code CLI backend.
///
/// Runs the `claude` CLI in headless mode (`-p --output-format json`).
/// Uses the user's existing Claude Code subscription — no API key needed.
pub struct ClaudeCodeBackend {
    config: ModelConfig,
}

impl ClaudeCodeBackend {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl LlmBackend for ClaudeCodeBackend {
    async fn chat(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        _tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse> {
        // Build the prompt from messages (last user message is the main prompt)
        let prompt = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if prompt.is_empty() {
            return Err(AgentorError::Agent(
                "No user message found for ClaudeCode provider".into(),
            ));
        }

        let mut cmd = tokio::process::Command::new("claude");
        cmd.arg("-p").arg(&prompt);
        cmd.arg("--output-format").arg("json");
        cmd.arg("--max-turns")
            .arg(self.config.max_turns.to_string());

        if !self.config.model_id.is_empty() && self.config.model_id != "default" {
            cmd.arg("--model").arg(&self.config.model_id);
        }

        if let Some(sys) = system_prompt {
            cmd.arg("--append-system-prompt").arg(sys);
        }

        cmd.arg("--permission-mode").arg("plan");
        cmd.arg("--no-session-persistence");

        tracing::info!(prompt_len = prompt.len(), "ClaudeCode: spawning claude CLI");

        let output = cmd.output().await.map_err(|e| {
            AgentorError::Agent(format!(
                "Failed to run 'claude' CLI. Is Claude Code installed? Error: {e}"
            ))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Err(AgentorError::Http(format!(
                "Claude Code CLI failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }

        let result_json: serde_json::Value = stdout
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str(line).ok())
            .ok_or_else(|| {
                AgentorError::Agent(format!(
                    "Could not parse Claude Code output as JSON. stdout: {}",
                    &stdout[..stdout.len().min(500)]
                ))
            })?;

        let is_error = result_json["is_error"].as_bool().unwrap_or(false);
        let result_text = result_json["result"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        if is_error {
            return Err(AgentorError::Agent(format!(
                "Claude Code error: {result_text}"
            )));
        }

        if let Some(cost) = result_json["total_cost_usd"].as_f64() {
            let input_tokens = result_json["usage"]["input_tokens"].as_u64().unwrap_or(0);
            let output_tokens = result_json["usage"]["output_tokens"].as_u64().unwrap_or(0);
            tracing::info!(
                cost_usd = cost,
                input_tokens = input_tokens,
                output_tokens = output_tokens,
                num_turns = result_json["num_turns"].as_u64().unwrap_or(0),
                "ClaudeCode: response received"
            );
        }

        Ok(LlmResponse::Done(result_text))
    }

    async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        JoinHandle<AgentorResult<LlmResponse>>,
    )> {
        // ClaudeCode doesn't support streaming natively in subprocess mode,
        // so we simulate it with a single result.
        let response = self.chat(system_prompt, messages, tools).await?;
        let (tx, rx) = mpsc::channel::<StreamEvent>(16);
        let handle = tokio::spawn(async move {
            let _ = tx.send(StreamEvent::Done).await;
            Ok(response)
        });
        Ok((rx, handle))
    }
}
