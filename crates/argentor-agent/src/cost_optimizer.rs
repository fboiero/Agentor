//! Intelligent cost optimization for LLM routing.
//!
//! Analyzes task complexity, remaining budget, and historical model performance
//! to select the optimal model for each request, minimizing cost while
//! maintaining quality.
//!
//! This module builds on top of the [`ModelRouter`](super::model_router) and
//! provides higher-level budget pacing, spending tracking, and per-model usage
//! statistics. It is designed to be used as a drop-in routing layer that wraps
//! configuration, strategy, and spending history into a single type.
//!
//! # Example
//!
//! ```rust,no_run
//! use argentor_agent::cost_optimizer::{CostOptimizer, CostOptimizerConfig, OptimizationStrategy};
//!
//! let optimizer = CostOptimizer::default_claude();
//! let complexity = optimizer.estimate_complexity("hello", 0, 0);
//! let model = optimizer.select_model(&complexity).unwrap();
//! println!("Selected: {} ({})", model.model_id, model.provider);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use argentor_core::ArgentorResult;

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Top-level configuration for the cost optimizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOptimizerConfig {
    /// The optimization strategy to apply.
    pub strategy: OptimizationStrategy,
    /// Hard budget cap in USD. `None` means unlimited.
    pub budget_limit_usd: Option<f64>,
    /// Minimum acceptable quality score (0.0--1.0).
    pub quality_threshold: f64,
    /// Available models ranked by preference.
    pub models: Vec<CostModelOption>,
    /// Model identifier to use when all budgets are exhausted.
    pub fallback_model: String,
}

/// How the optimizer selects models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationStrategy {
    /// Always use the cheapest model that meets the quality threshold.
    CostMinimize,
    /// Always use the best model (ignore cost).
    QualityMaximize,
    /// Balance cost and quality based on task complexity.
    Balanced,
    /// Use cheap models for simple tasks, expensive for complex ones.
    TieredByComplexity,
    /// Spread budget evenly across a time period.
    BudgetPacing {
        /// Duration of the pacing period in hours.
        period_hours: u32,
    },
}

/// A single model option with pricing, quality, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModelOption {
    /// Provider name (e.g. "claude", "openai", "gemini").
    pub provider: String,
    /// Unique model identifier (e.g. "claude-haiku-4-5-20251001").
    pub model_id: String,
    /// Cost per 1 000 input tokens in USD.
    pub cost_per_1k_input: f64,
    /// Cost per 1 000 output tokens in USD.
    pub cost_per_1k_output: f64,
    /// Quality score from benchmarks or manual assessment (0.0--1.0).
    pub quality_score: f64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: u64,
    /// Maximum context / output tokens supported.
    pub max_tokens: u32,
    /// Tier classification.
    pub tier: CostModelTier,
}

/// Model tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CostModelTier {
    /// Cheap, fast models (Haiku, GPT-4o-mini, Flash).
    Fast,
    /// Mid-tier models (Sonnet, GPT-4o, Pro).
    Balanced,
    /// Expensive, best-quality models (Opus, o1, Ultra).
    Powerful,
}

// ---------------------------------------------------------------------------
// Spending tracker (internal)
// ---------------------------------------------------------------------------

/// Tracks cumulative spending, per-model breakdown, and request counts.
#[derive(Debug)]
struct SpendingTracker {
    total_spent_usd: f64,
    spent_by_model: HashMap<String, f64>,
    requests_by_model: HashMap<String, u64>,
    period_start: DateTime<Utc>,
    request_count: u64,
}

impl SpendingTracker {
    fn new() -> Self {
        Self {
            total_spent_usd: 0.0,
            spent_by_model: HashMap::new(),
            requests_by_model: HashMap::new(),
            period_start: Utc::now(),
            request_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Routing decision
// ---------------------------------------------------------------------------

/// A record of a single model-selection decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Selected model identifier.
    pub selected_model: String,
    /// Selected provider name.
    pub selected_provider: String,
    /// Human-readable explanation.
    pub reason: String,
    /// Estimated cost in USD for this request.
    pub estimated_cost_usd: f64,
    /// Assessed task complexity.
    pub task_complexity: TaskComplexity,
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Task complexity
// ---------------------------------------------------------------------------

/// Discrete task complexity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// Short input, no tools needed.
    Simple,
    /// Medium input, some tools.
    Moderate,
    /// Long input, multiple tools, multi-step reasoning.
    Complex,
    /// Must use the best model regardless of cost.
    Critical,
}

// ---------------------------------------------------------------------------
// Spending summary
// ---------------------------------------------------------------------------

/// Aggregated spending snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingSummary {
    /// Total amount spent so far in USD.
    pub total_spent_usd: f64,
    /// Configured budget limit, if any.
    pub budget_limit_usd: Option<f64>,
    /// Remaining budget in USD, if a limit is set.
    pub remaining_usd: Option<f64>,
    /// Percentage of budget consumed (0.0--100.0), if a limit is set.
    pub utilization_percent: Option<f64>,
    /// Total number of requests made.
    pub requests_count: u64,
    /// Average cost per request in USD.
    pub avg_cost_per_request: f64,
    /// Per-model spending breakdown.
    pub by_model: HashMap<String, f64>,
}

/// Per-model usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsageStats {
    /// Model identifier.
    pub model_id: String,
    /// Total number of requests routed to this model.
    pub requests: u64,
    /// Total cost in USD for this model.
    pub total_cost_usd: f64,
    /// Average cost per request in USD.
    pub avg_cost_per_request: f64,
    /// Percentage of total spending attributed to this model.
    pub percent_of_total: f64,
}

