use agentor_core::{AgentorResult, ToolCall, ToolResult};
use agentor_security::Capability;
use agentor_skills::skill::{Skill, SkillDescriptor};
use async_trait::async_trait;
use std::time::Duration;
use tracing::{info, warn};

/// Shell execution skill. Runs commands in a sandboxed subprocess.
/// The command must match the allowed_commands in the ShellExec capability.
pub struct ShellSkill {
    descriptor: SkillDescriptor,
}

impl ShellSkill {
    pub fn new() -> Self {
        Self {
            descriptor: SkillDescriptor {
                name: "shell".to_string(),
                description: "Execute a shell command. Only allowed commands can be run.".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds (default: 30, max: 300)",
                            "default": 30
                        }
                    },
                    "required": ["command"]
                }),
                required_capabilities: vec![Capability::ShellExec {
                    allowed_commands: vec![], // Configured at runtime
                }],
            },
        }
    }
}

impl Default for ShellSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ShellSkill {
    fn descriptor(&self) -> &SkillDescriptor {
        &self.descriptor
    }

    async fn execute(&self, call: ToolCall) -> AgentorResult<ToolResult> {
        let command = call.arguments["command"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        if command.is_empty() {
            return Ok(ToolResult::error(&call.id, "Empty command"));
        }

        let timeout_secs = call.arguments["timeout_secs"]
            .as_u64()
            .unwrap_or(30)
            .min(300);

        info!(command = %command, timeout = timeout_secs, "Executing shell command");

        // Sanitize: block dangerous patterns
        let dangerous = ["rm -rf /", "mkfs", "dd if=", ":(){ :|:& };:"];
        for pattern in &dangerous {
            if command.contains(pattern) {
                warn!(command = %command, "Blocked dangerous command");
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Command blocked: contains dangerous pattern '{}'", pattern),
                ));
            }
        }

        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let response = serde_json::json!({
                    "exit_code": exit_code,
                    "stdout": truncate_output(&stdout, 50_000),
                    "stderr": truncate_output(&stderr, 10_000),
                });

                if output.status.success() {
                    Ok(ToolResult::success(&call.id, response.to_string()))
                } else {
                    Ok(ToolResult::error(&call.id, response.to_string()))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(
                &call.id,
                format!("Failed to execute command: {}", e),
            )),
            Err(_) => Ok(ToolResult::error(
                &call.id,
                format!("Command timed out after {}s", timeout_secs),
            )),
        }
    }
}

fn truncate_output(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... [truncated, {} total bytes]", &s[..max_len], s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shell_echo() {
        let skill = ShellSkill::new();
        let call = ToolCall {
            id: "test_1".to_string(),
            name: "shell".to_string(),
            arguments: serde_json::json!({"command": "echo hello"}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_blocks_dangerous() {
        let skill = ShellSkill::new();
        let call = ToolCall {
            id: "test_2".to_string(),
            name: "shell".to_string(),
            arguments: serde_json::json!({"command": "rm -rf /"}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("blocked"));
    }

    #[tokio::test]
    async fn test_shell_timeout() {
        let skill = ShellSkill::new();
        let call = ToolCall {
            id: "test_3".to_string(),
            name: "shell".to_string(),
            arguments: serde_json::json!({"command": "sleep 10", "timeout_secs": 1}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("timed out"));
    }

    #[tokio::test]
    async fn test_shell_empty_command() {
        let skill = ShellSkill::new();
        let call = ToolCall {
            id: "test_4".to_string(),
            name: "shell".to_string(),
            arguments: serde_json::json!({"command": ""}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
    }
}
