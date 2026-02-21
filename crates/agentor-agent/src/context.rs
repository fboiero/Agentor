use agentor_core::Message;

/// Manages the context window for LLM calls.
/// Handles message history, truncation, and token estimation.
pub struct ContextWindow {
    messages: Vec<Message>,
    system_prompt: Option<String>,
    max_messages: usize,
}

impl ContextWindow {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            max_messages,
        }
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
        self.truncate();
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    fn truncate(&mut self) {
        if self.messages.len() > self.max_messages {
            let excess = self.messages.len() - self.max_messages;
            self.messages.drain(..excess);
        }
    }

    /// Rough token estimation (4 chars ≈ 1 token).
    pub fn estimated_tokens(&self) -> usize {
        let sys_tokens = self
            .system_prompt
            .as_ref()
            .map(|s| s.len() / 4)
            .unwrap_or(0);
        let msg_tokens: usize = self.messages.iter().map(|m| m.content.len() / 4).sum();
        sys_tokens + msg_tokens
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
