//! TOML-based Workflow DSL for declarative pipeline definitions.
//!
//! Parses TOML workflow files into the executable [`WorkflowDefinition`] used
//! by the [`WorkflowEngine`].  This allows non-Rust developers to author
//! multi-step agent pipelines with a simple, human-readable syntax.
//!
//! # Example TOML
//!
//! ```toml
//! [workflow]
//! name = "code-review-pipeline"
//! description = "Automated code review with security scan"
//! version = "1.0"
//!
//! [[steps]]
//! id = "analyze"
//! name = "Code Analysis"
//! type = "agent"
//! role = "code_analyst"
//! prompt = "Analyze the following code for quality issues: {{input}}"
//! tools = ["code_analysis", "file_read"]
//! timeout_seconds = 60
//!
//! [[steps]]
//! id = "security"
//! name = "Security Scan"
//! type = "agent"
//! role = "security_auditor"
//! prompt = "Scan for security vulnerabilities: {{steps.analyze.output}}"
//! tools = ["secret_scanner", "prompt_guard"]
//! depends_on = ["analyze"]
//!
//! [[steps]]
//! id = "review"
//! name = "Final Review"
//! type = "agent"
//! role = "tech_lead"
//! prompt = "Review analysis and security findings, produce final report"
//! depends_on = ["analyze", "security"]
//!
//! [[steps]]
//! id = "notify"
//! name = "Send Notification"
//! type = "http"
//! url = "{{env.WEBHOOK_URL}}"
//! method = "POST"
//! body = '{"status": "complete", "report": "{{steps.review.output}}"}'
//! depends_on = ["review"]
//!
//! [triggers]
//! cron = "0 9 * * MON"
//! webhook = true
//! ```

use argentor_core::{ArgentorError, ArgentorResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::workflow::{
    FailureAction, StepCondition, StepType, WorkflowDefinition, WorkflowStepDef, WorkflowTrigger,
};

// ---------------------------------------------------------------------------
// TOML schema types
// ---------------------------------------------------------------------------

/// Top-level structure of a TOML workflow file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowToml {
    /// Workflow metadata.
    pub workflow: WorkflowMeta,
    /// Ordered list of pipeline steps.
    pub steps: Vec<StepToml>,
    /// Optional trigger configuration.
    pub triggers: Option<TriggerConfig>,
    /// Optional workflow-level variables for template interpolation.
    pub variables: Option<HashMap<String, String>>,
}

/// Metadata about the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMeta {
    /// Unique name used as the workflow identifier.
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Semantic version string.
    pub version: Option<String>,
    /// Author or team name.
    pub author: Option<String>,
    /// Freeform tags for categorization.
    pub tags: Option<Vec<String>>,
    /// Default maximum retries for any step (overridable per step).
    pub max_retries: Option<u32>,
    /// Overall workflow timeout in seconds.
    pub timeout_seconds: Option<u64>,
}

/// A single step in the TOML workflow definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepToml {
    /// Unique identifier within the workflow.
    pub id: String,
    /// Human-readable step name.
    pub name: String,
    /// Step kind: `"agent"`, `"http"`, `"condition"`, `"delay"`,
    /// `"notification"`, `"assign_human"`.
    #[serde(rename = "type")]
    pub step_type: String,

    // -- Agent fields --
    /// Agent role (required for `type = "agent"`).
    pub role: Option<String>,
    /// Prompt template with `{{...}}` placeholders.
    pub prompt: Option<String>,
    /// List of tool names the agent may use.
    pub tools: Option<Vec<String>>,
    /// Model override (e.g., `"claude-3-opus"`).
    pub model: Option<String>,

    // -- HTTP fields --
    /// Target URL (required for `type = "http"`).
    pub url: Option<String>,
    /// HTTP method (default `"POST"`).
    pub method: Option<String>,
    /// HTTP headers.
    pub headers: Option<HashMap<String, String>>,
    /// Request body template.
    pub body: Option<String>,

    // -- Condition fields --
    /// Boolean expression (required for `type = "condition"`).
    pub condition: Option<String>,
    /// Step id to jump to when condition is true.
    pub on_true: Option<String>,
    /// Step id to jump to when condition is false.
    pub on_false: Option<String>,

    // -- Delay fields --
    /// Seconds to wait (required for `type = "delay"`).
    pub delay_seconds: Option<u64>,

    // -- Notification fields --
    /// Notification channel (required for `type = "notification"`).
    pub channel: Option<String>,
    /// Message template for notifications.
    pub message: Option<String>,

    // -- Assign human fields --
    /// Team to assign to (required for `type = "assign_human"`).
    pub team: Option<String>,

    // -- Common fields --
    /// Step ids that must complete before this step runs.
    pub depends_on: Option<Vec<String>>,
    /// Maximum execution time in seconds.
    pub timeout_seconds: Option<u64>,
    /// Number of retries on failure.
    pub retry: Option<u32>,
    /// Failure strategy: `"skip"`, `"abort"`, `"retry"`, or a step id to
    /// jump to.
    pub on_failure: Option<String>,
    /// Guard condition expression; step runs only when this evaluates true.
    pub run_if: Option<String>,
}

/// Trigger configuration for the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Cron expression (e.g., `"0 9 * * MON"`).
    pub cron: Option<String>,
    /// Whether the workflow can be triggered by a webhook.
    pub webhook: Option<bool>,
    /// Event name that triggers the workflow.
    pub on_event: Option<String>,
}

