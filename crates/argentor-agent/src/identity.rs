use serde::{Deserialize, Serialize};
use std::path::Path;

/// Agent personality configuration (equivalent to SOUL.md / AGENTS.md).
///
/// Defines the agent's identity, communication style, and behavioral constraints.
/// Loaded from a TOML/YAML config or inline in code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPersonality {
    /// Display name for the agent.
    pub name: String,
    /// Short tagline or role description.
    pub role: String,
    /// Detailed personality/behavior instructions injected as system prompt.
    pub instructions: String,
    /// Communication style preferences.
    #[serde(default)]
    pub style: CommunicationStyle,
    /// Hard constraints the agent must never violate.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Knowledge domains the agent specializes in.
    #[serde(default)]
    pub expertise: Vec<String>,
    /// Thinking level for chain-of-thought reasoning.
    #[serde(default)]
    pub thinking_level: ThinkingLevel,
}

impl AgentPersonality {
    /// Build the system prompt from this personality.
    pub fn to_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!("You are {}, {}.", self.name, self.role));
        parts.push(self.instructions.clone());

        if !self.style.tone.is_empty() {
            parts.push(format!("Communication style: {}.", self.style.tone));
        }
        if let Some(lang) = &self.style.language {
            parts.push(format!("Respond in {}.", lang));
        }

        if !self.constraints.is_empty() {
            parts.push("CONSTRAINTS (never violate these):".into());
            for c in &self.constraints {
                parts.push(format!("- {c}"));
            }
        }

        if !self.expertise.is_empty() {
            parts.push(format!(
                "Your areas of expertise: {}.",
                self.expertise.join(", ")
            ));
        }

        match self.thinking_level {
            ThinkingLevel::Off => {}
            ThinkingLevel::Low => {
                parts.push("Think briefly before responding.".into());
            }
            ThinkingLevel::Medium => {
                parts.push(
                    "Think step-by-step before responding. Show your reasoning concisely.".into(),
                );
            }
            ThinkingLevel::High => {
                parts.push(
                    "Think deeply and methodically before responding. Explore multiple approaches, \
                     consider edge cases, and show your full reasoning process."
                        .into(),
                );
            }
        }

        parts.join("\n\n")
    }

    /// Load personality from a TOML file (e.g., SOUL.toml or agents.toml).
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {e}"))
        } else {
            Err("Unsupported personality file format (use .toml)".into())
        }
    }
}

impl Default for AgentPersonality {
    fn default() -> Self {
        Self {
            name: "Argentor".into(),
            role: "a secure AI assistant".into(),
            instructions: "Help users accomplish tasks using the available tools. \
                          Each tool runs in a sandboxed environment with specific permissions. \
                          Always explain what you're doing before using a tool."
                .into(),
            style: CommunicationStyle::default(),
            constraints: vec![
                "Never execute destructive operations without explicit user confirmation".into(),
                "Never expose API keys, passwords, or sensitive credentials".into(),
            ],
            expertise: vec![],
            thinking_level: ThinkingLevel::Off,
        }
    }
}

/// Communication style preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStyle {
    /// Tone descriptor (e.g., "professional", "casual", "technical").
    #[serde(default = "default_tone")]
    pub tone: String,
    /// Preferred response language (e.g., "English", "Spanish"). None = auto-detect.
    pub language: Option<String>,
    /// Whether to use markdown formatting.
    #[serde(default = "default_true")]
    pub use_markdown: bool,
    /// Maximum response length hint (soft limit).
    pub max_response_length: Option<usize>,
}

impl Default for CommunicationStyle {
    fn default() -> Self {
        Self {
            tone: default_tone(),
            language: None,
            use_markdown: true,
            max_response_length: None,
        }
    }
}

fn default_tone() -> String {
    "professional and concise".into()
}

fn default_true() -> bool {
    true
}

/// Thinking level for chain-of-thought reasoning.
///
/// Controls how much internal reasoning the agent shows before responding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    /// No explicit thinking — respond directly.
    #[default]
    Off,
    /// Brief internal check before responding.
    Low,
    /// Step-by-step reasoning shown concisely.
    Medium,
    /// Deep, thorough reasoning with multiple approaches explored.
    High,
}

/// Session command parsed from user input (e.g., "/status", "/compact").
#[derive(Debug, Clone, PartialEq)]
pub enum SessionCommand {
    /// Show current session status (messages, turns, active skills).
    Status,
    /// Start a new session, clearing history.
    New,
    /// Reset the current session (keep config, clear messages).
    Reset,
    /// Compact the context window (summarize old messages).
    Compact,
    /// Set thinking level.
    Think(ThinkingLevel),
    /// Show token/cost usage estimate.
    Usage,
    /// List available skills.
    Skills,
    /// Show audit log for current session.
    Audit,
    /// Show help for available commands.
    Help,
    /// Not a command — regular user message.
    NotACommand,
}

