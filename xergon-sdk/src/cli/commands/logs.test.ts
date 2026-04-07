/**
 * Tests for CLI command: logs
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  parseDuration,
  formatTimestamp,
  levelColor,
  renderLogEntry,
  passesLevelFilter,
  normalizeLogEntry,
  normalizeAlert,
  normalizeService,
  formatLogs,
  LogService,
  logsAction,
  logsCommand,
  mockSearchResult,
  mockStats,
  mockAlerts,
  mockServices,
  type LogEntry,
  type LogLevel,
} from './logs';

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
  };
}

function createMockContext(overrides?: Record<string, any>) {
  return {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      color: false,
      timeout: 30000,
    },
    output: createMockOutput(),
    ...overrides,
  };
}

// ── parseDuration tests ────────────────────────────────────────────

describe('parseDuration', () => {
  it('parses hours', () => {
    expect(parseDuration('1h')).toBe(3600_000);
  });
  it('parses minutes', () => {
    expect(parseDuration('30m')).toBe(1800_000);
  });
  it('parses seconds', () => {
    expect(parseDuration('45s')).toBe(45_000);
  });
  it('parses fractional hours', () => {
    expect(parseDuration('2.5h')).toBe(9000_000);
  });
  it('returns 0 for invalid format', () => {
    expect(parseDuration('abc')).toBe(0);
  });
  it('returns 0 for empty string', () => {
    expect(parseDuration('')).toBe(0);
  });
  it('is case insensitive', () => {
    expect(parseDuration('1H')).toBe(3600_000);
    expect(parseDuration('30M')).toBe(1800_000);
  });
});

// ── formatTimestamp tests ──────────────────────────────────────────

describe('formatTimestamp', () => {
  it('formats ISO timestamp', () => {
    const ts = '2026-04-06T23:45:00.000Z';
    const result = formatTimestamp(ts);
    expect(result).toContain('04-06');
    expect(result).toContain('23:45:00');
  });
  it('returns raw string for invalid timestamp', () => {
    expect(formatTimestamp('not-a-date')).toBe('not-a-date');
  });
  it('handles empty string', () => {
    const result = formatTimestamp('');
    expect(result).toBe('');
  });
});

// ── levelColor tests ───────────────────────────────────────────────

describe('levelColor', () => {
  it('returns plain text when color is disabled', () => {
    expect(levelColor('error', 'ERROR', false)).toBe('ERROR');
  });
  it('includes ANSI codes when color is enabled', () => {
    const result = levelColor('error', 'ERROR', true);
    expect(result).toContain('\x1b[31m');
    expect(result).toContain('\x1b[0m');
  });
  it('uses correct color for warn', () => {
    const result = levelColor('warn', 'WARN', true);
    expect(result).toContain('\x1b[33m');
  });
  it('uses correct color for info', () => {
    const result = levelColor('info', 'INFO', true);
    expect(result).toContain('\x1b[36m');
  });
  it('uses correct color for debug', () => {
    const result = levelColor('debug', 'DEBUG', true);
    expect(result).toContain('\x1b[32m');
  });
});

// ── renderLogEntry tests ───────────────────────────────────────────

describe('renderLogEntry', () => {
  const baseEntry: LogEntry = {
    timestamp: '2026-04-06T12:00:00Z',
    level: 'info',
    message: 'Test message',
  };

  it('renders basic entry without color', () => {
    const result = renderLogEntry(baseEntry, false);
    expect(result).toContain('INFO');
    expect(result).toContain('Test message');
  });
  it('includes service when present', () => {
    const result = renderLogEntry({ ...baseEntry, service: 'relay' }, false);
    expect(result).toContain('[relay]');
  });
  it('includes traceId when present', () => {
    const result = renderLogEntry({ ...baseEntry, traceId: 'abc123def456' }, false);
    expect(result).toContain('[abc123de]');
  });
});

// ── passesLevelFilter tests ────────────────────────────────────────

describe('passesLevelFilter', () => {
  const entries: LogEntry[] = [
    { timestamp: '', level: 'debug', message: '' },
    { timestamp: '', level: 'info', message: '' },
    { timestamp: '', level: 'warn', message: '' },
    { timestamp: '', level: 'error', message: '' },
  ];

  it('passes all when no filter', () => {
    expect(entries.every(e => passesLevelFilter(e))).toBe(true);
  });
  it('filters by error level', () => {
    expect(passesLevelFilter(entries[3], 'error')).toBe(true);
    expect(passesLevelFilter(entries[0], 'error')).toBe(false);
  });
  it('filters by warn level', () => {
    expect(passesLevelFilter(entries[2], 'warn')).toBe(true);
    expect(passesLevelFilter(entries[3], 'warn')).toBe(true);
    expect(passesLevelFilter(entries[1], 'warn')).toBe(false);
  });
});

// ── normalizeLogEntry tests ────────────────────────────────────────

describe('normalizeLogEntry', () => {
  it('normalizes full entry', () => {
    const entry = normalizeLogEntry({
      timestamp: '2026-01-01T00:00:00Z',
      level: 'error',
      message: 'test',
      service: 'relay',
    });
    expect(entry.level).toBe('error');
    expect(entry.service).toBe('relay');
    expect(entry.message).toBe('test');
  });
  it('handles alternative field names', () => {
    const entry = normalizeLogEntry({
      time: '2026-01-01',
      severity: 'warn',
      msg: 'hello',
      component: 'auth',
    });
    expect(entry.timestamp).toBe('2026-01-01');
    expect(entry.level).toBe('warn');
    expect(entry.message).toBe('hello');
    expect(entry.service).toBe('auth');
  });
  it('defaults to info level', () => {
    const entry = normalizeLogEntry({ message: 'test' });
    expect(entry.level).toBe('info');
  });
});

// ── normalizeAlert tests ───────────────────────────────────────────

describe('normalizeAlert', () => {
  it('normalizes full alert', () => {
    const alert = normalizeAlert({
      id: 'a1',
      type: 'error_spike',
      severity: 'critical',
      message: 'test alert',
      service: 'relay',
      timestamp: '2026-01-01',
    });
    expect(alert.id).toBe('a1');
    expect(alert.type).toBe('error_spike');
    expect(alert.severity).toBe('critical');
  });
  it('provides defaults for missing fields', () => {
    const alert = normalizeAlert({});
    expect(alert.id).toBeTruthy();
    expect(alert.severity).toBe('warning');
    expect(alert.acknowledged).toBe(false);
  });
});

// ── normalizeService tests ─────────────────────────────────────────

describe('normalizeService', () => {
  it('normalizes service info', () => {
    const svc = normalizeService({
      name: 'relay',
      status: 'active',
      log_count: 100,
      last_log: '2026-01-01',
    });
    expect(svc.name).toBe('relay');
    expect(svc.logCount).toBe(100);
    expect(svc.lastLog).toBe('2026-01-01');
  });
  it('provides defaults', () => {
    const svc = normalizeService({});
    expect(svc.name).toBe('unknown');
    expect(svc.status).toBe('active');
    expect(svc.logCount).toBe(0);
  });
});

// ── formatLogs tests ───────────────────────────────────────────────

describe('formatLogs', () => {
  const entries: LogEntry[] = [
    { timestamp: '2026-01-01', level: 'info', message: 'hello', service: 'relay' },
    { timestamp: '2026-01-02', level: 'error', message: 'bad', service: 'auth' },
  ];

  it('formats as JSON', () => {
    const result = formatLogs(entries, 'json');
    const parsed = JSON.parse(result);
    expect(parsed).toHaveLength(2);
    expect(parsed[0].message).toBe('hello');
  });
  it('formats as CSV with header', () => {
    const result = formatLogs(entries, 'csv');
    expect(result.startsWith('timestamp,level,service,message,traceId')).toBe(true);
    expect(result.split('\n').length).toBe(3); // header + 2 rows
  });
  it('formats as text', () => {
    const result = formatLogs(entries, 'text');
    expect(result).toContain('INFO');
    expect(result).toContain('hello');
  });
});

// ── mock data generators tests ─────────────────────────────────────

describe('mockSearchResult', () => {
  it('returns results with text query', () => {
    const result = mockSearchResult({ text: 'error', limit: 10 });
    expect(result.query.text).toBe('error');
    expect(result.totalMatches).toBeGreaterThanOrEqual(0);
    expect(result.tookMs).toBeGreaterThan(0);
  });
  it('returns all entries without query', () => {
    const result = mockSearchResult({ limit: 5 });
    expect(result.entries.length).toBe(5);
  });
});

describe('mockStats', () => {
  it('returns global stats', () => {
    const stats = mockStats();
    expect(stats.totalEntries).toBeGreaterThan(0);
    expect(stats.byLevel.error).toBeGreaterThan(0);
    expect(stats.topErrors.length).toBeGreaterThan(0);
  });
  it('returns service-specific stats', () => {
    const stats = mockStats('relay');
    expect(stats.byService).toHaveProperty('relay');
  });
});

describe('mockAlerts', () => {
  it('returns array of alerts', () => {
    const alerts = mockAlerts();
    expect(alerts.length).toBeGreaterThan(0);
    expect(alerts.some(a => a.severity === 'critical')).toBe(true);
    expect(alerts.some(a => a.acknowledged)).toBe(true);
  });
});

describe('mockServices', () => {
  it('returns array of services', () => {
    const services = mockServices();
    expect(services.length).toBeGreaterThan(0);
    expect(services.some(s => s.status === 'active')).toBe(true);
    expect(services.some(s => s.status === 'inactive')).toBe(true);
  });
});

// ── LogService tests ───────────────────────────────────────────────

describe('LogService', () => {
  it('constructs with base URL', () => {
    const svc = new LogService('https://example.com/api');
    // Private field - just ensure no error
    expect(svc).toBeDefined();
  });
  it('fetchLogs returns entries (mock)', async () => {
    const svc = new LogService('https://nonexistent.invalid');
    const entries = await svc.fetchLogs({ limit: 10 });
    // Should return empty since fetch fails
    expect(Array.isArray(entries)).toBe(true);
  });
  it('searchLogs returns results (mock)', async () => {
    const svc = new LogService('https://nonexistent.invalid');
    const result = await svc.searchLogs({ text: 'test' });
    expect(result.totalMatches).toBeGreaterThanOrEqual(0);
    expect(Array.isArray(result.entries)).toBe(true);
  });
  it('getStats returns stats (mock)', async () => {
    const svc = new LogService('https://nonexistent.invalid');
    const stats = await svc.getStats();
    expect(stats.totalEntries).toBeGreaterThanOrEqual(0);
    expect(stats.byLevel).toBeDefined();
  });
  it('getAlerts returns alerts (mock)', async () => {
    const svc = new LogService('https://nonexistent.invalid');
    const alerts = await svc.getAlerts();
    expect(Array.isArray(alerts)).toBe(true);
  });
  it('getServices returns services (mock)', async () => {
    const svc = new LogService('https://nonexistent.invalid');
    const services = await svc.getServices();
    expect(Array.isArray(services)).toBe(true);
  });
});

// ── logsCommand definition ─────────────────────────────────────────

describe('logsCommand', () => {
  it('has correct name', () => {
    expect(logsCommand.name).toBe('logs');
  });
  it('has description', () => {
    expect(logsCommand.description).toContain('logs');
  });
  it('has aliases', () => {
    expect(logsCommand.aliases).toContain('log');
  });
  it('has options', () => {
    expect(logsCommand.options.length).toBeGreaterThan(0);
    expect(logsCommand.options.some(o => o.name === 'json')).toBe(true);
    expect(logsCommand.options.some(o => o.name === 'follow')).toBe(true);
    expect(logsCommand.options.some(o => o.name === 'level')).toBe(true);
    expect(logsCommand.options.some(o => o.name === 'service')).toBe(true);
    expect(logsCommand.options.some(o => o.name === 'limit')).toBe(true);
    expect(logsCommand.options.some(o => o.name === 'format')).toBe(true);
  });
  it('has action function', () => {
    expect(typeof logsCommand.action).toBe('function');
  });
});

// ── logsAction integration tests ───────────────────────────────────

describe('logsAction', () => {
  const mockOutput = createMockOutput();
  const mockCtx: any = {
    client: null,
    config: { baseUrl: 'https://relay.xergon.gg', color: false, timeout: 30000 } as any,
    output: mockOutput as any,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('defaults to tail when no subcommand', async () => {
    await logsAction({ command: 'logs', positional: [], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles stats subcommand', async () => {
    await logsAction({ command: 'logs', positional: ['stats'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles services subcommand', async () => {
    await logsAction({ command: 'logs', positional: ['services'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles alerts subcommand', async () => {
    await logsAction({ command: 'logs', positional: ['alerts'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles export subcommand', async () => {
    await logsAction({ command: 'logs', positional: ['export', 'json'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles search subcommand', async () => {
    await logsAction({ command: 'logs', positional: ['search', 'error'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles json output flag', async () => {
    await logsAction({ command: 'logs', positional: ['stats'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
  it('rejects invalid log level', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await logsAction({ command: 'logs', positional: ['tail'], options: { level: 'invalid' } }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('search requires query text', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await logsAction({ command: 'logs', positional: ['search'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('export rejects invalid format', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await logsAction({ command: 'logs', positional: ['export', 'xml'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
});
