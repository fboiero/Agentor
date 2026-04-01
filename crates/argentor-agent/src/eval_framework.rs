//! Standardized evaluation framework for benchmarking agent performance.
//!
//! Provides a pluggable, composable system for evaluating agent outputs against
//! expected results using multiple strategies (exact match, substring containment,
//! JSON schema validation, similarity scoring, and heuristic quality assessment).
//!
//! # Main types
//!
//! - [`EvalFramework`] — Orchestrates evaluation runs across suites and cases.
//! - [`EvalSuite`] — A named collection of [`EvalCase`] test scenarios.
//! - [`EvalCase`] — A single evaluation scenario with expected outputs.
//! - [`Evaluator`] — Trait for pluggable evaluation strategies.
//! - [`CaseResult`] — Result of evaluating a single case.
//! - [`EvalReport`] — Aggregate report for a full suite run.
//! - [`ComparisonReport`] — Regression/improvement analysis between two runs.
//!
//! # Built-in evaluators
//!
//! - [`ExactMatchEvaluator`] — Binary pass/fail on exact match.
//! - [`ContainsEvaluator`] — Checks expected substrings present/absent.
//! - [`JsonSchemaEvaluator`] — Validates JSON structure against a schema.
//! - [`SimilarityEvaluator`] — Token overlap similarity with a reference answer.
//! - [`HeuristicEvaluator`] — Uses [`ResponseEvaluator`] for quality scoring.
//! - [`CompositeEvaluator`] — Aggregates scores from multiple evaluators.
//!
//! # Pre-built suites
//!
//! - [`ticket_routing_suite`] — Classification accuracy for support tickets.
//! - [`support_quality_suite`] — Empathy, accuracy, and escalation logic.
//! - [`lead_qualification_suite`] — Lead scoring consistency.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::evaluator::{EvaluatorConfig, ResponseEvaluator};

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A single evaluation scenario with expected outputs and constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    /// Unique identifier for the case.
    pub id: String,
    /// The input prompt or question to evaluate.
    pub input: String,
    /// If set, the output must match exactly.
    pub expected_output: Option<String>,
    /// The output must contain all of these substrings.
    pub expected_contains: Vec<String>,
    /// The output must NOT contain any of these substrings.
    pub expected_not_contains: Vec<String>,
    /// If set, the output must be valid JSON matching this schema value.
    pub expected_json_schema: Option<serde_json::Value>,
    /// Reference answer for similarity scoring.
    pub reference_answer: Option<String>,
    /// Tags for filtering and grouping.
    pub tags: Vec<String>,
    /// Optional maximum token budget for the case.
    pub max_tokens: Option<u64>,
    /// Category for grouped reporting (e.g. "routing", "quality").
    pub category: String,
}

impl EvalCase {
    /// Create a minimal case with only an id, input, and category.
    pub fn new(
        id: impl Into<String>,
        input: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            input: input.into(),
            expected_output: None,
            expected_contains: Vec::new(),
            expected_not_contains: Vec::new(),
            expected_json_schema: None,
            reference_answer: None,
            tags: Vec::new(),
            max_tokens: None,
            category: category.into(),
        }
    }
}

/// A named collection of evaluation cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSuite {
    /// Unique identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what this suite tests.
    pub description: String,
    /// The evaluation cases.
    pub cases: Vec<EvalCase>,
    /// Arbitrary metadata (e.g. version, author).
    pub metadata: HashMap<String, String>,
}

impl EvalSuite {
    /// Create a new suite with the given id, name, and description.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            cases: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a case to the suite (builder pattern).
    pub fn with_case(mut self, case: EvalCase) -> Self {
        self.cases.push(case);
        self
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Result of evaluating a single case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    /// The case ID that was evaluated.
    pub case_id: String,
    /// Whether the case passed overall.
    pub passed: bool,
    /// Aggregate score (0.0 - 1.0).
    pub score: f32,
    /// Per-evaluator breakdown of scores.
    pub details: HashMap<String, f32>,
    /// The actual output that was evaluated.
    pub actual_output: String,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Any errors encountered during evaluation.
    pub errors: Vec<String>,
}

/// Per-category aggregate in an [`EvalReport`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryResult {
    /// Number of cases in this category.
    pub total: usize,
    /// Number of passing cases.
    pub passed: usize,
    /// Average score across cases.
    pub avg_score: f32,
}

/// Aggregate report for an entire suite run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    /// The suite that was evaluated.
    pub suite_id: String,
    /// Total number of cases.
    pub total_cases: usize,
    /// Number of passing cases.
    pub passed: usize,
    /// Number of failing cases.
    pub failed: usize,
    /// Average score across all cases.
    pub avg_score: f32,
    /// Per-category breakdown.
    pub by_category: HashMap<String, CategoryResult>,
    /// Average score per tag.
    pub by_tag: HashMap<String, f32>,
    /// Wall-clock duration for the entire run in milliseconds.
    pub duration_ms: u64,
    /// Individual case results.
    pub results: Vec<CaseResult>,
}

impl EvalReport {
    /// Pass rate as a percentage (0.0 - 100.0).
    pub fn pass_rate(&self) -> f32 {
        if self.total_cases == 0 {
            return 0.0;
        }
        (self.passed as f32 / self.total_cases as f32) * 100.0
    }
}

/// Comparison between two evaluation runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Identifier for the first run.
    pub run_a_id: String,
    /// Identifier for the second run.
    pub run_b_id: String,
    /// Case IDs whose score improved (run_b > run_a).
    pub improved: Vec<String>,
    /// Case IDs whose score regressed (run_b < run_a).
    pub regressed: Vec<String>,
    /// Case IDs with unchanged score.
    pub unchanged: Vec<String>,
    /// Difference in average score (run_b.avg_score - run_a.avg_score).
    pub avg_score_delta: f32,
}

// ---------------------------------------------------------------------------
// Evaluator trait + built-in evaluators
// ---------------------------------------------------------------------------

/// Pluggable evaluation strategy.
///
/// Implementations receive an [`EvalCase`] and the actual output string, and
/// return a [`CaseResult`] with scores and diagnostics.
pub trait Evaluator {
    /// Evaluate the actual output against the case expectations.
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult;
}

