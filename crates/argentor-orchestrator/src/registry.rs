//! Centralized agent registry that tracks all agent definitions,
//! their capabilities, and deployment state.
//!
//! The [`AgentRegistry`] provides thread-safe registration, lookup,
//! filtering, and catalog import/export for [`AgentDefinition`]s.

use crate::types::AgentRole;
use argentor_core::{ArgentorError, ArgentorResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Complete definition of an agent in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Unique agent identifier.
    pub id: Uuid,
    /// Human-readable agent name.
    pub name: String,
    /// Role this agent fulfills.
    pub role: AgentRole,
    /// Semantic version string.
    pub version: String,
    /// Free-text description of the agent's purpose.
    pub description: String,
    /// High-level capability labels (e.g. "code-generation", "security-audit").
    pub capabilities: Vec<String>,
    /// Names of skills this agent requires at runtime.
    pub required_skills: Vec<String>,
    /// Preferred LLM model identifier (e.g. "claude-sonnet-4-20250514").
    pub model_preference: Option<String>,
    /// Arbitrary key-value labels (e.g. "team" -> "backend").
    pub tags: HashMap<String, String>,
    /// UTC timestamp of agent registration.
    pub created_at: DateTime<Utc>,
    /// UTC timestamp of the last update.
    pub updated_at: DateTime<Utc>,
}

/// Partial update payload for an existing [`AgentDefinition`].
///
/// Only fields set to `Some(...)` are applied; `None` fields are left unchanged.
#[derive(Debug, Clone, Default)]
pub struct AgentUpdate {
    /// New name, if changing.
    pub name: Option<String>,
    /// New description, if changing.
    pub description: Option<String>,
    /// Replacement capability labels, if changing.
    pub capabilities: Option<Vec<String>>,
    /// Replacement required skills list, if changing.
    pub required_skills: Option<Vec<String>>,
    /// New model preference (use `Some(None)` to clear it).
    pub model_preference: Option<Option<String>>,
    /// Replacement tags map, if changing.
    pub tags: Option<HashMap<String, String>>,
}

/// Serializable snapshot of every definition in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCatalog {
    /// Catalog format version.
    pub version: String,
    /// UTC timestamp of when this snapshot was exported.
    pub exported_at: DateTime<Utc>,
    /// All agent definitions in the registry at export time.
    pub agents: Vec<AgentDefinition>,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Thread-safe, centralized agent registry.
///
/// All public methods acquire the internal lock for the minimum required
/// duration and never hold it across await points, so the registry is safe
/// to share across async tasks.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    inner: Arc<RwLock<RegistryInner>>,
}

#[derive(Debug, Default)]
struct RegistryInner {
    agents: HashMap<Uuid, AgentDefinition>,
    name_index: HashMap<String, Uuid>,
}

