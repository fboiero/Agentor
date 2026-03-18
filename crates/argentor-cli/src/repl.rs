//! Interactive REPL (Read-Eval-Print Loop) for agent debugging.
//!
//! Provides an interactive command-line shell for exploring and
//! debugging Argentor agents, skills, sessions, and metrics.
//!
//! # Main types
//!
//! - [`Repl`] — The interactive shell engine.
//! - [`ReplCommand`] — Parsed user commands.
//! - [`ReplContext`] — Shared state available to all commands.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ReplCommand
// ---------------------------------------------------------------------------

/// A parsed REPL command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    /// Show help for all commands or a specific command.
    Help(Option<String>),
    /// List available skills.
    Skills,
    /// Show details of a specific skill.
    SkillInfo(String),
    /// List active sessions.
    Sessions,
    /// Show the current agent configuration.
    Config,
    /// Show system metrics.
    Metrics,
    /// Show system health status.
    Health,
    /// Set a configuration value.
    Set(String, String),
    /// Get a configuration value.
    Get(String),
    /// Show command history.
    History,
    /// Clear the terminal.
    Clear,
    /// Show REPL version.
    Version,
    /// Exit the REPL.
    Exit,
    /// Unknown command.
    Unknown(String),
    /// Empty line (no-op).
    Empty,
}

impl ReplCommand {
    /// Parse a command from user input.
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if input.is_empty() {
            return Self::Empty;
        }

        let mut parts = input.splitn(3, ' ');
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let arg1 = parts.next().map(|s| s.to_string());
        let arg2 = parts.next().map(|s| s.to_string());

