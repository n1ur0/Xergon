/**
 * Cancellation tokens for cooperative request cancellation.
 *
 * Provides CancellationToken and CancellationManager for:
 * - Per-request cancellation via AbortController/AbortSignal
 * - Token chaining (child cancels when parent cancels)
 * - Timeout-based auto-cancellation
 * - Bulk cancel-all for shutting down
 */

let tokenCounter = 0;

// ── CancellationToken ────────────────────────────────────────────────

export class CancellationToken {
  readonly id: string;
  readonly signal: AbortSignal;
  private readonly controller: AbortController;
  private readonly children = new Set<CancellationToken>();
  private _reason: string | undefined;

  constructor(id?: string) {
    this.id = id ?? `token-${++tokenCounter}`;
    this.controller = new AbortController();
    this.signal = this.controller.signal;
  }

  /** Check if this token has been cancelled. */
  get isCancelled(): boolean {
    return this.signal.aborted;
  }

  /** The reason for cancellation, if cancelled. */
  get reason(): string | undefined {
    return this._reason;
  }

  /** Cancel this token and all linked children. */
  cancel(reason?: string): void {
    if (this.isCancelled) return;
    this._reason = reason;
    this.controller.abort(reason);
    // Propagate to children
    for (const child of this.children) {
      child.cancel(reason ?? `Cancelled by parent token ${this.id}`);
    }
  }

  /** Throw a standard error if this token is cancelled. */
  throwIfCancelled(): void {
    if (this.isCancelled) {
      throw new DOMException(this._reason ?? `Token ${this.id} was cancelled`, 'AbortError');
    }
  }

  /**
   * Link this token to a parent so that when the parent is cancelled,
   * this token (and its children) are also cancelled.
   */
  linkTo(parent: CancellationToken): void {
    if (parent.isCancelled) {
      this.cancel(`Parent token ${parent.id} already cancelled`);
      return;
    }
    parent.children.add(this);
  }

  /**
   * Create a derived token that auto-cancels after the given timeout.
   * The original token is NOT cancelled; only the returned token.
   */
  withTimeout(ms: number): CancellationToken {
    const timeoutToken = new CancellationToken(`timeout-${this.id}`);
    // Link to parent so parent cancellation cancels timeout too
    timeoutToken.linkTo(this);

    const timer = setTimeout(() => {
      timeoutToken.cancel(`Timeout after ${ms}ms`);
    }, ms);

    // Clean up timer if cancelled before timeout
    this.signal.addEventListener('abort', () => {
      clearTimeout(timer);
    }, { once: true });

    // Also clean up when the timeout token itself is aborted
    timeoutToken.signal.addEventListener('abort', () => {
      clearTimeout(timer);
    }, { once: true });

    return timeoutToken;
  }
}

// ── CancellationManager ─────────────────────────────────────────────

export class CancellationManager {
  private readonly tokens = new Map<string, CancellationToken>();

  /** Create a new cancellation token tracked by this manager. */
  createToken(id?: string): CancellationToken {
    const token = new CancellationToken(id);
    this.tokens.set(token.id, token);

    // Auto-remove when cancelled
    token.signal.addEventListener('abort', () => {
      this.tokens.delete(token.id);
    }, { once: true });

    return token;
  }

  /** Cancel all active tokens managed by this manager. */
  cancelAll(reason?: string): void {
    for (const token of this.tokens.values()) {
      token.cancel(reason ?? 'All tokens cancelled via cancelAll()');
    }
  }

  /** Get the count of currently active (non-cancelled) tokens. */
  activeCount(): number {
    return this.tokens.size;
  }

  /** Get a specific token by ID, if it exists and is active. */
  getToken(id: string): CancellationToken | undefined {
    const token = this.tokens.get(id);
    if (token && !token.isCancelled) return token;
    return undefined;
  }

  /** Dispose of all tracked tokens and clean up. */
  dispose(): void {
    this.cancelAll('CancellationManager disposed');
    this.tokens.clear();
  }
}
