/**
 * Tests for the chain CLI command.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    chain: {
      scanBoxes: vi.fn().mockResolvedValue([
        {
          boxId: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
          type: 'provider',
          value: '1000000000',
          region: 'us-east',
          model: 'llama-3.3-70b',
          tokens: [{ tokenId: 'tok1', amount: '100' }],
        },
        {
          boxId: 'f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5',
          type: 'staking',
          value: '500000000',
          region: 'eu-west',
          model: 'mistral-7b',
          tokens: [],
        },
      ]),
      getBox: vi.fn().mockResolvedValue({
        boxId: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
        value: '1000000000',
        ergoTree: '1006040004000e36100204deadbeef',
        registers: { R4: 'provider', R5: 'llama-3.3-70b' },
        tokens: [{ tokenId: 'tok1', amount: '100', name: 'XERGON' }],
        creationHeight: 123456,
        transactionId: 'tx123',
        index: 0,
      }),
      getBalance: vi.fn().mockResolvedValue({
        address: '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg',
        nanoErgs: '10000000000',
        ergs: '10.0',
        tokens: [{ tokenId: 'tok1', amount: '100', name: 'XERGON' }],
        boxesCount: 5,
      }),
      submitTx: vi.fn().mockResolvedValue({
        txId: 'abc123def456abc123def456abc123def456abc123def456abc123def456abc1',
        status: 'submitted',
      }),
      verifyBox: vi.fn().mockResolvedValue({
        boxId: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
        valid: true,
        expectedContract: 'provider',
        actualContract: 'provider',
        message: 'Box matches expected provider contract',
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

describe('Chain Command', () => {
  let chainCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/chain');
    chainCommand = mod.chainCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(chainCommand.name).toBe('chain');
    expect(chainCommand.aliases).toContain('onchain');
    expect(chainCommand.aliases).toContain('utxo');
  });

  it('has all expected options', () => {
    const optionNames = chainCommand.options.map(o => o.name);
    expect(optionNames).toContain('node');
    expect(optionNames).toContain('network');
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('type');
    expect(optionNames).toContain('region');
    expect(optionNames).toContain('model');
    expect(optionNames).toContain('box_id');
    expect(optionNames).toContain('token_id');
    expect(optionNames).toContain('contract');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(chainCommand.action(
      { command: 'chain', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('scan|boxes|balance|tx|verify'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('scan subcommand lists boxes via client', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chainCommand.action(
      { command: 'chain', positional: ['scan'], options: {} },
      ctx
    );
    expect(mockClient.chain.scanBoxes).toHaveBeenCalledWith({ type: undefined, region: undefined, model: undefined });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('provider'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('staking'));
    writeSpy.mockRestore();
  });

  it('scan subcommand filters by type', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chainCommand.action(
      { command: 'chain', positional: ['scan'], options: { type: 'provider' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('provider'));
    expect(writeSpy).not.toHaveBeenCalledWith(expect.stringContaining('staking'));
    writeSpy.mockRestore();
  });

  it('boxes subcommand inspects a box by ID', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chainCommand.action(
      { command: 'chain', positional: ['boxes'], options: { box_id: 'a1b2c3d4e5f6' } },
      ctx
    );
    expect(mockClient.chain.getBox).toHaveBeenCalledWith({ boxId: 'a1b2c3d4e5f6', tokenId: undefined });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Box Details'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('XERGON'));
    writeSpy.mockRestore();
  });

  it('balance subcommand shows ERG balance', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chainCommand.action(
      { command: 'chain', positional: ['balance', '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg'], options: {} },
      ctx
    );
    expect(mockClient.chain.getBalance).toHaveBeenCalledWith('9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('10.0'));
    writeSpy.mockRestore();
  });

  it('tx subcommand submits a hex transaction', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const txHex = 'deadbeef01234567';
    await chainCommand.action(
      { command: 'chain', positional: ['tx', txHex], options: {} },
      ctx
    );
    expect(mockClient.chain.submitTx).toHaveBeenCalledWith(txHex);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('submitted'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('abc123'));
    writeSpy.mockRestore();
  });

  it('tx subcommand rejects non-hex input', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(chainCommand.action(
      { command: 'chain', positional: ['tx', 'not-hex!!!'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('hex'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('verify subcommand checks a box against a contract', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chainCommand.action(
      { command: 'chain', positional: ['verify'], options: { box_id: 'a1b2c3d4', contract: 'provider' } },
      ctx
    );
    expect(mockClient.chain.verifyBox).toHaveBeenCalledWith('a1b2c3d4', 'provider');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('matches expected'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chainCommand.action(
      { command: 'chain', positional: ['scan'], options: { json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"boxId"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"type"'));
    writeSpy.mockRestore();
  });

  it('rejects unknown subcommands', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(chainCommand.action(
      { command: 'chain', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });
});