impl SessionCommand {
    /// Parse a user input string into a session command.
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return Self::NotACommand;
        }

        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg_owned = parts.get(1).map(|s| s.trim().to_lowercase());
        let arg = arg_owned.as_deref();

        match cmd.as_str() {
            "/status" => Self::Status,
            "/new" => Self::New,
            "/reset" => Self::Reset,
            "/compact" => Self::Compact,
            "/think" => {
                let level = match arg {
                    Some("off") | Some("0") | None => ThinkingLevel::Off,
                    Some("low") | Some("1") => ThinkingLevel::Low,
                    Some("medium") | Some("med") | Some("2") => ThinkingLevel::Medium,
                    Some("high") | Some("3") => ThinkingLevel::High,
                    _ => ThinkingLevel::Off,
                };
                Self::Think(level)
            }
            "/usage" => Self::Usage,
            "/skills" => Self::Skills,
            "/audit" => Self::Audit,
            "/help" => Self::Help,
            _ => Self::NotACommand,
        }
    }

    /// Generate help text listing all available commands.
    pub fn help_text() -> &'static str {
        "/status   — Show session status (messages, turns, skills)\n\
         /new      — Start a new session\n\
         /reset    — Reset current session (keep config)\n\
         /compact  — Summarize old messages to save context\n\
         /think    — Set thinking level: off, low, medium, high\n\
         /usage    — Show estimated token usage\n\
         /skills   — List available skills\n\
         /audit    — Show audit log for this session\n\
         /help     — Show this help message"
    }
}

/// Context compaction: summarize old messages to free up context window space.
///
/// When the context approaches the limit (e.g., 80% full), older messages
/// are replaced with a summary to maintain coherent conversation without
/// losing critical context.
pub struct ContextCompactor {
    /// Threshold (0.0-1.0) at which auto-compaction triggers.
    pub threshold: f32,
    /// Number of recent messages to always keep uncompacted.
    pub keep_recent: usize,
}

impl ContextCompactor {
    pub fn new(threshold: f32, keep_recent: usize) -> Self {
        Self {
            threshold,
            keep_recent,
        }
    }

    /// Check if compaction should trigger based on current context usage.
    pub fn should_compact(&self, current_messages: usize, max_messages: usize) -> bool {
        if max_messages == 0 {
            return false;
        }
        let usage = current_messages as f32 / max_messages as f32;
        usage >= self.threshold
    }

    /// Generate a compaction summary prompt for the LLM.
    /// Returns the messages to summarize and the recent messages to keep.
    pub fn split_for_compaction<T: Clone>(&self, messages: &[T]) -> (Vec<T>, Vec<T>) {
        if messages.len() <= self.keep_recent {
            return (vec![], messages.to_vec());
        }

        let split_point = messages.len() - self.keep_recent;
        let to_summarize = messages[..split_point].to_vec();
        let to_keep = messages[split_point..].to_vec();
        (to_summarize, to_keep)
    }

    /// Build the summary request prompt from messages to compact.
    pub fn build_summary_prompt(message_texts: &[String]) -> String {
        let mut prompt = String::from(
            "Summarize the following conversation history into a concise summary. \
             Preserve key decisions, important context, and any pending tasks. \
             Be thorough but concise:\n\n",
        );

        for (i, text) in message_texts.iter().enumerate() {
            prompt.push_str(&format!("[Message {}]: {}\n", i + 1, text));
        }

        prompt
    }
}

