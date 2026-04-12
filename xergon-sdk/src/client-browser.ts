/**
 * XergonClient - Browser-compatible wrapper
 * 
 * This is a browser-safe version of XergonClient that doesn't
 * depend on Node.js-only modules.
 */

import { XergonClientCore } from './client';
import type { XergonClientConfig } from './types';

export class XergonClient {
  private core: XergonClientCore;

  constructor(config: XergonClientConfig = {}) {
    this.core = new XergonClientCore(config);
  }

  // ── Auth ─────────────────────────────────────────────────────────────

  authenticate(publicKey: string, privateKey: string): void {
    this.core.authenticate(publicKey, privateKey);
  }

  setPublicKey(pk: string): void {
    this.core.setPublicKey(pk);
  }

  clearAuth(): void {
    this.core.clearAuth();
  }

  getPublicKey(): string | null {
    return this.core.getPublicKey();
  }

  getBaseUrl(): string {
    return this.core.getBaseUrl();
  }

  addInterceptor(fn: import('./types').LogInterceptor): void {
    this.core.addInterceptor(fn);
  }

  removeInterceptor(fn: import('./types').LogInterceptor): void {
    this.core.removeInterceptor(fn);
  }

  async authStatus(): Promise<import('./types').AuthStatus> {
    return this.core.get<import('./types').AuthStatus>('/v1/auth/status');
  }

  // ── Chat (OpenAI-compatible) ─────────────────────────────────────────

  readonly chat = {
    completions: {
      create: (params: import('./types').ChatCompletionParams, options?: { signal?: AbortSignal }) =>
        import('./chat').then(m => m.createChatCompletion(this.core, params, options)),
      stream: (params: import('./types').ChatCompletionParams, options?: { signal?: AbortSignal; sseRetry?: import('./sse-retry').SSERetryOptions | false }) =>
        import('./chat').then(m => m.streamChatCompletion(this.core, params, options)),
    },
  };

  // ── Health ───────────────────────────────────────────────────────────

  readonly health = {
    check: () => import('./health').then(m => m.healthCheck(this.core)),
  };

  readonly ready = {
    check: () => import('./health').then(m => m.readyCheck(this.core)),
  };

  // ── Models ───────────────────────────────────────────────────────────

  readonly models = {
    list: () => import('./models').then(m => m.listModels(this.core)),
  };

  // ── Balance ──────────────────────────────────────────────────────────

  readonly balance = {
    get: (userPk: string) => import('./balance').then(m => m.getBalance(this.core, userPk)),
  };
}
