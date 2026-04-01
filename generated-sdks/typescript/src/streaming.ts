/**
 * SSE streaming helpers for the Argentor TypeScript SDK.
 */

/**
 * Parse a Server-Sent Events response into an async generator of parsed data
 * objects. Stops when it encounters a `data: [DONE]` message.
 */
export async function* parseSSEStream(
  response: Response,
): AsyncGenerator<Record<string, unknown>> {
  const reader = response.body?.getReader();
  if (!reader) {
    throw new Error('Response body is not readable');
  }

  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';

      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith('data: ')) {
          const data = trimmed.slice('data: '.length).trim();
          if (data === '[DONE]') {
            return;
          }
          yield JSON.parse(data) as Record<string, unknown>;
        }
      }
    }

    // Process any remaining buffer content
    if (buffer.trim().startsWith('data: ')) {
      const data = buffer.trim().slice('data: '.length).trim();
      if (data && data !== '[DONE]') {
        yield JSON.parse(data) as Record<string, unknown>;
      }
    }
  } finally {
    reader.releaseLock();
  }
}
