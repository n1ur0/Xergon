/**
 * CLI command: metrics
 *
 * Real-time metrics, dashboards, and analytics for the Xergon Network.
 *
 * Usage:
 *   xergon metrics dashboard     -- Real-time metrics dashboard (ANSI)
 *   xergon metrics query <name>  -- Query a specific metric
 *   xergon metrics top           -- Top models/providers by metric
 *   xergon metrics history       -- Metric history with chart
 *   xergon metrics alerts        -- Active metric alerts
 *   xergon metrics export        -- Export metrics data
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

type MetricCategory = 'latency' | 'throughput' | 'error' | 'cost' | 'availability' | 'quality';
type TimeRange = '1h' | '6h' | '24h' | '7d' | '30d' | '90d';
type ExportFormat = 'json' | 'csv';
type AlertSeverity = 'critical' | 'warning' | 'info';
type SortDirection = 'asc' | 'desc';

interface MetricPoint {
  timestamp: string;
  value: number;
  label?: string;
}

interface MetricDefinition {
  name: string;
  label: string;
  category: MetricCategory;
  unit: string;
  description: string;
  thresholds: { warning: number; critical: number; direction: 'above' | 'below' };
}

interface MetricQueryResult {
  metric: string;
  label: string;
  current: number;
  unit: string;
  category: MetricCategory;
  points: MetricPoint[];
  timeRange: TimeRange;
  thresholds: { warning: number; critical: number; direction: 'above' | 'below' };
  status: 'healthy' | 'warning' | 'critical';
  change: number; // percentage change over period
}

interface TopEntry {
  name: string;
  value: number;
  unit: string;
  change: number;
  rank: number;
}

interface MetricAlert {
  id: string;
  metric: string;
  label: string;
  severity: AlertSeverity;
  message: string;
  current: number;
  threshold: number;
  direction: 'above' | 'below';
  timestamp: string;
  acknowledged: boolean;
  duration: string;
}

interface DashboardData {
  timestamp: string;
  uptime: number;
  totalRequests: number;
  activeProviders: number;
  activeModels: number;
  metrics: Array<{
    name: string;
    label: string;
    current: number;
    unit: string;
    status: 'healthy' | 'warning' | 'critical';
    sparkline: string;
  }>;
  topProviders: TopEntry[];
  topModels: TopEntry[];
  alerts: MetricAlert[];
}

// ── Constants ──────────────────────────────────────────────────────

const TIME_RANGES: TimeRange[] = ['1h', '6h', '24h', '7d', '30d', '90d'];

function parseTimeRange(range: string): number {
  const match = range.match(/^(\d+)(h|d)$/);
  if (!match) return 3600_000;
  const val = parseInt(match[1], 10);
  const unit = match[2];
  return unit === 'h' ? val * 3600_000 : val * 86400_000;
}

const METRIC_DEFINITIONS: MetricDefinition[] = [
  {
    name: 'p50_latency',
    label: 'P50 Latency',
    category: 'latency',
    unit: 'ms',
    description: 'Median request latency',
    thresholds: { warning: 2000, critical: 5000, direction: 'above' },
  },
  {
    name: 'p99_latency',
    label: 'P99 Latency',
    category: 'latency',
    unit: 'ms',
    description: '99th percentile request latency',
    thresholds: { warning: 5000, critical: 15000, direction: 'above' },
  },
  {
    name: 'requests_per_sec',
    label: 'Requests/sec',
    category: 'throughput',
    unit: 'req/s',
    description: 'Average requests per second',
    thresholds: { warning: 10, critical: 1, direction: 'below' },
  },
  {
    name: 'error_rate',
    label: 'Error Rate',
    category: 'error',
    unit: '%',
    description: 'Percentage of failed requests',
    thresholds: { warning: 5, critical: 15, direction: 'above' },
  },
  {
    name: 'availability',
    label: 'Availability',
    category: 'availability',
    unit: '%',
    description: 'Service uptime percentage',
    thresholds: { warning: 99, critical: 95, direction: 'below' },
  },
  {
    name: 'cost_per_1k_tokens',
    label: 'Cost/1K Tokens',
    category: 'cost',
    unit: '$',
    description: 'Average cost per 1000 tokens',
    thresholds: { warning: 0.05, critical: 0.15, direction: 'above' },
  },
  {
    name: 'token_throughput',
    label: 'Token Throughput',
    category: 'throughput',
    unit: 'tok/s',
    description: 'Average tokens generated per second',
    thresholds: { warning: 100, critical: 20, direction: 'below' },
  },
  {
    name: 'quality_score',
    label: 'Quality Score',
    category: 'quality',
    unit: '',
    description: 'Composite quality score (0-100)',
    thresholds: { warning: 70, critical: 50, direction: 'below' },
  },
];

// ── Sparkline ──────────────────────────────────────────────────────

const SPARKLINE_CHARS = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

function generateSparkline(values: number[], width: number = 20): string {
  if (values.length === 0) return '·'.repeat(width);
  if (values.length === 1) return SPARKLINE_CHARS[4].repeat(width);

  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min;

  if (range === 0) return SPARKLINE_CHARS[4].repeat(width);

  // Resample to width
  const step = Math.max(1, Math.floor(values.length / width));
  const sampled: number[] = [];
  for (let i = 0; i < width && i * step < values.length; i++) {
    sampled.push(values[i * step]);
  }

  return sampled.map(v => {
    const normalized = (v - min) / range;
    const idx = Math.min(SPARKLINE_CHARS.length - 1, Math.floor(normalized * SPARKLINE_CHARS.length));
    return SPARKLINE_CHARS[idx];
  }).join('');
}

// ── Threshold evaluation ───────────────────────────────────────────

function evaluateStatus(
  value: number,
  thresholds: { warning: number; critical: number; direction: 'above' | 'below' },
): 'healthy' | 'warning' | 'critical' {
  if (thresholds.direction === 'above') {
    if (value >= thresholds.critical) return 'critical';
    if (value >= thresholds.warning) return 'warning';
    return 'healthy';
  } else {
    if (value <= thresholds.critical) return 'critical';
    if (value <= thresholds.warning) return 'warning';
    return 'healthy';
  }
}

function statusColor(status: 'healthy' | 'warning' | 'critical', text: string, useColor: boolean): string {
  if (!useColor) return text;
  const codes: Record<string, string> = {
    healthy: '\x1b[32m',
    warning: '\x1b[33m',
    critical: '\x1b[31m',
  };
  return `${codes[status]}${text}\x1b[0m`;
}

function severityColor(severity: AlertSeverity, text: string, useColor: boolean): string {
  if (!useColor) return text;
  const codes: Record<string, string> = {
    critical: '\x1b[31m',
    warning: '\x1b[33m',
    info: '\x1b[36m',
  };
  return `${codes[severity]}${text}\x1b[0m`;
}

function formatNumber(n: number, decimals: number = 2): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toFixed(decimals);
}

// ── Metrics API service ────────────────────────────────────────────

class MetricsService {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  private async fetchJSON<T>(url: string, timeoutMs: number = 15_000): Promise<T | null> {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) return null;
      return await res.json() as T;
    } catch {
      return null;
    }
  }

  async queryMetric(name: string, timeRange: TimeRange): Promise<MetricQueryResult | null> {
    const ms = parseTimeRange(timeRange);
    const since = new Date(Date.now() - ms).toISOString();
    const data = await this.fetchJSON<any>(
      `${this.baseUrl}/api/v1/metrics/${name}?since=${since}&range=${timeRange}`,
    );
    return data ? normalizeMetricQuery(data, name, timeRange) : null;
  }

  async getTopResources(type: 'providers' | 'models', metric: string, limit: number): Promise<TopEntry[] | null> {
    const data = await this.fetchJSON<any>(
      `${this.baseUrl}/api/v1/metrics/top/${type}?metric=${metric}&limit=${limit}`,
    );
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.items ?? data.data ?? []);
      return items.map(normalizeTopEntry);
    }
    return null;
  }

  async getAlerts(): Promise<MetricAlert[] | null> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/metrics/alerts`);
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.alerts ?? data.data ?? []);
      return items.map(normalizeAlert);
    }
    return null;
  }

  async getHistory(name: string, timeRange: TimeRange): Promise<MetricPoint[]> {
    const ms = parseTimeRange(timeRange);
    const since = new Date(Date.now() - ms).toISOString();
    const data = await this.fetchJSON<any>(
      `${this.baseUrl}/api/v1/metrics/${name}/history?since=${since}`,
    );
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.points ?? data.data ?? []);
      return items.map(normalizeMetricPoint);
    }
    return [];
  }
}

// ── Normalizers ────────────────────────────────────────────────────

function normalizeMetricPoint(raw: any): MetricPoint {
  return {
    timestamp: raw.timestamp ?? raw.time ?? new Date().toISOString(),
    value: Number(raw.value ?? raw.count ?? 0),
    label: raw.label,
  };
}

function normalizeMetricQuery(raw: any, name: string, timeRange: TimeRange): MetricQueryResult {
  const def = METRIC_DEFINITIONS.find(d => d.name === name);
  const points: any[] = Array.isArray(raw.points) ? raw.points : (raw.data ?? []);
  const current = raw.current ?? (points.length > 0 ? points[points.length - 1].value : 0);
  const thresholds = raw.thresholds ?? def?.thresholds ?? { warning: 0, critical: 0, direction: 'above' as const };

  return {
    metric: name,
    label: raw.label ?? def?.label ?? name,
    current: Number(current),
    unit: raw.unit ?? def?.unit ?? '',
    category: raw.category ?? def?.category ?? 'throughput',
    points: points.map(normalizeMetricPoint),
    timeRange,
    thresholds,
    status: evaluateStatus(Number(current), thresholds),
    change: Number(raw.change ?? 0),
  };
}

function normalizeTopEntry(raw: any, index?: number): TopEntry {
  return {
    name: raw.name ?? raw.id ?? raw.model ?? raw.provider ?? 'unknown',
    value: Number(raw.value ?? raw.score ?? raw.count ?? 0),
    unit: raw.unit ?? '',
    change: Number(raw.change ?? 0),
    rank: raw.rank ?? (index ?? 0) + 1,
  };
}

function normalizeAlert(raw: any): MetricAlert {
  return {
    id: raw.id ?? `alert-${Date.now().toString(36)}`,
    metric: raw.metric ?? raw.name ?? 'unknown',
    label: raw.label ?? raw.metric ?? 'Unknown Metric',
    severity: raw.severity ?? 'warning',
    message: raw.message ?? raw.msg ?? 'Metric alert',
    current: Number(raw.current ?? raw.value ?? 0),
    threshold: Number(raw.threshold ?? raw.limit ?? 0),
    direction: raw.direction ?? 'above',
    timestamp: raw.timestamp ?? new Date().toISOString(),
    acknowledged: raw.acknowledged ?? false,
    duration: raw.duration ?? 'unknown',
  };
}

// ── Mock data generators ───────────────────────────────────────────

function generateMetricPoints(baseValue: number, variance: number, count: number): MetricPoint[] {
  const points: MetricPoint[] = [];
  const now = Date.now();
  for (let i = count - 1; i >= 0; i--) {
    const noise = (Math.random() - 0.5) * 2 * variance;
    points.push({
      timestamp: new Date(now - i * 60_000).toISOString(),
      value: Math.max(0, baseValue + noise),
    });
  }
  return points;
}

function mockMetricQuery(name: string, timeRange: TimeRange): MetricQueryResult {
  const def = METRIC_DEFINITIONS.find(d => d.name === name);
  const baseValues: Record<string, number> = {
    p50_latency: 850,
    p99_latency: 3200,
    requests_per_sec: 245,
    error_rate: 2.1,
    availability: 99.7,
    cost_per_1k_tokens: 0.018,
    token_throughput: 1240,
    quality_score: 87,
  };
  const variances: Record<string, number> = {
    p50_latency: 300,
    p99_latency: 1500,
    requests_per_sec: 80,
    error_rate: 1.5,
    availability: 0.3,
    cost_per_1k_tokens: 0.005,
    token_throughput: 400,
    quality_score: 5,
  };

  const base = baseValues[name] ?? 100;
  const variance = variances[name] ?? 20;
  const rangeMs = parseTimeRange(timeRange);
  const pointCount = Math.min(60, Math.max(12, Math.floor(rangeMs / 60_000)));
  const points = generateMetricPoints(base, variance, pointCount);
  const current = points[points.length - 1].value;
  const change = ((current - points[0].value) / Math.max(1, points[0].value)) * 100;

  return {
    metric: name,
    label: def?.label ?? name,
    current,
    unit: def?.unit ?? '',
    category: def?.category ?? 'throughput',
    points,
    timeRange,
    thresholds: def?.thresholds ?? { warning: 0, critical: 0, direction: 'above' as const },
    status: evaluateStatus(current, def?.thresholds ?? { warning: 0, critical: 0, direction: 'above' }),
    change,
  };
}

function mockTopProviders(): TopEntry[] {
  const providers = [
    { name: 'provider-001', value: 15420, change: 12.3 },
    { name: 'provider-002', value: 12890, change: -3.1 },
    { name: 'provider-003', value: 11200, change: 8.7 },
    { name: 'provider-004', value: 9650, change: -1.2 },
    { name: 'provider-005', value: 8340, change: 15.6 },
  ];
  return providers.map((p, i) => ({ ...p, unit: 'req/s', rank: i + 1 }));
}

function mockTopModels(): TopEntry[] {
  const models = [
    { name: 'llama-3.3-70b', value: 24500, change: 5.2 },
    { name: 'llama-3.1-8b', value: 18900, change: -2.4 },
    { name: 'mixtral-8x7b', value: 15200, change: 18.1 },
    { name: 'mistral-7b', value: 12100, change: 0.8 },
    { name: 'qwen-72b', value: 9800, change: -7.3 },
  ];
  return models.map((m, i) => ({ ...m, unit: 'req/s', rank: i + 1 }));
}

function mockAlerts(): MetricAlert[] {
  const now = new Date();
  return [
    {
      id: 'm-alert-001',
      metric: 'p99_latency',
      label: 'P99 Latency',
      severity: 'critical',
      message: 'P99 latency exceeded 15s threshold on relay',
      current: 18200,
      threshold: 15000,
      direction: 'above',
      timestamp: new Date(now.getTime() - 300_000).toISOString(),
      acknowledged: false,
      duration: '5m',
    },
    {
      id: 'm-alert-002',
      metric: 'error_rate',
      label: 'Error Rate',
      severity: 'warning',
      message: 'Error rate elevated to 7.2% (threshold: 5%)',
      current: 7.2,
      threshold: 5,
      direction: 'above',
      timestamp: new Date(now.getTime() - 900_000).toISOString(),
      acknowledged: false,
      duration: '15m',
    },
    {
      id: 'm-alert-003',
      metric: 'availability',
      label: 'Availability',
      severity: 'warning',
      message: 'Provider provider-003 availability dropped to 97.1%',
      current: 97.1,
      threshold: 99,
      direction: 'below',
      timestamp: new Date(now.getTime() - 1800_000).toISOString(),
      acknowledged: true,
      duration: '30m',
    },
    {
      id: 'm-alert-004',
      metric: 'cost_per_1k_tokens',
      label: 'Cost/1K Tokens',
      severity: 'info',
      message: 'Token cost trending upward: $0.045/1K (24h avg)',
      current: 0.045,
      threshold: 0.05,
      direction: 'above',
      timestamp: new Date(now.getTime() - 3600_000).toISOString(),
      acknowledged: false,
      duration: '1h',
    },
  ];
}

// ── Subcommand: dashboard ──────────────────────────────────────────

async function handleDashboard(
  args: ParsedArgs,
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const refresh = args.options.refresh ? Number(args.options.refresh) : 0;
  const useColor = ctx.config.color;

  // Collect data
  const metricsService = new MetricsService(ctx.config.baseUrl);
  const dashboardMetrics: DashboardData['metrics'] = [];

  for (const def of METRIC_DEFINITIONS) {
    const serverResult = await metricsService.queryMetric(def.name, '1h');
    const result = serverResult ?? mockMetricQuery(def.name, '1h');

    dashboardMetrics.push({
      name: result.metric,
      label: result.label,
      current: result.current,
      unit: result.unit,
      status: result.status,
      sparkline: generateSparkline(result.points.map(p => p.value)),
    });
  }

  const topProviders = mockTopProviders();
  const topModels = mockTopModels();
  const alerts = mockAlerts();

  const dashboard: DashboardData = {
    timestamp: new Date().toISOString(),
    uptime: 99.95,
    totalRequests: 2450000,
    activeProviders: 47,
    activeModels: 23,
    metrics: dashboardMetrics,
    topProviders,
    topModels,
    alerts: alerts.filter(a => !a.acknowledged),
  };

  if (outputJson) {
    ctx.output.write(JSON.stringify(dashboard, null, 2));
    return;
  }

  // ANSI dashboard rendering
  const clear = useColor ? '\x1b[2J\x1b[H' : '';
  const reset = '\x1b[0m';
  const bold = useColor ? '\x1b[1m' : '';
  const dim = useColor ? '\x1b[2m' : '';
  const cyan = useColor ? '\x1b[36m' : '';
  const green = useColor ? '\x1b[32m' : '';
  const yellow = useColor ? '\x1b[33m' : '';
  const red = useColor ? '\x1b[31m' : '';
  const sep = dim + '\u2500'.repeat(60) + reset;

  ctx.output.write(clear);
  ctx.output.write(`${bold}${cyan}  XERGON NETWORK -- METRICS DASHBOARD${reset}`);
  ctx.output.write(sep);
  ctx.output.write(`  ${green}Uptime:${reset} ${dashboard.uptime}%    ` +
    `${green}Requests:${reset} ${formatNumber(dashboard.totalRequests)}    ` +
    `${green}Providers:${reset} ${dashboard.activeProviders}    ` +
    `${green}Models:${reset} ${dashboard.activeModels}`);
  ctx.output.write(`  ${dim}Updated: ${dashboard.timestamp}${reset}`);
  ctx.output.write('');

  // Metrics grid
  ctx.output.write(`  ${bold}${yellow}NETWORK METRICS${reset}`);
  ctx.output.write(sep);

  for (const m of dashboardMetrics) {
    const statusMark = m.status === 'healthy'
      ? `${green}\u2713${reset}`
      : m.status === 'warning'
        ? `${yellow}\u26A0${reset}`
        : `${red}\u2717${reset}`;

    const valueColor = m.status === 'healthy' ? green : m.status === 'warning' ? yellow : red;
    const changeStr = m.status !== 'healthy'
      ? `  ${m.status === 'critical' ? red : yellow}${m.status.toUpperCase()}${reset}`
      : '';

    ctx.output.write(
      `  ${statusMark} ${m.label.padEnd(18)} ${valueColor}${formatNumber(m.current)}${m.unit.padStart(4)}${reset}  ${m.sparkline}${changeStr}`
    );
  }

  // Top providers
  ctx.output.write('');
  ctx.output.write(`  ${bold}${yellow}TOP PROVIDERS${reset}`);
  ctx.output.write(sep);
  for (const p of dashboard.topProviders.slice(0, 5)) {
    const changeColor = p.change >= 0 ? green : red;
    const changeMark = p.change >= 0 ? '\u2191' : '\u2193';
    ctx.output.write(
      `  ${String(p.rank).padStart(2)}. ${p.name.padEnd(20)} ${formatNumber(p.value).padStart(8)} ${p.unit}  ${changeColor}${changeMark}${Math.abs(p.change).toFixed(1)}%${reset}`
    );
  }

  // Top models
  ctx.output.write('');
  ctx.output.write(`  ${bold}${yellow}TOP MODELS${reset}`);
  ctx.output.write(sep);
  for (const m of dashboard.topModels.slice(0, 5)) {
    const changeColor = m.change >= 0 ? green : red;
    const changeMark = m.change >= 0 ? '\u2191' : '\u2193';
    ctx.output.write(
      `  ${String(m.rank).padStart(2)}. ${m.name.padEnd(20)} ${formatNumber(m.value).padStart(8)} ${m.unit}  ${changeColor}${changeMark}${Math.abs(m.change).toFixed(1)}%${reset}`
    );
  }

  // Active alerts
  if (dashboard.alerts.length > 0) {
    ctx.output.write('');
    ctx.output.write(`  ${bold}${red}ACTIVE ALERTS (${dashboard.alerts.length})${reset}`);
    ctx.output.write(sep);
    for (const a of dashboard.alerts) {
      const sevIcon = a.severity === 'critical' ? `${red}\u25CF${reset}` : `${yellow}\u25CF${reset}`;
      ctx.output.write(`  ${sevIcon} ${a.label}: ${a.message}`);
      ctx.output.write(`      ${dim}Current: ${a.current} | Threshold: ${a.threshold} | Duration: ${a.duration}${reset}`);
    }
  }

  ctx.output.write('');
  ctx.output.write(`  ${dim}Press Ctrl+C to exit${reset}`);
}

// ── Subcommand: query ──────────────────────────────────────────────

async function handleQuery(
  args: ParsedArgs,
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const metricName = args.positional[1];
  if (!metricName) {
    ctx.output.writeError('Usage: xergon metrics query <metric_name>');
    ctx.output.info(`Available metrics: ${METRIC_DEFINITIONS.map(d => d.name).join(', ')}`);
    process.exit(1);
    return;
  }

  const def = METRIC_DEFINITIONS.find(d => d.name === metricName);
  if (!def) {
    // Fuzzy match
    const lower = metricName.toLowerCase();
    const match = METRIC_DEFINITIONS.find(d => d.name.includes(lower) || d.label.toLowerCase().includes(lower));
    if (!match) {
      ctx.output.writeError(`Unknown metric: "${metricName}"`);
      ctx.output.info(`Available: ${METRIC_DEFINITIONS.map(d => d.name).join(', ')}`);
      process.exit(1);
      return;
    }
  }

  const timeRange = (args.options.range as TimeRange) ?? '1h';
  if (!TIME_RANGES.includes(timeRange)) {
    ctx.output.writeError(`Invalid time range: "${timeRange}". Valid: ${TIME_RANGES.join(', ')}`);
    process.exit(1);
    return;
  }

  const useColor = ctx.config.color;
  const metricsService = new MetricsService(ctx.config.baseUrl);
  const serverResult = await metricsService.queryMetric(metricName, timeRange);
  const result = serverResult ?? mockMetricQuery(metricName, timeRange);

  if (outputJson) {
    ctx.output.write(JSON.stringify(result, null, 2));
    return;
  }

  const statusColorStr = statusColor(result.status, result.status.toUpperCase(), useColor);
  const changeIcon = result.change >= 0 ? '\u2191' : '\u2193';
  const changeColorStr = result.change >= 0
    ? (useColor ? '\x1b[32m' : '')
    : (useColor ? '\x1b[31m' : '');
  const reset = '\x1b[0m';

  ctx.output.write(ctx.output.colorize(`Metric: ${result.label}`, 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));
  ctx.output.write(`  Current:     ${formatNumber(result.current)} ${result.unit}`);
  ctx.output.write(`  Status:      ${statusColorStr}`);
  ctx.output.write(`  Change:      ${changeColorStr}${changeIcon} ${Math.abs(result.change).toFixed(1)}%${reset}`);
  ctx.output.write(`  Time Range:  ${result.timeRange}`);
  ctx.output.write(`  Category:    ${result.category}`);
  ctx.output.write(`  Warning:     ${result.thresholds.direction === 'above' ? '>' : '<'} ${result.thresholds.warning} ${result.unit}`);
  ctx.output.write(`  Critical:    ${result.thresholds.direction === 'above' ? '>' : '<'} ${result.thresholds.critical} ${result.unit}`);
  ctx.output.write('');

  // Sparkline
  ctx.output.write(`  ${ctx.output.colorize('Trend:', 'yellow')} ${generateSparkline(result.points.map(p => p.value), 40)}`);

  // Recent points
  ctx.output.write('');
  ctx.output.write(`  ${ctx.output.colorize('Recent values:', 'yellow')}`);
  const recent = result.points.slice(-5);
  for (const p of recent) {
    const time = new Date(p.timestamp).toLocaleTimeString();
    ctx.output.write(`    ${time.padEnd(12)} ${formatNumber(p.value).padStart(10)} ${result.unit}`);
  }
}

// ── Subcommand: top ────────────────────────────────────────────────

async function handleTop(
  args: ParsedArgs,
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const type = (args.options.type as 'providers' | 'models') ?? 'providers';
  const metric = args.options.metric ? String(args.options.metric) : 'requests_per_sec';
  const limit = args.options.limit ? Number(args.options.limit) : 10;
  const sort = (args.options.sort as SortDirection) ?? 'desc';
  const useColor = ctx.config.color;

  const metricsService = new MetricsService(ctx.config.baseUrl);
  const serverResult = type === 'providers'
    ? await metricsService.getTopResources('providers', metric, limit)
    : await metricsService.getTopResources('models', metric, limit);

  const entries = serverResult ?? (type === 'providers' ? mockTopProviders() : mockTopModels());
  const sorted = [...entries].sort((a, b) => sort === 'desc' ? b.value - a.value : a.value - b.value).slice(0, limit);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ type, metric, sort, entries: sorted }, null, 2));
    return;
  }

  const reset = '\x1b[0m';
  const title = type === 'providers' ? 'Top Providers' : 'Top Models';
  ctx.output.write(ctx.output.colorize(`${title} by ${metric}`, 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));

  for (const entry of sorted) {
    const changeColor = entry.change >= 0
      ? (useColor ? '\x1b[32m' : '')
      : (useColor ? '\x1b[31m' : '');
    const changeIcon = entry.change >= 0 ? '\u2191' : '\u2193';
    const rankStr = useColor ? `\x1b[1m${String(entry.rank).padStart(2)}${reset}` : String(entry.rank).padStart(2);

    ctx.output.write(
      `  ${rankStr}. ${entry.name.padEnd(22)} ${formatNumber(entry.value).padStart(10)} ${entry.unit}  ${changeColor}${changeIcon}${Math.abs(entry.change).toFixed(1)}%${reset}`
    );
  }

  ctx.output.write('');
  ctx.output.info(`Showing ${sorted.length} of ${entries.length} ${type}`);
}

// ── Subcommand: history ────────────────────────────────────────────

async function handleHistory(
  args: ParsedArgs,
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const metricName = args.positional[1];
  if (!metricName) {
    ctx.output.writeError('Usage: xergon metrics history <metric_name>');
    process.exit(1);
    return;
  }

  const timeRange = (args.options.range as TimeRange) ?? '24h';
  const def = METRIC_DEFINITIONS.find(d => d.name === metricName);

  const metricsService = new MetricsService(ctx.config.baseUrl);
  const serverPoints = await metricsService.getHistory(metricName, timeRange);
  const points = serverPoints.length > 0
    ? serverPoints
    : mockMetricQuery(metricName, timeRange).points;

  if (outputJson) {
    ctx.output.write(JSON.stringify({
      metric: metricName,
      label: def?.label ?? metricName,
      timeRange,
      pointCount: points.length,
      points,
    }, null, 2));
    return;
  }

  const label = def?.label ?? metricName;
  const unit = def?.unit ?? '';
  const thresholds = def?.thresholds;

  ctx.output.write(ctx.output.colorize(`${label} -- History (${timeRange})`, 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim'));

  if (points.length === 0) {
    ctx.output.write('  No data available for this time range');
    return;
  }

  // Sparkline
  ctx.output.write('');
  ctx.output.write(`  ${ctx.output.colorize('Trend:', 'yellow')} ${generateSparkline(points.map(p => p.value), 50)}`);

  // Stats
  const values = points.map(p => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const avg = values.reduce((a, b) => a + b, 0) / values.length;
  const latest = values[values.length - 1];

  ctx.output.write('');
  ctx.output.write(`  Latest:  ${formatNumber(latest)} ${unit}`);
  ctx.output.write(`  Average: ${formatNumber(avg)} ${unit}`);
  ctx.output.write(`  Min:     ${formatNumber(min)} ${unit}`);
  ctx.output.write(`  Max:     ${formatNumber(max)} ${unit}`);

  if (thresholds) {
    const currentStatus = evaluateStatus(latest, thresholds);
    ctx.output.write(`  Status:  ${statusColor(currentStatus, currentStatus.toUpperCase(), ctx.config.color)}`);
    ctx.output.write(`  Warn:    ${thresholds.direction === 'above' ? '>' : '<'} ${thresholds.warning} ${unit}`);
    ctx.output.write(`  Crit:    ${thresholds.direction === 'above' ? '>' : '<'} ${thresholds.critical} ${unit}`);
  }

  // Sample points table
  ctx.output.write('');
  ctx.output.write(`  ${ctx.output.colorize('Sample Data Points:', 'yellow')}`);
  const step = Math.max(1, Math.floor(points.length / 10));
  for (let i = 0; i < points.length; i += step) {
    const p = points[i];
    const time = new Date(p.timestamp).toLocaleString();
    ctx.output.write(`    ${time.padEnd(22)} ${formatNumber(p.value).padStart(10)} ${unit}`);
  }
}

// ── Subcommand: alerts ─────────────────────────────────────────────

async function handleAlerts(
  args: ParsedArgs,
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const severity = args.options.severity ? String(args.options.severity) : undefined;
  const useColor = ctx.config.color;

  const metricsService = new MetricsService(ctx.config.baseUrl);
  const serverAlerts = await metricsService.getAlerts();
  const allAlerts = serverAlerts ?? mockAlerts();

  let alerts = allAlerts;
  if (severity) {
    alerts = alerts.filter(a => a.severity === severity);
  }

  if (outputJson) {
    ctx.output.write(JSON.stringify({ total: alerts.length, alerts }, null, 2));
    return;
  }

  const reset = '\x1b[0m';
  const criticalCount = alerts.filter(a => a.severity === 'critical' && !a.acknowledged).length;
  const warningCount = alerts.filter(a => a.severity === 'warning' && !a.acknowledged).length;

  ctx.output.write(ctx.output.colorize('Metric Alerts', 'bold'));
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));

  if (criticalCount > 0) {
    ctx.output.write(`  ${useColor ? '\x1b[31m' : ''}\u25CF ${criticalCount} critical${reset}`);
  }
  if (warningCount > 0) {
    ctx.output.write(`  ${useColor ? '\x1b[33m' : ''}\u25CF ${warningCount} warning${reset}`);
  }

  ctx.output.write('');

  for (const a of alerts) {
    const sevIcon = a.severity === 'critical'
      ? `${useColor ? '\x1b[31m' : ''}\u25CF CRITICAL${reset}`
      : a.severity === 'warning'
        ? `${useColor ? '\x1b[33m' : ''}\u25CF WARNING${reset}`
        : `${useColor ? '\x1b[36m' : ''}\u25CF INFO${reset}`;

    const ackStr = a.acknowledged ? `${useColor ? '\x1b[2m' : ''}[ACK]${reset}` : '';

    ctx.output.write(`  ${sevIcon} ${ackStr} ${a.label}`);
    ctx.output.write(`    ${a.message}`);
    ctx.output.write(`    ${useColor ? '\x1b[2m' : ''}Current: ${a.current} | Threshold: ${a.threshold} (${a.direction}) | Duration: ${a.duration} | Since: ${a.timestamp}${reset}`);
    ctx.output.write('');
  }

  if (alerts.length === 0) {
    ctx.output.success('No active alerts');
  }

  ctx.output.info(`Total: ${alerts.length} alert(s)`);
}

// ── Subcommand: export ─────────────────────────────────────────────

async function handleExport(
  args: ParsedArgs,
  ctx: CLIContext,
): Promise<void> {
  const format = (args.options.format as ExportFormat) ?? 'json';
  const metric = args.options.metric ? String(args.options.metric) : undefined;
  const timeRange = (args.options.range as TimeRange) ?? '24h';
  const outputPath = args.options.output ? String(args.options.output) : undefined;

  const metricsService = new MetricsService(ctx.config.baseUrl);

  const metricsToExport = metric
    ? [metric]
    : METRIC_DEFINITIONS.map(d => d.name);

  const exportData: Record<string, MetricQueryResult> = {};

  for (const name of metricsToExport) {
    const serverResult = await metricsService.queryMetric(name, timeRange);
    exportData[name] = serverResult ?? mockMetricQuery(name, timeRange);
  }

  let output: string;
  if (format === 'csv') {
    // CSV export: one row per metric with current values
    const header = 'metric,label,category,current,unit,status,change_pct,time_range';
    const rows = Object.values(exportData).map(r =>
      `${r.metric},${r.label},${r.category},${r.current},${r.unit},${r.status},${r.change.toFixed(1)},${r.timeRange}`
    );
    output = [header, ...rows].join('\n');
  } else {
    output = JSON.stringify({
      exportedAt: new Date().toISOString(),
      timeRange,
      metrics: exportData,
    }, null, 2);
  }

  if (outputPath) {
    const fs = await import('node:fs');
    fs.writeFileSync(outputPath, output);
    ctx.output.success(`Metrics exported to ${outputPath}`);
  } else {
    ctx.output.write(output);
  }
}

// ── Options ────────────────────────────────────────────────────────

const metricsOptions: CommandOption[] = [
  {
    name: 'range',
    short: '-r',
    long: '--range',
    description: 'Time range (1h, 6h, 24h, 7d, 30d, 90d)',
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
    name: 'refresh',
    short: '',
    long: '--refresh',
    description: 'Dashboard refresh interval in seconds',
    required: false,
    type: 'number',
  },
  {
    name: 'type',
    short: '-t',
    long: '--type',
    description: 'Resource type for top (providers, models)',
    required: false,
    type: 'string',
  },
  {
    name: 'metric',
    short: '-m',
    long: '--metric',
    description: 'Specific metric name',
    required: false,
    type: 'string',
  },
  {
    name: 'limit',
    short: '-l',
    long: '--limit',
    description: 'Number of results to show',
    required: false,
    type: 'number',
  },
  {
    name: 'sort',
    short: '',
    long: '--sort',
    description: 'Sort direction (asc, desc)',
    required: false,
    type: 'string',
  },
  {
    name: 'severity',
    short: '',
    long: '--severity',
    description: 'Filter alerts by severity',
    required: false,
    type: 'string',
  },
  {
    name: 'format',
    short: '-f',
    long: '--format',
    description: 'Export format (json, csv)',
    required: false,
    type: 'string',
  },
  {
    name: 'output',
    short: '-o',
    long: '--output',
    description: 'Output file path for export',
    required: false,
    type: 'string',
  },
];

// ── Command action ─────────────────────────────────────────────────

async function metricsAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const sub = args.positional[0];

  switch (sub) {
    case 'dashboard':
    case 'dash': {
      await handleDashboard(args, ctx, outputJson);
      break;
    }

    case 'query':
    case 'q': {
      await handleQuery(args, ctx, outputJson);
      break;
    }

    case 'top': {
      await handleTop(args, ctx, outputJson);
      break;
    }

    case 'history':
    case 'hist': {
      await handleHistory(args, ctx, outputJson);
      break;
    }

    case 'alerts':
    case 'alert': {
      await handleAlerts(args, ctx, outputJson);
      break;
    }

    case 'export': {
      await handleExport(args, ctx);
      break;
    }

    default: {
      // Default: show dashboard
      await handleDashboard(args, ctx, outputJson);
      break;
    }
  }
}

// ── Command definition ─────────────────────────────────────────────

export const metricsCommand: Command = {
  name: 'metrics',
  description: 'Real-time metrics, dashboards, and analytics',
  aliases: ['stats', 'analytics'],
  options: metricsOptions,
  action: metricsAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  generateSparkline,
  evaluateStatus,
  statusColor,
  severityColor,
  formatNumber,
  parseTimeRange,
  normalizeMetricPoint,
  normalizeMetricQuery,
  normalizeTopEntry,
  normalizeAlert,
  mockMetricQuery,
  mockTopProviders,
  mockTopModels,
  mockAlerts,
  mockAlerts as generateMockAlerts,
  MetricsService,
  metricsAction,
  handleDashboard,
  handleQuery,
  handleTop,
  handleHistory,
  handleAlerts,
  handleExport,
  METRIC_DEFINITIONS,
  TIME_RANGES,
  SPARKLINE_CHARS,
  type MetricPoint,
  type MetricDefinition,
  type MetricCategory,
  type TimeRange,
  type MetricQueryResult,
  type TopEntry,
  type MetricAlert,
  type DashboardData,
  type ExportFormat,
  type AlertSeverity,
  type SortDirection,
};
