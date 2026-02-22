use super::LlmBackend;
use crate::config::ModelConfig;
use crate::llm::LlmResponse;
use crate::stream::StreamEvent;
use agentor_core::{AgentorError, AgentorResult, Message, Role, ToolCall};
use agentor_skills::SkillDescriptor;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Claude (Anthropic) API backend.
pub struct ClaudeBackend {
    config: ModelConfig,
    http: reqwest::Client,
}

impl ClaudeBackend {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmBackend for ClaudeBackend {
    async fn chat(
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

    async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        JoinHandle<AgentorResult<LlmResponse>>,
    )> {
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
            "stream": true,
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
        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AgentorError::Http(format!(
                "Claude API error {}: {}",
                status, error_body
            )));
        }

        let (tx, rx) = mpsc::channel::<StreamEvent>(256);
        let byte_stream = resp.bytes_stream();

        let handle = tokio::spawn(async move {
            let mut stream = byte_stream;
            let mut buffer = String::new();
            let mut full_text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut active_tool_blocks: std::collections::HashMap<u64, (String, String, String)> =
                std::collections::HashMap::new();
            let mut stop_reason = String::from("end_turn");

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error {
                                message: format!("Stream read error: {}", e),
                            })
                            .await;
                        return Err(AgentorError::Http(format!("Stream read error: {}", e)));
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            let _ = tx.send(StreamEvent::Done).await;
                            continue;
                        }

                        let event: serde_json::Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        let event_type = event["type"].as_str().unwrap_or("");

                        match event_type {
                            "content_block_start" => {
                                let index = event["index"].as_u64().unwrap_or(0);
                                let block = &event["content_block"];
                                if block["type"].as_str() == Some("tool_use") {
                                    let id = block["id"].as_str().unwrap_or_default().to_string();
                                    let name =
                                        block["name"].as_str().unwrap_or_default().to_string();
                                    active_tool_blocks
                                        .insert(index, (id.clone(), name.clone(), String::new()));
                                    let _ = tx.send(StreamEvent::ToolCallStart { id, name }).await;
                                }
                            }

                            "content_block_delta" => {
                                let index = event["index"].as_u64().unwrap_or(0);
                                let delta = &event["delta"];
                                let delta_type = delta["type"].as_str().unwrap_or("");

                                match delta_type {
                                    "text_delta" => {
                                        if let Some(text) = delta["text"].as_str() {
                                            full_text.push_str(text);
                                            let _ = tx
                                                .send(StreamEvent::TextDelta {
                                                    text: text.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some(partial) = delta["partial_json"].as_str() {
                                            if let Some(block) = active_tool_blocks.get_mut(&index)
                                            {
                                                block.2.push_str(partial);
                                                let _ = tx
                                                    .send(StreamEvent::ToolCallDelta {
                                                        id: block.0.clone(),
                                                        arguments_delta: partial.to_string(),
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }

                            "content_block_stop" => {
                                let index = event["index"].as_u64().unwrap_or(0);
                                if let Some((id, name, args_json)) =
                                    active_tool_blocks.remove(&index)
                                {
                                    let arguments: serde_json::Value = serde_json::from_str(
                                        &args_json,
                                    )
                                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                                    tool_calls.push(ToolCall {
                                        id: id.clone(),
                                        name,
                                        arguments,
                                    });
                                    let _ = tx.send(StreamEvent::ToolCallEnd { id }).await;
                                }
                            }

                            "message_delta" => {
                                if let Some(sr) = event["delta"]["stop_reason"].as_str() {
                                    stop_reason = sr.to_string();
                                }
                            }

                            "message_stop" => {
                                let _ = tx.send(StreamEvent::Done).await;
                            }

                            _ => {}
                        }
                    }
                }
            }

            if !tool_calls.is_empty() {
                Ok(LlmResponse::ToolUse {
                    content: if full_text.is_empty() {
                        None
                    } else {
                        Some(full_text)
                    },
                    tool_calls,
                })
            } else if stop_reason == "end_turn" {
                Ok(LlmResponse::Done(full_text))
            } else {
                Ok(LlmResponse::Text(full_text))
            }
        });

        Ok((rx, handle))
    }
}

// -- Claude wire types --

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

pub fn parse_claude_response(body: &serde_json::Value) -> AgentorResult<LlmResponse> {
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
                let id = block["id"].as_str().unwrap_or_default().to_string();
                let name = block["name"].as_str().unwrap_or_default().to_string();
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
