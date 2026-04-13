use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

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
        /// Allowed host patterns (use `"*"` for wildcard, prefix with `.` for suffix matching).
        allowed_hosts: Vec<String>,
    },
    /// Permission to execute shell commands.
    ShellExec {
        /// Allowed base command names (e.g., `"ls"`, `"echo"`).
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

impl Capability {
    /// Return a type discriminator string for this capability variant.
    ///
    /// Values: `"file_read"`, `"file_write"`, `"network_access"`, `"shell_exec"`,
    /// `"env_read"`, `"database_query"`, `"browser_access"`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::FileRead { .. } => "file_read",
            Self::FileWrite { .. } => "file_write",
            Self::NetworkAccess { .. } => "network_access",
            Self::ShellExec { .. } => "shell_exec",
            Self::EnvRead { .. } => "env_read",
            Self::DatabaseQuery => "database_query",
            Self::BrowserAccess { .. } => "browser_access",
        }
    }
}

/// Result of a strict shell command check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCheckResult {
    /// The entire command is allowed.
    Allowed,
    /// The command was denied.
    Denied {
        /// Human-readable reason for denial.
        reason: String,
    },
    /// The command contains segments that need human review.
    NeedsReview {
        /// The individual command segments that could not be validated.
        segments: Vec<String>,
    },
}

/// A set of granted capabilities that can be queried for access-control decisions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    capabilities: HashSet<Capability>,
}

// ---------------------------------------------------------------------------
// Shell metacharacter splitting
// ---------------------------------------------------------------------------

/// Shell metacharacters that delimit separate commands.
/// Order matters: longer patterns must come before shorter overlapping ones
/// (e.g., `||` before `|`, `&&` before implicit single `&`).
const SHELL_METACHAR_PATTERNS: &[&str] = &["||", "&&", "|", ";", "$(", "`", "\n"];

/// Split a command string on shell metacharacters, returning the individual
/// command segments.  Each segment is trimmed but otherwise unmodified.
fn split_shell_segments(command: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut remaining = command.to_string();

    loop {
        // Find the earliest metacharacter in `remaining`.
        let mut earliest: Option<(usize, usize)> = None; // (position, pattern_len)
        for pat in SHELL_METACHAR_PATTERNS {
            if let Some(pos) = remaining.find(pat) {
                match earliest {
                    None => earliest = Some((pos, pat.len())),
                    Some((prev_pos, _)) if pos < prev_pos => {
                        earliest = Some((pos, pat.len()));
                    }
                    _ => {}
                }
            }
        }

        match earliest {
            Some((pos, len)) => {
                let before = remaining[..pos].trim().to_string();
                if !before.is_empty() {
                    segments.push(before);
                }
                remaining = remaining[pos + len..].to_string();
            }
            None => {
                let trimmed = remaining.trim().to_string();
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                break;
            }
        }
    }

    segments
}

/// Extract the base command (first whitespace-delimited token) from a command
/// segment, stripping any leading path components (e.g. `/usr/bin/ls` -> `ls`).
fn extract_base_command(segment: &str) -> &str {
    let token = segment.split_whitespace().next().unwrap_or("");
    // Strip path prefix
    token.rsplit('/').next().unwrap_or(token)
}

// ---------------------------------------------------------------------------
// IP address classification
// ---------------------------------------------------------------------------

/// Return `true` if the given IP address is private or reserved (loopback,
/// link-local, RFC 1918, etc.).
///
/// Checked ranges:
/// - IPv4: `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`,
///   `169.254.0.0/16`, `0.0.0.0/8`
/// - IPv6: `::1`, `fe80::/10`
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 127.0.0.0/8 — loopback
            octets[0] == 127
            // 10.0.0.0/8 — private
            || octets[0] == 10
            // 172.16.0.0/12 — private
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            // 192.168.0.0/16 — private
            || (octets[0] == 192 && octets[1] == 168)
            // 169.254.0.0/16 — link-local
            || (octets[0] == 169 && octets[1] == 254)
            // 0.0.0.0/8 — "this" network
            || octets[0] == 0
        }
        IpAddr::V6(v6) => {
            // ::1 — loopback
            if v6.is_loopback() {
                return true;
            }
            let segments = v6.segments();
            // fe80::/10 — link-local (first 10 bits = 1111 1110 10)
            (segments[0] & 0xffc0) == 0xfe80
        }
    }
}

// ---------------------------------------------------------------------------
// Path canonicalization helpers
// ---------------------------------------------------------------------------

