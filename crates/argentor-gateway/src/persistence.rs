//! JSON file-based persistence for control plane state.
//!
//! Provides transparent save/load operations so that deployments, agent
//! definitions, and health states survive server restarts.  Each data type
//! is stored in its own JSON file inside a configurable directory.
//!
//! # Atomic writes
//!
//! [`PersistentStore::save`] writes to a temporary file first and then renames
//! it into place, guaranteeing that readers never see a partially-written file.

use std::path::PathBuf;

use argentor_core::{ArgentorError, ArgentorResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::control_plane::ControlPlaneState;

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// Snapshot of control plane state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPlaneSnapshot {
    /// When this snapshot was taken.
    pub saved_at: DateTime<Utc>,
    /// Version for forward compatibility.
    pub version: u32,
    /// Serialized deployments.
    pub deployments: Vec<serde_json::Value>,
    /// Serialized agent definitions.
    pub agent_definitions: Vec<serde_json::Value>,
    /// Serialized health states.
    pub health_states: Vec<serde_json::Value>,
    /// Recent events (last 100).
    pub events: Vec<serde_json::Value>,
}

/// Credential snapshot for persistence (values encrypted/redacted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSnapshot {
    /// When this snapshot was taken.
    pub saved_at: DateTime<Utc>,
    /// Version for forward compatibility.
    pub version: u32,
    /// Serialized credentials.
    pub credentials: Vec<serde_json::Value>,
}

/// Token pool snapshot for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPoolSnapshot {
    /// When this snapshot was taken.
    pub saved_at: DateTime<Utc>,
    /// Version for forward compatibility.
    pub version: u32,
    /// Serialized tokens.
    pub tokens: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// PersistentStore
// ---------------------------------------------------------------------------

/// Persistent state store backed by JSON files.
///
/// Provides save/load operations for control plane data.  Each data type gets
/// its own JSON file in a configurable directory.
pub struct PersistentStore {
    /// Directory where state files are stored.
    data_dir: PathBuf,
}

