//! Advanced multi-agent collaboration patterns.
//!
//! Goes beyond the basic Orchestrator-Workers pattern to support sophisticated
//! multi-agent workflows: pipelines, map-reduce, adversarial debate, ensemble
//! voting, supervised review, and swarm convergence.

use crate::types::AgentRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Core pattern enum
// ---------------------------------------------------------------------------

/// A collaboration pattern describing how multiple agents interact to solve a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollaborationPattern {
    /// Sequential processing chain — each stage's output feeds the next stage's input,
    /// like Unix pipes: Agent A output -> Agent B input -> Agent C input.
    Pipeline { stages: Vec<PipelineStage> },

    /// Split work across N mapper agents, then reduce (aggregate) the partial results
    /// with a dedicated reducer agent.
    MapReduce {
        mapper_role: AgentRole,
        reducer_role: AgentRole,
        chunk_count: usize,
    },

    /// Two agents argue opposing positions while a judge evaluates and decides,
    /// improving decision quality through adversarial deliberation.
    Debate {
        proponent: AgentRole,
        opponent: AgentRole,
        judge: AgentRole,
        max_rounds: u32,
    },

    /// Multiple agents tackle the same task independently and results are aggregated
    /// via a configurable strategy (voting, best-of-N, concatenation, LLM synthesis).
    Ensemble {
        agents: Vec<AgentRole>,
        aggregation: AggregationStrategy,
    },

    /// A supervisor agent reviews worker outputs and can accept or reject them
    /// according to a configurable review policy.
    Supervisor {
        supervisor: AgentRole,
        workers: Vec<AgentRole>,
        review_policy: ReviewPolicy,
    },

    /// Agents iterate collaboratively until consensus or a convergence threshold is met.
    Swarm {
        roles: Vec<AgentRole>,
        max_iterations: u32,
        convergence_threshold: f64,
    },
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// A single stage in a [`CollaborationPattern::Pipeline`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    /// The agent role responsible for this stage.
    pub role: AgentRole,
    /// Human-readable description of what this stage does.
    pub description: String,
    /// Optional transformation hint applied to the output before passing it downstream.
    pub transform: Option<String>,
}

/// Strategy used by [`CollaborationPattern::Ensemble`] to aggregate results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum AggregationStrategy {
    /// Pick the result that appears most often among agents.
    MajorityVote,
    /// Pick the single best result according to a named metric.
    BestOfN { metric: String },
    /// Concatenate all results in order.
    Concatenate,
    /// Use an LLM to synthesize a unified answer from all results.
    LlmSynthesize,
}

/// Policy for when a supervisor reviews worker output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum ReviewPolicy {
    /// Supervisor reviews every piece of output.
    AlwaysReview,
    /// Supervisor randomly reviews a percentage of outputs.
    SamplePercent(f64),
    /// Supervisor only reviews outputs flagged as errors.
    OnError,
    /// No review — workers are fully trusted.
    Never,
}

/// Top-level configuration wrapping a [`CollaborationPattern`] with execution settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfig {
    pub pattern: CollaborationPattern,
    /// Maximum wall-clock seconds the entire pattern execution may take.
    pub timeout_secs: u64,
    /// Maximum number of retries on transient failures.
    pub max_retries: u32,
}

/// Outcome of executing a collaboration pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternResult {
    /// Name of the pattern that was executed (e.g. "pipeline", "debate").
    pub pattern_name: String,
    /// How many stages / rounds were completed.
    pub stages_completed: u32,
    /// Total stages / rounds that were planned.
    pub total_stages: u32,
    /// Identifiers (or summaries) of artifacts produced.
    pub artifacts: Vec<String>,
    /// Whether agents reached consensus (meaningful for Debate / Swarm).
    pub consensus_reached: bool,
    /// The final aggregated output text.
    pub final_output: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Ergonomic builder for [`PatternConfig`].
