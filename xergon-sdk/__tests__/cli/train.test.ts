/**
 * Tests for the train CLI command.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';
import * as fs from 'node:fs';

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    train: {
      start: vi.fn().mockResolvedValue({
        id: 'train001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        model: 'llama-3.3-70b',
        strategy: 'fedavg',
        totalRounds: 10,
        currentRound: 0,
        phase: 'collecting',
        minProviders: 3,
        participants: [],
        createdAt: '2026-04-06T00:00:00Z',
        updatedAt: '2026-04-06T00:00:00Z',
      }),
      join: vi.fn().mockResolvedValue({ roundId: 'train001', providerId: 'prov001', status: 'joined' }),
      status: vi.fn().mockResolvedValue({
        id: 'train001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        model: 'llama-3.3-70b',
        strategy: 'fedavg',
        totalRounds: 10,
        currentRound: 4,
        phase: 'training',
        minProviders: 3,
        participants: [
          { providerId: 'prov001abc123', status: 'submitted', deltaSize: 5242880, submittedAt: '2026-04-06T01:00:00Z' },
          { providerId: 'prov002def456', status: 'training', deltaSize: undefined, submittedAt: undefined },
          { providerId: 'prov003ghi789', status: 'joined', deltaSize: undefined, submittedAt: undefined },
        ],
        createdAt: '2026-04-06T00:00:00Z',
        updatedAt: '2026-04-06T01:00:00Z',
      }),
      submit: vi.fn().mockResolvedValue({ roundId: 'train001', status: 'submitted' }),
      list: vi.fn().mockResolvedValue([
        {
          id: 'train001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
          model: 'llama-3.3-70b',
          strategy: 'fedavg',
          phase: 'training',
          currentRound: 4,
          totalRounds: 10,
          participants: 3,
          createdAt: '2026-04-06T00:00:00Z',
        },
        {
          id: 'train002def456abc123def456abc123def456abc123def456abc123def456abc123def4',
          model: 'mixtral-8x7b',
          strategy: 'fedprox',
          phase: 'complete',
          currentRound: 5,
          totalRounds: 5,
          participants: 4,
          createdAt: '2026-04-05T00:00:00Z',
        },
      ]),
      cancel: vi.fn().mockResolvedValue({ roundId: 'train001', status: 'cancelled' }),
      aggregate: vi.fn().mockResolvedValue({
        roundId: 'train001',
        aggregatedWeightsUrl: 'ipfs://QmWeightsHash',
        participantsIncluded: 3,
        averageLoss: 0.0342,
        timestamp: '2026-04-06T02:00:00Z',
      }),
      distill: vi.fn().mockResolvedValue({
        jobId: 'distill001abc123',
        teacher: 'llama-3.3-70b',
        student: 'llama-3.3-8b',
        temperature: 4.0,
        alpha: 0.5,
        status: 'running',
        estimatedTime: '2h 30m',
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

describe('Train Command', () => {
  let trainCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/train');
    trainCommand = mod.trainCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(trainCommand.name).toBe('train');
    expect(trainCommand.aliases).toContain('federated');
    expect(trainCommand.aliases).toContain('fl');
  });

  it('has all expected options', () => {
    const optionNames = trainCommand.options.map(o => o.name);
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('round_id');
    expect(optionNames).toContain('model');
    expect(optionNames).toContain('strategy');
    expect(optionNames).toContain('rounds');
    expect(optionNames).toContain('min_providers');
    expect(optionNames).toContain('provider_id');
    expect(optionNames).toContain('delta_file');
    expect(optionNames).toContain('status');
    expect(optionNames).toContain('reason');
    expect(optionNames).toContain('teacher');
    expect(optionNames).toContain('student');
    expect(optionNames).toContain('temperature');
    expect(optionNames).toContain('alpha');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trainCommand.action(
      { command: 'train', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('start|join|status|submit|list|cancel|aggregate|distill'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('start subcommand starts a training round', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['start'], options: { model: 'llama-3.3-70b', rounds: 10, strategy: 'fedavg', min_providers: 3 } },
      ctx
    );
    expect(mockClient.train.start).toHaveBeenCalledWith({
      model: 'llama-3.3-70b',
      rounds: 10,
      strategy: 'fedavg',
      minProviders: 3,
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('started successfully'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('train001'));
    writeSpy.mockRestore();
  });

  it('start subcommand requires --model flag', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trainCommand.action(
      { command: 'train', positional: ['start'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--model'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('start subcommand rejects invalid strategy', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trainCommand.action(
      { command: 'train', positional: ['start'], options: { model: 'test', strategy: 'invalid' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('fedavg, fedprox'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('join subcommand joins a training round', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['join'], options: { round_id: 'train001', provider_id: 'prov001' } },
      ctx
    );
    expect(mockClient.train.join).toHaveBeenCalledWith({ roundId: 'train001', providerId: 'prov001' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('joined'));
    writeSpy.mockRestore();
  });

  it('status subcommand shows training round status', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['status'], options: { round_id: 'train001' } },
      ctx
    );
    expect(mockClient.train.status).toHaveBeenCalledWith('train001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Training Round Status'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('prov001'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('5.0 MB'));
    writeSpy.mockRestore();
  });

  it('list subcommand lists training rounds', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['list'], options: {} },
      ctx
    );
    expect(mockClient.train.list).toHaveBeenCalledWith({ status: 'all' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('llama-3.3-70b'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('mixtral-8x7b'));
    writeSpy.mockRestore();
  });

  it('list subcommand filters by status', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['list'], options: { status: 'complete' } },
      ctx
    );
    expect(mockClient.train.list).toHaveBeenCalledWith({ status: 'complete' });
    writeSpy.mockRestore();
  });

  it('cancel subcommand cancels a training round', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['cancel'], options: { round_id: 'train001', reason: 'Not needed' } },
      ctx
    );
    expect(mockClient.train.cancel).toHaveBeenCalledWith('train001', 'Not needed');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('cancelled'));
    writeSpy.mockRestore();
  });

  it('aggregate subcommand triggers aggregation', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['aggregate'], options: { round_id: 'train001' } },
      ctx
    );
    expect(mockClient.train.aggregate).toHaveBeenCalledWith('train001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Aggregation completed'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('0.034'));
    writeSpy.mockRestore();
  });

  it('distill subcommand starts knowledge distillation', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['distill'], options: { teacher: 'llama-3.3-70b', student: 'llama-3.3-8b', temperature: 4.0, alpha: 0.5 } },
      ctx
    );
    expect(mockClient.train.distill).toHaveBeenCalledWith({
      teacher: 'llama-3.3-70b',
      student: 'llama-3.3-8b',
      temperature: 4.0,
      alpha: 0.5,
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('distillation job started'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on start', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['start'], options: { model: 'llama-3.3-70b', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"id"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"model"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"phase"'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on list', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['list'], options: { json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"id"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"strategy"'));
    writeSpy.mockRestore();
  });

  it('rejects unknown subcommands', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(trainCommand.action(
      { command: 'train', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('status subcommand shows table format when --format table', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await trainCommand.action(
      { command: 'train', positional: ['status'], options: { round_id: 'train001', format: 'table' } },
      ctx
    );
    expect(mockClient.train.status).toHaveBeenCalledWith('train001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Provider'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('SUBMITTED'));
    writeSpy.mockRestore();
  });
});