/// Return `true` if the raw path string contains a NUL byte (`\0`).
///
/// Paths with NUL bytes are rejected outright — they can cause C-level string
/// truncation which may bypass higher-level path checks (CWE-158).
fn contains_nul_byte(path: &str) -> bool {
    path.bytes().any(|b| b == 0)
}

/// Return `true` if the raw bytes contain an overlong UTF-8 encoding of `.`
/// or `/`.  These are invalid UTF-8 sequences that some legacy decoders
/// interpret as ordinary characters, enabling path traversal.
///
/// Common patterns:
/// - `\xC0\xAE` — overlong encoding of `.` (U+002E)
/// - `\xE0\x80\xAE` — 3-byte overlong encoding of `.`
/// - `\xC0\xAF` — overlong encoding of `/` (U+002F)
fn contains_overlong_utf8(raw: &[u8]) -> bool {
    for i in 0..raw.len() {
        if i + 1 < raw.len() {
            // 2-byte overlong for ASCII range: C0 xx or C1 xx
            if (raw[i] == 0xC0 || raw[i] == 0xC1) && (raw[i + 1] & 0xC0) == 0x80 {
                return true;
            }
        }
        if i + 2 < raw.len() {
            // 3-byte overlong for BMP range: E0 80..9F xx
            if raw[i] == 0xE0 && raw[i + 1] < 0xA0 && (raw[i + 2] & 0xC0) == 0x80 {
                return true;
            }
        }
        if i + 3 < raw.len() {
            // 4-byte overlong: F0 80..8F xx xx
            if raw[i] == 0xF0 && raw[i + 1] < 0x90 && (raw[i + 2] & 0xC0) == 0x80 {
                return true;
            }
        }
    }
    false
}

/// Sanitize a path string through multiple defence layers before
/// constructing a `Path`:
///
/// 1. Reject NUL bytes (CWE-158)
/// 2. Percent-decode (CWE-22 — URL-encoded traversal)
/// 3. Reject overlong UTF-8 (CWE-176)
/// 4. Apply Unicode NFKC normalization (CWE-176 — compatibility decomposition)
///
/// Returns `None` if the path is rejected, or `Some(sanitized_string)` if safe.
fn sanitize_path_string(path: &str) -> Option<String> {
    // Layer 1: reject NUL bytes
    if contains_nul_byte(path) {
        return None;
    }

    // Layer 2: percent-decode (handles %2e%2e%2f → ../ etc.)
    let decoded = percent_decode_str(path).collect::<Vec<u8>>();

    // Layer 3: reject overlong UTF-8 sequences in the decoded bytes
    if contains_overlong_utf8(&decoded) {
        return None;
    }

    // Convert decoded bytes to UTF-8 (lossy — invalid sequences become U+FFFD)
    let decoded_str = String::from_utf8_lossy(&decoded);

    // Reject if the decoded string still contains NUL
    if decoded_str.bytes().any(|b| b == 0) {
        return None;
    }

    // Layer 4: Unicode NFKC normalization (decomposes fullwidth, ligatures, etc.)
    let normalized: String = decoded_str.nfkc().collect();

    Some(normalized)
}

/// Normalize a path by resolving `.` and `..` components logically (without
/// touching the filesystem).  The result is an absolute-looking path with no
/// `.` or `..` segments.
///
/// Before component analysis, the path string is sanitized through
/// [`sanitize_path_string`] which handles percent-decoding, overlong UTF-8
/// rejection, and NFKC normalization.
fn normalize_path(path: &Path) -> Option<std::path::PathBuf> {
    use std::path::Component;

    // Apply sanitization pipeline to the string representation
    let path_str = path.to_string_lossy();
    let sanitized = sanitize_path_string(&path_str)?;
    let sanitized_path = Path::new(&sanitized);

    let mut components: Vec<std::ffi::OsString> = Vec::new();
    for component in sanitized_path.components() {
        match component {
            Component::CurDir => { /* skip "." */ }
            Component::ParentDir => {
                // Pop the last normal component (if any).  Never pop past the
                // root — that would silently allow escaping `/`.
                if components
                    .last()
                    .is_some_and(|c| Path::new(c).file_name().is_some())
                {
                    components.pop();
                }
            }
            _ => {
                components.push(component.as_os_str().to_os_string());
            }
        }
    }
    if components.is_empty() {
        return Some(std::path::PathBuf::from("/"));
    }
    let mut result = std::path::PathBuf::new();
    for c in components {
        result.push(c);
    }
    Some(result)
}

