use agentor_core::{AgentorError, AgentorResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use tracing::{info, warn};

/// Manifest describing a skill package for the secure registry.
///
/// Every skill published to or installed from the registry must include
/// a manifest with its metadata, declared capabilities, and integrity checksum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Unique skill name (e.g., "file_read", "web_search").
    pub name: String,
    /// Semantic version (e.g., "1.0.0").
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Author name or organization.
    pub author: String,
    /// SPDX license identifier.
    pub license: Option<String>,
    /// SHA-256 hash of the WASM binary.
    pub checksum: String,
    /// Capabilities this skill declares it needs.
    pub capabilities: Vec<String>,
    /// Ed25519 signature over the canonical manifest bytes (excluding signature field).
    #[serde(default)]
    pub signature: Option<String>,
    /// Public key (hex-encoded) of the signer.
    #[serde(default)]
    pub signer_key: Option<String>,
    /// Minimum Agentor version required.
    #[serde(default)]
    pub min_agentor_version: Option<String>,
    /// Tags for discoverability.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Repository URL.
    #[serde(default)]
    pub repository: Option<String>,
}

impl SkillManifest {
    /// Compute SHA-256 checksum of a WASM binary file.
    pub fn compute_checksum(wasm_bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes);
        hex::encode(hasher.finalize())
    }

    /// Verify that the checksum matches the given WASM bytes.
    pub fn verify_checksum(&self, wasm_bytes: &[u8]) -> bool {
        let computed = Self::compute_checksum(wasm_bytes);
        constant_time_eq(self.checksum.as_bytes(), computed.as_bytes())
    }

    /// Returns the canonical bytes used for signing (manifest without signature fields).
    pub fn canonical_bytes(&self) -> AgentorResult<Vec<u8>> {
        let mut manifest_for_signing = self.clone();
        manifest_for_signing.signature = None;
        manifest_for_signing.signer_key = None;
        serde_json::to_vec(&manifest_for_signing)
            .map_err(|e| AgentorError::Config(format!("Failed to serialize manifest: {e}")))
    }

    /// Sign this manifest with an Ed25519 secret key (64 hex chars = 32 bytes).
    pub fn sign(&mut self, secret_key_hex: &str) -> AgentorResult<()> {
        let secret_bytes = hex::decode(secret_key_hex)
            .map_err(|e| AgentorError::Config(format!("Invalid secret key hex: {e}")))?;
        if secret_bytes.len() != 32 {
            return Err(AgentorError::Config(
                "Ed25519 secret key must be 32 bytes".into(),
            ));
        }

        let signing_key = ed25519_dalek::SigningKey::from_bytes(
            secret_bytes
                .as_slice()
                .try_into()
                .map_err(|_| AgentorError::Config("Invalid key length".into()))?,
        );

        let canonical = self.canonical_bytes()?;
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(&canonical);

        self.signature = Some(hex::encode(signature.to_bytes()));
        self.signer_key = Some(hex::encode(signing_key.verifying_key().to_bytes()));

        Ok(())
    }

    /// Verify the Ed25519 signature against a set of trusted public keys.
    pub fn verify_signature(&self, trusted_keys: &[String]) -> AgentorResult<bool> {
        let sig_hex = self
            .signature
            .as_ref()
            .ok_or_else(|| AgentorError::Config("Manifest has no signature".into()))?;

        let signer_hex = self
            .signer_key
            .as_ref()
            .ok_or_else(|| AgentorError::Config("Manifest has no signer key".into()))?;

        // Check if the signer key is in the trusted set
        if !trusted_keys.contains(signer_hex) {
            return Ok(false);
        }

        let sig_bytes = hex::decode(sig_hex)
            .map_err(|e| AgentorError::Config(format!("Invalid signature hex: {e}")))?;
        let key_bytes = hex::decode(signer_hex)
            .map_err(|e| AgentorError::Config(format!("Invalid key hex: {e}")))?;

        let signature = ed25519_dalek::Signature::from_bytes(
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| AgentorError::Config("Signature must be 64 bytes".into()))?,
        );

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(
            key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| AgentorError::Config("Public key must be 32 bytes".into()))?,
        )
        .map_err(|e| AgentorError::Config(format!("Invalid public key: {e}")))?;

        let canonical = self.canonical_bytes()?;

        use ed25519_dalek::Verifier;
        Ok(verifying_key.verify(&canonical, &signature).is_ok())
    }
}

