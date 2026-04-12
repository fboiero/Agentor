//! `TeeProvider` trait — common interface for spawning enclaves and getting attestations.

use crate::attestation::AttestationReport;
use crate::types::{EnclaveConfig, EnclaveInfo};
use argentor_core::ArgentorResult;
use async_trait::async_trait;

/// A TEE provider — abstraction over a specific hardware TEE backend.
///
/// Implementations exist per technology (AWS Nitro, Intel SGX, AMD SEV, ...).
/// All methods are async because real TEE operations involve IPC with
/// hypervisor / firmware / cloud APIs.
#[async_trait]
pub trait TeeProvider: Send + Sync {
    /// Human-readable provider name.
    fn name(&self) -> &str;

    /// TEE technology this provider implements.
    fn kind(&self) -> crate::types::TeeKind;

    /// Whether the required hardware and SDK are present on this host.
    fn is_available(&self) -> bool;

    /// Spawn a new enclave and return its metadata.
    async fn spawn_enclave(&self, config: EnclaveConfig) -> ArgentorResult<EnclaveInfo>;

    /// Terminate an enclave by ID.
    async fn terminate_enclave(&self, enclave_id: &str) -> ArgentorResult<()>;

    /// Request an attestation report bound to the given nonce.
    async fn get_attestation(
        &self,
        enclave_id: &str,
        nonce: &str,
    ) -> ArgentorResult<AttestationReport>;

    /// List all known enclaves for this provider.
    async fn list_enclaves(&self) -> ArgentorResult<Vec<EnclaveInfo>>;
}
