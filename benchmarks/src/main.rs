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
use argentor_benchmarks::metrics::{self, PairedTTest, Stats};
use argentor_benchmarks::report::RunReport;
use argentor_benchmarks::runners::{ArgentorRunner, ExternalRunner, MockRunner, Runner, RunnerKind};
use argentor_benchmarks::task::Task;
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
}

#[derive(Clone, Copy, ValueEnum)]
enum RunnerArg {
    Argentor,
    Langchain,
    Mock,
}

impl RunnerArg {
    #[allow(dead_code)]
    fn kind(&self) -> RunnerKind {
        match self {
            RunnerArg::Argentor => RunnerKind::Argentor,
            RunnerArg::Langchain => RunnerKind::Langchain,
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
    }

    Ok(())
}
