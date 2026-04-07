/**
 * Tests for the governance CLI command.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    governance: {
      list: vi.fn().mockResolvedValue([
        {
          id: 'gov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
          title: 'Increase staking reward rate',
          description: 'Increase the staking reward rate from 5% to 7%.',
          category: 'parameter',
          proposer: '0xabcdef1234567890abcdef1234567890',
          status: 'active',
          votesFor: 150,
          votesAgainst: 30,
          votesAbstain: 20,
          quorum: 100,
          totalVoters: 300,
          createdAt: '2026-01-01T00:00:00Z',
          expiresAt: '2026-02-01T00:00:00Z',
        },
        {
          id: 'gov002def456abc123def456abc123def456abc123def456abc123def456abc123def4',
          title: 'Add new supported model',
          description: 'Add deepseek-v3 to the supported models list.',
          category: 'upgrade',
          proposer: '0x1234567890abcdef1234567890abcdef',
          status: 'passed',
          votesFor: 200,
          votesAgainst: 10,
          votesAbstain: 5,
          quorum: 100,
          totalVoters: 300,
          createdAt: '2025-12-15T00:00:00Z',
          expiresAt: '2026-01-15T00:00:00Z',
        },
      ]),
      create: vi.fn().mockResolvedValue({
        id: 'gov003new123456abc123def456abc123def456abc123def456abc123def456abc12345',
        title: 'Test Proposal',
        description: 'A test proposal.',
        category: 'general',
        proposer: '0xabcdef1234567890abcdef1234567890',
        status: 'active',
        votesFor: 0,
        votesAgainst: 0,
        votesAbstain: 0,
        quorum: 100,
        totalVoters: 300,
        createdAt: '2026-04-06T00:00:00Z',
        expiresAt: '2026-05-06T00:00:00Z',
      }),
      vote: vi.fn().mockResolvedValue({ proposalId: 'gov001', vote: 'for', status: 'submitted' }),
      execute: vi.fn().mockResolvedValue({ txId: 'exec123abc456', status: 'executed', message: 'Proposal executed' }),
      status: vi.fn().mockResolvedValue({
        id: 'gov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        title: 'Increase staking reward rate',
        status: 'active',
        votesFor: 150,
        votesAgainst: 30,
        votesAbstain: 20,
        totalVotes: 200,
        quorum: 100,
        quorumMet: true,
        passes: true,
        turnout: '66.7%',
        timeRemaining: '24 days',
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

describe('Governance Command', () => {
  let governanceCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/governance');
    governanceCommand = mod.governanceCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(governanceCommand.name).toBe('governance');
    expect(governanceCommand.aliases).toContain('gov');
    expect(governanceCommand.aliases).toContain('proposals');
  });

  it('has all expected options', () => {
    const optionNames = governanceCommand.options.map(o => o.name);
    expect(optionNames).toContain('proposal_id');
    expect(optionNames).toContain('vote');
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('status');
    expect(optionNames).toContain('title');
    expect(optionNames).toContain('description');
    expect(optionNames).toContain('category');
    expect(optionNames).toContain('params');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(governanceCommand.action(
      { command: 'governance', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('list|create|vote|execute|status'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('list subcommand shows proposals', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['list'], options: {} },
      ctx
    );
    expect(mockClient.governance.list).toHaveBeenCalledWith({ status: undefined });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Increase staking reward rate'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Add new supported model'));
    writeSpy.mockRestore();
  });

  it('list subcommand filters by status', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['list'], options: { status: 'passed' } },
      ctx
    );
    expect(mockClient.governance.list).toHaveBeenCalledWith({ status: 'passed' });
    writeSpy.mockRestore();
  });

  it('create subcommand creates a proposal with --title flag', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['create'], options: { title: 'Test Proposal', description: 'A test proposal.', category: 'general' } },
      ctx
    );
    expect(mockClient.governance.create).toHaveBeenCalledWith({
      title: 'Test Proposal',
      description: 'A test proposal.',
      category: 'general',
      parameters: undefined,
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('created successfully'));
    writeSpy.mockRestore();
  });

  it('vote subcommand submits a for vote', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['vote'], options: { proposal_id: 'gov001', vote: 'for' } },
      ctx
    );
    expect(mockClient.governance.vote).toHaveBeenCalledWith({ proposalId: 'gov001', vote: 'for' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('for'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('submitted'));
    writeSpy.mockRestore();
  });

  it('vote subcommand rejects invalid vote values', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(governanceCommand.action(
      { command: 'governance', positional: ['vote'], options: { proposal_id: 'gov001', vote: 'invalid' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('for, against, abstain'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('execute subcommand executes a passed proposal', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['execute'], options: { proposal_id: 'gov002' } },
      ctx
    );
    expect(mockClient.governance.execute).toHaveBeenCalledWith('gov002');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('executed successfully'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('exec123'));
    writeSpy.mockRestore();
  });

  it('status subcommand shows detailed proposal status', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['status'], options: { proposal_id: 'gov001' } },
      ctx
    );
    expect(mockClient.governance.status).toHaveBeenCalledWith('gov001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Proposal Status'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('150'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('24 days'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on list', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['list'], options: { json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"id"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"title"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"status"'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on vote', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await governanceCommand.action(
      { command: 'governance', positional: ['vote'], options: { proposal_id: 'gov001', vote: 'for', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"proposalId"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"vote"'));
    writeSpy.mockRestore();
  });

  it('rejects unknown subcommands', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(governanceCommand.action(
      { command: 'governance', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });
});
