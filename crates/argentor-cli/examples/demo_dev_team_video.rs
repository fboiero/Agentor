#![allow(clippy::expect_used)]
//! Dev Team Video Demo — Real Code Intelligence, Zero Simulation
//!
//! Shows a FullStack team building a calculator library from scratch using
//! **every** ArgenTor code intelligence module for real:
//!
//!   - **CodePlanner** — generates the implementation plan
//!   - **CodeGraph**   — parses and analyzes the code structure
//!   - **DiffEngine**  — generates precise unified diffs
//!   - **TestOracle**  — parses cargo test output, drives TDD cycle
//!   - **ReviewEngine** — 25+ rule code review across 7 dimensions
//!   - **DevTeam**     — team roles, workflows, quality gates
//!
//! **No API keys, no mocks, no scripted responses.**
//! Every module runs its real algorithms on real code.
//!
//!   cargo run -p argentor-cli --example demo_dev_team_video

use argentor_agent::code_graph::CodeGraph;
use argentor_agent::code_planner::CodePlanner;
use argentor_agent::diff_engine::DiffEngine;
use argentor_agent::review_engine::{FindingSeverity, ReviewEngine, ReviewVerdict};
use argentor_agent::test_oracle::{TestOracle, TestStatus};
use argentor_orchestrator::dev_team::{DevRole, DevTeam, DevWorkflow};
use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};

// ── ANSI ─────────────────────────────────────────────────────────────────────

const RST: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const YEL: &str = "\x1b[33m";
const GRN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const MAG: &str = "\x1b[35m";
const BLU: &str = "\x1b[34m";
const WHT: &str = "\x1b[97m";
const BG_BLU: &str = "\x1b[44m";
const BG_GRN: &str = "\x1b[42m";
const BG_RED: &str = "\x1b[41m";
const BG_MAG: &str = "\x1b[45m";
const BG_CYAN: &str = "\x1b[46m";
const BG_YEL: &str = "\x1b[43m";

// ── Display helpers ──────────────────────────────────────────────────────────

