use agentor_core::{AgentorError, AgentorResult, Message, Role};
use agentor_security::{AuditLog, PermissionSet};
use agentor_security::audit::AuditOutcome;
use agentor_session::Session;
use agentor_skills::SkillRegistry;
use crate::config::ModelConfig;
use crate::context::ContextWindow;
use crate::llm::{LlmClient, LlmResponse};
use crate::stream::StreamEvent;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};


/// The Agent Runner: orchestrates the agentic loop.
/// Prompt -> LLM -> ToolCall -> Execute Skill -> Backfill -> Repeat.
pub struct AgentRunner {
    llm: LlmClient,
    skills: Arc<SkillRegistry>,
    permissions: PermissionSet,
    audit: Arc<AuditLog>,
    max_turns: u32,
}

impl AgentRunner {
    pub fn new(
        config: ModelConfig,
        skills: Arc<SkillRegistry>,
        permissions: PermissionSet,
        audit: Arc<AuditLog>,
    ) -> Self {
        let max_turns = config.max_turns;
        Self {
            llm: LlmClient::new(config),
            skills,
            permissions,
            audit,
            max_turns,
        }
    }

    /// Run the agentic loop for a session. Returns the final assistant response.
    pub async fn run(&self, session: &mut Session, user_input: &str) -> AgentorResult<String> {
        let session_id = session.id;

        // Add user message
        let user_msg = Message::user(user_input, session_id);
        session.add_message(user_msg);

        let mut context = ContextWindow::new(100);
        context.set_system_prompt(
            "You are Agentor, a secure AI assistant. You have access to tools (skills) \
             that you can use to help the user. Each tool runs in a sandboxed environment \
             with specific permissions. Always explain what you're doing before using a tool."
        );

        for msg in &session.messages {
            context.push(msg.clone());
        }

        let tool_descriptors: Vec<_> = self.skills.list_descriptors().into_iter().cloned().collect();

        info!(session_id = %session_id, "Starting agentic loop");

        for turn in 0..self.max_turns {
            info!(turn = turn, "Agentic loop turn");

            let response = self
                .llm
                .chat(
                    context.system_prompt(),
                    context.messages(),
                    &tool_descriptors,
                )
                .await?;

            match response {
                LlmResponse::Done(text) => {
                    let assistant_msg = Message::assistant(&text, session_id);
                    session.add_message(assistant_msg.clone());
                    context.push(assistant_msg);

                    self.audit.log_action(
                        session_id,
                        "agent_response",
                        None,
                        serde_json::json!({"turn": turn, "type": "final"}),
                        AuditOutcome::Success,
                    );

                    info!(session_id = %session_id, turns = turn + 1, "Agentic loop completed");
                    return Ok(text);
                }

                LlmResponse::Text(text) => {
                    let assistant_msg = Message::assistant(&text, session_id);
                    session.add_message(assistant_msg.clone());
                    context.push(assistant_msg);
                }

                LlmResponse::ToolUse {
                    content,
                    tool_calls,
                } => {
                    // Add any text content from the assistant
                    if let Some(text) = &content {
                        let msg = Message::assistant(text, session_id);
                        session.add_message(msg.clone());
                        context.push(msg);
                    }

                    // Execute each tool call
                    for call in tool_calls {
                        info!(
                            session_id = %session_id,
                            tool = %call.name,
                            call_id = %call.id,
                            "Executing tool call"
                        );

                        self.audit.log_action(
                            session_id,
                            "tool_call",
                            Some(call.name.clone()),
                            serde_json::json!({
                                "call_id": call.id,
                                "arguments": call.arguments,
                            }),
                            AuditOutcome::Success,
                        );

                        let result = self
                            .skills
                            .execute(call.clone(), &self.permissions)
                            .await;

                        match result {
                            Ok(tool_result) => {
                                let outcome = if tool_result.is_error {
                                    AuditOutcome::Error
                                } else {
                                    AuditOutcome::Success
                                };

                                self.audit.log_action(
                                    session_id,
                                    "tool_result",
                                    Some(call.name.clone()),
                                    serde_json::json!({
                                        "call_id": tool_result.call_id,
                                        "is_error": tool_result.is_error,
                                    }),
                                    outcome,
                                );

                                // Backfill tool result as a user message (tool role)
                                let result_content = serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_result.call_id,
                                    "content": tool_result.content,
                                    "is_error": tool_result.is_error,
                                });
                                let tool_msg = Message::new(
                                    Role::User,
                                    result_content.to_string(),
                                    session_id,
                                );
                                session.add_message(tool_msg.clone());
                                context.push(tool_msg);
                            }
                            Err(e) => {
                                error!(error = %e, tool = %call.name, "Tool execution failed");
                                self.audit.log_action(
                                    session_id,
                                    "tool_error",
                                    Some(call.name.clone()),
                                    serde_json::json!({"error": e.to_string()}),
                                    AuditOutcome::Error,
                                );

                                let error_msg = Message::new(
                                    Role::User,
                                    format!("Tool error: {}", e),
                                    session_id,
                                );
                                session.add_message(error_msg.clone());
                                context.push(error_msg);
                            }
                        }
                    }
                }
            }
        }

        warn!(
            session_id = %session_id,
            max_turns = self.max_turns,
            "Agentic loop reached max turns"
        );

        Err(AgentorError::Agent(format!(
            "Agentic loop exceeded maximum of {} turns",
            self.max_turns
        )))
    }

    /// Run the agentic loop with streaming.
    ///
    /// Works like `run()` but uses `chat_stream()` to send partial LLM output to
    /// the caller in real time via the provided `event_tx` channel.  Text responses
    /// are streamed token-by-token; tool calls are accumulated and then executed
    /// (non-streaming) before the next turn.
    pub async fn run_streaming(
        &self,
        session: &mut Session,
        user_input: &str,
        event_tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> AgentorResult<String> {
        let session_id = session.id;

        // Add user message
        let user_msg = Message::user(user_input, session_id);
        session.add_message(user_msg);

        let mut context = ContextWindow::new(100);
        context.set_system_prompt(
            "You are Agentor, a secure AI assistant. You have access to tools (skills) \
             that you can use to help the user. Each tool runs in a sandboxed environment \
             with specific permissions. Always explain what you're doing before using a tool."
        );

        for msg in &session.messages {
            context.push(msg.clone());
        }

        let tool_descriptors: Vec<_> = self.skills.list_descriptors().into_iter().cloned().collect();

        info!(session_id = %session_id, "Starting streaming agentic loop");

        for turn in 0..self.max_turns {
            info!(turn = turn, "Streaming agentic loop turn");

            // Start streaming from the LLM
            let (mut stream_rx, join_handle) = self
                .llm
                .chat_stream(
                    context.system_prompt(),
                    context.messages(),
                    &tool_descriptors,
                )
                .await?;

            // Forward all stream events to the caller
            while let Some(event) = stream_rx.recv().await {
                // Forward the event; if the receiver is gone, we keep going
                // so we can still collect the aggregated response.
                let _ = event_tx.send(event);
            }

            // Wait for the aggregated response
            let response = join_handle
                .await
                .map_err(|e| AgentorError::Agent(format!("Stream task panicked: {}", e)))??;

            match response {
                LlmResponse::Done(text) => {
                    let assistant_msg = Message::assistant(&text, session_id);
                    session.add_message(assistant_msg.clone());
                    context.push(assistant_msg);

                    self.audit.log_action(
                        session_id,
                        "agent_response",
                        None,
                        serde_json::json!({"turn": turn, "type": "final"}),
                        AuditOutcome::Success,
                    );

                    info!(
                        session_id = %session_id,
                        turns = turn + 1,
                        "Streaming agentic loop completed"
                    );
                    return Ok(text);
                }

                LlmResponse::Text(text) => {
                    let assistant_msg = Message::assistant(&text, session_id);
                    session.add_message(assistant_msg.clone());
                    context.push(assistant_msg);
                }

                LlmResponse::ToolUse {
                    content,
                    tool_calls,
                } => {
                    // Add any text content from the assistant
                    if let Some(text) = &content {
                        let msg = Message::assistant(text, session_id);
                        session.add_message(msg.clone());
                        context.push(msg);
                    }

                    // Execute each tool call (non-streaming)
                    for call in tool_calls {
                        info!(
                            session_id = %session_id,
                            tool = %call.name,
                            call_id = %call.id,
                            "Executing tool call (streaming mode)"
                        );

                        self.audit.log_action(
                            session_id,
                            "tool_call",
                            Some(call.name.clone()),
                            serde_json::json!({
                                "call_id": call.id,
                                "arguments": call.arguments,
                            }),
                            AuditOutcome::Success,
                        );

                        let result = self
                            .skills
                            .execute(call.clone(), &self.permissions)
                            .await;

                        match result {
                            Ok(tool_result) => {
                                let outcome = if tool_result.is_error {
                                    AuditOutcome::Error
                                } else {
                                    AuditOutcome::Success
                                };

                                self.audit.log_action(
                                    session_id,
                                    "tool_result",
                                    Some(call.name.clone()),
                                    serde_json::json!({
                                        "call_id": tool_result.call_id,
                                        "is_error": tool_result.is_error,
                                    }),
                                    outcome,
                                );

                                let result_content = serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_result.call_id,
                                    "content": tool_result.content,
                                    "is_error": tool_result.is_error,
                                });
                                let tool_msg = Message::new(
                                    Role::User,
                                    result_content.to_string(),
                                    session_id,
                                );
                                session.add_message(tool_msg.clone());
                                context.push(tool_msg);
                            }
                            Err(e) => {
                                error!(error = %e, tool = %call.name, "Tool execution failed (streaming)");
                                self.audit.log_action(
                                    session_id,
                                    "tool_error",
                                    Some(call.name.clone()),
                                    serde_json::json!({"error": e.to_string()}),
                                    AuditOutcome::Error,
                                );

                                let error_msg = Message::new(
                                    Role::User,
                                    format!("Tool error: {}", e),
                                    session_id,
                                );
                                session.add_message(error_msg.clone());
                                context.push(error_msg);
                            }
                        }
                    }
                }
            }
        }

        warn!(
            session_id = %session_id,
            max_turns = self.max_turns,
            "Streaming agentic loop reached max turns"
        );

        let _ = event_tx.send(StreamEvent::Error {
            message: format!("Agentic loop exceeded maximum of {} turns", self.max_turns),
        });

        Err(AgentorError::Agent(format!(
            "Agentic loop exceeded maximum of {} turns",
            self.max_turns
        )))
    }
}