/// Constant-time byte comparison to prevent timing attacks on checksums.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Result of vetting a skill package.
#[derive(Debug, Clone, Serialize)]
pub struct VettingResult {
    pub skill_name: String,
    pub passed: bool,
    pub checks: Vec<VettingCheck>,
}

/// Individual check in the vetting pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct VettingCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

/// Known dangerous capability patterns.
const DANGEROUS_CAPABILITIES: &[&str] = &[
    "shell_exec",
    "file_write",
    "network_access",
    "database_query",
    "browser_access",
];

/// Strings that indicate potential malicious intent in WASM imports.
const SUSPICIOUS_IMPORTS: &[&str] = &[
    "proc_exit",
    "fd_write",
    "environ_get",
    "args_get",
    "sock_connect",
];

/// Vet a skill package: checksum, signature, capabilities, and static analysis.
pub struct SkillVetter {
    /// Trusted public keys (hex-encoded Ed25519 verifying keys).
    trusted_keys: Vec<String>,
    /// Maximum allowed WASM binary size in bytes.
    max_wasm_size: usize,
    /// Whether unsigned skills are allowed (default: true for dev, false for prod).
    require_signatures: bool,
    /// Capabilities that are blocked entirely.
    blocked_capabilities: HashSet<String>,
}

impl SkillVetter {
    pub fn new() -> Self {
        Self {
            trusted_keys: Vec::new(),
            max_wasm_size: 10 * 1024 * 1024, // 10 MB
            require_signatures: false,
            blocked_capabilities: HashSet::new(),
        }
    }

    pub fn with_trusted_keys(mut self, keys: Vec<String>) -> Self {
        self.trusted_keys = keys;
        self
    }

    pub fn with_max_wasm_size(mut self, size: usize) -> Self {
        self.max_wasm_size = size;
        self
    }

    pub fn with_require_signatures(mut self, require: bool) -> Self {
        self.require_signatures = require;
        self
    }

    pub fn with_blocked_capabilities(mut self, caps: Vec<String>) -> Self {
        self.blocked_capabilities = caps.into_iter().collect();
        self
    }

    /// Run the full vetting pipeline on a skill package.
    pub fn vet(&self, manifest: &SkillManifest, wasm_bytes: &[u8]) -> VettingResult {
        let mut checks = Vec::new();

        // 1. Checksum verification
        let checksum_ok = manifest.verify_checksum(wasm_bytes);
        checks.push(VettingCheck {
            name: "checksum".into(),
            passed: checksum_ok,
            message: if checksum_ok {
                "SHA-256 checksum matches".into()
            } else {
                "SHA-256 checksum MISMATCH — binary may have been tampered with".into()
            },
        });

        // 2. Size limit
        let size_ok = wasm_bytes.len() <= self.max_wasm_size;
        checks.push(VettingCheck {
            name: "size_limit".into(),
            passed: size_ok,
            message: format!(
                "Binary size: {} bytes (limit: {})",
                wasm_bytes.len(),
                self.max_wasm_size
            ),
        });

        // 3. Signature verification
        let sig_ok = if self.require_signatures || manifest.signature.is_some() {
            match manifest.verify_signature(&self.trusted_keys) {
                Ok(valid) => {
                    checks.push(VettingCheck {
                        name: "signature".into(),
                        passed: valid,
                        message: if valid {
                            "Ed25519 signature valid from trusted key".into()
                        } else {
                            "Signature invalid or signer not trusted".into()
                        },
                    });
                    valid
                }
                Err(e) => {
                    checks.push(VettingCheck {
                        name: "signature".into(),
                        passed: false,
                        message: format!("Signature verification error: {e}"),
                    });
                    false
                }
            }
        } else {
            checks.push(VettingCheck {
                name: "signature".into(),
                passed: true,
                message: "No signature required (dev mode)".into(),
            });
            true
        };

        // 4. Capability analysis
        let cap_ok = self.check_capabilities(manifest, &mut checks);

        // 5. WASM static analysis (import scanning)
        let imports_ok = self.analyze_wasm_imports(wasm_bytes, &mut checks);

        let passed = checksum_ok && size_ok && sig_ok && cap_ok && imports_ok;

        if passed {
            info!(skill = %manifest.name, "Skill vetting PASSED");
        } else {
            warn!(skill = %manifest.name, "Skill vetting FAILED");
        }

        VettingResult {
            skill_name: manifest.name.clone(),
            passed,
            checks,
        }
    }

