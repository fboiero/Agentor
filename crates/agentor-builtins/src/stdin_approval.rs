use agentor_core::approval::{ApprovalChannel, ApprovalDecision, ApprovalRequest, RiskLevel};
use agentor_core::AgentorResult;
use async_trait::async_trait;
use std::time::Duration;

/// CLI-based approval channel that prompts the user via stdin/stderr.
///
/// Prints a colored approval request to stderr and reads the user's
/// decision from stdin. Designed for interactive terminal use with
/// the `agentor orchestrate --interactive-approval` flag.
pub struct StdinApprovalChannel {
    timeout: Duration,
}

impl StdinApprovalChannel {
    /// Create with a custom timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Create with the default 5-minute timeout.
    pub fn default_timeout() -> Self {
        Self {
            timeout: Duration::from_secs(300),
        }
    }
}

/// Format the approval prompt for display on stderr.
/// Returns the full prompt string with ANSI colors.
pub fn format_approval_prompt(request: &ApprovalRequest) -> String {
    let (color, label) = risk_level_style(&request.risk_level);

    let mut prompt = String::new();
    prompt.push_str("\n\x1b[1;37m╔══ APPROVAL REQUIRED ══╗\x1b[0m\n");
    prompt.push_str(&format!(
        "  Task:  {}\n",
        request.task_id
    ));
    prompt.push_str(&format!(
        "  Risk:  \x1b[{color}m{label}\x1b[0m\n"
    ));
    prompt.push_str(&format!(
        "  Desc:  {}\n",
        request.description
    ));
    if !request.context.is_empty() {
        prompt.push_str(&format!(
            "  Info:  {}\n",
            request.context
        ));
    }
    prompt.push_str("\x1b[1;37m╚═══════════════════════╝\x1b[0m\n");
    prompt.push_str("  Approve? [y/N/reason]: ");
    prompt
}

/// Returns (ANSI color code, label) for a given risk level.
pub fn risk_level_style(level: &RiskLevel) -> (&'static str, &'static str) {
    match level {
        RiskLevel::Low => ("32", "LOW"),
        RiskLevel::Medium => ("36", "MEDIUM"),
        RiskLevel::High => ("33", "HIGH"),
        RiskLevel::Critical => ("1;31", "CRITICAL"),
    }
}

/// Parse user input into an approval decision.
pub fn parse_approval_input(input: &str) -> (bool, Option<String>) {
    let trimmed = input.trim().to_lowercase();
    match trimmed.as_str() {
        "y" | "yes" => (true, None),
        "n" | "no" | "" => (false, None),
        other => (false, Some(other.to_string())),
    }
}

#[async_trait]
impl ApprovalChannel for StdinApprovalChannel {
    async fn request_approval(&self, request: ApprovalRequest) -> AgentorResult<ApprovalDecision> {
        let prompt = format_approval_prompt(&request);
        let timeout = self.timeout;

        // Print prompt to stderr (preserving stdout for piped output)
        eprint!("{prompt}");

        // Read from stdin in a blocking task with timeout
        let result = tokio::time::timeout(timeout, tokio::task::spawn_blocking(|| {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            input
        }))
        .await;

        let reviewer = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "cli-user".to_string());

        match result {
            Ok(Ok(input)) => {
                let (approved, reason) = parse_approval_input(&input);
                let decision_text = if approved { "APPROVED" } else { "DENIED" };
                eprintln!("  → {decision_text}\n");
                Ok(ApprovalDecision {
                    approved,
                    reason,
                    reviewer,
                })
            }
            Ok(Err(_)) => {
                eprintln!("  → DENIED (stdin error)\n");
                Ok(ApprovalDecision {
                    approved: false,
                    reason: Some("stdin read error".into()),
                    reviewer,
                })
            }
            Err(_) => {
                eprintln!("\n  → DENIED (timeout after {}s)\n", timeout.as_secs());
                Ok(ApprovalDecision {
                    approved: false,
                    reason: Some(format!("Timed out after {}s", timeout.as_secs())),
                    reviewer,
                })
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_format_approval_prompt_critical() {
        let request = ApprovalRequest {
            task_id: "deploy-prod".to_string(),
            description: "Deploy to production".to_string(),
            risk_level: RiskLevel::Critical,
            context: "Drops legacy table".to_string(),
        };
        let prompt = format_approval_prompt(&request);
        assert!(prompt.contains("deploy-prod"));
        assert!(prompt.contains("CRITICAL"));
        assert!(prompt.contains("Deploy to production"));
        assert!(prompt.contains("Drops legacy table"));
        assert!(prompt.contains("1;31")); // Red ANSI code
    }

    #[test]
    fn test_format_approval_prompt_no_context() {
        let request = ApprovalRequest {
            task_id: "task-1".to_string(),
            description: "Simple action".to_string(),
            risk_level: RiskLevel::Low,
            context: String::new(),
        };
        let prompt = format_approval_prompt(&request);
        assert!(!prompt.contains("Info:"));
        assert!(prompt.contains("LOW"));
        assert!(prompt.contains("32")); // Green ANSI code
    }

    #[test]
    fn test_risk_level_styles() {
        assert_eq!(risk_level_style(&RiskLevel::Low), ("32", "LOW"));
        assert_eq!(risk_level_style(&RiskLevel::Medium), ("36", "MEDIUM"));
        assert_eq!(risk_level_style(&RiskLevel::High), ("33", "HIGH"));
        assert_eq!(risk_level_style(&RiskLevel::Critical), ("1;31", "CRITICAL"));
    }

    #[test]
    fn test_parse_approval_input() {
        assert_eq!(parse_approval_input("y"), (true, None));
        assert_eq!(parse_approval_input("yes"), (true, None));
        assert_eq!(parse_approval_input("YES"), (true, None));
        assert_eq!(parse_approval_input("n"), (false, None));
        assert_eq!(parse_approval_input("no"), (false, None));
        assert_eq!(parse_approval_input(""), (false, None));
        assert_eq!(
            parse_approval_input("too risky"),
            (false, Some("too risky".to_string()))
        );
    }

    #[test]
    fn test_parse_approval_input_whitespace() {
        assert_eq!(parse_approval_input("  y  "), (true, None));
        assert_eq!(parse_approval_input("\n"), (false, None));
    }
}
