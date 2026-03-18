//! Implementation planning module for structured code change plans.
//!
//! Given a feature description, bug report, or refactoring goal, this module
//! generates a structured implementation plan with file-level granularity,
//! dependency ordering, effort estimation, and risk assessment.
//!
//! # Main types
//!
//! - [`CodePlanner`] — The planning engine that generates [`ImplementationPlan`]s.
//! - [`ImplementationPlan`] — A complete, validated plan with ordered steps.
//! - [`PlanStep`] — A single file-level operation within a plan.
//! - [`TaskType`] — Classification of the work (feature, bugfix, refactor, etc.).
//! - [`AgentRole`] — Which agent persona should execute each step.
//! - [`RiskAssessment`] — Automated risk analysis with mitigations.
//!
//! # Example
//!
//! ```rust
//! use argentor_agent::code_planner::{CodePlanner, PlannerConfig};
//!
//! let planner = CodePlanner::new();
//! let plan = planner.plan_feature(
//!     "Add retry logic",
//!     "Implement exponential backoff for HTTP requests",
//!     &["src/http.rs", "src/config.rs"],
//! );
//! assert!(!plan.steps.is_empty());
//! println!("{}", planner.format_as_markdown(&plan));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The type of implementation task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// Implement a new feature.
    Feature,
    /// Fix a bug.
    BugFix,
    /// Refactor existing code.
    Refactor,
    /// Add test coverage.
    AddTests,
    /// Performance optimization.
    Optimization,
    /// Security hardening.
    SecurityFix,
    /// Documentation.
    Documentation,
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Feature => write!(f, "Feature"),
            Self::BugFix => write!(f, "Bug Fix"),
            Self::Refactor => write!(f, "Refactor"),
            Self::AddTests => write!(f, "Add Tests"),
            Self::Optimization => write!(f, "Optimization"),
            Self::SecurityFix => write!(f, "Security Fix"),
            Self::Documentation => write!(f, "Documentation"),
        }
    }
}

/// File operation in a plan step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOperation {
    /// Create a new file.
    Create,
    /// Modify an existing file.
    Modify,
    /// Delete a file.
    Delete,
    /// Rename a file.
    Rename {
        /// The new path after renaming.
        new_path: String,
    },
}

impl fmt::Display for FileOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create => write!(f, "Create"),
            Self::Modify => write!(f, "Modify"),
            Self::Delete => write!(f, "Delete"),
            Self::Rename { new_path } => write!(f, "Rename → {new_path}"),
        }
    }
}

/// Effort estimation for a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effort {
    /// Less than 20 lines changed.
    Small,
    /// Between 20 and 100 lines changed.
    Medium,
    /// More than 100 lines changed.
    Large,
}

impl fmt::Display for Effort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Small => write!(f, "Small (<20 lines)"),
            Self::Medium => write!(f, "Medium (20-100 lines)"),
            Self::Large => write!(f, "Large (>100 lines)"),
        }
    }
}

/// Agent role assignment for plan steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    /// Designs architecture and makes structural decisions.
    Architect,
    /// Writes implementation code.
    Implementer,
    /// Writes and maintains tests.
    Tester,
    /// Reviews code for quality and correctness.
    Reviewer,
    /// Investigates and fixes bugs.
    Debugger,
    /// Handles infrastructure, CI/CD, and deployment.
    DevOps,
    /// Audits security and hardens code.
    SecurityAuditor,
    /// Writes and updates documentation.
    Documenter,
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Architect => write!(f, "Architect"),
            Self::Implementer => write!(f, "Implementer"),
            Self::Tester => write!(f, "Tester"),
            Self::Reviewer => write!(f, "Reviewer"),
            Self::Debugger => write!(f, "Debugger"),
            Self::DevOps => write!(f, "DevOps"),
            Self::SecurityAuditor => write!(f, "Security Auditor"),
            Self::Documenter => write!(f, "Documenter"),
        }
    }
}

/// Risk level for a plan or individual risk factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Minimal risk; unlikely to cause issues.
    Low,
    /// Moderate risk; review carefully.
    Medium,
    /// High risk; requires careful testing and review.
    High,
    /// Critical risk; may break production systems.
    Critical,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single step in an implementation plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step number (1-based).
    pub order: usize,
    /// File to operate on.
    pub file: String,
    /// What to do with the file.
    pub operation: FileOperation,
    /// Human-readable description of the change.
    pub description: String,
    /// Specific instructions for the agent executing this step.
    pub instructions: Vec<String>,
    /// Dependencies: step numbers that must complete before this one.
    pub depends_on: Vec<usize>,
    /// Estimated effort for this step.
    pub effort: Effort,
    /// Which agent role should handle this step.
    pub assigned_role: AgentRole,
    /// Whether this step modifies public API.
    pub breaks_api: bool,
}

/// A single risk factor within a [`RiskAssessment`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Description of the risk.
    pub description: String,
    /// Severity level.
    pub level: RiskLevel,
}

/// Risk assessment for an implementation plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// Overall risk level (the maximum across all factors).
    pub level: RiskLevel,
    /// Specific risk factors identified.
    pub factors: Vec<RiskFactor>,
    /// Suggested mitigations.
    pub mitigations: Vec<String>,
}

/// Testing strategy for a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStrategy {
    /// Types of tests to write (e.g., "unit", "integration", "property").
    pub test_types: Vec<String>,
    /// Files that need test updates.
    pub test_files: Vec<String>,
    /// Minimum test coverage target (0.0–1.0).
    pub coverage_target: Option<f32>,
    /// Manual testing steps a human should perform.
    pub manual_checks: Vec<String>,
}

/// A complete implementation plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationPlan {
    /// Human-readable title.
    pub title: String,
    /// Type of task.
    pub task_type: TaskType,
    /// Detailed description of what will be done.
    pub description: String,
    /// Ordered steps.
    pub steps: Vec<PlanStep>,
    /// Files that will be affected.
    pub affected_files: Vec<String>,
    /// Risk assessment.
    pub risk: RiskAssessment,
    /// Estimated total effort.
    pub total_effort: Effort,
    /// Suggested testing strategy.
    pub test_strategy: TestStrategy,
    /// Rollback instructions if something goes wrong.
    pub rollback: String,
}

// ---------------------------------------------------------------------------
// PlannerConfig
// ---------------------------------------------------------------------------

/// Configuration for the [`CodePlanner`].
#[derive(Debug, Clone)]
pub struct PlannerConfig {
    /// Maximum steps in a plan (default: 20).
    pub max_steps: usize,
    /// Whether to include rollback instructions (default: true).
    pub include_rollback: bool,
    /// Whether to include test strategy (default: true).
    pub include_test_strategy: bool,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            max_steps: 20,
            include_rollback: true,
            include_test_strategy: true,
        }
    }
}

