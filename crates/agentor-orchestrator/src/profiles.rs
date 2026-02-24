use crate::types::{AgentProfile, AgentRole};
use agentor_agent::ModelConfig;

/// Create default agent profiles for the multi-agent system.
/// Uses the provided base config as template, adjusting per role.
pub fn default_profiles(base_config: &ModelConfig) -> Vec<AgentProfile> {
    vec![
        orchestrator_profile(base_config),
        spec_profile(base_config),
        coder_profile(base_config),
        tester_profile(base_config),
        reviewer_profile(base_config),
    ]
}

fn orchestrator_profile(base: &ModelConfig) -> AgentProfile {
    let mut model = base.clone();
    model.max_turns = 30;
    model.temperature = 0.3;

    AgentProfile {
        role: AgentRole::Orchestrator,
        model,
        system_prompt: ORCHESTRATOR_PROMPT.to_string(),
        allowed_skills: vec![
            "agent_delegate".to_string(),
            "task_status".to_string(),
            "human_approval".to_string(),
            "artifact_store".to_string(),
            "memory_search".to_string(),
        ],
        tool_group: Some("orchestration".to_string()),
        max_turns: 30,
    }
}

fn spec_profile(base: &ModelConfig) -> AgentProfile {
    let mut model = base.clone();
    model.max_turns = 15;
    model.temperature = 0.4;

    AgentProfile {
        role: AgentRole::Spec,
        model,
        system_prompt: SPEC_PROMPT.to_string(),
        allowed_skills: vec![
            "memory_search".to_string(),
            "memory_store".to_string(),
        ],
        tool_group: Some("minimal".to_string()),
        max_turns: 15,
    }
}

fn coder_profile(base: &ModelConfig) -> AgentProfile {
    let mut model = base.clone();
    model.max_turns = 20;
    model.temperature = 0.2;

    AgentProfile {
        role: AgentRole::Coder,
        model,
        system_prompt: CODER_PROMPT.to_string(),
        allowed_skills: vec!["memory_search".to_string()],
        tool_group: Some("minimal".to_string()),
        max_turns: 20,
    }
}

fn tester_profile(base: &ModelConfig) -> AgentProfile {
    let mut model = base.clone();
    model.max_turns = 15;
    model.temperature = 0.2;

    AgentProfile {
        role: AgentRole::Tester,
        model,
        system_prompt: TESTER_PROMPT.to_string(),
        allowed_skills: vec!["memory_search".to_string()],
        tool_group: Some("minimal".to_string()),
        max_turns: 15,
    }
}

fn reviewer_profile(base: &ModelConfig) -> AgentProfile {
    let mut model = base.clone();
    model.max_turns = 10;
    model.temperature = 0.3;

    AgentProfile {
        role: AgentRole::Reviewer,
        model,
        system_prompt: REVIEWER_PROMPT.to_string(),
        allowed_skills: vec![
            "memory_search".to_string(),
            "human_approval".to_string(),
        ],
        tool_group: Some("minimal".to_string()),
        max_turns: 10,
    }
}

const ORCHESTRATOR_PROMPT: &str = "\
You are the Orchestrator agent in a multi-agent system called Agentor. \
Your job is to decompose complex tasks into subtasks, delegate them to \
specialized worker agents (Spec, Coder, Tester, Reviewer), and synthesize \
the final result.

Rules:
1. Break tasks into clear, independent subtasks when possible.
2. Assign each subtask to the most appropriate worker role.
3. Define dependencies between subtasks (e.g., Coder depends on Spec).
4. Monitor progress and handle failures by reassigning or adjusting.
5. Request human approval for high-risk operations (security changes, \
   deployments, data deletion).
6. Synthesize all artifacts into a coherent final deliverable.
7. Never write code yourself — delegate to Coder.
";

const SPEC_PROMPT: &str = "\
You are the Spec agent in Agentor. Your job is to analyze requirements \
and produce clear, actionable technical specifications.

IMPORTANT: Respond with your specification as plain text. Do NOT call any tools — \
just write your analysis and specification directly in your response.