// ---------------------------------------------------------------------------
// Template context & resolution
// ---------------------------------------------------------------------------

/// Context available during template variable resolution.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    /// The initial workflow input.
    pub input: String,
    /// Outputs keyed by step id.
    pub step_outputs: HashMap<String, String>,
    /// Workflow-level variables.
    pub variables: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Severity of a validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// The workflow cannot be compiled.
    Error,
    /// The workflow can compile but may behave unexpectedly.
    Warning,
}

/// A single validation finding.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// The step that caused the error, if applicable.
    pub step_id: Option<String>,
    /// Human-readable description of the problem.
    pub message: String,
    /// Error or warning.
    pub severity: ValidationSeverity,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            ValidationSeverity::Error => "ERROR",
            ValidationSeverity::Warning => "WARNING",
        };
        match &self.step_id {
            Some(id) => write!(f, "[{prefix}] step '{id}': {}", self.message),
            None => write!(f, "[{prefix}] {}", self.message),
        }
    }
}

// ---------------------------------------------------------------------------
// DSL parser / compiler
// ---------------------------------------------------------------------------

/// Entry point for the TOML workflow DSL.
///
/// Provides methods to parse, validate, and compile TOML workflow
/// definitions into the executable [`WorkflowDefinition`] consumed by the
/// [`WorkflowEngine`].
pub struct WorkflowDsl;

impl WorkflowDsl {
    /// Parse a TOML string into a [`WorkflowToml`].
    pub fn parse(toml_str: &str) -> ArgentorResult<WorkflowToml> {
        toml::from_str(toml_str)
            .map_err(|e| ArgentorError::Config(format!("TOML parse error: {e}")))
    }

    /// Parse a TOML file from disk.
    pub fn parse_file(path: &Path) -> ArgentorResult<WorkflowToml> {
        let content = std::fs::read_to_string(path).map_err(ArgentorError::Io)?;
        Self::parse(&content)
    }

    /// Validate a parsed workflow, returning all findings.
    ///
    /// An empty vector means the workflow is valid.
    pub fn validate(toml: &WorkflowToml) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Must have at least one step.
        if toml.steps.is_empty() {
            errors.push(ValidationError {
                step_id: None,
                message: "workflow has no steps".to_string(),
                severity: ValidationSeverity::Error,
            });
            return errors;
        }

        // Collect step ids for cross-referencing.
        let step_ids: HashSet<&str> = toml.steps.iter().map(|s| s.id.as_str()).collect();

        // Check for duplicate ids.
        {
            let mut seen = HashSet::new();
            for step in &toml.steps {
                if !seen.insert(&step.id) {
                    errors.push(ValidationError {
                        step_id: Some(step.id.clone()),
                        message: format!("duplicate step id '{}'", step.id),
                        severity: ValidationSeverity::Error,
                    });
                }
            }
        }

        // Per-step validation.
        for step in &toml.steps {
            // Validate step type and required fields.
            match step.step_type.as_str() {
                "agent" => {
                    if step.role.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "agent step requires 'role' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    if step.prompt.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "agent step requires 'prompt' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                }
                "http" => {
                    if step.url.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "http step requires 'url' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                }
                "condition" => {
                    if step.condition.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "condition step requires 'condition' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    if step.on_true.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "condition step requires 'on_true' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    if step.on_false.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "condition step requires 'on_false' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    // Check that branch targets exist.
                    if let Some(ref target) = step.on_true {
                        if !step_ids.contains(target.as_str()) {
                            errors.push(ValidationError {
                                step_id: Some(step.id.clone()),
                                message: format!("on_true references unknown step '{target}'"),
                                severity: ValidationSeverity::Error,
                            });
                        }
                    }
                    if let Some(ref target) = step.on_false {
                        if !step_ids.contains(target.as_str()) {
                            errors.push(ValidationError {
                                step_id: Some(step.id.clone()),
                                message: format!("on_false references unknown step '{target}'"),
                                severity: ValidationSeverity::Error,
                            });
                        }
                    }
                }
                "delay" => {
                    if step.delay_seconds.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "delay step requires 'delay_seconds' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                }
                "notification" => {
                    if step.channel.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "notification step requires 'channel' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    if step.message.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "notification step requires 'message' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                }
                "assign_human" => {
                    if step.team.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "assign_human step requires 'team' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    if step.message.is_none() {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "assign_human step requires 'message' field".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                }
                unknown => {
                    errors.push(ValidationError {
                        step_id: Some(step.id.clone()),
                        message: format!("unknown step type '{unknown}'"),
                        severity: ValidationSeverity::Error,
                    });
                }
            }

            // Validate depends_on references.
            if let Some(ref deps) = step.depends_on {
                for dep in deps {
                    if !step_ids.contains(dep.as_str()) {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: format!("depends_on references unknown step '{dep}'"),
                            severity: ValidationSeverity::Error,
                        });
                    }
                    if dep == &step.id {
                        errors.push(ValidationError {
                            step_id: Some(step.id.clone()),
                            message: "step depends on itself".to_string(),
                            severity: ValidationSeverity::Error,
                        });
                    }
                }
            }

            // Validate on_failure if it looks like a goto target.
            if let Some(ref action) = step.on_failure {
                match action.as_str() {
                    "skip" | "abort" | "retry" => {}
                    target => {
                        if !step_ids.contains(target) {
                            errors.push(ValidationError {
                                step_id: Some(step.id.clone()),
                                message: format!(
                                    "on_failure value '{target}' is neither a known action \
                                     (skip/abort/retry) nor a valid step id"
                                ),
                                severity: ValidationSeverity::Error,
                            });
                        }
                    }
                }
            }

            // Warn if timeout is very large.
            if let Some(t) = step.timeout_seconds {
                if t > 3600 {
                    errors.push(ValidationError {
                        step_id: Some(step.id.clone()),
                        message: format!("timeout_seconds ({t}s) exceeds 1 hour"),
                        severity: ValidationSeverity::Warning,
                    });
                }
            }
        }

        // Circular dependency detection (topological sort via Kahn's algorithm).
        errors.extend(detect_cycles(&toml.steps));

        errors
    }

