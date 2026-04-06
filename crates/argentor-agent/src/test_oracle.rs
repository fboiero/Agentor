//! Test execution analysis and TDD loop engine.
//!
//! This module parses test output from multiple frameworks (cargo test, pytest,
//! jest, go test), maps failures to code locations, classifies errors, suggests
//! fix strategies, and drives the red-green-refactor TDD cycle.
//!
//! # Main types
//!
//! - [`TestOracle`] — Parses test output, analyzes failures, drives TDD loops.
//! - [`TestRunSummary`] — Structured summary of a test run.
//! - [`FailureAnalysis`] — Root-cause analysis with fix strategy for a failure.
//! - [`TddCycle`] — State machine for the red-green-refactor cycle.

use regex::Regex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Supported test frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TestFramework {
    /// Rust `cargo test`
    CargoTest,
    /// Python `pytest`
    Pytest,
    /// JavaScript/TypeScript `jest`
    Jest,
    /// Go `go test`
    GoTest,
    /// Unknown or unsupported framework.
    Unknown,
}

/// Status of a single test case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    /// Test passed.
    Passed,
    /// Test failed.
    Failed,
    /// Test was skipped / ignored.
    Skipped,
    /// Test encountered an error (e.g., compilation, setup failure).
    Error,
}

/// A single parsed test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Fully qualified test name (e.g., `module::test_name`).
    pub name: String,
    /// Outcome of the test.
    pub status: TestStatus,
    /// Duration in milliseconds, if reported.
    pub duration_ms: Option<u64>,
    /// Error or failure message, if any.
    pub error_message: Option<String>,
    /// Source file where the test is defined, if known.
    pub file: Option<String>,
    /// Source line number, if known.
    pub line: Option<usize>,
    /// Captured stdout, if any.
    pub stdout: Option<String>,
}

/// Summary of a test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunSummary {
    /// Which framework produced this output.
    pub framework: TestFramework,
    /// Total number of tests.
    pub total: usize,
    /// Number of passed tests.
    pub passed: usize,
    /// Number of failed tests.
    pub failed: usize,
    /// Number of skipped/ignored tests.
    pub skipped: usize,
    /// Number of error (non-failure) tests.
    pub errors: usize,
    /// Total duration in milliseconds, if reported.
    pub duration_ms: Option<u64>,
    /// Individual test case results.
    pub cases: Vec<TestCase>,
}

/// Analysis of a test failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAnalysis {
    /// Name of the failing test.
    pub test_name: String,
    /// Classified error type.
    pub error_type: ErrorType,
    /// The raw error message.
    pub error_message: String,
    /// Source file, if determined.
    pub source_file: Option<String>,
    /// Source line, if determined.
    pub source_line: Option<usize>,
    /// Human-readable explanation of the probable cause.
    pub likely_cause: String,
    /// Suggested strategy for fixing the failure.
    pub fix_strategy: FixStrategy,
    /// Symbols (functions, types, variables) related to the failure.
    pub related_symbols: Vec<String>,
}

/// Classification of error types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorType {
    /// Compilation error (Rust `error[E...]`, etc.).
    CompilationError,
    /// Type mismatch between expected and actual types.
    TypeMismatch,
    /// Missing import, module, or dependency.
    MissingImport,
    /// Assertion failure (`assert!`, `assert_eq!`, etc.).
    AssertionFailure,
    /// Panic from `.unwrap()` or `.expect()` on `None`/`Err`.
    PanicUnwrap,
    /// Test exceeded its time limit.
    Timeout,
    /// Generic runtime error.
    RuntimeError,
    /// Logical error in the implementation.
    LogicError,
    /// OS-level permission denied.
    PermissionDenied,
    /// Resource not found (file, endpoint, etc.).
    NotFound,
    /// Could not determine the error type.
    Unknown,
}

/// Suggested strategy for fixing a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixStrategy {
    /// Add a missing import or dependency.
    AddImport {
        /// The module or crate to import.
        module: String,
    },
    /// Fix a type mismatch by changing the type.
    FixType {
        /// The expected type.
        expected: String,
        /// The actual type found.
        actual: String,
    },
    /// Update an assertion's expected value.
    UpdateAssertion,
    /// Handle an unwrap/expect that panicked.
    AddErrorHandling,
    /// Fix logic in the implementation.
    FixLogic {
        /// Hint about what the logic error might be.
        hint: String,
    },
    /// Add a missing function, method, or field.
    AddMissing {
        /// Description of what is missing.
        what: String,
    },
    /// Increase timeout or optimize for performance.
    FixPerformance,
    /// General investigation needed.
    Investigate {
        /// Suggestion for what to look at.
        suggestion: String,
    },
}

/// State of the TDD cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TddPhase {
    /// Write a failing test (Red).
    Red,
    /// Make the test pass with minimal code (Green).
    Green,
    /// Improve code quality without changing behavior (Refactor).
    Refactor,
    /// All tests pass, cycle complete.
    Complete,
}

/// TDD cycle state machine.
///
/// Tracks the current phase, iteration count, and history of actions taken
/// during the red-green-refactor loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TddCycle {
    /// Current phase of the TDD cycle.
    pub phase: TddPhase,
    /// Name of the test being targeted, if any.
    pub target_test: Option<String>,
    /// Number of iterations completed so far.
    pub iterations: usize,
    /// Maximum number of iterations before aborting.
    pub max_iterations: usize,
    /// History of TDD iterations.
    pub history: Vec<TddIteration>,
}

/// A single iteration within a TDD cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TddIteration {
    /// The phase during this iteration.
    pub phase: TddPhase,
    /// Action taken (e.g., "wrote failing test", "implemented function").
    pub action: String,
    /// Test result after the action, if a test was run.
    pub test_result: Option<TestStatus>,
    /// Free-form notes about what happened.
    pub notes: String,
}

// ---------------------------------------------------------------------------
// TestOracle
// ---------------------------------------------------------------------------

/// The test oracle: parses test output, analyzes failures, drives TDD loops.
///
/// `TestOracle` is stateless; all methods are associated functions that operate
/// on the provided data. Create an instance with [`TestOracle::new`] when you
/// need an owned value, but all logic lives in the associated functions.
pub struct TestOracle;

impl TestOracle {
    /// Create a new `TestOracle` instance.
    pub fn new() -> Self {
        Self
    }

    /// Detect which test framework produced the given output.
    ///
    /// Inspects characteristic markers in the output text:
    /// - `test result:` or `running \d+ test` for cargo test
    /// - `PASSED` / `FAILED` with `pytest` style or `passed` / `failed` summary for pytest
    /// - `Tests:` summary line or `PASS`/`FAIL` with file paths for jest
    /// - `--- PASS:` / `--- FAIL:` for go test
    pub fn detect_framework(output: &str) -> TestFramework {
        // Cargo test: "test result:" or "running N test(s)"
        if output.contains("test result:")
            || Regex::new(r"running \d+ tests?")
                .ok()
                .and_then(|re| re.find(output))
                .is_some()
        {
            return TestFramework::CargoTest;
        }

        // Go test: "--- PASS:" or "--- FAIL:" (check before jest to avoid overlap)
        if output.contains("--- PASS:") || output.contains("--- FAIL:") {
            return TestFramework::GoTest;
        }

        // Pytest: line-level "PASSED" / "FAILED" with `::` separators, or summary
        if (output.contains("PASSED") || output.contains("FAILED"))
            && (output.contains("::") || output.contains(" passed"))
        {
            // Distinguish from jest: jest uses "Tests:" summary, pytest uses "passed"
            if output.contains("Tests:") && !output.contains(" passed in ") {
                return TestFramework::Jest;
            }
            return TestFramework::Pytest;
        }

        // Jest: "Tests:" summary or checkmark/cross symbols
        if output.contains("Tests:") || output.contains("\u{2713}") || output.contains("\u{2715}") {
            return TestFramework::Jest;
        }

        TestFramework::Unknown
    }