#[derive(Debug, Clone)]
pub struct PatternConfigBuilder {
    pattern: CollaborationPattern,
    timeout_secs: u64,
    max_retries: u32,
}

impl PatternConfig {
    /// Start building a pipeline pattern (stages will be added via the builder).
    pub fn pipeline() -> PatternConfigBuilder {
        PatternConfigBuilder {
            pattern: CollaborationPattern::Pipeline { stages: Vec::new() },
            timeout_secs: 300,
            max_retries: 1,
        }
    }

    /// Start building a debate pattern with default roles and 3 rounds.
    pub fn debate() -> PatternConfigBuilder {
        PatternConfigBuilder {
            pattern: CollaborationPattern::Debate {
                proponent: AgentRole::Coder,
                opponent: AgentRole::Reviewer,
                judge: AgentRole::Architect,
                max_rounds: 3,
            },
            timeout_secs: 300,
            max_retries: 1,
        }
    }

    /// Start building an ensemble pattern.
    pub fn ensemble() -> PatternConfigBuilder {
        PatternConfigBuilder {
            pattern: CollaborationPattern::Ensemble {
                agents: Vec::new(),
                aggregation: AggregationStrategy::MajorityVote,
            },
            timeout_secs: 300,
            max_retries: 1,
        }
    }

    /// Start building a map-reduce pattern.
    pub fn map_reduce() -> PatternConfigBuilder {
        PatternConfigBuilder {
            pattern: CollaborationPattern::MapReduce {
                mapper_role: AgentRole::Coder,
                reducer_role: AgentRole::Orchestrator,
                chunk_count: 4,
            },
            timeout_secs: 600,
            max_retries: 1,
        }
    }

    /// Start building a supervisor pattern.
    pub fn supervisor() -> PatternConfigBuilder {
        PatternConfigBuilder {
            pattern: CollaborationPattern::Supervisor {
                supervisor: AgentRole::Reviewer,
                workers: Vec::new(),
                review_policy: ReviewPolicy::AlwaysReview,
            },
            timeout_secs: 300,
            max_retries: 1,
        }
    }

    /// Start building a swarm pattern.
    pub fn swarm() -> PatternConfigBuilder {
        PatternConfigBuilder {
            pattern: CollaborationPattern::Swarm {
                roles: Vec::new(),
                max_iterations: 10,
                convergence_threshold: 0.9,
            },
            timeout_secs: 600,
            max_retries: 1,
        }
    }
}

impl PatternConfigBuilder {
    /// Set the execution timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set the maximum number of retries.
    pub fn with_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// Add a stage to a pipeline pattern. No-op if the pattern is not a pipeline.
    pub fn add_stage(mut self, stage: PipelineStage) -> Self {
        if let CollaborationPattern::Pipeline { ref mut stages } = self.pattern {
            stages.push(stage);
        }
        self
    }

    /// Add an agent to an ensemble pattern. No-op if not an ensemble.
    pub fn add_agent(mut self, role: AgentRole) -> Self {
        if let CollaborationPattern::Ensemble { ref mut agents, .. } = self.pattern {
            agents.push(role);
        }
        self
    }

    /// Set the aggregation strategy for an ensemble. No-op if not an ensemble.
    pub fn with_aggregation(mut self, strategy: AggregationStrategy) -> Self {
        if let CollaborationPattern::Ensemble {
            ref mut aggregation,
            ..
        } = self.pattern
        {
            *aggregation = strategy;
        }
        self
    }

    /// Set the roles participating in a debate.
    pub fn with_debate_roles(
        mut self,
        proponent: AgentRole,
        opponent: AgentRole,
        judge: AgentRole,
    ) -> Self {
        if let CollaborationPattern::Debate {
            proponent: ref mut p,
            opponent: ref mut o,
            judge: ref mut j,
            ..
        } = self.pattern
        {
            *p = proponent;
            *o = opponent;
            *j = judge;
        }
        self
    }

