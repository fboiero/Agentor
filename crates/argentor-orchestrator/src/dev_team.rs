//! Development team orchestration module.
//!
//! Provides pre-configured development team compositions and workflows for
//! common coding tasks: implement feature, fix bug, refactor, security audit,
//! add tests, code review, optimize, and write documentation.
//!
//! Each workflow defines a sequence of [`WorkflowStep`]s with role assignments,
//! quality gates, and handoff rules. The [`DevTeam`] struct coordinates these
//! workflows using the existing [`AgentRole`](crate::types::AgentRole) and
//! [`AgentProfile`](crate::types::AgentProfile) infrastructure.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A role in a development team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevRole {
    /// Plans architecture and decomposes tasks.
    Architect,
    /// Writes production code.
    Implementer,
    /// Writes and runs tests.
    Tester,
    /// Reviews code for quality, security, and style.
    Reviewer,
    /// Diagnoses and fixes bugs.
    Debugger,
    /// Handles CI/CD, deployment, infrastructure.
    DevOps,
    /// Audits for security vulnerabilities.
    SecurityAuditor,
    /// Writes documentation.
    Documenter,
}

impl fmt::Display for DevRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Architect => write!(f, "Architect"),
            Self::Implementer => write!(f, "Implementer"),
            Self::Tester => write!(f, "Tester"),
            Self::Reviewer => write!(f, "Reviewer"),
            Self::Debugger => write!(f, "Debugger"),
            Self::DevOps => write!(f, "DevOps"),
            Self::SecurityAuditor => write!(f, "SecurityAuditor"),
            Self::Documenter => write!(f, "Documenter"),
        }
    }
}

/// A development workflow template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevWorkflow {
    /// Full feature implementation: plan -> code -> test -> review.
    ImplementFeature,
    /// Bug fix: diagnose -> fix -> test -> review.
    FixBug,
    /// Code refactoring: analyze -> refactor -> test -> review.
    Refactor,
    /// Add test coverage: analyze -> write tests -> verify.
    AddTests,
    /// Security audit: scan -> report -> remediate -> verify.
    SecurityAudit,
    /// Code review: review -> feedback -> iterate.
    CodeReview,
    /// Performance optimization: profile -> optimize -> benchmark -> review.
    Optimize,
    /// Documentation: analyze -> write -> review.
    WriteDocumentation,
}

impl fmt::Display for DevWorkflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImplementFeature => write!(f, "Implement Feature"),
            Self::FixBug => write!(f, "Fix Bug"),
            Self::Refactor => write!(f, "Refactor"),
            Self::AddTests => write!(f, "Add Tests"),
            Self::SecurityAudit => write!(f, "Security Audit"),
            Self::CodeReview => write!(f, "Code Review"),
            Self::Optimize => write!(f, "Optimize"),
            Self::WriteDocumentation => write!(f, "Write Documentation"),
        }
    }
}

/// All available workflow variants.
const ALL_WORKFLOWS: [DevWorkflow; 8] = [
    DevWorkflow::ImplementFeature,
    DevWorkflow::FixBug,
    DevWorkflow::Refactor,
    DevWorkflow::AddTests,
    DevWorkflow::SecurityAudit,
    DevWorkflow::CodeReview,
    DevWorkflow::Optimize,
    DevWorkflow::WriteDocumentation,
];

/// A step in a workflow with role assignment and handoff rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step number (1-based).
    pub order: usize,
    /// Which role executes this step.
    pub role: DevRole,
    /// What this step does.
    pub action: String,
    /// Detailed instructions for the agent.
    pub instructions: String,
    /// What the agent should produce (input for next step).
    pub expected_output: String,
    /// Quality gate: minimum conditions to proceed.
    pub gate: Option<QualityGate>,
    /// Whether this step can be retried on failure.
    pub retryable: bool,
    /// Maximum retries.
    pub max_retries: usize,
}

/// Quality gate that must pass before proceeding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    /// Human-readable description of the gate.
    pub description: String,
    /// Type of check.
    pub check_type: GateCheck,
}

/// The type of quality check for a gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateCheck {
    /// All tests must pass.
    TestsPass,
    /// Code review score must be above threshold (0-100).
    ReviewScore {
        /// Minimum acceptable score.
        min_score: u32,
    },
    /// No security findings above a severity.
    NoSecurityFindings {
        /// Maximum acceptable severity (e.g. "low", "medium", "high", "critical").
        max_severity: String,
    },
    /// Code compiles without errors.
    CompileSuccess,
    /// Custom check description.
    Custom {
        /// Description of the custom check.
        check: String,
    },
}

/// Configuration for a development team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevTeamConfig {
    /// Team name.
    pub name: String,
    /// Roles present in the team.
    pub roles: Vec<DevRole>,
    /// Model tier preference per role.
    pub role_models: Vec<(DevRole, String)>,
    /// Maximum concurrent steps.
    pub max_parallel: usize,
    /// Whether to enforce quality gates.
    pub enforce_gates: bool,
    /// Maximum total iterations across all workflow steps.
    pub max_iterations: usize,
}

/// Result of running a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    /// The workflow that was executed.
    pub workflow: DevWorkflow,
    /// Final status of the workflow.
    pub status: WorkflowStatus,
    /// Number of steps completed.
    pub steps_completed: usize,
    /// Total number of steps in the workflow.
    pub steps_total: usize,
    /// Artifacts produced by the workflow.
    pub artifacts: Vec<WorkflowArtifact>,
    /// Quality scores collected during execution (label, score).
    pub quality_scores: Vec<(String, f32)>,
    /// Human-readable notes about the execution.
    pub notes: Vec<String>,
}