    /// Parse test output into a structured [`TestRunSummary`].
    ///
    /// Automatically detects the framework and delegates to the appropriate parser.
    pub fn parse_output(output: &str) -> TestRunSummary {
        let framework = Self::detect_framework(output);
        match framework {
            TestFramework::CargoTest => Self::parse_cargo_test(output),
            TestFramework::Pytest => Self::parse_pytest(output),
            TestFramework::Jest => Self::parse_jest(output),
            TestFramework::GoTest => Self::parse_go_test(output),
            TestFramework::Unknown => TestRunSummary {
                framework: TestFramework::Unknown,
                total: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
                errors: 0,
                duration_ms: None,
                cases: Vec::new(),
            },
        }
    }

    /// Parse `cargo test` output into a [`TestRunSummary`].
    fn parse_cargo_test(output: &str) -> TestRunSummary {
        let mut cases = Vec::new();
        let mut failure_outputs: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Collect failure stdout blocks:
        // ---- module::test_name stdout ----
        // <content until next ---- or "failures:" section>
        let stdout_re = Regex::new(r"---- (.+?) stdout ----").ok();
        if let Some(re) = &stdout_re {
            let lines: Vec<&str> = output.lines().collect();
            let mut i = 0;
            while i < lines.len() {
                if let Some(caps) = re.captures(lines[i]) {
                    let test_name = caps[1].to_string();
                    let mut content = Vec::new();
                    i += 1;
                    while i < lines.len()
                        && !lines[i].starts_with("----")
                        && !lines[i].starts_with("failures:")
                    {
                        content.push(lines[i]);
                        i += 1;
                    }
                    failure_outputs.insert(test_name, content.join("\n"));
                } else {
                    i += 1;
                }
            }
        }

        // Parse individual test lines: "test <name> ... ok/FAILED/ignored"
        let test_line_re = Regex::new(r"^test (.+?) \.\.\. (.+)$").ok();
        if let Some(re) = &test_line_re {
            for line in output.lines() {
                if let Some(caps) = re.captures(line) {
                    let name = caps[1].to_string();
                    let status_str = caps[2].trim();
                    let status = match status_str {
                        "ok" => TestStatus::Passed,
                        "FAILED" => TestStatus::Failed,
                        "ignored" => TestStatus::Skipped,
                        _ => TestStatus::Error,
                    };

                    // Extract error info from failure stdout
                    let (error_message, file, line_num) =
                        Self::extract_cargo_failure_info(failure_outputs.get(&name));

                    let stdout = failure_outputs.get(&name).cloned();

                    cases.push(TestCase {
                        name,
                        status,
                        duration_ms: None,
                        error_message,
                        file,
                        line: line_num,
                        stdout,
                    });
                }
            }
        }

        // Extract duration from "finished in X.XXs"
        let mut total_duration_ms: Option<u64> = None;
        let duration_re = Regex::new(r"finished in (\d+(?:\.\d+)?)s").ok();
        if let Some(re) = &duration_re {
            if let Some(caps) = re.captures(output) {
                if let Ok(secs) = caps[1].parse::<f64>() {
                    total_duration_ms = Some((secs * 1000.0) as u64);
                }
            }
        }

        let passed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Passed)
            .count();
        let failed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Failed)
            .count();
        let skipped = cases
            .iter()
            .filter(|c| c.status == TestStatus::Skipped)
            .count();
        let errors = cases
            .iter()
            .filter(|c| c.status == TestStatus::Error)
            .count();
        let total = cases.len();

        TestRunSummary {
            framework: TestFramework::CargoTest,
            total,
            passed,
            failed,
            skipped,
            errors,
            duration_ms: total_duration_ms,
            cases,
        }
    }

    /// Extract error message, file path, and line number from cargo test failure stdout.
    fn extract_cargo_failure_info(
        stdout: Option<&String>,
    ) -> (Option<String>, Option<String>, Option<usize>) {
        let Some(text) = stdout else {
            return (None, None, None);
        };

        let mut error_message = None;
        let mut file = None;
        let mut line = None;

        // Pattern: "thread '...' panicked at '<message>', <file>:<line>:<col>"
        let panic_re = Regex::new(r"panicked at '([^']*)'(?:,\s*(.+?):(\d+)(?::\d+)?)?").ok();
        // Alternative pattern for Rust 2021+ format:
        // "thread '...' panicked at <file>:<line>:<col>:\n<message>"
        let panic_v2_re = Regex::new(r"panicked at (.+?):(\d+)(?::\d+)?:?\s*\n(.+)").ok();

        if let Some(re) = &panic_re {
            if let Some(caps) = re.captures(text) {
                error_message = Some(caps[1].to_string());
                if let Some(f) = caps.get(2) {
                    file = Some(f.as_str().to_string());
                }
                if let Some(l) = caps.get(3) {
                    line = l.as_str().parse().ok();
                }
                return (error_message, file, line);
            }
        }

        if let Some(re) = &panic_v2_re {
            if let Some(caps) = re.captures(text) {
                file = Some(caps[1].to_string());
                line = caps[2].parse().ok();
                error_message = Some(caps[3].trim().to_string());
                return (error_message, file, line);
            }
        }

        // Fallback: use the whole stdout as the error message
        if !text.trim().is_empty() {
            error_message = Some(text.trim().to_string());
        }

        (error_message, file, line)
    }

    /// Parse `pytest` output into a [`TestRunSummary`].
    fn parse_pytest(output: &str) -> TestRunSummary {
        let mut cases = Vec::new();

        // Match lines like:
        //   PASSED test_file.py::test_name
        //   FAILED test_file.py::test_other - AssertionError: ...
        let result_re = Regex::new(r"(PASSED|FAILED|SKIPPED|ERROR)\s+(\S+?)(?:\s+-\s+(.+))?$").ok();

        if let Some(re) = &result_re {
            for line in output.lines() {
                let trimmed = line.trim();
                if let Some(caps) = re.captures(trimmed) {
                    let status = match &caps[1] {
                        "PASSED" => TestStatus::Passed,
                        "FAILED" => TestStatus::Failed,
                        "SKIPPED" => TestStatus::Skipped,
                        "ERROR" => TestStatus::Error,
                        _ => TestStatus::Error,
                    };
                    let full_name = caps[2].to_string();
                    let error_message = caps.get(3).map(|m| m.as_str().to_string());

                    // Split file::test_name
                    let (file, name) = if let Some(idx) = full_name.find("::") {
                        (
                            Some(full_name[..idx].to_string()),
                            full_name[idx + 2..].to_string(),
                        )
                    } else {
                        (None, full_name)
                    };

                    cases.push(TestCase {
                        name,
                        status,
                        duration_ms: None,
                        error_message,
                        file,
                        line: None,
                        stdout: None,
                    });
                }
            }
        }

        // Parse summary: "= 1 failed, 1 passed in 0.12s ="
        let mut total_duration_ms: Option<u64> = None;
        let summary_re = Regex::new(r"(\d+) failed.*?(\d+) passed.*?in (\d+(?:\.\d+)?)s").ok();
        let summary_pass_only = Regex::new(r"(\d+) passed.*?in (\d+(?:\.\d+)?)s").ok();

        if let Some(re) = &summary_re {
            if let Some(caps) = re.captures(output) {
                if let Ok(secs) = caps[3].parse::<f64>() {
                    total_duration_ms = Some((secs * 1000.0) as u64);
                }
            }
        }
        if total_duration_ms.is_none() {
            if let Some(re) = &summary_pass_only {
                if let Some(caps) = re.captures(output) {
                    if let Ok(secs) = caps[2].parse::<f64>() {
                        total_duration_ms = Some((secs * 1000.0) as u64);
                    }
                }
            }
        }

        let passed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Passed)
            .count();
        let failed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Failed)
            .count();
        let skipped = cases
            .iter()
            .filter(|c| c.status == TestStatus::Skipped)
            .count();
        let errors = cases
            .iter()
            .filter(|c| c.status == TestStatus::Error)
            .count();
        let total = cases.len();

        TestRunSummary {
            framework: TestFramework::Pytest,
            total,
            passed,
            failed,
            skipped,
            errors,
            duration_ms: total_duration_ms,
            cases,
        }
    }

    /// Parse `jest` output into a [`TestRunSummary`].
    fn parse_jest(output: &str) -> TestRunSummary {
        let mut cases = Vec::new();

        // Jest uses unicode checkmarks and crosses or PASS/FAIL prefixes.
        // Lines like:
        //   ✓ test name (5 ms)
        //   ✕ test name (10 ms)
        //   or:
        //   PASS src/test.spec.ts
        //   FAIL src/other.spec.ts

        // Track current file from PASS/FAIL header lines
        let mut current_file: Option<String> = None;
        let header_re = Regex::new(r"^(PASS|FAIL)\s+(.+)$").ok();
        // Match test result lines with optional duration
        let check_re = Regex::new(r"^\s*[✓✕]\s+(.+?)(?:\s+\((\d+)\s*ms\))?$").ok();
        // Also handle ASCII fallback: "√" and "×" or simple markers
        let check_ascii_re = Regex::new(r"^\s*(?:√|×)\s+(.+?)(?:\s+\((\d+)\s*ms\))?$").ok();

        // Jest output for failure details:
        //   Expected: 42
        //   Received: 0
        let expected_re = Regex::new(r"Expected:\s+(.+)").ok();
        let received_re = Regex::new(r"Received:\s+(.+)").ok();

        // Collect failure details per test (simplified: store last expected/received)
        let mut last_expected: Option<String> = None;
        let mut last_received: Option<String> = None;

        for line in output.lines() {
            let trimmed = line.trim();

            // Header line
            if let Some(re) = &header_re {
                if let Some(caps) = re.captures(trimmed) {
                    current_file = Some(caps[2].to_string());
                    continue;
                }
            }

            // Checkmark line (pass)
            if let Some(re) = &check_re {
                if trimmed.starts_with('\u{2713}') || trimmed.contains('\u{2713}') {
                    if let Some(caps) = re.captures(trimmed) {
                        let name = caps[1].trim().to_string();
                        let duration_ms = caps.get(2).and_then(|m| m.as_str().parse().ok());
                        cases.push(TestCase {
                            name,
                            status: TestStatus::Passed,
                            duration_ms,
                            error_message: None,
                            file: current_file.clone(),
                            line: None,
                            stdout: None,
                        });
                        continue;
                    }
                }
            }

            // Cross line (fail)
            if let Some(re) = &check_re {
                if trimmed.starts_with('\u{2715}') || trimmed.contains('\u{2715}') {
                    if let Some(caps) = re.captures(trimmed) {
                        let name = caps[1].trim().to_string();
                        let duration_ms = caps.get(2).and_then(|m| m.as_str().parse().ok());
                        cases.push(TestCase {
                            name,
                            status: TestStatus::Failed,
                            duration_ms,
                            error_message: None,
                            file: current_file.clone(),
                            line: None,
                            stdout: None,
                        });
                        continue;
                    }
                }
            }

            // ASCII fallback
            if let Some(re) = &check_ascii_re {
                if let Some(caps) = re.captures(trimmed) {
                    let name = caps[1].trim().to_string();
                    let duration_ms = caps.get(2).and_then(|m| m.as_str().parse().ok());
                    let is_pass = trimmed.contains('√');
                    cases.push(TestCase {
                        name,
                        status: if is_pass {
                            TestStatus::Passed
                        } else {
                            TestStatus::Failed
                        },
                        duration_ms,
                        error_message: None,
                        file: current_file.clone(),
                        line: None,
                        stdout: None,
                    });
                    continue;
                }
            }

            // Expected/Received for failure details
            if let Some(re) = &expected_re {
                if let Some(caps) = re.captures(trimmed) {
                    last_expected = Some(caps[1].to_string());
                }
            }
            if let Some(re) = &received_re {
                if let Some(caps) = re.captures(trimmed) {
                    last_received = Some(caps[1].to_string());
                }
            }
        }

        // Attach expected/received to the last failed test
        if last_expected.is_some() || last_received.is_some() {
            if let Some(failed_case) = cases.iter_mut().rfind(|c| c.status == TestStatus::Failed) {
                let msg = format!(
                    "Expected: {}, Received: {}",
                    last_expected.as_deref().unwrap_or("?"),
                    last_received.as_deref().unwrap_or("?")
                );
                failed_case.error_message = Some(msg);
            }
        }

        // Parse timing: "Time: 1.234 s"
        let time_re = Regex::new(r"Time:\s+(\d+(?:\.\d+)?)\s*s").ok();

        let mut total_duration_ms: Option<u64> = None;

        if let Some(re) = &time_re {
            if let Some(caps) = re.captures(output) {
                if let Ok(secs) = caps[1].parse::<f64>() {
                    total_duration_ms = Some((secs * 1000.0) as u64);
                }
            }
        }

        let passed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Passed)
            .count();
        let failed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Failed)
            .count();
        let skipped = cases
            .iter()
            .filter(|c| c.status == TestStatus::Skipped)
            .count();
        let errors = cases
            .iter()
            .filter(|c| c.status == TestStatus::Error)
            .count();
        let total = cases.len();

        TestRunSummary {
            framework: TestFramework::Jest,
            total,
            passed,
            failed,
            skipped,
            errors,
            duration_ms: total_duration_ms,
            cases,
        }
    }

    /// Parse `go test` output into a [`TestRunSummary`].
    fn parse_go_test(output: &str) -> TestRunSummary {
        let mut cases = Vec::new();

        // Go test lines:
        //   --- PASS: TestName (0.00s)
        //   --- FAIL: TestOther (0.01s)
        //   --- SKIP: TestSkip (0.00s)
        let result_re = Regex::new(r"--- (PASS|FAIL|SKIP): (\S+) \((\d+(?:\.\d+)?)s\)").ok();

        // Failure details:
        //     file_test.go:15: expected 42, got 0
        let detail_re = Regex::new(r"^\s+(\S+\.go):(\d+):\s+(.+)$").ok();

        // Collect failure details per test
        let mut failure_details: std::collections::HashMap<String, (String, String, usize)> =
            std::collections::HashMap::new();

        // First pass: collect failure details (they appear after the test name)
        let lines: Vec<&str> = output.lines().collect();
        let mut current_test: Option<String> = None;
        for line in &lines {
            if let Some(re) = &result_re {
                if let Some(caps) = re.captures(line) {
                    current_test = if &caps[1] == "FAIL" {
                        Some(caps[2].to_string())
                    } else {
                        None
                    };
                    continue;
                }
            }
            if let Some(ref test_name) = current_test {
                if let Some(re) = &detail_re {
                    if let Some(caps) = re.captures(line) {
                        let file = caps[1].to_string();
                        let line_num: usize = caps[2].parse().unwrap_or(0);
                        let msg = caps[3].to_string();
                        failure_details.insert(test_name.clone(), (file, msg, line_num));
                    }
                }
            }
        }

        // Second pass: parse test results
        // Note: detail lines may precede the --- FAIL line, so also scan before
        // We re-scan to build a map of detail lines per test name
        let mut pre_details: std::collections::HashMap<String, (String, String, usize)> =
            std::collections::HashMap::new();
        {
            let mut i = 0;
            while i < lines.len() {
                if let Some(re) = &detail_re {
                    if let Some(caps) = re.captures(lines[i]) {
                        let file = caps[1].to_string();
                        let line_num: usize = caps[2].parse().unwrap_or(0);
                        let msg = caps[3].to_string();
                        // Look ahead for the test name
                        for look_line in &lines[(i + 1)..] {
                            if let Some(rre) = &result_re {
                                if let Some(rcaps) = rre.captures(look_line) {
                                    if &rcaps[1] == "FAIL" {
                                        pre_details.insert(
                                            rcaps[2].to_string(),
                                            (file.clone(), msg.clone(), line_num),
                                        );
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                i += 1;
            }
        }

        if let Some(re) = &result_re {
            for line in output.lines() {
                if let Some(caps) = re.captures(line) {
                    let status = match &caps[1] {
                        "PASS" => TestStatus::Passed,
                        "FAIL" => TestStatus::Failed,
                        "SKIP" => TestStatus::Skipped,
                        _ => TestStatus::Error,
                    };
                    let name = caps[2].to_string();
                    let duration_ms = caps[3].parse::<f64>().ok().map(|s| (s * 1000.0) as u64);

                    let details = failure_details
                        .get(&name)
                        .or_else(|| pre_details.get(&name));

                    let (error_message, file, line_num) = match details {
                        Some((f, msg, ln)) => (Some(msg.clone()), Some(f.clone()), Some(*ln)),
                        None => (None, None, None),
                    };

                    cases.push(TestCase {
                        name,
                        status,
                        duration_ms,
                        error_message,
                        file,
                        line: line_num,
                        stdout: None,
                    });
                }
            }
        }

        // Parse overall duration: "ok  package/name  0.015s"
        let mut total_duration_ms: Option<u64> = None;
        let ok_re = Regex::new(r"ok\s+\S+\s+(\d+(?:\.\d+)?)s").ok();
        let fail_re = Regex::new(r"FAIL\s+\S+\s+(\d+(?:\.\d+)?)s").ok();
        if let Some(re) = &ok_re {
            if let Some(caps) = re.captures(output) {
                if let Ok(secs) = caps[1].parse::<f64>() {
                    total_duration_ms = Some((secs * 1000.0) as u64);
                }
            }
        }
        if total_duration_ms.is_none() {
            if let Some(re) = &fail_re {
                if let Some(caps) = re.captures(output) {
                    if let Ok(secs) = caps[1].parse::<f64>() {
                        total_duration_ms = Some((secs * 1000.0) as u64);
                    }
                }
            }
        }

        let passed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Passed)
            .count();
        let failed = cases
            .iter()
            .filter(|c| c.status == TestStatus::Failed)
            .count();
        let skipped = cases
            .iter()
            .filter(|c| c.status == TestStatus::Skipped)
            .count();
        let errors = cases
            .iter()
            .filter(|c| c.status == TestStatus::Error)
            .count();
        let total = cases.len();

        TestRunSummary {
            framework: TestFramework::GoTest,
            total,
            passed,
            failed,
            skipped,
            errors,
            duration_ms: total_duration_ms,
            cases,
        }
    }

    /// Analyze a test failure and produce a [`FailureAnalysis`] with fix strategy.
    ///
    /// Combines error classification, source location extraction, cause inference,
    /// and fix strategy suggestion into a single structured result.
    pub fn analyze_failure(test_case: &TestCase) -> FailureAnalysis {
        let error_message = test_case
            .error_message
            .clone()
            .unwrap_or_else(|| "unknown error".to_string());

        let error_type = Self::classify_error(&error_message);
        let fix_strategy = Self::suggest_fix(&error_type, &error_message);

        let likely_cause = Self::infer_cause(&error_type, &error_message);
        let related_symbols = Self::extract_symbols(&error_message);

        FailureAnalysis {
            test_name: test_case.name.clone(),
            error_type,
            error_message,
            source_file: test_case.file.clone(),
            source_line: test_case.line,
            likely_cause,
            fix_strategy,
            related_symbols,
        }
    }

    /// Classify an error message into an [`ErrorType`].
    ///
    /// Uses keyword matching against known patterns for each error category.
    pub fn classify_error(message: &str) -> ErrorType {
        let lower = message.to_lowercase();

        // Compilation errors (Rust-specific)
        if lower.contains("error[e") {
            return ErrorType::CompilationError;
        }

        // Permission denied
        if lower.contains("permission denied") {
            return ErrorType::PermissionDenied;
        }

        // Timeout
        if lower.contains("timeout") || lower.contains("timed out") {
            return ErrorType::Timeout;
        }

        // Panic from unwrap/expect
        if lower.contains("called `option::unwrap()` on a `none` value")
            || lower.contains("called `result::unwrap()` on an `err` value")
            || lower.contains("called `.unwrap()`")
            || lower.contains("called `.expect()`")
            || lower.contains("panicked at 'called `option::unwrap()`")
            || lower.contains("panicked at 'called `result::unwrap()`")
        {
            return ErrorType::PanicUnwrap;
        }

        // Type mismatch
        if lower.contains("type mismatch")
            || lower.contains("mismatched types")
            || (lower.contains("expected") && lower.contains("found"))
        {
            return ErrorType::TypeMismatch;
        }

        // Assertion failure
        if lower.contains("assertion")
            || lower.contains("assert_eq")
            || lower.contains("assert_ne")
            || lower.contains("assert!")
            || lower.contains("assertionerror")
        {
            return ErrorType::AssertionFailure;
        }

        // Missing import / not found
        if lower.contains("cannot find")
            || lower.contains("not found")
            || lower.contains("unresolved")
            || lower.contains("no such file")
            || lower.contains("module not found")
            || lower.contains("import error")
        {
            return ErrorType::MissingImport;
        }

        // Not found (more specific resource-level)
        if lower.contains("404") || lower.contains("resource not found") {
            return ErrorType::NotFound;
        }

        // Runtime error
        if lower.contains("runtime error")
            || lower.contains("segmentation fault")
            || lower.contains("stack overflow")
        {
            return ErrorType::RuntimeError;
        }

        // Logic error (catch-all for test failures with expected/got patterns)
        if lower.contains("expected") && lower.contains("got") {
            return ErrorType::LogicError;
        }

        ErrorType::Unknown
    }

    /// Suggest a fix strategy based on the error type and message.
    ///
    /// Returns the most appropriate [`FixStrategy`] variant for the given error.
    pub fn suggest_fix(error_type: &ErrorType, message: &str) -> FixStrategy {
        match error_type {
            ErrorType::MissingImport => {
                // Try to extract the module/symbol name from the message
                let module = Self::extract_missing_module(message);
                FixStrategy::AddImport { module }
            }
            ErrorType::TypeMismatch => {
                let (expected, actual) = Self::extract_type_mismatch(message);
                FixStrategy::FixType { expected, actual }
            }
            ErrorType::AssertionFailure => FixStrategy::UpdateAssertion,
            ErrorType::PanicUnwrap => FixStrategy::AddErrorHandling,
            ErrorType::Timeout => FixStrategy::FixPerformance,
            ErrorType::CompilationError => {
                // Try to determine if it's a missing item
                let lower = message.to_lowercase();
                if lower.contains("cannot find") || lower.contains("not found") {
                    let what = Self::extract_missing_item(message);
                    FixStrategy::AddMissing { what }
                } else {
                    FixStrategy::Investigate {
                        suggestion: "Review the compilation error and fix the source code"
                            .to_string(),
                    }
                }
            }
            ErrorType::LogicError => {
                let hint = Self::extract_logic_hint(message);
                FixStrategy::FixLogic { hint }
            }
            ErrorType::PermissionDenied => FixStrategy::Investigate {
                suggestion: "Check file/directory permissions or run with elevated privileges"
                    .to_string(),
            },
            ErrorType::NotFound => {
                let what = Self::extract_missing_item(message);
                FixStrategy::AddMissing { what }
            }
            ErrorType::RuntimeError => FixStrategy::Investigate {
                suggestion: "Debug the runtime error; check for null pointers, buffer overflows, or resource exhaustion".to_string(),
            },
            ErrorType::Unknown => FixStrategy::Investigate {
                suggestion: "Investigate the error message and test output for clues".to_string(),
            },
        }
    }

    /// Create a new TDD cycle targeting a specific test.
    ///
    /// Starts in the [`TddPhase::Red`] phase. The cycle will abort after
    /// `max_iterations` to prevent infinite loops.
    pub fn start_tdd(target_test: &str, max_iterations: usize) -> TddCycle {
        TddCycle {
            phase: TddPhase::Red,
            target_test: Some(target_test.to_string()),
            iterations: 0,
            max_iterations,
            history: Vec::new(),
        }
    }

    /// Advance the TDD cycle based on test results.
    ///
    /// State transitions:
    /// - **Red**: If the target test fails, move to Green (write implementation).
    ///   If it passes, skip to Refactor (test was already passing).
    /// - **Green**: If all tests pass, move to Refactor. Otherwise stay in Green.
    /// - **Refactor**: If all tests still pass, move to Complete. If something
    ///   broke, go back to Green.
    ///
    /// Returns a human-readable string describing the next action to take.
    pub fn advance_tdd(cycle: &mut TddCycle, summary: &TestRunSummary) -> String {
        cycle.iterations += 1;

        if cycle.iterations > cycle.max_iterations {
            let action = format!(
                "TDD cycle aborted: exceeded maximum iterations ({})",
                cycle.max_iterations
            );
            cycle.history.push(TddIteration {
                phase: cycle.phase.clone(),
                action: action.clone(),
                test_result: None,
                notes: "Max iterations exceeded".to_string(),
            });
            cycle.phase = TddPhase::Complete;
            return action;
        }

        let target = cycle.target_test.as_deref().unwrap_or("");
        let target_case = summary.cases.iter().find(|c| c.name.contains(target));
        let target_status = target_case.map(|c| &c.status);

        match &cycle.phase {
            TddPhase::Red => {
                let target_failed = target_status == Some(&TestStatus::Failed);
                if target_failed {
                    // Good: test is failing. Move to Green phase.
                    let action = format!(
                        "Test '{target}' is failing as expected. Write minimal implementation to make it pass."
                    );
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Red,
                        action: action.clone(),
                        test_result: Some(TestStatus::Failed),
                        notes: "Target test is red, moving to green phase".to_string(),
                    });
                    cycle.phase = TddPhase::Green;
                    action
                } else if target_status == Some(&TestStatus::Passed) {
                    // Test already passes — skip to refactor
                    let action =
                        format!("Test '{target}' already passes. Skipping to refactor phase.");
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Red,
                        action: action.clone(),
                        test_result: Some(TestStatus::Passed),
                        notes: "Target test was already passing".to_string(),
                    });
                    cycle.phase = TddPhase::Refactor;
                    action
                } else {
                    // Test not found or errored — need to write the test first
                    let action = format!(
                        "Write a failing test for '{target}'. The test must fail before writing implementation."
                    );
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Red,
                        action: action.clone(),
                        test_result: target_status.cloned(),
                        notes: "Target test not found or errored".to_string(),
                    });
                    action
                }
            }
            TddPhase::Green => {
                let all_pass = Self::all_passing(summary);
                if all_pass {
                    let action =
                        "All tests pass. Review the code for improvements (refactor phase)."
                            .to_string();
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Green,
                        action: action.clone(),
                        test_result: Some(TestStatus::Passed),
                        notes: "All tests green, moving to refactor".to_string(),
                    });
                    cycle.phase = TddPhase::Refactor;
                    action
                } else {
                    let failures = Self::failures(summary);
                    let names: Vec<&str> = failures.iter().map(|c| c.name.as_str()).collect();
                    let action = format!(
                        "Tests still failing: [{}]. Fix the implementation to make them pass.",
                        names.join(", ")
                    );
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Green,
                        action: action.clone(),
                        test_result: Some(TestStatus::Failed),
                        notes: format!("{} tests still failing", failures.len()),
                    });
                    action
                }
            }
            TddPhase::Refactor => {
                if Self::all_passing(summary) {
                    let action =
                        "Refactoring complete. All tests still pass. TDD cycle done.".to_string();
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Refactor,
                        action: action.clone(),
                        test_result: Some(TestStatus::Passed),
                        notes: "Refactor successful, cycle complete".to_string(),
                    });
                    cycle.phase = TddPhase::Complete;
                    action
                } else {
                    let action =
                        "Refactoring broke tests! Go back to green phase and fix them.".to_string();
                    cycle.history.push(TddIteration {
                        phase: TddPhase::Refactor,
                        action: action.clone(),
                        test_result: Some(TestStatus::Failed),
                        notes: "Refactor broke tests, returning to green".to_string(),
                    });
                    cycle.phase = TddPhase::Green;
                    action
                }
            }
            TddPhase::Complete => {
                "TDD cycle is already complete. Start a new cycle if needed.".to_string()
            }
        }
    }

    /// Generate a prompt for the LLM to fix a failing test.
    ///
    /// Produces a structured prompt including the error details, source location,
    /// cause analysis, and suggested fix strategy.
    pub fn fix_prompt(analysis: &FailureAnalysis) -> String {
        let mut prompt = String::new();

        prompt.push_str("## Fix Failing Test\n\n");
        prompt.push_str(&format!("**Test:** `{}`\n", analysis.test_name));
        prompt.push_str(&format!("**Error type:** {:?}\n", analysis.error_type));
        prompt.push_str(&format!("**Error message:** {}\n", analysis.error_message));

        if let Some(ref file) = analysis.source_file {
            prompt.push_str(&format!("**File:** `{file}`"));
            if let Some(line) = analysis.source_line {
                prompt.push_str(&format!(" (line {line})"));
            }
            prompt.push('\n');
        }

        prompt.push_str(&format!("**Likely cause:** {}\n", analysis.likely_cause));
        prompt.push_str(&format!(
            "**Fix strategy:** {}\n",
            Self::format_fix_strategy(&analysis.fix_strategy)
        ));

        if !analysis.related_symbols.is_empty() {
            prompt.push_str(&format!(
                "**Related symbols:** {}\n",
                analysis.related_symbols.join(", ")
            ));
        }

        prompt.push_str("\n### Instructions\n\n");
        prompt.push_str("1. Read the source file at the indicated location.\n");
        prompt.push_str("2. Apply the suggested fix strategy.\n");
        prompt.push_str("3. Run the test again to verify it passes.\n");
        prompt.push_str("4. Ensure no other tests are broken by the change.\n");

        prompt
    }

    /// Generate a prompt for the LLM to write a test for a feature.
    ///
    /// Produces a language-specific prompt that guides the LLM to write
    /// a test following TDD principles.
    pub fn test_prompt(feature_description: &str, language: &str) -> String {
        let framework_hint = match language.to_lowercase().as_str() {
            "rust" => {
                "Use `#[test]` and `assert_eq!`/`assert!` macros. Follow `cargo test` conventions."
            }
            "python" => "Use `pytest` conventions. Name test functions with `test_` prefix.",
            "javascript" | "typescript" | "js" | "ts" => {
                "Use `jest` conventions with `describe`/`it`/`expect`."
            }
            "go" => "Use Go testing conventions with `func TestXxx(t *testing.T)`.",
            _ => "Use the standard testing framework for the language.",
        };

        format!(
            "## Write a Failing Test (TDD Red Phase)\n\n\
             **Feature:** {feature_description}\n\
             **Language:** {language}\n\
             **Framework:** {framework_hint}\n\n\
             ### Instructions\n\n\
             1. Write a test that describes the expected behavior of the feature.\n\
             2. The test MUST fail when run (since the feature is not implemented yet).\n\
             3. Keep the test focused on a single behavior.\n\
             4. Use descriptive test names that explain what is being tested.\n\
             5. Include edge cases if the feature has boundary conditions.\n"
        )
    }

    /// Check if all tests in a summary passed (none failed or errored).
    pub fn all_passing(summary: &TestRunSummary) -> bool {
        summary.failed == 0 && summary.errors == 0
    }

    /// Get only the failed test cases from a summary.
    pub fn failures(summary: &TestRunSummary) -> Vec<&TestCase> {
        summary
            .cases
            .iter()
            .filter(|c| c.status == TestStatus::Failed)
            .collect()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Infer a human-readable likely cause from the error type and message.
    fn infer_cause(error_type: &ErrorType, message: &str) -> String {
        match error_type {
            ErrorType::CompilationError => {
                format!("Code does not compile: {}", Self::first_sentence(message))
            }
            ErrorType::TypeMismatch => {
                let (expected, actual) = Self::extract_type_mismatch(message);
                format!("Type mismatch: expected `{expected}`, got `{actual}`")
            }
            ErrorType::MissingImport => {
                let module = Self::extract_missing_module(message);
                format!("Missing import or unresolved symbol: `{module}`")
            }
            ErrorType::AssertionFailure => {
                format!(
                    "Assertion failed: the actual value did not match the expected value. {}",
                    Self::first_sentence(message)
                )
            }
            ErrorType::PanicUnwrap => {
                "Called `.unwrap()` or `.expect()` on a `None` or `Err` value".to_string()
            }
            ErrorType::Timeout => "Test exceeded its time limit".to_string(),
            ErrorType::RuntimeError => {
                format!("Runtime error: {}", Self::first_sentence(message))
            }
            ErrorType::LogicError => {
                format!(
                    "Logic error: output did not match expectations. {}",
                    Self::first_sentence(message)
                )
            }
            ErrorType::PermissionDenied => {
                "Insufficient permissions to access a resource".to_string()
            }
            ErrorType::NotFound => {
                format!("Resource not found: {}", Self::first_sentence(message))
            }
            ErrorType::Unknown => {
                format!("Unknown error: {}", Self::first_sentence(message))
            }
        }
    }

    /// Extract potential symbol names from an error message.
    fn extract_symbols(message: &str) -> Vec<String> {
        let mut symbols = Vec::new();

        // Extract backtick-quoted identifiers: `SomeSymbol`
        if let Ok(re) = Regex::new(r"`([A-Za-z_]\w*(?:::\w+)*)`") {
            for cap in re.captures_iter(message) {
                let sym = cap[1].to_string();
                if !symbols.contains(&sym) {
                    symbols.push(sym);
                }
            }
        }

        // Extract single-quoted identifiers: 'some_fn'
        if let Ok(re) = Regex::new(r"'([A-Za-z_]\w*(?:::\w+)*)'") {
            for cap in re.captures_iter(message) {
                let sym = cap[1].to_string();
                if sym.len() > 1 && !symbols.contains(&sym) {
                    symbols.push(sym);
                }
            }
        }

        symbols
    }

    /// Extract the module/symbol name from a "not found" / "cannot find" message.
    fn extract_missing_module(message: &str) -> String {
        // Try patterns like "cannot find `Foo`", "unresolved import `bar::baz`"
        if let Ok(re) = Regex::new(r"(?:cannot find|unresolved(?: import)?)\s+`([^`]+)`") {
            if let Some(caps) = re.captures(message) {
                return caps[1].to_string();
            }
        }
        // Try "module not found: xyz"
        if let Ok(re) = Regex::new(r"module not found:\s*(\S+)") {
            if let Some(caps) = re.captures(message) {
                return caps[1].to_string();
            }
        }
        // Fallback: extract backtick-quoted name
        if let Ok(re) = Regex::new(r"`([^`]+)`") {
            if let Some(caps) = re.captures(message) {
                return caps[1].to_string();
            }
        }
        "unknown".to_string()
    }

    /// Extract expected/actual types from a type mismatch message.
    fn extract_type_mismatch(message: &str) -> (String, String) {
        // "expected X, found Y"
        if let Ok(re) = Regex::new(r"expected\s+`?([^`,]+?)`?,?\s+found\s+`?([^`\s,]+)`?") {
            if let Some(caps) = re.captures(message) {
                return (caps[1].trim().to_string(), caps[2].trim().to_string());
            }
        }
        // "mismatched types: expected X but got Y"
        if let Ok(re) = Regex::new(r"expected\s+(\S+)\s+but\s+got\s+(\S+)") {
            if let Some(caps) = re.captures(message) {
                return (caps[1].to_string(), caps[2].to_string());
            }
        }
        ("unknown".to_string(), "unknown".to_string())
    }

    /// Extract a description of a missing item from a compilation error.
    fn extract_missing_item(message: &str) -> String {
        if let Ok(re) = Regex::new(
            r"(?:cannot find|not found)\s+(?:value|type|function|method|struct|trait|module)\s+`([^`]+)`",
        ) {
            if let Some(caps) = re.captures(message) {
                return caps[1].to_string();
            }
        }
        if let Ok(re) = Regex::new(r"`([^`]+)`") {
            if let Some(caps) = re.captures(message) {
                return caps[1].to_string();
            }
        }
        "unknown item".to_string()
    }

    /// Extract a hint about a logic error from the message.
    fn extract_logic_hint(message: &str) -> String {
        // "expected X, got Y"
        if let Ok(re) = Regex::new(r"expected\s+(.+?),\s+got\s+(.+)") {
            if let Some(caps) = re.captures(message) {
                return format!(
                    "Expected `{}` but got `{}` — check the computation logic",
                    caps[1].trim(),
                    caps[2].trim()
                );
            }
        }
        "Review the implementation logic against the test expectations".to_string()
    }

    /// Format a [`FixStrategy`] as a human-readable string.
    fn format_fix_strategy(strategy: &FixStrategy) -> String {
        match strategy {
            FixStrategy::AddImport { module } => {
                format!("Add missing import: `{module}`")
            }
            FixStrategy::FixType { expected, actual } => {
                format!("Fix type: change `{actual}` to `{expected}`")
            }
            FixStrategy::UpdateAssertion => "Update the assertion's expected value".to_string(),
            FixStrategy::AddErrorHandling => {
                "Replace `.unwrap()`/`.expect()` with proper error handling".to_string()
            }
            FixStrategy::FixLogic { hint } => {
                format!("Fix logic: {hint}")
            }
            FixStrategy::AddMissing { what } => {
                format!("Add missing: `{what}`")
            }
            FixStrategy::FixPerformance => {
                "Increase timeout or optimize the code for performance".to_string()
            }
            FixStrategy::Investigate { suggestion } => {
                format!("Investigate: {suggestion}")
            }
        }
    }

    /// Return the first sentence (up to `.` or end) of a message, trimmed.
    fn first_sentence(message: &str) -> &str {
        let trimmed = message.trim();
        match trimmed.find(". ") {
            Some(idx) => &trimmed[..idx + 1],
            None => trimmed,
        }
    }
}

