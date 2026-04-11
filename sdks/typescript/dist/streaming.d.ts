/**
 * SSE (Server-Sent Events) streaming helpers for the Argentor TypeScript SDK.
 */
/**
 * Parse a `fetch` Response whose body is a Server-Sent Events stream into an
 * async generator of parsed JSON objects.  Stops when the server sends
 * `data: [DONE]`.
 */
export declare function parseSSEStream(response: Response): AsyncGenerator<Record<string, unknown>>;
//# sourceMappingURL=streaming.d.ts.map