/// Best-effort path canonicalization.
///
/// First resolves `.` and `..` components logically, then attempts filesystem
/// canonicalization (which also resolves symlinks).  If the path does not exist,
/// we canonicalize the nearest existing ancestor and append the remaining
/// non-existent tail.  This prevents `..` from escaping an allowed directory
/// even when the target file does not yet exist.
fn canonicalize_best_effort(path: &Path) -> Option<std::path::PathBuf> {
    // Step 1: logical normalization (resolve . and .., sanitize encoding)
    let normalized = normalize_path(path)?;

    // Step 2: try full filesystem canonicalization (also resolves symlinks)
    if let Ok(canon) = normalized.canonicalize() {
        return Some(canon);
    }

    // Step 3: walk up to the nearest existing ancestor
    let mut tail_components: Vec<std::ffi::OsString> = Vec::new();
    let mut current = normalized.clone();

    while let Some(parent) = current.parent() {
        {
            if let Some(file_name) = current.file_name() {
                tail_components.push(file_name.to_os_string());
            }
            if parent.exists() {
                if let Ok(canonical_parent) = parent.canonicalize() {
                    let mut result = canonical_parent;
                    for component in tail_components.iter().rev() {
                        result.push(component);
                    }
                    return Some(result);
                }
            }
            current = parent.to_path_buf();
        }
    }

    Some(normalized)
}

/// Check whether `candidate` (canonicalized) lives under `allowed_prefix`
/// (also canonicalized).
///
/// Uses `std::path::Path::starts_with` which compares *full path components*,
/// so `/tmp-evil/file` does **not** match allowed prefix `/tmp`.
fn path_is_under(candidate: &Path, allowed_prefix: &Path) -> bool {
    candidate.starts_with(allowed_prefix)
}

