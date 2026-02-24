use crate::skill::{Skill, SkillDescriptor};
use agentor_core::{AgentorError, AgentorResult, ToolCall, ToolResult};
use agentor_security::PermissionSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// A named group of skills for progressive tool disclosure.
///
/// Tool groups allow exposing different sets of skills depending on context:
/// - `minimal`: basic utilities only (echo, time, help)
/// - `coding`: file operations, shell, memory
/// - `web`: HTTP fetch, browser
/// - `messaging`: Slack, Discord, Telegram
/// - `full`: all registered skills
/// - Custom groups defined in agentor.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGroup {
    pub name: String,
    pub description: String,
    pub skills: Vec<String>,
}

impl ToolGroup {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        skills: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            skills,
        }
    }
}

/// Default tool groups that ship with Agentor.
pub fn default_tool_groups() -> Vec<ToolGroup> {
    vec![
        ToolGroup::new(
            "minimal",
            "Basic utilities — safe for any context",
            vec!["echo".into(), "time".into(), "help".into()],
        ),
        ToolGroup::new(
            "coding",
            "File operations, shell, and memory for development tasks",
            vec![
                "file_read".into(),
                "file_write".into(),
                "shell".into(),
                "memory_store".into(),
                "memory_search".into(),
            ],
        ),
        ToolGroup::new(
            "web",
            "HTTP and browser access for web tasks",
            vec!["http_fetch".into(), "browser".into()],
        ),
        ToolGroup::new(
            "orchestration",
            "Skills for the orchestrator agent — delegation, approval, artifacts",
            vec![
                "agent_delegate".into(),
                "task_status".into(),
                "human_approval".into(),
                "artifact_store".into(),
                "memory_search".into(),
            ],
        ),
        ToolGroup::new(
            "full",
            "All registered skills — use with caution",
            vec![], // Empty = all skills
        ),
    ]
}

/// Central registry for all available skills.
pub struct SkillRegistry {
    skills: HashMap<String, Arc<dyn Skill>>,
    groups: HashMap<String, ToolGroup>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        let groups: HashMap<String, ToolGroup> = default_tool_groups()
            .into_iter()
            .map(|g| (g.name.clone(), g))
            .collect();