impl AgentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryInner::default())),
        }
    }

    /// Register a new agent definition.
    ///
    /// Returns the assigned `Uuid` on success, or an error if the name is
    /// already taken.
    pub fn register(&self, def: AgentDefinition) -> ArgentorResult<Uuid> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| ArgentorError::Agent(format!("registry lock poisoned: {e}")))?;

        if inner.name_index.contains_key(&def.name) {
            return Err(ArgentorError::Agent(format!(
                "agent with name '{}' already registered",
                def.name
            )));
        }

        let id = def.id;
        inner.name_index.insert(def.name.clone(), id);
        inner.agents.insert(id, def);
        Ok(id)
    }

    /// Remove and return the definition with the given ID.
    pub fn unregister(&self, id: Uuid) -> ArgentorResult<AgentDefinition> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| ArgentorError::Agent(format!("registry lock poisoned: {e}")))?;

        let def = inner
            .agents
            .remove(&id)
            .ok_or_else(|| ArgentorError::Agent(format!("agent {id} not found")))?;
        inner.name_index.remove(&def.name);
        Ok(def)
    }

    /// Apply a partial update to the definition identified by `id`.
    pub fn update(&self, id: Uuid, update: AgentUpdate) -> ArgentorResult<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| ArgentorError::Agent(format!("registry lock poisoned: {e}")))?;

        if !inner.agents.contains_key(&id) {
            return Err(ArgentorError::Agent(format!("agent {id} not found")));
        }

        // If the name is changing, validate uniqueness and update the index.
        if let Some(ref new_name) = update.name {
            let old_name = &inner.agents[&id].name;
            if new_name != old_name {
                if inner.name_index.contains_key(new_name) {
                    return Err(ArgentorError::Agent(format!(
                        "agent with name '{new_name}' already registered"
                    )));
                }
                let old_name_owned = old_name.clone();
                inner.name_index.remove(&old_name_owned);
                inner.name_index.insert(new_name.clone(), id);
            }
        }

        let Some(def) = inner.agents.get_mut(&id) else {
            return Err(ArgentorError::Agent(format!("agent {id} not found")));
        };
        if let Some(name) = update.name {
            def.name = name;
        }
        if let Some(description) = update.description {
            def.description = description;
        }
        if let Some(capabilities) = update.capabilities {
            def.capabilities = capabilities;
        }
        if let Some(required_skills) = update.required_skills {
            def.required_skills = required_skills;
        }
        if let Some(model_preference) = update.model_preference {
            def.model_preference = model_preference;
        }
        if let Some(tags) = update.tags {
            def.tags = tags;
        }
        def.updated_at = Utc::now();
        Ok(())
    }

    /// Look up a definition by its UUID.
    pub fn get(&self, id: Uuid) -> Option<AgentDefinition> {
        let inner = self.inner.read().ok()?;
        inner.agents.get(&id).cloned()
    }

    /// Look up a definition by its unique name.
    pub fn get_by_name(&self, name: &str) -> Option<AgentDefinition> {
        let inner = self.inner.read().ok()?;
        let id = inner.name_index.get(name)?;
        inner.agents.get(id).cloned()
    }

    /// Return all definitions with the given role.
    pub fn get_by_role(&self, role: &AgentRole) -> Vec<AgentDefinition> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner
            .agents
            .values()
            .filter(|d| &d.role == role)
            .cloned()
            .collect()
    }

    /// Return all definitions carrying the specified tag key-value pair.
    pub fn get_by_tag(&self, key: &str, value: &str) -> Vec<AgentDefinition> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner
            .agents
            .values()
            .filter(|d| d.tags.get(key).is_some_and(|v| v == value))
            .cloned()
            .collect()
    }

    /// Search definitions whose name or description contains `query` (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<AgentDefinition> {
        let lower = query.to_lowercase();
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner
            .agents
            .values()
            .filter(|d| {
                d.name.to_lowercase().contains(&lower)
                    || d.description.to_lowercase().contains(&lower)
            })
            .cloned()
            .collect()
    }

    /// Return all registered definitions.
    pub fn list_all(&self) -> Vec<AgentDefinition> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        inner.agents.values().cloned().collect()
    }

    /// Total number of registered definitions.
    pub fn count(&self) -> usize {
        self.inner.read().map_or(0, |g| g.agents.len())
    }

    /// Count of definitions per [`AgentRole`].
    pub fn count_by_role(&self) -> HashMap<AgentRole, usize> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(_) => return HashMap::new(),
        };
        let mut counts: HashMap<AgentRole, usize> = HashMap::new();
        for def in inner.agents.values() {
            *counts.entry(def.role.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Export the full registry as a serializable [`AgentCatalog`].
    pub fn export_catalog(&self) -> AgentCatalog {
        let agents = self.list_all();
        AgentCatalog {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            agents,
        }
    }

    /// Import definitions from a catalog.
    ///
    /// Definitions whose name already exists in the registry are silently
    /// skipped. Returns the number of definitions actually imported.
    pub fn import_catalog(&self, catalog: AgentCatalog) -> ArgentorResult<usize> {
        let mut imported = 0usize;
        for def in catalog.agents {
            match self.register(def) {
                Ok(_) => imported += 1,
                Err(_) => { /* skip duplicates */ }
            }
        }
        Ok(imported)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Predefined definitions for the 9 standard roles
// ---------------------------------------------------------------------------

/// Create [`AgentDefinition`]s for the nine standard agent roles.
///
/// Each definition comes pre-populated with sensible capabilities,
/// required skills, and descriptive metadata.
pub fn default_agent_definitions() -> Vec<AgentDefinition> {
    let now = Utc::now();

    vec![
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "orchestrator".to_string(),
            role: AgentRole::Orchestrator,
            version: "0.1.0".to_string(),
            description: "Decomposes tasks, delegates to workers, and synthesizes results."
                .to_string(),
            capabilities: vec![
                "task-decomposition".to_string(),
                "delegation".to_string(),
                "progress-tracking".to_string(),
                "result-synthesis".to_string(),
            ],
            required_skills: vec![
                "agent_delegate".to_string(),
                "task_status".to_string(),
                "human_approval".to_string(),
                "artifact_store".to_string(),
                "memory_search".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "control-plane".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "spec".to_string(),
            role: AgentRole::Spec,
            version: "0.1.0".to_string(),
            description: "Analyzes requirements and produces technical specifications.".to_string(),
            capabilities: vec![
                "requirements-analysis".to_string(),
                "specification-writing".to_string(),
            ],
            required_skills: vec!["memory_search".to_string(), "memory_store".to_string()],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "coder".to_string(),
            role: AgentRole::Coder,
            version: "0.1.0".to_string(),
            description: "Generates secure, idiomatic Rust code from specifications.".to_string(),
            capabilities: vec![
                "code-generation".to_string(),
                "refactoring".to_string(),
                "bug-fix".to_string(),
            ],
            required_skills: vec![
                "memory_search".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
                "shell".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "tester".to_string(),
            role: AgentRole::Tester,
            version: "0.1.0".to_string(),
            description: "Writes and runs comprehensive test suites.".to_string(),
            capabilities: vec![
                "unit-testing".to_string(),
                "integration-testing".to_string(),
                "test-coverage".to_string(),
            ],
            required_skills: vec![
                "memory_search".to_string(),
                "file_read".to_string(),
                "shell".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "reviewer".to_string(),
            role: AgentRole::Reviewer,
            version: "0.1.0".to_string(),
            description: "Reviews code for quality, security, and compliance.".to_string(),
            capabilities: vec![
                "code-review".to_string(),
                "quality-audit".to_string(),
                "compliance-check".to_string(),
            ],
            required_skills: vec![
                "memory_search".to_string(),
                "human_approval".to_string(),
                "file_read".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "architect".to_string(),
            role: AgentRole::Architect,
            version: "0.1.0".to_string(),
            description: "Designs system architecture, APIs, and technical documents.".to_string(),
            capabilities: vec![
                "system-design".to_string(),
                "api-design".to_string(),
                "architecture-review".to_string(),
            ],
            required_skills: vec![
                "memory_search".to_string(),
                "memory_store".to_string(),
                "file_read".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "security-auditor".to_string(),
            role: AgentRole::SecurityAuditor,
            version: "0.1.0".to_string(),
            description:
                "Performs security reviews, vulnerability analysis, and compliance audits."
                    .to_string(),
            capabilities: vec![
                "vulnerability-analysis".to_string(),
                "security-review".to_string(),
                "compliance-audit".to_string(),
                "threat-modeling".to_string(),
            ],
            required_skills: vec!["memory_search".to_string(), "file_read".to_string()],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "devops".to_string(),
            role: AgentRole::DevOps,
            version: "0.1.0".to_string(),
            description: "Handles deployment, infrastructure, CI/CD pipelines, and operations."
                .to_string(),
            capabilities: vec![
                "deployment".to_string(),
                "ci-cd".to_string(),
                "infrastructure".to_string(),
                "monitoring".to_string(),
            ],
            required_skills: vec![
                "memory_search".to_string(),
                "memory_store".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
                "shell".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: Uuid::new_v4(),
            name: "document-writer".to_string(),
            role: AgentRole::DocumentWriter,
            version: "0.1.0".to_string(),
            description:
                "Writes and maintains technical documentation, guides, and API references."
                    .to_string(),
            capabilities: vec![
                "documentation".to_string(),
                "api-reference".to_string(),
                "changelog".to_string(),
            ],
            required_skills: vec![
                "memory_search".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
            ],
            model_preference: None,
            tags: HashMap::from([("tier".to_string(), "worker".to_string())]),
            created_at: now,
            updated_at: now,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Helper: build a minimal definition with the given name and role.
    fn make_def(name: &str, role: AgentRole) -> AgentDefinition {
        let now = Utc::now();
        AgentDefinition {
            id: Uuid::new_v4(),
            name: name.to_string(),
            role,
            version: "0.1.0".to_string(),
            description: format!("Test agent: {name}"),
            capabilities: vec!["cap-a".to_string()],
            required_skills: vec!["skill-x".to_string()],
            model_preference: None,
            tags: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    fn make_tagged_def(
        name: &str,
        role: AgentRole,
        tags: HashMap<String, String>,
    ) -> AgentDefinition {
        let mut def = make_def(name, role);
        def.tags = tags;
        def
    }

    // 1. Register and get by id
    #[test]
    fn test_register_and_get_by_id() {
        let reg = AgentRegistry::new();
        let def = make_def("alpha", AgentRole::Coder);
        let id = def.id;
        reg.register(def).unwrap();

        let fetched = reg.get(id).unwrap();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.name, "alpha");
    }

    // 2. Register and get by name
    #[test]
    fn test_register_and_get_by_name() {
        let reg = AgentRegistry::new();
        let def = make_def("beta", AgentRole::Tester);
        let id = def.id;
        reg.register(def).unwrap();

        let fetched = reg.get_by_name("beta").unwrap();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.role, AgentRole::Tester);
    }

    // 3. Duplicate name rejected
    #[test]
    fn test_duplicate_name_rejected() {
        let reg = AgentRegistry::new();
        reg.register(make_def("gamma", AgentRole::Coder)).unwrap();

        let dup = make_def("gamma", AgentRole::Tester);
        let err = reg.register(dup).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("gamma"),
            "error should mention the name: {msg}"
        );
    }

    // 4. Unregister returns definition
    #[test]
    fn test_unregister_returns_definition() {
        let reg = AgentRegistry::new();
        let def = make_def("delta", AgentRole::Reviewer);
        let id = def.id;
        reg.register(def).unwrap();

        let removed = reg.unregister(id).unwrap();
        assert_eq!(removed.name, "delta");
        assert!(reg.get(id).is_none());
        assert!(reg.get_by_name("delta").is_none());
    }

    // 5. Unregister unknown returns error
    #[test]
    fn test_unregister_unknown_returns_error() {
        let reg = AgentRegistry::new();
        let result = reg.unregister(Uuid::new_v4());
        assert!(result.is_err());
    }

    // 6. Update changes fields
    #[test]
    fn test_update_changes_fields() {
        let reg = AgentRegistry::new();
        let def = make_def("epsilon", AgentRole::Spec);
        let id = def.id;
        reg.register(def).unwrap();

        reg.update(
            id,
            AgentUpdate {
                description: Some("Updated description".to_string()),
                capabilities: Some(vec!["new-cap".to_string()]),
                model_preference: Some(Some("gpt-4".to_string())),
                ..Default::default()
            },
        )
        .unwrap();

        let fetched = reg.get(id).unwrap();
        assert_eq!(fetched.description, "Updated description");
        assert_eq!(fetched.capabilities, vec!["new-cap"]);
        assert_eq!(fetched.model_preference.as_deref(), Some("gpt-4"));
        // Name should remain unchanged.
        assert_eq!(fetched.name, "epsilon");
    }

    // 7. Get by role returns matching
    #[test]
    fn test_get_by_role_returns_matching() {
        let reg = AgentRegistry::new();
        reg.register(make_def("c1", AgentRole::Coder)).unwrap();
        reg.register(make_def("c2", AgentRole::Coder)).unwrap();
        reg.register(make_def("t1", AgentRole::Tester)).unwrap();

        let coders = reg.get_by_role(&AgentRole::Coder);
        assert_eq!(coders.len(), 2);
        assert!(coders.iter().all(|d| d.role == AgentRole::Coder));

        let testers = reg.get_by_role(&AgentRole::Tester);
        assert_eq!(testers.len(), 1);
    }

    // 8. Get by tag filters correctly
    #[test]
    fn test_get_by_tag_filters_correctly() {
        let reg = AgentRegistry::new();
        reg.register(make_tagged_def(
            "tagged1",
            AgentRole::Coder,
            HashMap::from([("team".to_string(), "backend".to_string())]),
        ))
        .unwrap();
        reg.register(make_tagged_def(
            "tagged2",
            AgentRole::Tester,
            HashMap::from([("team".to_string(), "frontend".to_string())]),
        ))
        .unwrap();
        reg.register(make_tagged_def(
            "tagged3",
            AgentRole::Coder,
            HashMap::from([("team".to_string(), "backend".to_string())]),
        ))
        .unwrap();

        let backend = reg.get_by_tag("team", "backend");
        assert_eq!(backend.len(), 2);

        let frontend = reg.get_by_tag("team", "frontend");
        assert_eq!(frontend.len(), 1);

        let missing = reg.get_by_tag("team", "infra");
        assert!(missing.is_empty());
    }

    // 9. Search finds by name substring
    #[test]
    fn test_search_finds_by_name_substring() {
        let reg = AgentRegistry::new();
        reg.register(make_def("my-coder-agent", AgentRole::Coder))
            .unwrap();
        reg.register(make_def("my-tester", AgentRole::Tester))
            .unwrap();

        let results = reg.search("coder");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "my-coder-agent");
    }

    // 10. Search finds by description substring
    #[test]
    fn test_search_finds_by_description_substring() {
        let reg = AgentRegistry::new();
        let mut def = make_def("foo", AgentRole::Spec);
        def.description = "Handles security vulnerability scanning".to_string();
        reg.register(def).unwrap();
        reg.register(make_def("bar", AgentRole::Coder)).unwrap();

        let results = reg.search("vulnerability");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "foo");
    }

    // 11. List all returns everything
    #[test]
    fn test_list_all_returns_everything() {
        let reg = AgentRegistry::new();
        reg.register(make_def("a1", AgentRole::Coder)).unwrap();
        reg.register(make_def("a2", AgentRole::Tester)).unwrap();
        reg.register(make_def("a3", AgentRole::Reviewer)).unwrap();

        let all = reg.list_all();
        assert_eq!(all.len(), 3);
    }

    // 12. Count and count_by_role
    #[test]
    fn test_count_and_count_by_role() {
        let reg = AgentRegistry::new();
        reg.register(make_def("x1", AgentRole::Coder)).unwrap();
        reg.register(make_def("x2", AgentRole::Coder)).unwrap();
        reg.register(make_def("x3", AgentRole::Tester)).unwrap();

        assert_eq!(reg.count(), 3);

        let by_role = reg.count_by_role();
        assert_eq!(by_role.get(&AgentRole::Coder), Some(&2));
        assert_eq!(by_role.get(&AgentRole::Tester), Some(&1));
        assert_eq!(by_role.get(&AgentRole::Reviewer), None);
    }

    // 13. Export/import catalog roundtrip
    #[test]
    fn test_export_import_catalog_roundtrip() {
        let reg1 = AgentRegistry::new();
        reg1.register(make_def("exp1", AgentRole::Coder)).unwrap();
        reg1.register(make_def("exp2", AgentRole::Tester)).unwrap();

        let catalog = reg1.export_catalog();
        assert_eq!(catalog.agents.len(), 2);
        assert_eq!(catalog.version, "1.0");

        let reg2 = AgentRegistry::new();
        let imported = reg2.import_catalog(catalog).unwrap();
        assert_eq!(imported, 2);
        assert_eq!(reg2.count(), 2);
        assert!(reg2.get_by_name("exp1").is_some());
        assert!(reg2.get_by_name("exp2").is_some());
    }

    // 14. Import skips duplicates
    #[test]
    fn test_import_skips_duplicates() {
        let reg = AgentRegistry::new();
        reg.register(make_def("dup1", AgentRole::Coder)).unwrap();

        let catalog = AgentCatalog {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            agents: vec![
                make_def("dup1", AgentRole::Tester),   // name conflict
                make_def("dup2", AgentRole::Reviewer), // new
            ],
        };

        let imported = reg.import_catalog(catalog).unwrap();
        assert_eq!(imported, 1);
        assert_eq!(reg.count(), 2);
        // The original "dup1" should still be a Coder (not overwritten).
        let original = reg.get_by_name("dup1").unwrap();
        assert_eq!(original.role, AgentRole::Coder);
    }

    // 15. Default definitions creates 9 agents
    #[test]
    fn test_default_definitions_creates_9_agents() {
        let defs = default_agent_definitions();
        assert_eq!(defs.len(), 9);

        let roles: Vec<AgentRole> = defs.iter().map(|d| d.role.clone()).collect();
        assert!(roles.contains(&AgentRole::Orchestrator));
        assert!(roles.contains(&AgentRole::Spec));
        assert!(roles.contains(&AgentRole::Coder));
        assert!(roles.contains(&AgentRole::Tester));
        assert!(roles.contains(&AgentRole::Reviewer));
        assert!(roles.contains(&AgentRole::Architect));
        assert!(roles.contains(&AgentRole::SecurityAuditor));
        assert!(roles.contains(&AgentRole::DevOps));
        assert!(roles.contains(&AgentRole::DocumentWriter));

        // Every definition should have non-empty capabilities and skills.
        for def in &defs {
            assert!(
                !def.capabilities.is_empty(),
                "{} has no capabilities",
                def.name
            );
            assert!(
                !def.required_skills.is_empty(),
                "{} has no required_skills",
                def.name
            );
        }
    }

    // 16. Serialize/Deserialize AgentDefinition
    #[test]
    fn test_serialize_deserialize_agent_definition() {
        let def = make_def("serde-test", AgentRole::Architect);
        let json = serde_json::to_string(&def).unwrap();
        let parsed: AgentDefinition = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, def.id);
        assert_eq!(parsed.name, def.name);
        assert_eq!(parsed.role, def.role);
        assert_eq!(parsed.version, def.version);
        assert_eq!(parsed.description, def.description);
        assert_eq!(parsed.capabilities, def.capabilities);
        assert_eq!(parsed.required_skills, def.required_skills);
    }

    // 17. Search is case-insensitive
    #[test]
    fn test_search_case_insensitive() {
        let reg = AgentRegistry::new();
        reg.register(make_def("MySpecialAgent", AgentRole::Coder))
            .unwrap();

        let results = reg.search("myspecial");
        assert_eq!(results.len(), 1);

        let results_upper = reg.search("MYSPECIAL");
        assert_eq!(results_upper.len(), 1);
    }

    // 18. Update name with uniqueness validation
    #[test]
    fn test_update_name_rejects_duplicate() {
        let reg = AgentRegistry::new();
        reg.register(make_def("first", AgentRole::Coder)).unwrap();
        let def2 = make_def("second", AgentRole::Tester);
        let id2 = def2.id;
        reg.register(def2).unwrap();

        let result = reg.update(
            id2,
            AgentUpdate {
                name: Some("first".to_string()),
                ..Default::default()
            },
        );
        assert!(result.is_err());

        // Original name should still be intact.
        assert!(reg.get_by_name("second").is_some());
    }

    // 19. Default registry is empty
    #[test]
    fn test_default_registry_is_empty() {
        let reg = AgentRegistry::default();
        assert_eq!(reg.count(), 0);
        assert!(reg.list_all().is_empty());
    }

    // 20. Catalog serialization roundtrip
    #[test]
    fn test_catalog_serialization_roundtrip() {
        let defs = default_agent_definitions();
        let catalog = AgentCatalog {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            agents: defs,
        };

        let json = serde_json::to_string_pretty(&catalog).unwrap();
        let parsed: AgentCatalog = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, "1.0");
        assert_eq!(parsed.agents.len(), 9);
    }
}
