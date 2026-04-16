// SPDX-License-Identifier: AGPL-3.0-only
//! Developer Experience (DX) metrics for Phase 3 Track 3.
//!
//! DX is assessed across five independently measurable dimensions:
//!
//! 1. **TTFA** — Time-to-First-Agent: net LOC for a minimal working agent.
//! 2. **Tool definition LOC** — net lines to add one tool (with-tool - hello-world).
//! 3. **Error clarity** — 0–10 per error scenario (3 scenarios, mean reported).
//!    Each scenario is scored on three sub-dimensions (0–10 each):
//!    - `file_line`: does the error point to the user's code?
//!    - `names_problem`: does the message name what went wrong?
//!    - `suggests_fix`: does the message tell the developer what to do?
//! 4. **Type safety** — 0–10: does the framework use the type system to
//!    prevent misuse at compile/construction time vs. runtime?
//! 5. **Doc quality** — 0–10: subjective rating of official docs + examples
//!    (rated once per framework, justified in `DX_BENCHMARKS.md`).
//!
//! The `DxMetric` struct stores **observed values** only — no opinions.
//! Scoring interpretation lives in `docs/DX_BENCHMARKS.md`.

use serde::{Deserialize, Serialize};

/// Error-message clarity scores for one bug scenario (3-question rubric).
///
/// Each field is 0–10:
/// - `file_line`: does the error point to the user's code file/line?
/// - `names_problem`: does the message name what went wrong?
/// - `suggests_fix`: does the message tell the developer how to fix it?
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorScenarioScore {
    /// Scenario identifier (e.g. "missing_api_key", "typo_tool_name",
    /// "malformed_prompt_template").
    pub scenario: String,
    /// Does the error point to the user's own file/line? (0–10)
    pub file_line: f32,
    /// Does the error name the specific problem? (0–10)
    pub names_problem: f32,
    /// Does the error suggest a concrete fix? (0–10)
    pub suggests_fix: f32,
}

impl ErrorScenarioScore {
    /// Aggregate score for this scenario: mean of the three sub-dimensions.
    pub fn score(&self) -> f32 {
        (self.file_line + self.names_problem + self.suggests_fix) / 3.0
    }
}

/// DX metrics for one framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DxMetric {
    /// Framework identifier (e.g. "argentor", "langchain").
    pub framework: String,

    /// Net LOC for minimal working agent (hello_world).
    pub ttfa_loc: u32,

    /// Incremental net LOC to add one tool (with_tool - hello_world).
    pub tool_delta_loc: u32,

    /// Incremental net LOC for multi-turn support (multi_turn - hello_world).
    pub multi_turn_delta_loc: u32,

    /// Error message clarity scores, one entry per scenario.
    /// Expected: ["missing_api_key", "typo_tool_name", "malformed_prompt_template"].
    pub error_scores: Vec<ErrorScenarioScore>,

    /// Type-safety score 0–10.
    /// 10 = compile-time guarantees; 0 = entirely runtime / stringly typed.
    pub type_safety: f32,

    /// Documentation quality 0–10 (see DX_BENCHMARKS.md for rubric).
    pub doc_quality: f32,
}

impl DxMetric {
    /// Mean error-clarity score across all scenarios (0–10).
    /// Returns 0.0 if no scenarios have been scored.
    pub fn mean_error_score(&self) -> f32 {
        if self.error_scores.is_empty() {
            return 0.0;
        }
        let total: f32 = self.error_scores.iter().map(|s| s.score()).sum();
        total / self.error_scores.len() as f32
    }