        Self {
            skills: HashMap::new(),
            groups,
        }
    }

    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        let name = skill.descriptor().name.clone();
        info!(skill = %name, "Registered skill");
        self.skills.insert(name, skill);
    }

    /// Register a custom tool group.
    pub fn register_group(&mut self, group: ToolGroup) {
        info!(group = %group.name, skills = group.skills.len(), "Registered tool group");
        self.groups.insert(group.name.clone(), group);
    }

    /// Register multiple custom tool groups.
    pub fn register_groups(&mut self, groups: Vec<ToolGroup>) {
        for group in groups {
            self.register_group(group);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Skill>> {
        self.skills.get(name)
    }

    pub fn list_descriptors(&self) -> Vec<&SkillDescriptor> {
        self.skills.values().map(|s| s.descriptor()).collect()
    }

    /// List all registered tool groups.
    pub fn list_groups(&self) -> Vec<&ToolGroup> {
        self.groups.values().collect()
    }

    /// Get a tool group by name.
    pub fn get_group(&self, name: &str) -> Option<&ToolGroup> {
        self.groups.get(name)
    }

    /// Execute a tool call, checking permissions first.
    pub async fn execute(
        &self,
        call: ToolCall,
        permissions: &PermissionSet,
    ) -> AgentorResult<ToolResult> {
        let skill = self
            .skills
            .get(&call.name)
            .ok_or_else(|| AgentorError::Skill(format!("Unknown skill: {}", call.name)))?;

        // Check required capabilities
        for cap in &skill.descriptor().required_capabilities {
            if !permissions.has(cap) {
                warn!(
                    skill = %call.name,
                    capability = ?cap,
                    "Permission denied for skill execution"
                );
                return Ok(ToolResult::error(
                    &call.id,
                    format!(
                        "Permission denied: skill '{}' requires capability {:?}",
                        call.name, cap
                    ),
                ));
            }
        }

        skill.execute(call).await
    }

    /// Return only skills whose names appear in the given list.
    /// Used for progressive tool disclosure in multi-agent orchestration.
    pub fn filter_by_names(&self, names: &[String]) -> Vec<&SkillDescriptor> {
        let allowed: std::collections::HashSet<&str> = names.iter().map(std::string::String::as_str).collect();
        self.skills
            .values()
            .filter(|s| allowed.contains(s.descriptor().name.as_str()))
            .map(|s| s.descriptor())
            .collect()
    }

    /// Create a new registry containing only skills whose names appear in the given list.
    /// Used for progressive tool disclosure — each worker agent gets only the skills it needs.
    pub fn filter_to_new(&self, names: &[String]) -> Self {
        let allowed: std::collections::HashSet<&str> = names.iter().map(std::string::String::as_str).collect();
        let skills = self
            .skills
            .iter()
            .filter(|(name, _)| allowed.contains(name.as_str()))
            .map(|(name, skill)| (name.clone(), skill.clone()))
            .collect();
        Self {
            skills,
            groups: self.groups.clone(),
        }
    }

    /// Create a new registry containing only skills in the specified tool group.
    /// If the group has an empty skills list (like "full"), returns all skills.
    pub fn filter_by_group(&self, group_name: &str) -> AgentorResult<Self> {
        let group = self
            .groups
            .get(group_name)
            .ok_or_else(|| AgentorError::Config(format!("Unknown tool group: {group_name}")))?;

        if group.skills.is_empty() {
            // "full" group — return everything
            return Ok(Self {
                skills: self.skills.clone(),
                groups: self.groups.clone(),
            });
        }

        let skills = self.filter_to_new(&group.skills);
        Ok(skills)
    }

    /// Get skill names that belong to a specific group.
    pub fn skills_in_group(&self, group_name: &str) -> Vec<String> {
        match self.groups.get(group_name) {
            Some(group) if group.skills.is_empty() => {
                // "full" = all skills
                self.skills.keys().cloned().collect()
            }
            Some(group) => {
                // Return only skills that exist in both the group definition and registry
                group
                    .skills
                    .iter()
                    .filter(|name| self.skills.contains_key(name.as_str()))
                    .cloned()
                    .collect()
            }
            None => vec![],
        }
    }

    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::skill::{Skill, SkillDescriptor};
    use agentor_core::{AgentorResult, ToolCall, ToolResult};
    use async_trait::async_trait;

    struct TestSkill {
        descriptor: SkillDescriptor,
    }

    impl TestSkill {
        fn new(name: &str) -> Self {
            Self {
                descriptor: SkillDescriptor {
                    name: name.to_string(),
                    description: format!("Test skill {name}"),
                    parameters_schema: serde_json::json!({}),
                    required_capabilities: vec![],
                },
            }
        }
    }

    #[async_trait]
    impl Skill for TestSkill {
        fn descriptor(&self) -> &SkillDescriptor {
            &self.descriptor
        }
        async fn execute(&self, call: ToolCall) -> AgentorResult<ToolResult> {
            Ok(ToolResult::success(&call.id, "ok"))
        }
    }

    #[test]
    fn test_filter_by_names_subset() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("echo")));
        reg.register(Arc::new(TestSkill::new("time")));
        reg.register(Arc::new(TestSkill::new("memory_store")));

        let names = vec!["echo".to_string(), "time".to_string()];
        let filtered = reg.filter_by_names(&names);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_by_names_empty() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("echo")));

        let filtered = reg.filter_by_names(&[]);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_filter_by_names_no_match() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("echo")));

        let names = vec!["nonexistent".to_string()];
        let filtered = reg.filter_by_names(&names);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_registry_basic_operations() {
        let mut reg = SkillRegistry::new();
        assert_eq!(reg.skill_count(), 0);

        reg.register(Arc::new(TestSkill::new("echo")));
        assert_eq!(reg.skill_count(), 1);
        assert!(reg.get("echo").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_list_descriptors() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("a")));
        reg.register(Arc::new(TestSkill::new("b")));

        let descs = reg.list_descriptors();
        assert_eq!(descs.len(), 2);
    }

    #[test]
    fn test_default_tool_groups() {
        let reg = SkillRegistry::new();
        let groups = reg.list_groups();
        assert!(groups.len() >= 4);

        // Check that standard groups exist
        assert!(reg.get_group("minimal").is_some());
        assert!(reg.get_group("coding").is_some());
        assert!(reg.get_group("web").is_some());
        assert!(reg.get_group("full").is_some());
    }

    #[test]
    fn test_filter_by_group_minimal() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("echo")));
        reg.register(Arc::new(TestSkill::new("time")));
        reg.register(Arc::new(TestSkill::new("help")));
        reg.register(Arc::new(TestSkill::new("file_read")));
        reg.register(Arc::new(TestSkill::new("shell")));

        let minimal = reg.filter_by_group("minimal").unwrap();
        assert_eq!(minimal.skill_count(), 3);
        assert!(minimal.get("echo").is_some());
        assert!(minimal.get("time").is_some());
        assert!(minimal.get("help").is_some());
        assert!(minimal.get("file_read").is_none());
    }

    #[test]
    fn test_filter_by_group_full() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("echo")));
        reg.register(Arc::new(TestSkill::new("file_read")));
        reg.register(Arc::new(TestSkill::new("shell")));

        let full = reg.filter_by_group("full").unwrap();
        assert_eq!(full.skill_count(), 3);
    }

    #[test]
    fn test_filter_by_group_unknown() {
        let reg = SkillRegistry::new();
        assert!(reg.filter_by_group("nonexistent").is_err());
    }

    #[test]
    fn test_custom_tool_group() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("my_tool")));
        reg.register(Arc::new(TestSkill::new("other_tool")));

        reg.register_group(ToolGroup::new(
            "custom",
            "Custom group",
            vec!["my_tool".into()],
        ));

        let custom = reg.filter_by_group("custom").unwrap();
        assert_eq!(custom.skill_count(), 1);
        assert!(custom.get("my_tool").is_some());
    }

    #[test]
    fn test_skills_in_group() {
        let mut reg = SkillRegistry::new();
        reg.register(Arc::new(TestSkill::new("echo")));
        reg.register(Arc::new(TestSkill::new("time")));

        // Minimal group includes echo, time, help — but help isn't registered
        let skills = reg.skills_in_group("minimal");
        assert_eq!(skills.len(), 2); // echo and time only
        assert!(skills.contains(&"echo".to_string()));
        assert!(skills.contains(&"time".to_string()));
    }

    #[test]
    fn test_register_groups_batch() {
        let mut reg = SkillRegistry::new();
        reg.register_groups(vec![
            ToolGroup::new("a", "Group A", vec!["x".into()]),
            ToolGroup::new("b", "Group B", vec!["y".into()]),
        ]);
        assert!(reg.get_group("a").is_some());
        assert!(reg.get_group("b").is_some());
    }
}
