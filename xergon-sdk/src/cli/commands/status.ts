/**
 * CLI command: status
 *
 * Comprehensive system status for the Xergon Network agent and relay.
 *
 * Usage:
 *   xergon status              -- overall status (default)
 *   xergon status providers    -- list active providers with health, score, latency
 *   xergon status models       -- list models being served with request counts, GPU usage
 *   xergon status network      -- network stats (peers, block height, sync, relays)
 *   xergon status shards       -- model shard distribution across GPUs
 *   xergon status --json       -- machine-readable output
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ──────────────────────────────────────────────────────────

type HealthLevel = 'healthy' | 'degraded' | 'error' | 'unknown';

interface StatusCheck {
  name: string;
  status: HealthLevel;
  detail: string;
  latencyMs?: number;
}

interface AgentStatus {
  ponwScore: number;
  ergBalance: string;
  modelsServing: number;
  uptime: string;
  connectedRelays: number;
  agentVersion: string;
  agentUrl: string;
  checks: StatusCheck[];
  summary: { healthy: number; degraded: number; error: number };
}

interface ProviderEntry {
  id: string;
  address: string;
  health: HealthLevel;
  score: number;
  latencyMs: number;
  models: number;
  status: string;
}

interface ModelEntry {
  id: string;
  name: string;
  requests: number;
  gpuUsagePct: number;
  vramUsedGB: number;
  vramTotalGB: number;
  provider: string;
  status: string;
}

interface NetworkStats {
  peers: number;
  blockHeight: number;
  syncStatus: 'synced' | 'syncing' | 'behind' | 'unknown';
  syncProgress: number;
  connectedRelays: number;
  relayLatencyMs: number;
  networkUptime: string;
}

interface ShardEntry {
  modelId: string;
  shardIndex: number;
  totalShards: number;
  gpuId: string;
  gpuType: string;
  vramUsedGB: number;
  vramTotalGB: number;
  status: HealthLevel;
}

// ── Constants ──────────────────────────────────────────────────────

const DEFAULT_AGENT_URL = 'http://127.0.0.1:9099';
const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');
const CONFIG_FILE = () => path.join(CONFIG_DIR(), 'config.json');

// ── Options ────────────────────────────────────────────────────────

const statusOptions: CommandOption[] = [
  {
    name: 'agentUrl',
    short: '',
    long: '--agent-url',
    description: 'Agent REST API URL (default: http://127.0.0.1:9099)',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '-j',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
  {
    name: 'verbose',
    short: '-v',
    long: '--verbose',
    description: 'Show additional detail',
    required: false,
    type: 'boolean',
  },
];

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Safely fetch JSON from an endpoint with timeout.
 */
async function fetchJSON<T>(url: string, timeoutMs: number = 10_000): Promise<T | null> {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
    if (!res.ok) return null;
    return await res.json() as T;
  } catch {
    return null;
  }
}

/**
 * Measure latency to a URL.
 */
async function measureLatency(url: string, timeoutMs: number = 5_000): Promise<number> {
  const start = Date.now();
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
    return Date.now() - start;
  } catch {
    return -1;
  }
}

/**
 * Load the agent URL from config file or use default.
 */
function resolveAgentUrl(configUrl?: string): string {
  if (configUrl) return configUrl.replace(/\/+$/, '');

  try {
    const data = fs.readFileSync(CONFIG_FILE(), 'utf-8');
    const parsed = JSON.parse(data);
    if (parsed.agentUrl) return String(parsed.agentUrl).replace(/\/+$/, '');
  } catch {
    // No config file
  }

  return DEFAULT_AGENT_URL;
}

/**
 * Map a health string from an API to a HealthLevel.
 */
function toHealthLevel(status?: string, healthy?: boolean): HealthLevel {
  if (healthy === true) return 'healthy';
  if (healthy === false) return 'error';
  if (!status) return 'unknown';
  const s = status.toLowerCase();
  if (s === 'healthy' || s === 'active' || s === 'online' || s === 'ok') return 'healthy';
  if (s === 'degraded' || s === 'warning' || s === 'slow') return 'degraded';
  if (s === 'error' || s === 'offline' || s === 'down' || s === 'unhealthy') return 'error';
  return 'unknown';
}

/**
 * Format uptime from seconds to human-readable.
 */