    fn check_capabilities(&self, manifest: &SkillManifest, checks: &mut Vec<VettingCheck>) -> bool {
        let mut passed = true;

        // Check for blocked capabilities
        for cap in &manifest.capabilities {
            if self.blocked_capabilities.contains(cap) {
                checks.push(VettingCheck {
                    name: "blocked_capability".into(),
                    passed: false,
                    message: format!("Capability '{cap}' is blocked by policy"),
                });
                passed = false;
            }
        }

        // Flag dangerous capabilities (warning, not blocking)
        let dangerous: Vec<&str> = manifest
            .capabilities
            .iter()
            .filter(|c| DANGEROUS_CAPABILITIES.contains(&c.as_str()))
            .map(|c| c.as_str())
            .collect();

        if dangerous.is_empty() {
            checks.push(VettingCheck {
                name: "capability_risk".into(),
                passed: true,
                message: "No high-risk capabilities declared".into(),
            });
        } else {
            checks.push(VettingCheck {
                name: "capability_risk".into(),
                passed: true, // Warning only, doesn't fail
                message: format!("High-risk capabilities declared: {}", dangerous.join(", ")),
            });
        }

        passed
    }

    fn analyze_wasm_imports(&self, wasm_bytes: &[u8], checks: &mut Vec<VettingCheck>) -> bool {
        // Basic WASM validation: check magic number
        if wasm_bytes.len() < 8 {
            checks.push(VettingCheck {
                name: "wasm_valid".into(),
                passed: false,
                message: "Binary too small to be valid WASM".into(),
            });
            return false;
        }

        let magic = &wasm_bytes[0..4];
        if magic != b"\0asm" {
            checks.push(VettingCheck {
                name: "wasm_valid".into(),
                passed: false,
                message: "Invalid WASM magic number".into(),
            });
            return false;
        }

        checks.push(VettingCheck {
            name: "wasm_valid".into(),
            passed: true,
            message: "Valid WASM binary".into(),
        });

        // Scan for suspicious import names in the binary
        // This is a heuristic — we look for string patterns in the binary
        let binary_str = String::from_utf8_lossy(wasm_bytes);
        let mut found_suspicious = Vec::new();

        for import_name in SUSPICIOUS_IMPORTS {
            if binary_str.contains(import_name) {
                found_suspicious.push(*import_name);
            }
        }

        if found_suspicious.is_empty() {
            checks.push(VettingCheck {
                name: "import_analysis".into(),
                passed: true,
                message: "No suspicious WASM imports detected".into(),
            });
        } else {
            checks.push(VettingCheck {
                name: "import_analysis".into(),
                passed: true, // Warning only — WASI imports are often legitimate
                message: format!(
                    "Detected WASI imports (review recommended): {}",
                    found_suspicious.join(", ")
                ),
            });
        }

        true
    }
}

impl Default for SkillVetter {
    fn default() -> Self {
        Self::new()
    }
}

/// Local skill index for tracking installed skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIndex {
    /// Installed skills indexed by name.
    pub skills: Vec<SkillIndexEntry>,
}

/// An entry in the local skill index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillIndexEntry {
    pub manifest: SkillManifest,
    /// Path to the installed WASM binary (relative to skills directory).
    pub wasm_path: String,
    /// When this skill was installed.
    pub installed_at: String,
    /// Whether this skill passed vetting.
    pub vetted: bool,
}

