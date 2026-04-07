/**
 * CLI command: monitor
 *
 * Monitor network health, providers, uptime, SLAs, alerts, and anomalies
 * across the Xergon Network.
 *
 * Usage:
 *   xergon monitor health [--provider <id>]
 *   xergon monitor providers [--status healthy|degraded|down] [--region <region>]
 *   xergon monitor uptime [--provider <id>] [--hours <n>]
 *   xergon monitor slas [--provider <id>]
 *   xergon monitor alerts [--severity info|warning|critical] [--active-only]
 *   xergon monitor alerts/acknowledge --id <alert-id>
 *   xergon monitor anomalies [--severity low|medium|high|critical] [--active-only]
 *   xergon monitor topology
 *   xergon monitor latency [--matrix]
 *   xergon monitor stats
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

interface ProviderHealth {
  providerId: string;
  status: 'healthy' | 'degraded' | 'down' | 'unknown';
  lastHeartbeat: string;
  latencyMs: number;
  consecutiveFailures: number;
  region: string;
  modelsServed: string[];
  load: number;
}

interface UptimeStats {
  providerId: string;
  uptimePercent: number;
  avgResponseTimeMs: number;
  errorRate: number;
  totalChecks: number;
  period: string;
}

interface SLADefinition {
  id: string;
  providerId: string;
  name: string;
  targetUptime: number;
  maxResponseTimeMs: number;
  maxErrorRate: number;
  penaltyRate: number;
  createdAt: string;
}

interface SLACompliance {
  slaId: string;
  providerId: string;
  uptimePercent: number;
  avgResponseTimeMs: number;
  errorRate: number;
  violations: number;
  compliant: boolean;
}

interface AlertItem {
  id: string;
  ruleName: string;
  providerId: string;
  message: string;
  severity: 'info' | 'warning' | 'critical';
  triggeredAt: string;
  acknowledged: boolean;
}

interface AnomalyItem {
  id: string;
  anomalyType: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  description: string;
  affectedProviders: string[];
  detectedAt: string;
  resolved: boolean;
}

interface TopologyNode {
  nodeId: string;
  nodeType: string;
  address: string;
  region: string;
  status: string;
  uptimeMs: number;
  version: string;
}

interface LatencyPair {
  providerA: string;
  providerB: string;
  latencyMs: number;
  jitterMs: number;
  packetLoss: number;
}

interface NetworkStats {
  totalProviders: number;
  activeProviders: number;
  degradedProviders: number;
  downProviders: number;
  totalAlerts: number;
  activeAlerts: number;
  totalAnomalies: number;
  activeAnomalies: number;
  avgLatencyMs: number;
  overallHealthScore: number;
}

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function getSubcommand(args: ParsedArgs): string {
  return args.positional[0] || 'help';
}

function formatPercent(value: number): string {
  return (value * 100).toFixed(1) + '%';
}

function formatMs(value: number): string {
  if (value < 1000) return value + 'ms';
  return (value / 1000).toFixed(1) + 's';
}

// ── Subcommand: health ─────────────────────────────────────────────

async function handleHealth(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  try {
    let data: ProviderHealth | NetworkStats;

    if (providerId && ctx.client?.monitor?.getProviderHealth) {
      data = await ctx.client.monitor.getProviderHealth({ providerId });
    } else if (ctx.client?.monitor?.getNetworkStats) {
      data = await ctx.client.monitor.getNetworkStats({});
    } else {
      // Fallback mock
      data = {
        totalProviders: 42,
        activeProviders: 38,
        degradedProviders: 3,
        downProviders: 1,
        totalAlerts: 15,
        activeAlerts: 4,
        totalAnomalies: 7,
        activeAnomalies: 2,
        avgLatencyMs: 142,
        overallHealthScore: 94.5,
      } as NetworkStats;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(data, null, 2));
      return;
    }

    if ('totalProviders' in data) {
      // Network stats
      const s = data as NetworkStats;
      ctx.output.info('Network Health Overview');
      ctx.output.info('========================');
      ctx.output.write(`  Providers:  ${s.activeProviders}/${s.totalProviders} active (${s.degradedProviders} degraded, ${s.downProviders} down)`);
      ctx.output.write(`  Alerts:     ${s.activeAlerts}/${s.totalAlerts} active`);
      ctx.output.write(`  Anomalies:  ${s.activeAnomalies}/${s.totalAnomalies} active`);
      ctx.output.write(`  Avg Latency: ${formatMs(s.avgLatencyMs)}`);
      ctx.output.write(`  Health Score: ${s.overallHealthScore.toFixed(1)}/100`);
    } else {
      const h = data as ProviderHealth;
      ctx.output.info(`Provider: ${h.providerId}`);
      ctx.output.info(`  Status:     ${h.status}`);
      ctx.output.info(`  Latency:    ${formatMs(h.latencyMs)}`);
      ctx.output.info(`  Load:       ${formatPercent(h.load)}`);
      ctx.output.info(`  Region:     ${h.region}`);
      ctx.output.info(`  Models:     ${h.modelsServed.join(', ')}`);
      ctx.output.info(`  Failures:   ${h.consecutiveFailures}`);
      ctx.output.info(`  Last Beat:  ${h.lastHeartbeat}`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to fetch health data: ${err.message}`);
  }
}

// ── Subcommand: providers ──────────────────────────────────────────

async function handleProviders(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const statusFilter = args.options.status ? String(args.options.status) : undefined;
  const regionFilter = args.options.region ? String(args.options.region) : undefined;

  try {
    let providers: ProviderHealth[];

    if (ctx.client?.monitor?.listProviders) {
      providers = await ctx.client.monitor.listProviders({ status: statusFilter, region: regionFilter });
    } else {
      providers = [];
    }

    if (providers.length === 0) {
      ctx.output.info('No providers found matching criteria.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(providers, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = providers.map(p => ({
        ID: p.providerId.substring(0, 12) + '...',
        Status: p.status,
        Region: p.region,
        Latency: formatMs(p.latencyMs),
        Load: formatPercent(p.load),
        Models: p.modelsServed.length,
        Failures: p.consecutiveFailures,
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Providers (${providers.length})`));
      return;
    }

    for (const p of providers) {
      ctx.output.write(`${p.providerId}  [${p.status}]  ${p.region}  ${formatMs(p.latencyMs)}  load=${formatPercent(p.load)}`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to list providers: ${err.message}`);
  }
}

// ── Subcommand: uptime ─────────────────────────────────────────────

async function handleUptime(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const hours = args.options.hours ? Number(args.options.hours) : 24;

  try {
    let stats: UptimeStats[];

    if (ctx.client?.monitor?.getUptimeStats) {
      stats = await ctx.client.monitor.getUptimeStats({ providerId, hours });
    } else {
      stats = [];
    }

    if (stats.length === 0) {
      ctx.output.info('No uptime data available.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(stats, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = stats.map(s => ({
        Provider: s.providerId.substring(0, 12) + '...',
        Uptime: formatPercent(s.uptimePercent),
        'Avg Resp': formatMs(s.avgResponseTimeMs),
        'Error Rate': formatPercent(s.errorRate),
        Checks: s.totalChecks,
        Period: s.period,
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Uptime Stats (last ${hours}h)`));
      return;
    }

    for (const s of stats) {
      ctx.output.write(`${s.providerId}  uptime=${formatPercent(s.uptimePercent)}  avg=${formatMs(s.avgResponseTimeMs)}  errors=${formatPercent(s.errorRate)}  (${s.totalChecks} checks)`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to fetch uptime stats: ${err.message}`);
  }
}

// ── Subcommand: slas ───────────────────────────────────────────────

async function handleSlas(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  try {
    let data: { slas: SLADefinition[]; compliance: SLACompliance[] };

    if (ctx.client?.monitor?.getSLAs) {
      data = await ctx.client.monitor.getSLAs({ providerId });
    } else {
      data = { slas: [], compliance: [] };
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(data, null, 2));
      return;
    }

    if (data.slas.length === 0) {
      ctx.output.info('No SLA definitions found.');
      return;
    }

    ctx.output.info('SLA Definitions:');
    for (const sla of data.slas) {
      ctx.output.write(`  ${sla.id}: ${sla.name} (provider: ${sla.providerId})`);
      ctx.output.write(`    Target uptime: ${formatPercent(sla.targetUptime)}, Max resp: ${formatMs(sla.maxResponseTimeMs)}, Max err: ${formatPercent(sla.maxErrorRate)}`);
    }

    if (data.compliance.length > 0) {
      ctx.output.info('');
      ctx.output.info('SLA Compliance:');
      for (const c of data.compliance) {
        const icon = c.compliant ? '✓' : '✗';
        ctx.output.write(`  ${icon} ${c.slaId}: uptime=${formatPercent(c.uptimePercent)} (${c.violations} violations)`);
      }
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to fetch SLA data: ${err.message}`);
  }
}

// ── Subcommand: alerts ─────────────────────────────────────────────

async function handleAlerts(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const severity = args.options.severity ? String(args.options.severity) : undefined;
  const activeOnly = args.options['active-only'] === true;

  try {
    let alerts: AlertItem[];

    if (ctx.client?.monitor?.listAlerts) {
      alerts = await ctx.client.monitor.listAlerts({ severity, activeOnly });
    } else {
      alerts = [];
    }

    if (alerts.length === 0) {
      ctx.output.info('No alerts found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(alerts, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = alerts.map(a => ({
        ID: a.id.substring(0, 12) + '...',
        Rule: a.ruleName,
        Provider: a.providerId.substring(0, 12) + '...',
        Severity: a.severity.toUpperCase(),
        Ack: a.acknowledged ? '✓' : '✗',
        Time: new Date(a.triggeredAt).toISOString().slice(5, 16),
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Alerts (${alerts.length})`));
      return;
    }

    for (const a of alerts) {
      const ack = a.acknowledged ? '[ACK]' : '';
      ctx.output.write(`[${a.severity.toUpperCase()}] ${a.ruleName}: ${a.message} ${ack}`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to list alerts: ${err.message}`);
  }
}

// ── Subcommand: alerts/acknowledge ─────────────────────────────────

async function handleAlertsAcknowledge(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const alertId = args.options.id ? String(args.options.id) : '';

  if (!alertId) {
    ctx.output.writeError('Error: --id is required for alerts/acknowledge');
    process.exit(1);
  }

  try {
    if (ctx.client?.monitor?.acknowledgeAlert) {
      await ctx.client.monitor.acknowledgeAlert({ alertId });
      ctx.output.info(`Alert ${alertId} acknowledged.`);
    } else {
      ctx.output.info(`Alert ${alertId} acknowledged (mock).`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to acknowledge alert: ${err.message}`);
  }
}

// ── Subcommand: anomalies ──────────────────────────────────────────

async function handleAnomalies(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const severity = args.options.severity ? String(args.options.severity) : undefined;
  const activeOnly = args.options['active-only'] === true;

  try {
    let anomalies: AnomalyItem[];

    if (ctx.client?.monitor?.listAnomalies) {
      anomalies = await ctx.client.monitor.listAnomalies({ severity, activeOnly });
    } else {
      anomalies = [];
    }

    if (anomalies.length === 0) {
      ctx.output.info('No anomalies found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(anomalies, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = anomalies.map(a => ({
        ID: a.id.substring(0, 12) + '...',
        Type: a.anomalyType,
        Severity: a.severity.toUpperCase(),
        Providers: a.affectedProviders.length,
        Resolved: a.resolved ? '✓' : '✗',
        Time: new Date(a.detectedAt).toISOString().slice(5, 16),
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Anomalies (${anomalies.length})`));
      return;
    }

    for (const a of anomalies) {
      const status = a.resolved ? '[RESOLVED]' : '[ACTIVE]';
      ctx.output.write(`[${a.severity.toUpperCase()}] ${a.anomalyType}: ${a.description} ${status}`);
      ctx.output.write(`  Providers: ${a.affectedProviders.join(', ')}`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to list anomalies: ${err.message}`);
  }
}

// ── Subcommand: topology ──────────────────────────────────────────

async function handleTopology(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let nodes: TopologyNode[];

    if (ctx.client?.monitor?.getTopology) {
      nodes = await ctx.client.monitor.getTopology({});
    } else {
      nodes = [];
    }

    if (nodes.length === 0) {
      ctx.output.info('No topology data available.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(nodes, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = nodes.map(n => ({
        ID: n.nodeId.substring(0, 12) + '...',
        Type: n.nodeType,
        Region: n.region,
        Status: n.status,
        Version: n.version,
        Uptime: formatMs(n.uptimeMs),
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Network Topology (${nodes.length} nodes)`));
      return;
    }

    ctx.output.info('Network Topology:');
    for (const n of nodes) {
      ctx.output.write(`  ${n.nodeType}/${n.nodeId}  [${n.status}]  ${n.region}  v${n.version}  up=${formatMs(n.uptimeMs)}`);
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to fetch topology: ${err.message}`);
  }
}

// ── Subcommand: latency ────────────────────────────────────────────

async function handleLatency(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const showMatrix = args.options.matrix === true;

  try {
    let data: { pairs: LatencyPair[]; avgMs: number };

    if (ctx.client?.monitor?.getLatency) {
      data = await ctx.client.monitor.getLatency({ matrix: showMatrix });
    } else {
      data = { pairs: [], avgMs: 0 };
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(data, null, 2));
      return;
    }

    if (data.pairs.length === 0) {
      ctx.output.info('No latency data available.');
      return;
    }

    ctx.output.info(`Average Network Latency: ${formatMs(data.avgMs)}`);

    if (showMatrix) {
      ctx.output.info('');
      ctx.output.info('Latency Matrix:');
      const tableData = data.pairs.map(p => ({
        From: p.providerA.substring(0, 10),
        To: p.providerB.substring(0, 10),
        Latency: formatMs(p.latencyMs),
        Jitter: formatMs(p.jitterMs),
        Loss: (p.packetLoss * 100).toFixed(1) + '%',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Latency Pairs (${data.pairs.length})`));
    } else {
      for (const p of data.pairs) {
        ctx.output.write(`  ${p.providerA} <-> ${p.providerB}: ${formatMs(p.latencyMs)} (jitter: ${formatMs(p.jitterMs)}, loss: ${(p.packetLoss * 100).toFixed(1)}%)`);
      }
    }
  } catch (err: any) {
    ctx.output.writeError(`Failed to fetch latency data: ${err.message}`);
  }
}

// ── Subcommand: stats ──────────────────────────────────────────────

async function handleStats(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let stats: NetworkStats;

    if (ctx.client?.monitor?.getStats) {
      stats = await ctx.client.monitor.getStats({});
    } else {
      stats = {
        totalProviders: 0,
        activeProviders: 0,
        degradedProviders: 0,
        downProviders: 0,
        totalAlerts: 0,
        activeAlerts: 0,
        totalAnomalies: 0,
        activeAnomalies: 0,
        avgLatencyMs: 0,
        overallHealthScore: 0,
      };
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(stats, null, 2));
      return;
    }

    ctx.output.info('Xergon Network Statistics');
    ctx.output.info('==========================');
    ctx.output.write('');
    ctx.output.info('Providers:');
    ctx.output.write(`  Total:    ${stats.totalProviders}`);
    ctx.output.write(`  Active:   ${stats.activeProviders}`);
    ctx.output.write(`  Degraded: ${stats.degradedProviders}`);
    ctx.output.write(`  Down:     ${stats.downProviders}`);
    ctx.output.write('');
    ctx.output.info('Alerts:');
    ctx.output.write(`  Total:    ${stats.totalAlerts}`);
    ctx.output.write(`  Active:   ${stats.activeAlerts}`);
    ctx.output.write('');
    ctx.output.info('Anomalies:');
    ctx.output.write(`  Total:    ${stats.totalAnomalies}`);
    ctx.output.write(`  Active:   ${stats.activeAnomalies}`);
    ctx.output.write('');
    ctx.output.info('Performance:');
    ctx.output.write(`  Avg Latency:     ${formatMs(stats.avgLatencyMs)}`);
    ctx.output.write(`  Health Score:    ${stats.overallHealthScore.toFixed(1)}/100`);
  } catch (err: any) {
    ctx.output.writeError(`Failed to fetch stats: ${err.message}`);
  }
}

// ── Action dispatcher ──────────────────────────────────────────────

async function monitorAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = getSubcommand(args);

  switch (sub) {
    case 'health':
      await handleHealth(args, ctx);
      break;
    case 'providers':
      await handleProviders(args, ctx);
      break;
    case 'uptime':
      await handleUptime(args, ctx);
      break;
    case 'slas':
      await handleSlas(args, ctx);
      break;
    case 'alerts':
      await handleAlerts(args, ctx);
      break;
    case 'alerts/acknowledge':
      await handleAlertsAcknowledge(args, ctx);
      break;
    case 'anomalies':
      await handleAnomalies(args, ctx);
      break;
    case 'topology':
      await handleTopology(args, ctx);
      break;
    case 'latency':
      await handleLatency(args, ctx);
      break;
    case 'stats':
      await handleStats(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: health, providers, uptime, slas, alerts, alerts/acknowledge, anomalies, topology, latency, stats');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const monitorOptions: CommandOption[] = [
  {
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Provider ID to filter by',
    required: false,
    type: 'string',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter by provider status: healthy, degraded, down',
    required: false,
    type: 'string',
  },
  {
    name: 'region',
    short: '',
    long: '--region',
    description: 'Filter by provider region',
    required: false,
    type: 'string',
  },
  {
    name: 'hours',
    short: '',
    long: '--hours',
    description: 'Hours of uptime history to show (default: 24)',
    required: false,
    type: 'number',
  },
  {
    name: 'severity',
    short: '',
    long: '--severity',
    description: 'Filter by severity: info, warning, critical (alerts) or low, medium, high, critical (anomalies)',
    required: false,
    type: 'string',
  },
  {
    name: 'active-only',
    short: '',
    long: '--active-only',
    description: 'Show only active (unresolved) items',
    required: false,
    type: 'boolean',
  },
  {
    name: 'matrix',
    short: '',
    long: '--matrix',
    description: 'Show latency as a matrix table',
    required: false,
    type: 'boolean',
  },
  {
    name: 'id',
    short: '',
    long: '--id',
    description: 'Alert ID to acknowledge',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Output format: text, json, or table',
    required: false,
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const monitorCommand: Command = {
  name: 'monitor',
  description: 'Monitor network health, providers, uptime, SLAs, alerts, and anomalies',
  aliases: ['mon', 'health'],
  options: monitorOptions,
  action: monitorAction,
};