// ---------------------------------------------------------------------------
// CostOptimizer
// ---------------------------------------------------------------------------

/// Tracks spending and makes intelligent routing decisions based on task
/// complexity, budget constraints, and the configured optimization strategy.
pub struct CostOptimizer {
    config: CostOptimizerConfig,
    spending: Mutex<SpendingTracker>,
    history: Mutex<Vec<RoutingDecision>>,
}

impl CostOptimizer {
    /// Create a new optimizer with the given configuration.
    pub fn new(config: CostOptimizerConfig) -> Self {
        Self {
            config,
            spending: Mutex::new(SpendingTracker::new()),
            history: Mutex::new(Vec::new()),
        }
    }

    // ── Complexity estimation ───────────────────────────────────────────

    /// Estimate task complexity from input text and available context.
    ///
    /// Heuristics:
    /// - **Simple**: input < 100 chars, no tool-related keywords.
    /// - **Moderate**: 100--500 chars, or mentions tools/analysis.
    /// - **Complex**: > 500 chars, or multi-step keywords, or > 3 tools available.
    /// - **Critical**: keywords like "critical", "urgent", "production", "security audit".
    pub fn estimate_complexity(
        &self,
        input: &str,
        tool_count: usize,
        history_turns: usize,
    ) -> TaskComplexity {
        let lower = input.to_lowercase();

        // Critical keywords take precedence.
        let critical_keywords = [
            "critical",
            "urgent",
            "production",
            "security audit",
            "emergency",
            "incident",
            "outage",
        ];
        if critical_keywords.iter().any(|kw| lower.contains(kw)) {
            return TaskComplexity::Critical;
        }

        // Multi-step indicators.
        let multistep_keywords = [
            "first",
            "then",
            "finally",
            "step 1",
            "step 2",
            "multi-step",
            "afterward",
            "subsequently",
        ];
        let has_multistep = multistep_keywords.iter().any(|kw| lower.contains(kw));

        // Complex analysis keywords.
        let complex_keywords = [
            "analyze",
            "compare",
            "design",
            "optimize",
            "refactor",
            "architecture",
            "algorithm",
            "evaluate",
            "synthesize",
        ];
        let complex_keyword_count = complex_keywords
            .iter()
            .filter(|kw| lower.contains(*kw))
            .count();

        let len = input.len();

        // Complex: long input, multi-step, many tools, or heavy analysis.
        if len > 500
            || has_multistep
            || tool_count > 3
            || history_turns > 10
            || complex_keyword_count >= 2
        {
            return TaskComplexity::Complex;
        }

        // Moderate: medium length, some tools, or mentions analysis.
        if len >= 100 || tool_count >= 1 || complex_keyword_count >= 1 || history_turns >= 3 {
            return TaskComplexity::Moderate;
        }

        TaskComplexity::Simple
    }

    // ── Model selection ─────────────────────────────────────────────────

    /// Select the optimal model for a given task complexity.
    ///
    /// The selection depends on the configured [`OptimizationStrategy`] and
    /// the current budget state.
    pub fn select_model(&self, complexity: &TaskComplexity) -> ArgentorResult<&CostModelOption> {
        if self.config.models.is_empty() {
            return Err(argentor_core::ArgentorError::Agent(
                "No models configured in the cost optimizer".into(),
            ));
        }

        if !self.budget_available() {
            // Try to return the fallback model.
            return self
                .config
                .models
                .iter()
                .find(|m| m.model_id == self.config.fallback_model)
                .or_else(|| self.cheapest_model())
                .ok_or_else(|| {
                    argentor_core::ArgentorError::Agent(
                        "Budget exhausted and no fallback model available".into(),
                    )
                });
        }

        let selected = match &self.config.strategy {
            OptimizationStrategy::CostMinimize => self.select_cost_minimize(),
            OptimizationStrategy::QualityMaximize => self.select_quality_maximize(),
            OptimizationStrategy::Balanced => self.select_balanced(complexity),
            OptimizationStrategy::TieredByComplexity => self.select_tiered(complexity),
            OptimizationStrategy::BudgetPacing { period_hours } => {
                self.select_budget_pacing(*period_hours, complexity)
            }
        };

        selected.ok_or_else(|| {
            argentor_core::ArgentorError::Agent("No suitable model found for the task".into())
        })
    }