        match cmd.as_str() {
            "help" | "?" | "h" => Self::Help(arg1),
            "skills" | "skill" if arg1.is_none() => Self::Skills,
            "skills" | "skill" => Self::SkillInfo(arg1.unwrap_or_default()),
            "sessions" => Self::Sessions,
            "config" => Self::Config,
            "metrics" => Self::Metrics,
            "health" => Self::Health,
            "set" => match (arg1, arg2) {
                (Some(key), Some(value)) => Self::Set(key, value),
                _ => Self::Unknown("set requires KEY VALUE".to_string()),
            },
            "get" => match arg1 {
                Some(key) => Self::Get(key),
                None => Self::Unknown("get requires KEY".to_string()),
            },
            "history" => Self::History,
            "clear" | "cls" => Self::Clear,
            "version" | "ver" => Self::Version,
            "exit" | "quit" | "q" => Self::Exit,
            _ => Self::Unknown(input.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// CommandHelp
// ---------------------------------------------------------------------------

/// Help entry for a REPL command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHelp {
    /// Command name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Usage pattern.
    pub usage: String,
    /// Example usages.
    pub examples: Vec<String>,
}

/// Return help entries for all REPL commands.
pub fn command_help_entries() -> Vec<CommandHelp> {
    vec![
        CommandHelp {
            name: "help".to_string(),
            description: "Show help for all commands or a specific command".to_string(),
            usage: "help [command]".to_string(),
            examples: vec!["help".to_string(), "help skills".to_string()],
        },
        CommandHelp {
            name: "skills".to_string(),
            description: "List registered skills or show details of a specific skill".to_string(),
            usage: "skills [name]".to_string(),
            examples: vec!["skills".to_string(), "skills file_write".to_string()],
        },
        CommandHelp {
            name: "sessions".to_string(),
            description: "List active sessions".to_string(),
            usage: "sessions".to_string(),
            examples: vec!["sessions".to_string()],
        },
        CommandHelp {
            name: "config".to_string(),
            description: "Show current agent configuration".to_string(),
            usage: "config".to_string(),
            examples: vec!["config".to_string()],
        },
        CommandHelp {
            name: "metrics".to_string(),
            description: "Show system metrics summary".to_string(),
            usage: "metrics".to_string(),
            examples: vec!["metrics".to_string()],
        },
        CommandHelp {
            name: "health".to_string(),
            description: "Show system health status".to_string(),
            usage: "health".to_string(),
            examples: vec!["health".to_string()],
        },
        CommandHelp {
            name: "set".to_string(),
            description: "Set a configuration value".to_string(),
            usage: "set KEY VALUE".to_string(),
            examples: vec!["set model gpt-4".to_string(), "set timeout 30".to_string()],
        },
        CommandHelp {
            name: "get".to_string(),
            description: "Get a configuration value".to_string(),
            usage: "get KEY".to_string(),
            examples: vec!["get model".to_string()],
        },
        CommandHelp {
            name: "history".to_string(),
            description: "Show command history".to_string(),
            usage: "history".to_string(),
            examples: vec!["history".to_string()],
        },
        CommandHelp {
            name: "clear".to_string(),
            description: "Clear the terminal screen".to_string(),
            usage: "clear".to_string(),
            examples: vec!["clear".to_string()],
        },
        CommandHelp {
            name: "version".to_string(),
            description: "Show REPL version".to_string(),
            usage: "version".to_string(),
            examples: vec!["version".to_string()],
        },
        CommandHelp {
            name: "exit".to_string(),
            description: "Exit the REPL".to_string(),
            usage: "exit".to_string(),
            examples: vec!["exit".to_string(), "quit".to_string(), "q".to_string()],
        },
    ]
}

// ---------------------------------------------------------------------------
// ReplContext
// ---------------------------------------------------------------------------

/// Shared mutable state for the REPL session.
#[derive(Debug, Clone)]
pub struct ReplContext {
    /// Configuration key-value store.
    pub config: HashMap<String, String>,
    /// Command history.
    pub history: Vec<String>,
    /// Maximum history size.
    pub max_history: usize,
    /// Current prompt string.
    pub prompt: String,
}

impl Default for ReplContext {
    fn default() -> Self {
        Self {
            config: HashMap::new(),
            history: Vec::new(),
            max_history: 1000,
            prompt: "argentor> ".to_string(),
        }
    }
}

impl ReplContext {
    /// Create a new REPL context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a configuration value.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.config.insert(key.into(), value.into());
    }

    /// Get a configuration value.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    /// Add a command to history.
    pub fn add_to_history(&mut self, command: impl Into<String>) {
        let cmd = command.into();
        if !cmd.is_empty() {
            self.history.push(cmd);
            if self.history.len() > self.max_history {
                self.history.remove(0);
            }
        }
    }

    /// Get the last N commands from history.
    pub fn recent_history(&self, n: usize) -> &[String] {
        let start = self.history.len().saturating_sub(n);
        &self.history[start..]
    }
}

// ---------------------------------------------------------------------------
// ReplOutput
// ---------------------------------------------------------------------------

/// Structured output from a REPL command execution.
#[derive(Debug, Clone)]
pub struct ReplOutput {
    /// Lines of output text.
    pub lines: Vec<String>,
    /// Whether the REPL should exit.
    pub should_exit: bool,
    /// Whether the terminal should be cleared.
    pub should_clear: bool,
}

impl ReplOutput {
    /// Create output with text lines.
    pub fn text(lines: Vec<String>) -> Self {
        Self {
            lines,
            should_exit: false,
            should_clear: false,
        }
    }

    /// Create an exit signal.
    pub fn exit() -> Self {
        Self {
            lines: vec!["Goodbye!".to_string()],
            should_exit: true,
            should_clear: false,
        }
    }

    /// Create a clear screen signal.
    pub fn clear() -> Self {
        Self {
            lines: Vec::new(),
            should_exit: false,
            should_clear: true,
        }
    }

    /// Create an empty output (no-op).
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            should_exit: false,
            should_clear: false,
        }
    }
}

// ---------------------------------------------------------------------------
// execute_command
// ---------------------------------------------------------------------------