function formatUptime(seconds: number): string {
  if (seconds < 0) return 'unknown';
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

// ── Data Fetchers ──────────────────────────────────────────────────

/**
 * Fetch agent overview status.
 */
async function fetchAgentOverview(agentUrl: string): Promise<Partial<AgentStatus>> {
  const data: any = await fetchJSON(`${agentUrl}/api/v1/status`);
  if (!data) return {};

  return {
    ponwScore: data.ponwScore ?? data.ponw_score ?? data.score ?? 0,
    ergBalance: String(data.ergBalance ?? data.erg_balance ?? data.balance ?? '0'),
    modelsServing: data.modelsServing ?? data.models_serving ?? data.servingModels ?? 0,
    uptime: data.uptime ?? data.uptimeSeconds
      ? formatUptime(Number(data.uptime ?? data.uptimeSeconds))
      : 'unknown',
    connectedRelays: data.connectedRelays ?? data.connected_relays ?? data.relays ?? 0,
    agentVersion: data.version ?? data.agentVersion ?? data.v ?? 'unknown',
    agentUrl,
  };
}

/**
 * Fetch relay health.
 */
async function fetchRelayHealth(baseUrl: string): Promise<{ connected: boolean; latencyMs: number; version?: string }> {
  const start = Date.now();
  try {
    const res = await fetch(`${baseUrl.replace(/\/+$/, '')}/health`, {
      signal: AbortSignal.timeout(5_000),
    });
    const latencyMs = Date.now() - start;
    if (res.ok) {
      const body: any = await res.json().catch(() => ({}));
      return { connected: true, latencyMs, version: body.version ?? body.v };
    }
    return { connected: false, latencyMs };
  } catch {
    return { connected: false, latencyMs: Date.now() - start };
  }
}

/**
 * Fetch providers list from agent API.
 */
async function fetchProviders(agentUrl: string): Promise<ProviderEntry[]> {
  const data: any = await fetchJSON(`${agentUrl}/api/v1/providers`);
  if (!data) return [];

  let providers: any[] = [];
  if (Array.isArray(data)) providers = data;
  else if (data.data && Array.isArray(data.data)) providers = data.data;
  else if (data.providers && Array.isArray(data.providers)) providers = data.providers;

  return providers.map((p: any) => ({
    id: p.id ?? p.providerId ?? p.nodeId ?? 'unknown',
    address: p.address ?? p.nodeAddress ?? p.peerId ?? 'unknown',
    health: toHealthLevel(p.status, p.healthy),
    score: Number(p.score ?? p.ponwScore ?? p.reputation ?? 0),
    latencyMs: Number(p.latencyMs ?? p.latency ?? 0),
    models: Number(p.models ?? p.modelCount ?? 0),
    status: p.status ?? p.state ?? 'unknown',
  }));
}

/**
 * Fetch models list from agent API.
 */
async function fetchModels(agentUrl: string): Promise<ModelEntry[]> {
  const data: any = await fetchJSON(`${agentUrl}/api/v1/models`);
  if (!data) return [];

  let models: any[] = [];
  if (Array.isArray(data)) models = data;
  else if (data.data && Array.isArray(data.data)) models = data.data;
  else if (data.models && Array.isArray(data.models)) models = data.models;

  return models.map((m: any) => ({
    id: m.id ?? m.modelId ?? 'unknown',
    name: m.name ?? m.id ?? 'unknown',
    requests: Number(m.requests ?? m.requestCount ?? m.totalRequests ?? 0),
    gpuUsagePct: Number(m.gpuUsagePct ?? m.gpuUsage ?? m.gpu_utilization ?? 0),
    vramUsedGB: Number(m.vramUsedGB ?? m.vramUsed ?? m.vram_used_gb ?? 0),
    vramTotalGB: Number(m.vramTotalGB ?? m.vramTotal ?? m.vram_total_gb ?? 0),
    provider: m.provider ?? m.ownedBy ?? m.providerId ?? 'local',
    status: m.status ?? m.state ?? 'active',
  }));
}

/**
 * Fetch network stats from agent API.
 */
async function fetchNetworkStats(agentUrl: string): Promise<Partial<NetworkStats>> {
  const data: any = await fetchJSON(`${agentUrl}/api/v1/network`);
  if (!data) return {};

  const syncStatus = data.syncStatus ?? data.sync_status ?? data.sync ?? 'unknown';
  let sync: 'synced' | 'syncing' | 'behind' | 'unknown' = 'unknown';
  if (typeof syncStatus === 'boolean') {
    sync = syncStatus ? 'synced' : 'syncing';
  } else {
    const s = String(syncStatus).toLowerCase();
    if (s === 'synced' || s === 'true' || s === 'ok' || s === 'synced') sync = 'synced';
    else if (s === 'syncing' || s === 'catching_up') sync = 'syncing';
    else if (s === 'behind' || s === 'stale') sync = 'behind';
  }

  return {
    peers: Number(data.peers ?? data.peerCount ?? 0),
    blockHeight: Number(data.blockHeight ?? data.block_height ?? data.height ?? 0),
    syncStatus: sync,
    syncProgress: Number(data.syncProgress ?? data.sync_progress ?? 0),
    connectedRelays: Number(data.connectedRelays ?? data.relays ?? 0),
    relayLatencyMs: Number(data.relayLatencyMs ?? data.relayLatency ?? 0),
    networkUptime: data.uptime ? formatUptime(Number(data.uptime)) : 'unknown',
  };
}

/**
 * Fetch shard distribution from agent API.
 */
async function fetchShards(agentUrl: string): Promise<ShardEntry[]> {
  const data: any = await fetchJSON(`${agentUrl}/api/v1/shards`);
  if (!data) return [];

  let shards: any[] = [];
  if (Array.isArray(data)) shards = data;
  else if (data.data && Array.isArray(data.data)) shards = data.data;
  else if (data.shards && Array.isArray(data.shards)) shards = data.shards;

  return shards.map((s: any) => ({
    modelId: s.modelId ?? s.model_id ?? s.model ?? 'unknown',
    shardIndex: Number(s.shardIndex ?? s.shard_index ?? s.index ?? 0),
    totalShards: Number(s.totalShards ?? s.total_shards ?? 1),
    gpuId: s.gpuId ?? s.gpu_id ?? s.gpu ?? 'unknown',
    gpuType: s.gpuType ?? s.gpu_type ?? s.device ?? 'unknown',
    vramUsedGB: Number(s.vramUsedGB ?? s.vram_used_gb ?? 0),
    vramTotalGB: Number(s.vramTotalGB ?? s.vram_total_gb ?? 0),
    status: toHealthLevel(s.status, s.healthy),
  }));
}

// ── Health Checks ──────────────────────────────────────────────────

async function runHealthChecks(agentUrl: string, baseUrl: string, apiKey: string): Promise<StatusCheck[]> {
  const checks: StatusCheck[] = [];

  // Agent connectivity
  const agentLatency = await measureLatency(`${agentUrl}/health`);
  if (agentLatency >= 0) {
    checks.push({
      name: 'Agent',
      status: agentLatency < 500 ? 'healthy' : 'degraded',
      detail: `Online (${agentLatency}ms)`,
      latencyMs: agentLatency,
    });
  } else {
    checks.push({
      name: 'Agent',
      status: 'error',
      detail: `Cannot reach agent at ${agentUrl}. Is it running?`,
    });
  }

  // Relay connectivity
  const relay = await fetchRelayHealth(baseUrl);
  if (relay.connected) {
    checks.push({
      name: 'Relay',
      status: relay.latencyMs < 500 ? 'healthy' : 'degraded',
      detail: `Online (${relay.latencyMs}ms)${relay.version ? ` v${relay.version}` : ''}`,
      latencyMs: relay.latencyMs,
    });
  } else {
    checks.push({
      name: 'Relay',
      status: 'error',
      detail: `Cannot reach relay at ${baseUrl}`,
    });
  }

  // Wallet / API Key
  if (apiKey) {
    const masked = apiKey.length > 16
      ? `${apiKey.substring(0, 8)}...${apiKey.substring(apiKey.length - 8)}`
      : `${apiKey.substring(0, 8)}...`;
    checks.push({
      name: 'Wallet',
      status: apiKey.length >= 20 ? 'healthy' : 'degraded',
      detail: `Key configured (${masked})`,
    });
  } else {
    checks.push({
      name: 'Wallet',
      status: 'error',
      detail: 'No public key / API key configured',
    });
  }

  // Providers
  const providers = await fetchProviders(agentUrl);
  const healthyProviders = providers.filter(p => p.health === 'healthy').length;
  checks.push({
    name: 'Providers',
    status: healthyProviders > 0 ? 'healthy' : providers.length > 0 ? 'degraded' : 'error',
    detail: `${healthyProviders}/${providers.length} healthy`,
  });

  // Models
  const models = await fetchModels(agentUrl);
  checks.push({
    name: 'Models',
    status: models.length > 0 ? 'healthy' : 'degraded',
    detail: `${models.length} model(s) serving`,
  });

  return checks;
}

// ── Output Rendering ───────────────────────────────────────────────

const STATUS_ICONS: Record<HealthLevel, string> = {
  healthy: '\x1b[32m\x1b[1m  OK \x1b[0m',
  degraded: '\x1b[33m\x1b[1m WARN\x1b[0m',
  error: '\x1b[31m\x1b[1m ERR \x1b[0m',
  unknown: '\x1b[2m  ?  \x1b[0m',
};

function renderStatusIcon(status: HealthLevel): string {
  return STATUS_ICONS[status] ?? STATUS_ICONS.unknown;
}

function renderDefaultStatus(agent: AgentStatus, output: any, verbose: boolean): string {
  const lines: string[] = [];

  lines.push(output.colorize('Xergon Agent Status', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(56), 'dim'));
  lines.push('');

  // Agent info
  lines.push(`  ${output.colorize('Agent Version:'.padEnd(22), 'cyan')} ${agent.agentVersion}`);
  lines.push(`  ${output.colorize('Agent URL:'.padEnd(22), 'cyan')} ${agent.agentUrl}`);
  lines.push(`  ${output.colorize('PoNW Score:'.padEnd(22), 'cyan')} ${output.colorize(String(agent.ponwScore), agent.ponwScore > 50 ? 'green' : 'yellow')}`);
  lines.push(`  ${output.colorize('ERG Balance:'.padEnd(22), 'cyan')} ${agent.ergBalance} ERG`);
  lines.push(`  ${output.colorize('Models Serving:'.padEnd(22), 'cyan')} ${agent.modelsServing}`);
  lines.push(`  ${output.colorize('Uptime:'.padEnd(22), 'cyan')} ${agent.uptime}`);
  lines.push(`  ${output.colorize('Connected Relays:'.padEnd(22), 'cyan')} ${agent.connectedRelays}`);
  lines.push('');

  // Health checks
  lines.push(output.colorize('  Health Checks', 'bold'));
  lines.push(output.colorize('  ' + '\u2500'.repeat(52), 'dim'));

  for (const check of agent.checks) {
    const icon = renderStatusIcon(check.status);
    const detail = check.status === 'healthy'
      ? check.detail
      : output.colorize(check.detail, check.status === 'error' ? 'red' : 'yellow');
    lines.push(`    ${check.name.padEnd(14)} ${icon}  ${detail}`);
  }

  lines.push('');

  // Summary
  const parts: string[] = [];
  if (agent.summary.healthy > 0) parts.push(output.colorize(`${agent.summary.healthy} OK`, 'green'));
  if (agent.summary.degraded > 0) parts.push(output.colorize(`${agent.summary.degraded} WARN`, 'yellow'));
  if (agent.summary.error > 0) parts.push(output.colorize(`${agent.summary.error} ERR`, 'red'));
  lines.push(`  Summary: ${parts.join('  |  ')}`);
  lines.push('');

  return lines.join('\n');
}

function renderProvidersTable(providers: ProviderEntry[], output: any): string {
  const lines: string[] = [];

  lines.push(output.colorize('Active Providers', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(72), 'dim'));
  lines.push('');

  if (providers.length === 0) {
    lines.push('  No active providers found.');
    lines.push('');
    return lines.join('\n');
  }

  // Column widths
  const idW = Math.max(8, ...providers.map(p => p.id.length));
  const addrW = Math.max(10, ...providers.map(p => p.address.length));
  const statusW = 10;
  const scoreW = 8;
  const latW = 10;
  const modelW = 8;

  // Header
  const header = '  ' +
    output.colorize('ID'.padEnd(idW), 'bold') + '  ' +
    output.colorize('Address'.padEnd(addrW), 'bold') + '  ' +
    output.colorize('Health'.padEnd(statusW), 'bold') + '  ' +
    output.colorize('Score'.padEnd(scoreW), 'bold') + '  ' +
    output.colorize('Latency'.padEnd(latW), 'bold') + '  ' +
    output.colorize('Models'.padEnd(modelW), 'bold');
  lines.push(header);
  lines.push('  ' + '\u2500'.repeat(idW + addrW + statusW + scoreW + latW + modelW + 15));

  for (const p of providers) {
    const healthColor = p.health === 'healthy' ? 'green' : p.health === 'degraded' ? 'yellow' : 'red';
    const latColor = p.latencyMs < 200 ? 'green' : p.latencyMs < 1000 ? 'yellow' : 'red';

    lines.push(
      '  ' +
      p.id.substring(0, idW).padEnd(idW) + '  ' +
      p.address.substring(0, addrW).padEnd(addrW) + '  ' +
      output.colorize(p.health.toUpperCase().padEnd(statusW), healthColor) + '  ' +
      output.colorize(String(p.score).padEnd(scoreW), 'yellow') + '  ' +
      output.colorize(`${p.latencyMs}ms`.padEnd(latW), latColor) + '  ' +
      String(p.models).padEnd(modelW)
    );
  }

  lines.push('');
  lines.push(output.colorize(`  ${providers.length} provider(s)`, 'dim'));
  lines.push('');
  return lines.join('\n');
}

function renderModelsTable(models: ModelEntry[], output: any): string {
  const lines: string[] = [];

  lines.push(output.colorize('Models Being Served', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(72), 'dim'));
  lines.push('');

  if (models.length === 0) {
    lines.push('  No models currently being served.');
    lines.push('');
    return lines.join('\n');
  }

  const nameW = Math.max(8, ...models.map(m => m.name.length));
  const provW = Math.max(8, ...models.map(m => m.provider.length));
  const reqW = 10;
  const gpuW = 10;
  const vramW = 18;
  const statW = 10;

  const header = '  ' +
    output.colorize('Model'.padEnd(nameW), 'bold') + '  ' +
    output.colorize('Provider'.padEnd(provW), 'bold') + '  ' +
    output.colorize('Requests'.padEnd(reqW), 'bold') + '  ' +
    output.colorize('GPU %'.padEnd(gpuW), 'bold') + '  ' +
    output.colorize('VRAM'.padEnd(vramW), 'bold') + '  ' +
    output.colorize('Status'.padEnd(statW), 'bold');
  lines.push(header);
  lines.push('  ' + '\u2500'.repeat(nameW + provW + reqW + gpuW + vramW + statW + 15));

  for (const m of models) {
    const gpuColor = m.gpuUsagePct > 80 ? 'red' : m.gpuUsagePct > 50 ? 'yellow' : 'green';
    const statusColor = m.status === 'active' ? 'green' : m.status === 'loading' ? 'yellow' : 'dim';
    const vramStr = m.vramTotalGB > 0
      ? `${m.vramUsedGB.toFixed(1)}/${m.vramTotalGB.toFixed(1)} GB`
      : `${m.vramUsedGB.toFixed(1)} GB`;

    lines.push(
      '  ' +
      m.name.substring(0, nameW).padEnd(nameW) + '  ' +
      m.provider.substring(0, provW).padEnd(provW) + '  ' +
      String(m.requests).padEnd(reqW) + '  ' +
      output.colorize(`${m.gpuUsagePct.toFixed(0)}%`.padEnd(gpuW), gpuColor) + '  ' +
      output.colorize(vramStr.padEnd(vramW), 'cyan') + '  ' +
      output.colorize(m.status.padEnd(statW), statusColor)
    );
  }

  lines.push('');
  lines.push(output.colorize(`  ${models.length} model(s)`, 'dim'));
  lines.push('');
  return lines.join('\n');
}

function renderNetworkStats(stats: NetworkStats, output: any): string {
  const lines: string[] = [];

  lines.push(output.colorize('Network Statistics', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(56), 'dim'));
  lines.push('');

  // Sync status
  const syncColor = stats.syncStatus === 'synced' ? 'green'
    : stats.syncStatus === 'syncing' ? 'yellow'
    : stats.syncStatus === 'behind' ? 'red' : 'dim';
  const syncIcon = stats.syncStatus === 'synced' ? '\u25cf' : stats.syncStatus === 'syncing' ? '\u25cb' : '\u25cb';

  lines.push(`  ${output.colorize('Peers:'.padEnd(22), 'cyan')} ${stats.peers}`);
  lines.push(`  ${output.colorize('Block Height:'.padEnd(22), 'cyan')} ${stats.blockHeight.toLocaleString()}`);
  lines.push(`  ${output.colorize('Sync Status:'.padEnd(22), 'cyan')} ${output.colorize(`${syncIcon} ${stats.syncStatus.toUpperCase()}`, syncColor)}`);

  if (stats.syncStatus === 'syncing') {
    lines.push(`  ${output.colorize('Sync Progress:'.padEnd(22), 'cyan')} ${stats.syncProgress.toFixed(1)}%`);
  }

  lines.push(`  ${output.colorize('Connected Relays:'.padEnd(22), 'cyan')} ${stats.connectedRelays}`);
  lines.push(`  ${output.colorize('Relay Latency:'.padEnd(22), 'cyan')} ${stats.relayLatencyMs > 0 ? `${stats.relayLatencyMs}ms` : 'N/A'}`);
  lines.push(`  ${output.colorize('Network Uptime:'.padEnd(22), 'cyan')} ${stats.networkUptime}`);
  lines.push('');

  return lines.join('\n');
}

function renderShardsTable(shards: ShardEntry[], output: any): string {
  const lines: string[] = [];

  lines.push(output.colorize('Model Shard Distribution', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(72), 'dim'));
  lines.push('');

  if (shards.length === 0) {
    lines.push('  No shard data available.');
    lines.push('');
    return lines.join('\n');
  }

  const modelW = Math.max(8, ...shards.map(s => s.modelId.length));
  const shardW = 10;
  const gpuW = Math.max(8, ...shards.map(s => s.gpuId.length));
  const typeW = Math.max(8, ...shards.map(s => s.gpuType.length));
  const vramW = 18;
  const statW = 10;

  const header = '  ' +
    output.colorize('Model'.padEnd(modelW), 'bold') + '  ' +
    output.colorize('Shard'.padEnd(shardW), 'bold') + '  ' +
    output.colorize('GPU ID'.padEnd(gpuW), 'bold') + '  ' +
    output.colorize('GPU Type'.padEnd(typeW), 'bold') + '  ' +
    output.colorize('VRAM'.padEnd(vramW), 'bold') + '  ' +
    output.colorize('Status'.padEnd(statW), 'bold');
  lines.push(header);
  lines.push('  ' + '\u2500'.repeat(modelW + shardW + gpuW + typeW + vramW + statW + 15));

  // Group by model for readability
  const grouped = new Map<string, ShardEntry[]>();
  for (const s of shards) {
    const key = s.modelId;
    if (!grouped.has(key)) grouped.set(key, []);
    grouped.get(key)!.push(s);
  }

  for (const [modelId, modelShards] of grouped) {
    for (const s of modelShards) {
      const healthColor = s.status === 'healthy' ? 'green' : s.status === 'degraded' ? 'yellow' : 'red';
      const shardStr = `${s.shardIndex + 1}/${s.totalShards}`;
      const vramStr = s.vramTotalGB > 0
        ? `${s.vramUsedGB.toFixed(1)}/${s.vramTotalGB.toFixed(1)} GB`
        : `${s.vramUsedGB.toFixed(1)} GB`;

      lines.push(
        '  ' +
        modelId.substring(0, modelW).padEnd(modelW) + '  ' +
        shardStr.padEnd(shardW) + '  ' +
        s.gpuId.substring(0, gpuW).padEnd(gpuW) + '  ' +
        s.gpuType.substring(0, typeW).padEnd(typeW) + '  ' +
        output.colorize(vramStr.padEnd(vramW), 'cyan') + '  ' +
        output.colorize(s.status.toUpperCase().padEnd(statW), healthColor)
      );
    }
  }

  lines.push('');
  lines.push(output.colorize(`  ${shards.length} shard(s) across ${grouped.size} model(s)`, 'dim'));
  lines.push('');
  return lines.join('\n');
}

// ── Subcommand Handlers ───────────────────────────────────────────

async function handleDefaultStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const verbose = args.options.verbose === true;
  const baseUrl = ctx.config.baseUrl;
  const apiKey = ctx.config.apiKey;
  const agentUrl = resolveAgentUrl(String(args.options.agentUrl));

  // Fetch agent overview
  const overview = await fetchAgentOverview(agentUrl);

  // Run health checks
  const checks = await runHealthChecks(agentUrl, baseUrl, apiKey);

  const summary = {
    healthy: checks.filter(c => c.status === 'healthy').length,
    degraded: checks.filter(c => c.status === 'degraded').length,
    error: checks.filter(c => c.status === 'error').length,
  };

  const agent: AgentStatus = {
    ponwScore: overview.ponwScore ?? 0,
    ergBalance: overview.ergBalance ?? '0',
    modelsServing: overview.modelsServing ?? 0,
    uptime: overview.uptime ?? 'unknown',
    connectedRelays: overview.connectedRelays ?? 0,
    agentVersion: overview.agentVersion ?? 'unknown',
    agentUrl,
    checks,
    summary,
  };

  if (outputJson) {
    ctx.output.write(JSON.stringify(agent, null, 2));
  } else {
    ctx.output.write(renderDefaultStatus(agent, ctx.output, verbose));
  }

  if (summary.error > 0) process.exit(1);
}

