use agentor_agent::{AgentRunner, ModelConfig};
use agentor_gateway::{AuthConfig, GatewayServer};
use agentor_security::tls;
use agentor_security::{AuditLog, Capability, PermissionSet, RateLimiter};
use agentor_session::FileSessionStore;
use agentor_skills::{MarkdownSkillLoader, SkillConfig, SkillLoader, SkillRegistry, ToolGroup};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
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
    /// Generate a compliance report
    Compliance {
        #[command(subcommand)]
        action: ComplianceAction,
    },
    /// Run multi-agent orchestration on a task
    Orchestrate {
        /// The task description for the orchestrator to decompose and execute
        task: String,
        /// Directory to write output artifacts (code, tests, spec, review)
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
        /// Enable interactive stdin-based human approval for high-risk operations
        #[arg(long)]
        interactive_approval: bool,
        /// Timeout in seconds for interactive approval prompts (default: 300)
        #[arg(long, default_value = "300")]
        approval_timeout: u64,
    },
}

#[derive(Subcommand)]
enum SkillAction {
    /// List registered skills
    List,
}

#[derive(Subcommand)]
enum ComplianceAction {
    /// Generate a compliance report for all frameworks
    Report,
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
    #[serde(default)]
    mcp_servers: Vec<McpServerConfig>,
    /// Directory containing markdown skill files (.md with YAML frontmatter).
    #[serde(default)]
    markdown_skills_dir: Option<PathBuf>,
    /// Custom tool groups for progressive skill disclosure.
    #[serde(default)]
    tool_groups: Vec<ToolGroup>,
}