/// Execute a REPL command and return structured output.
pub fn execute_command(cmd: &ReplCommand, ctx: &mut ReplContext) -> ReplOutput {
    match cmd {
        ReplCommand::Help(specific) => execute_help(specific.as_deref()),
        ReplCommand::Skills => ReplOutput::text(vec![
            "Registered Skills:".to_string(),
            "  (Use in server context for live skill data)".to_string(),
            "  Run `argentor skill list` for full listing".to_string(),
        ]),
        ReplCommand::SkillInfo(name) => ReplOutput::text(vec![
            format!("Skill: {name}"),
            "  (Use in server context for live skill data)".to_string(),
        ]),
        ReplCommand::Sessions => ReplOutput::text(vec![
            "Active Sessions:".to_string(),
            "  (Use in server context for live session data)".to_string(),
        ]),
        ReplCommand::Config => {
            let mut lines = vec!["Configuration:".to_string()];
            if ctx.config.is_empty() {
                lines.push("  (no values set)".to_string());
            } else {
                let mut keys: Vec<&String> = ctx.config.keys().collect();
                keys.sort();
                for key in keys {
                    if let Some(val) = ctx.config.get(key) {
                        lines.push(format!("  {key} = {val}"));
                    }
                }
            }
            ReplOutput::text(lines)
        }
        ReplCommand::Metrics => ReplOutput::text(vec![
            "Metrics:".to_string(),
            "  (Use in server context for live metrics)".to_string(),
            "  Run `curl localhost:3000/metrics` for Prometheus export".to_string(),
        ]),
        ReplCommand::Health => ReplOutput::text(vec![
            "Health: OK".to_string(),
            "  REPL: running".to_string(),
        ]),
        ReplCommand::Set(key, value) => {
            ctx.set(key, value);
            ReplOutput::text(vec![format!("Set {key} = {value}")])
        }
        ReplCommand::Get(key) => match ctx.get(key) {
            Some(value) => ReplOutput::text(vec![format!("{key} = {value}")]),
            None => ReplOutput::text(vec![format!("{key} is not set")]),
        },
        ReplCommand::History => {
            let recent = ctx.recent_history(20);
            let mut lines = vec!["Command History:".to_string()];
            for (i, cmd) in recent.iter().enumerate() {
                lines.push(format!("  {:>3}  {cmd}", i + 1));
            }
            ReplOutput::text(lines)
        }
        ReplCommand::Clear => ReplOutput::clear(),
        ReplCommand::Version => ReplOutput::text(vec![
            format!("Argentor REPL v{}", env!("CARGO_PKG_VERSION")),
            "Interactive agent debugging shell".to_string(),
        ]),
        ReplCommand::Exit => ReplOutput::exit(),
        ReplCommand::Unknown(input) => ReplOutput::text(vec![
            format!("Unknown command: {input}"),
            "Type 'help' for available commands".to_string(),
        ]),
        ReplCommand::Empty => ReplOutput::empty(),
    }
}