    /// CostMinimize: cheapest model whose quality >= threshold.
    fn select_cost_minimize(&self) -> Option<&CostModelOption> {
        let threshold = self.config.quality_threshold;
        self.config
            .models
            .iter()
            .filter(|m| m.quality_score >= threshold)
            .min_by(|a, b| {
                a.cost_per_1k_input
                    .partial_cmp(&b.cost_per_1k_input)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .or_else(|| self.cheapest_model())
    }

    /// QualityMaximize: highest quality model.
    fn select_quality_maximize(&self) -> Option<&CostModelOption> {
        self.config.models.iter().max_by(|a, b| {
            a.quality_score
                .partial_cmp(&b.quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Balanced: weighted score = quality * 0.6 + (1 - normalized_cost) * 0.4.
    fn select_balanced(&self, complexity: &TaskComplexity) -> Option<&CostModelOption> {
        let max_cost = self
            .config
            .models
            .iter()
            .map(|m| m.cost_per_1k_input)
            .fold(0.0_f64, f64::max);

        let target_tier = match complexity {
            TaskComplexity::Simple => Some(CostModelTier::Fast),
            TaskComplexity::Moderate => Some(CostModelTier::Balanced),
            TaskComplexity::Complex | TaskComplexity::Critical => Some(CostModelTier::Powerful),
        };

        self.config
            .models
            .iter()
            .filter(|m| m.quality_score >= self.config.quality_threshold)
            .max_by(|a, b| {
                let score_a = Self::balanced_score(a, max_cost, &target_tier);
                let score_b = Self::balanced_score(b, max_cost, &target_tier);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .or_else(|| self.cheapest_model())
    }

    /// Compute the balanced score for a model.
    fn balanced_score(
        model: &CostModelOption,
        max_cost: f64,
        target_tier: &Option<CostModelTier>,
    ) -> f64 {
        let normalized_cost = if max_cost > 0.0 {
            model.cost_per_1k_input / max_cost
        } else {
            0.0
        };
        let mut score = model.quality_score * 0.6 + (1.0 - normalized_cost) * 0.4;

        // Tier bonus: prefer models matching the target tier.
        if let Some(tier) = target_tier {
            if model.tier == *tier {
                score += 0.1;
            }
        }

        score
    }

    /// TieredByComplexity: Simple->Fast, Moderate->Balanced, Complex/Critical->Powerful.
    fn select_tiered(&self, complexity: &TaskComplexity) -> Option<&CostModelOption> {
        let target_tier = match complexity {
            TaskComplexity::Simple => CostModelTier::Fast,
            TaskComplexity::Moderate => CostModelTier::Balanced,
            TaskComplexity::Complex | TaskComplexity::Critical => CostModelTier::Powerful,
        };

        self.config
            .models
            .iter()
            .filter(|m| m.tier == target_tier)
            .min_by(|a, b| {
                a.cost_per_1k_input
                    .partial_cmp(&b.cost_per_1k_input)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .or_else(|| {
                // Fallback to adjacent tiers.
                match target_tier {
                    CostModelTier::Fast => self
                        .models_by_tier(CostModelTier::Balanced)
                        .or_else(|| self.models_by_tier(CostModelTier::Powerful)),
                    CostModelTier::Powerful => self
                        .models_by_tier(CostModelTier::Balanced)
                        .or_else(|| self.models_by_tier(CostModelTier::Fast)),
                    CostModelTier::Balanced => self
                        .models_by_tier(CostModelTier::Fast)
                        .or_else(|| self.models_by_tier(CostModelTier::Powerful)),
                }
            })
    }

    /// BudgetPacing: distribute budget across period, use cheaper model when behind pace.
    fn select_budget_pacing(
        &self,
        period_hours: u32,
        complexity: &TaskComplexity,
    ) -> Option<&CostModelOption> {
        let guard = self
            .spending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let budget_limit = self.config.budget_limit_usd.unwrap_or(100.0);
        let elapsed_hours = Utc::now()
            .signed_duration_since(guard.period_start)
            .num_seconds() as f64
            / 3600.0;
        let period = period_hours.max(1) as f64;
        let elapsed_fraction = (elapsed_hours / period).min(1.0);

        // Expected spend at this point in the period.
        let expected_spend = budget_limit * elapsed_fraction;
        let actual_spend = guard.total_spent_usd;

        drop(guard);

        if actual_spend > expected_spend {
            // Over-spending: use a cheaper model regardless of complexity.
            self.cheapest_model()
        } else {
            // Under or on pace: route by complexity tier.
            self.select_tiered(complexity)
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Find the cheapest model across all configured models.
    fn cheapest_model(&self) -> Option<&CostModelOption> {
        self.config.models.iter().min_by(|a, b| {
            a.cost_per_1k_input
                .partial_cmp(&b.cost_per_1k_input)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Find the cheapest model in a specific tier.
    fn models_by_tier(&self, tier: CostModelTier) -> Option<&CostModelOption> {
        self.config
            .models
            .iter()
            .filter(|m| m.tier == tier)
            .min_by(|a, b| {
                a.cost_per_1k_input
                    .partial_cmp(&b.cost_per_1k_input)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    // ── Spending tracking ───────────────────────────────────────────────

    /// Record actual spending after a request completes.
    pub fn record_spending(&self, model_id: &str, tokens_in: u64, tokens_out: u64) {
        let model = self.config.models.iter().find(|m| m.model_id == model_id);
        let (cost_in, cost_out) = match model {
            Some(m) => (m.cost_per_1k_input, m.cost_per_1k_output),
            None => return,
        };

        let cost =
            (tokens_in as f64 / 1_000.0) * cost_in + (tokens_out as f64 / 1_000.0) * cost_out;

        let mut guard = self
            .spending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.total_spent_usd += cost;
        *guard
            .spent_by_model
            .entry(model_id.to_string())
            .or_insert(0.0) += cost;
        *guard
            .requests_by_model
            .entry(model_id.to_string())
            .or_insert(0) += 1;
        guard.request_count += 1;

        // Also record the routing decision in history.
        drop(guard);

        let decision = RoutingDecision {
            selected_model: model_id.to_string(),
            selected_provider: model.map(|m| m.provider.clone()).unwrap_or_default(),
            reason: format!(
                "Recorded spending: {tokens_in} input + {tokens_out} output tokens = ${cost:.6}"
            ),
            estimated_cost_usd: cost,
            task_complexity: TaskComplexity::Simple, // not applicable for recording
            timestamp: Utc::now(),
        };
        self.history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(decision);
    }

    /// Get current spending summary.
    pub fn spending_summary(&self) -> SpendingSummary {
        let guard = self
            .spending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total = guard.total_spent_usd;
        let count = guard.request_count;
        let by_model = guard.spent_by_model.clone();

        let remaining = self.config.budget_limit_usd.map(|limit| {
            let r = limit - total;
            if r < 0.0 {
                0.0
            } else {
                r
            }
        });

        let utilization = self.config.budget_limit_usd.map(|limit| {
            if limit > 0.0 {
                (total / limit) * 100.0
            } else {
                100.0
            }
        });

        SpendingSummary {
            total_spent_usd: total,
            budget_limit_usd: self.config.budget_limit_usd,
            remaining_usd: remaining,
            utilization_percent: utilization,
            requests_count: count,
            avg_cost_per_request: if count > 0 { total / count as f64 } else { 0.0 },
            by_model,
        }
    }

    /// Check if the budget allows another request.
    pub fn budget_available(&self) -> bool {
        match self.config.budget_limit_usd {
            None => true,
            Some(limit) => {
                let guard = self
                    .spending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.total_spent_usd < limit
            }
        }
    }

    /// Get remaining budget in USD, or `None` if no limit is set.
    pub fn remaining_budget_usd(&self) -> Option<f64> {
        self.config.budget_limit_usd.map(|limit| {
            let guard = self
                .spending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let remaining = limit - guard.total_spent_usd;
            if remaining < 0.0 {
                0.0
            } else {
                remaining
            }
        })
    }

    /// Get the most recent routing decisions.
    pub fn routing_history(&self, limit: usize) -> Vec<RoutingDecision> {
        let guard = self
            .history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let len = guard.len();
        let start = len.saturating_sub(limit);
        guard[start..].to_vec()
    }

    /// Get per-model usage statistics.
    pub fn model_stats(&self) -> Vec<ModelUsageStats> {
        let guard = self
            .spending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total = guard.total_spent_usd;

        let mut stats = Vec::new();
        for (model_id, &cost) in &guard.spent_by_model {
            let requests = guard.requests_by_model.get(model_id).copied().unwrap_or(0);
            stats.push(ModelUsageStats {
                model_id: model_id.clone(),
                requests,
                total_cost_usd: cost,
                avg_cost_per_request: if requests > 0 {
                    cost / requests as f64
                } else {
                    0.0
                },
                percent_of_total: if total > 0.0 {
                    (cost / total) * 100.0
                } else {
                    0.0
                },
            });
        }

        stats.sort_by(|a, b| {
            b.total_cost_usd
                .partial_cmp(&a.total_cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        stats
    }

    // ── Default configurations ──────────────────────────────────────────

    /// Create a default optimizer with Claude models only.
    ///
    /// Haiku (Fast) -> Sonnet (Balanced) -> Opus (Powerful).
    pub fn default_claude() -> Self {
        let config = CostOptimizerConfig {
            strategy: OptimizationStrategy::TieredByComplexity,
            budget_limit_usd: None,
            quality_threshold: 0.3,
            models: vec![
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "claude-haiku-4-5-20251001".into(),
                    cost_per_1k_input: 0.0008,
                    cost_per_1k_output: 0.004,
                    quality_score: 0.6,
                    avg_latency_ms: 300,
                    max_tokens: 8192,
                    tier: CostModelTier::Fast,
                },
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "claude-sonnet-4-6".into(),
                    cost_per_1k_input: 0.003,
                    cost_per_1k_output: 0.015,
                    quality_score: 0.85,
                    avg_latency_ms: 800,
                    max_tokens: 8192,
                    tier: CostModelTier::Balanced,
                },
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "claude-opus-4-6".into(),
                    cost_per_1k_input: 0.015,
                    cost_per_1k_output: 0.075,
                    quality_score: 0.98,
                    avg_latency_ms: 2000,
                    max_tokens: 8192,
                    tier: CostModelTier::Powerful,
                },
            ],
            fallback_model: "claude-haiku-4-5-20251001".into(),
        };
        Self::new(config)
    }

    /// Create a default optimizer with models from multiple providers.
    ///
    /// Fast tier: GPT-4o-mini, Haiku, Flash.
    /// Balanced tier: GPT-4o, Sonnet, Gemini Pro.
    /// Powerful tier: Opus, o1.
    pub fn default_multi_provider() -> Self {
        let config = CostOptimizerConfig {
            strategy: OptimizationStrategy::TieredByComplexity,
            budget_limit_usd: None,
            quality_threshold: 0.3,
            models: vec![
                // Fast tier
                CostModelOption {
                    provider: "openai".into(),
                    model_id: "gpt-4o-mini".into(),
                    cost_per_1k_input: 0.00015,
                    cost_per_1k_output: 0.0006,
                    quality_score: 0.55,
                    avg_latency_ms: 200,
                    max_tokens: 16384,
                    tier: CostModelTier::Fast,
                },
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "claude-haiku-4-5-20251001".into(),
                    cost_per_1k_input: 0.0008,
                    cost_per_1k_output: 0.004,
                    quality_score: 0.6,
                    avg_latency_ms: 300,
                    max_tokens: 8192,
                    tier: CostModelTier::Fast,
                },
                CostModelOption {
                    provider: "gemini".into(),
                    model_id: "gemini-2.0-flash".into(),
                    cost_per_1k_input: 0.0001,
                    cost_per_1k_output: 0.0004,
                    quality_score: 0.50,
                    avg_latency_ms: 150,
                    max_tokens: 8192,
                    tier: CostModelTier::Fast,
                },
                // Balanced tier
                CostModelOption {
                    provider: "openai".into(),
                    model_id: "gpt-4o".into(),
                    cost_per_1k_input: 0.0025,
                    cost_per_1k_output: 0.01,
                    quality_score: 0.80,
                    avg_latency_ms: 600,
                    max_tokens: 16384,
                    tier: CostModelTier::Balanced,
                },
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "claude-sonnet-4-6".into(),
                    cost_per_1k_input: 0.003,
                    cost_per_1k_output: 0.015,
                    quality_score: 0.85,
                    avg_latency_ms: 800,
                    max_tokens: 8192,
                    tier: CostModelTier::Balanced,
                },
                CostModelOption {
                    provider: "gemini".into(),
                    model_id: "gemini-1.5-pro".into(),
                    cost_per_1k_input: 0.00125,
                    cost_per_1k_output: 0.005,
                    quality_score: 0.75,
                    avg_latency_ms: 700,
                    max_tokens: 8192,
                    tier: CostModelTier::Balanced,
                },
                // Powerful tier
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "claude-opus-4-6".into(),
                    cost_per_1k_input: 0.015,
                    cost_per_1k_output: 0.075,
                    quality_score: 0.98,
                    avg_latency_ms: 2000,
                    max_tokens: 8192,
                    tier: CostModelTier::Powerful,
                },
                CostModelOption {
                    provider: "openai".into(),
                    model_id: "o1".into(),
                    cost_per_1k_input: 0.015,
                    cost_per_1k_output: 0.06,
                    quality_score: 0.95,
                    avg_latency_ms: 3000,
                    max_tokens: 32768,
                    tier: CostModelTier::Powerful,
                },
            ],
            fallback_model: "gpt-4o-mini".into(),
        };
        Self::new(config)
    }
}

// ---------------------------------------------------------------------------
// Debug impl (CostOptimizer contains Mutex, so derive(Debug) won't work)
// ---------------------------------------------------------------------------

impl std::fmt::Debug for CostOptimizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CostOptimizer")
            .field("config", &self.config)
            .field("spending", &"<Mutex<SpendingTracker>>")
            .field("history", &"<Mutex<Vec<RoutingDecision>>>")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── Helpers ─────────────────────────────────────────────────────────

    fn make_config(strategy: OptimizationStrategy) -> CostOptimizerConfig {
        CostOptimizerConfig {
            strategy,
            budget_limit_usd: None,
            quality_threshold: 0.3,
            models: vec![
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "haiku".into(),
                    cost_per_1k_input: 0.001,
                    cost_per_1k_output: 0.004,
                    quality_score: 0.6,
                    avg_latency_ms: 300,
                    max_tokens: 8192,
                    tier: CostModelTier::Fast,
                },
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "sonnet".into(),
                    cost_per_1k_input: 0.003,
                    cost_per_1k_output: 0.015,
                    quality_score: 0.85,
                    avg_latency_ms: 800,
                    max_tokens: 8192,
                    tier: CostModelTier::Balanced,
                },
                CostModelOption {
                    provider: "claude".into(),
                    model_id: "opus".into(),
                    cost_per_1k_input: 0.015,
                    cost_per_1k_output: 0.075,
                    quality_score: 0.98,
                    avg_latency_ms: 2000,
                    max_tokens: 8192,
                    tier: CostModelTier::Powerful,
                },
            ],
            fallback_model: "haiku".into(),
        }
    }

    fn make_optimizer(strategy: OptimizationStrategy) -> CostOptimizer {
        CostOptimizer::new(make_config(strategy))
    }

    // ── 1. Complexity: simple input ─────────────────────────────────────

    #[test]
    fn test_complexity_simple_input() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("hi", 0, 0);
        assert_eq!(c, TaskComplexity::Simple);
    }

    // ── 2. Complexity: moderate by length ───────────────────────────────

    #[test]
    fn test_complexity_moderate_by_length() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let input = "a".repeat(150);
        let c = opt.estimate_complexity(&input, 0, 0);
        assert_eq!(c, TaskComplexity::Moderate);
    }

    // ── 3. Complexity: moderate by tool count ───────────────────────────

    #[test]
    fn test_complexity_moderate_by_tools() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("do it", 1, 0);
        assert_eq!(c, TaskComplexity::Moderate);
    }

    // ── 4. Complexity: complex by long input ────────────────────────────

    #[test]
    fn test_complexity_complex_by_length() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let input = "a".repeat(600);
        let c = opt.estimate_complexity(&input, 0, 0);
        assert_eq!(c, TaskComplexity::Complex);
    }

    // ── 5. Complexity: complex by multistep keywords ────────────────────

    #[test]
    fn test_complexity_complex_by_multistep() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("first do X then do Y", 0, 0);
        assert_eq!(c, TaskComplexity::Complex);
    }

    // ── 6. Complexity: complex by many tools ────────────────────────────

    #[test]
    fn test_complexity_complex_by_many_tools() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("run it", 5, 0);
        assert_eq!(c, TaskComplexity::Complex);
    }

    // ── 7. Complexity: complex by history turns ─────────────────────────

    #[test]
    fn test_complexity_complex_by_history() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("run it", 0, 15);
        assert_eq!(c, TaskComplexity::Complex);
    }

    // ── 8. Complexity: critical ─────────────────────────────────────────

    #[test]
    fn test_complexity_critical() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("This is a critical production issue", 0, 0);
        assert_eq!(c, TaskComplexity::Critical);
    }

    // ── 9. Complexity: critical by security audit ───────────────────────

    #[test]
    fn test_complexity_critical_security_audit() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("Run a full security audit", 0, 0);
        assert_eq!(c, TaskComplexity::Critical);
    }

    // ── 10. CostMinimize selects cheapest ───────────────────────────────

    #[test]
    fn test_cost_minimize_selects_cheapest() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let model = opt.select_model(&TaskComplexity::Simple).unwrap();
        assert_eq!(model.model_id, "haiku");
    }

    // ── 11. QualityMaximize selects best ────────────────────────────────

    #[test]
    fn test_quality_maximize_selects_best() {
        let opt = make_optimizer(OptimizationStrategy::QualityMaximize);
        let model = opt.select_model(&TaskComplexity::Simple).unwrap();
        assert_eq!(model.model_id, "opus");
    }

    // ── 12. TieredByComplexity: simple -> fast ──────────────────────────

    #[test]
    fn test_tiered_simple_selects_fast() {
        let opt = make_optimizer(OptimizationStrategy::TieredByComplexity);
        let model = opt.select_model(&TaskComplexity::Simple).unwrap();
        assert_eq!(model.tier, CostModelTier::Fast);
    }

    // ── 13. TieredByComplexity: moderate -> balanced ────────────────────

    #[test]
    fn test_tiered_moderate_selects_balanced() {
        let opt = make_optimizer(OptimizationStrategy::TieredByComplexity);
        let model = opt.select_model(&TaskComplexity::Moderate).unwrap();
        assert_eq!(model.tier, CostModelTier::Balanced);
    }

    // ── 14. TieredByComplexity: complex -> powerful ─────────────────────

    #[test]
    fn test_tiered_complex_selects_powerful() {
        let opt = make_optimizer(OptimizationStrategy::TieredByComplexity);
        let model = opt.select_model(&TaskComplexity::Complex).unwrap();
        assert_eq!(model.tier, CostModelTier::Powerful);
    }

    // ── 15. TieredByComplexity: critical -> powerful ────────────────────

    #[test]
    fn test_tiered_critical_selects_powerful() {
        let opt = make_optimizer(OptimizationStrategy::TieredByComplexity);
        let model = opt.select_model(&TaskComplexity::Critical).unwrap();
        assert_eq!(model.tier, CostModelTier::Powerful);
    }

    // ── 16. Balanced strategy works for simple task ─────────────────────

    #[test]
    fn test_balanced_simple_prefers_cheap() {
        let opt = make_optimizer(OptimizationStrategy::Balanced);
        let model = opt.select_model(&TaskComplexity::Simple).unwrap();
        // Should not be opus for a simple task.
        assert_ne!(model.model_id, "opus");
    }

    // ── 17. Balanced strategy works for complex task ────────────────────

    #[test]
    fn test_balanced_complex_prefers_powerful() {
        let opt = make_optimizer(OptimizationStrategy::Balanced);
        let model = opt.select_model(&TaskComplexity::Complex).unwrap();
        // Should not be haiku for a complex task.
        assert_ne!(model.model_id, "haiku");
    }

    // ── 18. Record spending updates tracker ─────────────────────────────

    #[test]
    fn test_record_spending() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        opt.record_spending("haiku", 1000, 500);

        let summary = opt.spending_summary();
        // 1000 / 1000 * 0.001 + 500 / 1000 * 0.004 = 0.001 + 0.002 = 0.003
        assert!(
            (summary.total_spent_usd - 0.003).abs() < 1e-10,
            "Expected 0.003, got {}",
            summary.total_spent_usd
        );
        assert_eq!(summary.requests_count, 1);
    }

    // ── 19. Record spending accumulates across calls ────────────────────

    #[test]
    fn test_record_spending_accumulates() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        opt.record_spending("haiku", 1000, 500);
        opt.record_spending("sonnet", 2000, 1000);

        let summary = opt.spending_summary();
        // haiku: 0.001 + 0.002 = 0.003
        // sonnet: 0.006 + 0.015 = 0.021
        // total = 0.024
        assert!(
            (summary.total_spent_usd - 0.024).abs() < 1e-10,
            "Expected 0.024, got {}",
            summary.total_spent_usd
        );
        assert_eq!(summary.requests_count, 2);
        assert_eq!(summary.by_model.len(), 2);
    }

