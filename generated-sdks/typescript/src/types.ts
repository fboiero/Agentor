/**
 * TypeScript type definitions for the Argentor API.
 */

// ---------------------------------------------------------------------------
// Run Task
// ---------------------------------------------------------------------------

export interface RunTaskRequest {
  agent_role: string;
  context: string;
  model?: string;
  max_tokens?: number;
  tools?: string[];
}

export interface RunTaskResponse {
  task_id: string;
  status: string;
  result?: string;
  tokens_used?: number;
  duration_ms?: number;
  metadata?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Batch
// ---------------------------------------------------------------------------

export interface BatchTask {
  agent_role: string;
  context: string;
  model?: string;
  max_tokens?: number;
}

export interface BatchRequest {
  tasks: BatchTask[];
  max_concurrent: number;
}

export interface BatchTaskResult {
  task_id: string;
  status: string;
  result?: string;
  error?: string;
}

export interface BatchResponse {
  batch_id: string;
  results: BatchTaskResult[];
  total: number;
  succeeded: number;
  failed: number;
}

// ---------------------------------------------------------------------------
// Evaluate
// ---------------------------------------------------------------------------

export interface EvaluateRequest {
  response: string;
  context: string;
  criteria?: string[];
}

export interface CriterionScore {
  criterion: string;
  score: number;
  explanation?: string;
}

export interface EvaluateResponse {
  overall_score: number;
  scores: CriterionScore[];
  summary?: string;
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

export interface PersonaConfig {
  name: string;
  system_prompt?: string;
  temperature?: number;
  model?: string;
  tools?: string[];
  metadata?: Record<string, unknown>;
}

export interface CreatePersonaRequest {
  tenant_id: string;
  agent_role: string;
  persona: PersonaConfig;
}

export interface CreatePersonaResponse {
  persona_id: string;
  tenant_id: string;
  agent_role: string;
  created_at: string;
}

export interface PersonaSummary {
  persona_id: string;
  agent_role: string;
  name: string;
  created_at: string;
}

export interface ListPersonasResponse {
  tenant_id: string;
  personas: PersonaSummary[];
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

export interface ModelUsage {
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cost_usd?: number;
}

export interface UsageBreakdown {
  tenant_id: string;
  period_start: string;
  period_end: string;
  models: ModelUsage[];
  total_tokens: number;
  total_cost_usd?: number;
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

export interface HealthResponse {
  status: string;
  version: string;
  uptime_seconds?: number;
}

// ---------------------------------------------------------------------------
// Webhook Proxy
// ---------------------------------------------------------------------------

export interface WebhookProxyRequest {
  event: string;
  data: Record<string, unknown>;
  source?: string;
  secret?: string;
}

export interface WebhookProxyResponse {
  accepted: boolean;
  event_id?: string;
  message?: string;
}