// ---------------------------------------------------------------------------
// CodePlanner
// ---------------------------------------------------------------------------

/// The code implementation planner.
///
/// Generates structured [`ImplementationPlan`]s from high-level descriptions.
/// Plans include file-level granularity, dependency ordering, effort estimation,
/// risk assessment, and agent role assignment.
pub struct CodePlanner {
    config: PlannerConfig,
}

impl Default for CodePlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CodePlanner {
    /// Create a new planner with default configuration.
    pub fn new() -> Self {
        Self {
            config: PlannerConfig::default(),
        }
    }

    /// Create a new planner with the given configuration.
    pub fn with_config(config: PlannerConfig) -> Self {
        Self { config }
    }

    // ------------------------------------------------------------------
    // Public planning methods
    // ------------------------------------------------------------------

    /// Plan a new feature implementation.
    ///
    /// Generates steps to create and modify files based on the feature
    /// description and the list of existing files that may need updates.
    pub fn plan_feature(
        &self,
        title: &str,
        description: &str,
        existing_files: &[&str],
    ) -> ImplementationPlan {
        let mut steps = Vec::new();
        let mut order = 1;

        // Step 1: Design / interface (if this looks like a new module)
        let interface_file = Self::infer_interface_file(description, existing_files);
        steps.push(PlanStep {
            order,
            file: interface_file.clone(),
            operation: if existing_files.contains(&interface_file.as_str()) {
                FileOperation::Modify
            } else {
                FileOperation::Create
            },
            description: format!("Define interfaces and types for: {title}"),
            instructions: vec![
                "Define public trait or struct signatures".into(),
                "Add doc comments on all public items".into(),
                "Keep the API minimal — expose only what is needed".into(),
            ],
            depends_on: vec![],
            effort: Effort::Medium,
            assigned_role: AgentRole::Architect,
            breaks_api: false,
        });
        order += 1;

        // Step 2-N: Modify each existing file
        for file in existing_files {
            if order > self.config.max_steps {
                break;
            }
            steps.push(PlanStep {
                order,
                file: (*file).to_string(),
                operation: FileOperation::Modify,
                description: format!("Integrate feature '{title}' into {file}"),
                instructions: vec![
                    format!("Import new types from {interface_file}"),
                    "Wire the feature into existing logic".into(),
                    "Maintain backward compatibility".into(),
                ],
                depends_on: vec![1],
                effort: Self::estimate_step_effort(&FileOperation::Modify, description),
                assigned_role: Self::assign_role(&FileOperation::Modify, file),
                breaks_api: false,
            });
            order += 1;
        }

        // Test step
        if self.config.include_test_strategy && order <= self.config.max_steps {
            let test_file = Self::infer_test_file(&interface_file);
            steps.push(PlanStep {
                order,
                file: test_file,
                operation: FileOperation::Create,
                description: format!("Add tests for feature '{title}'"),
                instructions: vec![
                    "Cover happy path and error cases".into(),
                    "Test edge cases and boundary conditions".into(),
                ],
                depends_on: (1..order).collect(),
                effort: Effort::Medium,
                assigned_role: AgentRole::Tester,
                breaks_api: false,
            });
            order += 1;
        }

        // Review step
        if order <= self.config.max_steps {
            steps.push(PlanStep {
                order,
                file: String::new(),
                operation: FileOperation::Modify,
                description: "Code review of all changes".into(),
                instructions: vec![
                    "Verify public API consistency".into(),
                    "Check error handling is exhaustive".into(),
                    "Ensure no unwrap/expect in production code".into(),
                ],
                depends_on: (1..order).collect(),
                effort: Effort::Small,
                assigned_role: AgentRole::Reviewer,
                breaks_api: false,
            });
        }

        self.finalize_plan(title, TaskType::Feature, description, steps)
    }

    /// Plan a bug fix.
    ///
    /// Generates a focused plan: investigate the error, apply a fix, add a
    /// regression test, and review the change.
    pub fn plan_bugfix(
        &self,
        title: &str,
        error_message: &str,
        affected_files: &[&str],
    ) -> ImplementationPlan {
        let mut steps = Vec::new();
        let mut order = 1;
        let description = format!("Fix: {error_message}");

        // Step 1: Investigate
        let primary_file = affected_files.first().map_or("unknown".into(), |f| (*f).to_string());
        steps.push(PlanStep {
            order,
            file: primary_file.clone(),
            operation: FileOperation::Modify,
            description: format!("Investigate root cause in {primary_file}"),
            instructions: vec![
                format!("Reproduce the error: {error_message}"),
                "Add logging or debug prints to narrow down the issue".into(),
                "Identify the exact line / condition that triggers the bug".into(),
            ],
            depends_on: vec![],
            effort: Effort::Small,
            assigned_role: AgentRole::Debugger,
            breaks_api: false,
        });
        order += 1;

        // Step 2: Fix each affected file
        for file in affected_files {
            if order > self.config.max_steps {
                break;
            }
            steps.push(PlanStep {
                order,
                file: (*file).to_string(),
                operation: FileOperation::Modify,
                description: format!("Apply fix in {file}"),
                instructions: vec![
                    "Fix the identified root cause".into(),
                    "Handle edge cases that were missed".into(),
                    "Remove any debug logging added during investigation".into(),
                ],
                depends_on: vec![1],
                effort: Self::estimate_step_effort(&FileOperation::Modify, &description),
                assigned_role: AgentRole::Implementer,
                breaks_api: false,
            });
            order += 1;
        }

        // Step 3: Regression test
        if self.config.include_test_strategy && order <= self.config.max_steps {
            let test_file = Self::infer_test_file(&primary_file);
            steps.push(PlanStep {
                order,
                file: test_file,
                operation: FileOperation::Modify,
                description: "Add regression test for the bug".into(),
                instructions: vec![
                    format!("Write a test that reproduces: {error_message}"),
                    "Verify the fix resolves the issue".into(),
                ],
                depends_on: (2..order).collect(),
                effort: Effort::Small,
                assigned_role: AgentRole::Tester,
                breaks_api: false,
            });
            order += 1;
        }

        // Step 4: Review
        if order <= self.config.max_steps {
            steps.push(PlanStep {
                order,
                file: String::new(),
                operation: FileOperation::Modify,
                description: "Review the bug fix".into(),
                instructions: vec![
                    "Verify the fix is minimal and correct".into(),
                    "Ensure no regressions in adjacent code".into(),
                ],
                depends_on: (1..order).collect(),
                effort: Effort::Small,
                assigned_role: AgentRole::Reviewer,
                breaks_api: false,
            });
        }

        self.finalize_plan(title, TaskType::BugFix, &description, steps)
    }

