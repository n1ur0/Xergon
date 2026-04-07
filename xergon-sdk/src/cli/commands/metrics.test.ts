/**
 * Tests for CLI command: metrics
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
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
  metricsAction,
  metricsCommand,
  METRIC_DEFINITIONS,
  TIME_RANGES,
  SPARKLINE_CHARS,
  MetricsService,
  type MetricPoint,
  type MetricQueryResult,
  type TopEntry,
  type MetricAlert,
} from './metrics';

// ── Mock output formatter ──────────────────────────────────────────

function createMockOutput() {
  return {
    colorize: (text: string, _style: string) => text,
    write: vi.fn(),
    writeError: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
    warn: vi.fn(),
    formatTable: (data: any[]) => JSON.stringify(data),
    formatOutput: (data: any) => JSON.stringify(data, null, 2),
    formatText: (data: any, title?: string) => {
      let result = title ? `${title}\n` : '';
      if (typeof data === 'object' && data !== null) {
        for (const [k, v] of Object.entries(data as Record<string, any>)) {
          result += `  ${k}: ${v}\n`;
        }
      }
      return result;
    },
  };
}

function createMockContext(overrides?: Record<string, any>) {
  return {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      apiKey: '',
      defaultModel: 'llama-3.3-70b',
      outputFormat: 'text' as const,
      color: false,
      timeout: 30000,
    },
    output: createMockOutput(),
    ...overrides,
  };
}

// ── generateSparkline tests ────────────────────────────────────────

describe('generateSparkline', () => {
  it('returns dots for empty values', () => {
    const result = generateSparkline([], 10);
    expect(result).toBe('·'.repeat(10));
  });
  it('returns mid char for single value', () => {
    const result = generateSparkline([42], 10);
    expect(result).toBe(SPARKLINE_CHARS[4].repeat(10));
  });
  it('generates sparkline for ascending values', () => {
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    const result = generateSparkline(values, 8);
    expect(result.length).toBe(8);
    // Should end with a high bar
    expect(result[result.length - 1]).toBe('█');
  });
  it('generates sparkline for descending values', () => {
    const values = [8, 7, 6, 5, 4, 3, 2, 1];
    const result = generateSparkline(values, 8);
    expect(result.length).toBe(8);
    expect(result[0]).toBe('█');
  });
  it('handles constant values', () => {
    const values = [5, 5, 5, 5, 5];
    const result = generateSparkline(values, 5);
    expect(result).toBe(SPARKLINE_CHARS[4].repeat(5));
  });
  it('uses default width of 20', () => {
    const values = [1, 2, 3, 4, 5];
    const result = generateSparkline(values);
    expect(result.length).toBe(20);
  });
  it('respects custom width', () => {
    const values = [1, 2, 3, 4, 5];
    const result = generateSparkline(values, 40);
    expect(result.length).toBe(40);
  });
  it('uses all sparkline chars for wide range', () => {
    const values = [0, 1, 2, 3, 4, 5, 6, 7];
    const result = generateSparkline(values, 8);
    const uniqueChars = new Set(result.split(''));
    expect(uniqueChars.size).toBeGreaterThan(1);
  });
});

// ── evaluateStatus tests ───────────────────────────────────────────

describe('evaluateStatus', () => {
  const aboveThresholds = { warning: 5, critical: 15, direction: 'above' as const };
  const belowThresholds = { warning: 99, critical: 95, direction: 'below' as const };

  it('returns healthy for below-warning (above)', () => {
    expect(evaluateStatus(3, aboveThresholds)).toBe('healthy');
  });
  it('returns warning for at-warning (above)', () => {
    expect(evaluateStatus(5, aboveThresholds)).toBe('warning');
  });
  it('returns critical for at-critical (above)', () => {
    expect(evaluateStatus(15, aboveThresholds)).toBe('critical');
  });
  it('returns critical for above-critical (above)', () => {
    expect(evaluateStatus(20, aboveThresholds)).toBe('critical');
  });
  it('returns healthy for above-warning (below)', () => {
    expect(evaluateStatus(99.5, belowThresholds)).toBe('healthy');
  });
  it('returns warning for at-warning (below)', () => {
    expect(evaluateStatus(99, belowThresholds)).toBe('warning');
  });
  it('returns critical for at-critical (below)', () => {
    expect(evaluateStatus(95, belowThresholds)).toBe('critical');
  });
  it('returns critical for below-critical (below)', () => {
    expect(evaluateStatus(90, belowThresholds)).toBe('critical');
  });
});

// ── statusColor tests ──────────────────────────────────────────────

describe('statusColor', () => {
  it('returns plain text when color disabled', () => {
    expect(statusColor('healthy', 'HEALTHY', false)).toBe('HEALTHY');
  });
  it('includes green ANSI for healthy', () => {
    const result = statusColor('healthy', 'HEALTHY', true);
    expect(result).toContain('\x1b[32m');
  });
  it('includes yellow ANSI for warning', () => {
    const result = statusColor('warning', 'WARNING', true);
    expect(result).toContain('\x1b[33m');
  });
  it('includes red ANSI for critical', () => {
    const result = statusColor('critical', 'CRITICAL', true);
    expect(result).toContain('\x1b[31m');
  });
});

// ── severityColor tests ────────────────────────────────────────────

describe('severityColor', () => {
  it('returns plain text when color disabled', () => {
    expect(severityColor('critical', 'CRIT', false)).toBe('CRIT');
  });
  it('uses red for critical', () => {
    expect(severityColor('critical', 'X', true)).toContain('\x1b[31m');
  });
  it('uses yellow for warning', () => {
    expect(severityColor('warning', 'X', true)).toContain('\x1b[33m');
  });
  it('uses cyan for info', () => {
    expect(severityColor('info', 'X', true)).toContain('\x1b[36m');
  });
});

// ── formatNumber tests ─────────────────────────────────────────────

describe('formatNumber', () => {
  it('formats millions', () => {
    expect(formatNumber(2500000)).toBe('2.5M');
  });
  it('formats thousands', () => {
    expect(formatNumber(15420)).toBe('15.4K');
  });
  it('formats small numbers', () => {
    expect(formatNumber(42)).toBe('42.00');
  });
  it('formats with custom decimals', () => {
    expect(formatNumber(42, 1)).toBe('42.0');
  });
  it('formats zero', () => {
    expect(formatNumber(0)).toBe('0.00');
  });
});

// ── parseTimeRange tests ───────────────────────────────────────────

describe('parseTimeRange', () => {
  it('parses 1h', () => {
    expect(parseTimeRange('1h')).toBe(3600_000);
  });
  it('parses 24h', () => {
    expect(parseTimeRange('24h')).toBe(86400_000);
  });
  it('parses 7d', () => {
    expect(parseTimeRange('7d')).toBe(7 * 86400_000);
  });
  it('parses 30d', () => {
    expect(parseTimeRange('30d')).toBe(30 * 86400_000);
  });
  it('returns default for invalid format', () => {
    expect(parseTimeRange('invalid')).toBe(3600_000);
  });
});

// ── normalizeMetricPoint tests ─────────────────────────────────────

describe('normalizeMetricPoint', () => {
  it('normalizes full point', () => {
    const point = normalizeMetricPoint({
      timestamp: '2026-01-01T00:00:00Z',
      value: 42.5,
      label: 'test',
    });
    expect(point.timestamp).toBe('2026-01-01T00:00:00Z');
    expect(point.value).toBe(42.5);
    expect(point.label).toBe('test');
  });
  it('handles alternative field names', () => {
    const point = normalizeMetricPoint({
      time: '2026-01-01',
      count: 10,
    });
    expect(point.timestamp).toBe('2026-01-01');
    expect(point.value).toBe(10);
  });
  it('defaults to 0 for missing value', () => {
    const point = normalizeMetricPoint({});
    expect(point.value).toBe(0);
  });
});

// ── normalizeMetricQuery tests ─────────────────────────────────────

describe('normalizeMetricQuery', () => {
  it('normalizes full query result', () => {
    const raw = {
      label: 'Test Metric',
      current: 42,
      unit: 'ms',
      category: 'latency',
      points: [{ timestamp: '2026-01-01', value: 42 }],
      change: 5.2,
    };
    const result = normalizeMetricQuery(raw, 'test_metric', '1h');
    expect(result.metric).toBe('test_metric');
    expect(result.current).toBe(42);
    expect(result.timeRange).toBe('1h');
    expect(result.points).toHaveLength(1);
  });
  it('falls back to definition for missing fields', () => {
    const raw = { current: 42 };
    const result = normalizeMetricQuery(raw, 'p50_latency', '1h');
    expect(result.label).toBe('P50 Latency');
    expect(result.unit).toBe('ms');
    expect(result.category).toBe('latency');
  });
  it('evaluates status correctly', () => {
    const raw = { current: 20, thresholds: { warning: 10, critical: 15, direction: 'above' } };
    const result = normalizeMetricQuery(raw, 'test', '1h');
    expect(result.status).toBe('critical');
  });
});

// ── normalizeTopEntry tests ────────────────────────────────────────

describe('normalizeTopEntry', () => {
  it('normalizes with name', () => {
    const entry = normalizeTopEntry({ name: 'test-provider', value: 100, change: 5.0 }, 0);
    expect(entry.name).toBe('test-provider');
    expect(entry.value).toBe(100);
    expect(entry.rank).toBe(1);
  });
  it('provides defaults', () => {
    const entry = normalizeTopEntry({});
    expect(entry.name).toBe('unknown');
    expect(entry.value).toBe(0);
    expect(entry.rank).toBe(1);
  });
});

// ── normalizeAlert tests ───────────────────────────────────────────

describe('normalizeAlert', () => {
  it('normalizes full alert', () => {
    const alert = normalizeAlert({
      id: 'a1',
      metric: 'p99_latency',
      label: 'P99 Latency',
      severity: 'critical',
      message: 'High latency',
      current: 18000,
      threshold: 15000,
      direction: 'above',
      timestamp: '2026-01-01',
      duration: '5m',
    });
    expect(alert.id).toBe('a1');
    expect(alert.severity).toBe('critical');
    expect(alert.current).toBe(18000);
  });
  it('provides defaults', () => {
    const alert = normalizeAlert({});
    expect(alert.id).toBeTruthy();
    expect(alert.severity).toBe('warning');
    expect(alert.acknowledged).toBe(false);
    expect(alert.duration).toBe('unknown');
  });
});

// ── mockMetricQuery tests ──────────────────────────────────────────

describe('mockMetricQuery', () => {
  it('returns result with correct metric name', () => {
    const result = mockMetricQuery('p50_latency', '1h');
    expect(result.metric).toBe('p50_latency');
    expect(result.label).toBe('P50 Latency');
  });
  it('has data points', () => {
    const result = mockMetricQuery('error_rate', '24h');
    expect(result.points.length).toBeGreaterThan(0);
  });
  it('has valid status', () => {
    const result = mockMetricQuery('availability', '1h');
    expect(['healthy', 'warning', 'critical']).toContain(result.status);
  });
  it('has change value', () => {
    const result = mockMetricQuery('requests_per_sec', '1h');
    expect(typeof result.change).toBe('number');
  });
  it('respects time range for point count', () => {
    const short = mockMetricQuery('p50_latency', '1h');
    const long = mockMetricQuery('p50_latency', '7d');
    expect(long.points.length).toBeGreaterThanOrEqual(short.points.length);
  });
});

// ── mockTopProviders tests ─────────────────────────────────────────

describe('mockTopProviders', () => {
  it('returns 5 providers', () => {
    const providers = mockTopProviders();
    expect(providers).toHaveLength(5);
  });
  it('has ranked entries', () => {
    const providers = mockTopProviders();
    expect(providers[0].rank).toBe(1);
    expect(providers[4].rank).toBe(5);
  });
  it('has change values', () => {
    const providers = mockTopProviders();
    for (const p of providers) {
      expect(typeof p.change).toBe('number');
    }
  });
});

// ── mockTopModels tests ────────────────────────────────────────────

describe('mockTopModels', () => {
  it('returns 5 models', () => {
    const models = mockTopModels();
    expect(models).toHaveLength(5);
  });
  it('has model names', () => {
    const models = mockTopModels();
    for (const m of models) {
      expect(m.name).toBeTruthy();
    }
  });
});

// ── mockAlerts tests ───────────────────────────────────────────────

describe('mockAlerts', () => {
  it('returns alerts array', () => {
    const alerts = mockAlerts();
    expect(alerts.length).toBeGreaterThan(0);
  });
  it('has critical alerts', () => {
    const alerts = mockAlerts();
    expect(alerts.some(a => a.severity === 'critical')).toBe(true);
  });
  it('has acknowledged alerts', () => {
    const alerts = mockAlerts();
    expect(alerts.some(a => a.acknowledged)).toBe(true);
  });
  it('has unacknowledged alerts', () => {
    const alerts = mockAlerts();
    expect(alerts.some(a => !a.acknowledged)).toBe(true);
  });
  it('all alerts have IDs', () => {
    const alerts = mockAlerts();
    for (const a of alerts) {
      expect(a.id).toBeTruthy();
    }
  });
});

// ── METRIC_DEFINITIONS tests ───────────────────────────────────────

describe('METRIC_DEFINITIONS', () => {
  it('has at least 6 definitions', () => {
    expect(METRIC_DEFINITIONS.length).toBeGreaterThanOrEqual(6);
  });
  it('includes latency metrics', () => {
    const names = METRIC_DEFINITIONS.map(d => d.name);
    expect(names).toContain('p50_latency');
    expect(names).toContain('p99_latency');
  });
  it('includes throughput metrics', () => {
    const names = METRIC_DEFINITIONS.map(d => d.name);
    expect(names).toContain('requests_per_sec');
  });
  it('includes error metrics', () => {
    const names = METRIC_DEFINITIONS.map(d => d.name);
    expect(names).toContain('error_rate');
  });
  it('all definitions have thresholds', () => {
    for (const def of METRIC_DEFINITIONS) {
      expect(def.thresholds).toBeDefined();
      expect(typeof def.thresholds.warning).toBe('number');
      expect(typeof def.thresholds.critical).toBe('number');
      expect(['above', 'below']).toContain(def.thresholds.direction);
    }
  });
  it('all definitions have required fields', () => {
    for (const def of METRIC_DEFINITIONS) {
      expect(def.name).toBeTruthy();
      expect(def.label).toBeTruthy();
      expect(def.category).toBeTruthy();
      expect(def.unit).toBeTruthy();
      expect(def.description).toBeTruthy();
    }
  });
});

// ── TIME_RANGES tests ──────────────────────────────────────────────

describe('TIME_RANGES', () => {
  it('includes standard ranges', () => {
    expect(TIME_RANGES).toContain('1h');
    expect(TIME_RANGES).toContain('6h');
    expect(TIME_RANGES).toContain('24h');
    expect(TIME_RANGES).toContain('7d');
    expect(TIME_RANGES).toContain('30d');
    expect(TIME_RANGES).toContain('90d');
  });
});

// ── MetricsService tests ───────────────────────────────────────────

describe('MetricsService', () => {
  it('constructs with base URL', () => {
    const svc = new MetricsService('https://example.com');
    expect(svc).toBeDefined();
  });
  it('queryMetric returns null for invalid endpoint', async () => {
    const svc = new MetricsService('https://nonexistent.invalid');
    const result = await svc.queryMetric('p50_latency', '1h');
    expect(result).toBeNull();
  });
  it('getTopResources returns null for invalid endpoint', async () => {
    const svc = new MetricsService('https://nonexistent.invalid');
    const result = await svc.getTopResources('providers', 'requests_per_sec', 10);
    expect(result).toBeNull();
  });
  it('getAlerts returns null for invalid endpoint', async () => {
    const svc = new MetricsService('https://nonexistent.invalid');
    const result = await svc.getAlerts();
    expect(result).toBeNull();
  });
  it('getHistory returns empty array for invalid endpoint', async () => {
    const svc = new MetricsService('https://nonexistent.invalid');
    const result = await svc.getHistory('p50_latency', '1h');
    expect(result).toEqual([]);
  });
});

// ── metricsCommand definition ──────────────────────────────────────

describe('metricsCommand', () => {
  it('has correct name', () => {
    expect(metricsCommand.name).toBe('metrics');
  });
  it('has description', () => {
    expect(metricsCommand.description).toContain('metrics');
  });
  it('has aliases', () => {
    expect(metricsCommand.aliases).toContain('stats');
    expect(metricsCommand.aliases).toContain('analytics');
  });
  it('has options', () => {
    expect(metricsCommand.options.length).toBeGreaterThan(0);
    expect(metricsCommand.options.some(o => o.name === 'json')).toBe(true);
    expect(metricsCommand.options.some(o => o.name === 'range')).toBe(true);
    expect(metricsCommand.options.some(o => o.name === 'type')).toBe(true);
    expect(metricsCommand.options.some(o => o.name === 'limit')).toBe(true);
    expect(metricsCommand.options.some(o => o.name === 'format')).toBe(true);
    expect(metricsCommand.options.some(o => o.name === 'output')).toBe(true);
  });
  it('has action function', () => {
    expect(typeof metricsCommand.action).toBe('function');
  });
});

// ── metricsAction integration tests ────────────────────────────────

describe('metricsAction', () => {
  const mockOutput = createMockOutput();
  const mockCtx: any = {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      apiKey: '',
      defaultModel: 'llama-3.3-70b',
      outputFormat: 'text',
      color: false,
      timeout: 30000,
    } as any,
    output: mockOutput as any,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('defaults to dashboard', async () => {
    await metricsAction({ command: 'metrics', positional: [], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles dashboard subcommand', async () => {
    await metricsAction({ command: 'metrics', positional: ['dashboard'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles query subcommand', async () => {
    await metricsAction({ command: 'metrics', positional: ['query', 'p50_latency'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles top subcommand', async () => {
    await metricsAction({ command: 'metrics', positional: ['top'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles history subcommand', async () => {
    await metricsAction({ command: 'metrics', positional: ['history', 'p50_latency'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles alerts subcommand', async () => {
    await metricsAction({ command: 'metrics', positional: ['alerts'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles json output for query', async () => {
    await metricsAction({ command: 'metrics', positional: ['query', 'error_rate'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
  it('handles json output for alerts', async () => {
    await metricsAction({ command: 'metrics', positional: ['alerts'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
  it('query requires metric name', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await metricsAction({ command: 'metrics', positional: ['query'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('history requires metric name', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await metricsAction({ command: 'metrics', positional: ['history'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('handles export subcommand', async () => {
    await metricsAction({ command: 'metrics', positional: ['export'], options: { format: 'json' } }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles dash alias', async () => {
    await metricsAction({ command: 'metrics', positional: ['dash'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles q alias', async () => {
    await metricsAction({ command: 'metrics', positional: ['q', 'p50_latency'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
});
