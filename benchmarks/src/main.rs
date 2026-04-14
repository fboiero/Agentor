//! CLI for running benchmarks.
//!
//! Examples:
//! ```bash
//! # List discovered tasks
//! cargo run -p argentor-benchmarks -- list
//!
//! # Run a specific task on a runner
//! cargo run -p argentor-benchmarks --release -- run --task t1_pdf_summary --runner argentor
//!
//! # Run all tasks on all runners (mock)
//! cargo run -p argentor-benchmarks --release -- run-all
//! ```

use anyhow::Context;
use argentor_benchmarks::metrics;
use argentor_benchmarks::report::RunReport;
use argentor_benchmarks::runners::{ArgentorRunner, ExternalRunner, MockRunner, Runner, RunnerKind};
use argentor_benchmarks::task::Task;
use clap::{Parser, Subcommand, ValueEnum};
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
                // Honor $ARGENTOR_LC_RUNNER env var first; fallback to PATH lookup
                // (pip install -e "benchmarks/external/langchain_runner[dev]")
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
        Command::RunAll { runners } => {
            let tasks = Task::discover(&cli.tasks_dir)?;
            if tasks.is_empty() {
                anyhow::bail!("no tasks found in {:?}", cli.tasks_dir);
            }
            let mut all_metrics = Vec::new();
            for (task, dir) in &tasks {
                for r_arg in &runners {
                    let r = r_arg.build(false);
                    println!("▶ {}  [{}]", task.id, r.name());
                    let result = r.run(task, dir).await?;
                    let m = metrics::compute(task, &result);
                    all_metrics.push(m);
                }
            }
            let report = RunReport::new(all_metrics);
            println!("\n{}", report.to_markdown());
            // Persist JSON
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
            std::fs::write(&out, report.to_json()?)?;
            println!("\nResults written to {}", out.display());
        }
    }

    Ok(())
}