    /// Convert a parsed [`WorkflowToml`] into an executable
    /// [`WorkflowDefinition`].
    ///
    /// This performs validation first and returns the first error if the
    /// workflow is invalid.
    pub fn compile(toml: &WorkflowToml) -> ArgentorResult<WorkflowDefinition> {
        let validation_errors: Vec<_> = Self::validate(toml)
            .into_iter()
            .filter(|e| e.severity == ValidationSeverity::Error)
            .collect();
        if let Some(first) = validation_errors.first() {
            return Err(ArgentorError::Config(format!(
                "workflow validation failed: {first}"
            )));
        }

        let trigger = compile_trigger(&toml.triggers);

        let mut steps = Vec::with_capacity(toml.steps.len());
        for step_toml in &toml.steps {
            steps.push(compile_step(step_toml)?);
        }

        // Reorder steps based on dependency graph (topological sort) so the
        // linear WorkflowEngine can execute them in a valid order.
        let ordered = topological_sort(&toml.steps);
        let step_map: HashMap<&str, WorkflowStepDef> =
            steps.iter().map(|s| (s.id.as_str(), s.clone())).collect();
        let ordered_steps: Vec<WorkflowStepDef> = ordered
            .iter()
            .filter_map(|id| step_map.get(id.as_str()).cloned())
            .collect();

        Ok(WorkflowDefinition {
            id: toml.workflow.name.clone(),
            name: toml.workflow.name.clone(),
            description: toml.workflow.description.clone().unwrap_or_default(),
            trigger,
            steps: ordered_steps,
            timeout_seconds: toml.workflow.timeout_seconds,
        })
    }

    /// Parse a TOML string and compile it in one step.
    pub fn load(toml_str: &str) -> ArgentorResult<WorkflowDefinition> {
        let parsed = Self::parse(toml_str)?;
        Self::compile(&parsed)
    }

    /// Parse a TOML file and compile it in one step.
    pub fn load_file(path: &Path) -> ArgentorResult<WorkflowDefinition> {
        let parsed = Self::parse_file(path)?;
        Self::compile(&parsed)
    }

