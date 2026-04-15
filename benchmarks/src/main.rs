//! CLI for running benchmarks with N-sample statistical aggregation.
//!
//! Examples:
//! ```bash
//! # List discovered tasks
//! cargo run -p argentor-benchmarks -- list
//!
//! # Run N samples on each (task, runner) combo
//! cargo run -p argentor-benchmarks --release -- run-all \
//!   --runners argentor,langchain --samples 10
//! ```

use anyhow::Context;
use argentor_benchmarks::metrics::cost::{self as cost_metric, Scale};
use argentor_benchmarks::metrics::{self, compute_block_rate, BlockRateMetric, PairedTTest, Stats};
use argentor_benchmarks::report::RunReport;
use argentor_benchmarks::runners::{ArgentorRunner, ExternalRunner, MockRunner, Runner, RunnerKind};
use argentor_benchmarks::task::{Task, TaskKind, TaskResult};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Argentor benchmark harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Tasks directory (default: ./benchmarks/tasks)
    #[arg(long, global = true, default_value = "benchmarks/tasks")]
    tasks_dir: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// List discovered tasks
    List,
    /// Run a specific task
    Run {
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "argentor")]
        runner: RunnerArg,
        #[arg(long, default_value_t = false)]
        intelligence: bool,
    },
    /// Run all discovered tasks on all enabled runners
    RunAll {
        #[arg(long, value_delimiter = ',', default_value = "argentor,mock")]
        runners: Vec<RunnerArg>,
        /// Number of samples per (task, runner) pair. Default 1 for quick dev,
        /// use 10+ for statistically meaningful reports.
        #[arg(long, default_value_t = 1)]
        samples: usize,
    },
    /// Run security-track only: discover tasks with `kind: security` and
    /// compute block-rate / precision / recall / F1 per runner.
    Security {
        #[arg(long, value_delimiter = ',', default_value = "argentor,langchain,crewai,pydantic-ai,claude-agent-sdk")]
        runners: Vec<RunnerArg>,
        /// Number of samples per (task, runner) pair.
        #[arg(long, default_value_t = 1)]
        samples: usize,
    },
    /// Run cost-track only: discover `kind: cost` tasks and compute
    /// prompt-tokens-sent + dollar-cost per runner + scale projections.
    Cost {
        #[arg(long, value_delimiter = ',', default_value = "argentor,langchain,crewai,pydantic-ai,claude-agent-sdk")]
        runners: Vec<RunnerArg>,
        /// Number of samples per (task, runner) pair. Cost simulation is
        /// deterministic so 1 is plenty — higher values validate consistency.
        #[arg(long, default_value_t = 1)]
        samples: usize,
        /// Workload scale for dollar projections (small | mid | large | enterprise).
        #[arg(long, default_value = "mid")]
        scale: String,
        /// Pricing model (used for $/task, $/day, $/month, $/year).
        #[arg(long, default_value = "claude-sonnet-4")]
        pricing_model: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum RunnerArg {
    Argentor,
    Langchain,
    Crewai,
    PydanticAi,
    ClaudeAgentSdk,
    Mock,
}

impl RunnerArg {
    /// Whether this runner is Argentor (used to flip intelligence on for cost).
    fn is_argentor(&self) -> bool {
        matches!(self, RunnerArg::Argentor)
    }

    #[allow(dead_code)]
    fn kind(&self) -> RunnerKind {
        match self {
            RunnerArg::Argentor => RunnerKind::Argentor,
            RunnerArg::Langchain => RunnerKind::Langchain,
            RunnerArg::Crewai => RunnerKind::Crewai,
            RunnerArg::PydanticAi => RunnerKind::PydanticAi,
            RunnerArg::ClaudeAgentSdk => RunnerKind::ClaudeAgentSdk,
            RunnerArg::Mock => RunnerKind::Mock,
        }
    }

    fn build(&self, intelligence: bool) -> Box<dyn Runner> {
        match self {
            RunnerArg::Argentor => {
                let r = ArgentorRunner::new();
                if intelligence {
                    Box::new(r.with_intelligence())
                } else {
                    Box::new(r)
                }
            }
            RunnerArg::Langchain => {
                let cmd = std::env::var("ARGENTOR_LC_RUNNER")
                    .unwrap_or_else(|_| "argentor-lc-runner".to_string());
                Box::new(ExternalRunner::new(
                    cmd,
                    RunnerKind::Langchain,
                    "langchain v0.3 (mock-llm)",
                ))
            }
            RunnerArg::Crewai => {
                let cmd = std::env::var("ARGENTOR_CREWAI_RUNNER")
                    .unwrap_or_else(|_| "argentor-crewai-runner".to_string());
                Box::new(ExternalRunner::new(
                    cmd,
                    RunnerKind::Crewai,
                    "crewai v0.100 (mock-llm)",
                ))
            }
            RunnerArg::PydanticAi => {
                let cmd = std::env::var("ARGENTOR_PYDANTIC_AI_RUNNER")
                    .unwrap_or_else(|_| "argentor-pydantic-ai-runner".to_string());
                Box::new(ExternalRunner::new(
                    cmd,
                    RunnerKind::PydanticAi,
                    "pydantic-ai v0.5 (mock-llm)",
                ))
            }
            RunnerArg::ClaudeAgentSdk => {
                let cmd = std::env::var("ARGENTOR_CLAUDE_AGENT_SDK_RUNNER")
                    .unwrap_or_else(|_| "argentor-claude-agent-sdk-runner".to_string());
                Box::new(ExternalRunner::new(
                    cmd,
                    RunnerKind::ClaudeAgentSdk,
                    "claude-agent-sdk v0.2 (mock-llm)",
                ))
            }
            RunnerArg::Mock => Box::new(MockRunner::new()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List => {
            let tasks = Task::discover(&cli.tasks_dir)
                .with_context(|| format!("discovering tasks in {:?}", cli.tasks_dir))?;
            if tasks.is_empty() {
                println!("No tasks found in {:?}", cli.tasks_dir);
            } else {
                println!("Discovered {} tasks:", tasks.len());
                for (t, _) in &tasks {
                    println!("  {:<24} — {}", t.id, t.description);
                }
            }
        }
        Command::Run {
            task,
            runner,
            intelligence,
        } => {
            let task_yaml = cli.tasks_dir.join(&task).join("task.yaml");
            let (t, dir) = Task::load_yaml(&task_yaml)
                .with_context(|| format!("loading {:?}", task_yaml))?;
            let r = runner.build(intelligence);
            println!("Running {} on {}", t.id, r.name());
            let result = r.run(&t, &dir).await?;
            let m = metrics::compute(&t, &result);
            let report = RunReport::new(vec![m]);
            println!("\n{}", report.to_markdown());
        }
        Command::RunAll { runners, samples } => {
            let tasks = Task::discover(&cli.tasks_dir)?;
            if tasks.is_empty() {
                anyhow::bail!("no tasks found in {:?}", cli.tasks_dir);
            }

            println!(
                "Running {} tasks × {} runners × {} samples = {} total runs",
                tasks.len(),
                runners.len(),
                samples,
                tasks.len() * runners.len() * samples
            );

            let mut all_metrics = Vec::new();
            let mut latency_by_combo: HashMap<(String, String), Vec<f64>> = HashMap::new();

            for (task, dir) in &tasks {
                for r_arg in &runners {
                    let runner_box = r_arg.build(false);
                    let runner_name = runner_box.name();
                    println!("▶ {}  [{}] × {}", task.id, runner_name, samples);

                    for sample_idx in 0..samples {
                        let r = r_arg.build(false);
                        let result = r.run(task, dir).await?;
                        let m = metrics::compute(task, &result);

                        latency_by_combo
                            .entry((task.id.clone(), runner_name.clone()))
                            .or_default()
                            .push(m.latency.wall_ms as f64);

                        if sample_idx == 0 {
                            all_metrics.push(m);
                        }
                    }
                }
            }

            let report = RunReport::new(all_metrics);
            println!("\n{}", report.to_markdown());

            if samples > 1 {
                println!("\n## Latency stats (N={samples})\n");
                println!(
                    "| Task | Runner | Mean | Median | Stddev | Min | Max | P95 | P99 |"
                );
                println!(
                    "|------|--------|------|--------|--------|-----|-----|-----|-----|"
                );
                let mut keys: Vec<_> = latency_by_combo.keys().collect();
                keys.sort();
                for (task_id, runner_name) in keys {
                    let samples = &latency_by_combo[&(task_id.clone(), runner_name.clone())];
                    let s = Stats::from_samples(samples);
                    println!(
                        "| `{}` | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} |",
                        task_id,
                        runner_name,
                        s.mean,
                        s.median,
                        s.stddev,
                        s.min,
                        s.max,
                        s.p95,
                        s.p99,
                    );
                }

                // Paired t-tests: Argentor vs each other runner, per task
                let argentor_samples: HashMap<String, &Vec<f64>> = latency_by_combo
                    .iter()
                    .filter(|((_, r), _)| r.starts_with("argentor"))
                    .map(|((t, _), v)| (t.clone(), v))
                    .collect();

                if !argentor_samples.is_empty() {
                    println!("\n## Paired t-test (Argentor vs competitors)\n");
                    println!(
                        "| Task | Competitor | N | Argentor mean | Competitor mean | Diff | p-value | Signif | Effect |"
                    );
                    println!(
                        "|------|------------|---|---------------|-----------------|------|---------|--------|--------|"
                    );
                    for ((task_id, runner_name), samples) in &latency_by_combo {
                        if runner_name.starts_with("argentor") {
                            continue;
                        }
                        if let Some(ag_samples) = argentor_samples.get(task_id) {
                            if let Some(t) = PairedTTest::compute(ag_samples, samples) {
                                let sig = if t.is_significant() { "✓" } else { "✗" };
                                println!(
                                    "| `{}` | {} | {} | {:.1} | {:.1} | {:+.1} | {:.4} | {} | {} |",
                                    task_id,
                                    runner_name,
                                    t.n,
                                    ag_samples.iter().sum::<f64>() / t.n as f64,
                                    samples.iter().sum::<f64>() / t.n as f64,
                                    t.mean_diff,
                                    t.p_value,
                                    sig,
                                    t.effect_label(),
                                );
                            }
                        }
                    }
                }
            }

            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let out = cli
                .tasks_dir
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("results")
                .join(format!("run_{ts}.json"));
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            // Flatten tuple keys into strings so JSON serialization works
            let flat_samples: serde_json::Map<String, serde_json::Value> = latency_by_combo
                .iter()
                .map(|((task_id, runner_name), samples)| {
                    (
                        format!("{task_id} :: {runner_name}"),
                        serde_json::to_value(samples).unwrap_or(serde_json::Value::Null),
                    )
                })
                .collect();
            let payload = serde_json::json!({
                "summary": report,
                "samples_per_combo": samples,
                "latency_samples_ms": flat_samples,
            });
            std::fs::write(&out, serde_json::to_string_pretty(&payload)?)?;
            println!("\nResults written to {}", out.display());
        }
        Command::Security { runners, samples } => {
            run_security(&cli.tasks_dir, &runners, samples).await?;
        }
        Command::Cost {
            runners,
            samples,
            scale,
            pricing_model,
        } => {
            run_cost(&cli.tasks_dir, &runners, samples, &scale, &pricing_model).await?;
        }
    }

    Ok(())
}

/// Security-track runner. Discovers security tasks (kind == Security) and
/// computes block-rate / precision / recall / F1 per runner.
async fn run_security(
    tasks_dir: &std::path::Path,
    runners: &[RunnerArg],
    samples: usize,
) -> anyhow::Result<()> {
    let all_tasks = Task::discover(tasks_dir)
        .with_context(|| format!("discovering tasks in {tasks_dir:?}"))?;
    let security_tasks: Vec<_> = all_tasks
        .into_iter()
        .filter(|(t, _)| t.kind == TaskKind::Security)
        .collect();

    if security_tasks.is_empty() {
        anyhow::bail!(
            "no security tasks found in {:?} (looking for kind: security)",
            tasks_dir
        );
    }

    println!(
        "Running {} security tasks × {} runners × {} samples = {} total runs",
        security_tasks.len(),
        runners.len(),
        samples,
        security_tasks.len() * runners.len() * samples
    );

    // Collect all raw results per runner (across all samples) so we can
    // compute the aggregate BlockRateMetric afterwards.
    let mut results_by_runner: HashMap<String, Vec<TaskResult>> = HashMap::new();
    // Track which category each task falls into for the per-category breakdown.
    let mut category_of: HashMap<String, &'static str> = HashMap::new();

    for (task, dir) in &security_tasks {
        let category = if task.id.starts_with("sec_inj_") {
            "injection"
        } else if task.id.starts_with("sec_pii_") {
            "pii"
        } else if task.id.starts_with("sec_cmd_") {
            "command"
        } else {
            "other"
        };
        category_of.insert(task.id.clone(), category);

        for r_arg in runners {
            let runner_box = r_arg.build(false);
            let runner_name = runner_box.name();
            println!("▶ {}  [{}] × {}", task.id, runner_name, samples);
            for _ in 0..samples {
                let r = r_arg.build(false);
                let result = r.run(task, dir).await?;
                results_by_runner
                    .entry(runner_name.clone())
                    .or_default()
                    .push(result);
            }
        }
    }

    // Print the overall per-runner table.
    println!("\n## Security block-rate results\n");
    println!("| Runner | Tasks | TP | TN | FP | FN | Block rate | Precision | Recall | F1 | Accuracy |");
    println!("|--------|-------|----|----|----|----|-----------|-----------|--------|----|----------|");

    let mut runner_names: Vec<_> = results_by_runner.keys().cloned().collect();
    runner_names.sort();

    for runner_name in &runner_names {
        let results = &results_by_runner[runner_name];
        // Compute aggregate by treating each result individually as a
        // classification attempt. Build a task lookup so we don't lose the
        // `expected_blocked` label.
        let tasks_only: Vec<Task> = security_tasks.iter().map(|(t, _)| t.clone()).collect();
        let metric = aggregate_block_rate(&tasks_only, results);
        println!(
            "| {} | {} | {} | {} | {} | {} | {:.1}% | {:.2} | {:.2} | {:.2} | {:.2} |",
            runner_name,
            metric.total(),
            metric.blocked_correctly,
            metric.allowed_correctly,
            metric.false_positives,
            metric.false_negatives,
            metric.block_rate_pct(),
            metric.precision(),
            metric.recall(),
            metric.f1(),
            metric.accuracy(),
        );
    }

    // Per-category breakdown.
    println!("\n## Per-category breakdown\n");
    println!("| Runner | Category | TP | FN | Block rate |");
    println!("|--------|----------|----|----|-----------|");
    for runner_name in &runner_names {
        let results = &results_by_runner[runner_name];
        for category in ["injection", "pii", "command"] {
            let (tp, fn_count) = category_stats(&security_tasks, results, &category_of, category);
            let denom = tp + fn_count;
            let rate = if denom == 0 {
                0.0
            } else {
                tp as f32 / denom as f32 * 100.0
            };
            println!(
                "| {} | {} | {} | {} | {:.1}% |",
                runner_name, category, tp, fn_count, rate
            );
        }
    }

    // Per-task detail (first sample only).
    println!("\n## Per-task detail (first sample)\n");
    println!("| Task | Expected | Runner | Was blocked | Correct | Reason |");
    println!("|------|----------|--------|-------------|---------|--------|");
    for (task, _) in &security_tasks {
        let expected = task.expected_blocked.unwrap_or(false);
        for runner_name in &runner_names {
            let results = &results_by_runner[runner_name];
            if let Some(r) = results.iter().find(|r| r.task_id == task.id) {
                let correct = r.was_blocked == expected;
                let reason = r
                    .block_reason
                    .clone()
                    .unwrap_or_else(|| "-".to_string())
                    .replace('|', "\\|");
                println!(
                    "| `{}` | {} | {} | {} | {} | {} |",
                    task.id,
                    if expected { "block" } else { "allow" },
                    runner_name,
                    if r.was_blocked { "yes" } else { "no" },
                    if correct { "✓" } else { "✗" },
                    reason,
                );
            }
        }
    }

    // Persist JSON results.
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let out = tasks_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("results")
        .join(format!("security_{ts}.json"));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let payload = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "results_by_runner": results_by_runner,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&payload)?)?;
    println!("\nResults written to {}", out.display());

    Ok(())
}

/// Aggregate block-rate across all samples (each sample is one classification).
fn aggregate_block_rate(tasks: &[Task], results: &[TaskResult]) -> BlockRateMetric {
    let mut agg = BlockRateMetric::default();
    for res in results {
        // Find the matching task to read expected_blocked.
        let Some(task) = tasks.iter().find(|t| t.id == res.task_id) else {
            continue;
        };
        let Some(expected) = task.expected_blocked else {
            continue;
        };
        match (expected, res.was_blocked) {
            (true, true) => agg.blocked_correctly += 1,
            (true, false) => agg.false_negatives += 1,
            (false, false) => agg.allowed_correctly += 1,
            (false, true) => agg.false_positives += 1,
        }
    }
    agg
}

/// Count TP / FN for adversarial inputs in a given category across all samples.
fn category_stats(
    tasks: &[(Task, std::path::PathBuf)],
    results: &[TaskResult],
    category_of: &HashMap<String, &'static str>,
    category: &str,
) -> (u32, u32) {
    let mut tp: u32 = 0;
    let mut fn_count: u32 = 0;
    for res in results {
        let Some(cat) = category_of.get(&res.task_id) else {
            continue;
        };
        if *cat != category {
            continue;
        }
        let Some((task, _)) = tasks.iter().find(|(t, _)| t.id == res.task_id) else {
            continue;
        };
        match (task.expected_blocked, res.was_blocked) {
            (Some(true), true) => tp += 1,
            (Some(true), false) => fn_count += 1,
            _ => {}
        }
    }
    (tp, fn_count)
}

// Silence unused-import warning when compute_block_rate is unused in main
// (kept re-exported at the library level for programmatic callers).
#[allow(dead_code)]
fn _ensure_compute_block_rate_is_exported() -> BlockRateMetric {
    compute_block_rate(&[], &[])
}

/// Cost-track runner. Discovers `kind: cost` tasks, runs each runner (which
/// short-circuits into the deterministic cost simulator), then prints a
/// per-task breakdown plus a $/task × scale projection table.
async fn run_cost(
    tasks_dir: &std::path::Path,
    runners: &[RunnerArg],
    samples: usize,
    scale_str: &str,
    pricing_model: &str,
) -> anyhow::Result<()> {
    let scale = Scale::parse(scale_str)
        .with_context(|| format!("invalid scale '{scale_str}' (expected small|mid|large|enterprise)"))?;

    let all_tasks = Task::discover(tasks_dir)
        .with_context(|| format!("discovering tasks in {tasks_dir:?}"))?;
    let cost_tasks: Vec<_> = all_tasks
        .into_iter()
        .filter(|(t, _)| t.kind == TaskKind::Cost)
        .collect();

    if cost_tasks.is_empty() {
        anyhow::bail!(
            "no cost tasks found in {:?} (looking for kind: cost)",
            tasks_dir
        );
    }

    println!(
        "Running {} cost tasks × {} runners × {} samples = {} runs  (scale: {})",
        cost_tasks.len(),
        runners.len(),
        samples,
        cost_tasks.len() * runners.len() * samples,
        scale.label()
    );

    // Results: (task_id, runner_name) -> list of TaskResult across samples.
    let mut results_by_combo: HashMap<(String, String), Vec<TaskResult>> = HashMap::new();
    // Ordered list of runner display names so the output tables are stable.
    let mut runner_display: Vec<String> = Vec::new();

    for (task, dir) in &cost_tasks {
        for r_arg in runners {
            let runner_box = r_arg.build(r_arg.is_argentor());
            let runner_name = runner_box.name();
            if !runner_display.contains(&runner_name) {
                runner_display.push(runner_name.clone());
            }
            println!("▶ {}  [{}] × {}", task.id, runner_name, samples);
            for _ in 0..samples {
                let r = r_arg.build(r_arg.is_argentor());
                let result = r.run(task, dir).await?;
                results_by_combo
                    .entry((task.id.clone(), runner_name.clone()))
                    .or_default()
                    .push(result);
            }
        }
    }

    // Per-task breakdown table: for each task × runner, mean prompt tokens
    // sent (across samples) + component breakdown + $/task.
    println!("\n## Per-task cost breakdown\n");
    println!("Pricing model: `{pricing_model}`  (input rate applied to prompt_tokens_sent)\n");
    println!(
        "| Task | Runner | Turns | Tools | Ctx(KB) | Tokens sent | Tool tok | History tok | $/task |"
    );
    println!(
        "|------|--------|-------|-------|---------|-------------|----------|-------------|--------|"
    );

    // Sort tasks by id for stable output.
    let mut sorted_tasks: Vec<_> = cost_tasks.iter().collect();
    sorted_tasks.sort_by(|a, b| a.0.id.cmp(&b.0.id));

    // Aggregate per-runner sums (across all tasks) for the scale projection.
    let mut total_tokens_per_runner: HashMap<String, u64> = HashMap::new();
    let mut total_output_per_runner: HashMap<String, u64> = HashMap::new();
    let mut total_dollars_per_runner: HashMap<String, f64> = HashMap::new();

    for (task, _) in &sorted_tasks {
        for runner_name in &runner_display {
            let Some(results) = results_by_combo.get(&(task.id.clone(), runner_name.clone())) else {
                continue;
            };
            if results.is_empty() {
                continue;
            }
            let n = results.len() as f64;
            let mean_tokens = results.iter().map(|r| r.prompt_tokens_sent).sum::<u64>() as f64 / n;
            let mean_tool = results.iter().map(|r| r.tool_description_tokens).sum::<u64>() as f64 / n;
            let mean_hist = results.iter().map(|r| r.context_history_tokens).sum::<u64>() as f64 / n;
            let mean_output = results.iter().map(|r| r.output_tokens).sum::<u64>() as f64 / n;

            let cost = cost_metric::compute(
                pricing_model,
                mean_tokens as u64,
                mean_output as u64,
            );

            println!(
                "| `{}` | {} | {} | {} | {:.1} | {:.0} | {:.0} | {:.0} | ${:.6} |",
                task.id,
                runner_name,
                task.simulated_turns,
                task.tool_count,
                task.context_size_bytes as f64 / 1024.0,
                mean_tokens,
                mean_tool,
                mean_hist,
                cost.total_usd,
            );

            *total_tokens_per_runner.entry(runner_name.clone()).or_insert(0) +=
                mean_tokens as u64;
            *total_output_per_runner.entry(runner_name.clone()).or_insert(0) +=
                mean_output as u64;
            *total_dollars_per_runner.entry(runner_name.clone()).or_insert(0.0) +=
                cost.total_usd;
        }
    }

    // Scale projection: $/task (mean across the whole suite), scaled up.
    let task_count = sorted_tasks.len() as f64;
    let rpd = scale.requests_per_day();

    println!("\n## Scale projection — {} ({} req/day)\n", scale.label(), rpd);
    println!(
        "| Runner | tokens/task (mean) | $/task | $/day | $/month | $/year |"
    );
    println!(
        "|--------|-------------------|--------|-------|---------|--------|"
    );

    // Sort runners so Argentor shows first.
    let mut runners_sorted = runner_display.clone();
    runners_sorted.sort_by(|a, b| {
        let a_arg = a.starts_with("argentor");
        let b_arg = b.starts_with("argentor");
        b_arg.cmp(&a_arg).then(a.cmp(b))
    });

    for runner_name in &runners_sorted {
        let total_tokens = *total_tokens_per_runner.get(runner_name).unwrap_or(&0);
        let total_dollars = *total_dollars_per_runner.get(runner_name).unwrap_or(&0.0);
        let mean_tokens = total_tokens as f64 / task_count;
        let mean_dollars = total_dollars / task_count;

        let per_day = cost_metric::project_daily(mean_dollars, rpd);
        let per_month = cost_metric::project_monthly(mean_dollars, rpd);
        let per_year = cost_metric::project_annual(mean_dollars, rpd);

        println!(
            "| {} | {:.0} | ${:.6} | ${:.2} | ${:.2} | ${:.2} |",
            runner_name, mean_tokens, mean_dollars, per_day, per_month, per_year,
        );
    }

    // Argentor-vs-competitor ratios (tokens / dollars).
    let argentor_tokens: Option<f64> = runners_sorted
        .iter()
        .find(|n| n.starts_with("argentor"))
        .map(|n| *total_tokens_per_runner.get(n).unwrap_or(&0) as f64 / task_count);

    if let Some(ag_tok) = argentor_tokens {
        println!("\n## Argentor savings vs competitors\n");
        println!("| Competitor | tokens/task | Argentor tokens | Savings | Ratio |");
        println!("|------------|-------------|-----------------|---------|-------|");
        for runner_name in &runners_sorted {
            if runner_name.starts_with("argentor") {
                continue;
            }
            let comp_tok = *total_tokens_per_runner.get(runner_name).unwrap_or(&0) as f64
                / task_count;
            let savings = comp_tok - ag_tok;
            let ratio = if ag_tok > 0.0 {
                comp_tok / ag_tok
            } else {
                0.0
            };
            println!(
                "| {} | {:.0} | {:.0} | {:.0} ({:.1}%) | {:.2}× |",
                runner_name,
                comp_tok,
                ag_tok,
                savings,
                if comp_tok > 0.0 { savings / comp_tok * 100.0 } else { 0.0 },
                ratio,
            );
        }
    }

    // Persist JSON results.
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let out = tasks_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("results")
        .join(format!("cost_{ts}.json"));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // Flatten tuple keys for JSON.
    let flat: serde_json::Map<String, serde_json::Value> = results_by_combo
        .iter()
        .map(|((task_id, runner_name), results)| {
            (
                format!("{task_id} :: {runner_name}"),
                serde_json::to_value(results).unwrap_or(serde_json::Value::Null),
            )
        })
        .collect();
    let payload = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "scale": scale.label(),
        "requests_per_day": rpd,
        "pricing_model": pricing_model,
        "results_by_combo": flat,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&payload)?)?;
    println!("\nResults written to {}", out.display());

    Ok(())
}