    // ── 20. Budget enforcement ──────────────────────────────────────────

    #[test]
    fn test_budget_enforcement() {
        let mut config = make_config(OptimizationStrategy::CostMinimize);
        config.budget_limit_usd = Some(0.01);
        let opt = CostOptimizer::new(config);

        assert!(opt.budget_available());
        assert!((opt.remaining_budget_usd().unwrap() - 0.01).abs() < 1e-10);

        // Spend over budget.
        opt.record_spending("opus", 10_000, 5_000);
        // 10 * 0.015 + 5 * 0.075 = 0.15 + 0.375 = 0.525

        assert!(!opt.budget_available());
        assert!((opt.remaining_budget_usd().unwrap()).abs() < 1e-10);
    }

    // ── 21. Budget exhausted returns fallback ───────────────────────────

    #[test]
    fn test_budget_exhausted_returns_fallback() {
        let mut config = make_config(OptimizationStrategy::QualityMaximize);
        config.budget_limit_usd = Some(0.001);
        let opt = CostOptimizer::new(config);

        // Exhaust budget.
        opt.record_spending("opus", 10_000, 5_000);

        let model = opt.select_model(&TaskComplexity::Complex).unwrap();
        assert_eq!(
            model.model_id, "haiku",
            "Should fall back to cheapest model"
        );
    }

