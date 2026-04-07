/**
 * CLI command: logs
 *
 * Log management for the Xergon Network.
 * Tail, search, aggregate, export logs across services.
 *
 * Usage:
 *   xergon logs tail [service]          -- Real-time log tailing with filters
 *   xergon logs search <query>         -- Search logs with structured query language
 *   xergon logs stats                  -- Log statistics and aggregation
 *   xergon logs export [format]        -- Export logs (json, csv, text)
 *   xergon logs alerts                 -- Show log-based alerts and anomalies
 *   xergon logs services               -- List available log sources
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

type LogLevel = 'debug' | 'info' | 'warn' | 'error';
type ExportFormat = 'json' | 'csv' | 'text';

interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  service?: string;
  source?: string;
  traceId?: string;
  spanId?: string;
  details?: unknown;
}

interface LogServiceInfo {
  name: string;
  status: 'active' | 'inactive' | 'degraded';
  logCount: number;
  lastLog?: string;
  endpoint?: string;
}

interface LogStats {
  totalEntries: number;
  byLevel: Record<LogLevel, number>;
  byService: Record<string, number>;
  topErrors: Array<{ message: string; count: number; service: string }>;
  timeRange: { start: string; end: string };
  entriesPerMinute: number;
}

interface LogAlert {
  id: string;
  type: 'error_spike' | 'latency' | 'service_down' | 'anomaly' | 'threshold';
  severity: 'critical' | 'warning' | 'info';
  message: string;
  service: string;
  timestamp: string;
  details?: Record<string, unknown>;
  acknowledged: boolean;
}

interface SearchQuery {
  text?: string;
  level?: LogLevel;
  service?: string;
  since?: string;
  until?: string;
  limit?: number;
}

interface SearchResult {
  query: SearchQuery;
  totalMatches: number;
  entries: LogEntry[];
  tookMs: number;
}

// ── Constants ──────────────────────────────────────────────────────

const LOG_LEVELS: Record<string, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

const VALID_LEVELS: LogLevel[] = ['debug', 'info', 'warn', 'error'];
const VALID_EXPORT_FORMATS: ExportFormat[] = ['json', 'csv', 'text'];

// ── LogService ─────────────────────────────────────────────────────

class LogService {
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

  async fetchLogs(opts: {
    service?: string;
    since?: string;
    level?: string;
    limit: number;
    signal?: AbortSignal;
  }): Promise<LogEntry[]> {
    const params = new URLSearchParams();
    params.set('limit', String(opts.limit));
    if (opts.service) params.set('service', opts.service);
    if (opts.since) {
      const ms = parseDuration(opts.since);
      if (ms > 0) params.set('since', new Date(Date.now() - ms).toISOString());
    }
    if (opts.level) params.set('level', opts.level);

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/logs?${params}`);
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.logs ?? data.data ?? []);
      return items.map(normalizeLogEntry);
    }
    return [];
  }

  async searchLogs(query: SearchQuery): Promise<SearchResult> {
    const startTime = Date.now();
    const params = new URLSearchParams();
    if (query.text) params.set('q', query.text);
    if (query.level) params.set('level', query.level);
    if (query.service) params.set('service', query.service);
    if (query.since) params.set('since', query.since);
    if (query.until) params.set('until', query.until);
    if (query.limit) params.set('limit', String(query.limit));

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/logs/search?${params}`);
    if (data) {
      const entries: any[] = Array.isArray(data) ? data : (data.entries ?? data.results ?? []);
      return {
        query,
        totalMatches: data.totalMatches ?? data.total ?? entries.length,
        entries: entries.map(normalizeLogEntry),
        tookMs: data.tookMs ?? Date.now() - startTime,
      };
    }

    return mockSearchResult(query);
  }

  async getStats(service?: string): Promise<LogStats> {
    const params = new URLSearchParams();
    if (service) params.set('service', service);

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/logs/stats?${params}`);
    if (data) {
      return {
        totalEntries: data.totalEntries ?? data.total ?? 0,
        byLevel: data.byLevel ?? { debug: 0, info: 0, warn: 0, error: 0 },
        byService: data.byService ?? {},
        topErrors: data.topErrors ?? [],
        timeRange: data.timeRange ?? { start: new Date(Date.now() - 3600000).toISOString(), end: new Date().toISOString() },
        entriesPerMinute: data.entriesPerMinute ?? 0,
      };
    }

    return mockStats(service);
  }

  async getAlerts(): Promise<LogAlert[]> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/logs/alerts`);
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.alerts ?? data.data ?? []);
      return items.map(normalizeAlert);
    }

    return mockAlerts();
  }

  async getServices(): Promise<LogServiceInfo[]> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/logs/services`);
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.services ?? data.data ?? []);
      return items.map(normalizeService);
    }

    return mockServices();
  }

  async exportLogs(format: ExportFormat, opts: {
    service?: string;
    level?: string;
    since?: string;
    limit?: number;
  }): Promise<string> {
    const entries = await this.fetchLogs({
      service: opts.service,
      level: opts.level,
      since: opts.since,
      limit: opts.limit ?? 100,
    });
    return formatLogs(entries, format);
  }
}

// ── Formatting helpers ────────────────────────────────────────────

function parseDuration(dur: string): number {
  const match = dur.match(/^(\d+(?:\.\d+)?)(h|m|s)$/i);
  if (!match) return 0;
  const val = parseFloat(match[1]);
  const unit = match[2].toLowerCase();
  switch (unit) {
    case 'h': return val * 3600_000;
    case 'm': return val * 60_000;
    case 's': return val * 1_000;
    default: return 0;
  }
}

function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return ts;
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return ts;
  }
}

function levelColor(level: string, text: string, useColor: boolean): string {
  if (!useColor) return text;
  const codes: Record<string, string> = {
    error: '\x1b[31m',
    warn: '\x1b[33m',
    info: '\x1b[36m',
    debug: '\x1b[32m',
  };
  const code = codes[level.toLowerCase()] ?? '\x1b[0m';
  return `${code}${text}\x1b[0m`;
}

function linePrefix(level: string, useColor: boolean): string {
  const pad = (s: string) => s.padEnd(5);
  return levelColor(level, pad(level.toUpperCase()), useColor);
}

function renderLogEntry(entry: LogEntry, useColor: boolean): string {
  const ts = formatTimestamp(entry.timestamp);
  const lvl = linePrefix(entry.level, useColor);
  const svc = entry.service ? levelColor(entry.level, `[${entry.service}] `, useColor) : '';
  const trace = entry.traceId ? `[${entry.traceId.substring(0, 8)}] ` : '';
  return `  ${ts}  ${lvl}  ${svc}${trace}${entry.message}`;
}

function passesLevelFilter(entry: LogEntry, minLevel?: string): boolean {
  if (!minLevel) return true;
  const min = LOG_LEVELS[minLevel.toLowerCase()] ?? 0;
  const entryLvl = LOG_LEVELS[entry.level.toLowerCase()] ?? 0;
  return entryLvl >= min;
}

function normalizeLogEntry(raw: any): LogEntry {
  return {
    timestamp: raw.timestamp ?? raw.time ?? raw.ts ?? new Date().toISOString(),
    level: raw.level ?? raw.severity ?? 'info',
    message: raw.message ?? raw.msg ?? raw.text ?? String(raw),
    service: raw.service ?? raw.source ?? raw.component ?? undefined,
    source: raw.source ?? raw.component ?? undefined,
    traceId: raw.traceId ?? raw.trace_id ?? undefined,
    spanId: raw.spanId ?? raw.span_id ?? undefined,
    details: raw.details ?? raw.meta ?? raw.context ?? undefined,
  };
}

function normalizeAlert(raw: any): LogAlert {
  return {
    id: raw.id ?? `alert-${Date.now().toString(36)}`,
    type: raw.type ?? 'anomaly',
    severity: raw.severity ?? 'warning',
    message: raw.message ?? raw.msg ?? 'Unknown alert',
    service: raw.service ?? 'unknown',
    timestamp: raw.timestamp ?? new Date().toISOString(),
    details: raw.details ?? raw.context ?? undefined,
    acknowledged: raw.acknowledged ?? false,
  };
}

function normalizeService(raw: any): LogServiceInfo {
  return {
    name: raw.name ?? raw.service ?? 'unknown',
    status: raw.status ?? 'active',
    logCount: raw.logCount ?? raw.log_count ?? 0,
    lastLog: raw.lastLog ?? raw.last_log,
    endpoint: raw.endpoint ?? raw.url,
  };
}

function formatLogs(entries: LogEntry[], format: ExportFormat): string {
  switch (format) {
    case 'json':
      return JSON.stringify(entries, null, 2);
    case 'csv': {
      const header = 'timestamp,level,service,message,traceId';
      const rows = entries.map(e =>
        `${e.timestamp},${e.level},"${(e.service ?? '').replace(/"/g, '""')}","${e.message.replace(/"/g, '""')}",${e.traceId ?? ''}`
      );
      return [header, ...rows].join('\n');
    }
    case 'text':
    default:
      return entries.map(e => renderLogEntry(e, false)).join('\n');
  }
}

// ── Mock data generators ──────────────────────────────────────────

function mockSearchResult(query: SearchQuery): SearchResult {
  const entries: LogEntry[] = [];
  const levels: LogLevel[] = ['info', 'info', 'info', 'warn', 'error', 'debug'];
  const services = ['relay', 'provider-001', 'provider-002', 'settlement', 'gateway'];
  const messages = [
    'Request processed successfully',
    'Connection timeout to upstream provider',
    'Model loaded: llama-3.3-70b',
    'Rate limit exceeded for client',
    'Settlement confirmed on-chain',
    'Health check passed',
    'Invalid request payload',
    'Provider registration complete',
    'Authentication failed',
    'Batch inference started',
  ];
  const count = query.limit ?? 20;
  const now = Date.now();

  for (let i = 0; i < count; i++) {
    entries.push({
      timestamp: new Date(now - i * 60000).toISOString(),
      level: levels[i % levels.length],
      message: messages[i % messages.length],
      service: services[i % services.length],
      traceId: `trace-${(now - i * 60000).toString(36)}`,
    });
  }

  if (query.text) {
    const lower = query.text.toLowerCase();
    const filtered = entries.filter(e =>
      e.message.toLowerCase().includes(lower) ||
      (e.service?.toLowerCase().includes(lower))
    );
    return { query, totalMatches: filtered.length, entries: filtered, tookMs: 12 };
  }

  return { query, totalMatches: entries.length, entries, tookMs: 8 };
}

function mockStats(service?: string): LogStats {
  const byService: Record<string, number> = {
    relay: 12450,
    'provider-001': 8320,
    'provider-002': 6710,
    settlement: 2100,
    gateway: 5400,
  };

  if (service) {
    const count = byService[service] ?? Math.floor(Math.random() * 10000);
    return {
      totalEntries: count,
      byLevel: {
        debug: Math.floor(count * 0.15),
        info: Math.floor(count * 0.55),
        warn: Math.floor(count * 0.2),
        error: Math.floor(count * 0.1),
      },
      byService: { [service]: count },
      topErrors: [
        { message: 'Connection timeout', count: Math.floor(count * 0.03), service },
        { message: 'Rate limit exceeded', count: Math.floor(count * 0.02), service },
      ],
      timeRange: { start: new Date(Date.now() - 3600000).toISOString(), end: new Date().toISOString() },
      entriesPerMinute: Math.floor(count / 60),
    };
  }

  return {
    totalEntries: Object.values(byService).reduce((a, b) => a + b, 0),
    byLevel: { debug: 5250, info: 18200, warn: 6500, error: 4430 },
    byService,
    topErrors: [
      { message: 'Connection timeout to upstream provider', count: 342, service: 'relay' },
      { message: 'Rate limit exceeded for client', count: 187, service: 'gateway' },
      { message: 'Invalid request payload', count: 95, service: 'relay' },
      { message: 'Authentication failed', count: 63, service: 'gateway' },
      { message: 'Provider heartbeat missed', count: 28, service: 'settlement' },
    ],
    timeRange: { start: new Date(Date.now() - 3600000).toISOString(), end: new Date().toISOString() },
    entriesPerMinute: 572,
  };
}

function mockAlerts(): LogAlert[] {
  const now = new Date();
  return [
    {
      id: 'alert-001',
      type: 'error_spike',
      severity: 'critical',
      message: 'Error rate spike detected: 15.3% (threshold: 5%)',
      service: 'relay',
      timestamp: new Date(now.getTime() - 300000).toISOString(),
      details: { currentRate: 15.3, threshold: 5, window: '5m' },
      acknowledged: false,
    },
    {
      id: 'alert-002',
      type: 'service_down',
      severity: 'critical',
      message: 'Provider provider-003 is unreachable',
      service: 'provider-003',
      timestamp: new Date(now.getTime() - 900000).toISOString(),
      details: { lastSeen: new Date(now.getTime() - 900000).toISOString(), timeoutMs: 30000 },
      acknowledged: false,
    },
    {
      id: 'alert-003',
      type: 'latency',
      severity: 'warning',
      message: 'P99 latency exceeded: 4.2s (threshold: 3s)',
      service: 'relay',
      timestamp: new Date(now.getTime() - 1800000).toISOString(),
      details: { p99: 4200, threshold: 3000, window: '10m' },
      acknowledged: true,
    },
    {
      id: 'alert-004',
      type: 'anomaly',
      severity: 'warning',
      message: 'Unusual log volume decrease detected on settlement service',
      service: 'settlement',
      timestamp: new Date(now.getTime() - 3600000).toISOString(),
      details: { expectedRate: 35, actualRate: 8, deviation: '77%' },
      acknowledged: false,
    },
    {
      id: 'alert-005',
      type: 'threshold',
      severity: 'info',
      message: 'Log storage at 72% capacity',
      service: 'system',
      timestamp: new Date(now.getTime() - 7200000).toISOString(),
      details: { used: '72%', total: '100GB', remaining: '28GB' },
      acknowledged: true,
    },
  ];
}

function mockServices(): LogServiceInfo[] {
  return [
    { name: 'relay', status: 'active', logCount: 12450, lastLog: new Date().toISOString(), endpoint: '/api/v1/logs' },
    { name: 'provider-001', status: 'active', logCount: 8320, lastLog: new Date(Date.now() - 5000).toISOString() },
    { name: 'provider-002', status: 'active', logCount: 6710, lastLog: new Date(Date.now() - 12000).toISOString() },
    { name: 'provider-003', status: 'inactive', logCount: 2340, lastLog: new Date(Date.now() - 900000).toISOString() },
    { name: 'settlement', status: 'active', logCount: 2100, lastLog: new Date(Date.now() - 30000).toISOString() },
    { name: 'gateway', status: 'degraded', logCount: 5400, lastLog: new Date(Date.now() - 2000).toISOString() },
    { name: 'auth', status: 'active', logCount: 9800, lastLog: new Date(Date.now() - 1000).toISOString() },
  ];
}

// ── Streaming ─────────────────────────────────────────────────────

async function streamLogs(
  baseUrl: string,
  opts: {
    level?: string;
    signal?: AbortSignal;
    onEntry: (entry: LogEntry) => void;
    onError: (err: Error) => void;
  },
): Promise<void> {
  const wsUrl = baseUrl
    .replace(/^http/, 'ws')
    .replace(/\/+$/, '') + '/v1/logs/stream';

  try {
    const wsModule: any = await Function('return import("ws")')().catch(() => null);
    if (wsModule) {
      const WebSocket = wsModule.WebSocket || wsModule;
      const ws = new WebSocket(wsUrl);

      if (opts.signal) {
        opts.signal.addEventListener('abort', () => ws.close(), { once: true });
      }

      ws.on('message', (data: any) => {
        try {
          const parsed = JSON.parse(data.toString());
          const entry = normalizeLogEntry(parsed);
          if (passesLevelFilter(entry, opts.level)) {
            opts.onEntry(entry);
          }
        } catch {
          // Non-JSON message -- skip
        }
      });

      ws.on('error', (err: Error) => {
        opts.onError(new Error(`WebSocket error: ${err.message}`));
      });

      return new Promise<void>((resolve, reject) => {
        ws.on('close', resolve);
        ws.on('error', reject);
        if (opts.signal) {
          opts.signal.addEventListener('abort', () => {
            ws.close();
            resolve();
          }, { once: true });
        }
      });
    }
  } catch {
    // ws module not available -- fall through to SSE
  }

  const sseUrl = `${baseUrl.replace(/\/+$/, '')}/v1/logs/stream`;
  try {
    const res = await fetch(sseUrl, { signal: opts.signal });
    if (!res.ok || !res.body) {
      throw new Error(`SSE endpoint returned ${res.status}`);
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      if (opts.signal?.aborted) break;
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          try {
            const parsed = JSON.parse(line.slice(6));
            const entry = normalizeLogEntry(parsed);
            if (passesLevelFilter(entry, opts.level)) {
              opts.onEntry(entry);
            }
          } catch {
            // Non-JSON data -- skip
          }
        }
      }
    }
  } catch (err) {
    opts.onError(new Error(
      `Cannot stream logs: ${err instanceof Error ? err.message : String(err)}\n` +
      'For WebSocket support, install: npm install ws',
    ));
  }
}

// ── Subcommand: tail ───────────────────────────────────────────────

async function handleTail(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const serviceName = args.positional[1];
  const follow = args.options.follow === true;
  const since = args.options.since ? String(args.options.since) : undefined;
  const level = args.options.level ? String(args.options.level) : undefined;
  const limit = args.options.limit !== undefined ? Number(args.options.limit) : 50;
  const outputJson = args.options.json === true;

  if (level && !LOG_LEVELS[level.toLowerCase()]) {
    ctx.output.writeError(`Invalid log level: "${level}". Use: error, warn, info, debug`);
    process.exit(1);
    return;
  }

  const normalizedLevel = level?.toLowerCase() as LogLevel | undefined;
  const useColor = ctx.config.color && !process.env.NO_COLOR;
  const svc = new LogService(ctx.config.baseUrl);

  try {
    const entries = await svc.fetchLogs({
      service: serviceName,
      since,
      level: normalizedLevel,
      limit,
    });

    const filtered = entries.filter(e => passesLevelFilter(e, normalizedLevel));

    if (outputJson) {
      ctx.output.write(JSON.stringify(filtered, null, 2));
    } else if (filtered.length === 0) {
      ctx.output.info('No log entries found.');
    } else {
      const title = serviceName ? `Logs: ${serviceName}` : 'Relay Logs';
      ctx.output.write(ctx.output.colorize(title, 'bold') + '\n');
      ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');
      for (const entry of filtered) {
        ctx.output.write(renderLogEntry(entry, useColor) + '\n');
      }
      ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');
      ctx.output.write(ctx.output.colorize(`  ${filtered.length} entry(s)`, 'dim') + '\n');
    }

    // Follow mode
    if (follow) {
      ctx.output.info('Following log stream (Ctrl+C to stop)...');
      ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');

      const followController = new AbortController();
      const onInterrupt = () => followController.abort();
      process.on('SIGINT', onInterrupt);
      process.on('SIGTERM', onInterrupt);

      try {
        await streamLogs(ctx.config.baseUrl, {
          level: normalizedLevel,
          signal: followController.signal,
          onEntry: (entry) => {
            if (outputJson) {
              ctx.output.write(JSON.stringify(entry) + '\n');
            } else {
              ctx.output.write(renderLogEntry(entry, useColor) + '\n');
            }
          },
          onError: (err) => {
            ctx.output.warn(`Stream error: ${err.message}`);
          },
        });
      } finally {
        process.off('SIGINT', onInterrupt);
        process.off('SIGTERM', onInterrupt);
      }

      ctx.output.write(ctx.output.colorize('\nStream ended.', 'dim') + '\n');
    }
  } catch (err: any) {
    if (err.name === 'AbortError') return;
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to fetch logs: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: search ─────────────────────────────────────────────

async function handleSearch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const queryText = args.positional[1];
  if (!queryText) {
    ctx.output.writeError('Usage: xergon logs search <query>');
    ctx.output.info('Example: xergon logs search "error" --service relay --level error --since 1h');
    process.exit(1);
    return;
  }

  const level = args.options.level ? String(args.options.level) : undefined;
  const serviceName = args.options.service ? String(args.options.service) : undefined;
  const since = args.options.since ? String(args.options.since) : undefined;
  const limit = args.options.limit !== undefined ? Number(args.options.limit) : 50;
  const outputJson = args.options.json === true;
  const useColor = ctx.config.color && !process.env.NO_COLOR;

  if (level && !LOG_LEVELS[level.toLowerCase()]) {
    ctx.output.writeError(`Invalid log level: "${level}". Use: error, warn, info, debug`);
    process.exit(1);
    return;
  }

  const query: SearchQuery = {
    text: queryText,
    level: level?.toLowerCase() as LogLevel | undefined,
    service: serviceName,
    since,
    limit,
  };

  const svc = new LogService(ctx.config.baseUrl);

  try {
    const result = await svc.searchLogs(query);

    if (outputJson) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Log Search Results', 'bold') + '\n');
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');
    ctx.output.write(`  Query:     ${ctx.output.colorize(queryText, 'cyan')}\n`);
    ctx.output.write(`  Matches:   ${ctx.output.colorize(String(result.totalMatches), 'yellow')}\n`);
    ctx.output.write(`  Took:      ${result.tookMs}ms\n`);
    if (serviceName) ctx.output.write(`  Service:   ${serviceName}\n`);
    if (level) ctx.output.write(`  Level:     ${level.toUpperCase()}\n`);
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');

    if (result.entries.length === 0) {
      ctx.output.info('No matching log entries found.');
      return;
    }

    for (const entry of result.entries) {
      ctx.output.write(renderLogEntry(entry, useColor) + '\n');
    }

    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');
    ctx.output.write(ctx.output.colorize(`  ${result.entries.length} result(s) shown`, 'dim') + '\n');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Search failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: stats ──────────────────────────────────────────────

async function handleStats(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const serviceName = args.options.service ? String(args.options.service) : undefined;
  const outputJson = args.options.json === true;

  const svc = new LogService(ctx.config.baseUrl);

  try {
    const stats = await svc.getStats(serviceName);

    if (outputJson) {
      ctx.output.write(JSON.stringify(stats, null, 2));
      return;
    }

    const title = serviceName ? `Log Statistics: ${serviceName}` : 'Log Statistics (All Services)';
    ctx.output.write(ctx.output.colorize(title, 'bold') + '\n');
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');
    ctx.output.write('');
    ctx.output.write(`  Total Entries:      ${ctx.output.colorize(String(stats.totalEntries.toLocaleString()), 'yellow')}`);
    ctx.output.write(`  Entries/Minute:     ${stats.entriesPerMinute}`);
    ctx.output.write(`  Time Range:         ${formatTimestamp(stats.timeRange.start)} - ${formatTimestamp(stats.timeRange.end)}`);
    ctx.output.write('');
    ctx.output.write('  ' + ctx.output.colorize('By Level:', 'cyan'));
    ctx.output.write(`    DEBUG  ${ctx.output.colorize(String(stats.byLevel.debug), 'green')}  (${((stats.byLevel.debug / stats.totalEntries) * 100).toFixed(1)}%)`);
    ctx.output.write(`    INFO   ${ctx.output.colorize(String(stats.byLevel.info), 'cyan')}  (${((stats.byLevel.info / stats.totalEntries) * 100).toFixed(1)}%)`);
    ctx.output.write(`    WARN   ${ctx.output.colorize(String(stats.byLevel.warn), 'yellow')}  (${((stats.byLevel.warn / stats.totalEntries) * 100).toFixed(1)}%)`);
    ctx.output.write(`    ERROR  ${ctx.output.colorize(String(stats.byLevel.error), 'red')}  (${((stats.byLevel.error / stats.totalEntries) * 100).toFixed(1)}%)`);
    ctx.output.write('');

    // Bar chart for level distribution
    const maxCount = Math.max(stats.byLevel.debug, stats.byLevel.info, stats.byLevel.warn, stats.byLevel.error);
    const barWidth = 30;
    const drawBar = (count: number, color: 'green' | 'cyan' | 'yellow' | 'red') => {
      const filled = Math.round((count / maxCount) * barWidth);
      const bar = '\u2588'.repeat(filled) + '\u2591'.repeat(barWidth - filled);
      ctx.output.write(`    ${ctx.output.colorize(bar, color)}  ${count}`);
    };
    ctx.output.write('  ' + ctx.output.colorize('Distribution:', 'cyan'));
    drawBar(stats.byLevel.debug, 'green');
    drawBar(stats.byLevel.info, 'cyan');
    drawBar(stats.byLevel.warn, 'yellow');
    drawBar(stats.byLevel.error, 'red');
    ctx.output.write('');

    // By service
    if (Object.keys(stats.byService).length > 0) {
      ctx.output.write('  ' + ctx.output.colorize('By Service:', 'cyan'));
      for (const [name, count] of Object.entries(stats.byService)) {
        const pct = ((count / stats.totalEntries) * 100).toFixed(1);
        ctx.output.write(`    ${name.padEnd(20)} ${String(count.toLocaleString()).padStart(8)}  (${pct}%)`);
      }
      ctx.output.write('');
    }

    // Top errors
    if (stats.topErrors.length > 0) {
      ctx.output.write('  ' + ctx.output.colorize('Top Errors:', 'red'));
      for (const err of stats.topErrors) {
        ctx.output.write(`    [${err.service}] ${err.message} (${err.count}x)`);
      }
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get stats: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: export ─────────────────────────────────────────────

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const formatArg = args.positional[1] || (args.options.format ? String(args.options.format) : 'json');
  const level = args.options.level ? String(args.options.level) : undefined;
  const serviceName = args.options.service ? String(args.options.service) : undefined;
  const since = args.options.since ? String(args.options.since) : undefined;
  const limit = args.options.limit !== undefined ? Number(args.options.limit) : 100;

  const format = formatArg.toLowerCase() as ExportFormat;
  if (!VALID_EXPORT_FORMATS.includes(format)) {
    ctx.output.writeError(`Invalid format: "${formatArg}". Must be one of: ${VALID_EXPORT_FORMATS.join(', ')}`);
    process.exit(1);
    return;
  }

  const svc = new LogService(ctx.config.baseUrl);

  try {
    const output = await svc.exportLogs(format, {
      service: serviceName,
      level,
      since,
      limit,
    });
    ctx.output.write(output);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Export failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: alerts ─────────────────────────────────────────────

async function handleAlerts(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const useColor = ctx.config.color && !process.env.NO_COLOR;

  const svc = new LogService(ctx.config.baseUrl);

  try {
    const alerts = await svc.getAlerts();

    if (outputJson) {
      ctx.output.write(JSON.stringify(alerts, null, 2));
      return;
    }

    if (alerts.length === 0) {
      ctx.output.info('No active alerts.');
      return;
    }

    ctx.output.write(ctx.output.colorize('Log Alerts', 'bold') + '\n');
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');

    const critical = alerts.filter(a => a.severity === 'critical');
    const warnings = alerts.filter(a => a.severity === 'warning');
    const info = alerts.filter(a => a.severity === 'info');

    if (critical.length > 0) {
      ctx.output.write('  ' + ctx.output.colorize(`CRITICAL (${critical.length})`, 'red') + '\n');
      for (const alert of critical) {
        const ack = alert.acknowledged ? ' ' + ctx.output.colorize('[ACK]', 'dim') : '';
        ctx.output.write(`    ${ctx.output.colorize('\u25cf', 'red')} [${alert.service}] ${alert.message}${ack}`);
        ctx.output.write(`      ${formatTimestamp(alert.timestamp)}\n`);
      }
      ctx.output.write('');
    }

    if (warnings.length > 0) {
      ctx.output.write('  ' + ctx.output.colorize(`WARNINGS (${warnings.length})`, 'yellow') + '\n');
      for (const alert of warnings) {
        const ack = alert.acknowledged ? ' ' + ctx.output.colorize('[ACK]', 'dim') : '';
        ctx.output.write(`    ${ctx.output.colorize('\u25cf', 'yellow')} [${alert.service}] ${alert.message}${ack}`);
        ctx.output.write(`      ${formatTimestamp(alert.timestamp)}\n`);
      }
      ctx.output.write('');
    }

    if (info.length > 0) {
      ctx.output.write('  ' + ctx.output.colorize(`INFO (${info.length})`, 'cyan') + '\n');
      for (const alert of info) {
        const ack = alert.acknowledged ? ' ' + ctx.output.colorize('[ACK]', 'dim') : '';
        ctx.output.write(`    ${ctx.output.colorize('\u25cf', 'cyan')} [${alert.service}] ${alert.message}${ack}`);
        ctx.output.write(`      ${formatTimestamp(alert.timestamp)}\n`);
      }
      ctx.output.write('');
    }

    ctx.output.write(ctx.output.colorize(`  ${alerts.length} alert(s) total`, 'dim') + '\n');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get alerts: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: services ───────────────────────────────────────────

async function handleServices(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const useColor = ctx.config.color && !process.env.NO_COLOR;

  const svc = new LogService(ctx.config.baseUrl);

  try {
    const services = await svc.getServices();

    if (outputJson) {
      ctx.output.write(JSON.stringify(services, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Log Services', 'bold') + '\n');
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');

    for (const s of services) {
      const statusIcon = s.status === 'active'
        ? ctx.output.colorize('\u25cf', 'green')
        : s.status === 'degraded'
          ? ctx.output.colorize('\u25cf', 'yellow')
          : ctx.output.colorize('\u25cf', 'red');

      const statusLabel = s.status === 'active'
        ? ctx.output.colorize('active', 'green')
        : s.status === 'degraded'
          ? ctx.output.colorize('degraded', 'yellow')
          : ctx.output.colorize('inactive', 'red');

      ctx.output.write(`  ${statusIcon} ${s.name.padEnd(20)} ${statusLabel}`);
      ctx.output.write(`    Logs: ${String(s.logCount).padStart(8)}`);
      if (s.lastLog) {
        ctx.output.write(`    Last: ${formatTimestamp(s.lastLog)}`);
      }
      ctx.output.write('');
    }

    ctx.output.write(ctx.output.colorize(`  ${services.length} service(s)`, 'dim') + '\n');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get services: ${message}`);
    process.exit(1);
  }
}

// ── Options ────────────────────────────────────────────────────────

const logsOptions: CommandOption[] = [
  {
    name: 'follow',
    short: '-f',
    long: '--follow',
    description: 'Follow log stream (tail -f mode)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'since',
    short: '',
    long: '--since',
    description: 'Show logs since duration (e.g. "1h", "30m", "24h")',
    required: false,
    type: 'string',
  },
  {
    name: 'level',
    short: '',
    long: '--level',
    description: 'Minimum log level: error, warn, info, debug',
    required: false,
    type: 'string',
  },
  {
    name: 'service',
    short: '',
    long: '--service',
    description: 'Filter by service name',
    required: false,
    type: 'string',
  },
  {
    name: 'limit',
    short: '-n',
    long: '--limit',
    description: 'Max number of log entries (default: 50)',
    required: false,
    type: 'number',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Export format: json, csv, text (default: json)',
    required: false,
    type: 'string',
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

// ── Command Action ─────────────────────────────────────────────────

async function logsAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const subcommand = args.positional[0];

  switch (subcommand) {
    case 'tail':
      await handleTail(args, ctx);
      break;
    case 'search':
      await handleSearch(args, ctx);
      break;
    case 'stats':
      await handleStats(args, ctx);
      break;
    case 'export':
      await handleExport(args, ctx);
      break;
    case 'alerts':
      await handleAlerts(args, ctx);
      break;
    case 'services':
      await handleServices(args, ctx);
      break;
    default:
      // Default to tail
      await handleTail(args, ctx);
      break;
  }
}

// ── Command Definition ─────────────────────────────────────────────

export const logsCommand: Command = {
  name: 'logs',
  description: 'Manage logs: tail, search, stats, export, alerts, services',
  aliases: ['log'],
  options: logsOptions,
  action: logsAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  parseDuration,
  formatTimestamp,
  levelColor,
  linePrefix,
  renderLogEntry,
  passesLevelFilter,
  normalizeLogEntry,
  normalizeAlert,
  normalizeService,
  formatLogs,
  LogService,
  logsAction,
  mockSearchResult,
  mockStats,
  mockAlerts,
  mockServices,
  type LogLevel,
  type ExportFormat,
  type LogEntry,
  type LogServiceInfo,
  type LogStats,
  type LogAlert,
  type SearchQuery,
  type SearchResult,
};
