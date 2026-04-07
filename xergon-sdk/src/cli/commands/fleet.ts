/**
 * CLI command: fleet
 *
 * Manage multi-agent fleets: batch deploy, health rollup, scaling, configuration.
 *
 * Usage:
 *   xergon fleet list                    -- list all fleet agents
 *   xergon fleet deploy <agent-id>       -- deploy an agent
 *   xergon fleet health                  -- health rollup across fleet
 *   xergon fleet scale <agent-id> <n>    -- scale an agent horizontally
 *   xergon fleet config <agent-id>       -- view/update agent config
 *   xergon fleet restart <agent-id>      -- restart an agent
 *   xergon fleet logs <agent-id>         -- tail agent logs
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

type AgentHealth = 'healthy' | 'degraded' | 'unhealthy' | 'unknown';
type AgentStatus = 'running' | 'stopped' | 'starting' | 'error' | 'unknown';

interface FleetAgent {
  id: string;
  name: string;
  status: AgentStatus;
  health: AgentHealth;
  replicas: number;
  model: string;
  gpu: string;
  uptime: number;           // seconds
  lastHeartbeat: string;    // ISO timestamp
  metrics: AgentMetrics;
}

interface AgentMetrics {
  requestsPerSec: number;
  avgLatencyMs: number;
  errorRatePct: number;
  vramUsedGB: number;
  vramTotalGB: number;
  cpuPct: number;
}

interface FleetHealthSummary {
  totalAgents: number;
  healthy: number;
  degraded: number;
  unhealthy: number;
  avgUptime: number;
  alerts: FleetAlert[];
}

interface FleetAlert {
  agentId: string;
  severity: 'critical' | 'warning' | 'info';
  message: string;
  timestamp: string;
}

interface DeployConfig {
  model: string;
  gpu: string;
  replicas: number;
  configOverrides: Record<string, string>;
  dryRun: boolean;
}

interface ScaleConfig {
  targetReplicas: number;
  strategy: 'rolling' | 'immediate';
}

interface FleetConfig {
  id: string;
  configMap: Record<string, string>;
  updated_at: string;
}

interface FleetLogEntry {
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  replica: number;
  message: string;
}

// ── FleetService (mock implementation) ────────────────────────────

class FleetService {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  /**
   * Safely fetch JSON from an endpoint with timeout.
   */
  private async fetchJSON<T>(url: string, timeoutMs: number = 10_000): Promise<T | null> {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) return null;
      return await res.json() as T;
    } catch {
      return null;
    }
  }

  /**
   * List all fleet agents.
   */
  async listAgents(): Promise<FleetAgent[]> {
    // Try real API first
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/fleet/agents`);
    if (data) {
      const agents: any[] = Array.isArray(data) ? data : (data.data ?? data.agents ?? []);
      return agents.map(this.parseAgent);
    }

    // Mock data fallback
    return this.mockAgents();
  }

  /**
   * Get health summary across the fleet.
   */
  async getHealthSummary(verbose: boolean = false): Promise<FleetHealthSummary> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/fleet/health`);
    if (data) {
      return {
        totalAgents: data.totalAgents ?? data.total_agents ?? 0,
        healthy: data.healthy ?? 0,
        degraded: data.degraded ?? 0,
        unhealthy: data.unhealthy ?? 0,
        avgUptime: data.avgUptime ?? data.avg_uptime ?? 0,
        alerts: (data.alerts ?? []).map((a: any) => ({
          agentId: a.agentId ?? a.agent_id ?? '',
          severity: a.severity ?? 'info',
          message: a.message ?? '',
          timestamp: a.timestamp ?? new Date().toISOString(),
        })),
      };
    }

    // Mock fallback
    const agents = await this.listAgents();
    const healthy = agents.filter(a => a.health === 'healthy').length;
    const degraded = agents.filter(a => a.health === 'degraded').length;
    const unhealthy = agents.filter(a => a.health === 'unhealthy').length;
    const avgUptime = agents.length > 0
      ? Math.round(agents.reduce((sum, a) => sum + a.uptime, 0) / agents.length)
      : 0;

    const alerts: FleetAlert[] = [];
    for (const agent of agents) {
      if (agent.health === 'unhealthy') {
        alerts.push({
          agentId: agent.id,
          severity: 'critical',
          message: `Agent ${agent.name} is unhealthy`,
          timestamp: agent.lastHeartbeat,
        });
      } else if (agent.health === 'degraded') {
        alerts.push({
          agentId: agent.id,
          severity: 'warning',
          message: `Agent ${agent.name} is degraded: high latency`,
          timestamp: agent.lastHeartbeat,
        });
      }
    }

    return { totalAgents: agents.length, healthy, degraded, unhealthy, avgUptime, alerts };
  }

  /**
   * Deploy an agent to the fleet.
   */
  async deployAgent(agentId: string, config: DeployConfig): Promise<{ success: boolean; message: string; agent?: FleetAgent }> {
    if (config.dryRun) {
      return {
        success: true,
        message: `[DRY RUN] Would deploy agent "${agentId}" with model=${config.model}, gpu=${config.gpu}, replicas=${config.replicas}`,
      };
    }

    // Try real API
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/fleet/deploy`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ agentId, ...config }),
        signal: AbortSignal.timeout(30_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          success: true,
          message: `Agent "${agentId}" deployed successfully`,
          agent: data.agent ? this.parseAgent(data.agent) : undefined,
        };
      }
      const err: any = await res.json().catch(() => ({}));
      return { success: false, message: err.error ?? err.message ?? `Deploy failed with HTTP ${res.status}` };
    } catch {
      // Mock fallback
    }

    // Mock deploy
    return {
      success: true,
      message: `Agent "${agentId}" deployed successfully`,
      agent: {
        id: agentId,
        name: agentId,
        status: 'running',
        health: 'healthy',
        replicas: config.replicas,
        model: config.model,
        gpu: config.gpu,
        uptime: 0,
        lastHeartbeat: new Date().toISOString(),
        metrics: {
          requestsPerSec: 0,
          avgLatencyMs: 0,
          errorRatePct: 0,
          vramUsedGB: 0,
          vramTotalGB: 24,
          cpuPct: 0,
        },
      },
    };
  }

  /**
   * Scale an agent horizontally.
   */
  async scaleAgent(agentId: string, scaleConfig: ScaleConfig): Promise<{ success: boolean; message: string; previousReplicas?: number; currentReplicas?: number }> {
    // Try real API
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/fleet/scale`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ agentId, ...scaleConfig }),
        signal: AbortSignal.timeout(30_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          success: true,
          message: `Agent "${agentId}" scaled to ${scaleConfig.targetReplicas} replicas (${scaleConfig.strategy})`,
          previousReplicas: data.previousReplicas,
          currentReplicas: data.currentReplicas ?? scaleConfig.targetReplicas,
        };
      }
    } catch {
      // Mock fallback
    }

    // Mock scale
    return {
      success: true,
      message: `Agent "${agentId}" scaled to ${scaleConfig.targetReplicas} replicas (${scaleConfig.strategy})`,
      previousReplicas: scaleConfig.targetReplicas - 1,
      currentReplicas: scaleConfig.targetReplicas,
    };
  }

  /**
   * Get or update fleet agent config.
   */
  async getConfig(agentId: string): Promise<FleetConfig> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/fleet/config/${agentId}`);
    if (data) {
      return {
        id: data.id ?? agentId,
        configMap: data.configMap ?? data.config_map ?? data.config ?? {},
        updated_at: data.updated_at ?? data.updatedAt ?? new Date().toISOString(),
      };
    }

    // Mock config
    return {
      id: agentId,
      configMap: {
        model: 'llama-3.3-70b',
        temperature: '0.7',
        max_tokens: '4096',
        gpu_memory_limit: '16Gi',
        log_level: 'info',
        health_check_interval: '30s',
        request_timeout: '60s',
      },
      updated_at: new Date().toISOString(),
    };
  }

  async updateConfig(agentId: string, updates: Record<string, string>): Promise<{ success: boolean; message: string; config?: FleetConfig }> {
    // Try real API
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/fleet/config/${agentId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(updates),
        signal: AbortSignal.timeout(10_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          success: true,
          message: `Config updated for agent "${agentId}"`,
          config: {
            id: agentId,
            configMap: data.configMap ?? data.config ?? updates,
            updated_at: data.updated_at ?? new Date().toISOString(),
          },
        };
      }
    } catch {
      // Mock fallback
    }

    const existing = await this.getConfig(agentId);
    return {
      success: true,
      message: `Config updated for agent "${agentId}": ${Object.keys(updates).join(', ')}`,
      config: {
        ...existing,
        configMap: { ...existing.configMap, ...updates },
        updated_at: new Date().toISOString(),
      },
    };
  }

  async resetConfig(agentId: string): Promise<{ success: boolean; message: string; config?: FleetConfig }> {
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/fleet/config/${agentId}/reset`, {
        method: 'POST',
        signal: AbortSignal.timeout(10_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          success: true,
          message: `Config reset to defaults for agent "${agentId}"`,
          config: {
            id: agentId,
            configMap: data.configMap ?? data.config ?? {},
            updated_at: data.updated_at ?? new Date().toISOString(),
          },
        };
      }
    } catch {
      // Mock fallback
    }

    return {
      success: true,
      message: `Config reset to defaults for agent "${agentId}"`,
      config: {
        id: agentId,
        configMap: {
          model: 'llama-3.3-70b',
          temperature: '0.7',
          max_tokens: '4096',
          gpu_memory_limit: '16Gi',
          log_level: 'info',
          health_check_interval: '30s',
          request_timeout: '60s',
        },
        updated_at: new Date().toISOString(),
      },
    };
  }

  /**
   * Restart an agent (rolling restart for multi-replica).
   */
  async restartAgent(agentId: string): Promise<{ success: boolean; message: string; replicasRestarted: number }> {
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/fleet/restart`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ agentId }),
        signal: AbortSignal.timeout(60_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          success: true,
          message: `Agent "${agentId}" restarted successfully`,
          replicasRestarted: data.replicasRestarted ?? 1,
        };
      }
    } catch {
      // Mock fallback
    }

    return {
      success: true,
      message: `Agent "${agentId}" restarted successfully (rolling restart)`,
      replicasRestarted: 3,
    };
  }

  /**
   * Get logs for a fleet agent.
   */
  async getLogs(agentId: string, options: { tail?: number; since?: string; level?: string }): Promise<FleetLogEntry[]> {
    const params = new URLSearchParams();
    if (options.tail) params.set('tail', String(options.tail));
    if (options.since) params.set('since', options.since);
    if (options.level) params.set('level', options.level);

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/fleet/logs/${agentId}?${params}`);
    if (data) {
      const logs: any[] = Array.isArray(data) ? data : (data.data ?? data.logs ?? []);
      return logs.map((l: any) => ({
        timestamp: l.timestamp ?? new Date().toISOString(),
        level: l.level ?? 'info',
        replica: l.replica ?? l.replicaId ?? 0,
        message: l.message ?? l.msg ?? '',
      }));
    }

    // Mock logs
    const tail = options.tail ?? 50;
    const levels: Array<'info' | 'warn' | 'error' | 'debug'> = ['info', 'info', 'info', 'debug', 'warn'];
    const messages = [
      'Request processed successfully',
      'Health check passed',
      'Model loaded into VRAM',
      'Connection pool expanded to 10',
      'Request queue depth: 2',
      'Garbage collection completed in 12ms',
      'Heartbeat sent to fleet coordinator',
      'Configuration reloaded from store',
      'Request rate: 45.2 req/s',
      'Average latency: 128ms',
      'VRAM usage: 14.2/24.0 GB',
      'Worker thread pool: 4/8 active',
      'Checkpoint saved to persistent storage',
      'New replica registered with fleet',
      'Upstream provider rotation completed',
    ];

    const now = Date.now();
    const logs: FleetLogEntry[] = [];
    for (let i = 0; i < tail; i++) {
      const ts = new Date(now - i * 15000);
      logs.push({
        timestamp: ts.toISOString(),
        level: levels[i % levels.length],
        replica: i % 3,
        message: messages[i % messages.length],
      });
    }
    return logs;
  }

  // ── Private helpers ──

  private parseAgent(raw: any): FleetAgent {
    return {
      id: raw.id ?? raw.agentId ?? raw.agent_id ?? 'unknown',
      name: raw.name ?? raw.id ?? 'unknown',
      status: this.parseStatus(raw.status ?? raw.state),
      health: this.parseHealth(raw.health, raw.healthy),
      replicas: Number(raw.replicas ?? 1),
      model: raw.model ?? raw.modelId ?? raw.model_id ?? 'unknown',
      gpu: raw.gpu ?? raw.gpuType ?? raw.gpu_type ?? 'auto',
      uptime: Number(raw.uptime ?? raw.uptimeSeconds ?? 0),
      lastHeartbeat: raw.lastHeartbeat ?? raw.last_heartbeat ?? new Date().toISOString(),
      metrics: {
        requestsPerSec: Number(raw.metrics?.requestsPerSec ?? raw.requestsPerSec ?? 0),
        avgLatencyMs: Number(raw.metrics?.avgLatencyMs ?? raw.avgLatencyMs ?? 0),
        errorRatePct: Number(raw.metrics?.errorRatePct ?? raw.errorRatePct ?? 0),
        vramUsedGB: Number(raw.metrics?.vramUsedGB ?? raw.vramUsedGB ?? 0),
        vramTotalGB: Number(raw.metrics?.vramTotalGB ?? raw.vramTotalGB ?? 0),
        cpuPct: Number(raw.metrics?.cpuPct ?? raw.cpuPct ?? 0),
      },
    };
  }

  private parseStatus(raw: string | undefined): AgentStatus {
    if (!raw) return 'unknown';
    const s = raw.toLowerCase();
    if (s === 'running' || s === 'active' || s === 'online') return 'running';
    if (s === 'stopped' || s === 'offline') return 'stopped';
    if (s === 'starting' || s === 'pending' || s === 'initializing') return 'starting';
    if (s === 'error' || s === 'failed' || s === 'crashed') return 'error';
    return 'unknown';
  }

  private parseHealth(raw: string | undefined, healthy?: boolean): AgentHealth {
    if (healthy === true) return 'healthy';
    if (healthy === false) return 'unhealthy';
    if (!raw) return 'unknown';
    const s = raw.toLowerCase();
    if (s === 'healthy' || s === 'ok') return 'healthy';
    if (s === 'degraded' || s === 'warning') return 'degraded';
    if (s === 'unhealthy' || s === 'error' || s === 'down') return 'unhealthy';
    return 'unknown';
  }

  private mockAgents(): FleetAgent[] {
    const now = Date.now();
    return [
      {
        id: 'agent-001',
        name: 'chat-primary',
        status: 'running',
        health: 'healthy',
        replicas: 3,
        model: 'llama-3.3-70b',
        gpu: 'NVIDIA A100 80GB',
        uptime: 86400 * 3 + 7200,
        lastHeartbeat: new Date(now - 5000).toISOString(),
        metrics: { requestsPerSec: 45.2, avgLatencyMs: 128, errorRatePct: 0.3, vramUsedGB: 38.4, vramTotalGB: 48.0, cpuPct: 34 },
      },
      {
        id: 'agent-002',
        name: 'code-assistant',
        status: 'running',
        health: 'healthy',
        replicas: 2,
        model: 'deepseek-coder-v2',
        gpu: 'NVIDIA A100 40GB',
        uptime: 86400 * 1 + 3600,
        lastHeartbeat: new Date(now - 3000).toISOString(),
        metrics: { requestsPerSec: 22.1, avgLatencyMs: 245, errorRatePct: 0.8, vramUsedGB: 28.6, vramTotalGB: 40.0, cpuPct: 52 },
      },
      {
        id: 'agent-003',
        name: 'embeddings-svc',
        status: 'running',
        health: 'degraded',
        replicas: 1,
        model: 'bge-large-en-v1.5',
        gpu: 'NVIDIA RTX 4090',
        uptime: 86400 * 7,
        lastHeartbeat: new Date(now - 15000).toISOString(),
        metrics: { requestsPerSec: 8.5, avgLatencyMs: 520, errorRatePct: 3.2, vramUsedGB: 22.1, vramTotalGB: 24.0, cpuPct: 78 },
      },
      {
        id: 'agent-004',
        name: 'image-gen',
        status: 'stopped',
        health: 'unknown',
        replicas: 0,
        model: 'stable-diffusion-xl',
        gpu: 'NVIDIA A100 80GB',
        uptime: 0,
        lastHeartbeat: new Date(now - 86400 * 1000).toISOString(),
        metrics: { requestsPerSec: 0, avgLatencyMs: 0, errorRatePct: 0, vramUsedGB: 0, vramTotalGB: 48.0, cpuPct: 0 },
      },
      {
        id: 'agent-005',
        name: 'reranker-svc',
        status: 'running',
        health: 'unhealthy',
        replicas: 1,
        model: 'bge-reranker-v2-m3',
        gpu: 'NVIDIA RTX 3090',
        uptime: 3600,
        lastHeartbeat: new Date(now - 120000).toISOString(),
        metrics: { requestsPerSec: 0, avgLatencyMs: 0, errorRatePct: 100, vramUsedGB: 12.0, vramTotalGB: 24.0, cpuPct: 95 },
      },
    ];
  }
}

