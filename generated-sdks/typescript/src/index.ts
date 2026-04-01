/**
 * argentor_client — TypeScript client for the Argentor API.
 */

import type {
  RunTaskResponse,
  BatchResponse,
  EvaluateResponse,
  CreatePersonaResponse,
  ListPersonasResponse,
  UsageBreakdown,
  HealthResponse,
  WebhookProxyResponse,
} from './types';
import { parseSSEStream } from './streaming';

export class ArgentorClient {
  private readonly baseUrl: string;
  private readonly headers: Record<string, string>;

  constructor(options?: {
    baseUrl?: string;
    apiKey?: string;
    tenantId?: string;
  }) {
    this.baseUrl = options?.baseUrl ?? 'http://localhost:3000';
    this.headers = {
      'Content-Type': 'application/json',
      'X-API-Key': options?.apiKey ?? '',
      'X-Tenant-ID': options?.tenantId ?? '',
    };
  }

  /**
   * Execute a single agent task.
   */
  async runTask(
    agentRole: string,
    context: string,
    options?: { model?: string; maxTokens?: number; tools?: string[] },
  ): Promise<RunTaskResponse> {
    const payload: Record<string, unknown> = { agent_role: agentRole, context };
    if (options?.model) payload.model = options.model;
    if (options?.maxTokens) payload.max_tokens = options.maxTokens;
    if (options?.tools) payload.tools = options.tools;

    const resp = await fetch(`${this.baseUrl}/v1/run`, {
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<RunTaskResponse>;
  }

  /**
   * Stream task results via SSE.
   */
  async *runTaskStream(
    agentRole: string,
    context: string,
    options?: { model?: string; maxTokens?: number; tools?: string[] },
  ): AsyncGenerator<Record<string, unknown>> {
    const payload: Record<string, unknown> = { agent_role: agentRole, context };
    if (options?.model) payload.model = options.model;
    if (options?.maxTokens) payload.max_tokens = options.maxTokens;
    if (options?.tools) payload.tools = options.tools;

    const resp = await fetch(`${this.baseUrl}/v1/run/stream`, {
      method: 'POST',
      headers: { ...this.headers, 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    if (!resp.ok) {
      throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    }

    yield* parseSSEStream(resp);
  }

  /**
   * Submit a batch of tasks for parallel execution.
   */
  async batch(
    tasks: Array<{ agentRole: string; context: string; model?: string; maxTokens?: number }>,
    options?: { maxConcurrent?: number },
  ): Promise<BatchResponse> {
    const payload = {
      tasks: tasks.map((t) => ({
        agent_role: t.agentRole,
        context: t.context,
        model: t.model,
        max_tokens: t.maxTokens,
      })),
      max_concurrent: options?.maxConcurrent ?? 5,
    };
    const resp = await fetch(`${this.baseUrl}/v1/batch`, {
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<BatchResponse>;
  }

  /**
   * Evaluate an agent response against criteria.
   */
  async evaluate(
    response: string,
    context: string,
    criteria?: string[],
  ): Promise<EvaluateResponse> {
    const payload: Record<string, unknown> = { response, context };
    if (criteria) payload.criteria = criteria;

    const resp = await fetch(`${this.baseUrl}/v1/evaluate`, {
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<EvaluateResponse>;
  }

  /**
   * Create a new agent persona for a tenant.
   */
  async createPersona(
    tenantId: string,
    agentRole: string,
    persona: Record<string, unknown>,
  ): Promise<CreatePersonaResponse> {
    const payload = { tenant_id: tenantId, agent_role: agentRole, persona };
    const resp = await fetch(`${this.baseUrl}/v1/personas`, {
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<CreatePersonaResponse>;
  }

  /**
   * List all personas for a tenant.
   */
  async listPersonas(tenantId: string): Promise<ListPersonasResponse> {
    const resp = await fetch(
      `${this.baseUrl}/v1/personas?tenant_id=${encodeURIComponent(tenantId)}`,
      { headers: this.headers },
    );
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<ListPersonasResponse>;
  }

  /**
   * Get usage breakdown for a tenant.
   */
  async getUsage(tenantId: string): Promise<UsageBreakdown> {
    const resp = await fetch(
      `${this.baseUrl}/v1/usage/${encodeURIComponent(tenantId)}`,
      { headers: this.headers },
    );
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<UsageBreakdown>;
  }

  /**
   * Check API server health.
   */
  async health(): Promise<HealthResponse> {
    const resp = await fetch(`${this.baseUrl}/health`, {
      headers: this.headers,
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<HealthResponse>;
  }

  /**
   * Forward a webhook event through the proxy.
   */
  async webhookProxy(
    event: string,
    data: Record<string, unknown>,
    options?: { source?: string; secret?: string },
  ): Promise<WebhookProxyResponse> {
    const payload: Record<string, unknown> = { event, data };
    if (options?.source) payload.source = options.source;
    if (options?.secret) payload.secret = options.secret;

    const resp = await fetch(`${this.baseUrl}/v1/webhooks/proxy`, {
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    return resp.json() as Promise<WebhookProxyResponse>;
  }
}

export { ArgentorClient as default };
export type * from './types';
