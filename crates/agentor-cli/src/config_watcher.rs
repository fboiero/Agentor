//! Config hot-reload watcher.
//!
//! Watches an `agentor.toml` file for modifications and invokes a callback
//! with the freshly parsed [`ReloadableConfig`] after a debounce window.
#![allow(dead_code)]

use agentor_core::{AgentorError, AgentorResult};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;

/// Subset of the full Agentor configuration that supports hot-reload.
///
/// Every section is optional so that a partial config file (containing only
/// the sections the operator wants to tweak at runtime) is accepted.
/// Values are kept as opaque `serde_json::Value` to avoid coupling this
/// module to the full config types defined in `main.rs`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReloadableConfig {
    #[serde(default)]
    pub security: Option<serde_json::Value>,
    #[serde(default)]
    pub skills: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub mcp_servers: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub tool_groups: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub webhooks: Option<Vec<serde_json::Value>>,
}

/// Watches a config file on disk and calls back on every (debounced) change.
///
/// The watcher is kept alive as long as this struct is alive; dropping it
/// stops the background thread and releases the file-system watch.
pub struct ConfigWatcher {
    /// Stored to prevent the watcher from being dropped (which would stop
    /// watching the file).
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Start watching `config_path` for modifications.
    ///
    /// * `debounce_ms` -- minimum milliseconds between two successive reload
    ///   callbacks.  Use `500` as a sensible default.
    /// * `on_reload` -- called on a background thread each time the config
    ///   file is modified and successfully parsed.  Parse errors are logged
    ///   via `tracing::warn` and do **not** invoke the callback.
    pub fn start<F>(
        config_path: PathBuf,
        debounce_ms: u64,
        on_reload: F,
    ) -> AgentorResult<Self>
    where
        F: Fn(ReloadableConfig) + Send + Sync + 'static,
    {
        let (tx, rx) = std_mpsc::channel();

        let mut watcher = notify::recommended_watcher(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        let _ = tx.send(());
                    }
                }
            },
        )
        .map_err(|e| {
            AgentorError::Config(format!("Failed to create file watcher: {e}"))
        })?;

        watcher
            .watch(config_path.as_ref(), RecursiveMode::NonRecursive)
            .map_err(|e| {
                AgentorError::Config(format!("Failed to watch config file: {e}"))
            })?;

        let path = config_path.clone();
        std::thread::spawn(move || {
            let mut last_reload = std::time::Instant::now();
            let debounce = std::time::Duration::from_millis(debounce_ms);

            while rx.recv().is_ok() {
                // Drain any additional events that arrived during the debounce
                // window so we only reload once per burst of writes.
                while rx.try_recv().is_ok() {}

                let now = std::time::Instant::now();
                if now.duration_since(last_reload) < debounce {
                    std::thread::sleep(debounce - now.duration_since(last_reload));
                }

                last_reload = std::time::Instant::now();

                match parse_config(&path) {
                    Ok(config) => on_reload(config),
                    Err(e) => tracing::warn!(error = %e, "Failed to reload config"),
                }
            }

            tracing::debug!("Config watcher thread exiting");
        });

        tracing::info!(path = %config_path.display(), "Config hot-reload watcher started");

        Ok(Self { _watcher: watcher })
    }
}

/// Read and parse a TOML config file into a [`ReloadableConfig`].
pub fn parse_config(path: &Path) -> AgentorResult<ReloadableConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AgentorError::Config(format!(
            "Failed to read config '{}': {}",
            path.display(),
            e
        ))
    })?;
    let config: ReloadableConfig = toml::from_str(&content).map_err(|e| {
        AgentorError::Config(format!(
            "Failed to parse config '{}': {}",
            path.display(),
            e
        ))
    })?;
    Ok(config)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_valid_config_all_sections() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file_mut(),
            r#"
[security]
max_requests_per_second = 20.0

[[skills]]
name = "test"

[[tool_groups]]
name = "custom"
"#
        )
        .unwrap();

        let config = parse_config(tmp.path()).unwrap();
        assert!(config.security.is_some());
        assert!(config.skills.is_some());
        assert_eq!(config.skills.as_ref().unwrap().len(), 1);
        assert!(config.tool_groups.is_some());
        assert_eq!(config.tool_groups.as_ref().unwrap().len(), 1);
        // Not specified in the file -- must be None.
        assert!(config.mcp_servers.is_none());
        assert!(config.webhooks.is_none());
    }

    #[test]
    fn test_parse_empty_config_all_none() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        // Write an empty (but valid) TOML document.
        writeln!(tmp.as_file_mut()).unwrap();

        let config = parse_config(tmp.path()).unwrap();
        assert!(config.security.is_none());
        assert!(config.skills.is_none());
        assert!(config.mcp_servers.is_none());
        assert!(config.tool_groups.is_none());
        assert!(config.webhooks.is_none());
    }

    #[test]
    fn test_parse_invalid_toml_returns_error() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp.as_file_mut(), "{{{{invalid toml!!!!").unwrap();
        let result = parse_config(tmp.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("Failed to parse config"),
            "unexpected error: {err_msg}"
        );
    }

    #[test]
    fn test_parse_nonexistent_file_returns_error() {
        let result = parse_config(Path::new("/nonexistent/path/config.toml"));
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("Failed to read config"),
            "unexpected error: {err_msg}"
        );
    }
}