/// MCP server config — delegates to the manager's type.
type McpServerConfig = agentor_mcp::McpServerConfig;

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
#[allow(dead_code)]
struct SecurityConfig {
    #[serde(default = "default_rps")]
    max_requests_per_second: f64,
    #[serde(default = "default_burst")]
    max_burst: f64,
    #[serde(default = "default_max_msg_len")]
    max_message_length: usize,
    #[serde(default)]
    api_keys: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_requests_per_second: default_rps(),
            max_burst: default_burst(),
            max_message_length: default_max_msg_len(),
            api_keys: vec![],
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

/// Expand `${VAR_NAME}` patterns in a string with environment variable values.
/// Unknown variables are replaced with empty strings.
fn expand_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                var_name.push(c);
            }
            if let Ok(val) = std::env::var(&var_name) {
                result.push_str(&val);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Load markdown skills from the configured directory into the registry.
/// Returns the prompt injection text to append to system prompts.
async fn load_markdown_skills(
    config: &AgentorConfig,
    config_dir: &std::path::Path,
    registry: &mut SkillRegistry,
) -> String {
    let dir = match &config.markdown_skills_dir {
        Some(d) => {
            if d.is_absolute() {
                d.clone()
            } else {
                config_dir.join(d)
            }
        }
        None => return String::new(),
    };

    let loader = MarkdownSkillLoader::new(dir);
    match loader.load_all().await {
        Ok(loaded) => {
            let prompt_text = loaded.build_prompt_injection();

            // Register callable markdown skills
            for skill in loaded.callable {
                registry.register(skill);
            }

            info!(
                prompt_skills = loaded.prompts.len(),
                "Markdown skills loaded"
            );
            prompt_text
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load markdown skills");
            String::new()
        }
    }
}

/// Register custom tool groups from config.
fn load_tool_groups(config: &AgentorConfig, registry: &mut SkillRegistry) {
    if !config.tool_groups.is_empty() {
        registry.register_groups(config.tool_groups.clone());
        info!(
            count = config.tool_groups.len(),
            "Custom tool groups registered"
        );
    }
}

/// Wait for a shutdown signal (Ctrl+C or SIGTERM).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, shutting down..."),
        _ = terminate => info!("Received SIGTERM, shutting down..."),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present (API keys, tokens)
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    // For orchestrate command, suppress JSON logs to stderr unless RUST_LOG is explicitly set.
    // This keeps the output clean for piping.
    let is_orchestrate = matches!(cli.command, Commands::Orchestrate { .. });
    let default_level = if is_orchestrate && std::env::var("RUST_LOG").is_err() {
        "warn"
    } else {
        "info"
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level)),
        )
        .json()
        .with_writer(std::io::stderr)
        .init();

    // Load config with environment variable expansion
    let config_str = tokio::fs::read_to_string(&cli.config).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to read config file '{}': {}",
            cli.config.display(),
            e
        )
    })?;
    let config_str = expand_env_vars(&config_str);
    let config: AgentorConfig = toml::from_str(&config_str)?;

    // Resolve config base directory (for relative skill paths)
    let config_dir = cli
        .config
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    match cli.command {
        Commands::Serve { host, port } => {
            let host = host.unwrap_or_else(|| config.server.host.clone());
            let port = port.unwrap_or(config.server.port);

            info!("Starting Agentor gateway on {}:{}", host, port);

            // Initialize security
            let audit = Arc::new(AuditLog::new(config.data_dir.join("audit")));
            let rate_limiter = Arc::new(RateLimiter::new(
                config.security.max_burst,
                config.security.max_requests_per_second,
            ));
            let auth_config = AuthConfig::new(config.security.api_keys.clone());
            if auth_config.is_enabled() {
                info!(
                    keys = config.security.api_keys.len(),
                    "API key auth enabled"
                );
            }

            // Initialize sessions
            let sessions = Arc::new(FileSessionStore::new(config.data_dir.join("sessions")).await?);

            // Initialize vector memory (persistent)
            let memory_path = config.data_dir.join("memory").join("vectors.jsonl");
            let vector_store: Arc<dyn agentor_memory::VectorStore> = Arc::new(
                agentor_memory::FileVectorStore::new(memory_path)
                    .await
                    .expect("Failed to initialize vector store"),
            );
            let embedder: Arc<dyn agentor_memory::EmbeddingProvider> =
                Arc::new(agentor_memory::LocalEmbedding::default());
            info!("Vector memory initialized");

            // Load skills: builtins (with memory) first, then WASM skills from config
            let mut registry = SkillRegistry::new();
            agentor_builtins::register_builtins_with_memory(&mut registry, vector_store, embedder);
            info!(count = registry.skill_count(), "Built-in skills registered");

            if !config.skills.is_empty() {
                let loader = SkillLoader::new()?;
                let loaded = loader.load_all(&config.skills, &config_dir, &mut registry)?;
                info!(count = loaded, "WASM skills loaded from config");
            }

            // Load markdown skills and tool groups
            let _prompt_injection = load_markdown_skills(&config, &config_dir, &mut registry).await;
            load_tool_groups(&config, &mut registry);

            // Connect to MCP servers and register their tools as skills
            let mcp_manager = Arc::new(agentor_mcp::McpServerManager::new());
            if !config.mcp_servers.is_empty() {
                let errors = mcp_manager
                    .connect_all(&config.mcp_servers, &mut registry)
                    .await;
                if !errors.is_empty() {
                    tracing::warn!(
                        failed = errors.len(),
                        "Some MCP servers failed to connect"
                    );
                }
                // Start background health check loop (60s default)
                let mgr = mcp_manager.clone();
                mgr.start_health_loop(std::time::Duration::from_secs(60));
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

            let app = GatewayServer::build_with_middleware(
                agent,
                sessions,
                Some(rate_limiter),
                auth_config,
            );

            let addr = format!("{}:{}", host, port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;

            if config.server.tls.enabled {
                // TLS/mTLS mode
                let tls_config = agentor_security::tls::TlsConfig {
                    enabled: true,
                    cert_path: config.server.tls.cert_path.clone(),
                    key_path: config.server.tls.key_path.clone(),
                    client_ca_path: config.server.tls.client_ca_path.clone(),
                };
                tls::validate_tls_config(&tls_config).await?;
                let acceptor = tls::build_tls_acceptor(&tls_config).await?;

                info!("Agentor gateway listening on {} (TLS enabled)", addr);

                // Graceful shutdown for TLS mode
                let shutdown = shutdown_signal();
                tokio::pin!(shutdown);

                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            let (stream, peer_addr) = result?;
                            let acceptor = acceptor.clone();
                            let app = app.clone();
                            tokio::spawn(async move {
                                match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        let io = hyper_util::rt::TokioIo::new(tls_stream);
                                        let svc = hyper_util::service::TowerToHyperService::new(app);
                                        let conn = hyper_util::server::conn::auto::Builder::new(
                                            hyper_util::rt::TokioExecutor::new(),
                                        );
                                        if let Err(e) = conn.serve_connection(io, svc).await {
                                            tracing::error!(
                                                peer = %peer_addr,
                                                error = %e,
                                                "TLS connection error"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            peer = %peer_addr,
                                            error = %e,
                                            "TLS handshake failed"
                                        );
                                    }
                                }
                            });
                        }
                        _ = &mut shutdown => {
                            info!("Shutting down TLS server");
                            break;
                        }
                    }
                }
            } else {
                info!("Agentor gateway listening on {}", addr);
                axum::serve(listener, app)
                    .with_graceful_shutdown(shutdown_signal())
                    .await?;
            }

            info!("Agentor gateway stopped");
        }
        Commands::Skill { action } => match action {
            SkillAction::List => {
                // Load all skills: builtins + config + markdown
                let mut registry = SkillRegistry::new();
                agentor_builtins::register_builtins(&mut registry);
                if !config.skills.is_empty() {
                    let loader = SkillLoader::new()?;
                    let _ = loader.load_all(&config.skills, &config_dir, &mut registry);
                }
                let _ = load_markdown_skills(&config, &config_dir, &mut registry).await;
                load_tool_groups(&config, &mut registry);

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

                // Show tool groups
                let groups = registry.list_groups();
                if !groups.is_empty() {
                    println!("\nTool Groups:");
                    for group in &groups {
                        let available = registry.skills_in_group(&group.name);
                        if group.skills.is_empty() {
                            println!("  {} — {} [all skills]", group.name, group.description);
                        } else {
                            println!(
                                "  {} — {} [{}/{}]",
                                group.name,
                                group.description,
                                available.len(),
                                group.skills.len()
                            );
                        }
                    }
                }
            }
        },
        Commands::Orchestrate {
            task,
            output_dir,
            interactive_approval,
            approval_timeout,
        } => {
            // Initialize security
            let audit = Arc::new(AuditLog::new(config.data_dir.join("audit")));

            // Load skills: builtins + config + markdown
            let mut registry = SkillRegistry::new();
            if interactive_approval {
                let channel = Arc::new(agentor_builtins::StdinApprovalChannel::new(
                    std::time::Duration::from_secs(approval_timeout),
                ));
                agentor_builtins::register_builtins_with_approval(&mut registry, channel);
                info!("Interactive approval enabled (timeout: {}s)", approval_timeout);
            } else {
                agentor_builtins::register_builtins(&mut registry);
            }
            if !config.skills.is_empty() {
                let loader = SkillLoader::new()?;
                let _ = loader.load_all(&config.skills, &config_dir, &mut registry);
            }
            let _prompt_injection = load_markdown_skills(&config, &config_dir, &mut registry).await;
            load_tool_groups(&config, &mut registry);

            // Build permissions
            let mut permissions = PermissionSet::new();
            for desc in registry.list_descriptors() {
                for cap in &desc.required_capabilities {
                    permissions.grant(cap.clone());
                }
            }

            let skills = Arc::new(registry);

            // Build compliance hooks for automated event tracking
            let iso27001_module = Arc::new(agentor_compliance::Iso27001Module::new());
            let iso42001_module = Arc::new(agentor_compliance::Iso42001Module::new());
            let system_id = uuid::Uuid::new_v4();

            let iso27001_hook = Arc::new(agentor_compliance::Iso27001Hook::new(iso27001_module.clone()));
            let iso42001_hook = Arc::new(agentor_compliance::Iso42001Hook::new(
                iso42001_module.clone(),
                system_id,
            ));

            let mut compliance_chain = agentor_compliance::ComplianceHookChain::new();
            compliance_chain.add(iso27001_hook);
            compliance_chain.add(iso42001_hook);
            let compliance_chain = Arc::new(compliance_chain);

            // Create orchestrator with progress callback, compliance hooks, and optional output dir
            let mut orchestrator =
                agentor_orchestrator::Orchestrator::new(&config.model, skills, permissions, audit);
            orchestrator = orchestrator
                .with_progress(|role, msg| {
                    eprintln!("  [{:>10}] {}", role, msg);
                })
                .with_compliance(compliance_chain.clone());
            if let Some(dir) = output_dir {
                orchestrator = orchestrator.with_output_dir(dir);
            }

            eprintln!("Agentor Multi-Agent Orchestrator");
            eprintln!("================================");
            eprintln!("Task: {}", task);
            eprintln!();

            let start = Instant::now();

            // Run pipeline
            match orchestrator.run(&task).await {
                Ok(result) => {
                    let duration = start.elapsed();
                    eprintln!();
                    eprintln!("{} ({:.1}s)", result.summary, duration.as_secs_f64());

                    // Show written files
                    if !result.written_files.is_empty() {
                        eprintln!();
                        eprintln!("Files written:");
                        for f in &result.written_files {
                            eprintln!("  {}", f);
                        }
                    }

                    eprintln!();

                    // Print artifacts to stdout (pipeable)
                    for artifact in &result.artifacts {
                        let label = match artifact.kind {
                            agentor_orchestrator::ArtifactKind::Spec => "SPEC",
                            agentor_orchestrator::ArtifactKind::Code => "CODE",
                            agentor_orchestrator::ArtifactKind::Test => "TEST",
                            agentor_orchestrator::ArtifactKind::Review => "REVIEW",
                            agentor_orchestrator::ArtifactKind::Report => "REPORT",
                        };
                        println!("=== {} ===", label);
                        println!("{}", artifact.content);
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("Orchestration failed: {}", e);
                    std::process::exit(1);
                }
            }

            // Persist compliance event counts as a summary
            let access_events = iso27001_module.access_event_count().await;
            let transparency_logs = iso42001_module.transparency_log_count().await;
            if access_events > 0 || transparency_logs > 0 {
                eprintln!(
                    "Compliance: {} ISO 27001 access events, {} ISO 42001 transparency logs",
                    access_events, transparency_logs
                );
            }
        }
        Commands::Compliance { action } => match action {
            ComplianceAction::Report => {
                use agentor_compliance::{
                    dpga::{assess_agentor_dpga, DpgaInput},
                    gdpr::GdprModule,
                    iso27001::Iso27001Module,
                    iso42001::Iso42001Module,
                };

                println!("Agentor Compliance Report");
                println!("========================\n");

                // GDPR
                let gdpr = GdprModule::new();
                let gdpr_report = gdpr.assess(true, true, true, false);
                println!("{}:", gdpr_report.framework);
                println!("  Status: {:?}", gdpr_report.status);
                for f in &gdpr_report.findings {
                    let icon = if f.compliant { "+" } else { "-" };
                    println!("  [{}] {}: {}", icon, f.title, f.description);
                    if !f.recommendation.is_empty() {
                        println!("      Recommendation: {}", f.recommendation);
                    }
                }

                println!();

                // ISO 27001
                let iso27001 = Iso27001Module::new();
                let iso_report = iso27001.assess(true, true, true, true, false);
                println!("{}:", iso_report.framework);
                println!("  Status: {:?}", iso_report.status);
                for f in &iso_report.findings {
                    let icon = if f.compliant { "+" } else { "-" };
                    println!("  [{}] {}: {}", icon, f.title, f.description);
                    if !f.recommendation.is_empty() {
                        println!("      Recommendation: {}", f.recommendation);
                    }
                }

                println!();

                // ISO 42001
                let iso42001 = Iso42001Module::new();
                let ai_report = iso42001.assess(true, true, true, true, true);
                println!("{}:", ai_report.framework);
                println!("  Status: {:?}", ai_report.status);
                for f in &ai_report.findings {
                    let icon = if f.compliant { "+" } else { "-" };
                    println!("  [{}] {}: {}", icon, f.title, f.description);
                    if !f.recommendation.is_empty() {
                        println!("      Recommendation: {}", f.recommendation);
                    }
                }

                println!();

                // DPGA
                let dpga_input = DpgaInput {
                    has_open_license: true,
                    has_sdg_docs: true,
                    has_open_data: true,
                    has_privacy: true,
                    has_docs: true,
                    has_open_standards: true,
                    has_governance: false,
                    has_do_no_harm: true,
                    has_interop: true,
                };
                let dpga_report = assess_agentor_dpga(&dpga_input);
                println!("{}:", dpga_report.framework);
                println!("  Status: {:?}", dpga_report.status);
                for f in &dpga_report.findings {
                    let icon = if f.compliant { "+" } else { "-" };
                    println!("  [{}] {}: {}", icon, f.title, f.description);
                    if !f.recommendation.is_empty() {
                        println!("      Recommendation: {}", f.recommendation);
                    }
                }

                println!("\n{}", dpga_report.summary);

                // Persist all reports to disk
                let report_dir = config.data_dir.join("compliance_reports");
                let store = agentor_compliance::JsonReportStore::new(&report_dir);
                let reports = [&gdpr_report, &iso_report, &ai_report, &dpga_report];
                let mut saved = 0;
                for report in &reports {
                    match store.save_report(report).await {
                        Ok(path) => {
                            saved += 1;
                            println!("  Saved: {}", path.display());
                        }
                        Err(e) => {
                            eprintln!("  Failed to save {} report: {}", report.framework, e);
                        }
                    }
                }
                if saved > 0 {
                    println!(
                        "\n{} report(s) saved to {}",
                        saved,
                        report_dir.display()
                    );
                }
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars_known() {
        std::env::set_var("AGENTOR_TEST_VAR", "hello123");
        let result = expand_env_vars("key = \"${AGENTOR_TEST_VAR}\"");
        assert_eq!(result, "key = \"hello123\"");
        std::env::remove_var("AGENTOR_TEST_VAR");
    }

    #[test]
    fn test_expand_env_vars_unknown() {
        let result = expand_env_vars("key = \"${AGENTOR_NONEXISTENT_12345}\"");
        assert_eq!(result, "key = \"\"");
    }

    #[test]
    fn test_expand_env_vars_no_vars() {
        let result = expand_env_vars("plain text without vars");
        assert_eq!(result, "plain text without vars");
    }

    #[test]
    fn test_expand_env_vars_multiple() {
        std::env::set_var("AGENTOR_A", "foo");
        std::env::set_var("AGENTOR_B", "bar");
        let result = expand_env_vars("${AGENTOR_A} and ${AGENTOR_B}");
        assert_eq!(result, "foo and bar");
        std::env::remove_var("AGENTOR_A");
        std::env::remove_var("AGENTOR_B");
    }
}