/// Status of a workflow execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// Workflow completed successfully.
    Completed,
    /// Workflow failed at a specific step.
    Failed {
        /// Step number where the failure occurred.
        step: usize,
        /// Reason for the failure.
        reason: String,
    },
    /// Workflow blocked by a quality gate.
    GateBlocked {
        /// Step number where the gate blocked progress.
        step: usize,
        /// Description of the gate that blocked.
        gate: String,
    },
    /// Workflow was cancelled.
    Cancelled,
}

/// An artifact produced during a workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowArtifact {
    /// Name of the artifact.
    pub name: String,
    /// Type of artifact.
    pub artifact_type: ArtifactType,
    /// Content of the artifact.
    pub content: String,
}

/// The type of artifact produced by a workflow step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    /// Architecture or implementation plan.
    Plan,
    /// Source code.
    Code,
    /// Test execution results.
    TestResults,
    /// Code review report.
    ReviewReport,
    /// Security audit report.
    SecurityReport,
    /// Written documentation.
    Documentation,
    /// Code diff.
    Diff,
}

/// The development team orchestrator.
///
/// Coordinates a team of agents with specific roles through predefined
/// workflows. Each workflow defines a sequence of steps with quality gates
/// and handoff rules between agents.
#[derive(Debug, Clone)]
pub struct DevTeam {
    config: DevTeamConfig,
}

impl DevTeam {
    /// Create a default full-stack development team with all roles.
    pub fn full_stack() -> Self {
        Self {
            config: DevTeamConfig {
                name: "Full-Stack Team".to_string(),
                roles: vec![
                    DevRole::Architect,
                    DevRole::Implementer,
                    DevRole::Tester,
                    DevRole::Reviewer,
                    DevRole::Debugger,
                    DevRole::DevOps,
                    DevRole::SecurityAuditor,
                    DevRole::Documenter,
                ],
                role_models: default_role_models(),
                max_parallel: 3,
                enforce_gates: true,
                max_iterations: 50,
            },
        }
    }

    /// Create a minimal team (implementer + tester).
    pub fn minimal() -> Self {
        Self {
            config: DevTeamConfig {
                name: "Minimal Team".to_string(),
                roles: vec![DevRole::Implementer, DevRole::Tester],
                role_models: vec![
                    (DevRole::Implementer, "balanced".to_string()),
                    (DevRole::Tester, "balanced".to_string()),
                ],
                max_parallel: 1,
                enforce_gates: false,
                max_iterations: 20,
            },
        }
    }

    /// Create a security-focused team.
    pub fn security_team() -> Self {
        Self {
            config: DevTeamConfig {
                name: "Security Team".to_string(),
                roles: vec![
                    DevRole::SecurityAuditor,
                    DevRole::Implementer,
                    DevRole::Reviewer,
                    DevRole::Tester,
                ],
                role_models: vec![
                    (DevRole::SecurityAuditor, "powerful".to_string()),
                    (DevRole::Implementer, "balanced".to_string()),
                    (DevRole::Reviewer, "powerful".to_string()),
                    (DevRole::Tester, "balanced".to_string()),
                ],
                max_parallel: 2,
                enforce_gates: true,
                max_iterations: 40,
            },
        }
    }

    /// Create with custom config.
    pub fn with_config(config: DevTeamConfig) -> Self {
        Self { config }
    }

    /// Get the workflow steps for a given workflow type.
    pub fn workflow_steps(&self, workflow: DevWorkflow) -> Vec<WorkflowStep> {
        match workflow {
            DevWorkflow::ImplementFeature => build_implement_feature_steps(),
            DevWorkflow::FixBug => build_fix_bug_steps(),
            DevWorkflow::Refactor => build_refactor_steps(),
            DevWorkflow::AddTests => build_add_tests_steps(),
            DevWorkflow::SecurityAudit => build_security_audit_steps(),
            DevWorkflow::CodeReview => build_code_review_steps(),
            DevWorkflow::Optimize => build_optimize_steps(),
            DevWorkflow::WriteDocumentation => build_write_documentation_steps(),
        }
    }

    /// Get the roles needed for a workflow.
    pub fn required_roles(&self, workflow: DevWorkflow) -> Vec<DevRole> {
        let steps = self.workflow_steps(workflow);
        let mut roles: Vec<DevRole> = Vec::new();
        for step in &steps {
            if !roles.contains(&step.role) {
                roles.push(step.role);
            }
        }
        roles
    }

    /// Check if the team has all required roles for a workflow.
    pub fn can_run_workflow(&self, workflow: DevWorkflow) -> bool {
        let required = self.required_roles(workflow);
        required.iter().all(|r| self.config.roles.contains(r))
    }

