// ---------------------------------------------------------------------------
// Argentor Dashboard — TypeScript interfaces for API responses
// ---------------------------------------------------------------------------

/** Control-plane summary (GET /api/v1/control-plane/summary) */
export interface ControlPlaneSummary {
  total_deployments: number;
  running_instances: number;
  healthy_agents: number;
  total_tasks: number;
}

/** Control-plane event (GET /api/v1/control-plane/events) */
export interface ControlPlaneEvent {
  event_type?: string;
  kind?: string;
  message?: string;
  description?: string;
  timestamp?: string;
}

// --- Deployments -----------------------------------------------------------

export type DeploymentStatus =
  | "running"
  | "stopped"
  | "degraded"
  | "failed"
  | "dead"
  | "unknown";

export interface Deployment {
  id: string;
  name?: string;
  deployment_name?: string;
  role?: string;
  replicas?: number;
  status?: DeploymentStatus;
  tasks_completed?: number;
  total_tasks?: number;
  errors?: number;
  total_errors?: number;
}

export interface CreateDeploymentRequest {
  deployment_name: string;
  role: string;
  replicas: number;
}

export interface ScaleDeploymentRequest {
  replicas: number;
}

// --- Agents ----------------------------------------------------------------

export interface Agent {
  id: string;
  name?: string;
  agent_name?: string;
  role?: string;
  version?: string;
  capabilities?: string[] | string;
  skills?: string[] | string;
}

export interface RegisterAgentRequest {
  agent_name: string;
  role: string;
  version: string;
  capabilities: string[];
}

// --- Health ----------------------------------------------------------------

export type HealthStatus = "healthy" | "degraded" | "unhealthy" | "dead" | "unknown";

export interface HealthProbe {
  name?: string;
  probe_name?: string;
  ok?: boolean;
  passed?: boolean;
  status?: string;
}

export interface AgentHealth {
  id?: string;
  agent_name?: string;
  name?: string;
  status?: HealthStatus;
  health_status?: HealthStatus;
  role?: string;
  probes?: HealthProbe[];
  checks?: HealthProbe[];
  last_check?: string;
  last_heartbeat?: string;
  updated_at?: string;
}

export interface HealthSummary {
  healthy: number;
  degraded: number;
  unhealthy: number;
  dead: number;
}

export interface HealthResponse {
  summary?: HealthSummary;
  agents?: AgentHealth[] | Record<string, AgentHealth>;
  health_states?: AgentHealth[] | Record<string, AgentHealth>;
}

// --- Skills ----------------------------------------------------------------

export interface Skill {
  name: string;
  description?: string;
  version?: string;
  parameters?: Record<string, unknown>;
}

// --- Metrics ---------------------------------------------------------------

export interface ParsedMetric {
  name: string;
  value: string;
}

// --- Chat / Streaming ------------------------------------------------------

export interface ChatRequest {
  message: string;
  session_id?: string;
}

export interface ChatResponse {
  response: string;
  session_id: string;
}

/** SSE event from POST /api/v1/chat/stream */
export interface StreamEvent {
  type: string;
  data?: string;
  content?: string;
  session_id?: string;
  error?: string;
}

// --- Generic ---------------------------------------------------------------

export interface ApiError {
  error: string;
  hint?: string;
}