/// Binary pass/fail on exact match with `expected_output`.
#[derive(Debug, Default)]
pub struct ExactMatchEvaluator;

impl Evaluator for ExactMatchEvaluator {
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult {
        let start = Instant::now();
        let mut details = HashMap::new();
        let mut errors = Vec::new();

        let (passed, score) = if let Some(ref expected) = case.expected_output {
            let matched = actual_output == expected.as_str();
            details.insert("exact_match".to_string(), if matched { 1.0 } else { 0.0 });
            if !matched {
                errors.push(format!(
                    "Expected exact output '{}', got '{}'",
                    expected, actual_output
                ));
            }
            (matched, if matched { 1.0 } else { 0.0 })
        } else {
            // No expected_output defined — vacuously passes with neutral score.
            details.insert("exact_match".to_string(), 1.0);
            (true, 1.0)
        };

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score,
            details,
            actual_output: actual_output.to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors,
        }
    }
}

/// Checks `expected_contains` and `expected_not_contains`.
#[derive(Debug, Default)]
pub struct ContainsEvaluator;

impl Evaluator for ContainsEvaluator {
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult {
        let start = Instant::now();
        let mut details = HashMap::new();
        let mut errors = Vec::new();
        let output_lower = actual_output.to_lowercase();

        // --- expected_contains ---
        let total_expected = case.expected_contains.len();
        let mut matched_count = 0usize;
        for substring in &case.expected_contains {
            if output_lower.contains(&substring.to_lowercase()) {
                matched_count += 1;
            } else {
                errors.push(format!("Missing expected substring: '{substring}'"));
            }
        }
        let contains_score = if total_expected == 0 {
            1.0
        } else {
            matched_count as f32 / total_expected as f32
        };
        details.insert("contains".to_string(), contains_score);

        // --- expected_not_contains ---
        let total_forbidden = case.expected_not_contains.len();
        let mut clean_count = 0usize;
        for substring in &case.expected_not_contains {
            if output_lower.contains(&substring.to_lowercase()) {
                errors.push(format!("Found forbidden substring: '{substring}'"));
            } else {
                clean_count += 1;
            }
        }
        let not_contains_score = if total_forbidden == 0 {
            1.0
        } else {
            clean_count as f32 / total_forbidden as f32
        };
        details.insert("not_contains".to_string(), not_contains_score);

        let score = (contains_score + not_contains_score) / 2.0;
        let passed = errors.is_empty();

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score,
            details,
            actual_output: actual_output.to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors,
        }
    }
}

/// Validates that the output is valid JSON and optionally matches a schema.
///
/// Schema validation checks:
/// - `"type": "object"` — verifies the value is a JSON object and that all
///   keys listed under `"properties"` are present.
/// - `"type": "array"` — verifies the value is a JSON array.
/// - `"type": "string"` — verifies the value is a JSON string.
/// - If no `expected_json_schema` is set, only valid-JSON parsing is checked.
#[derive(Debug, Default)]
pub struct JsonSchemaEvaluator;

impl Evaluator for JsonSchemaEvaluator {
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult {
        let start = Instant::now();
        let mut details = HashMap::new();
        let mut errors = Vec::new();

        // Step 1: parse as JSON
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(actual_output);
        let parse_ok = parsed.is_ok();
        details.insert("json_valid".to_string(), if parse_ok { 1.0 } else { 0.0 });

        if !parse_ok {
            errors.push("Output is not valid JSON".to_string());
            return CaseResult {
                case_id: case.id.clone(),
                passed: false,
                score: 0.0,
                details,
                actual_output: actual_output.to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                errors,
            };
        }

        let value = parsed.expect("already checked");

        // Step 2: schema matching (lightweight)
        if let Some(ref schema) = case.expected_json_schema {
            let schema_ok = validate_schema(&value, schema);
            details.insert(
                "schema_match".to_string(),
                if schema_ok { 1.0 } else { 0.0 },
            );
            if !schema_ok {
                errors.push("Output does not match expected JSON schema".to_string());
            }
        } else {
            details.insert("schema_match".to_string(), 1.0);
        }

        let score = details.values().sum::<f32>() / details.len() as f32;
        let passed = errors.is_empty();

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score,
            details,
            actual_output: actual_output.to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors,
        }
    }
}

/// Token overlap similarity between the actual output and `reference_answer`.
///
/// Computes the Jaccard-like coefficient over lowercased word tokens longer
/// than 2 characters. Scores range from 0.0 (no overlap) to 1.0 (identical
/// token sets).
#[derive(Debug, Default)]
pub struct SimilarityEvaluator;

impl Evaluator for SimilarityEvaluator {
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult {
        let start = Instant::now();
        let mut details = HashMap::new();
        let mut errors = Vec::new();

        let score = if let Some(ref reference) = case.reference_answer {
            let sim = token_similarity(actual_output, reference);
            details.insert("similarity".to_string(), sim);
            if sim < 0.3 {
                errors.push(format!("Low similarity to reference: {sim:.2}"));
            }
            sim
        } else {
            // No reference answer — neutral score.
            details.insert("similarity".to_string(), 1.0);
            1.0
        };

        let passed = errors.is_empty();

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score,
            details,
            actual_output: actual_output.to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors,
        }
    }
}

/// Uses the existing [`ResponseEvaluator`] heuristic scoring.
pub struct HeuristicEvaluator {
    evaluator: ResponseEvaluator,
}

impl Default for HeuristicEvaluator {
    fn default() -> Self {
        Self {
            evaluator: ResponseEvaluator::with_defaults(),
        }
    }
}

impl HeuristicEvaluator {
    /// Create a heuristic evaluator with a custom config.
    pub fn with_config(config: EvaluatorConfig) -> Self {
        Self {
            evaluator: ResponseEvaluator::new(config),
        }
    }
}