    /// Composite DX score (0–10).
    ///
    /// Weights (justified in DX_BENCHMARKS.md):
    /// - Error clarity:       30% — most time-wasting friction for new users
    /// - Type safety:         25% — correlates with long-term maintainability
    /// - Tool delta LOC:      20% — most common next step after hello-world
    /// - TTFA LOC:            15% — first impression, diminishing returns past ~20 LOC
    /// - Doc quality:         10% — matters most on first contact
    ///
    /// LOC scores are inverted (fewer = better) and normalised using the
    /// reference values passed in. If `max_loc` is 0 the LOC terms are
    /// excluded (score is computed from quality dimensions only).
    pub fn composite(&self, max_ttfa_loc: u32, max_tool_delta_loc: u32) -> f32 {
        let error_score = self.mean_error_score(); // already 0–10

        // Invert LOC: (max - this) / max * 10, clamped to [0,10]
        let ttfa_score = if max_ttfa_loc == 0 {
            5.0
        } else {
            let inv =
                (max_ttfa_loc.saturating_sub(self.ttfa_loc)) as f32 / max_ttfa_loc as f32 * 10.0;
            inv.clamp(0.0, 10.0)
        };

        let tool_score = if max_tool_delta_loc == 0 {
            5.0
        } else {
            let inv = (max_tool_delta_loc.saturating_sub(self.tool_delta_loc)) as f32
                / max_tool_delta_loc as f32
                * 10.0;
            inv.clamp(0.0, 10.0)
        };

        0.30 * error_score
            + 0.25 * self.type_safety
            + 0.20 * tool_score
            + 0.15 * ttfa_score
            + 0.10 * self.doc_quality
    }
}

/// Build observed DX metrics from the benchmark data collected in
/// `benchmarks/dx/` and `docs/DX_BENCHMARKS.md`.
///
/// These values are **static observations** — the LOC counts come from
/// the comment headers in each example file; error scores come from the
/// error scenario analysis in each `errors/*.{rs,py}` file.
pub fn observed_metrics() -> Vec<DxMetric> {
    vec![
        DxMetric {
            framework: "argentor".to_string(),
            ttfa_loc: 14,
            tool_delta_loc: 16,      // 30 - 14
            multi_turn_delta_loc: 4, // 18 - 14
            error_scores: vec![
                ErrorScenarioScore {
                    scenario: "missing_api_key".to_string(),
                    file_line: 0.0,
                    names_problem: 10.0,
                    suggests_fix: 10.0,
                },
                ErrorScenarioScore {
                    scenario: "typo_tool_name".to_string(),
                    file_line: 0.0,
                    names_problem: 10.0,
                    suggests_fix: 8.0,
                },
                ErrorScenarioScore {
                    scenario: "malformed_prompt_template".to_string(),
                    file_line: 0.0,
                    names_problem: 0.0,
                    suggests_fix: 0.0,
                },
            ],
            type_safety: 9.0,
            doc_quality: 6.0,
        },
        DxMetric {
            framework: "langchain".to_string(),
            ttfa_loc: 5,
            tool_delta_loc: 11,       // 16 - 5
            multi_turn_delta_loc: 16, // 21 - 5
            error_scores: vec![
                ErrorScenarioScore {
                    scenario: "missing_api_key".to_string(),
                    file_line: 0.0,
                    names_problem: 4.0,
                    suggests_fix: 0.0,
                },
                ErrorScenarioScore {
                    scenario: "typo_tool_name".to_string(),
                    file_line: 0.0,
                    names_problem: 9.0,
                    suggests_fix: 7.0,
                },
                ErrorScenarioScore {
                    scenario: "malformed_prompt_template".to_string(),
                    file_line: 3.0,
                    names_problem: 6.0,
                    suggests_fix: 3.0,
                },
            ],
            type_safety: 3.0,
            doc_quality: 8.0,
        },
        DxMetric {
            framework: "crewai".to_string(),
            ttfa_loc: 14,
            tool_delta_loc: 5,       // 19 - 14
            multi_turn_delta_loc: 2, // 16 - 14 (but not idiomatic — see DX_BENCHMARKS.md)
            error_scores: vec![
                ErrorScenarioScore {
                    scenario: "missing_api_key".to_string(),
                    file_line: 0.0,
                    names_problem: 5.0,
                    suggests_fix: 1.0,
                },
                ErrorScenarioScore {
                    scenario: "typo_tool_name".to_string(),
                    file_line: 0.0,
                    names_problem: 6.0,
                    suggests_fix: 4.0,
                },
                ErrorScenarioScore {
                    scenario: "malformed_prompt_template".to_string(),
                    file_line: 0.0,
                    names_problem: 0.0,
                    suggests_fix: 0.0,
                },
            ],
            type_safety: 2.0,
            doc_quality: 6.0,
        },
        DxMetric {
            framework: "pydantic_ai".to_string(),
            ttfa_loc: 7,
            tool_delta_loc: 3,       // 10 - 7
            multi_turn_delta_loc: 4, // 11 - 7
            error_scores: vec![
                ErrorScenarioScore {
                    scenario: "missing_api_key".to_string(),
                    file_line: 0.0,
                    names_problem: 5.0,
                    suggests_fix: 2.0,
                },
                ErrorScenarioScore {
                    scenario: "typo_tool_name".to_string(),
                    file_line: 5.0,
                    names_problem: 7.0,
                    suggests_fix: 3.0,
                },
                ErrorScenarioScore {
                    scenario: "malformed_prompt_template".to_string(),
                    file_line: 0.0,
                    names_problem: 0.0,
                    suggests_fix: 0.0,
                },
            ],
            type_safety: 8.0,
            doc_quality: 7.0,
        },
        DxMetric {
            framework: "claude_agent_sdk".to_string(),
            ttfa_loc: 9,
            tool_delta_loc: 23,      // 32 - 9
            multi_turn_delta_loc: 8, // 17 - 9
            error_scores: vec![
                ErrorScenarioScore {
                    scenario: "missing_api_key".to_string(),
                    file_line: 0.0,
                    names_problem: 10.0,
                    suggests_fix: 9.0,
                },
                ErrorScenarioScore {
                    scenario: "typo_tool_name".to_string(),
                    file_line: 0.0,
                    names_problem: 0.0,
                    suggests_fix: 0.0,
                },
                ErrorScenarioScore {
                    scenario: "malformed_prompt_template".to_string(),
                    file_line: 0.0,
                    names_problem: 0.0,
                    suggests_fix: 0.0,
                },
            ],
            type_safety: 5.0,
            doc_quality: 9.0,
        },
    ]
}