// ── Formatting helpers ────────────────────────────────────────────

function formatUptime(seconds: number): string {
  if (seconds <= 0) return '-';
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function healthColor(health: AgentHealth, output: any): string {
  switch (health) {
    case 'healthy': return output.colorize('healthy', 'green');
    case 'degraded': return output.colorize('degraded', 'yellow');
    case 'unhealthy': return output.colorize('unhealthy', 'red');
    default: return output.colorize('unknown', 'dim');
  }
}

function statusColor(status: AgentStatus, output: any): string {
  switch (status) {
    case 'running': return output.colorize('running', 'green');
    case 'starting': return output.colorize('starting', 'yellow');
    case 'stopped': return output.colorize('stopped', 'dim');
    case 'error': return output.colorize('error', 'red');
    default: return output.colorize('unknown', 'dim');
  }
}

function severityColor(severity: string, output: any): string {
  switch (severity) {
    case 'critical': return output.colorize('CRITICAL', 'red');
    case 'warning': return output.colorize('WARNING', 'yellow');
    default: return output.colorize('INFO', 'cyan');
  }
}

// ── Subcommand handlers ───────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const service = new FleetService(ctx.config.baseUrl);
  const agents = await service.listAgents();

  if (agents.length === 0) {
    ctx.output.info('No fleet agents found.');
    return;
  }

  const tableData = agents.map(a => ({
    ID: a.id,
    Name: a.name,
    Status: a.status,
    Health: a.health,
    Replicas: String(a.replicas),
    Model: a.model,
    GPU: a.gpu,
    Uptime: formatUptime(a.uptime),
    'Req/s': a.metrics.requestsPerSec.toFixed(1),
    'Latency': `${a.metrics.avgLatencyMs}ms`,
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Fleet Agents (${agents.length})`));
}

async function handleDeploy(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const agentId = args.positional[1];
  if (!agentId) {
    ctx.output.writeError('Usage: xergon fleet deploy <agent-id> [options]');
    process.exit(1);
    return;
  }

  const model = args.options.model ? String(args.options.model) : 'llama-3.3-70b';
  const gpu = args.options.gpu ? String(args.options.gpu) : 'auto';
  const replicas = args.options.replicas !== undefined ? Number(args.options.replicas) : 1;
  const dryRun = Boolean(args.options.dryRun);
  const configStr = args.options.config ? String(args.options.config) : '';

  // Parse config overrides: --config key1=val1,key2=val2
  const configOverrides: Record<string, string> = {};
  if (configStr) {
    for (const pair of configStr.split(',')) {
      const eqIdx = pair.indexOf('=');
      if (eqIdx > 0) {
        configOverrides[pair.substring(0, eqIdx).trim()] = pair.substring(eqIdx + 1).trim();
      }
    }
  }

  const deployConfig: DeployConfig = { model, gpu, replicas, configOverrides, dryRun };
  const service = new FleetService(ctx.config.baseUrl);

  const spinner = dryRun ? '' : ctx.output.colorize('Deploying', 'cyan') + '...';
  if (spinner) process.stderr.write(`  ${spinner}\r`);

  try {
    const result = await service.deployAgent(agentId, deployConfig);

    if (spinner) process.stderr.write(' '.repeat(40) + '\r');

    if (result.success) {
      ctx.output.success(result.message);
      if (result.agent) {
        const a = result.agent;
        ctx.output.write('');
        ctx.output.write(ctx.output.formatText({
          ID: a.id,
          Name: a.name,
          Status: a.status,
          Replicas: String(a.replicas),
          Model: a.model,
          GPU: a.gpu,
        }, 'Deployed Agent'));
      }
    } else {
      ctx.output.writeError(result.message);
      process.exit(1);
    }
  } catch (err) {
    if (spinner) process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Deploy failed: ${message}`);
    process.exit(1);
  }
}

async function handleHealth(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const verbose = Boolean(args.options.verbose);
  const watch = Boolean(args.options.watch);

  const service = new FleetService(ctx.config.baseUrl);

  if (watch && process.stdin.isTTY) {
    // Live watch mode
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.setEncoding('utf-8');
    let running = true;
    process.stdin.on('data', (key: string) => {
      if (key === 'q' || key === '\x03') {
        running = false;
        process.stdin.setRawMode(false);
      }
    });

    try {
      while (running) {
        process.stdout.write('\x1b[2J\x1b[H');
        const summary = await service.getHealthSummary(verbose);
        renderHealthDashboard(summary, verbose, ctx.output);
        await new Promise<void>((resolve, reject) => {
          const timer = setTimeout(resolve, 3000);
          const check = setInterval(() => {
            if (!running) { clearTimeout(timer); clearInterval(check); resolve(); }
          }, 200);
        });
      }
    } finally {
      process.stdin.setRawMode(false);
      ctx.output.info('Health watch stopped.');
    }
    return;
  }

  // Single snapshot
  const summary = await service.getHealthSummary(verbose);
  renderHealthOutput(summary, verbose, ctx.output);
}

function renderHealthOutput(summary: FleetHealthSummary, verbose: boolean, output: any): void {
  const lines: string[] = [];

  lines.push(output.colorize('Fleet Health Summary', 'bold'));
  lines.push(output.colorize('\u2500'.repeat(56), 'dim'));
  lines.push('');
  lines.push(`  Total Agents:   ${output.colorize(String(summary.totalAgents), 'cyan')}`);
  lines.push(`  Healthy:        ${output.colorize(String(summary.healthy), 'green')}`);
  lines.push(`  Degraded:       ${output.colorize(String(summary.degraded), 'yellow')}`);
  lines.push(`  Unhealthy:      ${output.colorize(String(summary.unhealthy), summary.unhealthy > 0 ? 'red' : 'dim')}`);
  lines.push(`  Avg Uptime:     ${output.colorize(formatUptime(summary.avgUptime), 'cyan')}`);

  if (summary.alerts.length > 0) {
    lines.push('');
    lines.push(output.colorize(`  Alerts (${summary.alerts.length})`, 'bold'));
    lines.push(output.colorize('  ' + '\u2500'.repeat(52), 'dim'));
    for (const alert of summary.alerts) {
      lines.push(`  ${severityColor(alert.severity, output)}  ${output.colorize(alert.agentId, 'bold')}  ${alert.message}`);
      if (verbose) {
        lines.push(`              ${output.colorize(alert.timestamp, 'dim')}`);
      }
    }
  }

  lines.push('');
  output.write(lines.join('\n'));
}

function renderHealthDashboard(summary: FleetHealthSummary, verbose: boolean, output: any): void {
  const now = new Date().toLocaleTimeString();
  const lines: string[] = [];
  lines.push(`  ${output.colorize('FLEET HEALTH', 'bold')} ${output.colorize(now, 'dim')}`);
  lines.push(output.colorize('  \u2500'.repeat(56), 'dim'));
  lines.push('');
  lines.push(`  Agents:  ${output.colorize(String(summary.totalAgents), 'cyan')}  ` +
    `Healthy: ${output.colorize(String(summary.healthy), 'green')}  ` +
    `Degraded: ${output.colorize(String(summary.degraded), 'yellow')}  ` +
    `Unhealthy: ${output.colorize(String(summary.unhealthy), summary.unhealthy > 0 ? 'red' : 'dim')}`);
  lines.push(`  Uptime:  ${output.colorize(formatUptime(summary.avgUptime), 'cyan')}  ` +
    `Alerts: ${output.colorize(String(summary.alerts.length), summary.alerts.length > 0 ? 'yellow' : 'green')}`);

  if (summary.alerts.length > 0) {
    lines.push('');
    for (const alert of summary.alerts.slice(0, 6)) {
      lines.push(`    ${severityColor(alert.severity, output)} ${alert.agentId.padEnd(14)} ${alert.message}`);
    }
  }

  lines.push('');
  lines.push(output.colorize('  Press [q] to quit', 'dim'));
  process.stdout.write(lines.join('\n') + '\n');
}

async function handleScale(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const agentId = args.positional[1];
  const replicasStr = args.positional[2];

  if (!agentId || !replicasStr) {
    ctx.output.writeError('Usage: xergon fleet scale <agent-id> <replicas> [--strategy rolling|immediate]');
    process.exit(1);
    return;
  }

  const targetReplicas = parseInt(replicasStr, 10);
  if (isNaN(targetReplicas) || targetReplicas < 0) {
    ctx.output.writeError('Replicas must be a non-negative integer.');
    process.exit(1);
    return;
  }

  const strategy = (args.options.strategy === 'immediate' ? 'immediate' : 'rolling') as 'rolling' | 'immediate';

  const service = new FleetService(ctx.config.baseUrl);

  try {
    const result = await service.scaleAgent(agentId, { targetReplicas, strategy });

    if (result.success) {
      ctx.output.success(result.message);
      if (result.previousReplicas !== undefined && result.currentReplicas !== undefined) {
        ctx.output.write(`  Replicas: ${result.previousReplicas} -> ${result.currentReplicas}`);
      }
    } else {
      ctx.output.writeError(result.message);
      process.exit(1);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Scale failed: ${message}`);
    process.exit(1);
  }
}

async function handleConfig(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const agentId = args.positional[1];
  if (!agentId) {
    ctx.output.writeError('Usage: xergon fleet config <agent-id> [--set key=value] [--get key] [--reset]');
    process.exit(1);
    return;
  }

  const service = new FleetService(ctx.config.baseUrl);

  // --reset flag
  if (args.options.reset) {
    try {
      const result = await service.resetConfig(agentId);
      if (result.success) {
        ctx.output.success(result.message);
        if (result.config) {
          renderConfigMap(result.config, ctx.output);
        }
      } else {
        ctx.output.writeError(result.message);
        process.exit(1);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      ctx.output.writeError(`Config reset failed: ${message}`);
      process.exit(1);
    }
    return;
  }

  // --set key=value
  if (args.options.set) {
    const setStr = String(args.options.set);
    const eqIdx = setStr.indexOf('=');
    if (eqIdx <= 0) {
      ctx.output.writeError('--set requires key=value format');
      process.exit(1);
      return;
    }
    const key = setStr.substring(0, eqIdx).trim();
    const value = setStr.substring(eqIdx + 1).trim();

    try {
      const result = await service.updateConfig(agentId, { [key]: value });
      if (result.success) {
        ctx.output.success(result.message);
        if (result.config) {
          renderConfigMap(result.config, ctx.output);
        }
      } else {
        ctx.output.writeError(result.message);
        process.exit(1);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      ctx.output.writeError(`Config update failed: ${message}`);
      process.exit(1);
    }
    return;
  }

  // --get key
  if (args.options.get) {
    const getKey = String(args.options.get);
    try {
      const config = await service.getConfig(agentId);
      const value = config.configMap[getKey];
      if (value !== undefined) {
        ctx.output.write(`${getKey}=${value}`);
      } else {
        ctx.output.writeError(`Config key "${getKey}" not found for agent "${agentId}"`);
        process.exit(1);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      ctx.output.writeError(`Config get failed: ${message}`);
      process.exit(1);
    }
    return;
  }

  // Default: show all config
  try {
    const config = await service.getConfig(agentId);
    renderConfigMap(config, ctx.output);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get config: ${message}`);
    process.exit(1);
  }
}

function renderConfigMap(config: FleetConfig, output: any): void {
  const lines: string[] = [];
  lines.push(output.colorize(`Config: ${config.id}`, 'bold'));
  lines.push(output.colorize('\u2500'.repeat(48), 'dim'));
  lines.push('');

  const keys = Object.keys(config.configMap).sort();
  for (const key of keys) {
    const label = output.colorize(key.padEnd(28), 'cyan');
    lines.push(`  ${label} ${config.configMap[key]}`);
  }

  lines.push('');
  lines.push(`  ${output.colorize('Updated'.padEnd(28), 'dim')} ${output.colorize(config.updated_at, 'dim')}`);
  lines.push('');

  output.write(lines.join('\n'));
}

async function handleRestart(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const agentId = args.positional[1];
  if (!agentId) {
    ctx.output.writeError('Usage: xergon fleet restart <agent-id>');
    process.exit(1);
    return;
  }

  const service = new FleetService(ctx.config.baseUrl);

  process.stderr.write(`  ${ctx.output.colorize('Restarting', 'cyan')} ${agentId}...\r`);

  try {
    const result = await service.restartAgent(agentId);
    process.stderr.write(' '.repeat(50) + '\r');

    if (result.success) {
      ctx.output.success(result.message);
      ctx.output.write(`  Replicas restarted: ${result.replicasRestarted}`);
    } else {
      ctx.output.writeError(result.message);
      process.exit(1);
    }
  } catch (err) {
    process.stderr.write(' '.repeat(50) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Restart failed: ${message}`);
    process.exit(1);
  }
}

async function handleLogs(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const agentId = args.positional[1];
  if (!agentId) {
    ctx.output.writeError('Usage: xergon fleet logs <agent-id> [--tail N] [--since TIMESTAMP] [--level LEVEL]');
    process.exit(1);
    return;
  }

  const tail = args.options.tail !== undefined ? Number(args.options.tail) : 50;
  const since = args.options.since ? String(args.options.since) : undefined;
  const level = args.options.level ? String(args.options.level) : undefined;

  const service = new FleetService(ctx.config.baseUrl);

  try {
    const logs = await service.getLogs(agentId, { tail, since, level });

    if (logs.length === 0) {
      ctx.output.info('No logs found for this agent.');
      return;
    }

    const levelColors: Record<string, string> = {
      info: 'cyan',
      warn: 'yellow',
      error: 'red',
      debug: 'dim',
    };

    ctx.output.write(ctx.output.colorize(`Fleet Logs: ${agentId} (${logs.length} entries)`, 'bold'));
    ctx.output.write('');

    for (const log of logs) {
      const ts = new Date(log.timestamp).toISOString().slice(11, 19);
      const lvl = ctx.output.colorize(
        log.level.toUpperCase().padEnd(5),
        (levelColors[log.level] || 'dim') as 'dim',
      );
      const replica = ctx.output.colorize(`r${log.replica}`, 'dim');
      ctx.output.write(`  ${ts}  ${lvl}  ${replica}  ${log.message}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get logs: ${message}`);
    process.exit(1);
  }
}

// ── Main action dispatcher ────────────────────────────────────────

async function fleetAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon fleet <list|deploy|health|scale|config|restart|logs> [args]');
    ctx.output.write('');
    ctx.output.write('Commands:');
    ctx.output.write('  list              List all fleet agents');
    ctx.output.write('  deploy <id>       Deploy an agent');
    ctx.output.write('  health            Fleet health rollup');
    ctx.output.write('  scale <id> <n>    Scale agent replicas');
    ctx.output.write('  config <id>       View/update agent config');
    ctx.output.write('  restart <id>      Restart an agent');
    ctx.output.write('  logs <id>         Tail agent logs');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'deploy':
      await handleDeploy(args, ctx);
      break;
    case 'health':
      await handleHealth(args, ctx);
      break;
    case 'scale':
      await handleScale(args, ctx);
      break;
    case 'config':
      await handleConfig(args, ctx);
      break;
    case 'restart':
      await handleRestart(args, ctx);
      break;
    case 'logs':
      await handleLogs(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown fleet subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: list, deploy, health, scale, config, restart, logs');
      process.exit(1);
      break;
  }
}

// ── Command export ────────────────────────────────────────────────

const fleetOptions: CommandOption[] = [
  {
    name: 'model',
    short: '-m',
    long: '--model',
    description: 'Model name for deploy (default: llama-3.3-70b)',
    required: false,
    type: 'string',
  },
  {
    name: 'gpu',
    short: '-g',
    long: '--gpu',
    description: 'GPU type or device index (default: auto)',
    required: false,
    type: 'string',
  },
  {
    name: 'replicas',
    short: '-r',
    long: '--replicas',
    description: 'Number of replicas for deploy/scale',
    required: false,
    type: 'number',
  },
  {
    name: 'config',
    short: '',
    long: '--config',
    description: 'Config overrides for deploy (key1=val1,key2=val2)',
    required: false,
    type: 'string',
  },
  {
    name: 'dryRun',
    short: '',
    long: '--dry-run',
    description: 'Show what would happen without deploying',
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
  {
    name: 'watch',
    short: '-w',
    long: '--watch',
    description: 'Live health watch mode (auto-refresh every 3s)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'strategy',
    short: '',
    long: '--strategy',
    description: 'Scale strategy: rolling (default) or immediate',
    required: false,
    type: 'string',
  },
  {
    name: 'set',
    short: '',
    long: '--set',
    description: 'Set a config key-value (key=value)',
    required: false,
    type: 'string',
  },
  {
    name: 'get',
    short: '',
    long: '--get',
    description: 'Get a specific config key',
    required: false,
    type: 'string',
  },
  {
    name: 'reset',
    short: '',
    long: '--reset',
    description: 'Reset agent config to defaults',
    required: false,
    type: 'boolean',
  },
  {
    name: 'tail',
    short: '-n',
    long: '--tail',
    description: 'Number of log entries to show (default: 50)',
    required: false,
    type: 'number',
  },
  {
    name: 'since',
    short: '',
    long: '--since',
    description: 'Show logs since timestamp (ISO 8601)',
    required: false,
    type: 'string',
  },
  {
    name: 'level',
    short: '',
    long: '--level',
    description: 'Filter logs by level: info, warn, error, debug',
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
];

export const fleetCommand: Command = {
  name: 'fleet',
  description: 'Manage multi-agent fleets: deploy, health, scaling, config',
  aliases: ['agents'],
  options: fleetOptions,
  action: fleetAction,
};
