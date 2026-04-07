/**
 * CLI command: monitor
 *
 * Real-time dashboard showing relay health, active providers, current model,
 * recent requests, token usage, GPU stats, and error rate.
 *
 * Usage:
 *   xergon monitor              -- real-time dashboard (auto-refresh every 2s)
 *   xergon monitor --interval 5000  -- custom refresh interval
 *   xergon monitor --no-stream  -- single snapshot
 *   xergon monitor --json       -- machine-readable output
 *
 * Keyboard controls (streaming mode):
 *   q = quit, r = refresh, f = filter, d = toggle details
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ──────────────────────────────────────────────────────────

type StatusLevel = 'ok' | 'warn' | 'err' | 'unknown';

interface RelayHealth {
  connected: boolean;
  latencyMs: number;
  requestRate: number;
  uptime?: string;
  version?: string;
}

interface ProviderStats {
  total: number;
  healthy: number;
  degraded: number;
}

interface ModelInfo {
  name: string;
  provider: string;
  contextWindowUsed: number;
  contextWindowTotal: number;
}

interface RecentRequest {
  id: string;
  model: string;
  latencyMs: number;
  status: 'success' | 'error' | 'streaming';
  tokensUsed?: number;
  timestamp: Date;
}

interface TokenUsage {
  totalTokens: number;
  tokensPerMin: number;
  promptTokens: number;
  completionTokens: number;
}

interface GpuStats {
  connected: boolean;
  vramUsedGB: number;
  vramTotalGB: number;
  utilizationPct: number;
  temperatureC?: number;
}

interface ErrorRate {
  totalErrors: number;
  errorRatePct: number;
  lastFiveMinErrors: number;
  lastFiveMinTotal: number;
}

interface MonitorSnapshot {
  timestamp: string;
  relay: RelayHealth;
  providers: ProviderStats;
  model: ModelInfo;
  recentRequests: RecentRequest[];
  tokenUsage: TokenUsage;
  gpu: GpuStats;
  errorRate: ErrorRate;
}

// ── ANSI helpers ───────────────────────────────────────────────────

const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';
const DIM = '\x1b[2m';
const CYAN = '\x1b[36m';
const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const BLUE = '\x1b[34m';
const MAGENTA = '\x1b[35m';
const WHITE = '\x1b[37m';
const BG_GREEN = '\x1b[42m';
const BG_RED = '\x1b[41m';
const BG_YELLOW = '\x1b[43m';

function c(text: string, ...codes: string[]): string {
  return codes.join('') + text + RESET;
}

function clearScreen(): void {
  process.stdout.write('\x1b[2J\x1b[H');
}

function statusIcon(level: StatusLevel): string {
  switch (level) {
    case 'ok': return c(' ● ', BG_GREEN, BLACK);
    case 'warn': return c(' ● ', BG_YELLOW, BLACK);
    case 'err': return c(' ● ', BG_RED, WHITE);
    default: return c(' ● ', DIM, WHITE);
  }
}

const BLACK = '\x1b[30m';

// ── Data fetchers ──────────────────────────────────────────────────

async function fetchRelayHealth(baseUrl: string, apiKey: string): Promise<RelayHealth> {
  const start = Date.now();
  try {
    const headers: Record<string, string> = {};
    if (apiKey) headers['X-Xergon-Public-Key'] = apiKey;

    const res = await fetch(`${baseUrl.replace(/\/+$/, '')}/health`, {
      headers,
      signal: AbortSignal.timeout(5000),
    });
    const latencyMs = Date.now() - start;

    if (res.ok) {
      const body: any = await res.json().catch(() => ({}));
      return {
        connected: true,
        latencyMs,
        requestRate: body.requestRate ?? 0,
        uptime: body.uptime ?? undefined,
        version: body.version ?? body.v ?? undefined,
      };
    }

    return { connected: false, latencyMs, requestRate: 0 };
  } catch {
    return { connected: false, latencyMs: Date.now() - start, requestRate: 0 };
  }
}

async function fetchProviders(baseUrl: string): Promise<ProviderStats> {
  try {
    const res = await fetch(`${baseUrl.replace(/\/+$/, '')}/v1/providers`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!res.ok) return { total: 0, healthy: 0, degraded: 0 };

    const data: any = await res.json();
    let providers: any[] = [];
    if (Array.isArray(data)) providers = data;
    else if (data.data && Array.isArray(data.data)) providers = data.data;
    else if (data.providers && Array.isArray(data.providers)) providers = data.providers;

    const healthy = providers.filter(
      (p: any) => p.status === 'active' || p.status === 'healthy' || p.healthy === true
    ).length;

    return { total: providers.length, healthy, degraded: providers.length - healthy };
  } catch {
    return { total: 0, healthy: 0, degraded: 0 };
  }
}

async function fetchModels(baseUrl: string): Promise<ModelInfo> {
  try {
    const res = await fetch(`${baseUrl.replace(/\/+$/, '')}/v1/models`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!res.ok) {
      return { name: 'unknown', provider: 'unknown', contextWindowUsed: 0, contextWindowTotal: 0 };
    }

    const data: any = await res.json();
    let models: any[] = [];
    if (Array.isArray(data)) models = data;
    else if (data.data && Array.isArray(data.data)) models = data.data;

    const first = models[0];
    return {
      name: first?.id ?? first?.name ?? 'none',
      provider: first?.provider ?? first?.owned_by ?? 'relay',
      contextWindowUsed: 0,
      contextWindowTotal: first?.context_length ?? first?.contextWindow ?? 0,
    };
  } catch {
    return { name: 'unknown', provider: 'unknown', contextWindowUsed: 0, contextWindowTotal: 0 };
  }
}

function loadLocalStats(): TokenUsage {
  let totalTokens = 0;
  let promptTokens = 0;
  let completionTokens = 0;

  try {
    const statsPath = path.join(os.homedir(), '.xergon', 'session-stats.json');
    const data = fs.readFileSync(statsPath, 'utf-8');
    const stats = JSON.parse(data);
    totalTokens = stats.totalTokens ?? 0;
    promptTokens = stats.promptTokens ?? 0;
    completionTokens = stats.completionTokens ?? 0;
  } catch {
    // No session stats yet
  }

  return {
    totalTokens,
    tokensPerMin: 0,
    promptTokens,
    completionTokens,
  };
}

function loadGpuStats(): GpuStats {
  try {
    const gpuPath = path.join(os.homedir(), '.xergon', 'gpu-stats.json');
    const data = fs.readFileSync(gpuPath, 'utf-8');
    const stats = JSON.parse(data);
    return {
      connected: true,
      vramUsedGB: stats.vramUsedGB ?? 0,
      vramTotalGB: stats.vramTotalGB ?? 0,
      utilizationPct: stats.utilizationPct ?? 0,
      temperatureC: stats.temperatureC,
    };
  } catch {
    return { connected: false, vramUsedGB: 0, vramTotalGB: 0, utilizationPct: 0 };
  }
}

function loadErrorStats(): ErrorRate {
  try {
    const errPath = path.join(os.homedir(), '.xergon', 'error-stats.json');
    const data = fs.readFileSync(errPath, 'utf-8');
    const stats = JSON.parse(data);
    return {
      totalErrors: stats.totalErrors ?? 0,
      errorRatePct: stats.errorRatePct ?? 0,
      lastFiveMinErrors: stats.lastFiveMinErrors ?? 0,
      lastFiveMinTotal: stats.lastFiveMinTotal ?? 0,
    };
  } catch {
    return { totalErrors: 0, errorRatePct: 0, lastFiveMinErrors: 0, lastFiveMinTotal: 0 };
  }
}

// ── Dashboard renderer ─────────────────────────────────────────────

interface DashboardState {
  filter: string;
  showDetails: boolean;
  tick: number;
}

function renderDashboard(
  snapshot: MonitorSnapshot,
  state: DashboardState,
  terminalWidth: number,
  config: { baseUrl: string; defaultModel: string },
): string {
  const w = terminalWidth;
  const sep = c('─'.repeat(Math.min(w - 4, 72)), DIM);

  const lines: string[] = [];

  // Header
  lines.push(c('  XERGON MONITOR', BOLD, CYAN) + ' '.repeat(Math.max(0, w - 24)) +
    c(new Date().toLocaleTimeString(), DIM));
  lines.push(sep);

  // ── Relay Health ──
  const relayStatus = snapshot.relay.connected ? 'ok' : 'err';
  lines.push('  ' + statusIcon(relayStatus) + ' ' + c('RELAY', BOLD) +
    ' '.repeat(Math.max(0, 50)) +
    c(snapshot.relay.connected ? 'CONNECTED' : 'DISCONNECTED', snapshot.relay.connected ? GREEN : RED));
  lines.push('    Latency: ' + c(`${snapshot.relay.latencyMs}ms`, snapshot.relay.latencyMs < 200 ? GREEN : snapshot.relay.latencyMs < 1000 ? YELLOW : RED) +
    '   Rate: ' + c(`${snapshot.relay.requestRate.toFixed(1)} req/s`, CYAN) +
    (snapshot.relay.version ? '   Version: ' + c(snapshot.relay.version, DIM) : ''));
  lines.push('');

  // ── Providers ──
  const providerStatus = snapshot.providers.total > 0 ? 'ok' : 'warn';
  lines.push('  ' + statusIcon(providerStatus) + ' ' + c('PROVIDERS', BOLD));
  lines.push('    Total: ' + c(String(snapshot.providers.total), WHITE) +
    '   Healthy: ' + c(String(snapshot.providers.healthy), GREEN) +
    '   Degraded: ' + c(String(snapshot.providers.degraded), snapshot.providers.degraded > 0 ? YELLOW : DIM));
  lines.push('');

  // ── Current Model ──
  lines.push('  ' + c('●', BLUE) + ' ' + c('MODEL', BOLD));
  lines.push('    ' + c(snapshot.model.name, CYAN) +
    '   Provider: ' + c(snapshot.model.provider, DIM) +
    (snapshot.model.contextWindowTotal > 0 ? `   Context: ${snapshot.model.contextWindowTotal}` : ''));
  lines.push('');

  // ── Token Usage ──
  lines.push('  ' + c('●', MAGENTA) + ' ' + c('TOKENS', BOLD));
  lines.push('    Session: ' + c(String(snapshot.tokenUsage.totalTokens), WHITE) +
    '   Rate: ' + c(`${snapshot.tokenUsage.tokensPerMin.toFixed(0)} tok/min`, CYAN));
  if (state.showDetails) {
    lines.push('    Prompt: ' + c(String(snapshot.tokenUsage.promptTokens), DIM) +
      '   Completion: ' + c(String(snapshot.tokenUsage.completionTokens), DIM));
  }
  lines.push('');

  // ── GPU Stats ──
  const gpuStatus = snapshot.gpu.connected ? 'ok' : 'unknown';
  lines.push('  ' + statusIcon(gpuStatus) + ' ' + c('GPU', BOLD));
  if (snapshot.gpu.connected) {
    const vramPct = snapshot.gpu.vramTotalGB > 0
      ? ((snapshot.gpu.vramUsedGB / snapshot.gpu.vramTotalGB) * 100).toFixed(0)
      : '0';
    lines.push('    VRAM: ' + c(`${snapshot.gpu.vramUsedGB.toFixed(1)}/${snapshot.gpu.vramTotalGB.toFixed(1)} GB`, WHITE) +
      ' (' + c(`${vramPct}%`, Number(vramPct) > 80 ? RED : YELLOW) + ')' +
      '   Util: ' + c(`${snapshot.gpu.utilizationPct.toFixed(0)}%`, snapshot.gpu.utilizationPct > 80 ? RED : GREEN) +
      (snapshot.gpu.temperatureC !== undefined ? `   Temp: ${c(`${snapshot.gpu.temperatureC}°C`, snapshot.gpu.temperatureC > 80 ? RED : GREEN)}` : ''));
  } else {
    lines.push('    ' + c('Not connected', DIM));
  }
  lines.push('');

  // ── Error Rate ──
  const errStatus = snapshot.errorRate.errorRatePct > 10 ? 'err' : snapshot.errorRate.errorRatePct > 0 ? 'warn' : 'ok';
  lines.push('  ' + statusIcon(errStatus) + ' ' + c('ERRORS', BOLD));
  lines.push('    Rate (5m): ' + c(`${snapshot.errorRate.errorRatePct.toFixed(1)}%`, errStatus === 'ok' ? GREEN : errStatus === 'warn' ? YELLOW : RED) +
    '   Errors (5m): ' + c(`${snapshot.errorRate.lastFiveMinErrors}/${snapshot.errorRate.lastFiveMinTotal}`, WHITE));
  lines.push('');

  // ── Recent Requests ──
  lines.push(sep);
  lines.push('  ' + c('RECENT REQUESTS', BOLD) + (state.filter ? ' ' + c(`[filter: ${state.filter}]`, YELLOW) : ''));
  lines.push('');

  const filteredReqs = state.filter
    ? snapshot.recentRequests.filter(r => r.model.includes(state.filter) || r.status.includes(state.filter))
    : snapshot.recentRequests;

  const maxReqs = Math.min(filteredReqs.length, 8);
  for (let i = 0; i < maxReqs; i++) {
    const req = filteredReqs[i];
    const statusColor = req.status === 'success' ? GREEN : req.status === 'streaming' ? CYAN : RED;
    const time = req.timestamp.toLocaleTimeString();
    lines.push('    ' + c(time, DIM) + '  ' +
      c(req.model.substring(0, 20).padEnd(20), WHITE) + '  ' +
      c(`${String(req.latencyMs).padStart(5)}ms`, req.latencyMs < 200 ? GREEN : req.latencyMs < 1000 ? YELLOW : RED) +
      '  ' + c(req.status.toUpperCase().padEnd(9), statusColor));
  }

  if (filteredReqs.length === 0) {
    lines.push('    ' + c('No recent requests', DIM));
  }

  lines.push('');
  lines.push(sep);

  // Controls footer
  lines.push('  ' + c('[q]', BOLD) + ' quit  ' +
    c('[r]', BOLD) + ' refresh  ' +
    c('[f]', BOLD) + ' filter  ' +
    c('[d]', BOLD) + ' details  ' +
    '  ' + c(`Interval: ${2000}ms`, DIM));

  return lines.join('\n');
}

// ── Collect snapshot ───────────────────────────────────────────────

async function collectSnapshot(baseUrl: string, apiKey: string, defaultModel: string): Promise<MonitorSnapshot> {
  const [relay, providers, model, tokenUsage, gpu, errorRate] = await Promise.all([
    fetchRelayHealth(baseUrl, apiKey),
    fetchProviders(baseUrl),
    fetchModels(baseUrl),
    Promise.resolve(loadLocalStats()),
    Promise.resolve(loadGpuStats()),
    Promise.resolve(loadErrorStats()),
  ]);

  return {
    timestamp: new Date().toISOString(),
    relay,
    providers,
    model: { ...model, name: defaultModel || model.name },
    recentRequests: [],
    tokenUsage,
    gpu,
    errorRate,
  };
}

// ── Options ────────────────────────────────────────────────────────

const monitorOptions: CommandOption[] = [
  {
    name: 'interval',
    short: '',
    long: '--interval',
    description: 'Refresh interval in milliseconds (default: 2000)',
    required: false,
    type: 'number',
  },
  {
    name: 'noStream',
    short: '',
    long: '--no-stream',
    description: 'Show a single snapshot instead of live dashboard',
    required: false,
    type: 'boolean',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
];

// ── Streaming dashboard (raw mode) ─────────────────────────────────

async function runStreamingDashboard(
  baseUrl: string,
  apiKey: string,
  defaultModel: string,
  intervalMs: number,
): Promise<void> {
  const state: DashboardState = { filter: '', showDetails: false, tick: 0 };

  // Enter raw mode
  if (process.stdin.isTTY) {
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.setEncoding('utf-8');
  }

  let running = true;

  // Handle key presses
  const onKey = (key: string) => {
    if (key === 'q' || key === '\x03') { // q or Ctrl+C
      running = false;
      return;
    }
    if (key === 'r') {
      state.tick++;
      return;
    }
    if (key === 'd') {
      state.showDetails = !state.showDetails;
      state.tick++;
      return;
    }
    if (key === 'f') {
      // Simple filter toggle: cycle through common filters
      const filters = ['', 'success', 'error', 'streaming'];
      const currentIdx = filters.indexOf(state.filter);
      state.filter = filters[(currentIdx + 1) % filters.length];
      state.tick++;
    }
  };

  process.stdin.on('data', onKey);

  // Get terminal width
  const termWidth = process.stdout.columns ?? 80;

  try {
    while (running) {
      const snapshot = await collectSnapshot(baseUrl, apiKey, defaultModel);
      clearScreen();
      process.stdout.write(renderDashboard(snapshot, state, termWidth, { baseUrl, defaultModel }));

      // Wait for interval or key press
      await new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, intervalMs);
        // Check if stopped during wait
        const check = setInterval(() => {
          if (!running) {
            clearTimeout(timer);
            clearInterval(check);
            resolve();
          }
        }, 100);
      });
    }
  } finally {
    // Restore terminal
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(false);
      process.stdin.pause();
    }
    process.stdin.removeListener('data', onKey);
    clearScreen();
    process.stdout.write(c('Monitor stopped.', DIM) + '\n');
  }
}

// ── Command action ─────────────────────────────────────────────────

async function monitorAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const noStream = args.options.noStream === true;
  const intervalMs = Number(args.options.interval) || 2000;

  const baseUrl = ctx.config.baseUrl;
  const apiKey = ctx.config.apiKey;
  const defaultModel = ctx.config.defaultModel;

  if (outputJson) {
    // Single snapshot as JSON
    const snapshot = await collectSnapshot(baseUrl, apiKey, defaultModel);
    ctx.output.write(JSON.stringify(snapshot, null, 2));
    return;
  }

  if (noStream) {
    // Single snapshot, text mode
    const snapshot = await collectSnapshot(baseUrl, apiKey, defaultModel);
    const termWidth = process.stdout.columns ?? 80;
    const state: DashboardState = { filter: '', showDetails: true, tick: 0 };
    ctx.output.write(renderDashboard(snapshot, state, termWidth, { baseUrl, defaultModel }));
    return;
  }

  // Streaming dashboard
  await runStreamingDashboard(baseUrl, apiKey, defaultModel, intervalMs);
}

// ── Command definition ─────────────────────────────────────────────

export const monitorCommand: Command = {
  name: 'monitor',
  description: 'Real-time dashboard for relay health, providers, and usage',
  aliases: ['dash', 'dashboard'],
  options: monitorOptions,
  action: monitorAction,
};