    /// Plan a refactoring.
    ///
    /// Generates steps to identify scope, perform the refactoring, update
    /// references, update tests, and review.
    pub fn plan_refactor(
        &self,
        title: &str,
        target: &str,
        goal: &str,
        affected_files: &[&str],
    ) -> ImplementationPlan {
        let mut steps = Vec::new();
        let mut order = 1;
        let description = format!("Refactor {target}: {goal}");

        // Step 1: Identify scope
        steps.push(PlanStep {
            order,
            file: target.to_string(),
            operation: FileOperation::Modify,
            description: format!("Identify refactoring scope in {target}"),
            instructions: vec![
                format!("Goal: {goal}"),
                "Map all usages of the target across the codebase".into(),
                "Determine which files will need corresponding updates".into(),
            ],
            depends_on: vec![],
            effort: Effort::Small,
            assigned_role: AgentRole::Architect,
            breaks_api: false,
        });
        order += 1;

        // Step 2: Perform the refactoring on each affected file
        for file in affected_files {
            if order > self.config.max_steps {
                break;
            }
            let is_target = *file == target;
            steps.push(PlanStep {
                order,
                file: (*file).to_string(),
                operation: FileOperation::Modify,
                description: if is_target {
                    format!("Apply refactoring to primary target: {file}")
                } else {
                    format!("Update references in {file}")
                },
                instructions: if is_target {
                    vec![
                        format!("Refactor according to goal: {goal}"),
                        "Preserve external behavior".into(),
                    ]
                } else {
                    vec![
                        "Update imports and references to match refactored code".into(),
                        "Verify compilation after changes".into(),
                    ]
                },
                depends_on: vec![1],
                effort: Self::estimate_step_effort(&FileOperation::Modify, &description),
                assigned_role: AgentRole::Implementer,
                breaks_api: is_target,
            });
            order += 1;
        }

        // Step 3: Update tests
        if self.config.include_test_strategy && order <= self.config.max_steps {
            let test_file = Self::infer_test_file(target);
            steps.push(PlanStep {
                order,
                file: test_file,
                operation: FileOperation::Modify,
                description: "Update tests after refactoring".into(),
                instructions: vec![
                    "Update test imports and assertions to reflect the refactored API".into(),
                    "Add tests for any new abstractions introduced".into(),
                ],
                depends_on: (2..order).collect(),
                effort: Effort::Medium,
                assigned_role: AgentRole::Tester,
                breaks_api: false,
            });
            order += 1;
        }

        // Step 4: Review
        if order <= self.config.max_steps {
            steps.push(PlanStep {
                order,
                file: String::new(),
                operation: FileOperation::Modify,
                description: "Review refactoring changes".into(),
                instructions: vec![
                    "Verify behavior is preserved".into(),
                    "Check for dead code left behind".into(),
                    "Ensure naming consistency".into(),
                ],
                depends_on: (1..order).collect(),
                effort: Effort::Small,
                assigned_role: AgentRole::Reviewer,
                breaks_api: false,
            });
        }

        self.finalize_plan(title, TaskType::Refactor, &description, steps)
    }

    /// Plan adding test coverage.
    ///
    /// For each source file, generates a step to create or update its test file.
    pub fn plan_tests(&self, title: &str, source_files: &[&str]) -> ImplementationPlan {
        let mut steps = Vec::new();
        let mut order = 1;

        for file in source_files {
            if order > self.config.max_steps {
                break;
            }
            let test_file = Self::infer_test_file(file);
            steps.push(PlanStep {
                order,
                file: test_file,
                operation: FileOperation::Create,
                description: format!("Write tests for {file}"),
                instructions: vec![
                    format!("Read and understand the public API of {file}"),
                    "Write unit tests for each public function and method".into(),
                    "Cover edge cases: empty inputs, boundary values, error paths".into(),
                    "Use descriptive test names that explain the scenario".into(),
                ],
                depends_on: vec![],
                effort: Effort::Medium,
                assigned_role: AgentRole::Tester,
                breaks_api: false,
            });
            order += 1;
        }

        // Run tests step
        if order <= self.config.max_steps {
            steps.push(PlanStep {
                order,
                file: String::new(),
                operation: FileOperation::Modify,
                description: "Run test suite and verify coverage".into(),
                instructions: vec![
                    "Run `cargo test` and verify all tests pass".into(),
                    "Check coverage report if available".into(),
                ],
                depends_on: (1..order).collect(),
                effort: Effort::Small,
                assigned_role: AgentRole::Tester,
                breaks_api: false,
            });
        }

        let description = format!("Add test coverage for {} source file(s)", source_files.len());
        self.finalize_plan(title, TaskType::AddTests, &description, steps)
    }

    // ------------------------------------------------------------------
    // Validation and analysis
    // ------------------------------------------------------------------

    /// Validate a plan: check dependencies form a DAG with no cycles and all
    /// referenced dependencies exist.
    ///
    /// Returns `Ok(())` if the plan is valid, or `Err` with a description of
    /// the problem.
    pub fn validate_plan(&self, plan: &ImplementationPlan) -> Result<(), String> {
        let step_orders: HashSet<usize> = plan.steps.iter().map(|s| s.order).collect();

        // Check all dependencies reference existing steps
        for step in &plan.steps {
            for dep in &step.depends_on {
                if !step_orders.contains(dep) {
                    return Err(format!(
                        "Step {} depends on step {dep}, which does not exist",
                        step.order
                    ));
                }
                if *dep == step.order {
                    return Err(format!("Step {} depends on itself", step.order));
                }
            }
        }

        // Cycle detection via topological sort (Kahn's algorithm)
        let mut in_degree: HashMap<usize, usize> = HashMap::new();
        let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();

        for step in &plan.steps {
            in_degree.entry(step.order).or_insert(0);
            adjacency.entry(step.order).or_default();
            for dep in &step.depends_on {
                adjacency.entry(*dep).or_default().push(step.order);
                *in_degree.entry(step.order).or_insert(0) += 1;
            }
        }

        let mut queue: Vec<usize> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(order, _)| *order)
            .collect();

        let mut visited = 0usize;

