use agentor_core::{AgentorError, AgentorResult, Message, Role, ToolCall};
use crate::config::{LlmProvider, ModelConfig};
use agentor_skills::SkillDescriptor;
use serde::Serialize;

/// Response from the LLM — either text content or a tool call request.
#[derive(Debug)]
pub enum LlmResponse {
    Text(String),
    ToolUse {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    Done(String),
}

/// LLM client that handles API calls to Claude and OpenAI.
pub struct LlmClient {
    config: ModelConfig,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(config: ModelConfig) -> Self {
        let http = reqwest::Client::new();
        Self { config, http }
    }

    pub async fn chat(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse> {
        match self.config.provider {
            LlmProvider::Claude => self.chat_claude(system_prompt, messages, tools).await,
            LlmProvider::OpenAi => self.chat_openai(system_prompt, messages, tools).await,
        }
    }

    async fn chat_claude(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse> {
        let url = format!("{}/v1/messages", self.config.base_url());

        let api_messages: Vec<ClaudeMessage> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| ClaudeMessage {
                role: match m.role {
                    Role::User | Role::Tool => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect();

        let claude_tools: Vec<ClaudeTool> = tools
            .iter()
            .map(|t| ClaudeTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters_schema.clone(),
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.config.model_id,
            "max_tokens": self.config.max_tokens,
            "messages": api_messages,
        });

        if let Some(sys) = system_prompt {
            body["system"] = serde_json::json!(sys);
        }

        if !claude_tools.is_empty() {
            body["tools"] = serde_json::to_value(&claude_tools)
                .map_err(|e| AgentorError::Agent(e.to_string()))?;
        }

        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentorError::Http(e.to_string()))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AgentorError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(AgentorError::Http(format!(
                "Claude API error {}: {}",
                status, resp_body
            )));
        }

        parse_claude_response(&resp_body)
    }

    async fn chat_openai(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse> {
        let url = format!("{}/v1/chat/completions", self.config.base_url());

        let mut api_messages: Vec<serde_json::Value> = Vec::new();

        if let Some(sys) = system_prompt {
            api_messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }

        for m in messages {
            if m.role == Role::System {
                continue;
            }
            api_messages.push(serde_json::json!({
                "role": match m.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    Role::System => unreachable!(),
                },
                "content": m.content
            }));
        }

        let mut body = serde_json::json!({
            "model": self.config.model_id,
            "max_tokens": self.config.max_tokens,
            "messages": api_messages,
        });

        if !tools.is_empty() {
            let openai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters_schema,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(openai_tools);
        }

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentorError::Http(e.to_string()))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AgentorError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(AgentorError::Http(format!(
                "OpenAI API error {}: {}",
                status, resp_body
            )));
        }

        parse_openai_response(&resp_body)
    }
}

// -- Claude types --

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ClaudeTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

fn parse_claude_response(body: &serde_json::Value) -> AgentorResult<LlmResponse> {
    let content = body["content"]
        .as_array()
        .ok_or_else(|| AgentorError::Agent("Missing content in Claude response".into()))?;

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in content {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(t) = block["text"].as_str() {
                    text_parts.push(t.to_string());
                }
            }
            Some("tool_use") => {
                let id = block["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let name = block["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let arguments = block["input"].clone();
                tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments,
                });
            }
            _ => {}
        }
    }

    if !tool_calls.is_empty() {
        Ok(LlmResponse::ToolUse {
            content: if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join("\n"))
            },
            tool_calls,
        })
    } else {
        let stop_reason = body["stop_reason"].as_str().unwrap_or("end_turn");
        let text = text_parts.join("\n");
        if stop_reason == "end_turn" {
            Ok(LlmResponse::Done(text))
        } else {
            Ok(LlmResponse::Text(text))
        }
    }
}

fn parse_openai_response(body: &serde_json::Value) -> AgentorResult<LlmResponse> {
    let choice = &body["choices"][0];
    let message = &choice["message"];
    let content = message["content"].as_str().unwrap_or_default().to_string();

    if let Some(tool_calls_json) = message["tool_calls"].as_array() {
        let tool_calls: Vec<ToolCall> = tool_calls_json
            .iter()
            .filter_map(|tc| {
                let id = tc["id"].as_str()?.to_string();
                let name = tc["function"]["name"].as_str()?.to_string();
                let arguments: serde_json::Value =
                    serde_json::from_str(tc["function"]["arguments"].as_str()?)
                        .unwrap_or_default();
                Some(ToolCall {
                    id,
                    name,
                    arguments,
                })
            })
            .collect();

        Ok(LlmResponse::ToolUse {
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls,
        })
    } else {
        let finish_reason = choice["finish_reason"].as_str().unwrap_or("stop");
        if finish_reason == "stop" {
            Ok(LlmResponse::Done(content))
        } else {
            Ok(LlmResponse::Text(content))
        }
    }
}