impl SkillIndex {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    /// Load the index from a JSON file.
    pub fn load(path: &Path) -> AgentorResult<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| AgentorError::Config(format!("Failed to read skill index: {e}")))?;
        serde_json::from_str(&content)
            .map_err(|e| AgentorError::Config(format!("Failed to parse skill index: {e}")))
    }

    /// Save the index to a JSON file.
    pub fn save(&self, path: &Path) -> AgentorResult<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| AgentorError::Config(format!("Failed to serialize skill index: {e}")))?;
        std::fs::write(path, content)
            .map_err(|e| AgentorError::Config(format!("Failed to write skill index: {e}")))?;
        Ok(())
    }

    /// Install a skill: vet it, copy WASM, add to index.
    pub fn install(
        &mut self,
        manifest: SkillManifest,
        wasm_bytes: &[u8],
        skills_dir: &Path,
        vetter: &SkillVetter,
    ) -> AgentorResult<VettingResult> {
        let result = vetter.vet(&manifest, wasm_bytes);

        if !result.passed {
            return Ok(result);
        }

        // Remove existing version if present
        self.skills.retain(|e| e.manifest.name != manifest.name);

        // Write WASM binary
        let wasm_filename = format!("{}-{}.wasm", manifest.name, manifest.version);
        let wasm_path = skills_dir.join(&wasm_filename);
        std::fs::create_dir_all(skills_dir)
            .map_err(|e| AgentorError::Config(format!("Failed to create skills dir: {e}")))?;
        std::fs::write(&wasm_path, wasm_bytes)
            .map_err(|e| AgentorError::Config(format!("Failed to write WASM binary: {e}")))?;

        // Add to index
        self.skills.push(SkillIndexEntry {
            manifest,
            wasm_path: wasm_filename,
            installed_at: chrono::Utc::now().to_rfc3339(),
            vetted: true,
        });

        info!(skill = %result.skill_name, "Skill installed successfully");

        Ok(result)
    }

    /// Uninstall a skill by name.
    pub fn uninstall(&mut self, name: &str, skills_dir: &Path) -> AgentorResult<bool> {
        let entry = self.skills.iter().find(|e| e.manifest.name == name);
        if let Some(entry) = entry {
            let wasm_path = skills_dir.join(&entry.wasm_path);
            if wasm_path.exists() {
                std::fs::remove_file(&wasm_path).map_err(|e| {
                    AgentorError::Config(format!("Failed to remove WASM binary: {e}"))
                })?;
            }
            self.skills.retain(|e| e.manifest.name != name);
            info!(skill = name, "Skill uninstalled");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get a skill entry by name.
    pub fn get(&self, name: &str) -> Option<&SkillIndexEntry> {
        self.skills.iter().find(|e| e.manifest.name == name)
    }

    /// List all installed skills.
    pub fn list(&self) -> &[SkillIndexEntry] {
        &self.skills
    }
}

impl Default for SkillIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // Minimal valid WASM binary (empty module)
    fn minimal_wasm() -> Vec<u8> {
        vec![
            0x00, 0x61, 0x73, 0x6d, // magic: \0asm
            0x01, 0x00, 0x00, 0x00, // version: 1
        ]
    }

    fn test_manifest(wasm_bytes: &[u8]) -> SkillManifest {
        SkillManifest {
            name: "test_skill".into(),
            version: "1.0.0".into(),
            description: "A test skill".into(),
            author: "test".into(),
            license: Some("MIT".into()),
            checksum: SkillManifest::compute_checksum(wasm_bytes),
            capabilities: vec!["file_read".into()],
            signature: None,
            signer_key: None,
            min_agentor_version: None,
            tags: vec!["test".into()],
            repository: None,
        }
    }

    #[test]
    fn checksum_matches_valid_binary() {
        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm);
        assert!(manifest.verify_checksum(&wasm));
    }

    #[test]
    fn checksum_rejects_tampered_binary() {
        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm);
        let mut tampered = wasm.clone();
        tampered.push(0xff);
        assert!(!manifest.verify_checksum(&tampered));
    }

    #[test]
    fn vetting_passes_valid_skill() {
        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm);
        let vetter = SkillVetter::new();
        let result = vetter.vet(&manifest, &wasm);
        assert!(result.passed);
        assert!(result.checks.iter().all(|c| c.passed));
    }

    #[test]
    fn vetting_fails_checksum_mismatch() {
        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm);
        let tampered = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0xff];
        let vetter = SkillVetter::new();
        let result = vetter.vet(&manifest, &tampered);
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "checksum" && !c.passed));
    }

    #[test]
    fn vetting_fails_size_limit() {
        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm);
        let vetter = SkillVetter::new().with_max_wasm_size(4); // Too small
        let result = vetter.vet(&manifest, &wasm);
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "size_limit" && !c.passed));
    }

    #[test]
    fn vetting_fails_invalid_wasm_magic() {
        let fake = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let manifest = test_manifest(&fake);
        let vetter = SkillVetter::new();
        let result = vetter.vet(&manifest, &fake);
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "wasm_valid" && !c.passed));
    }

    #[test]
    fn vetting_fails_blocked_capability() {
        let wasm = minimal_wasm();
        let mut manifest = test_manifest(&wasm);
        manifest.capabilities = vec!["shell_exec".into()];
        let vetter = SkillVetter::new().with_blocked_capabilities(vec!["shell_exec".into()]);
        let result = vetter.vet(&manifest, &wasm);
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "blocked_capability" && !c.passed));
    }

    #[test]
    fn vetting_warns_dangerous_capabilities() {
        let wasm = minimal_wasm();
        let mut manifest = test_manifest(&wasm);
        manifest.capabilities = vec!["shell_exec".into(), "network_access".into()];
        let vetter = SkillVetter::new();
        let result = vetter.vet(&manifest, &wasm);
        assert!(result.passed); // Warnings don't fail
        let risk_check = result
            .checks
            .iter()
            .find(|c| c.name == "capability_risk")
            .unwrap();
        assert!(risk_check.message.contains("shell_exec"));
    }

    #[test]
    fn ed25519_sign_and_verify() {
        let wasm = minimal_wasm();
        let mut manifest = test_manifest(&wasm);

        // Generate a keypair
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_hex = hex::encode(signing_key.to_bytes());
        let public_hex = hex::encode(signing_key.verifying_key().to_bytes());

        // Sign
        manifest.sign(&secret_hex).unwrap();
        assert!(manifest.signature.is_some());

        // Verify with trusted key
        let trusted = vec![public_hex.clone()];
        assert!(manifest.verify_signature(&trusted).unwrap());

        // Verify with untrusted key fails
        let untrusted =
            vec!["0000000000000000000000000000000000000000000000000000000000000000".into()];
        assert!(!manifest.verify_signature(&untrusted).unwrap());
    }

    #[test]
    fn vetting_requires_signature_when_configured() {
        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm); // No signature
        let vetter = SkillVetter::new().with_require_signatures(true);
        let result = vetter.vet(&manifest, &wasm);
        assert!(!result.passed);
        assert!(result
            .checks
            .iter()
            .any(|c| c.name == "signature" && !c.passed));
    }

    #[test]
    fn vetting_signed_skill_passes() {
        let wasm = minimal_wasm();
        let mut manifest = test_manifest(&wasm);

        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_hex = hex::encode(signing_key.to_bytes());
        let public_hex = hex::encode(signing_key.verifying_key().to_bytes());

        manifest.sign(&secret_hex).unwrap();

        let vetter = SkillVetter::new()
            .with_require_signatures(true)
            .with_trusted_keys(vec![public_hex]);

        let result = vetter.vet(&manifest, &wasm);
        assert!(result.passed);
    }

    #[test]
    fn skill_index_install_and_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        let index_path = dir.path().join("index.json");

        let wasm = minimal_wasm();
        let manifest = test_manifest(&wasm);
        let vetter = SkillVetter::new();

        let mut index = SkillIndex::new();
        let result = index
            .install(manifest, &wasm, &skills_dir, &vetter)
            .unwrap();
        assert!(result.passed);
        assert_eq!(index.list().len(), 1);
        assert!(index.get("test_skill").is_some());

        // Verify WASM was written
        let wasm_path = skills_dir.join("test_skill-1.0.0.wasm");
        assert!(wasm_path.exists());

        // Save and reload index
        index.save(&index_path).unwrap();
        let reloaded = SkillIndex::load(&index_path).unwrap();
        assert_eq!(reloaded.list().len(), 1);

        // Uninstall
        assert!(index.uninstall("test_skill", &skills_dir).unwrap());
        assert_eq!(index.list().len(), 0);
        assert!(!wasm_path.exists());
    }

    #[test]
    fn skill_index_rejects_failed_vetting() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");

        let wasm = minimal_wasm();
        let mut manifest = test_manifest(&wasm);
        manifest.checksum = "wrong_checksum".into(); // Will fail vetting

        let vetter = SkillVetter::new();
        let mut index = SkillIndex::new();
        let result = index
            .install(manifest, &wasm, &skills_dir, &vetter)
            .unwrap();
        assert!(!result.passed);
        assert_eq!(index.list().len(), 0); // Not installed
    }

    #[test]
    fn skill_index_upgrade_replaces_old_version() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");

        let wasm = minimal_wasm();
        let vetter = SkillVetter::new();
        let mut index = SkillIndex::new();

        // Install v1
        let manifest_v1 = test_manifest(&wasm);
        index
            .install(manifest_v1, &wasm, &skills_dir, &vetter)
            .unwrap();
        assert_eq!(index.list().len(), 1);

        // Install v2 (same name, different version)
        let mut manifest_v2 = test_manifest(&wasm);
        manifest_v2.version = "2.0.0".into();
        index
            .install(manifest_v2, &wasm, &skills_dir, &vetter)
            .unwrap();
        assert_eq!(index.list().len(), 1);
        assert_eq!(index.get("test_skill").unwrap().manifest.version, "2.0.0");
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }
}