impl Default for TestOracle {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- Framework detection -----------------------------------------------

    #[test]
    fn test_detect_framework_cargo() {
        let output = r#"
running 3 tests
test core::test_add ... ok
test core::test_sub ... ok
test core::test_mul ... FAILED

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
"#;
        assert_eq!(
            TestOracle::detect_framework(output),
            TestFramework::CargoTest
        );
    }

    #[test]
    fn test_detect_framework_pytest() {
        let output = r#"
PASSED test_math.py::test_add
FAILED test_math.py::test_divide - ZeroDivisionError: division by zero
========================= 1 failed, 1 passed in 0.12s =========================
"#;
        assert_eq!(TestOracle::detect_framework(output), TestFramework::Pytest);
    }

    #[test]
    fn test_detect_framework_jest() {
        let output = r#"
PASS src/math.spec.ts
  ✓ adds numbers (5 ms)
FAIL src/divide.spec.ts
  ✕ divides by zero (10 ms)
    Expected: "error"
    Received: undefined

Tests: 1 failed, 1 passed, 2 total
Time: 1.234 s
"#;
        assert_eq!(TestOracle::detect_framework(output), TestFramework::Jest);
    }

    #[test]
    fn test_detect_framework_go() {
        let output = r#"
=== RUN   TestAdd
--- PASS: TestAdd (0.00s)
=== RUN   TestDivide
    math_test.go:15: expected no error, got division by zero
--- FAIL: TestDivide (0.01s)
FAIL
exit status 1
FAIL	example.com/math	0.015s
"#;
        assert_eq!(TestOracle::detect_framework(output), TestFramework::GoTest);
    }

