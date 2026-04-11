/**
 * Custom error classes for the Argentor TypeScript SDK.
 */
import type { ApiErrorBody } from './types';
/**
 * Base error for all Argentor SDK errors.
 */
export declare class ArgentorError extends Error {
    constructor(message: string);
}
/**
 * Thrown when the Argentor API returns a non-2xx response.
 */
export declare class ArgentorAPIError extends ArgentorError {
    readonly statusCode: number;
    readonly responseBody: ApiErrorBody;
    constructor(message: string, statusCode: number, responseBody?: ApiErrorBody);
}
/**
 * Thrown when the SDK cannot reach the Argentor server.
 */
export declare class ArgentorConnectionError extends ArgentorError {
    constructor(message: string);
}
/**
 * Thrown when a request to the Argentor API times out.
 */
export declare class ArgentorTimeoutError extends ArgentorError {
    constructor(message: string);
}
//# sourceMappingURL=errors.d.ts.map