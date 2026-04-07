/**
 * Xergon SDK -- Canary Deployment
 *
 * Progressive traffic shifting between a baseline model and a canary model.
 * Monitors success rate and latency, auto-promotes or auto-rollbacks based
 * on configurable thresholds.
 *
 * @example
 * ```ts
 * import { startCanary, checkCanary, promoteCanary } from '@xergon/sdk';
 *
 * const canary = await startCanary({
 *   model: 'llama-3.3-70b',
 *   canaryModel: 'llama-3.3-70b-ft-v2',
 *   canaryPercentage: 10,
 *   successThreshold: 0.95,
 *   errorThreshold: 0.05,
 *   minRequests: 100,
 *   duration: 60,
 *   autoPromote: true,
 *   autoRollback: true,
 * });
 * ```
 */

import * as crypto from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export interface CanaryConfig {
  model: string;
  canaryModel: string;
  canaryPercentage: number;       // 0-100, percentage of traffic to canary
  successThreshold: number;        // min success rate to promote (0.0-1.0)
  errorThreshold: number;          // max error rate before rollback (0.0-1.0)
  minRequests: number;             // min requests before evaluating (default 100)
  duration: number;                // max canary duration in minutes (default 60)
  autoPromote: boolean;
  autoRollback: boolean;
  metrics?: CanaryMetrics;
}

export interface CanaryMetrics {
  totalRequests: number;
  canaryRequests: number;
  baselineSuccessRate: number;
  canarySuccessRate: number;
  baselineLatencyP50: number;
  canaryLatencyP50: number;
  status: CanaryStatus;
}

export type CanaryStatus = 'running' | 'promoted' | 'rolled_back' | 'expired';

export interface CanaryDeployment extends CanaryConfig {
  id: string;
  startedAt: string;
  endedAt?: string;
  metrics: CanaryMetrics;
}

export interface CanaryCheckResult {
  id: string;
  model: string;
  canaryModel: string;
  status: CanaryStatus;
  metrics: CanaryMetrics;
  recommendation: 'promote' | 'rollback' | 'continue' | 'insufficient_data';
  reason: string;
}

// ── In-Memory Store ─────────────────────────────────────────────────

const activeCanaries = new Map<string, CanaryDeployment>();