/// Compute composite DX scores for all observed frameworks.
///
/// Normalises LOC dimensions against the worst observed value in the set,
/// so the result is self-contained (no external reference required).
pub fn compute_all() -> Vec<(String, f32)> {
    let metrics = observed_metrics();

    let max_ttfa = metrics.iter().map(|m| m.ttfa_loc).max().unwrap_or(1);
    let max_tool = metrics.iter().map(|m| m.tool_delta_loc).max().unwrap_or(1);

    metrics
        .into_iter()
        .map(|m| {
            let score = m.composite(max_ttfa, max_tool);
            (m.framework.clone(), score)
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn error_scenario_score_mean() {
        let s = ErrorScenarioScore {
            scenario: "test".to_string(),
            file_line: 0.0,
            names_problem: 9.0,
            suggests_fix: 6.0,
        };
        let expected = (0.0 + 9.0 + 6.0) / 3.0;
        assert!((s.score() - expected).abs() < 0.001);
    }

    #[test]
    fn mean_error_score_empty() {
        let m = DxMetric {
            framework: "empty".to_string(),
            ttfa_loc: 10,
            tool_delta_loc: 5,
            multi_turn_delta_loc: 3,
            error_scores: vec![],
            type_safety: 5.0,
            doc_quality: 5.0,
        };
        assert_eq!(m.mean_error_score(), 0.0);
    }

    #[test]
    fn mean_error_score_single() {
        let m = DxMetric {
            framework: "test".to_string(),
            ttfa_loc: 10,
            tool_delta_loc: 5,
            multi_turn_delta_loc: 3,
            error_scores: vec![ErrorScenarioScore {
                scenario: "s".to_string(),
                file_line: 3.0,
                names_problem: 6.0,
                suggests_fix: 9.0,
            }],
            type_safety: 5.0,
            doc_quality: 5.0,
        };
        assert!((m.mean_error_score() - 6.0).abs() < 0.001);
    }

    #[test]
    fn composite_perfect_framework() {
        // A framework with 0 LOC, 10/10 everything → should score 10.0
        let m = DxMetric {
            framework: "perfect".to_string(),
            ttfa_loc: 0,
            tool_delta_loc: 0,
            multi_turn_delta_loc: 0,
            error_scores: vec![ErrorScenarioScore {
                scenario: "s".to_string(),
                file_line: 10.0,
                names_problem: 10.0,
                suggests_fix: 10.0,
            }],
            type_safety: 10.0,
            doc_quality: 10.0,
        };
        let score = m.composite(30, 30);
        assert!((score - 10.0).abs() < 0.001);
    }

    #[test]
    fn composite_worst_framework() {
        // A framework with max LOC and 0 on quality → score = 0.0
        let m = DxMetric {
            framework: "worst".to_string(),
            ttfa_loc: 30,
            tool_delta_loc: 30,
            multi_turn_delta_loc: 30,
            error_scores: vec![ErrorScenarioScore {
                scenario: "s".to_string(),
                file_line: 0.0,
                names_problem: 0.0,
                suggests_fix: 0.0,
            }],
            type_safety: 0.0,
            doc_quality: 0.0,
        };
        let score = m.composite(30, 30);
        assert!(score.abs() < 0.001);
    }

    #[test]
    fn observed_metrics_have_five_frameworks() {
        assert_eq!(observed_metrics().len(), 5);
    }

    #[test]
    fn all_frameworks_have_three_error_scenarios() {
        for m in observed_metrics() {
            assert_eq!(
                m.error_scores.len(),
                3,
                "framework {} should have 3 error scenarios",
                m.framework
            );
        }
    }

    #[test]
    fn compute_all_returns_five_scores() {
        let scores = compute_all();
        assert_eq!(scores.len(), 5);
        for (name, score) in &scores {
            assert!(
                (0.0..=10.0).contains(score),
                "score for {} is out of range: {}",
                name,
                score
            );
        }
    }

    #[test]
    fn argentor_has_best_type_safety() {
        let metrics = observed_metrics();
        let argentor = metrics.iter().find(|m| m.framework == "argentor").unwrap();
        let max_type_safety = metrics
            .iter()
            .map(|m| m.type_safety)
            .fold(0.0_f32, f32::max);
        assert!(
            (argentor.type_safety - max_type_safety).abs() < 0.001,
            "expected Argentor to have highest type_safety score"
        );
    }

    #[test]
    fn pydantic_ai_has_lowest_tool_loc() {
        let metrics = observed_metrics();
        let pydantic = metrics
            .iter()
            .find(|m| m.framework == "pydantic_ai")
            .unwrap();
        let min_tool_delta = metrics.iter().map(|m| m.tool_delta_loc).min().unwrap();
        assert_eq!(
            pydantic.tool_delta_loc, min_tool_delta,
            "PydanticAI should have the lowest tool_delta_loc"
        );
    }

    #[test]
    fn scores_are_stable() {
        // Regression test: ensure composite scores don't drift unexpectedly.
        let scores = compute_all();
        let map: std::collections::HashMap<String, f32> = scores.into_iter().collect();

        // PydanticAI should score above 5.0 (wins on LOC ergonomics)
        assert!(
            map["pydantic_ai"] > 5.0,
            "pydantic_ai score dropped below 5.0"
        );
        // Argentor is penalised by Rust verbosity on TTFA but scores above 4.0
        // thanks to type-safety and error-message quality wins.
        assert!(map["argentor"] > 4.0, "argentor score dropped below 4.0");

        // Claude SDK should NOT be penalised to near-zero despite verbose tool LOC
        assert!(
            map["claude_agent_sdk"] > 2.0,
            "claude_agent_sdk score is suspiciously low"
        );
    }
}
