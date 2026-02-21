use agentor_agent::{AgentRunner, ModelConfig};
use agentor_gateway::GatewayServer;
use agentor_security::{AuditLog, Capability, PermissionSet, RateLimiter};
use agentor_session::FileSessionStore;
use agentor_skills::{SkillConfig, SkillLoader, SkillRegistry};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "agentor", about = "Agentor — Secure AI Agent Framework")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "agentor.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Serve {
        /// Host to bind to (overrides config)
        #[arg(long)]
        host: Option<String>,
        /// Port to listen on (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Manage skills
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
}

#[derive(Subcommand)]
enum SkillAction {
    /// List registered skills
    List,
}

#[derive(Deserialize)]
struct AgentorConfig {
    model: ModelConfig,
    #[serde(default = "default_data_dir")]
    data_dir: PathBuf,
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    security: SecurityConfig,
    #[serde(default)]
    skills: Vec<SkillConfig>,
}

#[derive(Deserialize)]
struct ServerConfig {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    tls: TlsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            tls: TlsConfig::default(),
        }
    }
}

#[derive(Deserialize, Default)]
struct TlsConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    cert_path: String,
    #[serde(default)]
    key_path: String,
    #[serde(default)]
    client_ca_path: String,
}

#[derive(Deserialize)]
struct SecurityConfig {
    #[serde(default = "default_rps")]
    max_requests_per_second: f64,
    #[serde(default = "default_burst")]
    max_burst: f64,
    #[serde(default = "default_max_msg_len")]
    max_message_length: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_requests_per_second: default_rps(),
            max_burst: default_burst(),
            max_message_length: default_max_msg_len(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}
fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    3000
}
fn default_rps() -> f64 {
    10.0
}
fn default_burst() -> f64 {
    50.0
}
fn default_max_msg_len() -> usize {
    100_000
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let cli = Cli::parse();

    // Load config
    let config_str = tokio::fs::read_to_string(&cli.config).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to read config file '{}': {}",
            cli.config.display(),
            e
        )
    })?;
    let config: AgentorConfig = toml::from_str(&config_str)?;

    // Resolve config base directory (for relative skill paths)
    let config_dir = cli
        .config
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    match cli.command {
        Commands::Serve { host, port } => {
            let host = host.unwrap_or(config.server.host);
            let port = port.unwrap_or(config.server.port);

            info!("Starting Agentor gateway on {}:{}", host, port);

            // Initialize security
            let audit = Arc::new(AuditLog::new(config.data_dir.join("audit")));
            let _rate_limiter = Arc::new(RateLimiter::new(
                config.security.max_burst,
                config.security.max_requests_per_second,
            ));

            // Initialize sessions
            let sessions = Arc::new(
                FileSessionStore::new(config.data_dir.join("sessions")).await?,
            );

            // Load skills from config
            let mut registry = SkillRegistry::new();
            if !config.skills.is_empty() {
                let loader = SkillLoader::new()?;
                let loaded = loader.load_all(&config.skills, &config_dir, &mut registry)?;
                info!(count = loaded, "Skills loaded from config");
            }

            // Build permissions from all loaded skills' required capabilities
            let mut permissions = PermissionSet::new();
            for desc in registry.list_descriptors() {
                for cap in &desc.required_capabilities {
                    permissions.grant(cap.clone());
                }
            }

            let skills = Arc::new(registry);

            let agent = Arc::new(AgentRunner::new(
                config.model,
                skills.clone(),
                permissions,
                audit,
            ));

            let app = GatewayServer::build(agent, sessions);

            let addr = format!("{}:{}", host, port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            info!("Agentor gateway listening on {}", addr);
            axum::serve(listener, app).await?;
        }
        Commands::Skill { action } => match action {
            SkillAction::List => {
                // Load skills from config to show what's available
                let mut registry = SkillRegistry::new();
                if !config.skills.is_empty() {
                    let loader = SkillLoader::new()?;
                    let _ = loader.load_all(&config.skills, &config_dir, &mut registry);
                }

                let skills = registry.list_descriptors();
                if skills.is_empty() {
                    println!("No skills registered.");
                    println!("Configure skills in agentor.toml under [[skills]]");
                } else {
                    println!("Registered skills:");
                    for skill in &skills {
                        println!("  {} — {}", skill.name, skill.description);
                        if !skill.required_capabilities.is_empty() {
                            println!("    Capabilities:");
                            for cap in &skill.required_capabilities {
                                match cap {
                                    Capability::FileRead { allowed_paths } => {
                                        println!("      file_read: {:?}", allowed_paths);
                                    }
                                    Capability::FileWrite { allowed_paths } => {
                                        println!("      file_write: {:?}", allowed_paths);
                                    }
                                    Capability::NetworkAccess { allowed_hosts } => {
                                        println!("      network: {:?}", allowed_hosts);
                                    }
                                    Capability::ShellExec { allowed_commands } => {
                                        println!("      shell: {:?}", allowed_commands);
                                    }
                                    _ => {
                                        println!("      {:?}", cap);
                                    }
                                }
                            }
                        }
                    }
                    println!("\nTotal: {} skill(s)", skills.len());
                }
            }
        },
    }

    Ok(())
}
