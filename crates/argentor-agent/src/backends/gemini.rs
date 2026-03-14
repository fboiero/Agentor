use super::LlmBackend;
use crate::config::ModelConfig;
use crate::llm::LlmResponse;
use crate::stream::StreamEvent;
use argentor_core::{ArgentorError, ArgentorResult, Message, Role, ToolCall};
use argentor_skills::SkillDescriptor;
use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Google Gemini API backend.
///
/// Implements the Gemini `generateContent` and `streamGenerateContent` REST APIs.
/// Supports function calling via Gemini's `functionDeclarations` format.
pub struct GeminiBackend {
    config: ModelConfig,
    http: reqwest::Client,
}

impl GeminiBackend {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    fn build_contents(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
    ) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
        let system_instruction = system_prompt.map(|sys| {
            serde_json::json!({
                "parts": [{ "text": sys }]
            })
        });

        let contents: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "model",
                    Role::System => unreachable!(),
                };
                serde_json::json!({
                    "role": role,
                    "parts": [{ "text": m.content }]
                })
            })
            .collect();

        (system_instruction, contents)
    }

    fn build_tools(&self, tools: &[SkillDescriptor]) -> Option<serde_json::Value> {
        if tools.is_empty() {
            return None;
        }

        let declarations: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                let mut decl = serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                });
                // Gemini uses "parameters" with OpenAPI schema format
                if !t.parameters_schema.is_null() && t.parameters_schema != serde_json::json!({}) {
                    decl["parameters"] = t.parameters_schema.clone();
                }
                decl
            })
            .collect();

        Some(serde_json::json!([{
            "functionDeclarations": declarations
        }]))
    }

    fn chat_url(&self) -> String {
        format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.config.base_url(),
            self.config.model_id,
            self.config.api_key,
        )
    }

    fn stream_url(&self) -> String {
        format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.config.base_url(),
            self.config.model_id,
            self.config.api_key,
        )
    }
}

#[async_trait]
impl LlmBackend for GeminiBackend {
    async fn chat(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> ArgentorResult<LlmResponse> {
        let url = self.chat_url();
        let (system_instruction, contents) = self.build_contents(system_prompt, messages);

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": self.config.temperature,
                "maxOutputTokens": self.config.max_tokens,
            },
        });

        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }

        if let Some(t) = self.build_tools(tools) {
            body["tools"] = t;
        }

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ArgentorError::Http(e.to_string()))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ArgentorError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(ArgentorError::Http(format!(
                "Gemini API error {status}: {resp_body}"
            )));
        }

        parse_gemini_response(&resp_body)
    }

    async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> ArgentorResult<(
        mpsc::Receiver<StreamEvent>,
        JoinHandle<ArgentorResult<LlmResponse>>,
    )> {
        let url = self.stream_url();
        let (system_instruction, contents) = self.build_contents(system_prompt, messages);

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": self.config.temperature,
                "maxOutputTokens": self.config.max_tokens,
            },
        });

        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }

        if let Some(t) = self.build_tools(tools) {
            body["tools"] = t;
        }

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ArgentorError::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(ArgentorError::Http(format!(
                "Gemini API error {status}: {error_body}"
            )));
        }

        let (tx, rx) = mpsc::channel::<StreamEvent>(256);
        let byte_stream = resp.bytes_stream();

        let handle = tokio::spawn(async move {
            let mut stream = byte_stream;
            let mut buffer = String::new();
            let mut full_text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut tool_idx: u32 = 0;

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error {
                                message: format!("Stream read error: {e}"),
                            })
                            .await;
                        return Err(ArgentorError::Http(format!("Stream read error: {e}")));
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
                        let event: serde_json::Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        // Process each candidate's parts
                        if let Some(candidates) = event["candidates"].as_array() {
                            for candidate in candidates {
                                if let Some(parts) = candidate["content"]["parts"].as_array() {
                                    for part in parts {
                                        if let Some(text) = part["text"].as_str() {
                                            full_text.push_str(text);
                                            let _ = tx
                                                .send(StreamEvent::TextDelta {
                                                    text: text.to_string(),
                                                })
                                                .await;
                                        }

                                        if let Some(fc) = part.get("functionCall") {
                                            let name =
                                                fc["name"].as_str().unwrap_or_default().to_string();
                                            let args = fc
                                                .get("args")
                                                .cloned()
                                                .unwrap_or(serde_json::json!({}));
                                            let id = format!("gemini-tc-{tool_idx}");
                                            tool_idx += 1;

                                            let _ = tx
                                                .send(StreamEvent::ToolCallStart {
                                                    id: id.clone(),
                                                    name: name.clone(),
                                                })
                                                .await;
                                            let _ = tx
                                                .send(StreamEvent::ToolCallEnd { id: id.clone() })
                                                .await;

                                            tool_calls.push(ToolCall {
                                                id,
                                                name,
                                                arguments: args,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let _ = tx.send(StreamEvent::Done).await;

            if !tool_calls.is_empty() {
                Ok(LlmResponse::ToolUse {
                    content: if full_text.is_empty() {
                        None
                    } else {
                        Some(full_text)
                    },
                    tool_calls,
                })
            } else {
                Ok(LlmResponse::Done(full_text))
            }
        });

        Ok((rx, handle))
    }
}

pub fn parse_gemini_response(body: &serde_json::Value) -> ArgentorResult<LlmResponse> {
    let candidates = body["candidates"]
        .as_array()
        .ok_or_else(|| ArgentorError::Agent("Missing candidates in Gemini response".into()))?;

    let candidate = candidates
        .first()
        .ok_or_else(|| ArgentorError::Agent("Empty candidates in Gemini response".into()))?;

    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or_else(|| ArgentorError::Agent("Missing parts in Gemini response".into()))?;

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_idx: u32 = 0;

    for part in parts {
        if let Some(text) = part["text"].as_str() {
            text_parts.push(text.to_string());
        }

        if let Some(fc) = part.get("functionCall") {
            let name = fc["name"].as_str().unwrap_or_default().to_string();
            let args = fc.get("args").cloned().unwrap_or(serde_json::json!({}));
            let id = format!("gemini-tc-{tool_idx}");
            tool_idx += 1;

            tool_calls.push(ToolCall {
                id,
                name,
                arguments: args,
            });
        }
    }

    if !tool_calls.is_empty() {
        Ok(LlmResponse::ToolUse {
            content: if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join(""))
            },
            tool_calls,
        })
    } else {
        let finish_reason = candidate["finishReason"].as_str().unwrap_or("STOP");
        let text = text_parts.join("");
        if finish_reason == "STOP" {
            Ok(LlmResponse::Done(text))
        } else {
            Ok(LlmResponse::Text(text))
        }
    }
}