/// Execute the help command.
fn execute_help(specific: Option<&str>) -> ReplOutput {
    let entries = command_help_entries();

    if let Some(name) = specific {
        let name_lower = name.to_lowercase();
        if let Some(entry) = entries.iter().find(|e| e.name == name_lower) {
            let mut lines = vec![
                format!("{} — {}", entry.name, entry.description),
                format!("  Usage: {}", entry.usage),
            ];
            if !entry.examples.is_empty() {
                lines.push("  Examples:".to_string());
                for ex in &entry.examples {
                    lines.push(format!("    {ex}"));
                }
            }
            return ReplOutput::text(lines);
        }
        return ReplOutput::text(vec![format!("Unknown command: {name}")]);
    }

    let mut lines = vec!["Available Commands:".to_string()];
    for entry in &entries {
        lines.push(format!("  {:12} {}", entry.name, entry.description));
    }
    lines.push(String::new());
    lines.push("Type 'help <command>' for detailed help on a specific command.".to_string());
    ReplOutput::text(lines)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // 1. Parse help command
    #[test]
    fn test_parse_help() {
        assert_eq!(ReplCommand::parse("help"), ReplCommand::Help(None));
        assert_eq!(
            ReplCommand::parse("help skills"),
            ReplCommand::Help(Some("skills".to_string()))
        );
        assert_eq!(ReplCommand::parse("?"), ReplCommand::Help(None));
        assert_eq!(ReplCommand::parse("h"), ReplCommand::Help(None));
    }

    // 2. Parse skills command
    #[test]
    fn test_parse_skills() {
        assert_eq!(ReplCommand::parse("skills"), ReplCommand::Skills);
        assert_eq!(
            ReplCommand::parse("skills file_write"),
            ReplCommand::SkillInfo("file_write".to_string())
        );
        assert_eq!(ReplCommand::parse("skill"), ReplCommand::Skills);
    }

    // 3. Parse set/get commands
    #[test]
    fn test_parse_set_get() {
        assert_eq!(
            ReplCommand::parse("set model gpt-4"),
            ReplCommand::Set("model".to_string(), "gpt-4".to_string())
        );
        assert_eq!(
            ReplCommand::parse("get model"),
            ReplCommand::Get("model".to_string())
        );
    }

    // 4. Parse exit commands
    #[test]
    fn test_parse_exit() {
        assert_eq!(ReplCommand::parse("exit"), ReplCommand::Exit);
        assert_eq!(ReplCommand::parse("quit"), ReplCommand::Exit);
        assert_eq!(ReplCommand::parse("q"), ReplCommand::Exit);
    }

    // 5. Parse empty input
    #[test]
    fn test_parse_empty() {
        assert_eq!(ReplCommand::parse(""), ReplCommand::Empty);
        assert_eq!(ReplCommand::parse("   "), ReplCommand::Empty);
    }

    // 6. Parse unknown command
    #[test]
    fn test_parse_unknown() {
        assert_eq!(
            ReplCommand::parse("foobar"),
            ReplCommand::Unknown("foobar".to_string())
        );
    }

    // 7. Parse misc commands
    #[test]
    fn test_parse_misc() {
        assert_eq!(ReplCommand::parse("sessions"), ReplCommand::Sessions);
        assert_eq!(ReplCommand::parse("config"), ReplCommand::Config);
        assert_eq!(ReplCommand::parse("metrics"), ReplCommand::Metrics);
        assert_eq!(ReplCommand::parse("health"), ReplCommand::Health);
        assert_eq!(ReplCommand::parse("history"), ReplCommand::History);
        assert_eq!(ReplCommand::parse("clear"), ReplCommand::Clear);
        assert_eq!(ReplCommand::parse("cls"), ReplCommand::Clear);
        assert_eq!(ReplCommand::parse("version"), ReplCommand::Version);
        assert_eq!(ReplCommand::parse("ver"), ReplCommand::Version);
    }

    // 8. Case insensitive parsing
    #[test]
    fn test_case_insensitive() {
        assert_eq!(ReplCommand::parse("HELP"), ReplCommand::Help(None));
        assert_eq!(ReplCommand::parse("Skills"), ReplCommand::Skills);
        assert_eq!(ReplCommand::parse("EXIT"), ReplCommand::Exit);
    }

    // 9. Context set and get
    #[test]
    fn test_context_set_get() {
        let mut ctx = ReplContext::new();
        ctx.set("model", "gpt-4");
        assert_eq!(ctx.get("model").unwrap(), "gpt-4");
        assert!(ctx.get("nonexistent").is_none());
    }

    // 10. Context history
    #[test]
    fn test_context_history() {
        let mut ctx = ReplContext::new();
        ctx.add_to_history("help");
        ctx.add_to_history("skills");
        ctx.add_to_history("exit");
        assert_eq!(ctx.history.len(), 3);
        assert_eq!(ctx.recent_history(2), &["skills".to_string(), "exit".to_string()]);
    }

    // 11. History max size
    #[test]
    fn test_history_max_size() {
        let mut ctx = ReplContext::new();
        ctx.max_history = 3;
        for i in 0..5 {
            ctx.add_to_history(format!("cmd-{i}"));
        }
        assert_eq!(ctx.history.len(), 3);
        assert_eq!(ctx.history[0], "cmd-2");
    }

    // 12. Empty string not added to history
    #[test]
    fn test_empty_not_in_history() {
        let mut ctx = ReplContext::new();
        ctx.add_to_history("");
        assert!(ctx.history.is_empty());
    }

    // 13. Execute help command
    #[test]
    fn test_execute_help() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Help(None), &mut ctx);
        assert!(!output.lines.is_empty());
        assert!(!output.should_exit);
    }

    // 14. Execute specific help
    #[test]
    fn test_execute_specific_help() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Help(Some("skills".to_string())), &mut ctx);
        assert!(output.lines[0].contains("skills"));
    }

    // 15. Execute set command
    #[test]
    fn test_execute_set() {
        let mut ctx = ReplContext::new();
        let output = execute_command(
            &ReplCommand::Set("model".to_string(), "gpt-4".to_string()),
            &mut ctx,
        );
        assert!(output.lines[0].contains("Set model = gpt-4"));
        assert_eq!(ctx.get("model").unwrap(), "gpt-4");
    }

    // 16. Execute get command
    #[test]
    fn test_execute_get() {
        let mut ctx = ReplContext::new();
        ctx.set("key", "value");
        let output = execute_command(&ReplCommand::Get("key".to_string()), &mut ctx);
        assert!(output.lines[0].contains("key = value"));
    }

    // 17. Execute get missing key
    #[test]
    fn test_execute_get_missing() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Get("nope".to_string()), &mut ctx);
        assert!(output.lines[0].contains("not set"));
    }

    // 18. Execute exit
    #[test]
    fn test_execute_exit() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Exit, &mut ctx);
        assert!(output.should_exit);
    }

    // 19. Execute clear
    #[test]
    fn test_execute_clear() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Clear, &mut ctx);
        assert!(output.should_clear);
    }

    // 20. Execute empty
    #[test]
    fn test_execute_empty() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Empty, &mut ctx);
        assert!(output.lines.is_empty());
        assert!(!output.should_exit);
    }

    // 21. Execute unknown
    #[test]
    fn test_execute_unknown() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Unknown("foo".to_string()), &mut ctx);
        assert!(output.lines[0].contains("Unknown command"));
    }

    // 22. Execute config with values
    #[test]
    fn test_execute_config() {
        let mut ctx = ReplContext::new();
        ctx.set("a", "1");
        ctx.set("b", "2");
        let output = execute_command(&ReplCommand::Config, &mut ctx);
        assert!(output.lines.len() >= 3); // header + 2 values
    }

    // 23. Execute config empty
    #[test]
    fn test_execute_config_empty() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Config, &mut ctx);
        assert!(output.lines[1].contains("no values set"));
    }

    // 24. Command help entries are complete
    #[test]
    fn test_command_help_entries() {
        let entries = command_help_entries();
        assert!(entries.len() >= 10);
        for entry in &entries {
            assert!(!entry.name.is_empty());
            assert!(!entry.description.is_empty());
            assert!(!entry.usage.is_empty());
        }
    }

    // 25. Set with spaces in value
    #[test]
    fn test_set_with_spaces() {
        assert_eq!(
            ReplCommand::parse("set prompt hello world"),
            ReplCommand::Set("prompt".to_string(), "hello world".to_string())
        );
    }

    // 26. Version output
    #[test]
    fn test_execute_version() {
        let mut ctx = ReplContext::new();
        let output = execute_command(&ReplCommand::Version, &mut ctx);
        assert!(output.lines[0].contains("Argentor REPL"));
    }

    // 27. Help for unknown command
    #[test]
    fn test_help_unknown_command() {
        let mut ctx = ReplContext::new();
        let output = execute_command(
            &ReplCommand::Help(Some("nonexistent".to_string())),
            &mut ctx,
        );
        assert!(output.lines[0].contains("Unknown command"));
    }
}