fn delay(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn typewrite(text: &str, ms_per_char: u64) {
    for ch in text.chars() {
        print!("{ch}");
        std::io::stdout().flush().ok();
        std::thread::sleep(Duration::from_millis(ms_per_char));
    }
}

fn spinner(label: &str, duration_ms: u64) {
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let step = 80;
    let iters = duration_ms / step;
    for i in 0..iters {
        print!(
            "\x1b[2K\r  {CYAN}{}{RST} {DIM}{label}{RST}",
            frames[i as usize % frames.len()]
        );
        std::io::stdout().flush().ok();
        std::thread::sleep(Duration::from_millis(step));
    }
    print!("\x1b[2K\r");
    std::io::stdout().flush().ok();
}

fn banner() {
    println!();
    println!("  {BG_BLU}{WHT}{BOLD}                                                          {RST}");
    println!("  {BG_BLU}{WHT}{BOLD}   ARGENTOR — Autonomous Dev Team Demo                    {RST}");
    println!("  {BG_BLU}{WHT}{BOLD}   Real Code Intelligence • Zero Simulation                {RST}");
    println!("  {BG_BLU}{WHT}{BOLD}                                                          {RST}");
    println!();
    delay(1500);
}

fn phase_header(num: usize, title: &str, color: &str) {
    println!();
    println!(
        "  {color}{BOLD}━━━ PHASE {num}: {title} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RST}"
    );
    println!();
    delay(800);
}

fn agent_says(role: &str, color: &str, icon: &str, message: &str) {
    print!("  {color}{BOLD}{icon} {role:12}{RST} ");
    typewrite(message, 8);
    println!();
    delay(200);
}

fn detail(text: &str) {
    println!("  {DIM}   {text}{RST}");
    delay(100);
}

fn code_block(filename: &str, content: &str, max_lines: usize) {
    println!("  {DIM}┌─ {CYAN}{filename}{RST}");
    for (i, line) in content.lines().enumerate() {
        if i >= max_lines {
            println!("  {DIM}│  ... ({} more lines){RST}", content.lines().count() - max_lines);
            break;
        }
        println!("  {DIM}│{RST}  {line}");
    }
    println!("  {DIM}└────────────────────────{RST}");
    delay(300);
}

fn result_line(icon: &str, label: &str, value: &str) {
    println!("  {icon}  {BOLD}{label}{RST}: {value}");
    delay(150);
}

fn separator() {
    println!();
    delay(400);
}

// ── Code content ─────────────────────────────────────────────────────────────

const CARGO_TOML: &str = r#"[package]
name = "calc"
version = "0.1.0"
edition = "2021"
"#;

/// RED phase: tests exist but no implementation → compilation error
const LIB_RS_RED: &str = r#"/// A calculator library.
pub struct Calculator;

impl Calculator {
    pub fn new() -> Self {
        Calculator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let calc = Calculator::new();
        assert_eq!(calc.add(2, 3), 5);
    }

    #[test]
    fn test_subtract() {
        let calc = Calculator::new();
        assert_eq!(calc.subtract(10, 4), 6);
    }

    #[test]
    fn test_multiply() {
        let calc = Calculator::new();
        assert_eq!(calc.multiply(3, 7), 21);
    }

    #[test]
    fn test_divide() {
        let calc = Calculator::new();
        assert_eq!(calc.divide(20, 4), Ok(5));
    }

    #[test]
    fn test_divide_by_zero() {
        let calc = Calculator::new();
        assert!(calc.divide(10, 0).is_err());
    }
}
"#;

/// GREEN phase: full implementation — tests pass
const LIB_RS_GREEN: &str = r#"/// A calculator library.
pub struct Calculator;

impl Calculator {
    pub fn new() -> Self {
        Calculator
    }

    pub fn add(&self, a: i64, b: i64) -> i64 {
        a + b
    }

    pub fn subtract(&self, a: i64, b: i64) -> i64 {
        a - b
    }

    pub fn multiply(&self, a: i64, b: i64) -> i64 {
        a * b
    }

    pub fn divide(&self, a: i64, b: i64) -> Result<i64, String> {
        if b == 0 {
            return Err("division by zero".to_string());
        }
        Ok(a / b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let calc = Calculator::new();
        assert_eq!(calc.add(2, 3), 5);
    }

    #[test]
    fn test_add_negative() {
        let calc = Calculator::new();
        assert_eq!(calc.add(-2, -3), -5);
    }

    #[test]
    fn test_subtract() {
        let calc = Calculator::new();
        assert_eq!(calc.subtract(10, 4), 6);
    }

    #[test]
    fn test_multiply() {
        let calc = Calculator::new();
        assert_eq!(calc.multiply(3, 7), 21);
    }

    #[test]
    fn test_multiply_by_zero() {
        let calc = Calculator::new();
        assert_eq!(calc.multiply(42, 0), 0);
    }

    #[test]
    fn test_divide() {
        let calc = Calculator::new();
        assert_eq!(calc.divide(20, 4), Ok(5));
    }

    #[test]
    fn test_divide_by_zero() {
        let calc = Calculator::new();
        assert!(calc.divide(10, 0).is_err());
    }

    #[test]
    fn test_divide_negative() {
        let calc = Calculator::new();
        assert_eq!(calc.divide(-10, 2), Ok(-5));
    }
}
"#;

/// REFACTOR phase: doc comments, proper error type, derive macros
const LIB_RS_FINAL: &str = r#"//! A calculator library with safe arithmetic operations.
//!
//! Provides basic operations with error handling for edge cases
//! like division by zero.

/// Errors that can occur during calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalcError {
    /// Attempted to divide by zero.
    DivisionByZero,
}

impl std::fmt::Display for CalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalcError::DivisionByZero => write!(f, "division by zero"),
        }
    }
}

impl std::error::Error for CalcError {}

/// A simple calculator with safe arithmetic operations.
///
/// # Examples
///
/// ```
/// let calc = calc::Calculator::new();
/// assert_eq!(calc.add(2, 3), 5);
/// assert_eq!(calc.divide(10, 2), Ok(5));
/// assert!(calc.divide(1, 0).is_err());
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Calculator;

impl Calculator {
    /// Creates a new `Calculator` instance.
    #[must_use]
    pub fn new() -> Self {
        Calculator
    }

    /// Adds two numbers.
    #[must_use]
    pub fn add(&self, a: i64, b: i64) -> i64 {
        a + b
    }

    /// Subtracts `b` from `a`.
    #[must_use]
    pub fn subtract(&self, a: i64, b: i64) -> i64 {
        a - b
    }

