use crate::registry::SkillRegistry;
use crate::wasm_runtime::WasmSkillRuntime;
use agentor_core::{AgentorError, AgentorResult};
use agentor_security::Capability;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

/// Skill entry from the TOML configuration file.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillConfig {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub skill_type: SkillType,
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub parameters_schema: serde_json::Value,
    #[serde(default)]
    pub capabilities: CapabilityConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    Wasm,
    Native,
}

/// Capabilities declared in config for a skill.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CapabilityConfig {
    #[serde(default)]
    pub file_read: Vec<String>,
    #[serde(default)]
    pub file_write: Vec<String>,
    #[serde(default)]
    pub network_access: Vec<String>,
    #[serde(default)]
    pub shell_exec: Vec<String>,
    #[serde(default)]
    pub env_read: Vec<String>,
    #[serde(default)]
    pub browser_access: Vec<String>,
    #[serde(default)]
    pub database_query: bool,
}

impl CapabilityConfig {
    pub fn to_capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();

        if !self.file_read.is_empty() {
            caps.push(Capability::FileRead {
                allowed_paths: self.file_read.clone(),
            });
        }
        if !self.file_write.is_empty() {
            caps.push(Capability::FileWrite {
                allowed_paths: self.file_write.clone(),
            });
        }
        if !self.network_access.is_empty() {
            caps.push(Capability::NetworkAccess {
                allowed_hosts: self.network_access.clone(),
            });
        }
        if !self.shell_exec.is_empty() {
            caps.push(Capability::ShellExec {
                allowed_commands: self.shell_exec.clone(),
            });
        }
        if !self.env_read.is_empty() {
            caps.push(Capability::EnvRead {
                allowed_vars: self.env_read.clone(),
            });
        }
        if !self.browser_access.is_empty() {
            caps.push(Capability::BrowserAccess {
                allowed_domains: self.browser_access.clone(),
            });
        }
        if self.database_query {
            caps.push(Capability::DatabaseQuery);
        }

        caps
    }
}

/// Loads skills from configuration into a registry.
pub struct SkillLoader {
    wasm_runtime: WasmSkillRuntime,
}

impl SkillLoader {
    pub fn new() -> AgentorResult<Self> {
        Ok(Self {
            wasm_runtime: WasmSkillRuntime::new()?,
        })
    }

    /// Load all skills from config into the registry.
    pub fn load_all(
        &self,
        configs: &[SkillConfig],
        base_dir: &Path,
        registry: &mut SkillRegistry,
    ) -> AgentorResult<usize> {
        let mut loaded = 0;

        for config in configs {
            match self.load_one(config, base_dir, registry) {
                Ok(()) => {
                    info!(skill = %config.name, "Loaded skill");
                    loaded += 1;
                }
                Err(e) => {
                    warn!(skill = %config.name, error = %e, "Failed to load skill, skipping");
                }
            }
        }

        info!(total = loaded, "Skills loaded");
        Ok(loaded)
    }

    fn load_one(
        &self,
        config: &SkillConfig,
        base_dir: &Path,
        registry: &mut SkillRegistry,
    ) -> AgentorResult<()> {
        match config.skill_type {
            SkillType::Wasm => {
                let path = config
                    .path
                    .as_ref()
                    .ok_or_else(|| {
                        AgentorError::Config(format!(
                            "WASM skill '{}' requires a 'path' field",
                            config.name
                        ))
                    })?;

                let full_path = if path.is_absolute() {
                    path.clone()
                } else {
                    base_dir.join(path)
                };

                if !full_path.exists() {
                    return Err(AgentorError::Config(format!(
                        "WASM skill '{}' path does not exist: {}",
                        config.name,
                        full_path.display()
                    )));
                }

                let capabilities = config.capabilities.to_capabilities();

                let skill = self.wasm_runtime.load_skill(
                    &full_path,
                    config.name.clone(),
                    config.description.clone(),
                    config.parameters_schema.clone(),
                    capabilities,
                )?;

                registry.register(Arc::new(skill));
                Ok(())
            }
            SkillType::Native => {
                warn!(
                    skill = %config.name,
                    "Native skills must be registered programmatically, skipping config entry"
                );
                Ok(())
            }
        }
    }
}

impl Default for SkillLoader {
    fn default() -> Self {
        Self::new().expect("Failed to create SkillLoader")
    }
}