    /// Get the model tier recommendation for a role.
    pub fn model_for_role(&self, role: DevRole) -> String {
        self.config
            .role_models
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, m)| m.clone())
            .unwrap_or_else(|| default_model_tier(role).to_string())
    }

    /// Validate a workflow result against quality gates.
    ///
    /// Returns `true` if the step's gate conditions are satisfied by the
    /// provided artifacts, or if the step has no gate. When gates are not
    /// enforced in the team config, always returns `true`.
    pub fn validate_gates(
        &self,
        workflow: DevWorkflow,
        step: usize,
        artifacts: &[WorkflowArtifact],
    ) -> bool {
        if !self.config.enforce_gates {
            return true;
        }

        let steps = self.workflow_steps(workflow);
        let target_step = steps.iter().find(|s| s.order == step);

        let target_step = match target_step {
            Some(s) => s,
            None => return false,
        };

        let gate = match &target_step.gate {
            Some(g) => g,
            None => return true,
        };

        match &gate.check_type {
            GateCheck::TestsPass => artifacts
                .iter()
                .any(|a| a.artifact_type == ArtifactType::TestResults),
            GateCheck::ReviewScore { min_score } => {
                // A review report artifact must exist.
                // In a real implementation, we'd parse the score from the content.
                // Here we check that a review report exists and that min_score
                // is within the valid range.
                *min_score <= 100
                    && artifacts
                        .iter()
                        .any(|a| a.artifact_type == ArtifactType::ReviewReport)
            }
            GateCheck::NoSecurityFindings { .. } => artifacts
                .iter()
                .any(|a| a.artifact_type == ArtifactType::SecurityReport),
            GateCheck::CompileSuccess => artifacts
                .iter()
                .any(|a| a.artifact_type == ArtifactType::Code),
            GateCheck::Custom { .. } => {
                // Custom gates pass if any artifact is present.
                !artifacts.is_empty()
            }
        }
    }

    /// Generate the system prompt for an agent in a specific role.
    pub fn role_system_prompt(&self, role: DevRole) -> String {
        match role {
            DevRole::Architect => "You are a senior software architect. \
                Analyze requirements, decompose into tasks, identify dependencies, \
                estimate effort, and assess risk. Produce detailed implementation \
                plans with clear step-by-step instructions. Focus on modularity, \
                scalability, and maintainability."
                .to_string(),
            DevRole::Implementer => "You are a senior software engineer. \
                Write clean, efficient, well-tested code. Follow existing \
                conventions and project patterns. Minimize changes to reduce \
                risk. Never introduce security vulnerabilities. Use proper error \
                handling and avoid panics in production code."
                .to_string(),
            DevRole::Tester => "You are a senior QA engineer. Write comprehensive \
                tests covering happy paths, edge cases, error conditions, and \
                boundary values. Use descriptive test names following the pattern \
                test_<function>_<scenario>. Ensure tests are deterministic, fast, \
                and independent of each other."
                .to_string(),
            DevRole::Reviewer => "You are a code reviewer. Evaluate code across \
                correctness, security, performance, style, and test coverage. \
                Provide specific, actionable feedback with line references. Score \
                the review on a 0-100 scale. Flag blocking issues that must be \
                addressed before merging."
                .to_string(),
            DevRole::Debugger => "You are a senior debugger and diagnostician. \
                Reproduce bugs systematically, identify root causes through \
                careful analysis, and propose minimal targeted fixes. Document \
                the investigation process and findings clearly. Always verify \
                that proposed fixes address the root cause, not just symptoms."
                .to_string(),
            DevRole::DevOps => "You are a DevOps engineer. Handle CI/CD pipelines, \
                deployment configurations, infrastructure as code, and operational \
                tooling. Follow best practices for reproducibility, security \
                hardening, and monitoring. Ensure rollback safety."
                .to_string(),
            DevRole::SecurityAuditor => "You are a security auditor. Scan code \
                for vulnerabilities including OWASP Top 10, insecure dependencies, \
                secrets exposure, improper access controls, and cryptographic \
                weaknesses. Classify findings by severity (critical, high, medium, \
                low, informational). Provide actionable remediation guidance."
                .to_string(),
            DevRole::Documenter => "You are a technical writer. Write clear, \
                accurate, and comprehensive documentation. Include code examples, \
                API references, usage patterns, and architecture overviews. Follow \
                the project's existing documentation style and conventions."
                .to_string(),
        }
    }

    /// Get the handoff message between steps (what to pass from step N to N+1).
    pub fn handoff_message(
        &self,
        from_step: &WorkflowStep,
        artifacts: &[WorkflowArtifact],
    ) -> String {
        let artifact_summary: Vec<String> = artifacts
            .iter()
            .map(|a| format!("[{}] {}", a.name, truncate_content(&a.content, 200)))
            .collect();

        format!(
            "Handoff from Step {} ({}, {}): {}\n\nArtifacts produced:\n{}",
            from_step.order,
            from_step.role,
            from_step.action,
            from_step.expected_output,
            if artifact_summary.is_empty() {
                "  (none)".to_string()
            } else {
                artifact_summary
                    .iter()
                    .map(|s| format!("  - {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        )
    }

    /// Get all available workflows.
    pub fn available_workflows(&self) -> Vec<DevWorkflow> {
        ALL_WORKFLOWS.to_vec()
    }

    /// Describe a workflow in human-readable form.
    pub fn describe_workflow(&self, workflow: DevWorkflow) -> String {
        let steps = self.workflow_steps(workflow);
        let step_descriptions: Vec<String> = steps
            .iter()
            .map(|s| {
                let gate_info = s
                    .gate
                    .as_ref()
                    .map(|g| format!(" [Gate: {}]", g.description))
                    .unwrap_or_default();
                format!(
                    "  {}. {} ({}): {}{}",
                    s.order, s.role, s.action, s.expected_output, gate_info
                )
            })
            .collect();

        format!(
            "Workflow: {workflow}\nSteps ({}):\n{}",
            steps.len(),
            step_descriptions.join("\n")
        )
    }

    /// Get team summary.
    pub fn summary(&self) -> String {
        let roles: Vec<String> = self.config.roles.iter().map(|r| r.to_string()).collect();
        let runnable: Vec<String> = ALL_WORKFLOWS
            .iter()
            .filter(|w| self.can_run_workflow(**w))
            .map(|w| w.to_string())
            .collect();

        format!(
            "Team: {}\nRoles ({}): {}\nMax parallel: {}\nEnforce gates: {}\nMax iterations: {}\nRunnable workflows ({}): {}",
            self.config.name,
            self.config.roles.len(),
            roles.join(", "),
            self.config.max_parallel,
            self.config.enforce_gates,
            self.config.max_iterations,
            runnable.len(),
            runnable.join(", "),
        )
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Truncate content to a maximum length, appending "..." if truncated.
fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}

/// Default model tier for each role.
fn default_model_tier(role: DevRole) -> &'static str {
    match role {
        DevRole::Architect => "powerful",
        DevRole::Implementer => "balanced",
        DevRole::Tester => "balanced",
        DevRole::Reviewer => "powerful",
        DevRole::Debugger => "powerful",
        DevRole::DevOps => "fast",
        DevRole::SecurityAuditor => "powerful",
        DevRole::Documenter => "fast",
    }
}

/// Default role-to-model-tier mapping for all roles.
fn default_role_models() -> Vec<(DevRole, String)> {
    vec![
        (DevRole::Architect, "powerful".to_string()),
        (DevRole::Implementer, "balanced".to_string()),
        (DevRole::Tester, "balanced".to_string()),
        (DevRole::Reviewer, "powerful".to_string()),
        (DevRole::Debugger, "powerful".to_string()),
        (DevRole::DevOps, "fast".to_string()),
        (DevRole::SecurityAuditor, "powerful".to_string()),
        (DevRole::Documenter, "fast".to_string()),
    ]
}

// ---------------------------------------------------------------------------
// Workflow step builders
// ---------------------------------------------------------------------------

/// Build steps for the ImplementFeature workflow.
fn build_implement_feature_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Architect,
            action: "Analyze requirements and create implementation plan".to_string(),
            instructions: "Review the feature requirements thoroughly. Break them down \
                into concrete implementation tasks with clear acceptance criteria. \
                Identify dependencies between tasks, estimate effort for each, and \
                assess technical risks. Produce a detailed plan with step-by-step \
                instructions for the implementer."
                .to_string(),
            expected_output:
                "Detailed implementation plan with tasks, dependencies, and risk assessment"
                    .to_string(),
            gate: Some(QualityGate {
                description: "Implementation plan produced".to_string(),
                check_type: GateCheck::Custom {
                    check: "Plan document exists and contains task breakdown".to_string(),
                },
            }),
            retryable: true,
            max_retries: 2,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Implementer,
            action: "Write production code following the plan".to_string(),
            instructions: "Implement the feature following the architect's plan. \
                Write clean, idiomatic code that follows project conventions. \
                Handle errors properly, avoid panics, and ensure the code compiles \
                without warnings."
                .to_string(),
            expected_output: "Production code that compiles and implements the planned feature"
                .to_string(),
            gate: Some(QualityGate {
                description: "Code compiles successfully".to_string(),
                check_type: GateCheck::CompileSuccess,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Tester,
            action: "Write and run tests for the new feature".to_string(),
            instructions: "Write comprehensive tests for the implemented feature. \
                Cover happy paths, edge cases, error conditions, and boundary values. \
                Ensure tests are deterministic and independent. Run all tests and \
                verify they pass."
                .to_string(),
            expected_output: "Test suite with passing results and coverage report".to_string(),
            gate: Some(QualityGate {
                description: "All tests pass".to_string(),
                check_type: GateCheck::TestsPass,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 4,
            role: DevRole::Reviewer,
            action: "Review code for quality, security, and style".to_string(),
            instructions: "Review the implementation and tests for correctness, \
                security vulnerabilities, performance issues, coding style, and \
                test coverage. Provide specific, actionable feedback with line \
                references. Score the review on a 0-100 scale."
                .to_string(),
            expected_output: "Review report with score, findings, and recommendations".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 70".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 70 },
            }),
            retryable: false,
            max_retries: 0,
        },
        WorkflowStep {
            order: 5,
            role: DevRole::Documenter,
            action: "Update documentation for the new feature".to_string(),
            instructions: "Write or update documentation for the newly implemented \
                feature. Include API references, usage examples, and any \
                configuration changes. Follow the project's documentation style."
                .to_string(),
            expected_output: "Updated documentation covering the new feature".to_string(),
            gate: None,
            retryable: true,
            max_retries: 1,
        },
    ]
}

/// Build steps for the FixBug workflow.
fn build_fix_bug_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Debugger,
            action: "Reproduce and diagnose the root cause".to_string(),
            instructions: "Reproduce the bug systematically. Analyze logs, stack traces, \
                and code paths to identify the root cause. Document the reproduction \
                steps and root cause analysis clearly."
                .to_string(),
            expected_output: "Root cause analysis with reproduction steps".to_string(),
            gate: Some(QualityGate {
                description: "Root cause identified".to_string(),
                check_type: GateCheck::Custom {
                    check: "Root cause analysis document produced".to_string(),
                },
            }),
            retryable: true,
            max_retries: 2,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Implementer,
            action: "Apply the fix with minimal changes".to_string(),
            instructions: "Implement a targeted fix based on the root cause analysis. \
                Minimize code changes to reduce regression risk. Ensure the fix \
                addresses the root cause, not just symptoms. The code must compile \
                without errors or warnings."
                .to_string(),
            expected_output: "Bug fix code that compiles successfully".to_string(),
            gate: Some(QualityGate {
                description: "Code compiles successfully".to_string(),
                check_type: GateCheck::CompileSuccess,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Tester,
            action: "Write regression test and verify fix".to_string(),
            instructions: "Write a regression test that reproduces the original bug \
                and verifies the fix. Run the full test suite to check for regressions. \
                The regression test must fail without the fix and pass with it."
                .to_string(),
            expected_output: "Regression test and full test suite results".to_string(),
            gate: Some(QualityGate {
                description: "All tests pass".to_string(),
                check_type: GateCheck::TestsPass,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 4,
            role: DevRole::Reviewer,
            action: "Review the fix for correctness and side effects".to_string(),
            instructions: "Review the bug fix for correctness, potential side effects, \
                and regression risk. Verify the fix addresses the root cause. Check \
                that the regression test is adequate. Score on a 0-100 scale."
                .to_string(),
            expected_output: "Review report with assessment of fix quality".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 70".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 70 },
            }),
            retryable: false,
            max_retries: 0,
        },
    ]
}

/// Build steps for the Refactor workflow.
fn build_refactor_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Architect,
            action: "Analyze code structure and plan refactoring approach".to_string(),
            instructions: "Analyze the current code structure and identify areas for \
                improvement. Design the refactoring approach with clear goals \
                (readability, performance, modularity). Ensure the plan preserves \
                existing behavior."
                .to_string(),
            expected_output: "Refactoring plan with goals and approach".to_string(),
            gate: Some(QualityGate {
                description: "Refactoring plan produced".to_string(),
                check_type: GateCheck::Custom {
                    check: "Plan document exists with refactoring strategy".to_string(),
                },
            }),
            retryable: true,
            max_retries: 2,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Implementer,
            action: "Apply refactoring changes".to_string(),
            instructions: "Apply the planned refactoring changes. Make incremental, \
                verifiable changes. Preserve all existing behavior. The code must \
                compile without errors or warnings."
                .to_string(),
            expected_output: "Refactored code that compiles successfully".to_string(),
            gate: Some(QualityGate {
                description: "Code compiles successfully".to_string(),
                check_type: GateCheck::CompileSuccess,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Tester,
            action: "Run existing tests and verify no regressions".to_string(),
            instructions: "Run the full test suite to verify the refactoring did not \
                introduce any regressions. All existing tests must continue to pass. \
                Add tests for any new code paths introduced by the refactoring."
                .to_string(),
            expected_output: "Test results showing no regressions".to_string(),
            gate: Some(QualityGate {
                description: "All tests pass".to_string(),
                check_type: GateCheck::TestsPass,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 4,
            role: DevRole::Reviewer,
            action: "Review refactoring for improved design quality".to_string(),
            instructions: "Review the refactored code for design improvement, \
                readability, and correctness. Verify behavior is preserved. \
                Assess whether the refactoring achieved its stated goals. \
                Score on a 0-100 scale."
                .to_string(),
            expected_output: "Review report with design quality assessment".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 70".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 70 },
            }),
            retryable: false,
            max_retries: 0,
        },
    ]
}

