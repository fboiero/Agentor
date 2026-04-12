//! Core TEE types — enclave info, identity, code measurement.

use serde::{Deserialize, Serialize};

/// The kind of TEE technology in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeeKind {
    /// AWS Nitro Enclaves (most accessible — runs in EC2).
    AwsNitro,
    /// Intel SGX (Software Guard Extensions) — most mature, requires SGX-enabled CPU.
    IntelSgx,
    /// AMD SEV-SNP (Secure Encrypted Virtualization) — for VMs.
    AmdSev,
    /// ARM TrustZone (mobile/embedded).
    ArmTrustZone,
    /// Stub for testing.
    Stub,
}

impl TeeKind {
    /// Human-readable name for the TEE technology.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AwsNitro => "AWS Nitro Enclaves",
            Self::IntelSgx => "Intel SGX",
            Self::AmdSev => "AMD SEV-SNP",
            Self::ArmTrustZone => "ARM TrustZone",
            Self::Stub => "Stub",
        }
    }

    /// Returns `true` if this TEE is VM-based (isolation at the VM boundary).
    pub fn is_vm_based(&self) -> bool {
        matches!(self, Self::AwsNitro | Self::AmdSev)
    }

    /// Returns `true` if this TEE is process-based (isolation within a process).
    pub fn is_process_based(&self) -> bool {
        matches!(self, Self::IntelSgx | Self::ArmTrustZone)
    }
}

/// Information about an enclave instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveInfo {
    /// Unique identifier for this enclave.
    pub enclave_id: String,
    /// The TEE technology backing this enclave.
    pub kind: TeeKind,
    /// Current lifecycle status.
    pub status: EnclaveStatus,
    /// Cryptographic measurements of the enclave's code.
    pub measurements: CodeMeasurements,
    /// UTC timestamp of when the enclave was spawned.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Lifecycle status of an enclave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnclaveStatus {
    /// Enclave is being set up (allocating memory, loading image).
    Initializing,
    /// Enclave is running and healthy.
    Running,
    /// Enclave is paused (state retained, not executing).
    Paused,
    /// Enclave has been terminated cleanly.
    Terminated,
    /// Enclave is in an error state.
    Error,
}

/// Cryptographic measurements of the enclave's code.
///
/// Each TEE technology exposes a slightly different set of measurements.
/// All hashes are hex-encoded.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeMeasurements {
    /// SHA-384 hash of enclave image (Nitro PCR0).
    pub image_hash: String,
    /// SHA-384 hash of bootstrap process (Nitro PCR1).
    pub kernel_hash: Option<String>,
    /// SHA-384 hash of application (Nitro PCR2).
    pub application_hash: Option<String>,
    /// MRENCLAVE for Intel SGX — identity of the enclave code.
    pub mrenclave: Option<String>,
    /// MRSIGNER for Intel SGX — identity of the enclave signer.
    pub mrsigner: Option<String>,
}

/// Configuration for spawning an enclave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveConfig {
    /// Which TEE technology to use.
    pub kind: TeeKind,
    /// How much memory the enclave should be allocated, in MB.
    pub memory_mb: u32,
    /// Number of vCPUs to allocate.
    pub cpu_count: u32,
    /// Whether to run in debug mode — MUST be false in production
    /// (debug mode disables memory encryption and attestation binding).
    pub debug_mode: bool,
    /// Path to the enclave image file (e.g. `.eif` for Nitro, `.sgx` for SGX).
    pub enclave_image_path: Option<String>,
}

impl EnclaveConfig {
    /// Create a production-safe default config for the given TEE kind.
    pub fn production(kind: TeeKind, memory_mb: u32, cpu_count: u32) -> Self {
        Self {
            kind,
            memory_mb,
            cpu_count,
            debug_mode: false,
            enclave_image_path: None,
        }
    }

    /// Create a debug config. DO NOT use in production.
    pub fn debug(kind: TeeKind) -> Self {
        Self {
            kind,
            memory_mb: 512,
            cpu_count: 1,
            debug_mode: true,
            enclave_image_path: None,
        }
    }

