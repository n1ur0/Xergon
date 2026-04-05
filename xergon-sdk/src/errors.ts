/**
 * XergonError -- typed error class matching the relay's error response format.
 *
 * Relay errors follow: {"error": {"type": "...", "message": "...", "code": N}}
 */

export type XergonErrorType =
  | 'invalid_request'
  | 'unauthorized'
  | 'forbidden'
  | 'not_found'
  | 'rate_limit_error'
  | 'internal_error'
  | 'service_unavailable';

export interface XergonErrorBody {
  type: XergonErrorType;
  message: string;
  code: number;
}

export class XergonError extends Error {
  public readonly type: XergonErrorType;
  public readonly code: number;

  constructor(errorBody: XergonErrorBody) {
    super(errorBody.message);
    this.name = 'XergonError';
    this.type = errorBody.type;
    this.code = errorBody.code;
  }

  static fromResponse(data: unknown): XergonError {
    if (
      data &&
      typeof data === 'object' &&
      'error' in data &&
      typeof (data as Record<string, unknown>).error === 'object' &&
      (data as Record<string, unknown>).error !== null
    ) {
      const err = (data as { error: Record<string, unknown> }).error;
      if (
        typeof err.type === 'string' &&
        typeof err.message === 'string' &&
        typeof err.code === 'number'
      ) {
        return new XergonError({
          type: err.type as XergonErrorType,
          message: err.message,
          code: err.code,
        });
      }
    }

    // Fallback for malformed error responses
    const message =
      data && typeof data === 'object' && 'message' in data
        ? String((data as Record<string, unknown>).message)
        : 'Unknown error';
    return new XergonError({
      type: 'internal_error',
      message,
      code: 500,
    });
  }

  get isUnauthorized(): boolean {
    return this.type === 'unauthorized';
  }

  get isRateLimited(): boolean {
    return this.type === 'rate_limit_error';
  }

  get isNotFound(): boolean {
    return this.type === 'not_found';
  }

  get isServiceUnavailable(): boolean {
    return this.type === 'service_unavailable';
  }
}