// ---------------------------------------------------------------------------
// PermissionSet implementation
// ---------------------------------------------------------------------------

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

    /// Check whether any capability of the given type name exists, regardless
    /// of its parameters.
    ///
    /// Valid type names: `"file_read"`, `"file_write"`, `"network_access"`,
    /// `"shell_exec"`, `"env_read"`, `"database_query"`, `"browser_access"`.
    pub fn has_capability_type(&self, cap_type: &str) -> bool {
        self.capabilities.iter().any(|c| c.type_name() == cap_type)
    }

    // ----- File checks (path-safe) -----------------------------------------

    /// Return `true` if any `FileRead` capability allows the given `Path`.
    ///
    /// The path is canonicalized to prevent traversal attacks (`..`, symlinks).
    pub fn check_file_read_path(&self, path: &Path) -> bool {
        let Some(canonical) = canonicalize_best_effort(path) else {
            return false; // rejected by sanitization (NUL, overlong, etc.)
        };
        self.capabilities.iter().any(|c| match c {
            Capability::FileRead { allowed_paths } => allowed_paths.iter().any(|p| {
                let Some(allowed_canon) = canonicalize_best_effort(Path::new(p)) else {
                    return false;
                };
                path_is_under(&canonical, &allowed_canon)
            }),
            _ => false,
        })
    }

    /// Return `true` if any `FileRead` capability allows the given path string.
    ///
    /// Backwards-compatible wrapper around [`Self::check_file_read_path`].
    /// The path is canonicalized internally to prevent traversal attacks.
    pub fn check_file_read(&self, path: &str) -> bool {
        self.check_file_read_path(Path::new(path))
    }

    /// Return `true` if any `FileWrite` capability allows the given `Path`.
    ///
    /// The path is canonicalized to prevent traversal attacks.
    pub fn check_file_write_path(&self, path: &Path) -> bool {
        let Some(canonical) = canonicalize_best_effort(path) else {
            return false;
        };
        self.capabilities.iter().any(|c| match c {
            Capability::FileWrite { allowed_paths } => allowed_paths.iter().any(|p| {
                let Some(allowed_canon) = canonicalize_best_effort(Path::new(p)) else {
                    return false;
                };
                path_is_under(&canonical, &allowed_canon)
            }),
            _ => false,
        })
    }

    /// Return `true` if any `FileWrite` capability allows the given path string.
    ///
    /// Backwards-compatible wrapper around [`Self::check_file_write_path`].
    pub fn check_file_write(&self, path: &str) -> bool {
        self.check_file_write_path(Path::new(path))
    }

    // ----- Network checks --------------------------------------------------

    /// Return `true` if any `NetworkAccess` capability allows the given host.
    ///
    /// Matching rules:
    /// - `"*"` allows any host.
    /// - An exact string match (case-insensitive) is accepted.
    /// - A pattern starting with `"."` (e.g. `".anthropic.com"`) is treated as
    ///   a domain-suffix match: the host must end with that exact suffix
    ///   preceded by a dot boundary, so `evil-anthropic.com` does **not** match
    ///   `.anthropic.com`, but `sub.anthropic.com` does.
    pub fn check_network(&self, host: &str) -> bool {
        let host_lower = host.to_lowercase();
        self.capabilities.iter().any(|c| match c {
            Capability::NetworkAccess { allowed_hosts } => allowed_hosts.iter().any(|h| {
                if h == "*" {
                    return true;
                }
                let pattern_lower = h.to_lowercase();
                // Exact match
                if host_lower == pattern_lower {
                    return true;
                }
                // Suffix match: pattern must start with '.'
                if let Some(suffix) = pattern_lower.strip_prefix('.') {
                    // host must end with ".<suffix>" — the host must have the
                    // pattern as a proper domain suffix after a dot boundary.
                    return host_lower.ends_with(&format!(".{suffix}"));
                }
                false
            }),
            _ => false,
        })
    }

    /// Return `true` if any `NetworkAccess` capability would allow access to
    /// the given IP address.  Private / reserved IPs are **always denied**,
    /// even if a wildcard `"*"` pattern is present.
    pub fn check_network_ip(&self, ip: &IpAddr) -> bool {
        if is_private_ip(ip) {
            return false;
        }
        // For public IPs, check the string representation against the normal
        // host rules so that numeric host patterns like "8.8.8.8" work.
        self.check_network(&ip.to_string())
    }

    // ----- Shell checks ----------------------------------------------------

    /// Return `true` if every command segment in `command` is allowed.
    ///
    /// The command is split on shell metacharacters (`|`, `&&`, `||`, `;`,
    /// `` ` ``, `$(`, newline).  Each segment's **base command** (the first
    /// token, with any path prefix stripped) is compared against the allowed
    /// commands list.  If *any* segment is not explicitly allowed, the entire
    /// command is denied.
    pub fn check_shell(&self, command: &str) -> bool {
        matches!(self.check_shell_strict(command), ShellCheckResult::Allowed)
    }

    /// Perform a detailed shell command check, returning a [`ShellCheckResult`].
    pub fn check_shell_strict(&self, command: &str) -> ShellCheckResult {
        let segments = split_shell_segments(command);

        if segments.is_empty() {
            return ShellCheckResult::Denied {
                reason: "empty command".to_string(),
            };
        }

        // Collect the set of all allowed base commands across all ShellExec
        // capabilities.
        let allowed: HashSet<&str> = self
            .capabilities
            .iter()
            .flat_map(|c| match c {
                Capability::ShellExec { allowed_commands } => allowed_commands
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect();

        if allowed.is_empty() {
            return ShellCheckResult::Denied {
                reason: "no shell_exec capability granted".to_string(),
            };
        }

        let mut denied_segments: Vec<String> = Vec::new();

        for segment in &segments {
            let base = extract_base_command(segment);
            if !allowed.contains(base) {
                denied_segments.push(segment.clone());
            }
        }

        if denied_segments.is_empty() {
            ShellCheckResult::Allowed
        } else if segments.len() > 1 {
            // Multiple segments with at least one disallowed — the pipeline /
            // chain as a whole is denied.
            ShellCheckResult::Denied {
                reason: format!(
                    "command chain contains disallowed segments: {}",
                    denied_segments.join(", ")
                ),
            }
        } else {
            ShellCheckResult::Denied {
                reason: format!(
                    "command '{}' is not in the allowed list",
                    extract_base_command(&denied_segments[0])
                ),
            }
        }
    }

    // ----- Misc ------------------------------------------------------------

    /// Return `true` if no capabilities have been granted.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Iterate over all granted capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // -----------------------------------------------------------------------
    // Original tests (preserved for backwards compat)
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Path traversal tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_path_traversal_simple() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileRead {
            allowed_paths: vec!["/tmp".to_string()],
        });

        // /tmp/../etc/shadow should NOT pass — it escapes /tmp
        assert!(
            !perms.check_file_read("/tmp/../etc/shadow"),
            "path traversal /tmp/../etc/shadow must be denied"
        );
    }

    #[test]
    fn test_path_traversal_deep() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileRead {
            allowed_paths: vec!["/tmp".to_string()],
        });

        assert!(
            !perms.check_file_read("/tmp/safe/../../etc/shadow"),
            "deep path traversal must be denied"
        );
    }

    #[test]
    fn test_path_normal_allowed() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileRead {
            allowed_paths: vec!["/tmp".to_string()],
        });

        assert!(
            perms.check_file_read("/tmp/myfile.txt"),
            "normal path under /tmp must be allowed"
        );
    }

    #[test]
    fn test_path_write_traversal() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileWrite {
            allowed_paths: vec!["/tmp".to_string()],
        });

        assert!(!perms.check_file_write("/tmp/../etc/shadow"));
        assert!(perms.check_file_write("/tmp/output.txt"));
    }

    #[test]
    fn test_path_similar_prefix_not_confused() {
        // /tmp-evil should NOT match allowed prefix /tmp
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileRead {
            allowed_paths: vec!["/tmp".to_string()],
        });
        assert!(
            !perms.check_file_read("/tmp-evil/file.txt"),
            "/tmp-evil must not match /tmp"
        );
    }

    // -----------------------------------------------------------------------
    // Network domain tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_network_spoofed_domain_denied() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["api.anthropic.com".to_string()],
        });

        // evil-api.anthropic.com should NOT match exact "api.anthropic.com"
        assert!(
            !perms.check_network("evil-api.anthropic.com"),
            "evil-api.anthropic.com must not match api.anthropic.com"
        );
    }

    #[test]
    fn test_network_suffix_matching() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec![".anthropic.com".to_string()],
        });

        // sub.anthropic.com should match .anthropic.com
        assert!(
            perms.check_network("sub.anthropic.com"),
            "sub.anthropic.com must match .anthropic.com"
        );
        // sub.api.anthropic.com should also match
        assert!(perms.check_network("sub.api.anthropic.com"));
    }

    #[test]
    fn test_network_suffix_does_not_match_partial() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec![".anthropic.com".to_string()],
        });

        // evil-anthropic.com should NOT match .anthropic.com
        assert!(
            !perms.check_network("evil-anthropic.com"),
            "evil-anthropic.com must not match .anthropic.com"
        );
        // anthropic.com itself should not match .anthropic.com (it's not a subdomain)
        assert!(!perms.check_network("anthropic.com"));
    }

    #[test]
    fn test_network_exact_match() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["api.anthropic.com".to_string()],
        });

        assert!(
            perms.check_network("api.anthropic.com"),
            "exact domain must match"
        );
        assert!(
            perms.check_network("API.ANTHROPIC.COM"),
            "exact domain match should be case-insensitive"
        );
    }

    // -----------------------------------------------------------------------
    // Network IP tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_private_ip_detection_ipv4() {
        // Loopback
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            127, 255, 255, 255
        ))));
        // 10.0.0.0/8
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        // 172.16.0.0/12
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255))));
        // 192.168.0.0/16
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        // 169.254.0.0/16 — link-local
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            169, 254, 169, 254
        ))));
        // 0.0.0.0
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));
    }

    #[test]
    fn test_public_ip_not_private() {
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
    }

    #[test]
    fn test_private_ip_detection_ipv6() {
        // ::1 — loopback
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        // fe80::1 — link-local
        assert!(is_private_ip(&IpAddr::V6(
            "fe80::1".parse::<Ipv6Addr>().unwrap()
        )));
    }

    #[test]
    fn test_check_network_ip_denies_private() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["*".to_string()],
        });
        // Even with wildcard, private IPs must be denied
        assert!(!perms.check_network_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(!perms.check_network_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!perms.check_network_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(!perms.check_network_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn test_check_network_ip_allows_public() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["*".to_string()],
        });
        assert!(perms.check_network_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    // -----------------------------------------------------------------------
    // Shell command hardening tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_shell_semicolon_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["ls".to_string()],
        });

        assert!(
            !perms.check_shell("ls; rm -rf /"),
            "semicolon-injected command must be denied"
        );
    }

    #[test]
    fn test_shell_pipe_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["echo".to_string()],
        });

        assert!(
            !perms.check_shell("echo hello | grep h"),
            "pipe to disallowed command must be denied"
        );
    }

    #[test]
    fn test_shell_subshell_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["echo".to_string()],
        });

        assert!(
            !perms.check_shell("echo $(cat /etc/passwd)"),
            "subshell injection must be denied"
        );
    }

    #[test]
    fn test_shell_backtick_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["echo".to_string()],
        });

        assert!(
            !perms.check_shell("echo `cat /etc/passwd`"),
            "backtick injection must be denied"
        );
    }

    #[test]
    fn test_shell_and_chain_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["ls".to_string()],
        });

        assert!(!perms.check_shell("ls && rm -rf /"));
    }

    #[test]
    fn test_shell_or_chain_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["ls".to_string()],
        });

        assert!(!perms.check_shell("ls || rm -rf /"));
    }

    #[test]
    fn test_shell_simple_allowed() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["echo".to_string()],
        });

        assert!(
            perms.check_shell("echo hello"),
            "simple allowed command must pass"
        );
    }

    #[test]
    fn test_shell_with_args_allowed() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["ls".to_string()],
        });

        assert!(perms.check_shell("ls -la"));
        assert!(perms.check_shell("ls"));
    }

    #[test]
    fn test_shell_allowed_pipe_both_allowed() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["echo".to_string(), "grep".to_string()],
        });

        assert!(
            perms.check_shell("echo hello | grep h"),
            "pipe with both commands allowed must pass"
        );
    }

    #[test]
    fn test_shell_strict_result() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["ls".to_string()],
        });

        let result = perms.check_shell_strict("ls; rm -rf /");
        match result {
            ShellCheckResult::Denied { reason } => {
                assert!(reason.contains("rm"));
            }
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn test_shell_newline_injection() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::ShellExec {
            allowed_commands: vec!["echo".to_string()],
        });

        assert!(!perms.check_shell("echo hello\nrm -rf /"));
    }

    // -----------------------------------------------------------------------
    // has_capability_type tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_has_capability_type() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileRead {
            allowed_paths: vec!["/tmp".to_string()],
        });
        perms.grant(Capability::NetworkAccess {
            allowed_hosts: vec!["example.com".to_string()],
        });

        assert!(perms.has_capability_type("file_read"));
        assert!(perms.has_capability_type("network_access"));
        assert!(!perms.has_capability_type("shell_exec"));
        assert!(!perms.has_capability_type("database_query"));
    }

    #[test]
    fn test_has_capability_type_database() {
        let mut perms = PermissionSet::new();
        perms.grant(Capability::DatabaseQuery);

        assert!(perms.has_capability_type("database_query"));
        assert!(!perms.has_capability_type("file_read"));
    }

    // -----------------------------------------------------------------------
    // Capability type_name tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_capability_type_names() {
        assert_eq!(
            Capability::FileRead {
                allowed_paths: vec![]
            }
            .type_name(),
            "file_read"
        );
        assert_eq!(
            Capability::FileWrite {
                allowed_paths: vec![]
            }
            .type_name(),
            "file_write"
        );
        assert_eq!(
            Capability::NetworkAccess {
                allowed_hosts: vec![]
            }
            .type_name(),
            "network_access"
        );
        assert_eq!(
            Capability::ShellExec {
                allowed_commands: vec![]
            }
            .type_name(),
            "shell_exec"
        );
        assert_eq!(
            Capability::EnvRead {
                allowed_vars: vec![]
            }
            .type_name(),
            "env_read"
        );
        assert_eq!(Capability::DatabaseQuery.type_name(), "database_query");
        assert_eq!(
            Capability::BrowserAccess {
                allowed_domains: vec![]
            }
            .type_name(),
            "browser_access"
        );
    }

    // -----------------------------------------------------------------------
    // split_shell_segments helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_split_shell_segments() {
        assert_eq!(split_shell_segments("ls"), vec!["ls"]);
        assert_eq!(split_shell_segments("ls; echo hi"), vec!["ls", "echo hi"]);
        assert_eq!(
            split_shell_segments("ls | grep foo"),
            vec!["ls", "grep foo"]
        );
        assert_eq!(split_shell_segments("a && b || c"), vec!["a", "b", "c"]);
        assert_eq!(
            split_shell_segments("echo $(cat /etc/passwd)"),
            vec!["echo", "cat /etc/passwd)"]
        );
    }

    #[test]
    fn test_extract_base_command() {
        assert_eq!(extract_base_command("ls -la"), "ls");
        assert_eq!(extract_base_command("echo hello world"), "echo");
        assert_eq!(extract_base_command("/usr/bin/ls -la"), "ls");
        assert_eq!(extract_base_command(""), "");
    }
}
