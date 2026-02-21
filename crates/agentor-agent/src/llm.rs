use crate::config::{LlmProvider, ModelConfig};
use crate::stream::StreamEvent;
use agentor_core::{AgentorError, AgentorResult, Message, Role, ToolCall};
use agentor_skills::SkillDescriptor;
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;

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

    // ---- Non-streaming API ----

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

    // ---- Streaming API ----

    /// Unified streaming chat that dispatches to the correct provider.
    /// Returns an `mpsc::Receiver<StreamEvent>` that yields events as the LLM
    /// generates its response, plus the final aggregated `LlmResponse` when done.
    pub async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        tokio::task::JoinHandle<AgentorResult<LlmResponse>>,
    )> {
        match self.config.provider {
            LlmProvider::Claude => {
                self.chat_stream_claude(system_prompt, messages, tools)
                    .await
            }
            LlmProvider::OpenAi => {
                self.chat_stream_openai(system_prompt, messages, tools)
                    .await
            }
        }
    }

    /// Stream a response from the Claude API.
    ///
    /// Sends the request with `"stream": true` and parses SSE events:
    /// - `content_block_start` (type=tool_use) -> ToolCallStart
    /// - `content_block_delta` (type=text_delta) -> TextDelta
    /// - `content_block_delta` (type=input_json_delta) -> ToolCallDelta
    /// - `content_block_stop` -> ToolCallEnd (if applicable)
    /// - `message_stop` -> Done
    async fn chat_stream_claude(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        tokio::task::JoinHandle<AgentorResult<LlmResponse>>,
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

            // Track current content blocks by index
            // Maps block index -> (id, name, accumulated arguments JSON)
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

                // Process complete SSE lines from the buffer
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

            // Build the final aggregated LlmResponse
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

    /// Stream a response from the OpenAI API.
    ///
    /// Sends the request with `"stream": true` and parses SSE `data: {...}` lines.
    /// Each chunk contains `choices[0].delta` with optional `content`, `tool_calls`,
    /// or `finish_reason`.
    async fn chat_stream_openai(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        tokio::task::JoinHandle<AgentorResult<LlmResponse>>,
    )> {
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
            "stream": true,
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
        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AgentorError::Http(format!(
                "OpenAI API error {}: {}",
                status, error_body
            )));
        }

        let (tx, rx) = mpsc::channel::<StreamEvent>(256);
        let byte_stream = resp.bytes_stream();

        let handle = tokio::spawn(async move {
            let mut stream = byte_stream;
            let mut buffer = String::new();
            let mut full_text = String::new();
            // Track tool calls by index: index -> (id, name, accumulated_arguments)
            let mut tool_call_map: std::collections::HashMap<u64, (String, String, String)> =
                std::collections::HashMap::new();
            let mut finish_reason = String::from("stop");

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

                // Process complete SSE lines
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

                        let choice = &event["choices"][0];

                        // Check for finish_reason
                        if let Some(fr) = choice["finish_reason"].as_str() {
                            finish_reason = fr.to_string();

                            // When done, emit ToolCallEnd for any open tool calls
                            if fr == "tool_calls" {
                                for (id, _name, _args) in tool_call_map.values() {
                                    let _ =
                                        tx.send(StreamEvent::ToolCallEnd { id: id.clone() }).await;
                                }
                            }

                            let _ = tx.send(StreamEvent::Done).await;
                            continue;
                        }

                        let delta = &choice["delta"];

                        // Text content delta
                        if let Some(content) = delta["content"].as_str() {
                            if !content.is_empty() {
                                full_text.push_str(content);
                                let _ = tx
                                    .send(StreamEvent::TextDelta {
                                        text: content.to_string(),
                                    })
                                    .await;
                            }
                        }

                        // Tool call deltas
                        if let Some(tc_array) = delta["tool_calls"].as_array() {
                            for tc in tc_array {
                                let idx = tc["index"].as_u64().unwrap_or(0);

                                // If this chunk contains an id, it's the start of a new tool call
                                if let Some(id) = tc["id"].as_str() {
                                    let name = tc["function"]["name"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_string();
                                    tool_call_map
                                        .insert(idx, (id.to_string(), name.clone(), String::new()));
                                    let _ = tx
                                        .send(StreamEvent::ToolCallStart {
                                            id: id.to_string(),
                                            name,
                                        })
                                        .await;
                                }

                                // Accumulate argument fragments
                                if let Some(args_delta) = tc["function"]["arguments"].as_str() {
                                    if !args_delta.is_empty() {
                                        if let Some(entry) = tool_call_map.get_mut(&idx) {
                                            entry.2.push_str(args_delta);
                                            let _ = tx
                                                .send(StreamEvent::ToolCallDelta {
                                                    id: entry.0.clone(),
                                                    arguments_delta: args_delta.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Build final aggregated LlmResponse
            if !tool_call_map.is_empty() {
                let tool_calls: Vec<ToolCall> = tool_call_map
                    .into_values()
                    .map(|(id, name, args_json)| {
                        let arguments: serde_json::Value =
                            serde_json::from_str(&args_json).unwrap_or_default();
                        ToolCall {
                            id,
                            name,
                            arguments,
                        }
                    })
                    .collect();

                Ok(LlmResponse::ToolUse {
                    content: if full_text.is_empty() {
                        None
                    } else {
                        Some(full_text)
                    },
                    tool_calls,
                })
            } else if finish_reason == "stop" {
                Ok(LlmResponse::Done(full_text))
            } else {
                Ok(LlmResponse::Text(full_text))
            }
        });

        Ok((rx, handle))
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
                    serde_json::from_str(tc["function"]["arguments"].as_str()?).unwrap_or_default();
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
