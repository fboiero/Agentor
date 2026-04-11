// ---------------------------------------------------------------------------
// Argentor Dashboard — API Client
// ---------------------------------------------------------------------------

import type {
  Agent,
  AgentHealth,
  ChatRequest,
  ChatResponse,
  ControlPlaneEvent,
  ControlPlaneSummary,
  CreateDeploymentRequest,
  Deployment,
  HealthResponse,
  HealthSummary,
  ParsedMetric,
  RegisterAgentRequest,
  ScaleDeploymentRequest,
  Skill,
} from "../types";

const BASE_URL = import.meta.env.VITE_API_BASE_URL as string | undefined ?? window.location.origin;
const CP_BASE = `${BASE_URL}/api/v1/control-plane`;
const API_BASE = `${BASE_URL}/api/v1`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

class ApiRequestError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiRequestError";
    this.status = status;
  }
}

async function request<T>(url: string, options?: RequestInit): Promise<T> {
  const resp = await fetch(url, options);
  if (!resp.ok) {
    let msg = `HTTP ${resp.status}`;
    try {
      const body = await resp.json() as { error?: string };
      if (body?.error) msg = body.error;
    } catch {
      // ignore parse errors
    }
    throw new ApiRequestError(msg, resp.status);
  }
  return resp.json() as Promise<T>;
}

/** Normalise object-or-array responses into a flat array. */
function normalizeArray<T extends { id?: string }>(
  data: T[] | Record<string, T>,
): T[] {
  if (Array.isArray(data)) return data;
  if (data && typeof data === "object") {
    return Object.entries(data).map(([key, val]) => {
      if (!val.id) val.id = key;
      return val;
    });
  }
  return [];
}

// ---------------------------------------------------------------------------
// Control Plane — Overview
// ---------------------------------------------------------------------------

export async function fetchSummary(): Promise<ControlPlaneSummary> {
  return request<ControlPlaneSummary>(`${CP_BASE}/summary`);
}

export async function fetchEvents(): Promise<ControlPlaneEvent[]> {
  const data = await request<ControlPlaneEvent[]>(`${CP_BASE}/events`);
  return Array.isArray(data) ? data : [];
}

// ---------------------------------------------------------------------------
// Deployments
// ---------------------------------------------------------------------------

export async function fetchDeployments(): Promise<Deployment[]> {
  const data = await request<Deployment[] | Record<string, Deployment>>(
    `${CP_BASE}/deployments`,
  );
  return normalizeArray(data);
}

export async function createDeployment(
  body: CreateDeploymentRequest,
): Promise<Deployment> {
  return request<Deployment>(`${CP_BASE}/deployments`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export async function scaleDeployment(
  id: string,
  body: ScaleDeploymentRequest,
): Promise<void> {
  await request<unknown>(
    `${CP_BASE}/deployments/${encodeURIComponent(id)}/scale`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    },
  );
}

export async function deleteDeployment(id: string): Promise<void> {
  await request<unknown>(
    `${CP_BASE}/deployments/${encodeURIComponent(id)}`,
    { method: "DELETE" },
  );
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export async function fetchAgents(): Promise<Agent[]> {
  const data = await request<Agent[] | Record<string, Agent>>(
    `${CP_BASE}/agents`,
  );
  return normalizeArray(data);
}

export async function registerAgent(body: RegisterAgentRequest): Promise<Agent> {
  return request<Agent>(`${CP_BASE}/agents`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

export async function fetchHealth(): Promise<{
  summary: HealthSummary;
  agents: AgentHealth[];
}> {
  const raw = await request<HealthResponse | AgentHealth[]>(
    `${CP_BASE}/health`,
  );

  let agents: AgentHealth[] = [];
  let summary: HealthSummary = { healthy: 0, degraded: 0, unhealthy: 0, dead: 0 };

  if (Array.isArray(raw)) {
    agents = raw;
  } else if (raw && typeof raw === "object") {
    if (raw.agents) {
      agents = normalizeArray(raw.agents as AgentHealth[] | Record<string, AgentHealth>);
    } else if (raw.health_states) {
      agents = normalizeArray(
        raw.health_states as AgentHealth[] | Record<string, AgentHealth>,
      );
    }
    if (raw.summary) {
      summary = raw.summary;
    }
  }

  // Compute summary from agents if the server didn't provide one
  if (!("summary" in (raw as object))) {
    summary = { healthy: 0, degraded: 0, unhealthy: 0, dead: 0 };
    for (const a of agents) {
      const s = (a.status ?? a.health_status ?? "unknown").toLowerCase();
      if (s === "healthy") summary.healthy++;
      else if (s === "degraded") summary.degraded++;
      else if (s === "unhealthy") summary.unhealthy++;
      else if (s === "dead") summary.dead++;
      else summary.unhealthy++;
    }
  }

  return { summary, agents };
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

export async function fetchSkills(): Promise<Skill[]> {
  const data = await request<Skill[]>(`${API_BASE}/skills`);
  return Array.isArray(data) ? data : [];
}

// ---------------------------------------------------------------------------
// Metrics (Prometheus text format)
// ---------------------------------------------------------------------------

export async function fetchMetricsRaw(): Promise<string> {
  const resp = await fetch(`${BASE_URL}/metrics`);
  if (!resp.ok) throw new ApiRequestError("Metrics not available", resp.status);
  return resp.text();
}

export function parsePrometheusMetrics(text: string): ParsedMetric[] {
  const metrics: ParsedMetric[] = [];
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const parts = trimmed.split(/\s+/);
    if (parts.length >= 2) {
      metrics.push({ name: parts[0], value: parts[1] });
    }
  }
  return metrics;
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

export async function sendChatMessage(
  body: ChatRequest,
): Promise<ChatResponse> {
  return request<ChatResponse>(`${API_BASE}/agent/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

/**
 * Opens an SSE connection to the streaming chat endpoint.
 * Returns an AbortController the caller can use to cancel.
 */
export function streamChat(
  body: ChatRequest,
  onEvent: (data: string) => void,
  onError: (err: Error) => void,
  onDone: () => void,
): AbortController {
  const controller = new AbortController();

  (async () => {
    try {
      const resp = await fetch(`${API_BASE}/chat/stream`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: controller.signal,
      });

      if (!resp.ok || !resp.body) {
        throw new Error(`Stream failed: HTTP ${resp.status}`);
      }

      const reader = resp.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        // Parse SSE frames
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed.startsWith("data:")) {
            const payload = trimmed.slice(5).trim();
            if (payload === "[DONE]") {
              onDone();
              return;
            }
            onEvent(payload);
          }
        }
      }

      onDone();
    } catch (err) {
      if ((err as DOMException).name !== "AbortError") {
        onError(err instanceof Error ? err : new Error(String(err)));
      }
    }
  })();

  return controller;
}