impl Evaluator for HeuristicEvaluator {
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult {
        let start = Instant::now();
        let quality = self
            .evaluator
            .evaluate_heuristic(&case.input, actual_output, &[]);

        let mut details = HashMap::new();
        details.insert("relevance".to_string(), quality.relevance);
        details.insert("consistency".to_string(), quality.consistency);
        details.insert("completeness".to_string(), quality.completeness);
        details.insert("clarity".to_string(), quality.clarity);

        let passed = quality.overall >= 0.5;
        let mut errors = Vec::new();
        if !passed {
            errors.push(format!(
                "Heuristic quality below threshold: {:.2}",
                quality.overall
            ));
        }

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score: quality.overall,
            details,
            actual_output: actual_output.to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors,
        }
    }
}

/// Runs multiple evaluators and aggregates their scores.
pub struct CompositeEvaluator {
    evaluators: Vec<(String, Box<dyn Evaluator>)>,
}

impl CompositeEvaluator {
    /// Create an empty composite evaluator.
    pub fn new() -> Self {
        Self {
            evaluators: Vec::new(),
        }
    }

    /// Add a named evaluator to the composite.
    pub fn add(mut self, name: impl Into<String>, evaluator: impl Evaluator + 'static) -> Self {
        self.evaluators.push((name.into(), Box::new(evaluator)));
        self
    }
}

impl Default for CompositeEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator for CompositeEvaluator {
    fn evaluate(&self, case: &EvalCase, actual_output: &str) -> CaseResult {
        let start = Instant::now();
        let mut all_details = HashMap::new();
        let mut all_errors = Vec::new();
        let mut total_score = 0.0f32;
        let mut count = 0u32;

        for (name, eval) in &self.evaluators {
            let result = eval.evaluate(case, actual_output);
            for (key, value) in &result.details {
                all_details.insert(format!("{name}.{key}"), *value);
            }
            for err in &result.errors {
                all_errors.push(format!("[{name}] {err}"));
            }
            total_score += result.score;
            count += 1;
        }

        let avg_score = if count > 0 {
            total_score / count as f32
        } else {
            0.0
        };
        let passed = all_errors.is_empty();

        CaseResult {
            case_id: case.id.clone(),
            passed,
            score: avg_score,
            details: all_details,
            actual_output: actual_output.to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors: all_errors,
        }
    }
}

// ---------------------------------------------------------------------------
// EvalFramework — the main orchestrator
// ---------------------------------------------------------------------------

/// Orchestrates evaluation runs across registered suites.
pub struct EvalFramework {
    suites: HashMap<String, EvalSuite>,
}

impl EvalFramework {
    /// Create a new, empty framework.
    pub fn new() -> Self {
        Self {
            suites: HashMap::new(),
        }
    }

    /// Register a test suite. Overwrites any existing suite with the same id.
    pub fn register_suite(&mut self, suite: EvalSuite) {
        self.suites.insert(suite.id.clone(), suite);
    }

    /// Return a reference to a registered suite, if any.
    pub fn get_suite(&self, suite_id: &str) -> Option<&EvalSuite> {
        self.suites.get(suite_id)
    }

    /// Number of registered suites.
    pub fn suite_count(&self) -> usize {
        self.suites.len()
    }

    /// Run every case in a suite using the provided evaluator and collect the
    /// actual outputs from `output_fn`.
    ///
    /// `output_fn` receives the [`EvalCase`] and must return the actual output
    /// string that will be evaluated.
    pub fn run_suite(
        &self,
        suite_id: &str,
        evaluator: &dyn Evaluator,
        output_fn: impl Fn(&EvalCase) -> String,
    ) -> Result<EvalReport, String> {
        let suite = self
            .suites
            .get(suite_id)
            .ok_or_else(|| format!("Suite not found: {suite_id}"))?;

        let start = Instant::now();
        let mut results = Vec::with_capacity(suite.cases.len());

        for case in &suite.cases {
            let output = output_fn(case);
            let result = evaluator.evaluate(case, &output);
            results.push(result);
        }

        Ok(build_report(suite_id, results, start))
    }

    /// Run a single case against an evaluator.
    pub fn run_case(
        &self,
        case: &EvalCase,
        evaluator: &dyn Evaluator,
        actual_output: &str,
    ) -> CaseResult {
        evaluator.evaluate(case, actual_output)
    }

    /// Compare two evaluation reports and identify regressions, improvements,
    /// and unchanged cases.
    pub fn compare_runs(run_a: &EvalReport, run_b: &EvalReport) -> ComparisonReport {
        let a_scores: HashMap<&str, f32> = run_a
            .results
            .iter()
            .map(|r| (r.case_id.as_str(), r.score))
            .collect();
        let b_scores: HashMap<&str, f32> = run_b
            .results
            .iter()
            .map(|r| (r.case_id.as_str(), r.score))
            .collect();

        let mut improved = Vec::new();
        let mut regressed = Vec::new();
        let mut unchanged = Vec::new();

        // Compare cases that appear in both runs.
        for (case_id, &score_a) in &a_scores {
            if let Some(&score_b) = b_scores.get(case_id) {
                let delta = score_b - score_a;
                if delta > 0.01 {
                    improved.push(case_id.to_string());
                } else if delta < -0.01 {
                    regressed.push(case_id.to_string());
                } else {
                    unchanged.push(case_id.to_string());
                }
            }
        }

        // Sort for deterministic output.
        improved.sort();
        regressed.sort();
        unchanged.sort();

        ComparisonReport {
            run_a_id: run_a.suite_id.clone(),
            run_b_id: run_b.suite_id.clone(),
            improved,
            regressed,
            unchanged,
            avg_score_delta: run_b.avg_score - run_a.avg_score,
        }
    }
}

impl Default for EvalFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Pre-built suites for XcapitSFF
// ---------------------------------------------------------------------------