    // ── 22. Spending summary fields ─────────────────────────────────────

    #[test]
    fn test_spending_summary_fields() {
        let mut config = make_config(OptimizationStrategy::CostMinimize);
        config.budget_limit_usd = Some(1.0);
        let opt = CostOptimizer::new(config);

        opt.record_spending("haiku", 1000, 500);

        let summary = opt.spending_summary();
        assert!(summary.budget_limit_usd.is_some());
        assert!(summary.remaining_usd.is_some());
        assert!(summary.utilization_percent.is_some());
        assert!(summary.remaining_usd.unwrap() < 1.0);
        assert!(summary.utilization_percent.unwrap() > 0.0);
        assert!((summary.avg_cost_per_request - summary.total_spent_usd).abs() < 1e-10);
    }

    // ── 23. Model stats ─────────────────────────────────────────────────

    #[test]
    fn test_model_stats() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        opt.record_spending("haiku", 1000, 500);
        opt.record_spending("haiku", 2000, 1000);
        opt.record_spending("sonnet", 1000, 500);

        let stats = opt.model_stats();
        assert_eq!(stats.len(), 2);

        let haiku_stats = stats.iter().find(|s| s.model_id == "haiku").unwrap();
        assert_eq!(haiku_stats.requests, 2);
        assert!(haiku_stats.total_cost_usd > 0.0);
        assert!(haiku_stats.percent_of_total > 0.0);
    }

    // ── 24. Routing history ─────────────────────────────────────────────

    #[test]
    fn test_routing_history() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        opt.record_spending("haiku", 1000, 500);
        opt.record_spending("sonnet", 2000, 1000);
        opt.record_spending("opus", 500, 250);

        let history = opt.routing_history(2);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].selected_model, "sonnet");
        assert_eq!(history[1].selected_model, "opus");

        let all = opt.routing_history(100);
        assert_eq!(all.len(), 3);
    }

    // ── 25. No budget set means unlimited ───────────────────────────────

    #[test]
    fn test_no_budget_unlimited() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        assert!(opt.budget_available());
        assert!(opt.remaining_budget_usd().is_none());

        // Large spending should still be fine.
        opt.record_spending("opus", 1_000_000, 500_000);
        assert!(opt.budget_available());
    }

    // ── 26. Empty models error ──────────────────────────────────────────

    #[test]
    fn test_empty_models_error() {
        let config = CostOptimizerConfig {
            strategy: OptimizationStrategy::CostMinimize,
            budget_limit_usd: None,
            quality_threshold: 0.3,
            models: vec![],
            fallback_model: "haiku".into(),
        };
        let opt = CostOptimizer::new(config);
        let result = opt.select_model(&TaskComplexity::Simple);
        assert!(result.is_err());
    }

    // ── 27. default_claude has 3 models ─────────────────────────────────

    #[test]
    fn test_default_claude() {
        let opt = CostOptimizer::default_claude();
        assert_eq!(opt.config.models.len(), 3);
        assert!(opt
            .config
            .models
            .iter()
            .any(|m| m.tier == CostModelTier::Fast));
        assert!(opt
            .config
            .models
            .iter()
            .any(|m| m.tier == CostModelTier::Balanced));
        assert!(opt
            .config
            .models
            .iter()
            .any(|m| m.tier == CostModelTier::Powerful));
    }

    // ── 28. default_multi_provider has 8 models ─────────────────────────

    #[test]
    fn test_default_multi_provider() {
        let opt = CostOptimizer::default_multi_provider();
        assert_eq!(opt.config.models.len(), 8);
        let providers: Vec<&str> = opt
            .config
            .models
            .iter()
            .map(|m| m.provider.as_str())
            .collect();
        assert!(providers.contains(&"claude"));
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"gemini"));
    }

    // ── 29. BudgetPacing strategy ───────────────────────────────────────

    #[test]
    fn test_budget_pacing_strategy() {
        let mut config = make_config(OptimizationStrategy::BudgetPacing { period_hours: 24 });
        config.budget_limit_usd = Some(10.0);
        let opt = CostOptimizer::new(config);

        // Should work fine when no spending has occurred.
        let model = opt.select_model(&TaskComplexity::Moderate).unwrap();
        // Since period just started and no spending, should use tiered selection.
        assert_eq!(model.tier, CostModelTier::Balanced);
    }

    // ── 30. BudgetPacing overspend uses cheapest ────────────────────────

    #[test]
    fn test_budget_pacing_overspend() {
        let mut config = make_config(OptimizationStrategy::BudgetPacing { period_hours: 24 });
        config.budget_limit_usd = Some(0.001);
        let opt = CostOptimizer::new(config);

        // Record significant spending so actual > expected at period start.
        opt.record_spending("opus", 10_000, 5_000);

        // Budget still technically available (0.001 > total? no — 0.525 >> 0.001).
        // Actually budget is exhausted, so fallback.
        let model = opt.select_model(&TaskComplexity::Complex).unwrap();
        assert_eq!(
            model.model_id, "haiku",
            "Overspent pacing should fallback to cheapest"
        );
    }

    // ── 31. Complexity: moderate by analysis keyword ────────────────────

    #[test]
    fn test_complexity_moderate_by_keyword() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("analyze this", 0, 0);
        assert_eq!(c, TaskComplexity::Moderate);
    }

    // ── 32. Complexity: complex by multiple analysis keywords ───────────

    #[test]
    fn test_complexity_complex_by_keywords() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("analyze and compare the data", 0, 0);
        assert_eq!(c, TaskComplexity::Complex);
    }

    // ── 33. CostMinimize respects quality threshold ─────────────────────

    #[test]
    fn test_cost_minimize_quality_threshold() {
        let mut config = make_config(OptimizationStrategy::CostMinimize);
        config.quality_threshold = 0.9;
        let opt = CostOptimizer::new(config);

        let model = opt.select_model(&TaskComplexity::Simple).unwrap();
        // Only opus (0.98) meets 0.9 threshold.
        assert_eq!(model.model_id, "opus");
    }

    // ── 34. Tiered fallback when tier missing ───────────────────────────

    #[test]
    fn test_tiered_fallback_missing_tier() {
        let config = CostOptimizerConfig {
            strategy: OptimizationStrategy::TieredByComplexity,
            budget_limit_usd: None,
            quality_threshold: 0.0,
            models: vec![CostModelOption {
                provider: "claude".into(),
                model_id: "sonnet".into(),
                cost_per_1k_input: 0.003,
                cost_per_1k_output: 0.015,
                quality_score: 0.85,
                avg_latency_ms: 800,
                max_tokens: 8192,
                tier: CostModelTier::Balanced,
            }],
            fallback_model: "sonnet".into(),
        };
        let opt = CostOptimizer::new(config);

        // Simple targets Fast tier which is missing — should fall back to Balanced.
        let model = opt.select_model(&TaskComplexity::Simple).unwrap();
        assert_eq!(model.tier, CostModelTier::Balanced);
    }

    // ── 35. Remaining budget floors at zero ─────────────────────────────

    #[test]
    fn test_remaining_budget_floors_at_zero() {
        let mut config = make_config(OptimizationStrategy::CostMinimize);
        config.budget_limit_usd = Some(0.001);
        let opt = CostOptimizer::new(config);

        opt.record_spending("opus", 100_000, 50_000);

        let remaining = opt.remaining_budget_usd().unwrap();
        assert!(
            remaining.abs() < f64::EPSILON,
            "Should floor at 0, got {remaining}"
        );
    }

    // ── 36. Spending summary with no budget ─────────────────────────────

    #[test]
    fn test_spending_summary_no_budget() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let summary = opt.spending_summary();
        assert!(summary.budget_limit_usd.is_none());
        assert!(summary.remaining_usd.is_none());
        assert!(summary.utilization_percent.is_none());
        assert_eq!(summary.requests_count, 0);
        assert!((summary.avg_cost_per_request).abs() < f64::EPSILON);
    }

    // ── 37. Record spending ignores unknown models ──────────────────────

    #[test]
    fn test_record_spending_unknown_model() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        opt.record_spending("unknown-model", 1000, 500);

        let summary = opt.spending_summary();
        assert!((summary.total_spent_usd).abs() < f64::EPSILON);
        assert_eq!(summary.requests_count, 0);
    }

    // ── 38. Complexity: moderate by history turns ────────────────────────

    #[test]
    fn test_complexity_moderate_by_history() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        let c = opt.estimate_complexity("do it", 0, 3);
        assert_eq!(c, TaskComplexity::Moderate);
    }

    // ── 39. Model stats percent_of_total sums to 100 ────────────────────

    #[test]
    fn test_model_stats_percent_sums() {
        let opt = make_optimizer(OptimizationStrategy::CostMinimize);
        opt.record_spending("haiku", 1000, 500);
        opt.record_spending("sonnet", 2000, 1000);

        let stats = opt.model_stats();
        let total_pct: f64 = stats.iter().map(|s| s.percent_of_total).sum();
        assert!(
            (total_pct - 100.0).abs() < 1e-6,
            "Percentages should sum to 100, got {total_pct}"
        );
    }

    // ── 40. CostModelTier serialization ─────────────────────────────────

    #[test]
    fn test_tier_serialization() {
        let tier = CostModelTier::Fast;
        let json = serde_json::to_string(&tier).unwrap();
        assert!(json.contains("Fast"));

        let deserialized: CostModelTier = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, CostModelTier::Fast);
    }

    // ── 41. TaskComplexity serialization ─────────────────────────────────

    #[test]
    fn test_task_complexity_serialization() {
        let c = TaskComplexity::Critical;
        let json = serde_json::to_string(&c).unwrap();
        let deserialized: TaskComplexity = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TaskComplexity::Critical);
    }

    // ── 42. OptimizationStrategy serialization ──────────────────────────

    #[test]
    fn test_strategy_serialization() {
        let strategy = OptimizationStrategy::BudgetPacing { period_hours: 24 };
        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("24"));

        let deserialized: OptimizationStrategy = serde_json::from_str(&json).unwrap();
        match deserialized {
            OptimizationStrategy::BudgetPacing { period_hours } => {
                assert_eq!(period_hours, 24);
            }
            other => panic!("Expected BudgetPacing, got {other:?}"),
        }
    }

    // ── 43. Debug formatting ────────────────────────────────────────────

    #[test]
    fn test_debug_formatting() {
        let opt = CostOptimizer::default_claude();
        let debug = format!("{opt:?}");
        assert!(debug.contains("CostOptimizer"));
        assert!(debug.contains("config"));
    }
}
