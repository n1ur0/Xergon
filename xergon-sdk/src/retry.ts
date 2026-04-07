/**
 * Retry with exponential backoff and jitter for HTTP requests.
 *
 * Wraps an async function so transient failures (429, 502, 503, 504,
 * network errors) are automatically retried with increasing delays.
 */

// ── Types ────────────────────────────────────────────────────────────

export type BackoffStrategy = 'exponential' | 'linear' | 'constant';

export interface RetryConfig {
  /** Maximum number of retry attempts (default: 3). */
  maxRetries: number;
  /** Base delay in ms before first retry (default: 1000). */
  baseDelayMs: number;
  /** Maximum delay cap in ms (default: 30000). */
  maxDelayMs: number;
  /** Exponential backoff multiplier (default: 2). */
  backoffMultiplier: number;
  /** Whether to add random jitter to delays (default: true). */
  jitter: boolean;
  /** Range of jitter in ms (default: 500). */
  jitterRangeMs: number;
  /** HTTP status codes that should trigger a retry (default: 408, 429, 500, 502, 503, 504). */
  retryableStatuses: number[];
  /** Error codes that should trigger a retry (default: ECONNREFUSED, ECONNRESET, ETIMEDOUT, ENOTFOUND). */
  retryableErrors: string[];
  /** Called before each retry with attempt number, error, and delay. */
  onRetry?: (attempt: number, error: Error, delayMs: number) => void;
}

/** @deprecated Use RetryConfig instead. Kept for backward compatibility. */
export interface RetryOptions {
  maxRetries?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  backoffFactor?: number;
  retryableStatuses?: number[];
  onRetry?: (attempt: number, delayMs: number, error: unknown) => void;
}

// ── Defaults ─────────────────────────────────────────────────────────

const DEFAULT_RETRY_CONFIG: RetryConfig = {
  maxRetries: 3,
  baseDelayMs: 1000,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
  jitter: true,
  jitterRangeMs: 500,
  retryableStatuses: [408, 429, 500, 502, 503, 504],
  retryableErrors: ['ECONNREFUSED', 'ECONNRESET', 'ETIMEDOUT', 'ENOTFOUND'],
};

const DEFAULT_OPTIONS: Required<Pick<RetryOptions,
  'maxRetries' | 'baseDelayMs' | 'maxDelayMs' | 'backoffFactor' | 'retryableStatuses'
>> = {
  maxRetries: 3,
  baseDelayMs: 1000,
  maxDelayMs: 30000,
  backoffFactor: 2,
  retryableStatuses: [429, 500, 502, 503, 504],
};

// ── Error Detection ──────────────────────────────────────────────────

/**
 * Check whether an error is a network/timeout error (not an HTTP status error).
 */
export function isNetworkError(err: unknown): boolean {
  if (err instanceof TypeError) {
    // fetch throws TypeError for network failures, DNS issues, CORS, etc.
    const msg = (err as TypeError).message.toLowerCase();
    return (
      msg.includes('fetch failed') ||
      msg.includes('network') ||
      msg.includes('econnrefused') ||
      msg.includes('econnreset') ||
      msg.includes('socket hang up') ||
      msg.includes('timeout') ||
      msg.includes('aborted')
    );
  }
  if (err instanceof Error) {
    const msg = err.message.toLowerCase();
    return (
      msg.includes('econnrefused') ||
      msg.includes('econnreset') ||
      msg.includes('socket hang up') ||
      msg.includes('etimedout') ||
      msg.includes('timeout')
    );
  }
  return false;
}

/**
 * Check whether an error should trigger a retry.
 */
function isRetryableError(err: unknown, retryableStatuses: number[]): boolean {
  if (isNetworkError(err)) return true;

  // Check for XergonError with a retryable status code
  if (
    err &&
    typeof err === 'object' &&
    'code' in err &&
    typeof (err as Record<string, unknown>).code === 'number'
  ) {
    const code = (err as Record<string, unknown>).code as number;
    return retryableStatuses.includes(code);
  }

  // Check for AbortError -- do NOT retry aborted requests
  if (err instanceof Error && err.name === 'AbortError') {
    return false;
  }

  return false;
}

// ── Backoff Calculation ──────────────────────────────────────────────

/**
 * Calculate delay with exponential backoff and jitter.
 * delay = min(baseDelay * factor^attempt + random(0, 1000), maxDelay)
 */
export function calculateBackoffDelay(
  attempt: number,
  baseDelayMs: number,
  maxDelayMs: number,
  backoffFactor: number,
): number {
  const exponentialDelay = baseDelayMs * Math.pow(backoffFactor, attempt);
  const jitter = Math.random() * 1000;
  return Math.min(exponentialDelay + jitter, maxDelayMs);
}

/**
 * Sleep for the specified number of milliseconds.
 * If a signal is provided and aborts before the delay, resolves immediately.
 */
function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    if (signal) {
      const onAbort = () => {
        clearTimeout(timer);
        resolve();
      };
      signal.addEventListener('abort', onAbort, { once: true });
    }
  });
}

// ── Legacy retryWithBackoff ──────────────────────────────────────────

/**
 * Execute `fn` with automatic retry and exponential backoff.
 *
 * Retries on retryable HTTP status codes and network errors.
 * Does NOT retry on AbortError (cancelled requests) or client errors (4xx except 429).
 */