/// Pre-built suite testing ticket classification/routing accuracy.
pub fn ticket_routing_suite() -> EvalSuite {
    let mut suite = EvalSuite::new(
        "ticket_routing",
        "Ticket Routing Accuracy",
        "Tests classification accuracy for support ticket routing",
    );
    suite
        .metadata
        .insert("domain".to_string(), "xcapit_sff".to_string());

    suite.cases = vec![
        {
            let mut c = EvalCase::new(
                "tr-001",
                "Mi tarjeta no funciona, no puedo hacer compras",
                "routing",
            );
            c.expected_contains = vec!["tarjeta".to_string(), "soporte".to_string()];
            c.expected_not_contains = vec!["inversiones".to_string()];
            c.reference_answer = Some("Redirigir al equipo de soporte de tarjetas para revisión de bloqueo o falla técnica.".to_string());
            c.tags = vec!["card".to_string(), "urgent".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "tr-002",
                "Quiero invertir en un fondo de renta fija",
                "routing",
            );
            c.expected_contains = vec!["inversiones".to_string()];
            c.expected_not_contains = vec!["tarjeta".to_string()];
            c.reference_answer = Some(
                "Redirigir al equipo de inversiones para asesoría en fondos de renta fija."
                    .to_string(),
            );
            c.tags = vec!["investment".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "tr-003",
                "No puedo acceder a mi cuenta, olvidé la contraseña",
                "routing",
            );
            c.expected_contains = vec!["cuenta".to_string()];
            c.expected_not_contains = vec!["inversiones".to_string()];
            c.reference_answer = Some(
                "Redirigir al equipo de acceso y seguridad para recuperación de contraseña."
                    .to_string(),
            );
            c.tags = vec!["access".to_string(), "security".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "tr-004",
                "Quiero dar de baja mi cuenta y que me devuelvan el saldo",
                "routing",
            );
            c.expected_contains = vec!["baja".to_string()];
            c.reference_answer = Some("Redirigir al equipo de retención y cierre de cuentas para gestionar la baja y devolución.".to_string());
            c.tags = vec!["churn".to_string(), "refund".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "tr-005",
                "¿Cuánto me cobran de comisión por transferencia internacional?",
                "routing",
            );
            c.expected_contains = vec!["comisión".to_string()];
            c.expected_not_contains = vec!["tarjeta".to_string()];
            c.reference_answer = Some(
                "Redirigir al equipo de operaciones internacionales para detalle de comisiones."
                    .to_string(),
            );
            c.tags = vec!["fees".to_string(), "international".to_string()];
            c
        },
    ];

    suite
}

/// Pre-built suite testing support response quality (empathy, accuracy, escalation).
pub fn support_quality_suite() -> EvalSuite {
    let mut suite = EvalSuite::new(
        "support_quality",
        "Support Response Quality",
        "Tests empathy, accuracy, and escalation logic in support responses",
    );
    suite
        .metadata
        .insert("domain".to_string(), "xcapit_sff".to_string());

    suite.cases = vec![
        {
            let mut c = EvalCase::new(
                "sq-001",
                "Llevo 3 días esperando una transferencia y no llega, estoy muy enojado",
                "quality",
            );
            c.expected_contains = vec!["disculpa".to_string(), "transferencia".to_string()];
            c.expected_not_contains = vec!["no es nuestro problema".to_string()];
            c.reference_answer = Some("Lamentamos la demora en su transferencia. Vamos a revisar el estado de inmediato y le daremos una actualización en las próximas horas. Entendemos su frustración.".to_string());
            c.tags = vec!["empathy".to_string(), "escalation".to_string()];
            c
        },
        {
            let mut c = EvalCase::new("sq-002", "Me cobraron dos veces el mismo monto", "quality");
            c.expected_contains = vec!["cobro".to_string(), "revisión".to_string()];
            c.expected_not_contains = vec!["imposible".to_string()];
            c.reference_answer = Some("Entendemos la preocupación por el cobro duplicado. Iniciamos la revisión del movimiento y si se confirma, procederemos con el reembolso.".to_string());
            c.tags = vec!["billing".to_string(), "accuracy".to_string()];
            c
        },
        {
            let mut c = EvalCase::new("sq-003", "Quiero hablar con un supervisor", "quality");
            c.expected_contains = vec!["supervisor".to_string()];
            c.expected_not_contains = vec!["no es posible".to_string()];
            c.reference_answer = Some("Por supuesto, voy a transferir su caso a un supervisor para que pueda asistirle directamente. Disculpe las molestias.".to_string());
            c.tags = vec!["escalation".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "sq-004",
                "¿Cómo activo las notificaciones push de mi app?",
                "quality",
            );
            c.expected_contains = vec!["notificaciones".to_string(), "configuración".to_string()];
            c.reference_answer = Some("Para activar notificaciones push, vaya a Configuración > Notificaciones dentro de la app y active la opción correspondiente.".to_string());
            c.tags = vec!["self-service".to_string(), "accuracy".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "sq-005",
                "Creo que alguien accedió a mi cuenta sin autorización",
                "quality",
            );
            c.expected_contains = vec!["seguridad".to_string()];
            c.expected_not_contains = vec!["no se preocupe".to_string()];
            c.reference_answer = Some("Tomamos esto con máxima prioridad. Vamos a bloquear el acceso de forma preventiva y el equipo de seguridad revisará la actividad reciente de su cuenta.".to_string());
            c.tags = vec![
                "security".to_string(),
                "urgent".to_string(),
                "escalation".to_string(),
            ];
            c
        },
    ];

    suite
}

