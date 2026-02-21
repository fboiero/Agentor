use agentor_core::{AgentorError, AgentorResult};
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    server::WebPkiClientVerifier,
    RootCertStore, ServerConfig,
};
use tokio_rustls::TlsAcceptor;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: String,
    pub key_path: String,
    /// If set, enables mTLS — clients must present a certificate signed by this CA.
    #[serde(default)]
    pub client_ca_path: String,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: String::new(),
            key_path: String::new(),
            client_ca_path: String::new(),
        }
    }
}

/// Build a TLS acceptor from the given config.
/// If `client_ca_path` is set, mTLS is enforced.
pub async fn build_tls_acceptor(config: &TlsConfig) -> AgentorResult<TlsAcceptor> {
    if !config.enabled {
        return Err(AgentorError::Config("TLS is not enabled".to_string()));
    }

    let certs = load_certs(&config.cert_path).await?;
    let key = load_private_key(&config.key_path).await?;

    let mut server_config = if !config.client_ca_path.is_empty() {
        // mTLS: require client certificates
        info!("mTLS enabled — requiring client certificates");
        let client_ca_certs = load_certs(&config.client_ca_path).await?;

        let mut root_store = RootCertStore::empty();
        for cert in client_ca_certs {
            root_store
                .add(cert)
                .map_err(|e| AgentorError::Config(format!("Invalid client CA cert: {}", e)))?;
        }

        let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| AgentorError::Config(format!("Failed to build client verifier: {}", e)))?;

        ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certs, key)
            .map_err(|e| AgentorError::Config(format!("TLS config error: {}", e)))?
    } else {
        // Standard TLS (no client cert required)
        info!("TLS enabled (no mTLS)");
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| AgentorError::Config(format!("TLS config error: {}", e)))?
    };

    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

async fn load_certs(path: &str) -> AgentorResult<Vec<CertificateDer<'static>>> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| AgentorError::Config(format!("Failed to read cert '{}': {}", path, e)))?;

    let mut reader = std::io::BufReader::new(data.as_slice());
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .filter_map(|r| r.ok())
        .collect();

    if certs.is_empty() {
        return Err(AgentorError::Config(format!(
            "No certificates found in '{}'",
            path
        )));
    }

    Ok(certs)
}

async fn load_private_key(path: &str) -> AgentorResult<PrivateKeyDer<'static>> {
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| AgentorError::Config(format!("Failed to read key '{}': {}", path, e)))?;

    let mut reader = std::io::BufReader::new(data.as_slice());

    // Try PKCS8 first, then RSA, then EC
    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| AgentorError::Config(format!("Failed to parse private key: {}", e)))?
        .ok_or_else(|| {
            AgentorError::Config(format!("No private key found in '{}'", path))
        })?;

    Ok(key)
}

/// Check if a path exists and is readable.
pub async fn validate_tls_config(config: &TlsConfig) -> AgentorResult<()> {
    if !config.enabled {
        return Ok(());
    }

    if config.cert_path.is_empty() {
        return Err(AgentorError::Config(
            "TLS enabled but cert_path is empty".to_string(),
        ));
    }
    if config.key_path.is_empty() {
        return Err(AgentorError::Config(
            "TLS enabled but key_path is empty".to_string(),
        ));
    }

    if !Path::new(&config.cert_path).exists() {
        return Err(AgentorError::Config(format!(
            "TLS cert not found: {}",
            config.cert_path
        )));
    }
    if !Path::new(&config.key_path).exists() {
        return Err(AgentorError::Config(format!(
            "TLS key not found: {}",
            config.key_path
        )));
    }

    if !config.client_ca_path.is_empty() && !Path::new(&config.client_ca_path).exists() {
        return Err(AgentorError::Config(format!(
            "Client CA cert not found: {}",
            config.client_ca_path
        )));
    }

    Ok(())
}