    /// Set the maximum number of rounds for a debate.
    pub fn with_max_rounds(mut self, rounds: u32) -> Self {
        if let CollaborationPattern::Debate {
            ref mut max_rounds, ..
        } = self.pattern
        {
            *max_rounds = rounds;
        }
        self
    }

    /// Add a worker to a supervisor pattern.
    pub fn add_worker(mut self, role: AgentRole) -> Self {
        if let CollaborationPattern::Supervisor {
            ref mut workers, ..
        } = self.pattern
        {
            workers.push(role);
        }
        self
    }

    /// Set the review policy for a supervisor pattern.
    pub fn with_review_policy(mut self, policy: ReviewPolicy) -> Self {
        if let CollaborationPattern::Supervisor {
            ref mut review_policy,
            ..
        } = self.pattern
        {
            *review_policy = policy;
        }
        self
    }

    /// Set the mapper and reducer roles for map-reduce.
    pub fn with_mapper_reducer(
        mut self,
        mapper: AgentRole,
        reducer: AgentRole,
        chunks: usize,
    ) -> Self {
        if let CollaborationPattern::MapReduce {
            ref mut mapper_role,
            ref mut reducer_role,
            ref mut chunk_count,
        } = self.pattern
        {
            *mapper_role = mapper;
            *reducer_role = reducer;
            *chunk_count = chunks;
        }
        self
    }

    /// Add a role to a swarm pattern.
    pub fn add_swarm_role(mut self, role: AgentRole) -> Self {
        if let CollaborationPattern::Swarm { ref mut roles, .. } = self.pattern {
            roles.push(role);
        }
        self
    }

    /// Set convergence parameters for a swarm.
    pub fn with_convergence(mut self, max_iterations: u32, threshold: f64) -> Self {
        if let CollaborationPattern::Swarm {
            max_iterations: ref mut mi,
            convergence_threshold: ref mut ct,
            ..
        } = self.pattern
        {
            *mi = max_iterations;
            *ct = threshold;
        }
        self
    }

