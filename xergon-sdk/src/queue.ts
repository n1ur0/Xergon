/**
 * Request queue with deduplication and debouncing.
 *
 * Provides RequestQueue for:
 * - Request deduplication: identical requests (same key) share a single in-flight promise
 * - Timeout: reject if a request takes too long
 * - Queue size limit
 * - Flush all pending requests
 */

// ── Types ────────────────────────────────────────────────────────────

export interface QueueOptions {
  /** Merge identical requests (same key) within this window in milliseconds. Default: 100. */
  deduplicateWindowMs?: number;
  /** Maximum number of pending requests. Default: 100. */
  maxQueueSize?: number;
  /** Default timeout per request in milliseconds. Default: 30000. */
  timeoutMs?: number;
}

export interface QueueStats {
  pending: number;
  completed: number;
  deduplicated: number;
}

// ── In-flight entry ──────────────────────────────────────────────────

interface InFlightEntry<T = unknown> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: any) => void;
  expiresAt: number;
}

// ── RequestQueue ─────────────────────────────────────────────────────

export class RequestQueue {
  private deduplicateWindowMs: number;
  private maxQueueSize: number;
  private timeoutMs: number;
  private pendingCount = 0;
  private completedCount = 0;
  private deduplicatedCount = 0;

  /** Currently in-flight requests keyed by dedup key. */
  private inFlight = new Map<string, InFlightEntry<any>>();
  /** Timer for auto-cleanup of expired dedup entries. */
  private cleanupTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(options?: QueueOptions) {
    this.deduplicateWindowMs = options?.deduplicateWindowMs ?? 100;
    this.maxQueueSize = options?.maxQueueSize ?? 100;
    this.timeoutMs = options?.timeoutMs ?? 30000;
  }

  /**
   * Enqueue a request. If an identical request (same key) is already
   * in-flight within the deduplication window, the same promise is returned.
   *
   * @param key - Deduplication key (e.g., serialized request params).
   * @param fn - The async function to execute if this is not a duplicate.
   * @param timeoutMs - Optional per-request timeout override.
   */
  enqueue<T>(key: string, fn: () => Promise<T>, timeoutMs?: number): Promise<T> {
    // Check for in-flight duplicate
    const existing = this.inFlight.get(key);
    if (existing) {
      this.deduplicatedCount++;
      return existing.promise as Promise<T>;
    }

    // Check queue size limit
    if (this.pendingCount >= this.maxQueueSize) {
      return Promise.reject(new Error('Request queue is full'));
    }

    this.pendingCount++;

    // Schedule cleanup of expired entries
    this.scheduleCleanup();

    let resolve!: (value: T) => void;
    let reject!: (reason: any) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });

    const effectiveTimeout = timeoutMs ?? this.timeoutMs;
    const expiresAt = Date.now() + effectiveTimeout;

    const entry: InFlightEntry<T> = {
      promise,
      resolve,
      reject,
      expiresAt,
    };

    this.inFlight.set(key, entry);

    let settled = false;

    // Set timeout
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      this.inFlight.delete(key);
      this.pendingCount--;
      this.completedCount++;
      reject(new Error(`Request timed out after ${effectiveTimeout}ms`));
    }, effectiveTimeout);

    // Execute
    fn()
      .then((value) => {
        clearTimeout(timer);
        if (settled) return;
        settled = true;
        this.inFlight.delete(key);
        this.pendingCount--;
        this.completedCount++;
        resolve(value);
      })
      .catch((err) => {
        clearTimeout(timer);
        if (settled) return;
        settled = true;
        this.inFlight.delete(key);
        this.pendingCount--;
        this.completedCount++;
        reject(err);
      });

    return promise;
  }

  /**
   * Reject all pending requests and clear the queue.
   */
  flush(): void {
    for (const [key, entry] of this.inFlight) {
      entry.reject(new Error('Queue flushed'));
      this.inFlight.delete(key);
      this.pendingCount--;
    }
    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
      this.cleanupTimer = null;
    }
  }

  /**
   * Get current queue statistics.
   */
  stats(): QueueStats {
    return {
      pending: this.pendingCount,
      completed: this.completedCount,
      deduplicated: this.deduplicatedCount,
    };
  }

  // ── Private ─────────────────────────────────────────────────────────

  private scheduleCleanup(): void {
    if (this.cleanupTimer) return;
    this.cleanupTimer = setTimeout(() => {
      this.cleanupExpired();
      this.cleanupTimer = null;
    }, this.deduplicateWindowMs + 10);
  }

  private cleanupExpired(): void {
    const now = Date.now();
    for (const [key, entry] of this.inFlight) {
      if (entry.expiresAt <= now) {
        // Already timed out or resolved, just clean up
        // (The timeout handler already removed it if it timed out)
        if (this.inFlight.has(key)) {
          this.inFlight.delete(key);
        }
      }
    }
  }
}
