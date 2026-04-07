/**
 * Tests for CLI command: status
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  toHealthLevel,
  formatUptime,
  resolveAgentUrl,
  measureLatency,
  renderStatusIcon,
  renderDefaultStatus,
  renderProvidersTable,
  renderModelsTable,
  renderNetworkStats,
  renderShardsTable,
  type AgentStatus,
  type ProviderEntry,
  type ModelEntry,
  type NetworkStats,
  type ShardEntry,
  type HealthLevel,
  statusCommand,
} from './status';

// Mock output formatter
function createMockOutput() {
  return {
    colorize: (text: string, _style: string) => text,
    write: vi.fn(),
    writeError: vi.fn(),
    setFormat: vi.fn(),
  };
}

// ── toHealthLevel ──────────────────────────────────────────────────

describe('toHealthLevel', () => {
  it('maps "healthy" to healthy', () => {
    expect(toHealthLevel('healthy')).toBe('healthy');
  });

  it('maps "active" to healthy', () => {
    expect(toHealthLevel('active')).toBe('healthy');
  });

  it('maps "degraded" to degraded', () => {
    expect(toHealthLevel('degraded')).toBe('degraded');
  });

  it('maps "error" to error', () => {
    expect(toHealthLevel('error')).toBe('error');
  });

  it('maps "offline" to error', () => {
    expect(toHealthLevel('offline')).toBe('error');
  });

  it('maps undefined to unknown', () => {
    expect(toHealthLevel(undefined)).toBe('unknown');
  });

  it('maps "foo" to unknown', () => {
    expect(toHealthLevel('foo')).toBe('unknown');
  });

  it('uses healthy=true boolean override', () => {
    expect(toHealthLevel('error', true)).toBe('healthy');
  });

  it('uses healthy=false boolean override', () => {
    expect(toHealthLevel('healthy', false)).toBe('error');
  });
});

// ── formatUptime ──────────────────────────────────────────────────

describe('formatUptime', () => {
  it('formats seconds only', () => {
    expect(formatUptime(90)).toBe('1m');
  });

  it('formats hours and minutes', () => {
    expect(formatUptime(3720)).toBe('1h 2m');
  });

  it('formats days, hours, and minutes', () => {
    expect(formatUptime(90061)).toBe('1d 1h 1m');
  });

  it('formats zero seconds', () => {
    expect(formatUptime(0)).toBe('0m');
  });

  it('returns unknown for negative values', () => {
    expect(formatUptime(-1)).toBe('unknown');
  });
});

// ── renderStatusIcon ──────────────────────────────────────────────

describe('renderStatusIcon', () => {
  it('returns non-empty string for healthy', () => {
    const result = renderStatusIcon('healthy');
    expect(result.length).toBeGreaterThan(0);
  });

  it('returns non-empty string for degraded', () => {
    const result = renderStatusIcon('degraded');
    expect(result.length).toBeGreaterThan(0);
  });

  it('returns non-empty string for error', () => {
    const result = renderStatusIcon('error');
    expect(result.length).toBeGreaterThan(0);
  });

  it('returns non-empty string for unknown', () => {
    const result = renderStatusIcon('unknown');
    expect(result.length).toBeGreaterThan(0);
  });
});

// ── resolveAgentUrl ───────────────────────────────────────────────

describe('resolveAgentUrl', () => {
  it('returns provided URL when given', () => {
    expect(resolveAgentUrl('http://localhost:8080')).toBe('http://localhost:8080');
  });

  it('strips trailing slashes from provided URL', () => {
    expect(resolveAgentUrl('http://localhost:8080/')).toBe('http://localhost:8080');
  });

  it('returns default when no config and no argument', () => {
    const result = resolveAgentUrl();
    expect(result).toContain('127.0.0.1');
    expect(result).toContain('9099');
  });
});

// ── renderDefaultStatus ───────────────────────────────────────────

describe('renderDefaultStatus', () => {
  const mockAgent: AgentStatus = {
    ponwScore: 75,
    ergBalance: '12.5',
    modelsServing: 3,
    uptime: '2h 30m',
    connectedRelays: 2,
    agentVersion: '0.1.0',
    agentUrl: 'http://127.0.0.1:9099',
    checks: [
      { name: 'Agent', status: 'healthy', detail: 'Online (50ms)' },
      { name: 'Relay', status: 'healthy', detail: 'Online (120ms)' },
    ],
    summary: { healthy: 2, degraded: 0, error: 0 },
  };

  it('renders agent status with all fields', () => {
    const output = createMockOutput();
    const result = renderDefaultStatus(mockAgent, output, false);
    expect(result).toContain('Xergon Agent Status');
    expect(result).toContain('0.1.0');
    expect(result).toContain('75');
    expect(result).toContain('12.5');
    expect(result).toContain('2h 30m');
    expect(result).toContain('Agent');
    expect(result).toContain('2 OK');
  });

  it('renders error count in summary', () => {
    const agentWithError = { ...mockAgent, summary: { healthy: 1, degraded: 0, error: 1 } };
    const output = createMockOutput();
    const result = renderDefaultStatus(agentWithError, output, false);
    expect(result).toContain('1 ERR');
  });
});

// ── renderProvidersTable ──────────────────────────────────────────

describe('renderProvidersTable', () => {
  it('renders empty providers list', () => {
    const output = createMockOutput();
    const result = renderProvidersTable([], output);
    expect(result).toContain('No active providers');
  });

  it('renders provider entries', () => {
    const providers: ProviderEntry[] = [
      { id: 'prov-1', address: '0x1234', health: 'healthy', score: 85, latencyMs: 45, models: 3, status: 'active' },
      { id: 'prov-2', address: '0x5678', health: 'degraded', score: 42, latencyMs: 800, models: 1, status: 'warning' },
    ];
    const output = createMockOutput();
    const result = renderProvidersTable(providers, output);
    expect(result).toContain('prov-1');
    expect(result).toContain('prov-2');
    expect(result).toContain('0x1234');
    expect(result).toContain('2 provider(s)');
  });
});

// ── renderModelsTable ─────────────────────────────────────────────

describe('renderModelsTable', () => {
  it('renders empty models list', () => {
    const output = createMockOutput();
    const result = renderModelsTable([], output);
    expect(result).toContain('No models');
  });

  it('renders model entries', () => {
    const models: ModelEntry[] = [
      { id: 'm1', name: 'llama-3.3-70b', requests: 150, gpuUsagePct: 65, vramUsedGB: 24.5, vramTotalGB: 48.0, provider: 'node-1', status: 'active' },
    ];
    const output = createMockOutput();
    const result = renderModelsTable(models, output);
    expect(result).toContain('llama-3.3-70b');
    expect(result).toContain('150');
    expect(result).toContain('1 model(s)');
  });
});

// ── renderNetworkStats ───────────────────────────────────────────

describe('renderNetworkStats', () => {
  it('renders network statistics', () => {
    const stats: NetworkStats = {
      peers: 12,
      blockHeight: 1234567,
      syncStatus: 'synced',
      syncProgress: 100,
      connectedRelays: 3,
      relayLatencyMs: 85,
      networkUptime: '99.9%',
    };
    const output = createMockOutput();
    const result = renderNetworkStats(stats, output);
    expect(result).toContain('12');
    expect(result).toContain('1,234,567');
    expect(result).toContain('SYNCED');
    expect(result).toContain('85ms');
  });

  it('shows sync progress when syncing', () => {
    const stats: NetworkStats = {
      peers: 5,
      blockHeight: 100,
      syncStatus: 'syncing',
      syncProgress: 45.5,
      connectedRelays: 1,
      relayLatencyMs: 200,
      networkUptime: 'unknown',
    };
    const output = createMockOutput();
    const result = renderNetworkStats(stats, output);
    expect(result).toContain('45.5%');
    expect(result).toContain('SYNCING');
  });
});

// ── renderShardsTable ─────────────────────────────────────────────

describe('renderShardsTable', () => {
  it('renders empty shards list', () => {
    const output = createMockOutput();
    const result = renderShardsTable([], output);
    expect(result).toContain('No shard data');
  });

  it('renders shard entries grouped by model', () => {
    const shards: ShardEntry[] = [
      { modelId: 'llama-70b', shardIndex: 0, totalShards: 2, gpuId: 'gpu-0', gpuType: 'A100', vramUsedGB: 20.0, vramTotalGB: 40.0, status: 'healthy' },
      { modelId: 'llama-70b', shardIndex: 1, totalShards: 2, gpuId: 'gpu-1', gpuType: 'A100', vramUsedGB: 20.0, vramTotalGB: 40.0, status: 'healthy' },
    ];
    const output = createMockOutput();
    const result = renderShardsTable(shards, output);
    expect(result).toContain('llama-70b');
    expect(result).toContain('1/2');
    expect(result).toContain('2/2');
    expect(result).toContain('gpu-0');
    expect(result).toContain('2 shard(s)');
    expect(result).toContain('1 model(s)');
  });
});

// ── statusCommand ─────────────────────────────────────────────────

describe('statusCommand', () => {
  it('has correct name', () => {
    expect(statusCommand.name).toBe('status');
  });

  it('has description', () => {
    expect(statusCommand.description).toBeTruthy();
  });

  it('has aliases', () => {
    expect(statusCommand.aliases).toContain('health');
    expect(statusCommand.aliases).toContain('check');
  });

  it('has options', () => {
    expect(statusCommand.options.length).toBeGreaterThan(0);
  });

  it('has action function', () => {
    expect(typeof statusCommand.action).toBe('function');
  });
});
