/**
 * Tests for the trust CLI command.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';

vi.mock('node:fs', () => ({
  default: {
    writeFileSync: vi.fn(),
    existsSync: vi.fn().mockReturnValue(true),
    readFileSync: vi.fn(),
  },
  writeFileSync: vi.fn(),
  existsSync: vi.fn().mockReturnValue(true),
  readFileSync: vi.fn(),
}));

import * as fs from 'node:fs';

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    trust: {
      score: vi.fn().mockImplementation((providerId: string) => {
        const scores: Record<string, any> = {
          'prov-a': {
            providerId: 'prov-a',
            providerName: 'Alpha Node',
            overallScore: 92.5,
            tee: 95,
            zk: 90,
            uptime: 94,
            ponw: 88,
            reviews: 85,
            lastUpdated: '2026-04-06T10:00:00Z',
          },
          'prov-b': {
            providerId: 'prov-b',
            providerName: 'Beta Node',
            overallScore: 67.3,
            tee: 72,
            zk: 65,
            uptime: 70,
            ponw: 55,
            reviews: 60,
            lastUpdated: '2026-04-06T09:00:00Z',
          },
        };
        return Promise.resolve(scores[providerId] || scores['prov-a']);
      }),
      providers: vi.fn().mockResolvedValue([
        {
          providerId: 'prov-a',
          providerName: 'Alpha Node',
          overallScore: 92.5,
          tee: 95,
          zk: 90,
          uptime: 94,
          ponw: 88,
          reviews: 85,
          lastUpdated: '2026-04-06T10:00:00Z',
        },
        {
          providerId: 'prov-b',
          providerName: 'Beta Node',
          overallScore: 67.3,
          tee: 72,
          zk: 65,
          uptime: 70,
          ponw: 55,
          reviews: 60,
          lastUpdated: '2026-04-06T09:00:00Z',
        },
        {
          providerId: 'prov-c',
          providerName: 'Gamma Node',
          overallScore: 35.0,
          tee: 40,
          zk: 30,
          uptime: 45,
          ponw: 20,
          reviews: 15,
          lastUpdated: '2026-04-06T08:00:00Z',
        },
      ]),
      history: vi.fn().mockResolvedValue([
        { timestamp: '2026-04-06T10:00:00Z', overallScore: 92.5, event: 'Proof verified' },
        { timestamp: '2026-04-05T10:00:00Z', overallScore: 90.0, event: 'Uptime check passed' },
        { timestamp: '2026-04-04T10:00:00Z', overallScore: 88.5, event: 'TEE attestation renewed' },
      ]),
      boost: vi.fn().mockResolvedValue({
        providerId: 'prov-a',
        previousScore: 92.5,
        newScore: 95.0,
        reason: 'Outstanding uptime',
        timestamp: '2026-04-06T12:00:00Z',
      }),
      slash: vi.fn().mockResolvedValue({
        providerId: 'prov-b',
        previousScore: 67.3,
        newScore: 57.3,
        slashed: 10,
        reason: 'Downtime violation',
        timestamp: '2026-04-06T12:00:00Z',
      }),
    },
    ...overrides,
  };
}

function createMockContext(client: any): CLIContext {
  const config: CLIConfig = {
    baseUrl: 'https://relay.xergon.gg',
    apiKey: '0xabcdef1234567890abcdef1234567890',
    defaultModel: 'llama-3.3-70b',
    outputFormat: 'text',
    color: false,
    timeout: 30000,
  };
  return {
    client,
    config,
    output: new OutputFormatter('text', false),
  };
}

// ── Tests ──────────────────────────────────────────────────────────

describe('Trust Command', () => {
  let trustCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/trust');
    trustCommand = mod.trustCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(trustCommand.name).toBe('trust');
    expect(trustCommand.aliases).toContain('reputation');
    expect(trustCommand.aliases).toContain('score');
  });

  it('has all expected options', () => {
    const optionNames = trustCommand.options.map(o => o.name);
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('provider');
    expect(optionNames).toContain('provider_a');
    expect(optionNames).toContain('provider_b');
    expect(optionNames).toContain('min_score');
    expect(optionNames).toContain('sort');
    expect(optionNames).toContain('last');
    expect(optionNames).toContain('output');
    expect(optionNames).toContain('amount');
    expect(optionNames).toContain('reason');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trustCommand.action(
      { command: 'trust', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('score|providers|history|export|compare|boost|slash'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('score subcommand shows trust score with breakdown', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['score'], options: { provider: 'prov-a' } },
      ctx
    );
    expect(mockClient.trust.score).toHaveBeenCalledWith('prov-a');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Alpha Node'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('92.5'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('TEE'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('ZK'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Uptime'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Reviews'));
    writeSpy.mockRestore();
  });

  it('score subcommand requires --provider flag', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trustCommand.action(
      { command: 'trust', positional: ['score'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--provider'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('providers subcommand lists providers', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['providers'], options: {} },
      ctx
    );
    expect(mockClient.trust.providers).toHaveBeenCalledWith({ minScore: 0 });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Alpha Node'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Beta Node'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Gamma Node'));
    writeSpy.mockRestore();
  });

  it('providers subcommand filters by min-score', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['providers'], options: { min_score: 70 } },
      ctx
    );
    expect(mockClient.trust.providers).toHaveBeenCalledWith({ minScore: 70 });
    writeSpy.mockRestore();
  });

  it('providers subcommand shows table format when --format table', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['providers'], options: { format: 'table' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Provider'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Score'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('TEE'));
    writeSpy.mockRestore();
  });

  it('history subcommand shows trust history', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['history'], options: { provider: 'prov-a' } },
      ctx
    );
    expect(mockClient.trust.history).toHaveBeenCalledWith('prov-a', { last: 20 });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('History'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Proof verified'));
    writeSpy.mockRestore();
  });

  it('history subcommand respects --last flag', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['history'], options: { provider: 'prov-a', last: 5 } },
      ctx
    );
    expect(mockClient.trust.history).toHaveBeenCalledWith('prov-a', { last: 5 });
    writeSpy.mockRestore();
  });

  it('export subcommand exports data to stdout in JSON', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['export'], options: { format: 'json' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"generatedAt"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"providers"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"overallScore"'));
    writeSpy.mockRestore();
  });

  it('export subcommand writes to file', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const writeFileSyncSpy = vi.mocked(fs.writeFileSync).mockImplementation(() => {});
    await trustCommand.action(
      { command: 'trust', positional: ['export'], options: { format: 'csv', output: '/tmp/trust.csv' } },
      ctx
    );
    expect(writeFileSyncSpy).toHaveBeenCalledWith('/tmp/trust.csv', expect.stringContaining('provider_id'), 'utf-8');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('exported'));
    writeSpy.mockRestore();
    writeFileSyncSpy.mockRestore();
  });

  it('compare subcommand compares two providers', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['compare'], options: { provider_a: 'prov-a', provider_b: 'prov-b' } },
      ctx
    );
    expect(mockClient.trust.score).toHaveBeenCalledWith('prov-a');
    expect(mockClient.trust.score).toHaveBeenCalledWith('prov-b');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Comparison'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Alpha Node'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Beta Node'));
    writeSpy.mockRestore();
  });

  it('boost subcommand boosts provider trust', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['boost'], options: { provider: 'prov-a', reason: 'Great uptime' } },
      ctx
    );
    expect(mockClient.trust.boost).toHaveBeenCalledWith({
      providerId: 'prov-a',
      reason: 'Great uptime',
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('boosted'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('95.0'));
    writeSpy.mockRestore();
  });

  it('slash subcommand slashes provider trust', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['slash'], options: { provider: 'prov-b', amount: 10, reason: 'Downtime' } },
      ctx
    );
    expect(mockClient.trust.slash).toHaveBeenCalledWith({
      providerId: 'prov-b',
      amount: 10,
      reason: 'Downtime',
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Slashed'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('57.3'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on score', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trustCommand.action(
      { command: 'trust', positional: ['score'], options: { provider: 'prov-a', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"overallScore"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"tee"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"zk"'));
    writeSpy.mockRestore();
  });

  it('rejects unknown subcommands', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trustCommand.action(
      { command: 'trust', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });
});
