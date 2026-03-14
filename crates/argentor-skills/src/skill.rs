use argentor_core::{ArgentorResult, ToolCall, ToolResult};
use argentor_security::Capability;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Metadata describing a skill's interface and required permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub required_capabilities: Vec<Capability>,
}

/// Trait that all skills must implement — whether native Rust or WASM.
#[async_trait]
pub trait Skill: Send + Sync {
    fn descriptor(&self) -> &SkillDescriptor;

    async fn execute(&self, call: ToolCall) -> ArgentorResult<ToolResult>;
}
