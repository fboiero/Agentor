use crate::skill::{Skill, SkillDescriptor};
use agentor_core::{AgentorError, AgentorResult, ToolCall, ToolResult};
use agentor_security::PermissionSet;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Central registry for all available skills.
pub struct SkillRegistry {
    skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        let name = skill.descriptor().name.clone();
        info!(skill = %name, "Registered skill");
        self.skills.insert(name, skill);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Skill>> {
        self.skills.get(name)
    }

    pub fn list_descriptors(&self) -> Vec<&SkillDescriptor> {
        self.skills.values().map(|s| s.descriptor()).collect()
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

    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