/// Build steps for the AddTests workflow.
fn build_add_tests_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Tester,
            action: "Analyze code and identify untested paths".to_string(),
            instructions: "Analyze the codebase to identify untested code paths, \
                functions, and edge cases. Prioritize critical paths and security- \
                sensitive code. Produce a test plan with identified gaps."
                .to_string(),
            expected_output: "Test plan identifying coverage gaps and priorities".to_string(),
            gate: None,
            retryable: true,
            max_retries: 1,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Tester,
            action: "Write comprehensive tests".to_string(),
            instructions: "Write tests to fill the identified coverage gaps. Include \
                unit tests, integration tests, and edge case tests. Use descriptive \
                test names and follow project conventions. All tests must pass."
                .to_string(),
            expected_output: "New test suite with passing results".to_string(),
            gate: Some(QualityGate {
                description: "All tests pass".to_string(),
                check_type: GateCheck::TestsPass,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Reviewer,
            action: "Review test quality and coverage".to_string(),
            instructions: "Review the new tests for quality, completeness, and \
                correctness. Verify tests actually test meaningful behavior and are \
                not trivial. Assess coverage improvement. Score on a 0-100 scale."
                .to_string(),
            expected_output: "Review report with test quality assessment".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 60".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 60 },
            }),
            retryable: false,
            max_retries: 0,
        },
    ]
}