async function handleProviders(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const agentUrl = resolveAgentUrl(String(args.options.agentUrl));

  const providers = await fetchProviders(agentUrl);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ providers, count: providers.length }, null, 2));
  } else {
    ctx.output.write(renderProvidersTable(providers, ctx.output));
  }
}

async function handleModels(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const agentUrl = resolveAgentUrl(String(args.options.agentUrl));

  const models = await fetchModels(agentUrl);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ models, count: models.length }, null, 2));
  } else {
    ctx.output.write(renderModelsTable(models, ctx.output));
  }
}

async function handleNetwork(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const agentUrl = resolveAgentUrl(String(args.options.agentUrl));
  const baseUrl = ctx.config.baseUrl;

  const stats = await fetchNetworkStats(agentUrl);

  // If agent doesn't return network data, try relay
  if (!stats.peers && !stats.blockHeight) {
    const relayHealth = await fetchRelayHealth(baseUrl);
    const networkStats: NetworkStats = {
      peers: 0,
      blockHeight: 0,
      syncStatus: 'unknown',
      syncProgress: 0,
      connectedRelays: relayHealth.connected ? 1 : 0,
      relayLatencyMs: relayHealth.latencyMs,
      networkUptime: 'unknown',
    };

    if (outputJson) {
      ctx.output.write(JSON.stringify(networkStats, null, 2));
    } else {
      ctx.output.write(renderNetworkStats(networkStats, ctx.output));
      ctx.output.write(ctx.output.colorize('  Note: Agent network API not available, showing relay data.', 'yellow') + '\n');
    }
    return;
  }

  const fullStats: NetworkStats = {
    peers: stats.peers ?? 0,
    blockHeight: stats.blockHeight ?? 0,
    syncStatus: stats.syncStatus ?? 'unknown',
    syncProgress: stats.syncProgress ?? 0,
    connectedRelays: stats.connectedRelays ?? 0,
    relayLatencyMs: stats.relayLatencyMs ?? 0,
    networkUptime: stats.networkUptime ?? 'unknown',
  };

  if (outputJson) {
    ctx.output.write(JSON.stringify(fullStats, null, 2));
  } else {
    ctx.output.write(renderNetworkStats(fullStats, ctx.output));
  }
}

