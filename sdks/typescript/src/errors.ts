/**
 * Custom error classes for the Argentor TypeScript SDK.
 */

import type { ApiErrorBody } from './types';

/**
 * Base error for all Argentor SDK errors.
 */
export class ArgentorError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ArgentorError';
  }
}

/**
 * Thrown when the Argentor API returns a non-2xx response.
 */
export class ArgentorAPIError extends ArgentorError {
  public readonly statusCode: number;
  public readonly responseBody: ApiErrorBody;

  constructor(message: string, statusCode: number, responseBody: ApiErrorBody = {}) {
    super(`HTTP ${statusCode}: ${message}`);
    this.name = 'ArgentorAPIError';
    this.statusCode = statusCode;
    this.responseBody = responseBody;
  }
}

/**
 * Thrown when the SDK cannot reach the Argentor server.
 */
export class ArgentorConnectionError extends ArgentorError {
  constructor(message: string) {
    super(message);
    this.name = 'ArgentorConnectionError';
  }
}

/**
 * Thrown when a request to the Argentor API times out.
 */
export class ArgentorTimeoutError extends ArgentorError {
  constructor(message: string) {
    super(message);
    this.name = 'ArgentorTimeoutError';
  }
}