    /// Resolve template placeholders in a string.
    ///
    /// Supported patterns:
    /// - `{{input}}` — the workflow input value
    /// - `{{steps.<id>.output}}` — output of a previous step
    /// - `{{env.<VAR>}}` — environment variable
    /// - `{{variables.<key>}}` or `{{var.<key>}}` — workflow-level variable
    pub fn resolve_template(template: &str, context: &TemplateContext) -> String {
        let mut result = String::with_capacity(template.len());
        let mut remaining = template;

        while let Some(start) = remaining.find("{{") {
            result.push_str(&remaining[..start]);
            let after_open = &remaining[start + 2..];
            if let Some(end) = after_open.find("}}") {
                let key = after_open[..end].trim();
                let value = resolve_placeholder(key, context);
                result.push_str(&value);
                remaining = &after_open[end + 2..];
            } else {
                // No closing `}}` — keep the literal text.
                result.push_str(&remaining[start..]);
                remaining = "";
            }
        }
        result.push_str(remaining);
        result
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve a single placeholder key to its value.
fn resolve_placeholder(key: &str, ctx: &TemplateContext) -> String {
    // {{input}}
    if key == "input" {
        return ctx.input.clone();
    }

    // {{steps.<id>.output}}
    if let Some(rest) = key.strip_prefix("steps.") {
        if let Some(step_id) = rest.strip_suffix(".output") {
            return ctx.step_outputs.get(step_id).cloned().unwrap_or_default();
        }
    }

    // {{env.<VAR>}}
    if let Some(var_name) = key.strip_prefix("env.") {
        return std::env::var(var_name).unwrap_or_default();
    }

    // {{variables.<key>}} or {{var.<key>}}
    if let Some(var_key) = key
        .strip_prefix("variables.")
        .or_else(|| key.strip_prefix("var."))
    {
        return ctx.variables.get(var_key).cloned().unwrap_or_default();
    }

    // Unknown placeholder — return empty.
    String::new()
}

/// Derive the [`WorkflowTrigger`] from the optional TOML trigger config.
fn compile_trigger(trigger: &Option<TriggerConfig>) -> WorkflowTrigger {
    match trigger {
        None => WorkflowTrigger::Manual,
        Some(tc) => {
            if let Some(ref cron_expr) = tc.cron {
                return WorkflowTrigger::Schedule {
                    cron: cron_expr.clone(),
                };
            }
            if let Some(ref event) = tc.on_event {
                return WorkflowTrigger::Webhook {
                    event: event.clone(),
                };
            }
            if tc.webhook == Some(true) {
                return WorkflowTrigger::Webhook {
                    event: "webhook".to_string(),
                };
            }
            WorkflowTrigger::Manual
        }
    }
}

/// Compile a single TOML step into a [`WorkflowStepDef`].
fn compile_step(s: &StepToml) -> ArgentorResult<WorkflowStepDef> {
    let step_type = match s.step_type.as_str() {
        "agent" => StepType::AgentTask {
            agent_role: s.role.clone().unwrap_or_default(),
            prompt_template: s.prompt.clone().unwrap_or_default(),
        },
        "http" => StepType::HttpCall {
            method: s.method.clone().unwrap_or_else(|| "POST".to_string()),
            url: s.url.clone().unwrap_or_default(),
            body_template: s.body.clone(),
        },
        "condition" => StepType::Condition {
            expression: s.condition.clone().unwrap_or_default(),
            if_true: s.on_true.clone().unwrap_or_default(),
            if_false: s.on_false.clone().unwrap_or_default(),
        },
        "delay" => StepType::Delay {
            seconds: s.delay_seconds.unwrap_or(0),
        },
        "notification" => StepType::Notification {
            channel: s.channel.clone().unwrap_or_default(),
            message_template: s.message.clone().unwrap_or_default(),
        },
        "assign_human" => StepType::AssignToHuman {
            team: s.team.clone().unwrap_or_default(),
            message: s.message.clone().unwrap_or_default(),
        },
        other => {
            return Err(ArgentorError::Config(format!(
                "unknown step type '{other}'"
            )));
        }
    };

    let on_failure = match s.on_failure.as_deref() {
        Some("skip") => FailureAction::Skip,
        Some("abort") | None => FailureAction::Abort,
        Some("retry") => FailureAction::Retry {
            max: s.retry.unwrap_or(3),
        },
        Some(step_id) => FailureAction::GoTo {
            step_id: step_id.to_string(),
        },
    };

    let condition = s.run_if.as_ref().map(|expr| {
        if expr == "previous_succeeded" {
            StepCondition::IfPreviousSucceeded
        } else {
            StepCondition::Expression(expr.clone())
        }
    });

    Ok(WorkflowStepDef {
        id: s.id.clone(),
        name: s.name.clone(),
        step_type,
        condition,
        on_failure,
        timeout_seconds: s.timeout_seconds,
    })
}

/// Detect cycles in the dependency graph.  Returns validation errors for any
/// cycle found.
fn detect_cycles(steps: &[StepToml]) -> Vec<ValidationError> {
    let ids: HashSet<&str> = steps.iter().map(|s| s.id.as_str()).collect();

    // Build adjacency list: step -> deps that precede it.
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for step in steps {
        in_degree.entry(step.id.as_str()).or_insert(0);
        if let Some(ref deps) = step.depends_on {
            for dep in deps {
                if ids.contains(dep.as_str()) {
                    dependents
                        .entry(dep.as_str())
                        .or_default()
                        .push(step.id.as_str());
                    *in_degree.entry(step.id.as_str()).or_insert(0) += 1;
                }
            }
        }
    }

    // Kahn's algorithm.
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut visited = 0usize;
    while let Some(node) = queue.pop() {
        visited += 1;
        if let Some(nexts) = dependents.get(node) {
            for &next in nexts {
                if let Some(deg) = in_degree.get_mut(next) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(next);
                    }
                }
            }
        }
    }

    if visited < ids.len() {
        // Find the steps that are part of a cycle.
        let cycle_steps: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&id, _)| id.to_string())
            .collect();
        vec![ValidationError {
            step_id: None,
            message: format!(
                "circular dependency detected among steps: [{}]",
                cycle_steps.join(", ")
            ),
            severity: ValidationSeverity::Error,
        }]
    } else {
        vec![]
    }
}

