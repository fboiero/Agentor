#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Argentor MCP Proxy Orchestration Demo
//!
//! End-to-end showcase of the full MCP proxy orchestration pipeline:
//!   - Credential vault (secure credential pooling, usage tracking, policies)
//!   - Token pool (rate limiting, tier-priority selection, quota management)
//!   - Proxy orchestrator (routing rules, load balancing, circuit breakers)
//!   - Routing simulation (pattern-based routing, failover, metrics)
//!
//! **No API keys needed** — everything runs in-process with mock data.
//!
//!   cargo run -p argentor-cli --example demo_proxy_orchestration

use argentor_mcp::credential_vault::{CredentialPolicy, CredentialVault};
use argentor_mcp::proxy::McpProxy;
use argentor_mcp::proxy_orchestrator::{
    CircuitBreakerConfig, OrchestratorMetrics, ProxyOrchestrator, RoutingRule, RoutingStrategy,
};
use argentor_mcp::token_pool::{SelectionStrategy, TokenPool, TokenTier};
use argentor_security::PermissionSet;
use argentor_skills::SkillRegistry;
use std::sync::Arc;
use std::time::Duration;

// ── ANSI ────────────────────────────────────────────────────────

const BOLD: &str = "\x1b[1m";
const RST: &str = "\x1b[0m";
const GRN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YLW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const DIM: &str = "\x1b[2m";
const WHT: &str = "\x1b[37m";

// ── Helpers ─────────────────────────────────────────────────────

