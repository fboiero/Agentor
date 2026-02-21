use agentor_agent::{AgentRunner, ModelConfig};
use agentor_gateway::GatewayServer;
use agentor_security::{AuditLog, PermissionSet};
use agentor_session::FileSessionStore;
use agentor_skills::SkillRegistry;
use clap::{Parser, Subcommand};
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
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,
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

#[derive(serde::Deserialize)]
struct AgentorConfig {
    model: ModelConfig,
    #[serde(default = "default_data_dir")]
    data_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
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

    match cli.command {
        Commands::Serve { host, port } => {
            info!("Starting Agentor gateway on {}:{}", host, port);

            // Initialize components
            let audit = Arc::new(AuditLog::new(config.data_dir.join("audit")));
            let sessions = Arc::new(
                FileSessionStore::new(config.data_dir.join("sessions")).await?,
            );
            let skills = Arc::new(SkillRegistry::new());
            let permissions = PermissionSet::new();

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
                let registry = SkillRegistry::new();
                let skills = registry.list_descriptors();
                if skills.is_empty() {
                    println!("No skills registered.");
                } else {
                    for skill in skills {
                        println!("  {} — {}", skill.name, skill.description);
                    }
                }
            }
        },
    }

    Ok(())
}