    /// Validate that the config is sensible.
    pub fn validate(&self) -> Result<(), String> {
        if self.memory_mb < 128 {
            return Err("memory_mb must be at least 128".into());
        }
        if self.cpu_count == 0 {
            return Err("cpu_count must be >= 1".into());
        }
        if self.memory_mb > 524_288 {
            return Err("memory_mb must be <= 524288 (512 GB)".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn tee_kind_display_names() {
        assert_eq!(TeeKind::AwsNitro.display_name(), "AWS Nitro Enclaves");
        assert_eq!(TeeKind::IntelSgx.display_name(), "Intel SGX");
        assert_eq!(TeeKind::AmdSev.display_name(), "AMD SEV-SNP");
        assert_eq!(TeeKind::ArmTrustZone.display_name(), "ARM TrustZone");
        assert_eq!(TeeKind::Stub.display_name(), "Stub");
    }

    #[test]
    fn tee_kind_vm_based_classification() {
        assert!(TeeKind::AwsNitro.is_vm_based());
        assert!(TeeKind::AmdSev.is_vm_based());
        assert!(!TeeKind::IntelSgx.is_vm_based());
        assert!(!TeeKind::ArmTrustZone.is_vm_based());
        assert!(!TeeKind::Stub.is_vm_based());
    }

    #[test]
    fn tee_kind_process_based_classification() {
        assert!(TeeKind::IntelSgx.is_process_based());
        assert!(TeeKind::ArmTrustZone.is_process_based());
        assert!(!TeeKind::AwsNitro.is_process_based());
        assert!(!TeeKind::AmdSev.is_process_based());
    }

    #[test]
    fn tee_kind_serde_roundtrip() {
        for k in [
            TeeKind::AwsNitro,
            TeeKind::IntelSgx,
            TeeKind::AmdSev,
            TeeKind::ArmTrustZone,
            TeeKind::Stub,
        ] {
            let s = serde_json::to_string(&k).unwrap();
            let back: TeeKind = serde_json::from_str(&s).unwrap();
            assert_eq!(k, back);
        }
    }

    #[test]
    fn enclave_status_equality() {
        assert_eq!(EnclaveStatus::Running, EnclaveStatus::Running);
        assert_ne!(EnclaveStatus::Running, EnclaveStatus::Paused);
    }

    #[test]
    fn enclave_status_serde_roundtrip() {
        for s in [
            EnclaveStatus::Initializing,
            EnclaveStatus::Running,
            EnclaveStatus::Paused,
            EnclaveStatus::Terminated,
            EnclaveStatus::Error,
        ] {
            let j = serde_json::to_string(&s).unwrap();
            let back: EnclaveStatus = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn code_measurements_default() {
        let m = CodeMeasurements::default();
        assert_eq!(m.image_hash, "");
        assert!(m.kernel_hash.is_none());
        assert!(m.application_hash.is_none());
        assert!(m.mrenclave.is_none());
        assert!(m.mrsigner.is_none());
    }

    #[test]
    fn code_measurements_serde_roundtrip() {
        let m = CodeMeasurements {
            image_hash: "abc123".into(),
            kernel_hash: Some("def456".into()),
            application_hash: None,
            mrenclave: Some("sgx-mrenclave".into()),
            mrsigner: Some("sgx-mrsigner".into()),
        };
        let j = serde_json::to_string(&m).unwrap();
        let back: CodeMeasurements = serde_json::from_str(&j).unwrap();
        assert_eq!(m.image_hash, back.image_hash);
        assert_eq!(m.mrenclave, back.mrenclave);
    }

    #[test]
    fn enclave_config_production_defaults() {
        let c = EnclaveConfig::production(TeeKind::AwsNitro, 2048, 2);
        assert_eq!(c.kind, TeeKind::AwsNitro);
        assert_eq!(c.memory_mb, 2048);
        assert_eq!(c.cpu_count, 2);
        assert!(!c.debug_mode);
    }

    #[test]
    fn enclave_config_debug_is_debug() {
        let c = EnclaveConfig::debug(TeeKind::IntelSgx);
        assert!(c.debug_mode);
        assert_eq!(c.kind, TeeKind::IntelSgx);
    }

    #[test]
    fn enclave_config_validation_ok() {
        let c = EnclaveConfig::production(TeeKind::AwsNitro, 512, 2);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn enclave_config_validation_rejects_low_memory() {
        let c = EnclaveConfig::production(TeeKind::AwsNitro, 64, 2);
        assert!(c.validate().is_err());
    }

    #[test]
    fn enclave_config_validation_rejects_zero_cpus() {
        let c = EnclaveConfig::production(TeeKind::AwsNitro, 1024, 0);
        assert!(c.validate().is_err());
    }

    #[test]
    fn enclave_config_validation_rejects_excessive_memory() {
        let c = EnclaveConfig::production(TeeKind::AwsNitro, 600_000, 2);
        assert!(c.validate().is_err());
    }

    #[test]
    fn enclave_config_serde_roundtrip() {
        let c = EnclaveConfig::production(TeeKind::AmdSev, 4096, 4);
        let j = serde_json::to_string(&c).unwrap();
        let back: EnclaveConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(back.memory_mb, 4096);
        assert_eq!(back.cpu_count, 4);
        assert_eq!(back.kind, TeeKind::AmdSev);
    }

    #[test]
    fn enclave_info_serde_roundtrip() {
        let info = EnclaveInfo {
            enclave_id: "enc-123".into(),
            kind: TeeKind::AwsNitro,
            status: EnclaveStatus::Running,
            measurements: CodeMeasurements::default(),
            created_at: chrono::Utc::now(),
        };
        let j = serde_json::to_string(&info).unwrap();
        let back: EnclaveInfo = serde_json::from_str(&j).unwrap();
        assert_eq!(back.enclave_id, "enc-123");
        assert_eq!(back.kind, TeeKind::AwsNitro);
        assert_eq!(back.status, EnclaveStatus::Running);
    }
}
