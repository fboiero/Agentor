use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A fine-grained permission token describing a specific capability an agent may hold.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Permission to read files under the given paths.
    FileRead {
        /// Allowed path prefixes for reading.
        allowed_paths: Vec<String>,
    },
    /// Permission to write files under the given paths.
    FileWrite {
        /// Allowed path prefixes for writing.
        allowed_paths: Vec<String>,
    },
    /// Permission to access network hosts.
    NetworkAccess {
        /// Allowed host patterns (use `"*"` for wildcard).
        allowed_hosts: Vec<String>,
    },
    /// Permission to execute shell commands.
    ShellExec {
        /// Allowed command prefixes.
        allowed_commands: Vec<String>,
    },
    /// Permission to read environment variables.
    EnvRead {
        /// Allowed variable names.
        allowed_vars: Vec<String>,
    },
    /// Permission to perform database queries.
    DatabaseQuery,
    /// Permission to access browser/web domains.
    BrowserAccess {
        /// Allowed domain patterns.
        allowed_domains: Vec<String>,
    },
}

/// A set of granted capabilities that can be queried for access-control decisions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    capabilities: HashSet<Capability>,
}

impl PermissionSet {
    /// Create an empty permission set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant a capability to this set.
    pub fn grant(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }

    /// Revoke a capability from this set.
    pub fn revoke(&mut self, cap: &Capability) {
        self.capabilities.remove(cap);
    }

    /// Check whether this set contains the exact capability.
    pub fn has(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    /// Return `true` if any `FileRead` capability allows the given path.
    pub fn check_file_read(&self, path: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::FileRead { allowed_paths } => {
                allowed_paths.iter().any(|p| path.starts_with(p))
            }
            _ => false,
        })
    }

    /// Return `true` if any `FileWrite` capability allows the given path.
    pub fn check_file_write(&self, path: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::FileWrite { allowed_paths } => {
                allowed_paths.iter().any(|p| path.starts_with(p))
            }
            _ => false,
        })
    }

    /// Return `true` if any `NetworkAccess` capability allows the given host.
    pub fn check_network(&self, host: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::NetworkAccess { allowed_hosts } => {
                allowed_hosts.iter().any(|h| h == "*" || host.ends_with(h))
            }
            _ => false,
        })
    }

    /// Return `true` if any `ShellExec` capability allows the given command.
    pub fn check_shell(&self, command: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::ShellExec { allowed_commands } => {
                allowed_commands.iter().any(|cmd| command.starts_with(cmd))
            }
            _ => false,
        })
    }

    /// Return `true` if no capabilities have been granted.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Iterate over all granted capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_set() {
        let mut perms = PermissionSet::new();
        assert!(perms.is_empty());

        let cap = Capability::FileRead {
            allowed_paths: vec!["/tmp".to_string()],
        };
        perms.grant(cap.clone());
        assert!(perms.has(&cap));
        assert!(perms.check_file_read("/tmp/file.txt"));
        assert!(!perms.check_file_read("/etc/passwd"));
    }

    #[test]
    fn test_network_capability() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["api.anthropic.com".to_string()],
        });
        assert!(perms.check_network("api.anthropic.com"));
        assert!(!perms.check_network("evil.com"));
    }

    #[test]
    fn test_wildcard_network() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["*".to_string()],
        });
        assert!(perms.check_network("any-host.com"));
    }
}