        while let Some(node) = queue.pop() {
            visited += 1;
            if let Some(neighbors) = adjacency.get(&node) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push(*neighbor);
                        }
                    }
                }
            }
        }

        if visited != plan.steps.len() {
            return Err("Cycle detected in step dependencies".into());
        }

        Ok(())
    }

    /// Get groups of steps that can be executed in parallel.
    ///
    /// Returns a vec of "waves", where each wave is a vec of step order
    /// numbers that have no mutual dependencies and can run concurrently.
    pub fn parallelizable_steps(&self, plan: &ImplementationPlan) -> Vec<Vec<usize>> {
        let mut in_degree: HashMap<usize, usize> = HashMap::new();
        let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();

        for step in &plan.steps {
            in_degree.entry(step.order).or_insert(0);
            adjacency.entry(step.order).or_default();
            for dep in &step.depends_on {
                adjacency.entry(*dep).or_default().push(step.order);
                *in_degree.entry(step.order).or_insert(0) += 1;
            }
        }

        let mut waves: Vec<Vec<usize>> = Vec::new();

        loop {
            let mut wave: Vec<usize> = in_degree
                .iter()
                .filter(|(_, deg)| **deg == 0)
                .map(|(order, _)| *order)
                .collect();

            if wave.is_empty() {
                break;
            }

            wave.sort_unstable();

            for &node in &wave {
                in_degree.remove(&node);
                if let Some(neighbors) = adjacency.get(&node) {
                    for neighbor in neighbors {
                        if let Some(deg) = in_degree.get_mut(neighbor) {
                            *deg = deg.saturating_sub(1);
                        }
                    }
                }
            }

            waves.push(wave);
        }

        waves
    }

    // ------------------------------------------------------------------
    // Formatting
    // ------------------------------------------------------------------

    /// Format the plan as a Markdown document.
    pub fn format_as_markdown(&self, plan: &ImplementationPlan) -> String {
        let mut md = String::new();

        md.push_str(&format!("# {}\n\n", plan.title));
        md.push_str(&format!("**Type:** {}\n\n", plan.task_type));
        md.push_str(&format!("**Total Effort:** {}\n\n", plan.total_effort));
        md.push_str(&format!("**Risk:** {}\n\n", plan.risk.level));
        md.push_str(&format!("## Description\n\n{}\n\n", plan.description));

        // Steps
        md.push_str("## Steps\n\n");
        for step in &plan.steps {
            md.push_str(&format!(
                "### Step {} — {} (`{}`)\n\n",
                step.order, step.operation, step.file
            ));
            md.push_str(&format!("**Role:** {} | **Effort:** {}", step.assigned_role, step.effort));
            if step.breaks_api {
                md.push_str(" | **BREAKS API**");
            }
            md.push('\n');
            if !step.depends_on.is_empty() {
                let deps: Vec<String> = step.depends_on.iter().map(|d| d.to_string()).collect();
                md.push_str(&format!("**Depends on:** {}\n", deps.join(", ")));
            }
            md.push('\n');
            md.push_str(&format!("{}\n\n", step.description));
            for instruction in &step.instructions {
                md.push_str(&format!("- {instruction}\n"));
            }
            md.push('\n');
        }

        // Affected files
        md.push_str("## Affected Files\n\n");
        for file in &plan.affected_files {
            md.push_str(&format!("- `{file}`\n"));
        }
        md.push('\n');

        // Risk
        md.push_str("## Risk Assessment\n\n");
        md.push_str(&format!("**Overall:** {}\n\n", plan.risk.level));
        if !plan.risk.factors.is_empty() {
            md.push_str("**Factors:**\n\n");
            for factor in &plan.risk.factors {
                md.push_str(&format!("- [{}] {}\n", factor.level, factor.description));
            }
            md.push('\n');
        }
        if !plan.risk.mitigations.is_empty() {
            md.push_str("**Mitigations:**\n\n");
            for m in &plan.risk.mitigations {
                md.push_str(&format!("- {m}\n"));
            }
            md.push('\n');
        }

        // Test strategy
        if self.config.include_test_strategy {
            md.push_str("## Test Strategy\n\n");
            if !plan.test_strategy.test_types.is_empty() {
                md.push_str(&format!(
                    "**Test types:** {}\n\n",
                    plan.test_strategy.test_types.join(", ")
                ));
            }
            if let Some(target) = plan.test_strategy.coverage_target {
                md.push_str(&format!("**Coverage target:** {:.0}%\n\n", target * 100.0));
            }
            if !plan.test_strategy.test_files.is_empty() {
                md.push_str("**Test files:**\n\n");
                for tf in &plan.test_strategy.test_files {
                    md.push_str(&format!("- `{tf}`\n"));
                }
                md.push('\n');
            }
            if !plan.test_strategy.manual_checks.is_empty() {
                md.push_str("**Manual checks:**\n\n");
                for mc in &plan.test_strategy.manual_checks {
                    md.push_str(&format!("- {mc}\n"));
                }
                md.push('\n');
            }
        }

        // Rollback
        if self.config.include_rollback && !plan.rollback.is_empty() {
            md.push_str("## Rollback\n\n");
            md.push_str(&plan.rollback);
            md.push('\n');
        }

        md
    }

    /// Generate an LLM prompt for executing a specific step of the plan.
    ///
    /// Returns a structured prompt including context, instructions, and
    /// constraints for the step at `step_index` (0-based index into
    /// `plan.steps`).
    pub fn step_prompt(&self, plan: &ImplementationPlan, step_index: usize) -> String {
        let Some(step) = plan.steps.get(step_index) else {
            return format!("Error: step index {step_index} is out of range (plan has {} steps)", plan.steps.len());
        };

        let mut prompt = String::new();

        prompt.push_str(&format!(
            "# Task: {} — Step {}/{}\n\n",
            plan.title,
            step.order,
            plan.steps.len()
        ));
        prompt.push_str(&format!("## Context\n\n{}\n\n", plan.description));
        prompt.push_str(&format!("## Your Role\n\n{}\n\n", step.assigned_role));
        prompt.push_str(&format!(
            "## Operation\n\n**{}** `{}`\n\n",
            step.operation, step.file
        ));
        prompt.push_str(&format!("## Description\n\n{}\n\n", step.description));
        prompt.push_str("## Instructions\n\n");
        for (i, instruction) in step.instructions.iter().enumerate() {
            prompt.push_str(&format!("{}. {instruction}\n", i + 1));
        }
        prompt.push('\n');

        // Constraints
        prompt.push_str("## Constraints\n\n");
        prompt.push_str("- Do NOT use `.unwrap()` or `.expect()` in production code\n");
        prompt.push_str("- Add `///` doc comments on all public types and methods\n");
        prompt.push_str("- Follow Rust 2021 edition conventions\n");
        if step.breaks_api {
            prompt.push_str("- **WARNING:** This step modifies public API — update all downstream consumers\n");
        }

        // Dependencies
        if !step.depends_on.is_empty() {
            prompt.push_str("\n## Prerequisites\n\n");
            prompt.push_str("The following steps must be completed first:\n\n");
            for dep in &step.depends_on {
                if let Some(dep_step) = plan.steps.iter().find(|s| s.order == *dep) {
                    prompt.push_str(&format!("- Step {dep}: {}\n", dep_step.description));
                }
            }
        }

        prompt
    }

    /// Infer the [`TaskType`] from a description string using keyword matching.
    pub fn infer_task_type(description: &str) -> TaskType {
        let lower = description.to_lowercase();

        if lower.contains("security")
            || lower.contains("vulnerability")
            || lower.contains("cve")
            || lower.contains("harden")
        {
            return TaskType::SecurityFix;
        }

        if lower.contains("fix")
            || lower.contains("bug")
            || lower.contains("error")
            || lower.contains("crash")
            || lower.contains("panic")
        {
            return TaskType::BugFix;
        }

        if lower.contains("refactor")
            || lower.contains("rename")
            || lower.contains("extract")
            || lower.contains("reorganize")
            || lower.contains("clean up")
        {
            return TaskType::Refactor;
        }

        if lower.contains("test")
            || lower.contains("coverage")
            || lower.contains("spec")
        {
            return TaskType::AddTests;
        }

        if lower.contains("perf")
            || lower.contains("optimize")
            || lower.contains("speed")
            || lower.contains("benchmark")
            || lower.contains("slow")
        {
            return TaskType::Optimization;
        }

        if lower.contains("doc")
            || lower.contains("readme")
            || lower.contains("comment")
        {
            return TaskType::Documentation;
        }

        TaskType::Feature
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Determine the best agent role for a file operation.
    fn assign_role(operation: &FileOperation, file: &str) -> AgentRole {
        let lower = file.to_lowercase();

        // Test files
        if lower.ends_with("_test.rs")
            || lower.starts_with("test_")
            || lower.ends_with(".spec.ts")
            || lower.ends_with("_test.go")
            || lower.contains("/tests/")
        {
            return AgentRole::Tester;
        }

        // Security files
        if lower.contains("security")
            || lower.contains("auth")
            || lower.contains("crypto")
        {
            return AgentRole::SecurityAuditor;
        }

        // DevOps files
        if lower.contains("dockerfile")
            || lower.contains("helm/")
            || lower.contains("terraform/")
            || lower.ends_with(".yml")
            || lower.ends_with(".yaml")
            || lower.contains("ci/")
            || lower.contains(".github/")
        {
            return AgentRole::DevOps;
        }

        // Documentation files
        if lower.ends_with(".md")
            || lower.contains("readme")
            || lower.contains("docs/")
        {
            return AgentRole::Documenter;
        }

        match operation {
            FileOperation::Create => {
                // New module files → Architect, others → Implementer
                if lower.contains("mod.rs") || lower.contains("lib.rs") {
                    AgentRole::Architect
                } else {
                    AgentRole::Implementer
                }
            }
            FileOperation::Delete => AgentRole::Architect,
            FileOperation::Rename { .. } => AgentRole::Architect,
            FileOperation::Modify => AgentRole::Implementer,
        }
    }

    /// Estimate effort for a single step based on its operation and description.
    fn estimate_step_effort(operation: &FileOperation, description: &str) -> Effort {
        let word_count = description.split_whitespace().count();

        match operation {
            FileOperation::Delete => Effort::Small,
            FileOperation::Rename { .. } => Effort::Small,
            FileOperation::Create => {
                if word_count > 30 {
                    Effort::Large
                } else {
                    Effort::Medium
                }
            }
            FileOperation::Modify => {
                if word_count > 40 {
                    Effort::Large
                } else if word_count > 15 {
                    Effort::Medium
                } else {
                    Effort::Small
                }
            }
        }
    }

    /// Assess risk for a plan based on its steps.
    fn assess_risk(&self, steps: &[PlanStep]) -> RiskAssessment {
        let mut factors: Vec<RiskFactor> = Vec::new();
        let mut mitigations: Vec<String> = Vec::new();

        // Check for delete operations
        let has_deletes = steps.iter().any(|s| s.operation == FileOperation::Delete);
        if has_deletes {
            factors.push(RiskFactor {
                description: "Plan includes file deletions".into(),
                level: RiskLevel::High,
            });
            mitigations.push("Verify deleted files have no remaining references before removal".into());
        }

        // Check for API-breaking changes
        let api_breaks = steps.iter().filter(|s| s.breaks_api).count();
        if api_breaks > 0 {
            factors.push(RiskFactor {
                description: format!("{api_breaks} step(s) modify public API"),
                level: RiskLevel::High,
            });
            mitigations.push("Update all downstream consumers and add migration notes".into());
        }

        // Check file count
        let unique_files: HashSet<&str> = steps
            .iter()
            .filter(|s| !s.file.is_empty())
            .map(|s| s.file.as_str())
            .collect();
        if unique_files.len() > 5 {
            factors.push(RiskFactor {
                description: format!("Large blast radius: {} files affected", unique_files.len()),
                level: RiskLevel::Medium,
            });
            mitigations.push("Consider splitting into smaller, incremental changes".into());
        }

        // Check for security-sensitive files
        let touches_security = steps.iter().any(|s| {
            let lower = s.file.to_lowercase();
            lower.contains("auth") || lower.contains("security") || lower.contains("crypto")
        });
        if touches_security {
            factors.push(RiskFactor {
                description: "Changes touch security-sensitive files".into(),
                level: RiskLevel::High,
            });
            mitigations.push("Require security review before merging".into());
        }

        // Check dependency depth
        let max_deps = steps.iter().map(|s| s.depends_on.len()).max().unwrap_or(0);
        if max_deps > 5 {
            factors.push(RiskFactor {
                description: format!("High step interdependency (max {max_deps} dependencies)"),
                level: RiskLevel::Medium,
            });
            mitigations.push("Verify dependency ordering carefully before executing".into());
        }

        // Determine overall level
        let level = factors
            .iter()
            .map(|f| f.level)
            .max()
            .unwrap_or(RiskLevel::Low);

        // If no factors were added, it's low risk
        if factors.is_empty() {
            factors.push(RiskFactor {
                description: "No significant risk factors identified".into(),
                level: RiskLevel::Low,
            });
        }

        RiskAssessment {
            level,
            factors,
            mitigations,
        }
    }

    /// Determine total effort from the steps in a plan.
    fn estimate_total_effort(&self, steps: &[PlanStep]) -> Effort {
        let large_count = steps.iter().filter(|s| s.effort == Effort::Large).count();
        let medium_count = steps.iter().filter(|s| s.effort == Effort::Medium).count();

        if large_count > 0 || medium_count > 3 || steps.len() > 8 {
            Effort::Large
        } else if medium_count > 0 || steps.len() > 3 {
            Effort::Medium
        } else {
            Effort::Small
        }
    }

    /// Generate a test strategy based on the plan steps.
    fn generate_test_strategy(&self, steps: &[PlanStep]) -> TestStrategy {
        let mut test_types = vec!["unit".to_string()];
        let mut test_files = Vec::new();
        let mut manual_checks = Vec::new();

        // Identify test files from steps
        for step in steps {
            if step.assigned_role == AgentRole::Tester && !step.file.is_empty() {
                test_files.push(step.file.clone());
            }
        }

        // If there are multiple files, add integration tests
        let unique_source_files: HashSet<&str> = steps
            .iter()
            .filter(|s| !s.file.is_empty() && s.assigned_role != AgentRole::Tester)
            .map(|s| s.file.as_str())
            .collect();
        if unique_source_files.len() > 1 {
            test_types.push("integration".to_string());
        }

        // If touching API, add manual checks
        let has_api_change = steps.iter().any(|s| s.breaks_api);
        if has_api_change {
            manual_checks.push("Verify API backward compatibility".into());
            manual_checks.push("Check documentation reflects new API".into());
        }

        // If touching security files, add security testing
        let touches_security = steps.iter().any(|s| {
            let lower = s.file.to_lowercase();
            lower.contains("auth") || lower.contains("security")
        });
        if touches_security {
            test_types.push("security".to_string());
            manual_checks.push("Run security audit tools".into());
        }

        // Coverage target heuristic
        let coverage_target = if test_files.is_empty() {
            None
        } else {
            Some(0.80)
        };

        TestStrategy {
            test_types,
            test_files,
            coverage_target,
            manual_checks,
        }
    }

    /// Generate rollback instructions from the plan steps.
    fn generate_rollback(&self, steps: &[PlanStep]) -> String {
        let mut rollback_lines = Vec::new();

        rollback_lines.push("To rollback this change:".to_string());
        rollback_lines.push(String::new());
        rollback_lines.push("1. `git revert <commit-hash>` to revert all changes.".into());

        // Specific rollback for created files
        let created_files: Vec<&str> = steps
            .iter()
            .filter(|s| s.operation == FileOperation::Create)
            .map(|s| s.file.as_str())
            .collect();
        if !created_files.is_empty() {
            rollback_lines.push(format!(
                "2. Delete newly created files: {}",
                created_files.join(", ")
            ));
        }

        // Specific rollback for renames
        let renames: Vec<(&str, &str)> = steps
            .iter()
            .filter_map(|s| {
                if let FileOperation::Rename { new_path } = &s.operation {
                    Some((new_path.as_str(), s.file.as_str()))
                } else {
                    None
                }
            })
            .collect();
        for (new, old) in &renames {
            rollback_lines.push(format!("3. Rename `{new}` back to `{old}`."));
        }

        // Specific rollback for deletes
        let deleted_files: Vec<&str> = steps
            .iter()
            .filter(|s| s.operation == FileOperation::Delete)
            .map(|s| s.file.as_str())
            .collect();
        if !deleted_files.is_empty() {
            rollback_lines.push(format!(
                "4. Restore deleted files from git: `git checkout HEAD -- {}`",
                deleted_files.join(" ")
            ));
        }

        rollback_lines.join("\n")
    }

    /// Infer the most likely interface/types file for a feature.
    fn infer_interface_file(description: &str, existing_files: &[&str]) -> String {
        // If any existing file looks like a types/model file, use it
        for file in existing_files {
            let lower = file.to_lowercase();
            if lower.contains("types") || lower.contains("model") || lower.contains("lib") {
                return (*file).to_string();
            }
        }

        // Otherwise, infer from the description
        let slug: String = description
            .to_lowercase()
            .split_whitespace()
            .take(3)
            .collect::<Vec<&str>>()
            .join("_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if slug.is_empty() {
            "src/types.rs".to_string()
        } else {
            format!("src/{slug}.rs")
        }
    }

    /// Infer the test file path for a given source file.
    fn infer_test_file(source: &str) -> String {
        if source.is_empty() {
            return "tests/test.rs".to_string();
        }

        // If file is in src/, put tests in tests/
        if let Some(name) = source.strip_prefix("src/") {
            let stem = name.trim_end_matches(".rs");
            return format!("tests/{stem}_test.rs");
        }

        // If file already looks like a test, return as-is
        if source.contains("test") {
            return source.to_string();
        }

        // Generic fallback
        let stem = source
            .rsplit('/')
            .next()
            .unwrap_or(source)
            .trim_end_matches(".rs");
        format!("tests/{stem}_test.rs")
    }

    /// Finalize a plan by computing risk, effort, test strategy, and affected files.
    fn finalize_plan(
        &self,
        title: &str,
        task_type: TaskType,
        description: &str,
        mut steps: Vec<PlanStep>,
    ) -> ImplementationPlan {
        // Truncate to max_steps
        steps.truncate(self.config.max_steps);

        let risk = self.assess_risk(&steps);
        let total_effort = self.estimate_total_effort(&steps);
        let test_strategy = if self.config.include_test_strategy {
            self.generate_test_strategy(&steps)
        } else {
            TestStrategy {
                test_types: vec![],
                test_files: vec![],
                coverage_target: None,
                manual_checks: vec![],
            }
        };
        let rollback = if self.config.include_rollback {
            self.generate_rollback(&steps)
        } else {
            String::new()
        };

        // Collect affected files (deduplicated, ordered)
        let mut seen = HashSet::new();
        let affected_files: Vec<String> = steps
            .iter()
            .filter(|s| !s.file.is_empty())
            .filter_map(|s| {
                if seen.insert(s.file.clone()) {
                    Some(s.file.clone())
                } else {
                    None
                }
            })
            .collect();

        ImplementationPlan {
            title: title.to_string(),
            task_type,
            description: description.to_string(),
            steps,
            affected_files,
            risk,
            total_effort,
            test_strategy,
            rollback,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_feature_basic() {
        let planner = CodePlanner::new();
        let plan = planner.plan_feature("Add retry logic", "Implement retries for HTTP", &[]);
        assert_eq!(plan.task_type, TaskType::Feature);
        assert!(!plan.steps.is_empty());
        assert_eq!(plan.title, "Add retry logic");
    }

    #[test]
    fn test_plan_feature_with_tests() {
        let planner = CodePlanner::new();
        let plan = planner.plan_feature(
            "Add caching",
            "Add response caching layer",
            &["src/http.rs"],
        );
        // Should have: interface + modify existing + test + review = 4 steps
        assert!(plan.steps.len() >= 3);
        let has_tester = plan.steps.iter().any(|s| s.assigned_role == AgentRole::Tester);
        assert!(has_tester, "Feature plan should include a testing step");
    }

    #[test]
    fn test_plan_bugfix() {
        let planner = CodePlanner::new();
        let plan = planner.plan_bugfix(
            "Fix null pointer",
            "NullPointerException in UserService.getUser()",
            &["src/user_service.rs"],
        );
        assert_eq!(plan.task_type, TaskType::BugFix);
        assert!(plan.steps.len() >= 3, "Bug fix should have investigate, fix, test, and review steps");
        let has_debugger = plan.steps.iter().any(|s| s.assigned_role == AgentRole::Debugger);
        assert!(has_debugger, "Bug fix should include an investigation step");
    }

    #[test]
    fn test_plan_refactor() {
        let planner = CodePlanner::new();
        let plan = planner.plan_refactor(
            "Extract HTTP module",
            "src/main.rs",
            "Move HTTP logic to its own module",
            &["src/main.rs", "src/lib.rs"],
        );
        assert_eq!(plan.task_type, TaskType::Refactor);
        assert!(!plan.steps.is_empty());
        let has_architect = plan.steps.iter().any(|s| s.assigned_role == AgentRole::Architect);
        assert!(has_architect, "Refactor should include architecture analysis");
    }

    #[test]
    fn test_plan_tests() {
        let planner = CodePlanner::new();
        let plan = planner.plan_tests(
            "Add tests for auth module",
            &["src/auth.rs", "src/session.rs"],
        );
        assert_eq!(plan.task_type, TaskType::AddTests);
        // One test step per source file + run step
        assert!(plan.steps.len() >= 2);
        assert!(plan.steps.iter().all(|s| s.assigned_role == AgentRole::Tester));
    }

    #[test]
    fn test_infer_task_type_feature() {
        assert_eq!(CodePlanner::infer_task_type("Add new dashboard widget"), TaskType::Feature);
        assert_eq!(CodePlanner::infer_task_type("Implement user profile page"), TaskType::Feature);
    }

    #[test]
    fn test_infer_task_type_bugfix() {
        assert_eq!(CodePlanner::infer_task_type("Fix login error"), TaskType::BugFix);
        assert_eq!(CodePlanner::infer_task_type("Bug in payment processing"), TaskType::BugFix);
        assert_eq!(CodePlanner::infer_task_type("Application crashes on startup"), TaskType::BugFix);
    }

    #[test]
    fn test_infer_task_type_refactor() {
        assert_eq!(CodePlanner::infer_task_type("Refactor database layer"), TaskType::Refactor);
        assert_eq!(CodePlanner::infer_task_type("Rename UserService to AccountService"), TaskType::Refactor);
        assert_eq!(CodePlanner::infer_task_type("Extract common utilities"), TaskType::Refactor);
    }

    #[test]
    fn test_infer_task_type_security() {
        assert_eq!(CodePlanner::infer_task_type("Fix security vulnerability"), TaskType::SecurityFix);
        assert_eq!(CodePlanner::infer_task_type("Harden authentication"), TaskType::SecurityFix);
        assert_eq!(CodePlanner::infer_task_type("Patch CVE-2024-1234"), TaskType::SecurityFix);
    }

    #[test]
    fn test_assign_role_test_file() {
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "src/auth_test.rs"),
            AgentRole::Tester
        );
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "test_utils.rs"),
            AgentRole::Tester
        );
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Create, "app.spec.ts"),
            AgentRole::Tester
        );
    }

    #[test]
    fn test_assign_role_security_file() {
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "src/security/auth.rs"),
            AgentRole::SecurityAuditor
        );
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "crypto_utils.rs"),
            AgentRole::SecurityAuditor
        );
    }

    #[test]
    fn test_assign_role_devops_file() {
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "Dockerfile"),
            AgentRole::DevOps
        );
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "deploy/helm/values.yaml"),
            AgentRole::DevOps
        );
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, ".github/workflows/ci.yml"),
            AgentRole::DevOps
        );
    }

    #[test]
    fn test_assign_role_docs_file() {
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Modify, "README.md"),
            AgentRole::Documenter
        );
        assert_eq!(
            CodePlanner::assign_role(&FileOperation::Create, "docs/api.md"),
            AgentRole::Documenter
        );
    }

    #[test]
    fn test_assess_risk_low() {
        let planner = CodePlanner::new();
        let steps = vec![PlanStep {
            order: 1,
            file: "src/new_feature.rs".into(),
            operation: FileOperation::Create,
            description: "Create a new utility file".into(),
            instructions: vec![],
            depends_on: vec![],
            effort: Effort::Small,
            assigned_role: AgentRole::Implementer,
            breaks_api: false,
        }];
        let risk = planner.assess_risk(&steps);
        assert_eq!(risk.level, RiskLevel::Low);
    }

    #[test]
    fn test_assess_risk_high_delete() {
        let planner = CodePlanner::new();
        let steps = vec![PlanStep {
            order: 1,
            file: "src/old_module.rs".into(),
            operation: FileOperation::Delete,
            description: "Remove deprecated module".into(),
            instructions: vec![],
            depends_on: vec![],
            effort: Effort::Small,
            assigned_role: AgentRole::Architect,
            breaks_api: false,
        }];
        let risk = planner.assess_risk(&steps);
        assert_eq!(risk.level, RiskLevel::High);
    }

    #[test]
    fn test_assess_risk_high_api_change() {
        let planner = CodePlanner::new();
        let steps = vec![PlanStep {
            order: 1,
            file: "src/api.rs".into(),
            operation: FileOperation::Modify,
            description: "Change public function signature".into(),
            instructions: vec![],
            depends_on: vec![],
            effort: Effort::Medium,
            assigned_role: AgentRole::Implementer,
            breaks_api: true,
        }];
        let risk = planner.assess_risk(&steps);
        assert_eq!(risk.level, RiskLevel::High);
    }

    #[test]
    fn test_validate_plan_valid() {
        let planner = CodePlanner::new();
        let plan = planner.plan_feature("Test feature", "A simple feature", &["src/main.rs"]);
        let result = planner.validate_plan(&plan);
        assert!(result.is_ok(), "Valid plan should pass validation: {result:?}");
    }

    #[test]
    fn test_validate_plan_cycle_detection() {
        let planner = CodePlanner::new();
        let plan = ImplementationPlan {
            title: "Cyclic plan".into(),
            task_type: TaskType::Feature,
            description: "A plan with cycles".into(),
            steps: vec![
                PlanStep {
                    order: 1,
                    file: "a.rs".into(),
                    operation: FileOperation::Create,
                    description: "Step 1".into(),
                    instructions: vec![],
                    depends_on: vec![2],
                    effort: Effort::Small,
                    assigned_role: AgentRole::Implementer,
                    breaks_api: false,
                },
                PlanStep {
                    order: 2,
                    file: "b.rs".into(),
                    operation: FileOperation::Create,
                    description: "Step 2".into(),
                    instructions: vec![],
                    depends_on: vec![1],
                    effort: Effort::Small,
                    assigned_role: AgentRole::Implementer,
                    breaks_api: false,
                },
            ],
            affected_files: vec!["a.rs".into(), "b.rs".into()],
            risk: RiskAssessment {
                level: RiskLevel::Low,
                factors: vec![],
                mitigations: vec![],
            },
            total_effort: Effort::Small,
            test_strategy: TestStrategy {
                test_types: vec![],
                test_files: vec![],
                coverage_target: None,
                manual_checks: vec![],
            },
            rollback: String::new(),
        };
        let result = planner.validate_plan(&plan);
        assert!(result.is_err(), "Plan with cycles should fail validation");
        assert!(
            result.unwrap_err().contains("Cycle"),
            "Error should mention cycle"
        );
    }

    #[test]
    fn test_validate_plan_missing_dependency() {
        let planner = CodePlanner::new();
        let plan = ImplementationPlan {
            title: "Missing dep".into(),
            task_type: TaskType::Feature,
            description: "A plan with missing dependency".into(),
            steps: vec![PlanStep {
                order: 1,
                file: "a.rs".into(),
                operation: FileOperation::Create,
                description: "Step 1".into(),
                instructions: vec![],
                depends_on: vec![99],
                effort: Effort::Small,
                assigned_role: AgentRole::Implementer,
                breaks_api: false,
            }],
            affected_files: vec!["a.rs".into()],
            risk: RiskAssessment {
                level: RiskLevel::Low,
                factors: vec![],
                mitigations: vec![],
            },
            total_effort: Effort::Small,
            test_strategy: TestStrategy {
                test_types: vec![],
                test_files: vec![],
                coverage_target: None,
                manual_checks: vec![],
            },
            rollback: String::new(),
        };
        let result = planner.validate_plan(&plan);
        assert!(result.is_err(), "Plan with missing dependency should fail");
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_parallelizable_steps() {
        let planner = CodePlanner::new();
        let plan = ImplementationPlan {
            title: "Parallel test".into(),
            task_type: TaskType::Feature,
            description: "Test parallelism".into(),
            steps: vec![
                PlanStep {
                    order: 1,
                    file: "a.rs".into(),
                    operation: FileOperation::Create,
                    description: "Step 1".into(),
                    instructions: vec![],
                    depends_on: vec![],
                    effort: Effort::Small,
                    assigned_role: AgentRole::Implementer,
                    breaks_api: false,
                },
                PlanStep {
                    order: 2,
                    file: "b.rs".into(),
                    operation: FileOperation::Create,
                    description: "Step 2".into(),
                    instructions: vec![],
                    depends_on: vec![],
                    effort: Effort::Small,
                    assigned_role: AgentRole::Implementer,
                    breaks_api: false,
                },
                PlanStep {
                    order: 3,
                    file: "c.rs".into(),
                    operation: FileOperation::Create,
                    description: "Step 3 (depends on 1 and 2)".into(),
                    instructions: vec![],
                    depends_on: vec![1, 2],
                    effort: Effort::Small,
                    assigned_role: AgentRole::Implementer,
                    breaks_api: false,
                },
            ],
            affected_files: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()],
            risk: RiskAssessment {
                level: RiskLevel::Low,
                factors: vec![],
                mitigations: vec![],
            },
            total_effort: Effort::Small,
            test_strategy: TestStrategy {
                test_types: vec![],
                test_files: vec![],
                coverage_target: None,
                manual_checks: vec![],
            },
            rollback: String::new(),
        };

        let waves = planner.parallelizable_steps(&plan);
        assert_eq!(waves.len(), 2, "Should have 2 waves: [1,2] then [3]");
        assert_eq!(waves[0], vec![1, 2]);
        assert_eq!(waves[1], vec![3]);
    }

    #[test]
    fn test_format_as_markdown() {
        let planner = CodePlanner::new();
        let plan = planner.plan_feature("Markdown test", "Test markdown generation", &[]);
        let md = planner.format_as_markdown(&plan);

        assert!(md.contains("# Markdown test"), "Should contain title");
        assert!(md.contains("**Type:** Feature"), "Should contain task type");
        assert!(md.contains("## Steps"), "Should contain steps section");
        assert!(md.contains("## Risk Assessment"), "Should contain risk section");
    }

    #[test]
    fn test_step_prompt() {
        let planner = CodePlanner::new();
        let plan = planner.plan_feature("Prompt test", "Test prompt generation", &[]);
        let prompt = planner.step_prompt(&plan, 0);

        assert!(prompt.contains("Prompt test"), "Should contain plan title");
        assert!(prompt.contains("## Instructions"), "Should contain instructions");
        assert!(prompt.contains("## Constraints"), "Should contain constraints");
        assert!(prompt.contains(".unwrap()"), "Should mention unwrap constraint");

        // Out-of-range index
        let error_prompt = planner.step_prompt(&plan, 999);
        assert!(error_prompt.contains("Error"), "Should return error for invalid index");
    }

    #[test]
    fn test_estimate_effort() {
        assert_eq!(
            CodePlanner::estimate_step_effort(&FileOperation::Delete, "Remove unused file"),
            Effort::Small,
        );
        assert_eq!(
            CodePlanner::estimate_step_effort(&FileOperation::Rename { new_path: "new.rs".into() }, "Rename module"),
            Effort::Small,
        );
        assert_eq!(
            CodePlanner::estimate_step_effort(&FileOperation::Create, "Create a new file with some code"),
            Effort::Medium,
        );
        // Large: Create with a very long description (>30 words)
        let long_desc = "word ".repeat(35);
        assert_eq!(
            CodePlanner::estimate_step_effort(&FileOperation::Create, &long_desc),
            Effort::Large,
        );
    }

    #[test]
    fn test_planner_config_defaults() {
        let config = PlannerConfig::default();
        assert_eq!(config.max_steps, 20);
        assert!(config.include_rollback);
        assert!(config.include_test_strategy);

        // CodePlanner::new() should use defaults
        let planner = CodePlanner::new();
        assert_eq!(planner.config.max_steps, 20);
    }
}
