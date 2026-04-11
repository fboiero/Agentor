/**
 * Argentor Agent SDK -- wrap the argentor CLI for agentic execution.
 *
 * Like Claude Agent SDK wraps Claude Code, this wraps the `argentor` binary
 * and communicates via NDJSON over stdin/stdout.
 *
 * @example
 * ```ts
 * import { query, AgentOptions } from '@argentor/sdk/agent';
 *
 * for await (const event of query('Fix the bug in auth.py', {
 *   provider: 'claude',
 *   model: 'claude-sonnet-4-20250514',
 *   apiKey: 'sk-...',
 * })) {
 *   console.log(event);
 * }
 * ```
 */
export interface McpServerConfig {
    name: string;
    command: string;
    args: string[];
}
export interface AgentOptions {
    /** LLM provider: "claude", "openai", "gemini", "ollama". */
    provider?: string;
    /** Model identifier (e.g. "claude-sonnet-4-20250514", "gpt-4o"). */
    model?: string;
    /** API key for the chosen provider. */
    apiKey?: string;
    /** Optional system prompt override. */
    systemPrompt?: string;
    /** Maximum agentic turns before stopping. */
    maxTurns?: number;
    /** Sampling temperature. */
    temperature?: number;
    /** Tool / skill names available to the agent. `undefined` = builtins. */
    tools?: string[];
    /** Permission mode: "default", "strict", "permissive", "plan". */
    permissionMode?: 'default' | 'strict' | 'permissive' | 'plan';
    /** Working directory for the agent subprocess. */
    workingDirectory?: string;
    /** MCP server configurations to attach. */
    mcpServers?: McpServerConfig[];
    /** Whether to include intermediate streaming tokens. */
    includeStreaming?: boolean;
    /** Explicit path to the argentor binary. */
    argentorBinary?: string;
}
export interface AgentEvent {
    type: 'system' | 'assistant' | 'tool_use' | 'tool_result' | 'stream' | 'result' | 'error' | 'guardrail';
    [key: string]: unknown;
}
/** Preset for Anthropic Claude models. */
export declare function claudeOptions(apiKey: string, overrides?: Partial<AgentOptions>): AgentOptions;
/** Preset for OpenAI models. */
export declare function openaiOptions(apiKey: string, overrides?: Partial<AgentOptions>): AgentOptions;
/** Preset for Google Gemini models. */
export declare function geminiOptions(apiKey: string, overrides?: Partial<AgentOptions>): AgentOptions;
/** Preset for local Ollama models (no API key required). */
export declare function ollamaOptions(model?: string, overrides?: Partial<AgentOptions>): AgentOptions;
/**
 * Run an agent query and yield events as they arrive.
 *
 * Spawns `argentor --headless` as a child process, sends init + query
 * messages over stdin (NDJSON), and yields parsed `AgentEvent` objects
 * from the stdout NDJSON stream.
 *
 * @example
 * ```ts
 * for await (const event of query('Explain this codebase', claudeOptions('sk-...'))) {
 *   if (event.type === 'assistant') console.log(event.text);
 *   if (event.type === 'result') console.log('Done:', event.text ?? event.output);
 * }
 * ```
 */
export declare function query(prompt: string, options?: AgentOptions): AsyncGenerator<AgentEvent>;
/**
 * Run a query and return just the final output string.
 *
 * Convenience wrapper around {@link query} for callers who only need
 * the final result.
 *
 * @throws Error if the agent returns an error event.
 */
export declare function querySimple(prompt: string, options?: AgentOptions): Promise<string>;
/** Run a prompt with Anthropic Claude and return the final text. */
export declare function askClaude(prompt: string, apiKey: string): Promise<string>;
/** Run a prompt with OpenAI and return the final text. */
export declare function askOpenai(prompt: string, apiKey: string): Promise<string>;
/** Run a prompt with Google Gemini and return the final text. */
export declare function askGemini(prompt: string, apiKey: string): Promise<string>;
/** Run a prompt with a local Ollama model and return the final text. */
export declare function askOllama(prompt: string, model?: string): Promise<string>;
//# sourceMappingURL=agent.d.ts.map