impl Default for ContextCompactor {
    fn default() -> Self {
        Self {
            threshold: 0.8,
            keep_recent: 10,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_personality_generates_prompt() {
        let p = AgentPersonality::default();
        let prompt = p.to_system_prompt();
        assert!(prompt.contains("Argentor"));
        assert!(prompt.contains("secure AI assistant"));
        assert!(prompt.contains("Never execute destructive"));
    }

    #[test]
    fn personality_with_thinking_level() {
        let mut p = AgentPersonality::default();
        p.thinking_level = ThinkingLevel::High;
        let prompt = p.to_system_prompt();
        assert!(prompt.contains("Think deeply"));
    }

    #[test]
    fn personality_with_language() {
        let mut p = AgentPersonality::default();
        p.style.language = Some("Spanish".into());
        let prompt = p.to_system_prompt();
        assert!(prompt.contains("Respond in Spanish"));
    }

    #[test]
    fn personality_with_expertise() {
        let mut p = AgentPersonality::default();
        p.expertise = vec!["Rust".into(), "security".into()];
        let prompt = p.to_system_prompt();
        assert!(prompt.contains("Rust, security"));
    }

    // Session command tests

    #[test]
    fn parse_status_command() {
        assert_eq!(SessionCommand::parse("/status"), SessionCommand::Status);
    }

    #[test]
    fn parse_new_command() {
        assert_eq!(SessionCommand::parse("/new"), SessionCommand::New);
    }

    #[test]
    fn parse_compact_command() {
        assert_eq!(SessionCommand::parse("/compact"), SessionCommand::Compact);
    }

    #[test]
    fn parse_think_levels() {
        assert_eq!(
            SessionCommand::parse("/think off"),
            SessionCommand::Think(ThinkingLevel::Off)
        );
        assert_eq!(
            SessionCommand::parse("/think low"),
            SessionCommand::Think(ThinkingLevel::Low)
        );
        assert_eq!(
            SessionCommand::parse("/think medium"),
            SessionCommand::Think(ThinkingLevel::Medium)
        );
        assert_eq!(
            SessionCommand::parse("/think high"),
            SessionCommand::Think(ThinkingLevel::High)
        );
        assert_eq!(
            SessionCommand::parse("/think 2"),
            SessionCommand::Think(ThinkingLevel::Medium)
        );
    }

    #[test]
    fn parse_other_commands() {
        assert_eq!(SessionCommand::parse("/usage"), SessionCommand::Usage);
        assert_eq!(SessionCommand::parse("/skills"), SessionCommand::Skills);
        assert_eq!(SessionCommand::parse("/audit"), SessionCommand::Audit);
        assert_eq!(SessionCommand::parse("/help"), SessionCommand::Help);
        assert_eq!(SessionCommand::parse("/reset"), SessionCommand::Reset);
    }

    #[test]
    fn parse_not_a_command() {
        assert_eq!(SessionCommand::parse("hello"), SessionCommand::NotACommand);
        assert_eq!(
            SessionCommand::parse("what is /status?"),
            SessionCommand::NotACommand
        );
    }

    #[test]
    fn parse_unknown_slash_command() {
        assert_eq!(
            SessionCommand::parse("/foobar"),
            SessionCommand::NotACommand
        );
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(SessionCommand::parse("/STATUS"), SessionCommand::Status);
        assert_eq!(
            SessionCommand::parse("/Think High"),
            SessionCommand::Think(ThinkingLevel::High)
        );
    }

    #[test]
    fn help_text_not_empty() {
        let help = SessionCommand::help_text();
        assert!(help.contains("/status"));
        assert!(help.contains("/compact"));
        assert!(help.contains("/think"));
    }

    // Context compactor tests

    #[test]
    fn compactor_triggers_at_threshold() {
        let compactor = ContextCompactor::new(0.8, 10);
        assert!(!compactor.should_compact(50, 100));
        assert!(compactor.should_compact(80, 100));
        assert!(compactor.should_compact(100, 100));
    }

    #[test]
    fn compactor_no_trigger_on_empty() {
        let compactor = ContextCompactor::new(0.8, 10);
        assert!(!compactor.should_compact(0, 0));
    }

    #[test]
    fn compactor_splits_correctly() {
        let compactor = ContextCompactor::new(0.8, 3);
        let messages: Vec<i32> = (1..=10).collect();

        let (to_summarize, to_keep) = compactor.split_for_compaction(&messages);
        assert_eq!(to_summarize, vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(to_keep, vec![8, 9, 10]);
    }

    #[test]
    fn compactor_keeps_all_when_few_messages() {
        let compactor = ContextCompactor::new(0.8, 10);
        let messages: Vec<i32> = (1..=5).collect();

        let (to_summarize, to_keep) = compactor.split_for_compaction(&messages);
        assert!(to_summarize.is_empty());
        assert_eq!(to_keep, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn summary_prompt_includes_messages() {
        let msgs = vec!["Hello".to_string(), "How are you?".to_string()];
        let prompt = ContextCompactor::build_summary_prompt(&msgs);
        assert!(prompt.contains("[Message 1]: Hello"));
        assert!(prompt.contains("[Message 2]: How are you?"));
    }

    // ThinkingLevel serde

    #[test]
    fn thinking_level_serde() {
        let json = serde_json::to_string(&ThinkingLevel::High).unwrap();
        assert_eq!(json, "\"high\"");

        let parsed: ThinkingLevel = serde_json::from_str("\"medium\"").unwrap();
        assert_eq!(parsed, ThinkingLevel::Medium);
    }

    #[test]
    fn personality_serialization_roundtrip() {
        let p = AgentPersonality::default();
        let json = serde_json::to_string(&p).unwrap();
        let p2: AgentPersonality = serde_json::from_str(&json).unwrap();
        assert_eq!(p.name, p2.name);
        assert_eq!(p.role, p2.role);
    }
}