export async function retryWithBackoff<T>(
  fn: () => Promise<T>,
  options?: RetryOptions,
): Promise<T> {
  const opts = { ...DEFAULT_OPTIONS, ...options };
  const { maxRetries, baseDelayMs, maxDelayMs, backoffFactor, retryableStatuses, onRetry } = opts;

  let lastError: unknown;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await fn();
    } catch (err) {
      lastError = err;

      // Don't retry if we've exhausted attempts
      if (attempt >= maxRetries) {
        break;
      }

      // Don't retry non-retryable errors
      if (!isRetryableError(err, retryableStatuses)) {
        break;
      }

      const delayMs = calculateBackoffDelay(attempt, baseDelayMs, maxDelayMs, backoffFactor);

      console.warn(
        `[xergon-sdk] Request failed (attempt ${attempt + 1}/${maxRetries + 1}), ` +
        `retrying in ${Math.round(delayMs)}ms...`,
        err instanceof Error ? err.message : String(err),
      );

      onRetry?.(attempt + 1, delayMs, err);

      await sleep(delayMs);
    }
  }

  throw lastError;
}

// ── RetryClient (new class-based API) ────────────────────────────────

export interface RetryStats {
  totalRetries: number;
  totalFailures: number;
  lastError?: Error;
}

/**
 * Class-based retry client with multiple backoff strategies,
 * jitter, AbortSignal support, and stats tracking.
 */
export class RetryClient {
  private config: RetryConfig;
  private stats: RetryStats = { totalRetries: 0, totalFailures: 0 };

  constructor(config?: Partial<RetryConfig>) {
    this.config = { ...DEFAULT_RETRY_CONFIG, ...config };
  }

  /**
   * Execute a function with retry and backoff.
   *
   * @param fn - Async function receiving an AbortSignal for per-attempt cancellation.
   * @param options - Optional strategy override and external AbortSignal.
   */
  async execute<T>(fn: (signal: AbortSignal) => Promise<T>, options?: {
    strategy?: BackoffStrategy;
    signal?: AbortSignal;
  }): Promise<T> {
    const strategy = options?.strategy ?? 'exponential';
    const { maxRetries, retryableStatuses, retryableErrors, onRetry } = this.config;

    // Check if already cancelled
    if (options?.signal?.aborted) {
      throw new DOMException('Operation aborted before execution', 'AbortError');
    }

    let lastError: Error | undefined;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      // Check external cancellation before each attempt
      if (options?.signal?.aborted) {
        throw new DOMException('Operation aborted', 'AbortError');
      }

      try {
        // Create a per-attempt abort controller (allows timeout per attempt)
        const attemptController = new AbortController();

        // If external signal aborts, abort the attempt too
        const onExternalAbort = () => attemptController.abort();
        options?.signal?.addEventListener('abort', onExternalAbort, { once: true });

        try {
          const result = await fn(attemptController.signal);
          return result;
        } finally {
          options?.signal?.removeEventListener('abort', onExternalAbort);
        }
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));

        // Don't retry if we've exhausted attempts
        if (attempt >= maxRetries) {
          this.stats.totalFailures++;
          this.stats.lastError = lastError;
          break;
        }

        // Don't retry AbortError
        if (lastError.name === 'AbortError') {
          this.stats.totalFailures++;
          this.stats.lastError = lastError;
          throw lastError;
        }

        // Don't retry non-retryable errors
        if (!this.isRetryableError(lastError, retryableStatuses, retryableErrors)) {
          this.stats.totalFailures++;
          this.stats.lastError = lastError;
          break;
        }

        // Calculate delay
        const delayMs = this.calculateDelay(attempt, strategy);

        this.stats.totalRetries++;
        onRetry?.(attempt + 1, lastError, delayMs);

        await sleep(delayMs, options?.signal);
      }
    }

    throw lastError;
  }

  /**
   * Calculate the delay for a given attempt number using the specified strategy.
   */
  calculateDelay(attempt: number, strategy?: BackoffStrategy): number {
    const s = strategy ?? 'exponential';
    const { baseDelayMs, maxDelayMs, backoffMultiplier, jitter, jitterRangeMs } = this.config;

    let delay: number;

    switch (s) {
      case 'exponential':
        delay = baseDelayMs * Math.pow(backoffMultiplier, attempt);
        break;
      case 'linear':
        delay = baseDelayMs + attempt * baseDelayMs;
        break;
      case 'constant':
        delay = baseDelayMs;
        break;
    }

    delay = Math.min(delay, maxDelayMs);

    if (jitter) {
      delay += Math.random() * jitterRangeMs;
    }

    return Math.min(delay, maxDelayMs);
  }

  /**
   * Check if an error is retryable based on status codes and error codes.
   */
  isRetryable(error: unknown): boolean {
    return this.isRetryableError(
      error,
      this.config.retryableStatuses,
      this.config.retryableErrors,
    );
  }

  /** Get cumulative retry statistics. */
  getStats(): RetryStats {
    return { ...this.stats };
  }

  /** Reset all retry statistics. */
  resetStats(): void {
    this.stats = { totalRetries: 0, totalFailures: 0 };
  }

  // ── Private ─────────────────────────────────────────────────────────

  private isRetryableError(
    err: unknown,
    retryableStatuses: number[],
    retryableErrors: string[],
  ): boolean {
    // AbortError is never retryable
    if (err instanceof Error && err.name === 'AbortError') {
      return false;
    }

    // Network errors
    if (isNetworkError(err)) return true;

    // HTTP status codes (XergonError has .code)
    if (
      err &&
      typeof err === 'object' &&
      'code' in err &&
      typeof (err as Record<string, unknown>).code === 'number'
    ) {
      return retryableStatuses.includes((err as Record<string, unknown>).code as number);
    }

    // Error code matching (e.g., Node.js system errors)
    if (
      err &&
      typeof err === 'object' &&
      'code' in err &&
      typeof (err as Record<string, unknown>).code === 'string'
    ) {
      return retryableErrors.includes((err as Record<string, unknown>).code as string);
    }

    return false;
  }
}
