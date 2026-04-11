/**
 * SSE (Server-Sent Events) streaming helpers for the Argentor TypeScript SDK.
 */
/**
 * Parse a `fetch` Response whose body is a Server-Sent Events stream into an
 * async generator of parsed JSON objects.  Stops when the server sends
 * `data: [DONE]`.
 */
export async function* parseSSEStream(response) {
    const reader = response.body?.getReader();
    if (!reader) {
        throw new Error('Response body is not readable');
    }
    const decoder = new TextDecoder();
    let buffer = '';
    try {
        while (true) {
            const { done, value } = await reader.read();
            if (done)
                break;
            buffer += decoder.decode(value, { stream: true });
            const lines = buffer.split('\n');
            // The last element may be an incomplete line -- keep it in the buffer.
            buffer = lines.pop() ?? '';
            for (const line of lines) {
                const trimmed = line.trim();
                if (trimmed.startsWith('data: ')) {
                    const data = trimmed.slice('data: '.length).trim();
                    if (data === '[DONE]') {
                        return;
                    }
                    yield JSON.parse(data);
                }
            }
        }
        // Process any remaining content left in the buffer.
        if (buffer.trim().startsWith('data: ')) {
            const data = buffer.trim().slice('data: '.length).trim();
            if (data && data !== '[DONE]') {
                yield JSON.parse(data);
            }
        }
    }
    finally {
        reader.releaseLock();
    }
}
//# sourceMappingURL=streaming.js.map