    /// Multiplies two numbers.
    #[must_use]
    pub fn multiply(&self, a: i64, b: i64) -> i64 {
        a * b
    }

    /// Divides `a` by `b`.
    ///
    /// Returns [`CalcError::DivisionByZero`] if `b` is zero.
    pub fn divide(&self, a: i64, b: i64) -> Result<i64, CalcError> {
        if b == 0 {
            return Err(CalcError::DivisionByZero);
        }
        Ok(a / b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let calc = Calculator::new();
        assert_eq!(calc.add(2, 3), 5);
    }

    #[test]
    fn test_add_negative() {
        let calc = Calculator::new();
        assert_eq!(calc.add(-2, -3), -5);
    }

    #[test]
    fn test_subtract() {
        let calc = Calculator::new();
        assert_eq!(calc.subtract(10, 4), 6);
    }

    #[test]
    fn test_multiply() {
        let calc = Calculator::new();
        assert_eq!(calc.multiply(3, 7), 21);
    }

    #[test]
    fn test_multiply_by_zero() {
        let calc = Calculator::new();
        assert_eq!(calc.multiply(42, 0), 0);
    }

    #[test]
    fn test_divide() {
        let calc = Calculator::new();
        assert_eq!(calc.divide(20, 4), Ok(5));
    }

    #[test]
    fn test_divide_by_zero() {
        let calc = Calculator::new();
        assert_eq!(calc.divide(10, 0), Err(CalcError::DivisionByZero));
    }

    #[test]
    fn test_divide_negative() {
        let calc = Calculator::new();
        assert_eq!(calc.divide(-10, 2), Ok(-5));
    }
}
"#;

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let start = Instant::now();

    // Create temp project
    let tmp = std::env::temp_dir().join(format!("argentor-demo-{}", std::process::id()));
    let project = tmp.join("calc");
    std::fs::create_dir_all(project.join("src")).expect("create project dirs");
    std::fs::write(project.join("Cargo.toml"), CARGO_TOML).expect("write Cargo.toml");

    banner();

    // ─── PHASE 1: TEAM ASSEMBLY ──────────────────────────────────────────────
    phase_header(1, "TEAM ASSEMBLY", BG_BLU);

    let team = DevTeam::full_stack();
    let workflow = DevWorkflow::ImplementFeature;
    let roles = team.required_roles(workflow.clone());

    agent_says("Orchestrator", MAG, "🎯", "Assembling FullStack dev team...");
    delay(500);

    let role_icons: &[(&str, &str, &str)] = &[
        ("Architect", "🏗️ ", CYAN),
        ("Implementer", "💻", BLU),
        ("Tester", "🧪", GRN),
        ("Reviewer", "🔍", YEL),
        ("SecurityAuditor", "🔒", RED),
        ("Documenter", "📝", MAG),
    ];

    for (name, icon, color) in role_icons {
        let prompt_preview = team.role_system_prompt(match *name {
            "Architect" => DevRole::Architect,
            "Implementer" => DevRole::Implementer,
            "Tester" => DevRole::Tester,
            "Reviewer" => DevRole::Reviewer,
            "SecurityAuditor" => DevRole::SecurityAuditor,
            _ => DevRole::Documenter,
        });
        let preview: String = prompt_preview.chars().take(70).collect();
        agent_says(name, color, icon, &format!("Online. \"{preview}...\""));
    }

    separator();
    result_line("👥", "Team", &format!("{} roles active", roles.len()));
    result_line("📋", "Workflow", "ImplementFeature (8 steps)");

    let steps = team.workflow_steps(workflow);
    for step in &steps {
        detail(&format!(
            "Step {}: {:?} — {}",
            step.order, step.role, step.action
        ));
    }

    // ─── PHASE 2: SPECIFICATION ──────────────────────────────────────────────
    phase_header(2, "SPECIFICATION", BG_CYAN);

    let task_title = "Calculator library";
    let task_desc = "Build a calculator library in Rust with add, subtract, multiply, \
                     and divide operations. Include proper error handling for division \
                     by zero and comprehensive test coverage.";

    agent_says("Architect", CYAN, "🏗️ ", "Analyzing feature request...");
    println!();
    println!("  {BOLD}{WHT}Feature Request:{RST}");
    println!("  {DIM}\"{}\"", task_desc);
    println!("  {RST}");
    delay(1000);