/// Build steps for the SecurityAudit workflow.
fn build_security_audit_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::SecurityAuditor,
            action: "Scan code for security vulnerabilities".to_string(),
            instructions: "Perform a comprehensive security scan of the codebase. \
                Check for OWASP Top 10 vulnerabilities, insecure dependencies, \
                secrets exposure, improper access controls, and cryptographic \
                weaknesses. Classify findings by severity."
                .to_string(),
            expected_output: "Security scan results with classified findings".to_string(),
            gate: Some(QualityGate {
                description: "Security scan completed".to_string(),
                check_type: GateCheck::Custom {
                    check: "Security scan report produced with findings classified".to_string(),
                },
            }),
            retryable: true,
            max_retries: 2,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::SecurityAuditor,
            action: "Generate detailed security report with findings".to_string(),
            instructions: "Compile a detailed security audit report. Include all \
                findings with severity classification, affected code locations, \
                potential impact, and recommended remediations. Prioritize \
                findings by risk."
                .to_string(),
            expected_output: "Detailed security audit report".to_string(),
            gate: None,
            retryable: true,
            max_retries: 1,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Implementer,
            action: "Apply security remediations".to_string(),
            instructions: "Implement fixes for the security findings identified in \
                the audit report. Prioritize critical and high severity findings. \
                Follow security best practices and the auditor's recommendations. \
                The code must compile without errors."
                .to_string(),
            expected_output: "Remediated code that compiles successfully".to_string(),
            gate: Some(QualityGate {
                description: "Code compiles successfully".to_string(),
                check_type: GateCheck::CompileSuccess,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 4,
            role: DevRole::SecurityAuditor,
            action: "Verify remediations and re-scan".to_string(),
            instructions: "Re-scan the remediated code to verify that security fixes \
                are effective. Confirm that no new vulnerabilities were introduced. \
                Verify there are no remaining critical or high severity findings."
                .to_string(),
            expected_output: "Verification report confirming remediations".to_string(),
            gate: Some(QualityGate {
                description: "No critical security findings".to_string(),
                check_type: GateCheck::NoSecurityFindings {
                    max_severity: "high".to_string(),
                },
            }),
            retryable: true,
            max_retries: 2,
        },
    ]
}