    /// Consume the builder and produce a [`PatternConfig`].
    pub fn build(self) -> PatternConfig {
        PatternConfig {
            pattern: self.pattern,
            timeout_secs: self.timeout_secs,
            max_retries: self.max_retries,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Returns a human-readable description of a collaboration pattern.
pub fn describe_pattern(pattern: &CollaborationPattern) -> String {
    match pattern {
        CollaborationPattern::Pipeline { stages } => {
            let names: Vec<String> = stages.iter().map(|s| s.role.to_string()).collect();
            format!(
                "Pipeline with {} stages: {}",
                stages.len(),
                names.join(" -> ")
            )
        }
        CollaborationPattern::MapReduce {
            mapper_role,
            reducer_role,
            chunk_count,
        } => {
            format!("MapReduce: {chunk_count} x {mapper_role} mappers -> {reducer_role} reducer")
        }
        CollaborationPattern::Debate {
            proponent,
            opponent,
            judge,
            max_rounds,
        } => {
            format!(
                "Debate: {proponent} vs {opponent}, judged by {judge} (max {max_rounds} rounds)"
            )
        }
        CollaborationPattern::Ensemble {
            agents,
            aggregation,
        } => {
            let names: Vec<String> = agents.iter().map(|a| a.to_string()).collect();
            let strat = match aggregation {
                AggregationStrategy::MajorityVote => "majority vote",
                AggregationStrategy::BestOfN { metric } => {
                    return format!(
                        "Ensemble of {} agents [{}], aggregated by best-of-N (metric: {metric})",
                        agents.len(),
                        names.join(", ")
                    );
                }
                AggregationStrategy::Concatenate => "concatenation",
                AggregationStrategy::LlmSynthesize => "LLM synthesis",
            };
            format!(
                "Ensemble of {} agents [{}], aggregated by {strat}",
                agents.len(),
                names.join(", ")
            )
        }
        CollaborationPattern::Supervisor {
            supervisor,
            workers,
            review_policy,
        } => {
            let worker_names: Vec<String> = workers.iter().map(|w| w.to_string()).collect();
            let policy = match review_policy {
                ReviewPolicy::AlwaysReview => "always review",
                ReviewPolicy::SamplePercent(p) => {
                    return format!(
                        "Supervisor {supervisor} overseeing [{}], reviewing {:.0}% of outputs",
                        worker_names.join(", "),
                        p * 100.0
                    );
                }
                ReviewPolicy::OnError => "review on error",
                ReviewPolicy::Never => "no review",
            };
            format!(
                "Supervisor {supervisor} overseeing [{}], policy: {policy}",
                worker_names.join(", ")
            )
        }
        CollaborationPattern::Swarm {
            roles,
            max_iterations,
            convergence_threshold,
        } => {
            let names: Vec<String> = roles.iter().map(|r| r.to_string()).collect();
            format!(
                "Swarm of {} agents [{}], max {max_iterations} iterations, convergence >= {convergence_threshold}",
                roles.len(),
                names.join(", ")
            )
        }
    }
}

/// Validates that a collaboration pattern has a sensible configuration.
///
/// Returns `Ok(())` if valid, or `Err(reason)` describing the problem.
pub fn validate_pattern(pattern: &CollaborationPattern) -> Result<(), String> {
    match pattern {
        CollaborationPattern::Pipeline { stages } => {
            if stages.is_empty() {
                return Err("Pipeline must have at least one stage".into());
            }
            Ok(())
        }
        CollaborationPattern::MapReduce { chunk_count, .. } => {
            if *chunk_count == 0 {
                return Err("MapReduce chunk_count must be > 0".into());
            }
            Ok(())
        }
        CollaborationPattern::Debate {
            max_rounds,
            proponent,
            opponent,
            ..
        } => {
            if *max_rounds == 0 {
                return Err("Debate must have at least 1 round".into());
            }
            if proponent == opponent {
                return Err("Debate proponent and opponent must be different roles".into());
            }
            Ok(())
        }
        CollaborationPattern::Ensemble { agents, .. } => {
            if agents.len() < 2 {
                return Err("Ensemble requires at least 2 agents".into());
            }
            Ok(())
        }
        CollaborationPattern::Supervisor { workers, .. } => {
            if workers.is_empty() {
                return Err("Supervisor pattern requires at least one worker".into());
            }
            Ok(())
        }
        CollaborationPattern::Swarm {
            roles,
            max_iterations,
            convergence_threshold,
        } => {
            if roles.len() < 2 {
                return Err("Swarm requires at least 2 agents".into());
            }
            if *max_iterations == 0 {
                return Err("Swarm must have at least 1 iteration".into());
            }
            if *convergence_threshold <= 0.0 || *convergence_threshold > 1.0 {
                return Err("Swarm convergence_threshold must be in (0.0, 1.0]".into());
            }
            Ok(())
        }
    }
}

/// Estimates the total token usage for a pattern given a per-agent token budget.
///
/// This is a rough heuristic:
/// - Pipeline: `stages * tokens_per_agent`
/// - MapReduce: `(chunk_count + 1) * tokens_per_agent`
/// - Debate: `(2 * max_rounds + 1) * tokens_per_agent`  (proponent + opponent per round, plus judge)
/// - Ensemble: `agents.len() * tokens_per_agent`  (plus aggregation cost if LLM)
/// - Supervisor: `(workers.len() + review_overhead) * tokens_per_agent`
/// - Swarm: `roles.len() * max_iterations * tokens_per_agent`
pub fn estimate_cost(pattern: &CollaborationPattern, tokens_per_agent: u64) -> u64 {
    match pattern {
        CollaborationPattern::Pipeline { stages } => stages.len() as u64 * tokens_per_agent,
        CollaborationPattern::MapReduce { chunk_count, .. } => {
            (*chunk_count as u64 + 1) * tokens_per_agent
        }
        CollaborationPattern::Debate { max_rounds, .. } => {
            // Each round: proponent + opponent; final: judge
            (2 * *max_rounds as u64 + 1) * tokens_per_agent
        }
        CollaborationPattern::Ensemble {
            agents,
            aggregation,
        } => {
            let base = agents.len() as u64 * tokens_per_agent;
            match aggregation {
                AggregationStrategy::LlmSynthesize => base + tokens_per_agent,
                _ => base,
            }
        }
        CollaborationPattern::Supervisor {
            workers,
            review_policy,
            ..
        } => {
            let worker_cost = workers.len() as u64 * tokens_per_agent;
            let review_cost = match review_policy {
                ReviewPolicy::AlwaysReview => workers.len() as u64 * tokens_per_agent,
                ReviewPolicy::SamplePercent(p) => {
                    (workers.len() as f64 * p * tokens_per_agent as f64) as u64
                }
                ReviewPolicy::OnError => tokens_per_agent, // assume ~1 review
                ReviewPolicy::Never => 0,
            };
            worker_cost + review_cost
        }
        CollaborationPattern::Swarm {
            roles,
            max_iterations,
            ..
        } => roles.len() as u64 * *max_iterations as u64 * tokens_per_agent,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- Pipeline --

    #[test]
    fn test_pipeline_describe() {
        let pattern = CollaborationPattern::Pipeline {
            stages: vec![
                PipelineStage {
                    role: AgentRole::Spec,
                    description: "Generate spec".into(),
                    transform: None,
                },
                PipelineStage {
                    role: AgentRole::Coder,
                    description: "Implement".into(),
                    transform: Some("extract_code".into()),
                },
            ],
        };
        let desc = describe_pattern(&pattern);
        assert!(desc.contains("Pipeline"));
        assert!(desc.contains("2 stages"));
        assert!(desc.contains("spec -> coder"));
    }

    #[test]
    fn test_pipeline_validate_empty() {
        let pattern = CollaborationPattern::Pipeline { stages: Vec::new() };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_pipeline_validate_ok() {
        let pattern = CollaborationPattern::Pipeline {
            stages: vec![PipelineStage {
                role: AgentRole::Coder,
                description: "Code it".into(),
                transform: None,
            }],
        };
        assert!(validate_pattern(&pattern).is_ok());
    }

    #[test]
    fn test_pipeline_estimate_cost() {
        let pattern = CollaborationPattern::Pipeline {
            stages: vec![
                PipelineStage {
                    role: AgentRole::Spec,
                    description: "Spec".into(),
                    transform: None,
                },
                PipelineStage {
                    role: AgentRole::Coder,
                    description: "Code".into(),
                    transform: None,
                },
                PipelineStage {
                    role: AgentRole::Tester,
                    description: "Test".into(),
                    transform: None,
                },
            ],
        };
        assert_eq!(estimate_cost(&pattern, 1000), 3000);
    }

    // -- MapReduce --

    #[test]
    fn test_map_reduce_validate_zero_chunks() {
        let pattern = CollaborationPattern::MapReduce {
            mapper_role: AgentRole::Coder,
            reducer_role: AgentRole::Orchestrator,
            chunk_count: 0,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_map_reduce_estimate() {
        let pattern = CollaborationPattern::MapReduce {
            mapper_role: AgentRole::Coder,
            reducer_role: AgentRole::Orchestrator,
            chunk_count: 4,
        };
        // 4 mappers + 1 reducer = 5
        assert_eq!(estimate_cost(&pattern, 500), 2500);
    }

    // -- Debate --

    #[test]
    fn test_debate_validate_same_roles() {
        let pattern = CollaborationPattern::Debate {
            proponent: AgentRole::Coder,
            opponent: AgentRole::Coder,
            judge: AgentRole::Architect,
            max_rounds: 3,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_debate_validate_zero_rounds() {
        let pattern = CollaborationPattern::Debate {
            proponent: AgentRole::Coder,
            opponent: AgentRole::Reviewer,
            judge: AgentRole::Architect,
            max_rounds: 0,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_debate_estimate() {
        let pattern = CollaborationPattern::Debate {
            proponent: AgentRole::Coder,
            opponent: AgentRole::Reviewer,
            judge: AgentRole::Architect,
            max_rounds: 3,
        };
        // 2 * 3 + 1 = 7
        assert_eq!(estimate_cost(&pattern, 1000), 7000);
    }

    // -- Ensemble --

    #[test]
    fn test_ensemble_validate_too_few() {
        let pattern = CollaborationPattern::Ensemble {
            agents: vec![AgentRole::Coder],
            aggregation: AggregationStrategy::MajorityVote,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_ensemble_llm_synthesize_extra_cost() {
        let pattern = CollaborationPattern::Ensemble {
            agents: vec![AgentRole::Coder, AgentRole::Reviewer, AgentRole::Architect],
            aggregation: AggregationStrategy::LlmSynthesize,
        };
        // 3 agents + 1 synthesis = 4
        assert_eq!(estimate_cost(&pattern, 1000), 4000);
    }

    // -- Supervisor --

    #[test]
    fn test_supervisor_validate_no_workers() {
        let pattern = CollaborationPattern::Supervisor {
            supervisor: AgentRole::Reviewer,
            workers: Vec::new(),
            review_policy: ReviewPolicy::AlwaysReview,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_supervisor_estimate_always_review() {
        let pattern = CollaborationPattern::Supervisor {
            supervisor: AgentRole::Reviewer,
            workers: vec![AgentRole::Coder, AgentRole::Tester],
            review_policy: ReviewPolicy::AlwaysReview,
        };
        // workers: 2 * 1000 = 2000, reviews: 2 * 1000 = 2000 => 4000
        assert_eq!(estimate_cost(&pattern, 1000), 4000);
    }

    // -- Swarm --

    #[test]
    fn test_swarm_validate_threshold_out_of_range() {
        let pattern = CollaborationPattern::Swarm {
            roles: vec![AgentRole::Coder, AgentRole::Reviewer],
            max_iterations: 5,
            convergence_threshold: 1.5,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_swarm_validate_zero_threshold() {
        let pattern = CollaborationPattern::Swarm {
            roles: vec![AgentRole::Coder, AgentRole::Reviewer],
            max_iterations: 5,
            convergence_threshold: 0.0,
        };
        assert!(validate_pattern(&pattern).is_err());
    }

    #[test]
    fn test_swarm_estimate() {
        let pattern = CollaborationPattern::Swarm {
            roles: vec![AgentRole::Coder, AgentRole::Reviewer, AgentRole::Tester],
            max_iterations: 10,
            convergence_threshold: 0.9,
        };
        // 3 roles * 10 iterations * 500 = 15000
        assert_eq!(estimate_cost(&pattern, 500), 15000);
    }

    // -- Builder --

    #[test]
    fn test_builder_pipeline() {
        let config = PatternConfig::pipeline()
            .add_stage(PipelineStage {
                role: AgentRole::Spec,
                description: "Requirements".into(),
                transform: None,
            })
            .add_stage(PipelineStage {
                role: AgentRole::Coder,
                description: "Implementation".into(),
                transform: Some("extract_rust".into()),
            })
            .with_timeout(120)
            .with_retries(3)
            .build();

        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.max_retries, 3);
        if let CollaborationPattern::Pipeline { stages } = &config.pattern {
            assert_eq!(stages.len(), 2);
            assert_eq!(stages[0].role, AgentRole::Spec);
            assert_eq!(stages[1].transform.as_deref(), Some("extract_rust"));
        } else {
            panic!("Expected Pipeline pattern");
        }
    }

    #[test]
    fn test_builder_debate() {
        let config = PatternConfig::debate()
            .with_debate_roles(
                AgentRole::Architect,
                AgentRole::SecurityAuditor,
                AgentRole::Reviewer,
            )
            .with_max_rounds(5)
            .with_timeout(600)
            .build();

        if let CollaborationPattern::Debate {
            proponent,
            opponent,
            judge,
            max_rounds,
        } = &config.pattern
        {
            assert_eq!(*proponent, AgentRole::Architect);
            assert_eq!(*opponent, AgentRole::SecurityAuditor);
            assert_eq!(*judge, AgentRole::Reviewer);
            assert_eq!(*max_rounds, 5);
        } else {
            panic!("Expected Debate pattern");
        }
    }

    #[test]
    fn test_builder_ensemble() {
        let config = PatternConfig::ensemble()
            .add_agent(AgentRole::Coder)
            .add_agent(AgentRole::Reviewer)
            .add_agent(AgentRole::Architect)
            .with_aggregation(AggregationStrategy::BestOfN {
                metric: "accuracy".into(),
            })
            .build();

        if let CollaborationPattern::Ensemble {
            agents,
            aggregation,
        } = &config.pattern
        {
            assert_eq!(agents.len(), 3);
            if let AggregationStrategy::BestOfN { metric } = aggregation {
                assert_eq!(metric, "accuracy");
            } else {
                panic!("Expected BestOfN strategy");
            }
        } else {
            panic!("Expected Ensemble pattern");
        }
    }

    // -- Serialization --

    #[test]
    fn test_pattern_config_roundtrip() {
        let config = PatternConfig::debate()
            .with_debate_roles(AgentRole::Coder, AgentRole::Reviewer, AgentRole::Architect)
            .with_max_rounds(3)
            .with_timeout(180)
            .with_retries(2)
            .build();

        let json = serde_json::to_string(&config).unwrap();
        let parsed: PatternConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.timeout_secs, 180);
        assert_eq!(parsed.max_retries, 2);
        if let CollaborationPattern::Debate { max_rounds, .. } = &parsed.pattern {
            assert_eq!(*max_rounds, 3);
        } else {
            panic!("Expected Debate after deserialization");
        }
    }

    #[test]
    fn test_pattern_result_construction() {
        let result = PatternResult {
            pattern_name: "debate".into(),
            stages_completed: 6,
            total_stages: 7,
            artifacts: vec!["spec.md".into(), "review.md".into()],
            consensus_reached: true,
            final_output: "Use approach A".into(),
        };
        assert!(result.consensus_reached);
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.stages_completed, 6);
    }

    #[test]
    fn test_describe_all_patterns() {
        // Verify describe_pattern does not panic for any variant.
        let patterns = vec![
            CollaborationPattern::Pipeline {
                stages: vec![PipelineStage {
                    role: AgentRole::Coder,
                    description: "code".into(),
                    transform: None,
                }],
            },
            CollaborationPattern::MapReduce {
                mapper_role: AgentRole::Coder,
                reducer_role: AgentRole::Orchestrator,
                chunk_count: 3,
            },
            CollaborationPattern::Debate {
                proponent: AgentRole::Coder,
                opponent: AgentRole::Reviewer,
                judge: AgentRole::Architect,
                max_rounds: 2,
            },
            CollaborationPattern::Ensemble {
                agents: vec![AgentRole::Coder, AgentRole::Reviewer],
                aggregation: AggregationStrategy::Concatenate,
            },
            CollaborationPattern::Supervisor {
                supervisor: AgentRole::Reviewer,
                workers: vec![AgentRole::Coder],
                review_policy: ReviewPolicy::SamplePercent(0.5),
            },
            CollaborationPattern::Swarm {
                roles: vec![AgentRole::Coder, AgentRole::Tester],
                max_iterations: 5,
                convergence_threshold: 0.8,
            },
        ];

        for p in &patterns {
            let desc = describe_pattern(p);
            assert!(!desc.is_empty(), "Description should not be empty");
        }
    }
}