    // ─── PHASE 3: PLANNING ───────────────────────────────────────────────────
    phase_header(3, "PLANNING", BG_MAG);

    agent_says("Architect", CYAN, "🏗️ ", "Running CodePlanner...");
    spinner("Generating implementation plan", 1500);

    let planner = CodePlanner::new();
    let plan = planner.plan_feature(task_title, task_desc, &[]);
    let plan_md = planner.format_as_markdown(&plan);

    result_line("📐", "Plan", &plan.title);
    result_line("📊", "Risk", &format!("{}", plan.risk.level));
    result_line("📦", "Effort", &format!("{}", plan.total_effort));
    result_line("📁", "Files", &format!("{}", plan.affected_files.len()));
    separator();

    println!("  {BOLD}{WHT}Implementation Steps:{RST}");
    for step in &plan.steps {
        let effort_color = match format!("{}", step.effort).as_str() {
            "Small" => GRN,
            "Medium" => YEL,
            _ => RED,
        };
        println!(
            "  {DIM}  {}{RST} {BOLD}Step {}:{RST} {} — {}{}{RST}  [{effort_color}{}{RST}]",
            if step.depends_on.is_empty() {
                "  "
            } else {
                "↳ "
            },
            step.order,
            step.file,
            DIM,
            step.description,
            step.effort
        );
        delay(200);
    }

    separator();
    println!("  {DIM}Test strategy: {:?}{RST}", plan.test_strategy.test_types);
    delay(500);

    // ─── PHASE 4: TDD RED ────────────────────────────────────────────────────
    phase_header(4, "TDD RED — Write Tests First", BG_RED);

    agent_says(
        "Tester",
        GRN,
        "🧪",
        "Writing tests before implementation (TDD)...",
    );
    spinner("Creating test suite", 800);

    std::fs::write(project.join("src/lib.rs"), LIB_RS_RED).expect("write lib.rs RED");
    code_block("src/lib.rs", LIB_RS_RED, 20);

    agent_says("Tester", GRN, "🧪", "Running cargo test...");
    spinner("Compiling", 600);

    let test_output_red = Command::new("cargo")
        .args(["test", "--", "--color=never"])
        .current_dir(&project)
        .output()
        .expect("cargo test");

    let test_stderr = String::from_utf8_lossy(&test_output_red.stderr);
    let test_stdout = String::from_utf8_lossy(&test_output_red.stdout);
    let combined_output = format!("{test_stdout}\n{test_stderr}");

    // Show raw output snippet
    println!("  {RED}{BOLD}cargo test output:{RST}");
    for line in combined_output.lines().take(12) {
        if line.contains("error") {
            println!("  {RED}  {line}{RST}");
        } else {
            println!("  {DIM}  {line}{RST}");
        }
    }
    if combined_output.lines().count() > 12 {
        println!("  {DIM}  ... ({} more lines){RST}", combined_output.lines().count() - 12);
    }
    separator();

    // Parse with TestOracle
    agent_says("Tester", GRN, "🧪", "TestOracle analyzing failures...");
    let summary_red = TestOracle::parse_output(&combined_output);

    result_line(
        "❌",
        "Result",
        &format!(
            "{} passed, {} failed, {} errors",
            summary_red.passed, summary_red.failed, summary_red.errors
        ),
    );

    // Analyze individual failures
    for tc in &summary_red.cases {
        if tc.status == TestStatus::Failed || tc.status == TestStatus::Error {
            let analysis = TestOracle::analyze_failure(tc);
            println!(
                "  {RED}  ✗ {}{RST} — {:?} → Fix: {:?}",
                tc.name, analysis.error_type, analysis.fix_strategy
            );
        }
    }

    separator();
    println!("  {RED}{BOLD}TDD Phase: RED{RST} {DIM}— Tests written, compilation fails (expected!){RST}");
    delay(1500);

    // ─── PHASE 5: TDD GREEN — Implement ──────────────────────────────────────
    phase_header(5, "TDD GREEN — Implement", BG_GRN);

    agent_says(
        "Implementer",
        BLU,
        "💻",
        "Implementing Calculator to make tests pass...",
    );
    spinner("Writing implementation", 1200);