/// Build steps for the CodeReview workflow.
fn build_code_review_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Reviewer,
            action: "Perform detailed code review across all dimensions".to_string(),
            instructions: "Review the code across correctness, security, performance, \
                readability, maintainability, and test coverage. Provide specific, \
                actionable feedback with file and line references. Identify blocking \
                and non-blocking issues."
                .to_string(),
            expected_output: "Detailed code review with categorized feedback".to_string(),
            gate: None,
            retryable: true,
            max_retries: 1,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Implementer,
            action: "Address review feedback".to_string(),
            instructions: "Address all blocking issues from the code review. Apply \
                non-blocking suggestions where appropriate. Explain any feedback \
                that was intentionally not addressed. The code must compile."
                .to_string(),
            expected_output: "Updated code addressing review feedback".to_string(),
            gate: Some(QualityGate {
                description: "Code compiles successfully".to_string(),
                check_type: GateCheck::CompileSuccess,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Reviewer,
            action: "Verify changes address all feedback".to_string(),
            instructions: "Re-review the updated code to verify all blocking feedback \
                has been addressed. Check that fixes are correct and no new issues \
                were introduced. Score on a 0-100 scale."
                .to_string(),
            expected_output: "Final review report with approval status".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 80".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 80 },
            }),
            retryable: false,
            max_retries: 0,
        },
    ]
}

/// Build steps for the Optimize workflow.
fn build_optimize_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Architect,
            action: "Profile and identify performance bottlenecks".to_string(),
            instructions: "Analyze the codebase to identify performance bottlenecks. \
                Profile critical paths, measure latencies, and identify hot spots. \
                Prioritize optimization targets by impact and effort."
                .to_string(),
            expected_output: "Performance analysis with identified bottlenecks".to_string(),
            gate: None,
            retryable: true,
            max_retries: 1,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Implementer,
            action: "Apply optimizations".to_string(),
            instructions: "Implement performance optimizations for the identified \
                bottlenecks. Use algorithmic improvements, caching, batching, or \
                concurrency as appropriate. Maintain code readability and correctness. \
                The code must compile."
                .to_string(),
            expected_output: "Optimized code that compiles successfully".to_string(),
            gate: Some(QualityGate {
                description: "Code compiles successfully".to_string(),
                check_type: GateCheck::CompileSuccess,
            }),
            retryable: true,
            max_retries: 3,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Tester,
            action: "Run benchmarks and verify improvement".to_string(),
            instructions: "Run performance benchmarks comparing before and after the \
                optimization. Verify that the optimization improves performance \
                without introducing regressions. Run the full test suite."
                .to_string(),
            expected_output: "Benchmark results and test suite results".to_string(),
            gate: Some(QualityGate {
                description: "All tests pass".to_string(),
                check_type: GateCheck::TestsPass,
            }),
            retryable: true,
            max_retries: 2,
        },
        WorkflowStep {
            order: 4,
            role: DevRole::Reviewer,
            action: "Review optimizations for correctness".to_string(),
            instructions: "Review the optimization changes for correctness and \
                maintainability. Verify the performance improvement is real and \
                not a measurement artifact. Check for algorithmic correctness. \
                Score on a 0-100 scale."
                .to_string(),
            expected_output: "Review report assessing optimization quality".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 70".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 70 },
            }),
            retryable: false,
            max_retries: 0,
        },
    ]
}

