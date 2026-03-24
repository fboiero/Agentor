//! Agent runner with LLM backend abstraction, failover, streaming, and context management.
//!
//! This crate implements the core agentic loop that drives Argentor agents,
//! including multi-provider LLM backends, automatic failover, token-aware
//! context windowing, and streaming event support.
//!
//! # Main types
//!
//! - [`AgentRunner`] — Executes the agentic loop: prompt, call tools, respond.
//! - [`ModelConfig`] — Configuration for model provider, name, and parameters.
//! - [`LlmProvider`] — Enum of supported LLM providers (OpenAI, Anthropic, etc.).
//! - [`ContextWindow`] — Token-aware sliding window over conversation history.
//! - [`StreamEvent`] — Events emitted during streamed agent responses.
//! - [`FailoverBackend`] — Multi-backend wrapper with automatic retry and failover.

/// LLM backend implementations.
pub mod backends;
/// Model and provider configuration.
pub mod config;
/// Token-aware context windowing.
pub mod context;
/// Failover and retry logic for LLM backends.
pub mod failover;
/// Agent identity, personality, session commands, and context compaction.
pub mod identity;
/// LLM client trait and HTTP transport.
pub mod llm;
/// Cost-aware model routing for multi-tier LLM selection.
pub mod model_router;
/// Agent runner and agentic loop.
pub mod runner;
/// Self-evaluation engine for agent response quality.
pub mod evaluator;
/// Streaming event types.
pub mod stream;
/// Adaptive memory integration for automatic context recall across sessions.
pub mod adaptive_memory;
/// ReAct (Reasoning + Acting) engine for structured agent reasoning.
pub mod react;
/// Token counting and cost estimation for different LLM providers.
pub mod token_counter;
/// Smart tool selection to reduce token waste and improve relevance.
pub mod tool_selector;
/// Lightweight code structure analysis: symbols, dependencies, call graph.
pub mod code_graph;
/// Precise diff generation, application, and validation.
pub mod diff_engine;
/// Test output parsing and TDD loop automation.
pub mod test_oracle;
/// Implementation planning with dependency ordering and risk assessment.
pub mod code_planner;
/// Multi-dimensional code review engine (security, performance, style, correctness).
pub mod review_engine;
/// Batch processor for grouping and executing multiple LLM requests.
pub mod batch_processor;
/// Circuit breaker for LLM provider resilience.
pub mod circuit_breaker;
/// Debug recorder for step-by-step agent reasoning traces.
pub mod debug_recorder;
/// In-memory LRU response cache for LLM calls with TTL expiration.
pub mod response_cache;
/// Structured output parsing and validation for LLM responses.
pub mod structured_output;
/// Production-grade guardrails for filtering, validating, and sanitizing LLM inputs/outputs.
pub mod guardrails;
/// Versioned prompt template management with A/B testing and chains.
pub mod prompt_manager;
/// Standardized evaluation framework for benchmarking agent performance.
pub mod eval_framework;

pub use backends::LlmBackend;
pub use config::{LlmProvider, ModelConfig};
pub use context::ContextWindow;
pub use failover::{FailoverBackend, RetryPolicy};
pub use identity::{AgentPersonality, ContextCompactor, SessionCommand, ThinkingLevel};
pub use llm::LlmClient;
pub use runner::AgentRunner;
pub use stream::StreamEvent;
pub use react::{ReActAction, ReActConfig, ReActEngine, ReActOutcome, ReActStep, ReActTrace};
pub use token_counter::{TokenCounter, TokenEstimate, UsageTracker};
pub use evaluator::{
    EvaluationAction, EvaluationResult, EvaluatorConfig, QualityScore, ResponseEvaluator,
};
pub use tool_selector::{SelectionStrategy, ToolSelection, ToolSelector, ToolStats};
pub use model_router::{
    ModelCost, ModelOption, ModelRouter, ModelTier, RoutingDecision, RoutingStrategy,
    TaskComplexity,
};
pub use adaptive_memory::{
    AdaptiveMemory, AdaptiveMemoryConfig, MemoryEntry, MemoryKind, RecallResult,
};
pub use code_graph::{
    CodeContext, CodeGraph, CodeGraphSummary, CodeSnippet, CodeSymbol, Dependency, ImpactAnalysis,
    Language, SymbolKind, Visibility,
};
pub use diff_engine::{
    ApplyResult, DiffConfig, DiffEngine, DiffHunk, DiffLine, DiffOperation, DiffPlan, FileDiff,
};
pub use test_oracle::{
    ErrorType, FailureAnalysis, FixStrategy, TddCycle, TddPhase, TestCase, TestFramework,
    TestOracle, TestRunSummary, TestStatus,
};
pub use code_planner::{
    AgentRole as PlannerRole, CodePlanner, Effort, FileOperation, ImplementationPlan, PlanStep,
    PlannerConfig, RiskAssessment, TaskType, TestStrategy,
};
pub use review_engine::{
    DimensionScore, FindingSeverity, ReviewConfig, ReviewDimension, ReviewEngine, ReviewFinding,
    ReviewReport, ReviewVerdict,
};
pub use batch_processor::{
    BatchConfig, BatchProcessor, BatchProcessorStats, BatchRequest, BatchResult, RequestResult,
    RequestStatus,
};
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerRegistry, CircuitBreakerStatus, CircuitConfig, CircuitState,
};
pub use debug_recorder::{
    DebugRecorder, DebugStep, DebugTrace, StepType, TokenUsage, TraceSummary,
};
pub use response_cache::{CacheKey, CacheMessage, CacheStats, ResponseCache};
pub use structured_output::{
    ExtractedOutput, ExtractionPattern, FieldDefinition, FieldType, OutputSchema,
    StructuredOutputParser, ValidationError,
};
pub use guardrails::{
    ContentPolicy, GuardrailEngine, GuardrailResult, GuardrailRule, PiiMatch, RuleSeverity,
    RuleType, Violation, redact_pii,
};
pub use prompt_manager::{
    ChainStep, PromptChain, PromptError, PromptManager, PromptTemplate, TemplateSummary,
    TemplateVariable, VarType, register_xcapit_templates, outreach_composer_v1,
    sales_qualifier_v1, support_responder_v1, ticket_router_v1,
};
pub use eval_framework::{
    CaseResult, CategoryResult, ComparisonReport, CompositeEvaluator, ContainsEvaluator,
    EvalCase, EvalFramework, EvalReport, EvalSuite, Evaluator, ExactMatchEvaluator,
    HeuristicEvaluator, JsonSchemaEvaluator, SimilarityEvaluator,
    lead_qualification_suite, support_quality_suite, ticket_routing_suite,
};