function generateId(): string {
  const ts = Date.now().toString(36);
  const rand = Math.random().toString(36).slice(2, 8);
  return `canary_${ts}_${rand}`;
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * Start a canary deployment.
 */
export function startCanary(config: CanaryConfig): CanaryDeployment {
  const id = generateId();

  const deployment: CanaryDeployment = {
    ...config,
    id,
    startedAt: new Date().toISOString(),
    metrics: {
      totalRequests: 0,
      canaryRequests: 0,
      baselineSuccessRate: 0,
      canarySuccessRate: 0,
      baselineLatencyP50: 0,
      canaryLatencyP50: 0,
      status: 'running',
    },
  };

  activeCanaries.set(id, deployment);
  return { ...deployment };
}

/**
 * Check canary status and metrics, including evaluation recommendation.
 */
export function checkCanary(id: string): CanaryCheckResult {
  const deployment = activeCanaries.get(id);
  if (!deployment) {
    throw new Error(`Canary deployment not found: ${id}`);
  }

  const { metrics, successThreshold, errorThreshold, minRequests, duration, startedAt } = deployment;

  // Check if expired
  const elapsed = (Date.now() - new Date(startedAt).getTime()) / 60000;
  if (elapsed >= duration && metrics.status === 'running') {
    metrics.status = 'expired';
    return buildCheckResult(deployment, 'insufficient_data', `Canary expired after ${duration} minutes`);
  }

  // Insufficient data
  if (metrics.totalRequests < minRequests) {
    return buildCheckResult(deployment, 'insufficient_data',
      `Need ${minRequests} requests, have ${metrics.totalRequests}`);
  }

  // Check error threshold (rollback)
  const canaryErrorRate = 1 - metrics.canarySuccessRate;
  if (canaryErrorRate > errorThreshold) {
    return buildCheckResult(deployment, 'rollback',
      `Canary error rate ${(canaryErrorRate * 100).toFixed(1)}% exceeds threshold ${(errorThreshold * 100).toFixed(1)}%`);
  }

  // Check success threshold (promote)
  if (metrics.canarySuccessRate >= successThreshold && metrics.canaryLatencyP50 > 0) {
    return buildCheckResult(deployment, 'promote',
      `Canary success rate ${(metrics.canarySuccessRate * 100).toFixed(1)}% meets threshold ${(successThreshold * 100).toFixed(1)}%`);
  }

  // Check latency regression
  if (metrics.baselineLatencyP50 > 0 && metrics.canaryLatencyP50 > metrics.baselineLatencyP50 * 2) {
    return buildCheckResult(deployment, 'rollback',
      `Canary latency p50 (${metrics.canaryLatencyP50}ms) is 2x+ baseline (${metrics.baselineLatencyP50}ms)`);
  }

  return buildCheckResult(deployment, 'continue', 'Canary is within acceptable thresholds');
}

function buildCheckResult(
  deployment: CanaryDeployment,
  recommendation: CanaryCheckResult['recommendation'],
  reason: string,
): CanaryCheckResult {
  return {
    id: deployment.id,
    model: deployment.model,
    canaryModel: deployment.canaryModel,
    status: deployment.metrics.status,
    metrics: { ...deployment.metrics },
    recommendation,
    reason,
  };
}

/**
 * Manually promote a canary to full deployment.
 */
export function promoteCanary(id: string): CanaryDeployment {
  const deployment = activeCanaries.get(id);
  if (!deployment) {
    throw new Error(`Canary deployment not found: ${id}`);
  }

  deployment.metrics.status = 'promoted';
  deployment.endedAt = new Date().toISOString();
  return { ...deployment };
}

/**
 * Manually rollback a canary to the baseline model.
 */
export function rollbackCanary(id: string): CanaryDeployment {
  const deployment = activeCanaries.get(id);
  if (!deployment) {
    throw new Error(`Canary deployment not found: ${id}`);
  }

  deployment.metrics.status = 'rolled_back';
  deployment.endedAt = new Date().toISOString();
  return { ...deployment };
}

/**
 * List all canary deployments (active and completed).
 */
export function listCanaries(): CanaryDeployment[] {
  return Array.from(activeCanaries.values()).map(d => ({ ...d }));
}

/**
 * Record a request result for a canary (for testing / manual tracking).
 */
export function recordCanaryRequest(
  id: string,
  isCanary: boolean,
  success: boolean,
  latencyMs: number,
): void {
  const deployment = activeCanaries.get(id);
  if (!deployment || deployment.metrics.status !== 'running') return;

  deployment.metrics.totalRequests++;
  if (isCanary) {
    deployment.metrics.canaryRequests++;
  }

  // Recalculate rates (simplified -- uses running average)
  const alpha = 0.1; // exponential moving average smoothing
  if (isCanary) {
    const currentRate = deployment.metrics.canarySuccessRate;
    deployment.metrics.canarySuccessRate = alpha * (success ? 1 : 0) + (1 - alpha) * currentRate;
    const currentLat = deployment.metrics.canaryLatencyP50;
    deployment.metrics.canaryLatencyP50 = alpha * latencyMs + (1 - alpha) * currentLat;
  } else {
    const currentRate = deployment.metrics.baselineSuccessRate;
    deployment.metrics.baselineSuccessRate = alpha * (success ? 1 : 0) + (1 - alpha) * currentRate;
    const currentLat = deployment.metrics.baselineLatencyP50;
    deployment.metrics.baselineLatencyP50 = alpha * latencyMs + (1 - alpha) * currentLat;
  }

  // Auto-evaluate
  if (deployment.metrics.totalRequests >= deployment.minRequests) {
    const check = checkCanary(id);
    if (deployment.autoPromote && check.recommendation === 'promote') {
      promoteCanary(id);
    } else if (deployment.autoRollback && check.recommendation === 'rollback') {
      rollbackCanary(id);
    }
  }
}

// ── History ─────────────────────────────────────────────────────────

const CANARY_HISTORY_FILE = 'canary_history.json';

export interface CanaryHistoryEntry {
  id: string;
  model: string;
  canaryModel: string;
  canaryPercentage: number;
  status: CanaryStatus;
  startedAt: string;
  endedAt?: string;
  metrics: CanaryMetrics;
}

/**
 * Load canary history from local storage.
 */
export function loadCanaryHistory(): CanaryHistoryEntry[] {
  try {
    const raw = fs.readFileSync(path.join(os.homedir(), '.xergon', CANARY_HISTORY_FILE), 'utf-8');
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

/**
 * Save a completed canary to history.
 */
export function saveCanaryToHistory(deployment: CanaryDeployment): void {
  const history = loadCanaryHistory();
  history.push({
    id: deployment.id,
    model: deployment.model,
    canaryModel: deployment.canaryModel,
    canaryPercentage: deployment.canaryPercentage,
    status: deployment.metrics.status,
    startedAt: deployment.startedAt,
    endedAt: deployment.endedAt,
    metrics: deployment.metrics,
  });
  try {
    const dir = path.join(os.homedir(), '.xergon');
    try { fs.mkdirSync(dir, { recursive: true }); } catch {}
    fs.writeFileSync(path.join(dir, CANARY_HISTORY_FILE), JSON.stringify(history, null, 2));
  } catch {
    // History is best-effort
  }
}