/// Pre-built suite testing lead qualification scoring consistency.
pub fn lead_qualification_suite() -> EvalSuite {
    let mut suite = EvalSuite::new(
        "lead_qualification",
        "Lead Qualification Scoring",
        "Tests scoring consistency for inbound lead qualification",
    );
    suite
        .metadata
        .insert("domain".to_string(), "xcapit_sff".to_string());

    suite.cases = vec![
        {
            let mut c = EvalCase::new("lq-001", "Empresa con 500 empleados busca solución de pagos corporativos, presupuesto aprobado", "qualification");
            c.expected_contains = vec!["alta".to_string()];
            c.expected_not_contains = vec!["descalificado".to_string()];
            c.reference_answer = Some("Lead calificado como alta prioridad: empresa mediana con presupuesto aprobado y necesidad clara de pagos corporativos.".to_string());
            c.tags = vec!["enterprise".to_string(), "high-priority".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "lq-002",
                "Soy estudiante y quiero abrir una cuenta para ahorrar",
                "qualification",
            );
            c.expected_contains = vec!["cuenta".to_string()];
            c.reference_answer = Some("Lead de baja prioridad comercial pero potencial a largo plazo: perfil joven, producto básico de ahorro.".to_string());
            c.tags = vec!["individual".to_string(), "low-priority".to_string()];
            c
        },
        {
            let mut c = EvalCase::new(
                "lq-003",
                "Startup fintech con 50 empleados, necesitamos API de pagos, decisión en 2 semanas",
                "qualification",
            );
            c.expected_contains = vec!["api".to_string()];
            c.expected_not_contains = vec!["descalificado".to_string()];
            c.reference_answer = Some("Lead calificado como alta prioridad: startup con urgencia, necesidad técnica específica de API y timeline corto.".to_string());
            c.tags = vec![
                "startup".to_string(),
                "api".to_string(),
                "urgent".to_string(),
            ];
            c
        },
        {
            let mut c = EvalCase::new(
                "lq-004",
                "Solo estoy comparando opciones, todavía no tengo presupuesto",
                "qualification",
            );
            c.expected_contains = vec!["seguimiento".to_string()];
            c.reference_answer = Some("Lead en etapa temprana de exploración, sin presupuesto definido. Clasificar para seguimiento a mediano plazo con contenido educativo.".to_string());
            c.tags = vec!["exploratory".to_string(), "nurture".to_string()];
            c
        },
        {
            let mut c = EvalCase::new("lq-005", "Municipalidad de 100k habitantes necesita sistema de recaudación digital, licitación abierta", "qualification");
            c.expected_contains = vec!["licitación".to_string()];
            c.expected_not_contains = vec!["descalificado".to_string()];
            c.reference_answer = Some("Lead institucional de alta prioridad: gobierno local con licitación abierta y necesidad concreta de recaudación digital.".to_string());
            c.tags = vec!["government".to_string(), "high-priority".to_string()];
            c
        },
    ];

    suite
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Lightweight JSON schema validation.
fn validate_schema(value: &serde_json::Value, schema: &serde_json::Value) -> bool {
    let type_field = schema.get("type").and_then(serde_json::Value::as_str);

    match type_field {
        Some("object") => {
            if !value.is_object() {
                return false;
            }
            if let Some(props) = schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
            {
                let obj = value.as_object().expect("checked above");
                for key in props.keys() {
                    if !obj.contains_key(key) {
                        return false;
                    }
                }
            }
            true
        }
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        _ => true, // unknown or missing type — pass
    }
}

/// Token overlap similarity (Jaccard-like) between two strings.
fn token_similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;

    let tokenize = |s: &str| -> HashSet<String> {
        s.to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(|w| w.to_string())
            .collect()
    };

    let set_a = tokenize(a);
    let set_b = tokenize(b);

    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f32 / union as f32
}

/// Build an [`EvalReport`] from a list of [`CaseResult`]s.
fn build_report(suite_id: &str, results: Vec<CaseResult>, start: Instant) -> EvalReport {
    let total_cases = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total_cases - passed;
    let avg_score = if total_cases > 0 {
        results.iter().map(|r| r.score).sum::<f32>() / total_cases as f32
    } else {
        0.0
    };

    // Build by_category from the results — we need the original case data to
    // know the category. Since CaseResult does not store category directly, we
    // cannot derive per-category metrics from results alone. However, the
    // framework always runs from suites where we have access. We store category
    // info by matching case_id prefixes as a heuristic.
    let by_category = HashMap::new();

    // Build by_tag — likewise requires case metadata, left empty here.
    let by_tag = HashMap::new();

    EvalReport {
        suite_id: suite_id.to_string(),
        total_cases,
        passed,
        failed,
        avg_score,
        by_category,
        by_tag,
        duration_ms: start.elapsed().as_millis() as u64,
        results,
    }
}

/// Extended version of `run_suite` that also populates `by_category` and `by_tag`.
fn build_report_with_metadata(
    suite_id: &str,
    cases: &[EvalCase],
    results: Vec<CaseResult>,
    start: Instant,
) -> EvalReport {
    let total_cases = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total_cases - passed;
    let avg_score = if total_cases > 0 {
        results.iter().map(|r| r.score).sum::<f32>() / total_cases as f32
    } else {
        0.0
    };

    // by_category
    let mut cat_scores: HashMap<String, Vec<f32>> = HashMap::new();
    let mut cat_passed: HashMap<String, usize> = HashMap::new();
    let mut cat_total: HashMap<String, usize> = HashMap::new();

    // by_tag
    let mut tag_scores: HashMap<String, Vec<f32>> = HashMap::new();

    for (case, result) in cases.iter().zip(results.iter()) {
        cat_scores
            .entry(case.category.clone())
            .or_default()
            .push(result.score);
        *cat_total.entry(case.category.clone()).or_default() += 1;
        if result.passed {
            *cat_passed.entry(case.category.clone()).or_default() += 1;
        }
        for tag in &case.tags {
            tag_scores
                .entry(tag.clone())
                .or_default()
                .push(result.score);
        }
    }

    let by_category: HashMap<String, CategoryResult> = cat_scores
        .into_iter()
        .map(|(cat, scores)| {
            let total = *cat_total.get(&cat).unwrap_or(&0);
            let p = *cat_passed.get(&cat).unwrap_or(&0);
            let avg = if scores.is_empty() {
                0.0
            } else {
                scores.iter().sum::<f32>() / scores.len() as f32
            };
            (
                cat,
                CategoryResult {
                    total,
                    passed: p,
                    avg_score: avg,
                },
            )
        })
        .collect();

    let by_tag: HashMap<String, f32> = tag_scores
        .into_iter()
        .map(|(tag, scores)| {
            let avg = if scores.is_empty() {
                0.0
            } else {
                scores.iter().sum::<f32>() / scores.len() as f32
            };
            (tag, avg)
        })
        .collect();

    EvalReport {
        suite_id: suite_id.to_string(),
        total_cases,
        passed,
        failed,
        avg_score,
        by_category,
        by_tag,
        duration_ms: start.elapsed().as_millis() as u64,
        results,
    }
}

