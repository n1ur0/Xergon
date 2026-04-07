/**
 * Bench -- benchmark utility for the Xergon SDK.
 *
 * Measures latency, throughput, and token usage for chat completion requests.
 * Supports concurrent requests, warmup rounds, and percentile calculations.
 *
 * @example
 * ```ts
 * import { runBench } from '@xergon/sdk';
 *
 * const result = await runBench({
 *   model: 'llama-3.3-70b',
 *   requests: 20,
 *   concurrent: 5,
 *   warmup: 3,
 * });
 *
 * console.log(`${result.requestsPerSecond.toFixed(1)} req/s`);
 * console.log(`p50: ${result.p50Latency}ms, p99: ${result.p99Latency}ms`);
 * ```
 */

// ── Types ───────────────────────────────────────────────────────────

export interface BenchConfig {
  /** Model identifier to benchmark. */
  model: string;
  /** Prompt text (default: "Hello, respond with a short sentence."). */
  prompt?: string;
  /** Maximum tokens in response (default: 64). */
  maxTokens?: number;
  /** Number of concurrent requests (default: 1). */
  concurrent?: number;
  /** Total number of requests to run (default: 10). */
  requests?: number;
  /** Number of warmup requests to discard (default: 2). */
  warmup?: number;
  /** Per-request timeout in ms (default: 30000). */
  timeout?: number;
}

export interface BenchResult {
  model: string;
  totalRequests: number;
  successful: number;
  failed: number;
  totalDuration: number; // ms
  requestsPerSecond: number;
  avgLatency: number; // ms
  p50Latency: number;
  p90Latency: number;
  p99Latency: number;
  minLatency: number;
  maxLatency: number;
  totalTokens: number;
  tokensPerSecond: number;
  errors: string[];
}

interface LatencySample {
  latency: number;
  tokens: number;
  error?: string;
}

// ── Helpers ─────────────────────────────────────────────────────────

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = (p / 100) * (sorted.length - 1);
  const lower = Math.floor(idx);
  const upper = Math.ceil(idx);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (idx - lower) * (sorted[upper] - sorted[lower]);
}

/**
 * Execute a single benchmark request and return latency + token count.
 */
async function runSingleRequest(
  baseUrl: string,
  apiKey: string | undefined,
  config: Required<BenchConfig>,
): Promise<LatencySample> {
  const start = performance.now();

  try {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (apiKey) {
      headers['X-Public-Key'] = apiKey;
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), config.timeout);

    const res = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        model: config.model,
        messages: [{ role: 'user', content: config.prompt }],
        max_tokens: config.maxTokens,
        stream: false,
      }),
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    if (!res.ok) {
      const body = await res.text().catch(() => '');
      return {
        latency: performance.now() - start,
        tokens: 0,
        error: `HTTP ${res.status}: ${body.slice(0, 200)}`,
      };
    }

    const data = await res.json();
    const latency = performance.now() - start;
    const tokens = data?.usage?.total_tokens ?? 0;

    return { latency, tokens };
  } catch (err: unknown) {
    const latency = performance.now() - start;
    const message = err instanceof Error ? err.message : String(err);
    return { latency, tokens: 0, error: message };
  }
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * Run a benchmark against the Xergon relay.
 *
 * Executes warmup rounds (discarded), then runs N requests (sequentially or
 * concurrently depending on the `concurrent` setting), collects latency data,
 * and computes percentile statistics.
 */
export async function runBench(
  config: BenchConfig & { baseUrl?: string; apiKey?: string },
): Promise<BenchResult> {
  const resolved: Required<BenchConfig> & { baseUrl: string; apiKey: string } = {
    model: config.model,
    prompt: config.prompt ?? 'Hello, respond with a short sentence.',
    maxTokens: config.maxTokens ?? 64,
    concurrent: config.concurrent ?? 1,
    requests: config.requests ?? 10,
    warmup: config.warmup ?? 2,
    timeout: config.timeout ?? 30000,
    baseUrl: config.baseUrl ?? 'https://relay.xergon.gg',
    apiKey: config.apiKey ?? '',
  };

  const errors: string[] = [];
  const allSamples: LatencySample[] = [];

  // ── Warmup phase ───────────────────────────────────────────────
  if (resolved.warmup > 0) {
    const warmupBatch = Math.min(resolved.warmup, resolved.concurrent);
    for (let i = 0; i < resolved.warmup; i += warmupBatch) {
      const batch = Math.min(warmupBatch, resolved.warmup - i);
      const promises = Array.from({ length: batch }, () =>
        runSingleRequest(resolved.baseUrl, resolved.apiKey, resolved),
      );
      await Promise.all(promises);
    }
  }

  // ── Benchmark phase ───────────────────────────────────────────
  const totalStart = performance.now();

  // Run requests in batches of `concurrent` size
  for (let i = 0; i < resolved.requests; i += resolved.concurrent) {
    const batchSize = Math.min(resolved.concurrent, resolved.requests - i);
    const promises = Array.from({ length: batchSize }, () =>
      runSingleRequest(resolved.baseUrl, resolved.apiKey, resolved),
    );
    const batchResults = await Promise.all(promises);
    allSamples.push(...batchResults);
  }

  const totalDuration = performance.now() - totalStart;

  // ── Compute statistics ────────────────────────────────────────
  const successful = allSamples.filter(s => !s.error);
  const failed = allSamples.filter(s => s.error);

  for (const f of failed) {
    errors.push(f.error ?? 'unknown error');
  }

  const latencies = successful.map(s => s.latency).sort((a, b) => a - b);
  const totalTokens = successful.reduce((sum, s) => sum + s.tokens, 0);

  return {
    model: resolved.model,
    totalRequests: resolved.requests,
    successful: successful.length,
    failed: failed.length,
    totalDuration: Math.round(totalDuration),
    requestsPerSecond:
      totalDuration > 0
        ? Math.round((resolved.requests / totalDuration) * 1000 * 100) / 100
        : 0,
    avgLatency:
      latencies.length > 0
        ? Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length)
        : 0,
    p50Latency: Math.round(percentile(latencies, 50)),
    p90Latency: Math.round(percentile(latencies, 90)),
    p99Latency: Math.round(percentile(latencies, 99)),
    minLatency: latencies.length > 0 ? Math.round(latencies[0]) : 0,
    maxLatency:
      latencies.length > 0 ? Math.round(latencies[latencies.length - 1]) : 0,
    totalTokens,
    tokensPerSecond:
      totalDuration > 0
        ? Math.round((totalTokens / totalDuration) * 1000 * 100) / 100
        : 0,
    errors,
  };
}