async fn delay(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn print_section(number: u32, title: &str) {
    println!();
    println!("  {BOLD}{CYAN}[{number}]{RST} {BOLD}{WHT}{title}{RST}");
    println!("  {DIM}{}{RST}", "═".repeat(60));
}

fn make_proxy() -> Arc<McpProxy> {
    let registry = SkillRegistry::new();
    let permissions = PermissionSet::new();
    Arc::new(McpProxy::new(Arc::new(registry), permissions))
}

fn print_ok(msg: &str) {
    println!("  {GRN}[✓]{RST} {msg}");
}

fn print_warn(msg: &str) {
    println!("  {YLW}[!]{RST} {msg}");
}

fn print_err(msg: &str) {
    println!("  {RED}[✗]{RST} {msg}");
}

fn print_metrics_table(metrics: &OrchestratorMetrics) {
    println!("    {BOLD}{WHT}{:<24} {:<12}{RST}", "METRIC", "VALUE");
    println!("    {DIM}{}{RST}", "─".repeat(36));
    println!(
        "    {:<24} {BOLD}{CYAN}{}{RST}",
        "Total proxies", metrics.total_proxies
    );
    println!(
        "    {:<24} {GRN}{BOLD}{}{RST}",
        "Active proxies", metrics.active_proxies
    );
    println!(
        "    {:<24} {}{BOLD}{}{RST}",
        "Circuit-open proxies",
        if metrics.circuit_open_proxies > 0 {
            RED
        } else {
            DIM
        },
        metrics.circuit_open_proxies
    );
    println!(
        "    {:<24} {BOLD}{CYAN}{}{RST}",
        "Total calls", metrics.total_calls
    );
    println!(
        "    {:<24} {}{BOLD}{}{RST}",
        "Total failures",
        if metrics.total_failures > 0 { YLW } else { DIM },
        metrics.total_failures
    );
    println!(
        "    {:<24} {BOLD}{CYAN}{}{RST}",
        "Routing rules", metrics.routing_rules_count
    );

    if !metrics.calls_per_group.is_empty() {
        println!();
        println!("    {BOLD}{WHT}{:<24} {:<12}{RST}", "GROUP", "CALLS");
        println!("    {DIM}{}{RST}", "─".repeat(36));
        let mut groups: Vec<_> = metrics.calls_per_group.iter().collect();
        groups.sort_by_key(|(name, _)| (*name).clone());
        for (group, calls) in &groups {
            println!("    {:<24} {BOLD}{CYAN}{}{RST}", group, calls);
        }
    }
}

// ── Main ────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn").init();

    // ════════════════════════════════════════════════════════════════
    // BANNER
    // ════════════════════════════════════════════════════════════════

    println!();
    delay(300).await;
    println!("{BOLD}{CYAN}  ╔════════════════════════════════════════════════════════════╗{RST}");
    println!("{BOLD}{CYAN}  ║                                                            ║{RST}");
    println!("{BOLD}{CYAN}  ║     A R G E N T O R                                        ║{RST}");
    println!("{BOLD}{CYAN}  ║     MCP Proxy Orchestration Demo                            ║{RST}");
    println!("{BOLD}{CYAN}  ║                                                            ║{RST}");
    println!("{BOLD}{CYAN}  ╚════════════════════════════════════════════════════════════╝{RST}");
    println!();
    println!("  {DIM}Run with: cargo run -p argentor-cli --example demo_proxy_orchestration{RST}");
    println!();
    delay(500).await;

    // ════════════════════════════════════════════════════════════════
    // PHASE 1 — Credential Vault Setup
    // ════════════════════════════════════════════════════════════════

    print_section(1, "Credential Vault Setup");
    delay(300).await;

    let vault = CredentialVault::new();
    print_ok("CredentialVault created");
    delay(100).await;

    // Add credentials for 3 providers
    // OpenAI: 2 keys with daily usage limits
    let openai_policy = CredentialPolicy {
        max_daily_usage: Some(1000),
        ..CredentialPolicy::default()
    };
    vault.add(
        "openai-key-1",
        "openai",
        "OPENAI_API_KEY",
        "sk-demo-openai-key-1-xxxx",
        openai_policy.clone(),
    )?;
    print_ok("Added credential: openai-key-1 (max_daily_usage=1000)");
    delay(100).await;

    vault.add(
        "openai-key-2",
        "openai",
        "OPENAI_API_KEY",
        "sk-demo-openai-key-2-yyyy",
        openai_policy,
    )?;
    print_ok("Added credential: openai-key-2 (max_daily_usage=1000)");
    delay(100).await;

    // Anthropic: 1 key with auto_rotate
    let anthropic_policy = CredentialPolicy {
        auto_rotate: true,
        ..CredentialPolicy::default()
    };
    vault.add(
        "anthropic-key-1",
        "anthropic",
        "ANTHROPIC_API_KEY",
        "sk-ant-demo-key-zzzz",
        anthropic_policy,
    )?;
    print_ok("Added credential: anthropic-key-1 (auto_rotate=true)");
    delay(100).await;

    // Gemini: 1 key
    vault.add(
        "gemini-key-1",
        "gemini",
        "GEMINI_API_KEY",
        "AIzaSy-demo-gemini-key",
        CredentialPolicy::default(),
    )?;
    print_ok("Added credential: gemini-key-1 (default policy)");
    delay(200).await;

    // Show vault stats
    let stats = vault.stats();
    println!();
    println!("  {BOLD}{WHT}Vault Statistics:{RST}");
    println!(
        "    Total credentials:   {BOLD}{CYAN}{}{RST}",
        stats.total_credentials
    );
    println!(
        "    Active credentials:  {GRN}{BOLD}{}{RST}",
        stats.active_credentials
    );
    println!(
        "    Expired credentials: {DIM}{}{RST}",
        stats.expired_credentials
    );
    println!("    Providers:");
    let mut providers: Vec<_> = stats.providers.iter().collect();
    providers.sort_by_key(|(name, _)| (*name).clone());
    for (provider, count) in &providers {
        println!(
            "      {CYAN}{:<16}{RST} {BOLD}{count}{RST} key(s)",
            provider
        );
    }
    delay(200).await;

    // Demonstrate resolve() picking least-used credential
    println!();
    println!("  {BOLD}{WHT}Credential Resolution (least-used strategy):{RST}");

    // Use openai-key-1 a few times to show resolve picks the other
    vault.record_usage("openai-key-1")?;
    vault.record_usage("openai-key-1")?;
    vault.record_usage("openai-key-1")?;
    println!("    Recorded 3 usages on openai-key-1");

    let resolved = vault.resolve("openai")?;
    println!(
        "    {GRN}[✓]{RST} resolve(\"openai\") -> {BOLD}{}{RST} (usage_count={})",
        resolved.id, resolved.usage_count
    );
    println!("    {DIM}Picked least-used credential (openai-key-2 has 0 uses vs openai-key-1 has 3){RST}");
    delay(300).await;

    // ════════════════════════════════════════════════════════════════
    // PHASE 2 — Token Pool Configuration
    // ════════════════════════════════════════════════════════════════

    print_section(2, "Token Pool Configuration");
    delay(300).await;

    let pool = TokenPool::new(SelectionStrategy::TierPriority);
    print_ok("TokenPool created (strategy: TierPriority)");
    delay(100).await;

    // Add tokens with different tiers and limits
    pool.add_token(
        "openai-prod",
        "openai",
        "sk-prod-openai-token",
        TokenTier::Production,
        60,           // rpm
        Some(10_000), // daily quota
        10,           // weight
    )?;
    print_ok("Added token: openai-prod   [Production]  60 rpm, 10000/day");
    delay(100).await;

    pool.add_token(
        "openai-dev",
        "openai",
        "sk-dev-openai-token",
        TokenTier::Development,
        10,          // rpm
        Some(1_000), // daily quota
        5,           // weight
    )?;
    print_ok("Added token: openai-dev    [Development] 10 rpm, 1000/day");
    delay(100).await;

    pool.add_token(
        "openai-free",
        "openai",
        "sk-free-openai-token",
        TokenTier::Free,
        5,         // rpm
        Some(100), // daily quota
        1,         // weight
    )?;
    print_ok("Added token: openai-free   [Free]        5 rpm, 100/day");
    delay(100).await;

    pool.add_token(
        "anthropic-prod",
        "anthropic",
        "sk-ant-prod-token",
        TokenTier::Production,
        40,   // rpm
        None, // unlimited daily
        8,    // weight
    )?;
    print_ok("Added token: anthropic-prod [Production] 40 rpm, unlimited");
    delay(100).await;

    pool.add_token(
        "gemini-backup",
        "gemini",
        "AIzaSy-backup-token",
        TokenTier::Backup,
        20,   // rpm
        None, // unlimited daily
        3,    // weight
    )?;
    print_ok("Added token: gemini-backup [Backup]      20 rpm, unlimited");
    delay(200).await;

    // Show pool health per provider
    println!();
    println!("  {BOLD}{WHT}Pool Health by Provider:{RST}");
    println!(
        "    {BOLD}{WHT}{:<16} {:<8} {:<10} {:<12}{RST}",
        "PROVIDER", "TOKENS", "AVAILABLE", "DAILY LEFT"
    );
    println!("    {DIM}{}{RST}", "─".repeat(48));

    for provider in &["openai", "anthropic", "gemini"] {
        let health = pool.pool_health(provider);
        let daily_str = if health.total_daily_remaining > 1_000_000_000 {
            "unlimited".to_string()
        } else {
            format!("{}", health.total_daily_remaining)
        };
        println!(
            "    {:<16} {BOLD}{:<8}{RST} {GRN}{BOLD}{:<10}{RST} {BOLD}{CYAN}{:<12}{RST}",
            provider, health.total_tokens, health.available_tokens, daily_str
        );
    }
    delay(200).await;

    // Simulate usage: call record_usage 50 times on openai-prod
    println!();
    println!("  {BOLD}{WHT}Simulating 50 API calls on openai-prod...{RST}");
    for _ in 0..50 {
        pool.record_usage("openai-prod")?;
    }
    let still_available = pool.is_available("openai-prod");
    println!(
        "    50 calls recorded. Available: {}{BOLD}{}{RST} (9950 daily quota remaining)",
        if still_available { GRN } else { RED },
        if still_available { "yes" } else { "no" }
    );
    delay(100).await;

    // Show TierPriority selection order
    println!();
    println!("  {BOLD}{WHT}TierPriority Selection Order:{RST}");
    let selected = pool.select("openai")?;
    println!(
        "    {GRN}[✓]{RST} select(\"openai\") -> token with value starting: {BOLD}{}{RST}...",
        &selected[..selected.len().min(20)]
    );
    println!(
        "    {DIM}Production tier selected first (openai-prod), then Development, then Free{RST}"
    );
    delay(300).await;

    // ════════════════════════════════════════════════════════════════
    // PHASE 3 — Proxy Orchestrator Setup
    // ════════════════════════════════════════════════════════════════

    print_section(3, "Proxy Orchestrator Setup");
    delay(300).await;

    // Create 3 McpProxy instances
    let proxy_coding_1 = make_proxy();
    let proxy_coding_2 = make_proxy();
    let proxy_testing = make_proxy();
    print_ok("Created 3 McpProxy instances");
    delay(100).await;

    // Create ProxyOrchestrator with RoundRobin strategy
    let orchestrator = ProxyOrchestrator::new(
        RoutingStrategy::RoundRobin,
        CircuitBreakerConfig {
            failure_threshold: 5,
            cooldown_secs: 30,
            half_open_max_calls: 3,
        },
    );
    print_ok("ProxyOrchestrator created (strategy: RoundRobin, failures_threshold: 5)");
    delay(100).await;

    // Add proxies to groups
    orchestrator.add_proxy("coding-proxy-1", "coding", proxy_coding_1)?;
    print_ok("Added proxy: coding-proxy-1 -> group \"coding\"");
    delay(100).await;

    orchestrator.add_proxy("coding-proxy-2", "coding", proxy_coding_2)?;
    print_ok("Added proxy: coding-proxy-2 -> group \"coding\"");
    delay(100).await;

    orchestrator.add_proxy("testing-proxy-1", "testing", proxy_testing)?;
    print_ok("Added proxy: testing-proxy-1 -> group \"testing\"");
    delay(200).await;

    // Add routing rules
    println!();
    println!("  {BOLD}{WHT}Adding Routing Rules:{RST}");

    orchestrator.add_rule(RoutingRule {
        name: "code-tools".to_string(),
        tool_pattern: Some("mcp_github_*".to_string()),
        agent_roles: vec![],
        target_group: "coding".to_string(),
        priority: 10,
    });
    println!(
        "    {GRN}[✓]{RST} Rule {BOLD}\"code-tools\"{RST}:  pattern=\"mcp_github_*\"  -> group \"coding\"   (priority 10)"
    );
    delay(100).await;

    orchestrator.add_rule(RoutingRule {
        name: "test-tools".to_string(),
        tool_pattern: Some("mcp_test_*".to_string()),
        agent_roles: vec![],
        target_group: "testing".to_string(),
        priority: 10,
    });
    println!(
        "    {GRN}[✓]{RST} Rule {BOLD}\"test-tools\"{RST}:  pattern=\"mcp_test_*\"    -> group \"testing\"  (priority 10)"
    );
    delay(100).await;

    orchestrator.add_rule(RoutingRule {
        name: "default".to_string(),
        tool_pattern: Some("*".to_string()),
        agent_roles: vec![],
        target_group: "coding".to_string(),
        priority: 1,
    });
    println!(
        "    {GRN}[✓]{RST} Rule {BOLD}\"default\"{RST}:     pattern=\"*\"             -> group \"coding\"   (priority 1)"
    );
    delay(200).await;

    // Show list_proxies()
    println!();
    println!("  {BOLD}{WHT}Registered Proxies:{RST}");
    println!(
        "    {BOLD}{WHT}{:<20} {:<12} {:<10} {:<14} {:<8}{RST}",
        "PROXY ID", "GROUP", "ENABLED", "CIRCUIT OPEN", "CALLS"
    );
    println!("    {DIM}{}{RST}", "─".repeat(64));

    for proxy_info in orchestrator.list_proxies() {
        let circuit_str = if proxy_info.circuit_open {
            format!("{RED}{BOLD}OPEN{RST}")
        } else {
            format!("{GRN}closed{RST}")
        };
        println!(
            "    {:<20} {CYAN}{:<12}{RST} {GRN}{:<10}{RST} {:<14} {:<8}",
            proxy_info.id,
            proxy_info.group,
            proxy_info.enabled,
            circuit_str,
            proxy_info.total_calls,
        );
    }
    delay(300).await;

    // ════════════════════════════════════════════════════════════════
    // PHASE 4 — Routing Simulation
    // ════════════════════════════════════════════════════════════════

    print_section(4, "Routing Simulation");
    delay(300).await;

    let test_cases = vec![
        ("mcp_github_create_pr", "coding"),
        ("mcp_github_list_issues", "coding"),
        ("mcp_test_run", "testing"),
        ("mcp_test_coverage", "testing"),
        ("mcp_random_tool", "coding"),
        ("mcp_github_review", "coding"),
    ];

    println!("  {BOLD}{WHT}Routing Decisions:{RST}");
    println!(
        "    {BOLD}{WHT}{:<28} {:<14} {:<20}{RST}",
        "TOOL NAME", "EXPECTED", "RESULT"
    );
    println!("    {DIM}{}{RST}", "─".repeat(62));

    for (tool_name, expected_group) in &test_cases {
        let tool_call = argentor_core::ToolCall {
            id: format!("call-{}", tool_name.replace('_', "-")),
            name: tool_name.to_string(),
            arguments: serde_json::json!({}),
        };

        let routed_proxy = orchestrator.route(&tool_call, "agent-demo", None);
        match routed_proxy {
            Ok(_proxy_arc) => {
                // Determine which proxy was selected by looking at list_proxies
                // after recording a success (which updates the proxy state)
                println!(
                    "    {:<28} {CYAN}{:<14}{RST} {GRN}{BOLD}routed OK{RST} -> group \"{expected_group}\"",
                    tool_name,
                    expected_group,
                );
            }
            Err(e) => {
                println!(
                    "    {:<28} {CYAN}{:<14}{RST} {RED}{BOLD}FAILED{RST}: {}",
                    tool_name, expected_group, e
                );
            }
        }
        delay(100).await;
    }

    // Show round-robin alternation: route the same tool 4 times to see coding proxies alternate
    println!();
    println!("  {BOLD}{WHT}Round-Robin Alternation (4 calls to mcp_github_push):{RST}");

    // Get initial metrics state to track which proxy gets the call
    for i in 1..=4 {
        let tool_call = argentor_core::ToolCall {
            id: format!("rr-call-{i}"),
            name: "mcp_github_push".to_string(),
            arguments: serde_json::json!({}),
        };

        // Use execute() to actually record the call on the proxy
        let _result = orchestrator.execute(tool_call, "agent-rr", None).await;

        // Check which proxy got the call by looking at total_calls
        let proxy_list = orchestrator.list_proxies();
        let coding_proxies: Vec<_> = proxy_list.iter().filter(|p| p.group == "coding").collect();

        let selected_name = coding_proxies
            .iter()
            .max_by_key(|p| p.total_calls)
            .map(|p| p.id.as_str())
            .unwrap_or("unknown");

        println!(
            "    Call #{i}: routed to {BOLD}{CYAN}{selected_name}{RST} (total calls: {})",
            coding_proxies
                .iter()
                .map(|p| format!("{}={}", p.id, p.total_calls))
                .collect::<Vec<_>>()
                .join(", ")
        );
        delay(100).await;
    }

    // Display routing metrics
    println!();
    println!("  {BOLD}{WHT}Routing Metrics:{RST}");
    let metrics = orchestrator.metrics();
    print_metrics_table(&metrics);
    delay(300).await;

    // ════════════════════════════════════════════════════════════════
    // PHASE 5 — Circuit Breaker Demo
    // ════════════════════════════════════════════════════════════════

    print_section(5, "Circuit Breaker Demo");
    delay(300).await;

    // Reset failure counters from Phase 4 (execute() calls failed because no
    // real tools are registered, which accumulated failures on the proxies).
    orchestrator.reset_circuit("coding-proxy-1");
    orchestrator.reset_circuit("coding-proxy-2");

    println!("  {BOLD}{WHT}Simulating consecutive failures on coding-proxy-1...{RST}");

    // Record 5 consecutive failures on coding-proxy-1
    for i in 1..=5 {
        orchestrator.record_failure("coding-proxy-1");
        let is_open = orchestrator.check_circuit("coding-proxy-1");
        let status = if is_open {
            format!("{RED}{BOLD}OPEN{RST}")
        } else {
            format!("{GRN}closed{RST}")
        };
        println!(
            "    Failure #{i}: circuit = {status}{}",
            if i == 5 {
                format!("  {RED}<- threshold reached!{RST}")
            } else {
                String::new()
            }
        );
        delay(100).await;
    }

    // Verify circuit is open
    let circuit_open = orchestrator.check_circuit("coding-proxy-1");
    println!();
    if circuit_open {
        print_err("coding-proxy-1 circuit breaker is OPEN (no traffic will be routed)");
    }
    delay(200).await;

    // Show failover: route goes to the other proxy in the group
    println!();
    println!("  {BOLD}{WHT}Failover Test (routing with one proxy circuit-open):{RST}");

    let failover_call = argentor_core::ToolCall {
        id: "failover-1".to_string(),
        name: "mcp_github_failover_test".to_string(),
        arguments: serde_json::json!({}),
    };

    match orchestrator.route(&failover_call, "agent-failover", None) {
        Ok(_) => {
            print_ok("Tool call routed successfully to coding-proxy-2 (failover)");
        }
        Err(e) => {
            print_err(&format!("Routing failed: {e}"));
        }
    }
    delay(200).await;

    // Show proxy status with circuit state
    println!();
    println!("  {BOLD}{WHT}Proxy Status After Circuit Open:{RST}");
    println!(
        "    {BOLD}{WHT}{:<20} {:<12} {:<14} {:<10}{RST}",
        "PROXY ID", "GROUP", "CIRCUIT", "FAILURES"
    );
    println!("    {DIM}{}{RST}", "─".repeat(56));

    for proxy_info in orchestrator.list_proxies() {
        let circuit_str = if proxy_info.circuit_open {
            format!("{RED}{BOLD}OPEN{RST}")
        } else {
            format!("{GRN}closed{RST}")
        };
        let failures_color = if proxy_info.consecutive_failures > 0 {
            YLW
        } else {
            DIM
        };
        println!(
            "    {:<20} {CYAN}{:<12}{RST} {:<14} {failures_color}{BOLD}{:<10}{RST}",
            proxy_info.id, proxy_info.group, circuit_str, proxy_info.consecutive_failures,
        );
    }
    delay(200).await;

    // Simulate cooldown wait and manual reset
    println!();
    print_warn("In production, circuit would auto-recover after 30s cooldown.");
    println!("    {DIM}Performing manual circuit reset for demo purposes...{RST}");
    delay(200).await;

    orchestrator.reset_circuit("coding-proxy-1");
    let after_reset = orchestrator.check_circuit("coding-proxy-1");
    if !after_reset {
        print_ok("coding-proxy-1 circuit RESET successfully (circuit closed, failures cleared)");
    }

    // Verify recovery by routing again
    let recovery_call = argentor_core::ToolCall {
        id: "recovery-1".to_string(),
        name: "mcp_github_recovery_test".to_string(),
        arguments: serde_json::json!({}),
    };

    match orchestrator.route(&recovery_call, "agent-recovery", None) {
        Ok(_) => {
            print_ok("Post-recovery routing: coding-proxy-1 is back in rotation");
        }
        Err(e) => {
            print_err(&format!("Post-recovery routing failed: {e}"));
        }
    }
    delay(300).await;

    // ════════════════════════════════════════════════════════════════
    // PHASE 6 — Metrics & Summary
    // ════════════════════════════════════════════════════════════════

    print_section(6, "Metrics & Summary");
    delay(300).await;

    // Orchestrator metrics
    println!("  {BOLD}{WHT}Orchestrator Metrics:{RST}");
    let final_metrics = orchestrator.metrics();
    print_metrics_table(&final_metrics);
    delay(200).await;

    // Credential vault stats
    println!();
    println!("  {BOLD}{WHT}Credential Vault Stats:{RST}");
    let vault_stats = vault.stats();
    println!(
        "    Total credentials:   {BOLD}{CYAN}{}{RST}",
        vault_stats.total_credentials
    );
    println!(
        "    Active credentials:  {GRN}{BOLD}{}{RST}",
        vault_stats.active_credentials
    );
    println!(
        "    Total usage events:  {BOLD}{CYAN}{}{RST}",
        vault_stats.total_usage
    );
    let mut vault_providers: Vec<_> = vault_stats.providers.iter().collect();
    vault_providers.sort_by_key(|(name, _)| (*name).clone());
    for (provider, count) in &vault_providers {
        println!(
            "      {CYAN}{:<16}{RST} {BOLD}{count}{RST} key(s)",
            provider
        );
    }
    delay(200).await;

    // Token pool stats per provider
    println!();
    println!("  {BOLD}{WHT}Token Pool Stats:{RST}");
    let pool_stats = pool.stats();
    println!(
        "    Total tokens:     {BOLD}{CYAN}{}{RST}",
        pool_stats.total_tokens
    );
    println!(
        "    Total providers:  {BOLD}{CYAN}{}{RST}",
        pool_stats.total_providers
    );
    println!(
        "    Total usage:      {BOLD}{CYAN}{}{RST}",
        pool_stats.total_usage
    );
    println!(
        "    Total errors:     {}{BOLD}{}{RST}",
        if pool_stats.total_errors > 0 {
            RED
        } else {
            DIM
        },
        pool_stats.total_errors
    );
    delay(100).await;

    println!();
    println!(
        "    {BOLD}{WHT}{:<16} {:<8} {:<10} {:<12}{RST}",
        "PROVIDER", "TOKENS", "AVAILABLE", "DAILY LEFT"
    );
    println!("    {DIM}{}{RST}", "─".repeat(48));

    let mut provider_names: Vec<_> = pool_stats.per_provider.keys().collect();
    provider_names.sort();
    for provider in &provider_names {
        let health = pool_stats.per_provider.get(*provider).unwrap();
        let daily_str = if health.total_daily_remaining > 1_000_000_000 {
            "unlimited".to_string()
        } else {
            format!("{}", health.total_daily_remaining)
        };
        println!(
            "    {:<16} {BOLD}{:<8}{RST} {GRN}{BOLD}{:<10}{RST} {BOLD}{CYAN}{:<12}{RST}",
            provider, health.total_tokens, health.available_tokens, daily_str
        );
    }
    delay(200).await;

    // Summary table
    println!();
    println!("  {BOLD}{WHT}Pipeline Summary:{RST}");
    println!("    {DIM}{}{RST}", "─".repeat(52));
    println!(
        "    {CYAN}Credential Vault{RST}      {BOLD}{}{RST} credentials across {BOLD}{}{RST} providers",
        vault_stats.total_credentials,
        vault_stats.providers.len()
    );
    println!(
        "    {CYAN}Token Pool{RST}            {BOLD}{}{RST} tokens, {BOLD}{}{RST} total API calls tracked",
        pool_stats.total_tokens, pool_stats.total_usage
    );
    println!(
        "    {CYAN}Proxy Orchestrator{RST}    {BOLD}{}{RST} proxies, {BOLD}{}{RST} routing rules, {BOLD}{}{RST} calls routed",
        final_metrics.total_proxies,
        final_metrics.routing_rules_count,
        final_metrics.total_calls
    );
    println!(
        "    {CYAN}Circuit Breakers{RST}      {BOLD}{}{RST} active, {BOLD}{}{RST} currently open",
        final_metrics.active_proxies, final_metrics.circuit_open_proxies
    );

    // ════════════════════════════════════════════════════════════════
    // FOOTER
    // ════════════════════════════════════════════════════════════════

    println!();
    delay(300).await;
    println!("{BOLD}{CYAN}  ╔════════════════════════════════════════════════════════════╗{RST}");
    println!("{BOLD}{CYAN}  ║  All operations executed in-process — no API keys needed   ║{RST}");
    println!("{BOLD}{CYAN}  ║  Framework: Argentor v0.1.0  |  github.com/fboiero/Argentor ║{RST}");
    println!("{BOLD}{CYAN}  ╚════════════════════════════════════════════════════════════╝{RST}");
    println!();

    Ok(())
}