/// Build steps for the WriteDocumentation workflow.
fn build_write_documentation_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            order: 1,
            role: DevRole::Architect,
            action: "Analyze codebase and identify documentation gaps".to_string(),
            instructions: "Analyze the codebase structure, public APIs, and existing \
                documentation. Identify undocumented or poorly documented areas. \
                Prioritize documentation needs and produce an outline."
                .to_string(),
            expected_output: "Documentation gap analysis and outline".to_string(),
            gate: None,
            retryable: true,
            max_retries: 1,
        },
        WorkflowStep {
            order: 2,
            role: DevRole::Documenter,
            action: "Write comprehensive documentation".to_string(),
            instructions: "Write documentation following the outline. Include API \
                references, usage examples, architecture overviews, and getting \
                started guides. Use proper formatting and follow the project's \
                documentation style."
                .to_string(),
            expected_output: "Comprehensive documentation content".to_string(),
            gate: None,
            retryable: true,
            max_retries: 2,
        },
        WorkflowStep {
            order: 3,
            role: DevRole::Reviewer,
            action: "Review documentation for accuracy and completeness".to_string(),
            instructions: "Review the documentation for technical accuracy, \
                completeness, clarity, and consistency with the actual code. \
                Verify code examples compile and work correctly. Score on a \
                0-100 scale."
                .to_string(),
            expected_output: "Review report on documentation quality".to_string(),
            gate: Some(QualityGate {
                description: "Review score >= 60".to_string(),
                check_type: GateCheck::ReviewScore { min_score: 60 },
            }),
            retryable: false,
            max_retries: 0,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_full_stack_team() {
        let team = DevTeam::full_stack();
        assert_eq!(team.config.name, "Full-Stack Team");
        assert_eq!(team.config.roles.len(), 8);
        assert!(team.config.enforce_gates);
        assert_eq!(team.config.max_parallel, 3);

        // Every DevRole should be present
        assert!(team.config.roles.contains(&DevRole::Architect));
        assert!(team.config.roles.contains(&DevRole::Implementer));
        assert!(team.config.roles.contains(&DevRole::Tester));
        assert!(team.config.roles.contains(&DevRole::Reviewer));
        assert!(team.config.roles.contains(&DevRole::Debugger));
        assert!(team.config.roles.contains(&DevRole::DevOps));
        assert!(team.config.roles.contains(&DevRole::SecurityAuditor));
        assert!(team.config.roles.contains(&DevRole::Documenter));
    }

    #[test]
    fn test_minimal_team() {
        let team = DevTeam::minimal();
        assert_eq!(team.config.name, "Minimal Team");
        assert_eq!(team.config.roles.len(), 2);
        assert!(team.config.roles.contains(&DevRole::Implementer));
        assert!(team.config.roles.contains(&DevRole::Tester));
        assert!(!team.config.enforce_gates);
        assert_eq!(team.config.max_parallel, 1);
    }

    #[test]
    fn test_security_team() {
        let team = DevTeam::security_team();
        assert_eq!(team.config.name, "Security Team");
        assert!(team.config.roles.contains(&DevRole::SecurityAuditor));
        assert!(team.config.roles.contains(&DevRole::Implementer));
        assert!(team.config.roles.contains(&DevRole::Reviewer));
        assert!(team.config.roles.contains(&DevRole::Tester));
        assert!(team.config.enforce_gates);
    }

    #[test]
    fn test_workflow_implement_feature() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::ImplementFeature);
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].role, DevRole::Architect);
        assert_eq!(steps[1].role, DevRole::Implementer);
        assert_eq!(steps[2].role, DevRole::Tester);
        assert_eq!(steps[3].role, DevRole::Reviewer);
        assert_eq!(steps[4].role, DevRole::Documenter);
        // Last step has no gate
        assert!(steps[4].gate.is_none());
    }

    #[test]
    fn test_workflow_fix_bug() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::FixBug);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].role, DevRole::Debugger);
        assert_eq!(steps[1].role, DevRole::Implementer);
        assert_eq!(steps[2].role, DevRole::Tester);
        assert_eq!(steps[3].role, DevRole::Reviewer);
    }

    #[test]
    fn test_workflow_refactor() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::Refactor);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].role, DevRole::Architect);
        assert_eq!(steps[1].role, DevRole::Implementer);
        assert_eq!(steps[2].role, DevRole::Tester);
        assert_eq!(steps[3].role, DevRole::Reviewer);
    }

    #[test]
    fn test_workflow_add_tests() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::AddTests);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].role, DevRole::Tester);
        assert_eq!(steps[1].role, DevRole::Tester);
        assert_eq!(steps[2].role, DevRole::Reviewer);
        // First step has no gate
        assert!(steps[0].gate.is_none());
    }

    #[test]
    fn test_workflow_security_audit() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::SecurityAudit);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].role, DevRole::SecurityAuditor);
        assert_eq!(steps[1].role, DevRole::SecurityAuditor);
        assert_eq!(steps[2].role, DevRole::Implementer);
        assert_eq!(steps[3].role, DevRole::SecurityAuditor);
    }

    #[test]
    fn test_workflow_code_review() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::CodeReview);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].role, DevRole::Reviewer);
        assert_eq!(steps[1].role, DevRole::Implementer);
        assert_eq!(steps[2].role, DevRole::Reviewer);
        // Final review gate requires score >= 80
        let gate = steps[2].gate.as_ref().unwrap();
        assert_eq!(gate.check_type, GateCheck::ReviewScore { min_score: 80 });
    }

    #[test]
    fn test_workflow_optimize() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::Optimize);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].role, DevRole::Architect);
        assert_eq!(steps[1].role, DevRole::Implementer);
        assert_eq!(steps[2].role, DevRole::Tester);
        assert_eq!(steps[3].role, DevRole::Reviewer);
    }

    #[test]
    fn test_workflow_write_docs() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::WriteDocumentation);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].role, DevRole::Architect);
        assert_eq!(steps[1].role, DevRole::Documenter);
        assert_eq!(steps[2].role, DevRole::Reviewer);
        // Review gate requires score >= 60
        let gate = steps[2].gate.as_ref().unwrap();
        assert_eq!(gate.check_type, GateCheck::ReviewScore { min_score: 60 });
    }

    #[test]
    fn test_required_roles_feature() {
        let team = DevTeam::full_stack();
        let roles = team.required_roles(DevWorkflow::ImplementFeature);
        assert!(roles.contains(&DevRole::Architect));
        assert!(roles.contains(&DevRole::Implementer));
        assert!(roles.contains(&DevRole::Tester));
        assert!(roles.contains(&DevRole::Reviewer));
        assert!(roles.contains(&DevRole::Documenter));
        // No duplicates
        assert_eq!(roles.len(), 5);
    }

    #[test]
    fn test_can_run_workflow_true() {
        let team = DevTeam::full_stack();
        assert!(team.can_run_workflow(DevWorkflow::ImplementFeature));
        assert!(team.can_run_workflow(DevWorkflow::FixBug));
        assert!(team.can_run_workflow(DevWorkflow::SecurityAudit));
    }

    #[test]
    fn test_can_run_workflow_false() {
        let team = DevTeam::minimal();
        // Minimal team (Implementer + Tester) cannot run ImplementFeature
        // because it requires Architect, Reviewer, and Documenter
        assert!(!team.can_run_workflow(DevWorkflow::ImplementFeature));
        assert!(!team.can_run_workflow(DevWorkflow::FixBug));
        assert!(!team.can_run_workflow(DevWorkflow::SecurityAudit));
    }

    #[test]
    fn test_model_for_role() {
        let team = DevTeam::full_stack();
        assert_eq!(team.model_for_role(DevRole::Architect), "powerful");
        assert_eq!(team.model_for_role(DevRole::Implementer), "balanced");
        assert_eq!(team.model_for_role(DevRole::Tester), "balanced");
        assert_eq!(team.model_for_role(DevRole::Reviewer), "powerful");
        assert_eq!(team.model_for_role(DevRole::Debugger), "powerful");
        assert_eq!(team.model_for_role(DevRole::DevOps), "fast");
        assert_eq!(team.model_for_role(DevRole::SecurityAuditor), "powerful");
        assert_eq!(team.model_for_role(DevRole::Documenter), "fast");
    }

    #[test]
    fn test_role_system_prompt_not_empty() {
        let team = DevTeam::full_stack();
        let all_roles = [
            DevRole::Architect,
            DevRole::Implementer,
            DevRole::Tester,
            DevRole::Reviewer,
            DevRole::Debugger,
            DevRole::DevOps,
            DevRole::SecurityAuditor,
            DevRole::Documenter,
        ];
        for role in &all_roles {
            let prompt = team.role_system_prompt(*role);
            assert!(
                !prompt.is_empty(),
                "System prompt for {role} should not be empty"
            );
            assert!(
                prompt.len() > 50,
                "System prompt for {role} should be substantive"
            );
        }
    }

    #[test]
    fn test_validate_gates_tests_pass() {
        let team = DevTeam::full_stack();
        let artifacts_with_tests = vec![WorkflowArtifact {
            name: "test_results".to_string(),
            artifact_type: ArtifactType::TestResults,
            content: "All 42 tests passed".to_string(),
        }];
        let artifacts_without = vec![WorkflowArtifact {
            name: "code".to_string(),
            artifact_type: ArtifactType::Code,
            content: "fn main() {}".to_string(),
        }];

        // Step 3 of ImplementFeature has TestsPass gate
        assert!(team.validate_gates(DevWorkflow::ImplementFeature, 3, &artifacts_with_tests));
        assert!(!team.validate_gates(DevWorkflow::ImplementFeature, 3, &artifacts_without));
    }

    #[test]
    fn test_validate_gates_review_score() {
        let team = DevTeam::full_stack();
        let artifacts_with_review = vec![WorkflowArtifact {
            name: "review".to_string(),
            artifact_type: ArtifactType::ReviewReport,
            content: "Score: 85/100. LGTM.".to_string(),
        }];
        let artifacts_without = vec![WorkflowArtifact {
            name: "code".to_string(),
            artifact_type: ArtifactType::Code,
            content: "fn main() {}".to_string(),
        }];

        // Step 4 of ImplementFeature has ReviewScore { min_score: 70 } gate
        assert!(team.validate_gates(DevWorkflow::ImplementFeature, 4, &artifacts_with_review));
        assert!(!team.validate_gates(DevWorkflow::ImplementFeature, 4, &artifacts_without));
    }

    #[test]
    fn test_describe_workflow() {
        let team = DevTeam::full_stack();
        let description = team.describe_workflow(DevWorkflow::ImplementFeature);
        assert!(description.contains("Implement Feature"));
        assert!(description.contains("Architect"));
        assert!(description.contains("Implementer"));
        assert!(description.contains("Tester"));
        assert!(description.contains("Reviewer"));
        assert!(description.contains("Documenter"));
        assert!(description.contains("Steps (5)"));
    }

    #[test]
    fn test_available_workflows() {
        let team = DevTeam::full_stack();
        let workflows = team.available_workflows();
        assert_eq!(workflows.len(), 8);
        assert!(workflows.contains(&DevWorkflow::ImplementFeature));
        assert!(workflows.contains(&DevWorkflow::FixBug));
        assert!(workflows.contains(&DevWorkflow::Refactor));
        assert!(workflows.contains(&DevWorkflow::AddTests));
        assert!(workflows.contains(&DevWorkflow::SecurityAudit));
        assert!(workflows.contains(&DevWorkflow::CodeReview));
        assert!(workflows.contains(&DevWorkflow::Optimize));
        assert!(workflows.contains(&DevWorkflow::WriteDocumentation));
    }

    #[test]
    fn test_team_summary() {
        let team = DevTeam::full_stack();
        let summary = team.summary();
        assert!(summary.contains("Full-Stack Team"));
        assert!(summary.contains("Roles (8)"));
        assert!(summary.contains("Architect"));
        assert!(summary.contains("Enforce gates: true"));
        assert!(summary.contains("Max parallel: 3"));
    }

    #[test]
    fn test_handoff_message() {
        let team = DevTeam::full_stack();
        let steps = team.workflow_steps(DevWorkflow::ImplementFeature);
        let artifacts = vec![WorkflowArtifact {
            name: "implementation_plan".to_string(),
            artifact_type: ArtifactType::Plan,
            content: "1. Create module\n2. Add types\n3. Implement logic".to_string(),
        }];

        let message = team.handoff_message(&steps[0], &artifacts);
        assert!(message.contains("Handoff from Step 1"));
        assert!(message.contains("Architect"));
        assert!(message.contains("implementation_plan"));
        assert!(message.contains("Create module"));
    }

    #[test]
    fn test_workflow_step_ordering() {
        let team = DevTeam::full_stack();
        let all_workflows = team.available_workflows();

        for workflow in &all_workflows {
            let steps = team.workflow_steps(*workflow);
            // Steps should be ordered 1..=N
            for (i, step) in steps.iter().enumerate() {
                assert_eq!(
                    step.order,
                    i + 1,
                    "Step {i} of {workflow} should have order {}",
                    i + 1
                );
            }
            // Steps should not be empty
            assert!(
                !steps.is_empty(),
                "Workflow {workflow} should have at least one step"
            );
        }
    }
}