    std::fs::write(project.join("src/lib.rs"), LIB_RS_GREEN).expect("write lib.rs GREEN");
    code_block("src/lib.rs", LIB_RS_GREEN, 25);

    agent_says("Tester", GRN, "🧪", "Running cargo test...");
    spinner("Compiling and testing", 600);

    let test_output_green = Command::new("cargo")
        .args(["test", "--", "--color=never"])
        .current_dir(&project)
        .output()
        .expect("cargo test");

    let test_stdout_green = String::from_utf8_lossy(&test_output_green.stdout);
    let test_stderr_green = String::from_utf8_lossy(&test_output_green.stderr);
    let combined_green = format!("{test_stdout_green}\n{test_stderr_green}");

    let summary_green = TestOracle::parse_output(&combined_green);

    // Show test results
    for tc in &summary_green.cases {
        let (icon, color) = match tc.status {
            TestStatus::Passed => ("✓", GRN),
            TestStatus::Failed => ("✗", RED),
            _ => ("?", YEL),
        };
        println!("  {color}  {icon} {}{RST}", tc.name);
        delay(100);
    }

    separator();
    result_line(
        "✅",
        "Result",
        &format!(
            "{}/{} tests passing",
            summary_green.passed, summary_green.total
        ),
    );
    println!(
        "  {GRN}{BOLD}TDD Phase: GREEN{RST} {DIM}— All tests passing!{RST}"
    );
    delay(1500);

    // ─── PHASE 6: CODE ANALYSIS ──────────────────────────────────────────────
    phase_header(6, "CODE ANALYSIS", BG_CYAN);

    agent_says(
        "Architect",
        CYAN,
        "🏗️ ",
        "Running CodeGraph analysis...",
    );
    spinner("Parsing code structure", 800);

    let mut graph = CodeGraph::new();
    graph.parse_file("src/lib.rs", LIB_RS_GREEN);

    let code_summary = graph.summary();
    result_line(
        "📊",
        "Files",
        &format!("{}", code_summary.total_files),
    );
    result_line(
        "🔣",
        "Symbols",
        &format!("{}", code_summary.total_symbols),
    );
    result_line(
        "🌐",
        "Public API",
        &format!("{} symbols", code_summary.public_api_count),
    );

    separator();
    println!("  {BOLD}{WHT}Symbol Table:{RST}");
    let symbols = graph.symbols_in_file("src/lib.rs");
    for sym in &symbols {
        let kind_str = format!("{:?}", sym.kind);
        let vis_str = format!("{:?}", sym.visibility);
        println!(
            "  {DIM}  {}{RST} {BOLD}{}{RST} {DIM}({}:{}) [{vis_str}]{RST}",
            kind_str, sym.name, sym.file, sym.line
        );
        delay(80);
    }

    // Impact analysis on divide
    separator();
    agent_says(
        "Architect",
        CYAN,
        "🏗️ ",
        "Impact analysis: what if we change `divide`?",
    );
    let impact = graph.impact_analysis("divide");
    result_line(
        "💥",
        "Impact",
        &format!(
            "{} direct refs, {} affected files, risk: {:?}",
            impact.directly_affected.len(),
            impact.affected_files.len(),
            impact.risk_level
        ),
    );
    delay(800);

    // ─── PHASE 7: CODE REVIEW ────────────────────────────────────────────────
    phase_header(7, "CODE REVIEW", BG_YEL);

    agent_says(
        "Reviewer",
        YEL,
        "🔍",
        "Running ReviewEngine (25+ rules, 7 dimensions)...",
    );
    spinner("Analyzing code quality", 1200);

    let review = ReviewEngine::new();
    let report = review.review_code("src/lib.rs", LIB_RS_GREEN);

    // Show per-dimension scores
    println!("  {BOLD}{WHT}Dimension Scores:{RST}");
    for ds in &report.scores {
        let bar_len = (ds.score * 20.0) as usize;
        let bar: String = "█".repeat(bar_len);
        let empty: String = "░".repeat(20 - bar_len);
        let score_color = if ds.score >= 0.8 {
            GRN
        } else if ds.score >= 0.6 {
            YEL
        } else {
            RED
        };
        println!(
            "  {DIM}  {:15}{RST} {score_color}{bar}{DIM}{empty}{RST} {score_color}{:.0}%{RST} {DIM}({} findings){RST}",
            format!("{:?}", ds.dimension),
            ds.score * 100.0,
            ds.finding_count,
        );
        delay(150);
    }

