/**
 * XergonClient -- core HTTP client with HMAC authentication.
 *
 * Provides the base request method used by all API modules.
 * Supports both HMAC-signed requests (private key) and Nautilus-style
 * public-key-only requests (wallet handles signing).
 */

import type { XergonClientConfig, LogInterceptor } from './types';
import type { RetryOptions } from './retry';
import { retryWithBackoff } from './retry';
import { hmacSign, buildHmacPayload } from './auth';
import { XergonError } from './errors';

const DEFAULT_BASE_URL = 'https://relay.xergon.gg';

export class XergonClientCore {
  private baseUrl: string;
  private publicKey: string | null;
  private privateKey: string | null;
  private interceptors: LogInterceptor[];
  private retryOptions: RetryOptions | false;

  constructor(config: XergonClientConfig & { retries?: RetryOptions | false } = {}) {
    this.baseUrl = (config.baseUrl ?? DEFAULT_BASE_URL).replace(/\/+$/, '');
    this.publicKey = config.publicKey ?? null;
    this.privateKey = config.privateKey ?? null;
    this.interceptors = [];
    // Default: enable retry with sensible defaults; set false to disable
    this.retryOptions = config.retries !== undefined ? config.retries : {};
  }

  // ── Auth ─────────────────────────────────────────────────────────────

  /**
   * Set full keypair for HMAC auth.
   */
  authenticate(publicKey: string, privateKey: string): void {
    this.publicKey = publicKey;
    this.privateKey = privateKey;
  }

  /**
   * Set only the public key (for Nautilus / wallet-managed signing).
   */
  setPublicKey(pk: string): void {
    this.publicKey = pk;
    this.privateKey = null;
  }

  /**
   * Clear all credentials.
   */
  clearAuth(): void {
    this.publicKey = null;
    this.privateKey = null;
  }

  getPublicKey(): string | null {
    return this.publicKey;
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }

  // ── Interceptors ─────────────────────────────────────────────────────

  /**
   * Add a log interceptor that receives request/response events.
   */
  addInterceptor(fn: LogInterceptor): void {
    this.interceptors.push(fn);
  }

  /**
   * Remove a previously-added interceptor.
   */
  removeInterceptor(fn: LogInterceptor): void {
    this.interceptors = this.interceptors.filter((f) => f !== fn);
  }

  private emitLog(event: Parameters<LogInterceptor>[0]): void {
    for (const fn of this.interceptors) {
      try {
        fn(event);
      } catch {
        // Swallow interceptor errors
      }
    }
  }

  // ── HTTP ─────────────────────────────────────────────────────────────

  /**
   * Build auth headers for a request.
   */
  async buildAuthHeaders(
    method: string,
    path: string,
    body: string,
  ): Promise<Record<string, string>> {
    const headers: Record<string, string> = {};

    if (!this.publicKey) return headers;

    headers['X-Xergon-Public-Key'] = this.publicKey;

    if (this.privateKey) {
      const timestamp = Math.floor(Date.now() / 1000);
      const payload = buildHmacPayload(body, timestamp);
      const signature = await hmacSign(payload, this.privateKey);
      headers['X-Xergon-Timestamp'] = String(timestamp);
      headers['X-Xergon-Signature'] = signature;
    }

    return headers;
  }

  /**
   * Core request method. All API methods use this.
   */
  async request<T>(
    method: string,
    path: string,
    body?: unknown,
    options?: {
      headers?: Record<string, string>;
      signal?: AbortSignal;
      skipAuth?: boolean;
      retries?: RetryOptions | false;
    },
  ): Promise<T> {
    const doRequest = async (): Promise<T> => {
      const url = `${this.baseUrl}${path}`;
      const bodyStr = body !== undefined ? JSON.stringify(body) : '';

      const startTime = Date.now();

      try {
        const headers: Record<string, string> = {
          ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
          ...options?.headers,
        };

        if (!options?.skipAuth) {
          const authHeaders = await this.buildAuthHeaders(method, path, bodyStr);
          Object.assign(headers, authHeaders);
        }

        const res = await fetch(url, {
          method,
          headers,
          body: body !== undefined ? bodyStr : undefined,
          signal: options?.signal,
        });

        const durationMs = Date.now() - startTime;

        this.emitLog({ method, url, status: res.status, durationMs });

        if (!res.ok) {
          let errorData: unknown;
          try {
            errorData = await res.json();
          } catch {
            errorData = { message: res.statusText };
          }
          this.emitLog({
            method,
            url,
            status: res.status,
            durationMs,
            error: String(errorData),
          });
          throw XergonError.fromResponse(errorData);
        }

        // Handle empty responses (204, etc.)
        const contentType = res.headers.get('content-type');
        if (
          !contentType ||
          contentType === 'text/plain' ||
          res.status === 204
        ) {
          const text = await res.text();
          return text as T;
        }

        return (await res.json()) as T;
      } catch (err) {
        if (err instanceof XergonError) throw err;

        const durationMs = Date.now() - startTime;
        this.emitLog({
          method,
          url,
          durationMs,
          error: err instanceof Error ? err.message : String(err),
        });
        throw err;
      }
    };

    // Determine retry config: per-request option > client-level > disabled
    const perRequestRetry = options?.retries !== undefined ? options.retries : this.retryOptions;

    if (perRequestRetry === false) {
      return doRequest();
    }

    return retryWithBackoff(doRequest, perRequestRetry);
  }

  /**
   * Convenience GET.
   */
  async get<T>(
    path: string,
    options?: {
      headers?: Record<string, string>;
      signal?: AbortSignal;
      skipAuth?: boolean;
    },
  ): Promise<T> {
    return this.request<T>('GET', path, undefined, options);
  }

  /**
   * Convenience POST.
   */
  async post<T>(
    path: string,
    body?: unknown,
    options?: {
      headers?: Record<string, string>;
      signal?: AbortSignal;
      skipAuth?: boolean;
    },
  ): Promise<T> {
    return this.request<T>('POST', path, body, options);
  }
}