async function handleShards(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const agentUrl = resolveAgentUrl(String(args.options.agentUrl));

  const shards = await fetchShards(agentUrl);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ shards, count: shards.length }, null, 2));
  } else {
    ctx.output.write(renderShardsTable(shards, ctx.output));
  }
}

// ── Command Action ─────────────────────────────────────────────────

async function statusAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  switch (sub) {
    case 'providers':
    case 'prov':
      await handleProviders(args, ctx);
      break;
    case 'models':
    case 'model':
      await handleModels(args, ctx);
      break;
    case 'network':
    case 'net':
      await handleNetwork(args, ctx);
      break;
    case 'shards':
    case 'shard':
      await handleShards(args, ctx);
      break;
    default:
      // Default: overall status
      await handleDefaultStatus(args, ctx);
      break;
  }
}

// ── Command Definition ─────────────────────────────────────────────

export const statusCommand: Command = {
  name: 'status',
  description: 'Show agent and relay status (providers, models, network, shards)',
  aliases: ['health', 'check'],
  options: statusOptions,
  action: statusAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  fetchAgentOverview,
  fetchProviders,
  fetchModels,
  fetchNetworkStats,
  fetchShards,
  fetchRelayHealth,
  runHealthChecks,
  resolveAgentUrl,
  toHealthLevel,
  formatUptime,
  measureLatency,
  renderDefaultStatus,
  renderProvidersTable,
  renderModelsTable,
  renderNetworkStats,
  renderShardsTable,
  renderStatusIcon,
  type AgentStatus,
  type ProviderEntry,
  type ModelEntry,
  type NetworkStats,
  type ShardEntry,
  type StatusCheck,
  type HealthLevel,
};