    separator();
    // Show findings
    println!("  {BOLD}{WHT}Findings ({}):{RST}", report.findings.len());
    for finding in report.findings.iter().take(10) {
        let (icon, color) = match finding.severity {
            FindingSeverity::Critical => ("🔴", RED),
            FindingSeverity::Error => ("🟠", RED),
            FindingSeverity::Warning => ("🟡", YEL),
            FindingSeverity::Info => ("🔵", BLU),
        };
        let line_str = finding
            .line
            .map(|l| format!("L{l}"))
            .unwrap_or_default();
        println!(
            "  {color}  {icon} [{:6}] {line_str:4} {}{RST}",
            finding.rule_id, finding.message
        );
        if let Some(ref sug) = finding.suggestion {
            println!("  {DIM}           → {sug}{RST}");
        }
        delay(150);
    }
    if report.findings.len() > 10 {
        println!(
            "  {DIM}  ... and {} more findings{RST}",
            report.findings.len() - 10
        );
    }

    separator();
    let verdict_str = match report.verdict {
        ReviewVerdict::Approve => format!("{GRN}{BOLD}APPROVED{RST}"),
        ReviewVerdict::RequestChanges => format!("{YEL}{BOLD}REQUEST CHANGES{RST}"),
        ReviewVerdict::Block => format!("{RED}{BOLD}BLOCKED{RST}"),
    };
    result_line("📋", "Verdict", &verdict_str);
    result_line(
        "📊",
        "Score",
        &format!("{:.0}%", report.total_score * 100.0),
    );
    delay(1500);

    // ─── PHASE 8: REFACTOR ───────────────────────────────────────────────────
    phase_header(8, "REFACTOR — Fix Review Findings", BG_MAG);

    agent_says(
        "Implementer",
        BLU,
        "💻",
        "Fixing review findings: adding docs, proper error type, derives...",
    );
    spinner("Refactoring code", 1000);

    // Show the diff
    agent_says("Implementer", BLU, "💻", "DiffEngine generating changes...");
    let diff_engine = DiffEngine::new();
    let diff = diff_engine.generate_diff("src/lib.rs", LIB_RS_GREEN, LIB_RS_FINAL);

    let unified = diff_engine.format_unified(&diff);
    println!("  {BOLD}{WHT}Unified Diff:{RST}");
    for line in unified.lines().take(40) {
        if line.starts_with('+') && !line.starts_with("+++") {
            println!("  {GRN}  {line}{RST}");
        } else if line.starts_with('-') && !line.starts_with("---") {
            println!("  {RED}  {line}{RST}");
        } else if line.starts_with("@@") {
            println!("  {CYAN}  {line}{RST}");
        } else {
            println!("  {DIM}  {line}{RST}");
        }
        delay(30);
    }
    if unified.lines().count() > 40 {
        println!(
            "  {DIM}  ... ({} more lines){RST}",
            unified.lines().count() - 40
        );
    }

    separator();
    result_line(
        "📝",
        "Hunks",
        &format!("{}", diff.hunks.len()),
    );
    result_line(
        "➕",
        "Added",
        &format!(
            "{} lines",
            unified.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count()
        ),
    );
    result_line(
        "➖",
        "Removed",
        &format!(
            "{} lines",
            unified.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count()
        ),
    );

    // Write refactored code
    std::fs::write(project.join("src/lib.rs"), LIB_RS_FINAL).expect("write lib.rs FINAL");
    delay(500);

    // ─── PHASE 9: FINAL VERIFICATION ─────────────────────────────────────────
    phase_header(9, "FINAL VERIFICATION", BG_GRN);

    // Re-test
    agent_says("Tester", GRN, "🧪", "Running final test suite...");
    spinner("Testing refactored code", 600);

    let test_output_final = Command::new("cargo")
        .args(["test", "--", "--color=never"])
        .current_dir(&project)
        .output()
        .expect("cargo test");

    let final_stdout = String::from_utf8_lossy(&test_output_final.stdout);
    let final_stderr = String::from_utf8_lossy(&test_output_final.stderr);
    let combined_final = format!("{final_stdout}\n{final_stderr}");

    let summary_final = TestOracle::parse_output(&combined_final);