/// Topological sort of steps by their `depends_on` edges.
///
/// Steps with no dependencies come first.  This is used by [`WorkflowDsl::compile`]
/// to reorder steps into a valid execution sequence.
fn topological_sort(steps: &[StepToml]) -> Vec<String> {
    let ids: HashSet<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for step in steps {
        in_degree.entry(step.id.as_str()).or_insert(0);
        if let Some(ref deps) = step.depends_on {
            for dep in deps {
                if ids.contains(dep.as_str()) {
                    dependents
                        .entry(dep.as_str())
                        .or_default()
                        .push(step.id.as_str());
                    *in_degree.entry(step.id.as_str()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();
    // Sort for deterministic output.
    queue.sort();

    let mut result = Vec::with_capacity(steps.len());
    while let Some(node) = queue.pop() {
        result.push(node.to_string());
        let mut nexts: Vec<&str> = dependents
            .get(node)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .to_vec();
        nexts.sort();
        for next in nexts {
            if let Some(deg) = in_degree.get_mut(next) {
                *deg -= 1;
                if *deg == 0 {
                    // Insert sorted to keep deterministic order.
                    let pos = queue.partition_point(|&x| x >= next);
                    queue.insert(pos, next);
                }
            }
        }
    }

    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // -- Helpers ------------------------------------------------------------

    fn minimal_toml() -> &'static str {
        r#"
[workflow]
name = "minimal"

[[steps]]
id = "s1"
name = "Step 1"
type = "agent"
role = "coder"
prompt = "Do something"
"#
    }

    fn full_toml() -> &'static str {
        r#"
[workflow]
name = "full-pipeline"
description = "A full pipeline with all step types"
version = "2.0"
author = "test"
tags = ["ci", "review"]
max_retries = 3
timeout_seconds = 600

[variables]
project = "argentor"
env_name = "staging"

[[steps]]
id = "analyze"
name = "Code Analysis"
type = "agent"
role = "code_analyst"
prompt = "Analyze {{input}} for project {{var.project}}"
tools = ["lint", "complexity"]
timeout_seconds = 60

[[steps]]
id = "check_severity"
name = "Check Severity"
type = "condition"
condition = "severity > 5"
on_true = "block"
on_false = "notify"
depends_on = ["analyze"]

[[steps]]
id = "block"
name = "Block Merge"
type = "http"
url = "https://api.example.com/block"
method = "POST"
body = '{"pr": "{{input}}"}'
depends_on = ["check_severity"]

[[steps]]
id = "notify"
name = "Send Notification"
type = "notification"
channel = "slack"
message = "Analysis complete for {{var.project}}"
depends_on = ["check_severity"]

[[steps]]
id = "wait"
name = "Cooldown"
type = "delay"
delay_seconds = 30
depends_on = ["notify"]

[[steps]]
id = "escalate"
name = "Escalate to Human"
type = "assign_human"
team = "platform"
message = "Review needed"
depends_on = ["block"]

[triggers]
cron = "0 9 * * MON"
"#
    }

    // -- Parse tests --------------------------------------------------------

    #[test]
    fn test_parse_minimal() {
        let wf = WorkflowDsl::parse(minimal_toml()).unwrap();
        assert_eq!(wf.workflow.name, "minimal");
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].step_type, "agent");
    }

    #[test]
    fn test_parse_full() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        assert_eq!(wf.workflow.name, "full-pipeline");
        assert_eq!(wf.workflow.version.as_deref(), Some("2.0"));
        assert_eq!(wf.workflow.author.as_deref(), Some("test"));
        assert_eq!(wf.steps.len(), 6);
        assert!(wf.triggers.is_some());
        assert!(wf.variables.is_some());
    }

    #[test]
    fn test_parse_invalid_toml() {
        let result = WorkflowDsl::parse("not valid {{{{ toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_step_types() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let types: Vec<&str> = wf.steps.iter().map(|s| s.step_type.as_str()).collect();
        assert!(types.contains(&"agent"));
        assert!(types.contains(&"condition"));
        assert!(types.contains(&"http"));
        assert!(types.contains(&"notification"));
        assert!(types.contains(&"delay"));
        assert!(types.contains(&"assign_human"));
    }

    #[test]
    fn test_parse_agent_fields() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let agent = wf.steps.iter().find(|s| s.id == "analyze").unwrap();
        assert_eq!(agent.role.as_deref(), Some("code_analyst"));
        assert!(agent.prompt.as_ref().unwrap().contains("{{input}}"));
        assert_eq!(agent.tools.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_parse_condition_fields() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let cond = wf.steps.iter().find(|s| s.id == "check_severity").unwrap();
        assert_eq!(cond.condition.as_deref(), Some("severity > 5"));
        assert_eq!(cond.on_true.as_deref(), Some("block"));
        assert_eq!(cond.on_false.as_deref(), Some("notify"));
    }

    #[test]
    fn test_parse_http_fields() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let http = wf.steps.iter().find(|s| s.id == "block").unwrap();
        assert_eq!(http.url.as_deref(), Some("https://api.example.com/block"));
        assert_eq!(http.method.as_deref(), Some("POST"));
        assert!(http.body.is_some());
    }

    #[test]
    fn test_parse_depends_on() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let escalate = wf.steps.iter().find(|s| s.id == "escalate").unwrap();
        assert_eq!(escalate.depends_on.as_ref().unwrap(), &["block"]);
    }

    #[test]
    fn test_parse_triggers() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let triggers = wf.triggers.unwrap();
        assert_eq!(triggers.cron.as_deref(), Some("0 9 * * MON"));
    }

    #[test]
    fn test_parse_variables() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let vars = wf.variables.unwrap();
        assert_eq!(vars.get("project").unwrap(), "argentor");
    }

    // -- Validation tests ---------------------------------------------------

    #[test]
    fn test_validate_valid_workflow() {
        let wf = WorkflowDsl::parse(full_toml()).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        let real_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == ValidationSeverity::Error)
            .collect();
        assert!(real_errors.is_empty(), "unexpected errors: {real_errors:?}");
    }

    #[test]
    fn test_validate_no_steps() {
        let toml_str = r#"
[workflow]
name = "empty"
"#;
        // Need to manually add empty steps via serde.
        let mut wf = WorkflowDsl::parse(
            &format!("{toml_str}\n[[steps]]\nid = \"x\"\nname = \"x\"\ntype = \"agent\"\nrole = \"r\"\nprompt = \"p\""),
        )
        .unwrap();
        wf.steps.clear();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors.iter().any(|e| e.message.contains("no steps")));
    }

    #[test]
    fn test_validate_unknown_step_type() {
        let toml_str = r#"
[workflow]
name = "bad-type"

[[steps]]
id = "s1"
name = "Step 1"
type = "teleport"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("unknown step type 'teleport'")));
    }

    #[test]
    fn test_validate_missing_agent_role() {
        let toml_str = r#"
[workflow]
name = "missing-role"

[[steps]]
id = "s1"
name = "Step 1"
type = "agent"
prompt = "hello"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors.iter().any(|e| e.message.contains("requires 'role'")));
    }

    #[test]
    fn test_validate_missing_agent_prompt() {
        let toml_str = r#"
[workflow]
name = "missing-prompt"

[[steps]]
id = "s1"
name = "Step 1"
type = "agent"
role = "coder"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'prompt'")));
    }

    #[test]
    fn test_validate_missing_http_url() {
        let toml_str = r#"
[workflow]
name = "missing-url"

[[steps]]
id = "s1"
name = "Step 1"
type = "http"
method = "GET"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors.iter().any(|e| e.message.contains("requires 'url'")));
    }

    #[test]
    fn test_validate_missing_condition_fields() {
        let toml_str = r#"
[workflow]
name = "missing-cond"

[[steps]]
id = "s1"
name = "Step 1"
type = "condition"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'condition'")));
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'on_true'")));
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'on_false'")));
    }

    #[test]
    fn test_validate_missing_delay_seconds() {
        let toml_str = r#"
[workflow]
name = "missing-delay"

[[steps]]
id = "s1"
name = "Step 1"
type = "delay"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'delay_seconds'")));
    }

    #[test]
    fn test_validate_missing_notification_fields() {
        let toml_str = r#"
[workflow]
name = "missing-notif"

[[steps]]
id = "s1"
name = "Step 1"
type = "notification"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'channel'")));
        assert!(errors
            .iter()
            .any(|e| e.message.contains("requires 'message'")));
    }

    #[test]
    fn test_validate_unknown_dependency() {
        let toml_str = r#"
[workflow]
name = "bad-dep"

[[steps]]
id = "s1"
name = "Step 1"
type = "agent"
role = "coder"
prompt = "hello"
depends_on = ["nonexistent"]
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("references unknown step 'nonexistent'")));
    }

    #[test]
    fn test_validate_self_dependency() {
        let toml_str = r#"
[workflow]
name = "self-dep"

[[steps]]
id = "s1"
name = "Step 1"
type = "agent"
role = "coder"
prompt = "hello"
depends_on = ["s1"]
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("depends on itself")));
    }

    #[test]
    fn test_validate_circular_dependency() {
        let toml_str = r#"
[workflow]
name = "circular"

[[steps]]
id = "a"
name = "A"
type = "agent"
role = "r"
prompt = "p"
depends_on = ["b"]

[[steps]]
id = "b"
name = "B"
type = "agent"
role = "r"
prompt = "p"
depends_on = ["a"]
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("circular dependency")));
    }

    #[test]
    fn test_validate_condition_unknown_branch_target() {
        let toml_str = r#"
[workflow]
name = "bad-branch"

[[steps]]
id = "s1"
name = "Check"
type = "condition"
condition = "x > 1"
on_true = "ghost"
on_false = "s1"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors.iter().any(|e| e
            .message
            .contains("on_true references unknown step 'ghost'")));
    }

    #[test]
    fn test_validate_duplicate_step_ids() {
        let toml_str = r#"
[workflow]
name = "dups"

[[steps]]
id = "s1"
name = "First"
type = "agent"
role = "r"
prompt = "p"

[[steps]]
id = "s1"
name = "Second"
type = "agent"
role = "r"
prompt = "p"
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicate step id")));
    }

    #[test]
    fn test_validate_large_timeout_warning() {
        let toml_str = r#"
[workflow]
name = "big-timeout"

[[steps]]
id = "s1"
name = "Slow Step"
type = "agent"
role = "r"
prompt = "p"
timeout_seconds = 7200
"#;
        let wf = WorkflowDsl::parse(toml_str).unwrap();
        let errors = WorkflowDsl::validate(&wf);
        assert!(errors.iter().any(|e| {
            e.severity == ValidationSeverity::Warning && e.message.contains("exceeds 1 hour")
        }));
    }

    // -- Compile tests ------------------------------------------------------

    #[test]
    fn test_compile_minimal() {
        let def = WorkflowDsl::load(minimal_toml()).unwrap();
        assert_eq!(def.id, "minimal");
        assert_eq!(def.steps.len(), 1);
        assert!(matches!(def.trigger, WorkflowTrigger::Manual));
    }

    #[test]
    fn test_compile_full() {
        let def = WorkflowDsl::load(full_toml()).unwrap();
        assert_eq!(def.id, "full-pipeline");
        assert_eq!(def.steps.len(), 6);
        assert!(matches!(
            def.trigger,
            WorkflowTrigger::Schedule { ref cron } if cron == "0 9 * * MON"
        ));
    }

    #[test]
    fn test_compile_step_types_round_trip() {
        let def = WorkflowDsl::load(full_toml()).unwrap();
        // Ensure each step type compiled correctly.
        let agent = def.steps.iter().find(|s| s.id == "analyze").unwrap();
        assert!(matches!(agent.step_type, StepType::AgentTask { .. }));

        let cond = def.steps.iter().find(|s| s.id == "check_severity").unwrap();
        assert!(matches!(cond.step_type, StepType::Condition { .. }));

        let http = def.steps.iter().find(|s| s.id == "block").unwrap();
        assert!(matches!(http.step_type, StepType::HttpCall { .. }));

        let notif = def.steps.iter().find(|s| s.id == "notify").unwrap();
        assert!(matches!(notif.step_type, StepType::Notification { .. }));

        let delay = def.steps.iter().find(|s| s.id == "wait").unwrap();
        assert!(matches!(delay.step_type, StepType::Delay { seconds: 30 }));

        let human = def.steps.iter().find(|s| s.id == "escalate").unwrap();
        assert!(matches!(human.step_type, StepType::AssignToHuman { .. }));
    }

    #[test]
    fn test_compile_failure_actions() {
        let toml_str = r#"
[workflow]
name = "failures"

[[steps]]
id = "s1"
name = "Skip on fail"
type = "agent"
role = "r"
prompt = "p"
on_failure = "skip"

[[steps]]
id = "s2"
name = "Abort on fail"
type = "agent"
role = "r"
prompt = "p"
on_failure = "abort"
depends_on = ["s1"]

[[steps]]
id = "s3"
name = "Retry on fail"
type = "agent"
role = "r"
prompt = "p"
on_failure = "retry"
retry = 5
depends_on = ["s2"]

[[steps]]
id = "s4"
name = "Goto on fail"
type = "agent"
role = "r"
prompt = "p"
on_failure = "s1"
depends_on = ["s3"]
"#;
        let def = WorkflowDsl::load(toml_str).unwrap();
        assert!(matches!(def.steps[0].on_failure, FailureAction::Skip));
        assert!(matches!(def.steps[1].on_failure, FailureAction::Abort));
        assert!(matches!(
            def.steps[2].on_failure,
            FailureAction::Retry { max: 5 }
        ));
        assert!(matches!(
            def.steps[3].on_failure,
            FailureAction::GoTo { ref step_id } if step_id == "s1"
        ));
    }

    #[test]
    fn test_compile_rejects_invalid() {
        let toml_str = r#"
[workflow]
name = "invalid"

[[steps]]
id = "s1"
name = "Bad"
type = "agent"
"#;
        let result = WorkflowDsl::load(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_trigger_webhook() {
        let toml_str = r#"
[workflow]
name = "wh"

[triggers]
webhook = true

[[steps]]
id = "s1"
name = "S"
type = "agent"
role = "r"
prompt = "p"
"#;
        let def = WorkflowDsl::load(toml_str).unwrap();
        assert!(matches!(def.trigger, WorkflowTrigger::Webhook { .. }));
    }

    #[test]
    fn test_compile_trigger_event() {
        let toml_str = r#"
[workflow]
name = "evt"

[triggers]
on_event = "push"

[[steps]]
id = "s1"
name = "S"
type = "agent"
role = "r"
prompt = "p"
"#;
        let def = WorkflowDsl::load(toml_str).unwrap();
        assert!(matches!(
            def.trigger,
            WorkflowTrigger::Webhook { ref event } if event == "push"
        ));
    }

    #[test]
    fn test_compile_topological_order() {
        // c depends on b, b depends on a.  Even if TOML order is c, a, b,
        // the compiled output must be a, b, c.
        let toml_str = r#"
[workflow]
name = "topo"

[[steps]]
id = "c"
name = "C"
type = "agent"
role = "r"
prompt = "p"
depends_on = ["b"]

[[steps]]
id = "a"
name = "A"
type = "agent"
role = "r"
prompt = "p"

[[steps]]
id = "b"
name = "B"
type = "agent"
role = "r"
prompt = "p"
depends_on = ["a"]
"#;
        let def = WorkflowDsl::load(toml_str).unwrap();
        let ids: Vec<&str> = def.steps.iter().map(|s| s.id.as_str()).collect();
        let pos_a = ids.iter().position(|&x| x == "a").unwrap();
        let pos_b = ids.iter().position(|&x| x == "b").unwrap();
        let pos_c = ids.iter().position(|&x| x == "c").unwrap();
        assert!(pos_a < pos_b, "a must come before b");
        assert!(pos_b < pos_c, "b must come before c");
    }

    // -- Template resolution tests ------------------------------------------

    #[test]
    fn test_template_input() {
        let ctx = TemplateContext {
            input: "hello world".to_string(),
            ..Default::default()
        };
        assert_eq!(
            WorkflowDsl::resolve_template("Say: {{input}}", &ctx),
            "Say: hello world"
        );
    }

    #[test]
    fn test_template_step_output() {
        let mut ctx = TemplateContext::default();
        ctx.step_outputs
            .insert("analyze".to_string(), "all good".to_string());
        assert_eq!(
            WorkflowDsl::resolve_template("Result: {{steps.analyze.output}}", &ctx),
            "Result: all good"
        );
    }

    #[test]
    fn test_template_env_var() {
        std::env::set_var("ARGENTOR_TEST_DSL_VAR", "secret123");
        let ctx = TemplateContext::default();
        assert_eq!(
            WorkflowDsl::resolve_template("Key: {{env.ARGENTOR_TEST_DSL_VAR}}", &ctx),
            "Key: secret123"
        );
        std::env::remove_var("ARGENTOR_TEST_DSL_VAR");
    }

    #[test]
    fn test_template_variables() {
        let mut ctx = TemplateContext::default();
        ctx.variables
            .insert("project".to_string(), "argentor".to_string());
        assert_eq!(
            WorkflowDsl::resolve_template("Project: {{variables.project}}", &ctx),
            "Project: argentor"
        );
        assert_eq!(
            WorkflowDsl::resolve_template("Project: {{var.project}}", &ctx),
            "Project: argentor"
        );
    }

    #[test]
    fn test_template_unknown_placeholder() {
        let ctx = TemplateContext::default();
        assert_eq!(WorkflowDsl::resolve_template("{{unknown.thing}}", &ctx), "");
    }

    #[test]
    fn test_template_no_placeholders() {
        let ctx = TemplateContext::default();
        assert_eq!(
            WorkflowDsl::resolve_template("plain text", &ctx),
            "plain text"
        );
    }

    #[test]
    fn test_template_multiple_placeholders() {
        let mut ctx = TemplateContext {
            input: "code.rs".to_string(),
            ..Default::default()
        };
        ctx.step_outputs
            .insert("lint".to_string(), "3 warnings".to_string());
        let result =
            WorkflowDsl::resolve_template("File: {{input}}, Lint: {{steps.lint.output}}", &ctx);
        assert_eq!(result, "File: code.rs, Lint: 3 warnings");
    }

    #[test]
    fn test_template_unclosed_placeholder() {
        let ctx = TemplateContext::default();
        // Unclosed `{{` should be kept as-is.
        assert_eq!(
            WorkflowDsl::resolve_template("begin {{no_close", &ctx),
            "begin {{no_close"
        );
    }

    #[test]
    fn test_template_empty_env_var() {
        let ctx = TemplateContext::default();
        // Non-existent env var resolves to empty string.
        let result = WorkflowDsl::resolve_template("{{env.DOES_NOT_EXIST_XYZZY_1234}}", &ctx);
        assert_eq!(result, "");
    }

    // -- File I/O tests -----------------------------------------------------

    #[test]
    fn test_parse_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(minimal_toml().as_bytes()).unwrap();
        let wf = WorkflowDsl::parse_file(tmp.path()).unwrap();
        assert_eq!(wf.workflow.name, "minimal");
    }

    #[test]
    fn test_load_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(full_toml().as_bytes()).unwrap();
        let def = WorkflowDsl::load_file(tmp.path()).unwrap();
        assert_eq!(def.id, "full-pipeline");
        assert_eq!(def.steps.len(), 6);
    }

    #[test]
    fn test_parse_file_not_found() {
        let result = WorkflowDsl::parse_file(Path::new("/tmp/nonexistent_workflow.toml"));
        assert!(result.is_err());
    }

    // -- Integration: compile and run with WorkflowEngine -------------------

    #[tokio::test]
    async fn test_compile_and_run_with_engine() {
        use crate::workflow::WorkflowEngine;

        let def = WorkflowDsl::load(minimal_toml()).unwrap();
        let engine = WorkflowEngine::new();
        engine.register_workflow(def).await;

        let run_id = engine
            .start("minimal", serde_json::json!({}))
            .await
            .unwrap();
        let result = engine.run_to_completion(&run_id).await.unwrap();
        assert_eq!(result.status, crate::workflow::RunStatus::Completed);
    }

    // -- Validation error Display -------------------------------------------

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError {
            step_id: Some("s1".to_string()),
            message: "missing role".to_string(),
            severity: ValidationSeverity::Error,
        };
        assert_eq!(format!("{err}"), "[ERROR] step 's1': missing role");

        let warn = ValidationError {
            step_id: None,
            message: "something odd".to_string(),
            severity: ValidationSeverity::Warning,
        };
        assert_eq!(format!("{warn}"), "[WARNING] something odd");
    }

    // -- Topological sort unit tests ----------------------------------------

    #[test]
    fn test_topological_sort_linear() {
        let steps = vec![
            StepToml {
                id: "c".into(),
                name: "C".into(),
                step_type: "agent".into(),
                depends_on: Some(vec!["b".into()]),
                role: None,
                prompt: None,
                tools: None,
                model: None,
                url: None,
                method: None,
                headers: None,
                body: None,
                condition: None,
                on_true: None,
                on_false: None,
                delay_seconds: None,
                channel: None,
                message: None,
                team: None,
                timeout_seconds: None,
                retry: None,
                on_failure: None,
                run_if: None,
            },
            StepToml {
                id: "a".into(),
                name: "A".into(),
                step_type: "agent".into(),
                depends_on: None,
                role: None,
                prompt: None,
                tools: None,
                model: None,
                url: None,
                method: None,
                headers: None,
                body: None,
                condition: None,
                on_true: None,
                on_false: None,
                delay_seconds: None,
                channel: None,
                message: None,
                team: None,
                timeout_seconds: None,
                retry: None,
                on_failure: None,
                run_if: None,
            },
            StepToml {
                id: "b".into(),
                name: "B".into(),
                step_type: "agent".into(),
                depends_on: Some(vec!["a".into()]),
                role: None,
                prompt: None,
                tools: None,
                model: None,
                url: None,
                method: None,
                headers: None,
                body: None,
                condition: None,
                on_true: None,
                on_false: None,
                delay_seconds: None,
                channel: None,
                message: None,
                team: None,
                timeout_seconds: None,
                retry: None,
                on_failure: None,
                run_if: None,
            },
        ];
        let sorted = topological_sort(&steps);
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }
}