Rules:
1. Break requirements into concrete implementation tasks.
2. Define interfaces, data types, and contracts.
3. Identify edge cases, security considerations, and constraints.
4. Reference existing code patterns and utilities when applicable.
5. Output your specification directly as text.
";

const CODER_PROMPT: &str = "\
You are the Coder agent in Agentor. You write secure, idiomatic Rust code \
following the project's patterns and conventions.

IMPORTANT: Respond with your code as plain text. Do NOT call any tools — \
just write the code directly in your response. Use markdown code blocks.

Rules:
1. Follow the specification provided by the Spec agent.
2. Write memory-safe, secure code — no unwrap() in production paths.
3. Use existing project patterns (traits, error handling, async).
4. Keep code simple — no over-engineering or premature abstractions.
5. Include inline comments only where logic is non-obvious.
6. Output code directly in your response with file paths as comments.
";

const TESTER_PROMPT: &str = "\
You are the Tester agent in Agentor. You write comprehensive tests \
for Rust code.

IMPORTANT: Respond with your test code as plain text. Do NOT call any tools — \
just write the tests directly in your response. Use markdown code blocks.

Rules:
1. Write unit tests covering happy paths, edge cases, and error conditions.
2. Follow existing test patterns in the project (tokio::test for async).
3. Test security boundaries and permission checks.
4. Use descriptive test names (test_<function>_<scenario>).
5. Output test code directly in your response.
";

const REVIEWER_PROMPT: &str = "\
You are the Reviewer agent in Agentor. You review code for quality, \
security, and compliance.

IMPORTANT: Respond with your review as plain text. Do NOT call any tools — \
just write your review report directly in your response.

Rules:
1. Check for OWASP Top 10 vulnerabilities.
2. Verify capability-based permission checks are in place.
3. Ensure proper error handling (no unwrap in prod paths).
4. Check for compliance with GDPR, ISO 27001, ISO 42001 requirements.
5. Flag any issues that require human review.
6. Output your review report directly in your response.
";

#[cfg(test)]
mod tests {
    use super::*;
    use agentor_agent::LlmProvider;

    fn test_config() -> ModelConfig {
        ModelConfig {
            provider: LlmProvider::Claude,
            model_id: "claude-sonnet-4-20250514".to_string(),
            api_key: "test-key".to_string(),
            api_base_url: None,
            temperature: 0.7,
            max_tokens: 4096,
            max_turns: 20,
            fallback_models: Vec::new(),
            retry_policy: None,
        }
    }

    #[test]
    fn test_default_profiles_count() {
        let profiles = default_profiles(&test_config());
        assert_eq!(profiles.len(), 5);
    }

    #[test]
    fn test_all_roles_covered() {
        let profiles = default_profiles(&test_config());
        let roles: Vec<AgentRole> = profiles.iter().map(|p| p.role).collect();
        assert!(roles.contains(&AgentRole::Orchestrator));
        assert!(roles.contains(&AgentRole::Spec));
        assert!(roles.contains(&AgentRole::Coder));
        assert!(roles.contains(&AgentRole::Tester));
        assert!(roles.contains(&AgentRole::Reviewer));
    }

    #[test]
    fn test_orchestrator_has_delegate_skill() {
        let profiles = default_profiles(&test_config());
        let orch = profiles
            .iter()
            .find(|p| p.role == AgentRole::Orchestrator)
            .unwrap();
        assert!(orch.allowed_skills.contains(&"agent_delegate".to_string()));
        assert!(orch.allowed_skills.contains(&"human_approval".to_string()));
    }

    #[test]
    fn test_coder_low_temperature() {
        let profiles = default_profiles(&test_config());
        let coder = profiles
            .iter()
            .find(|p| p.role == AgentRole::Coder)
            .unwrap();
        assert!(coder.model.temperature <= 0.3);
    }

    #[test]
    fn test_profiles_have_system_prompts() {
        let profiles = default_profiles(&test_config());
        for profile in &profiles {
            assert!(!profile.system_prompt.is_empty());
        }
    }
}