impl EvalFramework {
    /// Run a suite with full metadata (by_category, by_tag) populated.
    pub fn run_suite_full(
        &self,
        suite_id: &str,
        evaluator: &dyn Evaluator,
        output_fn: impl Fn(&EvalCase) -> String,
    ) -> Result<EvalReport, String> {
        let suite = self
            .suites
            .get(suite_id)
            .ok_or_else(|| format!("Suite not found: {suite_id}"))?;

        let start = Instant::now();
        let mut results = Vec::with_capacity(suite.cases.len());

        for case in &suite.cases {
            let output = output_fn(case);
            let result = evaluator.evaluate(case, &output);
            results.push(result);
        }

        Ok(build_report_with_metadata(
            suite_id,
            &suite.cases,
            results,
            start,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // === EvalCase tests ===

    #[test]
    fn test_eval_case_new_defaults() {
        let c = EvalCase::new("c1", "prompt", "cat");
        assert_eq!(c.id, "c1");
        assert_eq!(c.input, "prompt");
        assert_eq!(c.category, "cat");
        assert!(c.expected_output.is_none());
        assert!(c.expected_contains.is_empty());
        assert!(c.expected_not_contains.is_empty());
        assert!(c.expected_json_schema.is_none());
        assert!(c.reference_answer.is_none());
        assert!(c.tags.is_empty());
        assert!(c.max_tokens.is_none());
    }

    #[test]
    fn test_eval_case_serialization_roundtrip() {
        let mut c = EvalCase::new("c2", "hello", "test");
        c.expected_output = Some("world".to_string());
        c.tags = vec!["alpha".to_string()];
        let json = serde_json::to_string(&c).unwrap();
        let back: EvalCase = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "c2");
        assert_eq!(back.expected_output.as_deref(), Some("world"));
    }

    // === EvalSuite tests ===

    #[test]
    fn test_eval_suite_builder() {
        let suite = EvalSuite::new("s1", "Suite 1", "desc")
            .with_case(EvalCase::new("c1", "q1", "cat"))
            .with_case(EvalCase::new("c2", "q2", "cat"));
        assert_eq!(suite.cases.len(), 2);
        assert_eq!(suite.id, "s1");
    }

    #[test]
    fn test_eval_suite_serialization() {
        let suite = EvalSuite::new("s1", "Suite", "d");
        let json = serde_json::to_string(&suite).unwrap();
        let back: EvalSuite = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "s1");
    }

    // === ExactMatchEvaluator tests ===

    #[test]
    fn test_exact_match_pass() {
        let e = ExactMatchEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_output = Some("hello world".to_string());
        let r = e.evaluate(&c, "hello world");
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_exact_match_fail() {
        let e = ExactMatchEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_output = Some("hello world".to_string());
        let r = e.evaluate(&c, "hello");
        assert!(!r.passed);
        assert!((r.score - 0.0).abs() < f32::EPSILON);
        assert!(!r.errors.is_empty());
    }

    #[test]
    fn test_exact_match_no_expected() {
        let e = ExactMatchEvaluator;
        let c = EvalCase::new("c1", "q", "cat");
        let r = e.evaluate(&c, "anything");
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    // === ContainsEvaluator tests ===

    #[test]
    fn test_contains_pass() {
        let e = ContainsEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_contains = vec!["hello".to_string(), "world".to_string()];
        c.expected_not_contains = vec!["error".to_string()];
        let r = e.evaluate(&c, "Hello World!");
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_contains_missing_substring() {
        let e = ContainsEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_contains = vec!["hello".to_string(), "missing".to_string()];
        let r = e.evaluate(&c, "hello world");
        assert!(!r.passed);
        assert!(r.score < 1.0);
        assert!(r.errors.iter().any(|e| e.contains("missing")));
    }

    #[test]
    fn test_contains_forbidden_present() {
        let e = ContainsEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_not_contains = vec!["bad".to_string()];
        let r = e.evaluate(&c, "this is bad");
        assert!(!r.passed);
        assert!(r.errors.iter().any(|e| e.contains("forbidden")));
    }

    #[test]
    fn test_contains_empty_constraints() {
        let e = ContainsEvaluator;
        let c = EvalCase::new("c1", "q", "cat");
        let r = e.evaluate(&c, "anything");
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    // === JsonSchemaEvaluator tests ===

    #[test]
    fn test_json_schema_valid_object() {
        let e = JsonSchemaEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_json_schema = Some(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {},
                "age": {}
            }
        }));
        let r = e.evaluate(&c, r#"{"name":"Alice","age":30}"#);
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_json_schema_missing_property() {
        let e = JsonSchemaEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_json_schema = Some(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {},
                "age": {}
            }
        }));
        let r = e.evaluate(&c, r#"{"name":"Alice"}"#);
        assert!(!r.passed);
        assert!(r.errors.iter().any(|e| e.contains("schema")));
    }

    #[test]
    fn test_json_schema_not_json() {
        let e = JsonSchemaEvaluator;
        let c = EvalCase::new("c1", "q", "cat");
        let r = e.evaluate(&c, "not json at all");
        assert!(!r.passed);
        assert!((r.score - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_json_schema_array_type() {
        let e = JsonSchemaEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_json_schema = Some(serde_json::json!({"type": "array"}));
        let r = e.evaluate(&c, "[1,2,3]");
        assert!(r.passed);
    }

    #[test]
    fn test_json_schema_wrong_type() {
        let e = JsonSchemaEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_json_schema = Some(serde_json::json!({"type": "string"}));
        let r = e.evaluate(&c, "42");
        assert!(!r.passed);
    }

    // === SimilarityEvaluator tests ===

    #[test]
    fn test_similarity_identical() {
        let e = SimilarityEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.reference_answer = Some("The quick brown fox".to_string());
        let r = e.evaluate(&c, "The quick brown fox");
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_similarity_partial_overlap() {
        let e = SimilarityEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.reference_answer = Some("The quick brown fox jumps over the lazy dog".to_string());
        let r = e.evaluate(&c, "The quick brown cat sleeps under the lazy fence");
        assert!(r.score > 0.0);
        assert!(r.score < 1.0);
    }

    #[test]
    fn test_similarity_no_reference() {
        let e = SimilarityEvaluator;
        let c = EvalCase::new("c1", "q", "cat");
        let r = e.evaluate(&c, "anything");
        assert!(r.passed);
        assert!((r.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_similarity_no_overlap() {
        let e = SimilarityEvaluator;
        let mut c = EvalCase::new("c1", "q", "cat");
        c.reference_answer = Some("alpha beta gamma delta".to_string());
        let r = e.evaluate(&c, "one two three four five");
        assert!(!r.passed);
        assert!(r.score < 0.3);
    }

    // === HeuristicEvaluator tests ===

    #[test]
    fn test_heuristic_good_response() {
        let e = HeuristicEvaluator::default();
        let c = EvalCase::new("c1", "What is Rust?", "cat");
        let output = "Rust is a systems programming language focused on safety, \
                      concurrency, and performance. It achieves memory safety without \
                      garbage collection through its ownership system.";
        let r = e.evaluate(&c, output);
        assert!(r.score > 0.5);
        assert!(r.details.contains_key("relevance"));
        assert!(r.details.contains_key("clarity"));
    }

    #[test]
    fn test_heuristic_poor_response() {
        let e = HeuristicEvaluator::default();
        let c = EvalCase::new("c1", "Explain quantum computing", "cat");
        let r = e.evaluate(&c, "ok");
        assert!(r.score < 0.5);
        assert!(!r.passed);
    }

    // === CompositeEvaluator tests ===

    #[test]
    fn test_composite_aggregates() {
        let comp = CompositeEvaluator::new()
            .add("exact", ExactMatchEvaluator)
            .add("contains", ContainsEvaluator);

        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_output = Some("hello world".to_string());
        c.expected_contains = vec!["hello".to_string()];

        let r = comp.evaluate(&c, "hello world");
        assert!(r.passed);
        assert!(r.details.contains_key("exact.exact_match"));
        assert!(r.details.contains_key("contains.contains"));
    }

    #[test]
    fn test_composite_partial_failure() {
        let comp = CompositeEvaluator::new()
            .add("exact", ExactMatchEvaluator)
            .add("contains", ContainsEvaluator);

        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_output = Some("hello world".to_string());
        c.expected_contains = vec!["missing".to_string()];

        let r = comp.evaluate(&c, "hello world");
        assert!(!r.passed);
        // exact passes (1.0) but contains fails, so avg < 1.0
        assert!(r.score < 1.0);
    }

    #[test]
    fn test_composite_empty() {
        let comp = CompositeEvaluator::new();
        let c = EvalCase::new("c1", "q", "cat");
        let r = comp.evaluate(&c, "anything");
        assert!((r.score - 0.0).abs() < f32::EPSILON);
        assert!(r.passed); // no errors from zero evaluators
    }

    // === EvalFramework tests ===

    #[test]
    fn test_framework_register_and_get() {
        let mut fw = EvalFramework::new();
        assert_eq!(fw.suite_count(), 0);
        fw.register_suite(EvalSuite::new("s1", "Suite", "d"));
        assert_eq!(fw.suite_count(), 1);
        assert!(fw.get_suite("s1").is_some());
        assert!(fw.get_suite("s2").is_none());
    }

    #[test]
    fn test_framework_run_suite() {
        let mut fw = EvalFramework::new();
        let suite = EvalSuite::new("s1", "Test", "d")
            .with_case({
                let mut c = EvalCase::new("c1", "q1", "cat");
                c.expected_output = Some("answer1".to_string());
                c
            })
            .with_case({
                let mut c = EvalCase::new("c2", "q2", "cat");
                c.expected_output = Some("answer2".to_string());
                c
            });
        fw.register_suite(suite);

        let report = fw
            .run_suite("s1", &ExactMatchEvaluator, |case| {
                if case.id == "c1" {
                    "answer1".to_string()
                } else {
                    "wrong".to_string()
                }
            })
            .unwrap();

        assert_eq!(report.total_cases, 2);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        assert!((report.avg_score - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_framework_run_suite_not_found() {
        let fw = EvalFramework::new();
        let result = fw.run_suite("nope", &ExactMatchEvaluator, |_| String::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_framework_run_case() {
        let fw = EvalFramework::new();
        let mut c = EvalCase::new("c1", "q", "cat");
        c.expected_output = Some("yes".to_string());
        let r = fw.run_case(&c, &ExactMatchEvaluator, "yes");
        assert!(r.passed);
    }

    #[test]
    fn test_framework_compare_runs_improvement() {
        let run_a = EvalReport {
            suite_id: "s1".to_string(),
            total_cases: 2,
            passed: 1,
            failed: 1,
            avg_score: 0.5,
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            duration_ms: 100,
            results: vec![
                CaseResult {
                    case_id: "c1".to_string(),
                    passed: true,
                    score: 1.0,
                    details: HashMap::new(),
                    actual_output: String::new(),
                    duration_ms: 50,
                    errors: vec![],
                },
                CaseResult {
                    case_id: "c2".to_string(),
                    passed: false,
                    score: 0.0,
                    details: HashMap::new(),
                    actual_output: String::new(),
                    duration_ms: 50,
                    errors: vec!["fail".to_string()],
                },
            ],
        };

        let run_b = EvalReport {
            suite_id: "s1".to_string(),
            total_cases: 2,
            passed: 2,
            failed: 0,
            avg_score: 0.9,
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            duration_ms: 90,
            results: vec![
                CaseResult {
                    case_id: "c1".to_string(),
                    passed: true,
                    score: 1.0,
                    details: HashMap::new(),
                    actual_output: String::new(),
                    duration_ms: 40,
                    errors: vec![],
                },
                CaseResult {
                    case_id: "c2".to_string(),
                    passed: true,
                    score: 0.8,
                    details: HashMap::new(),
                    actual_output: String::new(),
                    duration_ms: 50,
                    errors: vec![],
                },
            ],
        };

        let cmp = EvalFramework::compare_runs(&run_a, &run_b);
        assert_eq!(cmp.improved, vec!["c2"]);
        assert!(cmp.regressed.is_empty());
        assert_eq!(cmp.unchanged, vec!["c1"]);
        assert!((cmp.avg_score_delta - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn test_framework_compare_runs_regression() {
        let run_a = EvalReport {
            suite_id: "a".to_string(),
            total_cases: 1,
            passed: 1,
            failed: 0,
            avg_score: 1.0,
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            duration_ms: 10,
            results: vec![CaseResult {
                case_id: "c1".to_string(),
                passed: true,
                score: 1.0,
                details: HashMap::new(),
                actual_output: String::new(),
                duration_ms: 10,
                errors: vec![],
            }],
        };

        let run_b = EvalReport {
            suite_id: "b".to_string(),
            total_cases: 1,
            passed: 0,
            failed: 1,
            avg_score: 0.3,
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            duration_ms: 10,
            results: vec![CaseResult {
                case_id: "c1".to_string(),
                passed: false,
                score: 0.3,
                details: HashMap::new(),
                actual_output: String::new(),
                duration_ms: 10,
                errors: vec!["fail".to_string()],
            }],
        };

        let cmp = EvalFramework::compare_runs(&run_a, &run_b);
        assert_eq!(cmp.regressed, vec!["c1"]);
        assert!(cmp.improved.is_empty());
        assert!((cmp.avg_score_delta - (-0.7)).abs() < f32::EPSILON);
    }

    // === EvalReport tests ===

    #[test]
    fn test_eval_report_pass_rate() {
        let report = EvalReport {
            suite_id: "s".to_string(),
            total_cases: 4,
            passed: 3,
            failed: 1,
            avg_score: 0.75,
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            duration_ms: 100,
            results: vec![],
        };
        assert!((report.pass_rate() - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_eval_report_pass_rate_empty() {
        let report = EvalReport {
            suite_id: "s".to_string(),
            total_cases: 0,
            passed: 0,
            failed: 0,
            avg_score: 0.0,
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            duration_ms: 0,
            results: vec![],
        };
        assert!((report.pass_rate() - 0.0).abs() < f32::EPSILON);
    }

    // === Pre-built suite tests ===

    #[test]
    fn test_ticket_routing_suite_structure() {
        let suite = ticket_routing_suite();
        assert_eq!(suite.id, "ticket_routing");
        assert_eq!(suite.cases.len(), 5);
        for case in &suite.cases {
            assert!(!case.id.is_empty());
            assert!(!case.input.is_empty());
            assert!(!case.expected_contains.is_empty());
            assert!(case.reference_answer.is_some());
            assert_eq!(case.category, "routing");
        }
    }

    #[test]
    fn test_support_quality_suite_structure() {
        let suite = support_quality_suite();
        assert_eq!(suite.id, "support_quality");
        assert_eq!(suite.cases.len(), 5);
        for case in &suite.cases {
            assert!(!case.id.is_empty());
            assert!(!case.input.is_empty());
            assert!(!case.expected_contains.is_empty());
            assert!(case.reference_answer.is_some());
            assert_eq!(case.category, "quality");
        }
    }

    #[test]
    fn test_lead_qualification_suite_structure() {
        let suite = lead_qualification_suite();
        assert_eq!(suite.id, "lead_qualification");
        assert_eq!(suite.cases.len(), 5);
        for case in &suite.cases {
            assert!(!case.id.is_empty());
            assert!(!case.input.is_empty());
            assert!(!case.expected_contains.is_empty());
            assert!(case.reference_answer.is_some());
            assert_eq!(case.category, "qualification");
        }
    }

    // === run_suite_full with metadata tests ===

    #[test]
    fn test_run_suite_full_by_category() {
        let mut fw = EvalFramework::new();
        let suite = EvalSuite::new("s1", "Full", "d")
            .with_case({
                let mut c = EvalCase::new("c1", "q1", "alpha");
                c.expected_output = Some("a".to_string());
                c.tags = vec!["t1".to_string()];
                c
            })
            .with_case({
                let mut c = EvalCase::new("c2", "q2", "beta");
                c.expected_output = Some("b".to_string());
                c.tags = vec!["t1".to_string(), "t2".to_string()];
                c
            });
        fw.register_suite(suite);

        let report = fw
            .run_suite_full("s1", &ExactMatchEvaluator, |case| {
                if case.id == "c1" {
                    "a".to_string()
                } else {
                    "b".to_string()
                }
            })
            .unwrap();

        assert_eq!(report.total_cases, 2);
        assert_eq!(report.passed, 2);
        assert!(report.by_category.contains_key("alpha"));
        assert!(report.by_category.contains_key("beta"));
        assert!(report.by_tag.contains_key("t1"));
        assert!(report.by_tag.contains_key("t2"));
    }

    // === token_similarity helper tests ===

    #[test]
    fn test_token_similarity_identical() {
        let s = token_similarity("hello world foo", "hello world foo");
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_token_similarity_no_overlap() {
        let s = token_similarity("alpha beta gamma", "delta epsilon zeta");
        assert!((s - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_token_similarity_both_empty() {
        let s = token_similarity("", "");
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    // === validate_schema helper tests ===

    #[test]
    fn test_validate_schema_boolean_type() {
        let schema = serde_json::json!({"type": "boolean"});
        assert!(validate_schema(&serde_json::json!(true), &schema));
        assert!(!validate_schema(&serde_json::json!(42), &schema));
    }

    #[test]
    fn test_validate_schema_number_type() {
        let schema = serde_json::json!({"type": "number"});
        assert!(validate_schema(&serde_json::json!(3.14), &schema));
        assert!(!validate_schema(&serde_json::json!("nope"), &schema));
    }

    #[test]
    fn test_validate_schema_unknown_type_passes() {
        let schema = serde_json::json!({"type": "unknown_type"});
        assert!(validate_schema(&serde_json::json!("anything"), &schema));
    }

    // === CaseResult / ComparisonReport serialization ===

    #[test]
    fn test_case_result_serialization() {
        let r = CaseResult {
            case_id: "c1".to_string(),
            passed: true,
            score: 0.95,
            details: HashMap::new(),
            actual_output: "out".to_string(),
            duration_ms: 42,
            errors: vec![],
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: CaseResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.case_id, "c1");
        assert!((back.score - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_comparison_report_serialization() {
        let cmp = ComparisonReport {
            run_a_id: "a".to_string(),
            run_b_id: "b".to_string(),
            improved: vec!["c1".to_string()],
            regressed: vec![],
            unchanged: vec!["c2".to_string()],
            avg_score_delta: 0.1,
        };
        let json = serde_json::to_string(&cmp).unwrap();
        let back: ComparisonReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.run_a_id, "a");
        assert_eq!(back.improved.len(), 1);
    }
}
