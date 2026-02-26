use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    FileRead { allowed_paths: Vec<String> },
    FileWrite { allowed_paths: Vec<String> },
    NetworkAccess { allowed_hosts: Vec<String> },
    ShellExec { allowed_commands: Vec<String> },
    EnvRead { allowed_vars: Vec<String> },
    DatabaseQuery,
    BrowserAccess { allowed_domains: Vec<String> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    capabilities: HashSet<Capability>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }

    pub fn revoke(&mut self, cap: &Capability) {
        self.capabilities.remove(cap);
    }

    pub fn has(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    pub fn check_file_read(&self, path: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::FileRead { allowed_paths } => {
                allowed_paths.iter().any(|p| path.starts_with(p))
            }
            _ => false,
        })
    }

    pub fn check_file_write(&self, path: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::FileWrite { allowed_paths } => {
                allowed_paths.iter().any(|p| path.starts_with(p))
            }
            _ => false,
        })
    }

    pub fn check_network(&self, host: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::NetworkAccess { allowed_hosts } => {
                allowed_hosts.iter().any(|h| h == "*" || host.ends_with(h))
            }
            _ => false,
        })
    }

    pub fn check_shell(&self, command: &str) -> bool {
        self.capabilities.iter().any(|c| match c {
            Capability::ShellExec { allowed_commands } => {
                allowed_commands.iter().any(|cmd| command.starts_with(cmd))
            }
            _ => false,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

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