impl PersistentStore {
    /// Create a new persistent store at the given directory.
    ///
    /// Creates the directory (and any missing parents) if it does not exist.
    pub fn new(data_dir: impl Into<PathBuf>) -> ArgentorResult<Self> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(&data_dir)?;
        Ok(Self { data_dir })
    }

    /// Save `data` as pretty-printed JSON to `{data_dir}/{name}.json`.
    ///
    /// The write is atomic: data is first written to a `.tmp` file and then
    /// renamed into place.  On Unix the file permissions are set to `0o600`.
    pub fn save<T: Serialize>(&self, name: &str, data: &T) -> ArgentorResult<()> {
        let sanitized = sanitize_name(name);
        let target = self.snapshot_path(&sanitized);
        let tmp = self.data_dir.join(format!("{sanitized}.tmp"));

        let json = serde_json::to_string_pretty(data)?;
        std::fs::write(&tmp, &json)?;

        // Restrict permissions on Unix before the rename so the final file is
        // never world-readable, even briefly.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&tmp, perms)?;
        }

        std::fs::rename(&tmp, &target)?;

        info!(path = %target.display(), "persisted snapshot");
        Ok(())
    }

    /// Load a snapshot from `{data_dir}/{name}.json`.
    ///
    /// Returns `Ok(None)` if the file does not exist.  Returns `Err` if the
    /// file exists but cannot be parsed.
    pub fn load<T: for<'de> Deserialize<'de>>(&self, name: &str) -> ArgentorResult<Option<T>> {
        let sanitized = sanitize_name(name);
        let path = self.snapshot_path(&sanitized);

        if !path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&path).map_err(|e| {
            ArgentorError::Gateway(format!("failed to read snapshot {}: {e}", path.display()))
        })?;

        let data: T = serde_json::from_str(&contents).map_err(|e| {
            ArgentorError::Gateway(format!(
                "corrupted snapshot file {}: {e}",
                path.display()
            ))
        })?;

        Ok(Some(data))
    }

    /// Delete the snapshot file `{data_dir}/{name}.json`.
    ///
    /// Returns `Ok(())` even if the file did not exist (idempotent).
    pub fn delete(&self, name: &str) -> ArgentorResult<()> {
        let sanitized = sanitize_name(name);
        let path = self.snapshot_path(&sanitized);

        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List all `.json` snapshot files in the data directory (without the
    /// `.json` extension).
    pub fn list_snapshots(&self) -> ArgentorResult<Vec<String>> {
        let mut names = Vec::new();

        for entry in std::fs::read_dir(&self.data_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }

        names.sort();
        Ok(names)
    }

    /// Return the full path for the named snapshot file.
    fn snapshot_path(&self, name: &str) -> PathBuf {
        self.data_dir.join(format!("{name}.json"))
    }
}

// ---------------------------------------------------------------------------
// Name sanitization
// ---------------------------------------------------------------------------

/// Replace characters that are unsafe in filenames with underscores.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ControlPlaneState integration helpers
// ---------------------------------------------------------------------------

/// The snapshot file name used for control plane state.
const CONTROL_PLANE_SNAPSHOT_NAME: &str = "control_plane";

/// Maximum number of events persisted in each snapshot.
const MAX_PERSISTED_EVENTS: usize = 100;

/// Save the current control plane state to disk.
///
/// Reads the state's internal `RwLock`s, serializes each collection as
/// `serde_json::Value`, and writes a [`ControlPlaneSnapshot`].
pub async fn save_control_plane_state(
    store: &PersistentStore,
    state: &ControlPlaneState,
) -> ArgentorResult<()> {
    // Acquire read locks (non-blocking in practice since handlers hold them
    // only briefly).
    let deployments = state.deployments.read().await;
    let agent_defs = state.agent_definitions.read().await;
    let healths = state.health_states.read().await;
    let events = state.events.read().await;

    let deployment_values: Vec<serde_json::Value> = deployments
        .values()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;

    let agent_def_values: Vec<serde_json::Value> = agent_defs
        .values()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;

    let health_values: Vec<serde_json::Value> = healths
        .values()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;

    // Only persist the last MAX_PERSISTED_EVENTS events.
    let event_start = events.len().saturating_sub(MAX_PERSISTED_EVENTS);
    let event_values: Vec<serde_json::Value> = events[event_start..]
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;

    // Drop locks before the (blocking) disk write.
    drop(deployments);
    drop(agent_defs);
    drop(healths);
    drop(events);

    let snapshot = ControlPlaneSnapshot {
        saved_at: Utc::now(),
        version: 1,
        deployments: deployment_values,
        agent_definitions: agent_def_values,
        health_states: health_values,
        events: event_values,
    };

    store.save(CONTROL_PLANE_SNAPSHOT_NAME, &snapshot)?;
    Ok(())
}

/// Load control plane state from disk and populate the in-memory state.
///
/// Returns `true` if a snapshot was found and successfully loaded, `false` if
/// no snapshot file existed.
pub async fn load_control_plane_state(
    store: &PersistentStore,
    state: &ControlPlaneState,
) -> ArgentorResult<bool> {
    let snapshot: Option<ControlPlaneSnapshot> = store.load(CONTROL_PLANE_SNAPSHOT_NAME)?;

    let snapshot = match snapshot {
        Some(s) => s,
        None => return Ok(false),
    };

    // --- Deployments ---
    {
        let mut deployments = state.deployments.write().await;
        for val in &snapshot.deployments {
            let dep = serde_json::from_value(val.clone()).map_err(|e| {
                ArgentorError::Gateway(format!("failed to deserialize deployment: {e}"))
            })?;
            let dep: crate::control_plane::DeploymentInfo = dep;
            deployments.insert(dep.id, dep);
        }
    }

    // --- Agent definitions ---
    {
        let mut agent_defs = state.agent_definitions.write().await;
        for val in &snapshot.agent_definitions {
            let def: crate::control_plane::AgentDefinitionInfo =
                serde_json::from_value(val.clone()).map_err(|e| {
                    ArgentorError::Gateway(format!("failed to deserialize agent definition: {e}"))
                })?;
            agent_defs.insert(def.id, def);
        }
    }

    // --- Health states ---
    {
        let mut healths = state.health_states.write().await;
        for val in &snapshot.health_states {
            let health: crate::control_plane::AgentHealthInfo =
                serde_json::from_value(val.clone()).map_err(|e| {
                    ArgentorError::Gateway(format!("failed to deserialize health state: {e}"))
                })?;
            healths.insert(health.agent_id, health);
        }
    }

    // --- Events ---
    {
        let mut events = state.events.write().await;
        for val in &snapshot.events {
            let event: crate::control_plane::ControlPlaneEvent =
                serde_json::from_value(val.clone()).map_err(|e| {
                    ArgentorError::Gateway(format!("failed to deserialize event: {e}"))
                })?;
            events.push(event);
        }
    }

    info!(
        deployments = snapshot.deployments.len(),
        agent_definitions = snapshot.agent_definitions.len(),
        health_states = snapshot.health_states.len(),
        events = snapshot.events.len(),
        "restored control plane state from snapshot"
    );

    Ok(true)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Create a unique temporary directory for a test.
    fn test_dir(label: &str) -> PathBuf {
        let id = uuid::Uuid::new_v4();
        let dir = std::env::temp_dir().join(format!("argentor_test_{label}_{id}"));
        // Ensure clean slate.
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn create_store_in_temp_directory() {
        let dir = test_dir("create_store");
        let store = PersistentStore::new(&dir);
        assert!(store.is_ok());
        assert!(dir.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_control_plane_snapshot() {
        let dir = test_dir("save_load_snapshot");
        let store = PersistentStore::new(&dir).unwrap();

        let snapshot = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![serde_json::json!({"id": "abc"})],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };

        store.save("test_snap", &snapshot).unwrap();
        let loaded: Option<ControlPlaneSnapshot> = store.load("test_snap").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.deployments.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let dir = test_dir("load_nonexistent");
        let store = PersistentStore::new(&dir).unwrap();
        let loaded: Option<ControlPlaneSnapshot> = store.load("does_not_exist").unwrap();
        assert!(loaded.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_overwrites_existing() {
        let dir = test_dir("overwrite");
        let store = PersistentStore::new(&dir).unwrap();

        let snap1 = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![serde_json::json!({"name": "first"})],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("overwrite_test", &snap1).unwrap();

        let snap2 = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 2,
            deployments: vec![serde_json::json!({"name": "second"})],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("overwrite_test", &snap2).unwrap();

        let loaded: ControlPlaneSnapshot = store.load("overwrite_test").unwrap().unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.deployments[0]["name"], "second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_snapshot() {
        let dir = test_dir("delete");
        let store = PersistentStore::new(&dir).unwrap();

        let snap = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("to_delete", &snap).unwrap();
        assert!(store.snapshot_path("to_delete").exists());

        store.delete("to_delete").unwrap();
        assert!(!store.snapshot_path("to_delete").exists());

        // Deleting again is idempotent.
        store.delete("to_delete").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_snapshots() {
        let dir = test_dir("list");
        let store = PersistentStore::new(&dir).unwrap();

        let snap = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("alpha", &snap).unwrap();
        store.save("bravo", &snap).unwrap();
        store.save("charlie", &snap).unwrap();

        let names = store.list_snapshots().unwrap();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_file_exists_after_save() {
        let dir = test_dir("atomic");
        let store = PersistentStore::new(&dir).unwrap();

        let snap = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("atomic_test", &snap).unwrap();

        // The final .json file must exist; no leftover .tmp file.
        assert!(store.snapshot_path("atomic_test").exists());
        assert!(!dir.join("atomic_test.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_file_returns_error() {
        let dir = test_dir("corrupted");
        let store = PersistentStore::new(&dir).unwrap();

        // Write garbage to the expected path.
        let path = store.snapshot_path("bad_data");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"NOT VALID JSON {{{{").unwrap();

        let result: ArgentorResult<Option<ControlPlaneSnapshot>> = store.load("bad_data");
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_snapshot_roundtrip() {
        let dir = test_dir("empty_snap");
        let store = PersistentStore::new(&dir).unwrap();

        let snap = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("empty", &snap).unwrap();

        let loaded: ControlPlaneSnapshot = store.load("empty").unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert!(loaded.deployments.is_empty());
        assert!(loaded.agent_definitions.is_empty());
        assert!(loaded.health_states.is_empty());
        assert!(loaded.events.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_field_preserved() {
        let dir = test_dir("version_field");
        let store = PersistentStore::new(&dir).unwrap();

        let snap = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 42,
            deployments: vec![],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };
        store.save("versioned", &snap).unwrap();

        let loaded: ControlPlaneSnapshot = store.load("versioned").unwrap().unwrap();
        assert_eq!(loaded.version, 42);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multiple_snapshots_in_same_directory() {
        let dir = test_dir("multi_snap");
        let store = PersistentStore::new(&dir).unwrap();

        let snap_a = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![serde_json::json!({"id": "a"})],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };

        let cred = CredentialSnapshot {
            saved_at: Utc::now(),
            version: 1,
            credentials: vec![serde_json::json!({"key": "redacted"})],
        };

        store.save("control_plane", &snap_a).unwrap();
        store.save("credentials", &cred).unwrap();

        let loaded_snap: ControlPlaneSnapshot = store.load("control_plane").unwrap().unwrap();
        let loaded_cred: CredentialSnapshot = store.load("credentials").unwrap().unwrap();
        assert_eq!(loaded_snap.deployments.len(), 1);
        assert_eq!(loaded_cred.credentials.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_with_special_characters_in_name_sanitized() {
        let dir = test_dir("special_chars");
        let store = PersistentStore::new(&dir).unwrap();

        let snap = ControlPlaneSnapshot {
            saved_at: Utc::now(),
            version: 1,
            deployments: vec![],
            agent_definitions: vec![],
            health_states: vec![],
            events: vec![],
        };

        // Names with path separators, dots, and spaces should be sanitized.
        store.save("../../etc/passwd", &snap).unwrap();
        let names = store.list_snapshots().unwrap();
        // Should NOT traverse outside the data directory.
        assert!(!names.iter().any(|n| n.contains('/')));
        // The sanitized file should exist inside data_dir.
        assert!(!names.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn credential_snapshot_roundtrip() {
        let dir = test_dir("cred_roundtrip");
        let store = PersistentStore::new(&dir).unwrap();

        let cred = CredentialSnapshot {
            saved_at: Utc::now(),
            version: 3,
            credentials: vec![
                serde_json::json!({"service": "openai", "api_key": "***"}),
                serde_json::json!({"service": "anthropic", "api_key": "***"}),
            ],
        };

        store.save("credentials", &cred).unwrap();
        let loaded: CredentialSnapshot = store.load("credentials").unwrap().unwrap();
        assert_eq!(loaded.version, 3);
        assert_eq!(loaded.credentials.len(), 2);
        assert_eq!(loaded.credentials[0]["service"], "openai");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn token_pool_snapshot_roundtrip() {
        let dir = test_dir("token_roundtrip");
        let store = PersistentStore::new(&dir).unwrap();

        let pool = TokenPoolSnapshot {
            saved_at: Utc::now(),
            version: 1,
            tokens: vec![
                serde_json::json!({"provider": "openai", "remaining": 50000}),
            ],
        };

        store.save("tokens", &pool).unwrap();
        let loaded: TokenPoolSnapshot = store.load("tokens").unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.tokens.len(), 1);
        assert_eq!(loaded.tokens[0]["provider"], "openai");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_path_construction() {
        let dir = test_dir("path_construction");
        let store = PersistentStore::new(&dir).unwrap();

        let path = store.snapshot_path("my_state");
        assert_eq!(path, dir.join("my_state.json"));

        let path2 = store.snapshot_path("control_plane");
        assert_eq!(path2, dir.join("control_plane.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn save_and_load_control_plane_state_integration() {
        let dir = test_dir("cp_integration");
        let store = PersistentStore::new(&dir).unwrap();
        let state = ControlPlaneState::new();

        // Populate state with one deployment and one agent definition.
        {
            let mut deployments = state.deployments.write().await;
            let id = uuid::Uuid::new_v4();
            deployments.insert(
                id,
                crate::control_plane::DeploymentInfo {
                    id,
                    name: "test-deploy".into(),
                    role: "coder".into(),
                    replicas: 2,
                    status: "running".into(),
                    auto_restart: true,
                    instances: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    total_tasks: 10,
                    total_errors: 0,
                    tags: std::collections::HashMap::new(),
                },
            );
        }
        {
            let mut defs = state.agent_definitions.write().await;
            let id = uuid::Uuid::new_v4();
            defs.insert(
                id,
                crate::control_plane::AgentDefinitionInfo {
                    id,
                    name: "test-agent".into(),
                    role: "tester".into(),
                    version: "0.1.0".into(),
                    description: "A test agent".into(),
                    capabilities: vec!["run_tests".into()],
                    tags: std::collections::HashMap::new(),
                    created_at: Utc::now(),
                },
            );
        }

        // Save.
        save_control_plane_state(&store, &state).await.unwrap();

        // Create a fresh state and load into it.
        let state2 = ControlPlaneState::new();
        let loaded = load_control_plane_state(&store, &state2).await.unwrap();
        assert!(loaded);

        let deployments = state2.deployments.read().await;
        assert_eq!(deployments.len(), 1);
        let dep = deployments.values().next().unwrap();
        assert_eq!(dep.name, "test-deploy");

        let defs = state2.agent_definitions.read().await;
        assert_eq!(defs.len(), 1);
        let def = defs.values().next().unwrap();
        assert_eq!(def.name, "test-agent");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn load_control_plane_state_no_file_returns_false() {
        let dir = test_dir("cp_no_file");
        let store = PersistentStore::new(&dir).unwrap();
        let state = ControlPlaneState::new();

        let loaded = load_control_plane_state(&store, &state).await.unwrap();
        assert!(!loaded);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