    // -- Cargo test parsing ------------------------------------------------

    #[test]
    fn test_parse_cargo_test_all_pass() {
        let output = r#"
running 2 tests
test utils::test_trim ... ok
test utils::test_concat ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
"#;
        let summary = TestOracle::parse_output(output);
        assert_eq!(summary.framework, TestFramework::CargoTest);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.duration_ms, Some(30));
    }

    #[test]
    fn test_parse_cargo_test_with_failures() {
        let output = r#"
running 3 tests
test math::test_add ... ok
test math::test_divide ... FAILED
test math::test_mul ... ok

failures:

---- math::test_divide stdout ----
thread 'math::test_divide' panicked at 'assertion failed: `(left == right)`
  left: `0`,
 right: `1`', src/math.rs:42:5
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    math::test_divide

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
"#;
        let summary = TestOracle::parse_output(output);
        assert_eq!(summary.framework, TestFramework::CargoTest);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);

        let failed = TestOracle::failures(&summary);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].name, "math::test_divide");
        assert!(failed[0].error_message.is_some());
        assert!(failed[0].stdout.is_some());
    }

    #[test]
    fn test_parse_cargo_test_with_ignored() {
        let output = r#"
running 3 tests
test net::test_connect ... ok
test net::test_timeout ... ignored
test net::test_retry ... ok

test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.05s
"#;
        let summary = TestOracle::parse_output(output);
        assert_eq!(summary.framework, TestFramework::CargoTest);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 1);
    }

    // -- Pytest parsing ----------------------------------------------------

    #[test]
    fn test_parse_pytest_output() {
        let output = r#"
PASSED test_calculator.py::test_add
PASSED test_calculator.py::test_subtract
FAILED test_calculator.py::test_divide - ZeroDivisionError: division by zero
SKIPPED test_calculator.py::test_network - reason: no network
========================= 1 failed, 2 passed, 1 skipped in 0.34s =========================
"#;
        let summary = TestOracle::parse_output(output);
        assert_eq!(summary.framework, TestFramework::Pytest);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);

        let failed = TestOracle::failures(&summary);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].name, "test_divide");
        assert_eq!(failed[0].file, Some("test_calculator.py".to_string()));
        assert!(failed[0]
            .error_message
            .as_ref()
            .unwrap()
            .contains("ZeroDivisionError"));
    }

    // -- Jest parsing ------------------------------------------------------

    #[test]
    fn test_parse_jest_output() {
        let output = "PASS src/math.spec.ts\n  \u{2713} adds two numbers (3 ms)\nFAIL src/divide.spec.ts\n  \u{2715} handles division by zero (8 ms)\n    Expected: \"error\"\n    Received: undefined\n\nTests: 1 failed, 1 passed, 2 total\nTime: 2.456 s\n";

        let summary = TestOracle::parse_output(output);
        assert_eq!(summary.framework, TestFramework::Jest);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.duration_ms, Some(2456));

        let failed = TestOracle::failures(&summary);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].name, "handles division by zero");
        assert!(failed[0].error_message.is_some());
        let err_msg = failed[0].error_message.as_ref().unwrap();
        assert!(err_msg.contains("Expected:"));
        assert!(err_msg.contains("Received:"));
    }

    // -- Go test parsing ---------------------------------------------------

    #[test]
    fn test_parse_go_test_output() {
        let output = r#"
=== RUN   TestAdd
--- PASS: TestAdd (0.00s)
=== RUN   TestSubtract
--- PASS: TestSubtract (0.00s)
=== RUN   TestDivide
    math_test.go:25: expected 5, got 0
--- FAIL: TestDivide (0.01s)
FAIL
exit status 1
FAIL	example.com/calculator	0.023s
"#;
        let summary = TestOracle::parse_output(output);
        assert_eq!(summary.framework, TestFramework::GoTest);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.duration_ms, Some(23));

        let failed = TestOracle::failures(&summary);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].name, "TestDivide");
    }

    // -- Error classification ----------------------------------------------

    #[test]
    fn test_classify_error_type_mismatch() {
        assert_eq!(
            TestOracle::classify_error("mismatched types: expected `u32`, found `i64`"),
            ErrorType::TypeMismatch
        );
        assert_eq!(
            TestOracle::classify_error("expected String, found &str"),
            ErrorType::TypeMismatch
        );
    }

    #[test]
    fn test_classify_error_missing_import() {
        assert_eq!(
            TestOracle::classify_error("cannot find value `HashMap` in this scope"),
            ErrorType::MissingImport
        );
        assert_eq!(
            TestOracle::classify_error("unresolved import `std::collections::BTreeMap`"),
            ErrorType::MissingImport
        );
    }

    #[test]
    fn test_classify_error_assertion() {
        assert_eq!(
            TestOracle::classify_error("assertion failed: `(left == right)`"),
            ErrorType::AssertionFailure
        );
        assert_eq!(
            TestOracle::classify_error("AssertionError: 2 != 3"),
            ErrorType::AssertionFailure
        );
    }

    #[test]
    fn test_classify_error_panic_unwrap() {
        assert_eq!(
            TestOracle::classify_error("called `Option::unwrap()` on a `None` value"),
            ErrorType::PanicUnwrap
        );
        assert_eq!(
            TestOracle::classify_error("called `Result::unwrap()` on an `Err` value: NotFound"),
            ErrorType::PanicUnwrap
        );
    }

    #[test]
    fn test_classify_error_compilation() {
        assert_eq!(
            TestOracle::classify_error("error[E0433]: failed to resolve: use of undeclared crate"),
            ErrorType::CompilationError
        );
    }

    // -- Failure analysis --------------------------------------------------

    #[test]
    fn test_analyze_failure() {
        let test_case = TestCase {
            name: "math::test_divide".to_string(),
            status: TestStatus::Failed,
            duration_ms: Some(10),
            error_message: Some(
                "assertion failed: `(left == right)`\n  left: `0`,\n right: `1`".to_string(),
            ),
            file: Some("src/math.rs".to_string()),
            line: Some(42),
            stdout: None,
        };

        let analysis = TestOracle::analyze_failure(&test_case);
        assert_eq!(analysis.test_name, "math::test_divide");
        assert_eq!(analysis.error_type, ErrorType::AssertionFailure);
        assert_eq!(analysis.source_file, Some("src/math.rs".to_string()));
        assert_eq!(analysis.source_line, Some(42));
        assert_eq!(analysis.fix_strategy, FixStrategy::UpdateAssertion);
    }

    // -- Fix suggestions ---------------------------------------------------

    #[test]
    fn test_suggest_fix_add_import() {
        let fix = TestOracle::suggest_fix(
            &ErrorType::MissingImport,
            "cannot find `HashMap` in this scope",
        );
        match fix {
            FixStrategy::AddImport { module } => {
                assert!(module.contains("HashMap"));
            }
            other => panic!("expected AddImport, got {other:?}"),
        }
    }

    #[test]
    fn test_suggest_fix_type_mismatch() {
        let fix = TestOracle::suggest_fix(&ErrorType::TypeMismatch, "expected `u32`, found `i64`");
        match fix {
            FixStrategy::FixType { expected, actual } => {
                assert_eq!(expected, "u32");
                assert_eq!(actual, "i64");
            }
            other => panic!("expected FixType, got {other:?}"),
        }
    }

    // -- TDD cycle ---------------------------------------------------------

    #[test]
    fn test_tdd_cycle_start() {
        let cycle = TestOracle::start_tdd("test_new_feature", 5);
        assert_eq!(cycle.phase, TddPhase::Red);
        assert_eq!(cycle.target_test, Some("test_new_feature".to_string()));
        assert_eq!(cycle.iterations, 0);
        assert_eq!(cycle.max_iterations, 5);
        assert!(cycle.history.is_empty());
    }

    #[test]
    fn test_tdd_cycle_advance_red_to_green() {
        let mut cycle = TestOracle::start_tdd("test_feature", 10);

        // Simulate the target test failing (Red phase confirmed)
        let summary = TestRunSummary {
            framework: TestFramework::CargoTest,
            total: 1,
            passed: 0,
            failed: 1,
            skipped: 0,
            errors: 0,
            duration_ms: Some(50),
            cases: vec![TestCase {
                name: "test_feature".to_string(),
                status: TestStatus::Failed,
                duration_ms: Some(50),
                error_message: Some("not yet implemented".to_string()),
                file: None,
                line: None,
                stdout: None,
            }],
        };

        let action = TestOracle::advance_tdd(&mut cycle, &summary);
        assert_eq!(cycle.phase, TddPhase::Green);
        assert!(action.contains("failing as expected"));
        assert_eq!(cycle.iterations, 1);
        assert_eq!(cycle.history.len(), 1);
    }

    #[test]
    fn test_tdd_cycle_advance_green_to_refactor() {
        let mut cycle = TestOracle::start_tdd("test_feature", 10);
        cycle.phase = TddPhase::Green;

        // All tests pass — move to Refactor
        let summary = TestRunSummary {
            framework: TestFramework::CargoTest,
            total: 2,
            passed: 2,
            failed: 0,
            skipped: 0,
            errors: 0,
            duration_ms: Some(30),
            cases: vec![
                TestCase {
                    name: "test_feature".to_string(),
                    status: TestStatus::Passed,
                    duration_ms: Some(15),
                    error_message: None,
                    file: None,
                    line: None,
                    stdout: None,
                },
                TestCase {
                    name: "test_other".to_string(),
                    status: TestStatus::Passed,
                    duration_ms: Some(15),
                    error_message: None,
                    file: None,
                    line: None,
                    stdout: None,
                },
            ],
        };

        let action = TestOracle::advance_tdd(&mut cycle, &summary);
        assert_eq!(cycle.phase, TddPhase::Refactor);
        assert!(action.contains("refactor"));
    }

    // -- Utility methods ---------------------------------------------------

    #[test]
    fn test_all_passing() {
        let passing = TestRunSummary {
            framework: TestFramework::CargoTest,
            total: 3,
            passed: 3,
            failed: 0,
            skipped: 0,
            errors: 0,
            duration_ms: None,
            cases: vec![],
        };
        assert!(TestOracle::all_passing(&passing));

        let failing = TestRunSummary {
            framework: TestFramework::CargoTest,
            total: 3,
            passed: 2,
            failed: 1,
            skipped: 0,
            errors: 0,
            duration_ms: None,
            cases: vec![],
        };
        assert!(!TestOracle::all_passing(&failing));
    }

    #[test]
    fn test_failures_filter() {
        let summary = TestRunSummary {
            framework: TestFramework::CargoTest,
            total: 4,
            passed: 2,
            failed: 2,
            skipped: 0,
            errors: 0,
            duration_ms: None,
            cases: vec![
                TestCase {
                    name: "test_a".to_string(),
                    status: TestStatus::Passed,
                    duration_ms: None,
                    error_message: None,
                    file: None,
                    line: None,
                    stdout: None,
                },
                TestCase {
                    name: "test_b".to_string(),
                    status: TestStatus::Failed,
                    duration_ms: None,
                    error_message: Some("assert failed".to_string()),
                    file: None,
                    line: None,
                    stdout: None,
                },
                TestCase {
                    name: "test_c".to_string(),
                    status: TestStatus::Passed,
                    duration_ms: None,
                    error_message: None,
                    file: None,
                    line: None,
                    stdout: None,
                },
                TestCase {
                    name: "test_d".to_string(),
                    status: TestStatus::Failed,
                    duration_ms: None,
                    error_message: Some("panic".to_string()),
                    file: None,
                    line: None,
                    stdout: None,
                },
            ],
        };

        let failures = TestOracle::failures(&summary);
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].name, "test_b");
        assert_eq!(failures[1].name, "test_d");
    }

    #[test]
    fn test_fix_prompt_generation() {
        let analysis = FailureAnalysis {
            test_name: "math::test_divide".to_string(),
            error_type: ErrorType::AssertionFailure,
            error_message: "assertion failed: `(left == right)`: left = 0, right = 1".to_string(),
            source_file: Some("src/math.rs".to_string()),
            source_line: Some(42),
            likely_cause: "Assertion failed".to_string(),
            fix_strategy: FixStrategy::UpdateAssertion,
            related_symbols: vec!["divide".to_string()],
        };

        let prompt = TestOracle::fix_prompt(&analysis);
        assert!(prompt.contains("math::test_divide"));
        assert!(prompt.contains("src/math.rs"));
        assert!(prompt.contains("line 42"));
        assert!(prompt.contains("assertion"));
        assert!(prompt.contains("Instructions"));
        assert!(prompt.contains("divide"));
    }
}