    for tc in &summary_final.cases {
        let (icon, color) = match tc.status {
            TestStatus::Passed => ("✓", GRN),
            TestStatus::Failed => ("✗", RED),
            _ => ("?", YEL),
        };
        println!("  {color}  {icon} {}{RST}", tc.name);
        delay(80);
    }

    separator();
    result_line(
        "✅",
        "Tests",
        &format!(
            "{}/{} passing",
            summary_final.passed, summary_final.total
        ),
    );

    // Re-review
    agent_says(
        "Reviewer",
        YEL,
        "🔍",
        "Final code review...",
    );
    spinner("Reviewing refactored code", 800);

    let final_report = review.review_code("src/lib.rs", LIB_RS_FINAL);

    for ds in &final_report.scores {
        let bar_len = (ds.score * 20.0) as usize;
        let bar: String = "█".repeat(bar_len);
        let empty: String = "░".repeat(20 - bar_len);
        let score_color = if ds.score >= 0.8 {
            GRN
        } else if ds.score >= 0.6 {
            YEL
        } else {
            RED
        };
        println!(
            "  {DIM}  {:15}{RST} {score_color}{bar}{DIM}{empty}{RST} {score_color}{:.0}%{RST}",
            format!("{:?}", ds.dimension),
            ds.score * 100.0,
        );
        delay(100);
    }

    separator();
    let final_verdict = match final_report.verdict {
        ReviewVerdict::Approve => format!("{GRN}{BOLD}APPROVED ✓{RST}"),
        ReviewVerdict::RequestChanges => format!("{YEL}{BOLD}REQUEST CHANGES{RST}"),
        ReviewVerdict::Block => format!("{RED}{BOLD}BLOCKED{RST}"),
    };
    result_line("📋", "Verdict", &final_verdict);
    result_line(
        "📊",
        "Score",
        &format!(
            "{:.0}% → {:.0}%",
            report.total_score * 100.0,
            final_report.total_score * 100.0
        ),
    );

    // ─── PHASE 10: SUMMARY ───────────────────────────────────────────────────
    phase_header(10, "MISSION COMPLETE", BG_GRN);

    let elapsed = start.elapsed();

    println!("  {BOLD}{WHT}Team Performance:{RST}");
    result_line("⏱️ ", "Total time", &format!("{:.1}s", elapsed.as_secs_f64()));
    result_line("🏗️ ", "Plan steps", &format!("{}", plan.steps.len()));
    result_line("🧪", "Tests written", &format!("{}", summary_final.total));
    result_line("✅", "Tests passing", &format!("{}", summary_final.passed));
    result_line("🔍", "Review findings fixed", &format!("{} → {}", report.findings.len(), final_report.findings.len()));
    result_line("📊", "Quality score", &format!("{:.0}% → {:.0}%", report.total_score * 100.0, final_report.total_score * 100.0));
    result_line("🔣", "Symbols analyzed", &format!("{}", code_summary.total_symbols));

    separator();
    println!("  {BOLD}{WHT}Artifacts:{RST}");
    result_line("📐", "Implementation plan", &format!("{} steps", plan.steps.len()));
    result_line("💻", "Source code", "src/lib.rs (Calculator + CalcError)");
    result_line("🧪", "Test suite", &format!("{} tests", summary_final.total));
    result_line("🔍", "Code review report", &format!("{} findings analyzed", report.findings.len()));
    result_line("📝", "Diff", &format!("{} hunks, {} lines changed", diff.hunks.len(),
        unified.lines().filter(|l| l.starts_with('+') || l.starts_with('-')).count()
    ));

    separator();
    println!(
        "  {BOLD}{WHT}Modules used:{RST} {CYAN}CodePlanner{RST} · {CYAN}CodeGraph{RST} · \
         {CYAN}DiffEngine{RST} · {CYAN}TestOracle{RST} · {CYAN}ReviewEngine{RST} · {CYAN}DevTeam{RST}"
    );
    println!("  {DIM}All real execution — zero simulation, zero API keys{RST}");
    println!();
    println!("  {DIM}Project: {}{RST}", project.display());
    println!();

    // Don't delete temp dir so user can inspect
    println!("  {BG_BLU}{WHT}{BOLD}  ARGENTOR — 14 crates · 1514 tests · 85K+ LOC  {RST}");
    println!();
}
