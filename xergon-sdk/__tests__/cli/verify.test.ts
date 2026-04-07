/**
 * Tests for the verify CLI command.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';

vi.mock('node:fs', () => ({
  default: {
    existsSync: vi.fn().mockReturnValue(true),
    readFileSync: vi.fn().mockReturnValue(Buffer.from('fake-proof-data')),
    writeFileSync: vi.fn(),
  },
  existsSync: vi.fn().mockReturnValue(true),
  readFileSync: vi.fn().mockReturnValue(Buffer.from('fake-proof-data')),
  writeFileSync: vi.fn(),
}));

import * as fs from 'node:fs';

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    verify: {
      proof: vi.fn().mockResolvedValue({
        proofId: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        valid: true,
        proofType: 'groth16',
        verifiedAt: '2026-04-06T12:00:00Z',
        details: 'ZK proof verified successfully. Circuit constraints satisfied.',
        commitment: '0xdeadbeef1234567890abcdef1234567890abcdef1234567890abcdef123456',
        circuit: 'xergon-inference-v2',
      }),
      commitment: vi.fn().mockResolvedValue({
        hash: '0xabc123def456',
        valueHash: '0xabc123def456',
        valid: true,
        algorithm: 'sha256',
        verifiedAt: '2026-04-06T12:00:00Z',
      }),
      anchor: vi.fn().mockResolvedValue({
        proofId: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        anchored: true,
        txId: 'tx001abc123def456abc123def456abc123def456abc123def456abc123def4',
        blockHeight: 987654,
        chainHeight: 987660,
        confirmations: 6,
        anchoredAt: '2026-04-06T10:00:00Z',
        boxId: 'box001abc123def456abc123def456abc123def456abc123def456abc123def4',
        register: 'R4',
        details: 'Proof hash anchored in Ergo box R4 register with 6 confirmations.',
      }),
      batch: vi.fn().mockResolvedValue({
        total: 3,
        passed: 2,
        failed: 1,
        results: [
          { proofId: 'proof001abc123', valid: true, message: 'Valid proof' },
          { proofId: 'proof002def456', valid: true, message: 'Valid proof' },
          { proofId: 'proof003ghi789', valid: false, message: 'Invalid circuit constraint' },
        ],
        verifiedAt: '2026-04-06T12:00:00Z',
      }),
      onchain: vi.fn().mockResolvedValue({
        boxId: 'box001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        register: 'R4',
        expectedHash: '0xdeadbeef1234567890abcdef12345678',
        actualHash: '0xdeadbeef1234567890abcdef12345678',
        valid: true,
        chainHeight: 987660,
        boxCreationHeight: 987654,
        boxValue: '10000000 nanoERG',
        registerValue: '0xdeadbeef1234567890abcdef12345678',
        verifiedAt: '2026-04-06T12:00:00Z',
        explanation: 'The R4 register of the Ergo box contains the expected proof hash. The proof was anchored at block height 987654 with 6 confirmations. Sigma protocol verification confirms the register value was set by the authorized contract.',
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

describe('Verify Command', () => {
  let verifyCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/verify');
    verifyCommand = mod.verifyCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(verifyCommand.name).toBe('verify');
    expect(verifyCommand.aliases).toContain('verification');
    expect(verifyCommand.aliases).toContain('check');
  });

  it('has all expected options', () => {
    const optionNames = verifyCommand.options.map(o => o.name);
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('proof_id');
    expect(optionNames).toContain('hash');
    expect(optionNames).toContain('value');
    expect(optionNames).toContain('file');
    expect(optionNames).toContain('box_id');
    expect(optionNames).toContain('register');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('proof|commitment|anchor|batch|onchain'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('proof subcommand verifies a ZK proof', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await verifyCommand.action(
      { command: 'verify', positional: ['proof'], options: { proof_id: 'proof001' } },
      ctx
    );
    expect(mockClient.verify.proof).toHaveBeenCalledWith({ proofId: 'proof001' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('VALID'));
    writeSpy.mockRestore();
  });

  it('proof subcommand requires --proof-id', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['proof'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--proof-id'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('commitment subcommand verifies value against hash', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(Buffer.from('test-value-data'));
    await verifyCommand.action(
      { command: 'verify', positional: ['commitment'], options: { hash: '0xabc123', value: '/tmp/value.bin' } },
      ctx
    );
    expect(mockClient.verify.commitment).toHaveBeenCalledWith({
      hash: '0xabc123',
      value: expect.any(Buffer),
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('VERIFIED'));
    writeSpy.mockRestore();
  });

  it('commitment subcommand requires --hash and --value', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['commitment'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--hash'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('commitment subcommand rejects missing value file', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    vi.mocked(fs.existsSync).mockReturnValue(false);
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['commitment'], options: { hash: '0xabc123', value: '/tmp/missing.bin' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('not found'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('anchor subcommand checks blockchain anchor', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await verifyCommand.action(
      { command: 'verify', positional: ['anchor'], options: { proof_id: 'proof001' } },
      ctx
    );
    expect(mockClient.verify.anchor).toHaveBeenCalledWith({ proofId: 'proof001' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('ANCHORED'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('confirmations'));
    writeSpy.mockRestore();
  });

  it('anchor subcommand requires --proof-id', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['anchor'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--proof-id'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('batch subcommand batch verifies proofs from file', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const batchData = JSON.stringify([
      { proofId: 'p1', data: 'proof1' },
      { proofId: 'p2', data: 'proof2' },
      { proofId: 'p3', data: 'proof3' },
    ]);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockImplementation((p: any, enc?: any) => {
      if (enc === 'utf-8') return batchData;
      return Buffer.from('fake');
    });
    await verifyCommand.action(
      { command: 'verify', positional: ['batch'], options: { file: '/tmp/batch.json' } },
      ctx
    );
    expect(mockClient.verify.batch).toHaveBeenCalledWith({
      proofs: expect.arrayContaining([
        expect.objectContaining({ proofId: 'p1' }),
      ]),
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Batch Verification'));
    writeSpy.mockRestore();
  });

  it('batch subcommand requires --file', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['batch'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--file'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('batch subcommand rejects missing batch file', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    vi.mocked(fs.existsSync).mockReturnValue(false);
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['batch'], options: { file: '/tmp/missing.json' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('not found'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('onchain subcommand verifies Ergo box register', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await verifyCommand.action(
      { command: 'verify', positional: ['onchain'], options: { box_id: 'box001', register: 'R4' } },
      ctx
    );
    expect(mockClient.verify.onchain).toHaveBeenCalledWith({ boxId: 'box001', register: 'R4' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('PASSED'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Register Model'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('R4'));
    writeSpy.mockRestore();
  });

  it('onchain subcommand requires --box-id and --register', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['onchain'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--box-id'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('onchain subcommand rejects invalid register', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['onchain'], options: { box_id: 'box001', register: 'R1' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('R4, R5, R6'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('unknown subcommand shows error', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('proof outputs JSON when --json flag is set', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await verifyCommand.action(
      { command: 'verify', positional: ['proof'], options: { proof_id: 'proof001', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"proofId"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"valid"'));
    writeSpy.mockRestore();
  });

  it('onchain outputs JSON when --json flag is set', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await verifyCommand.action(
      { command: 'verify', positional: ['onchain'], options: { box_id: 'box001', register: 'R4', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"boxId"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"valid"'));
    writeSpy.mockRestore();
  });

  it('handles client not available error', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    const noClientCtx = createMockContext({ verify: undefined });
    await expect(verifyCommand.action(
      { command: 'verify', positional: ['proof'], options: { proof_id: 'proof001' } },
      noClientCtx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('not available'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('anchor shows confirmation count info', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await verifyCommand.action(
      { command: 'verify', positional: ['anchor'], options: { proof_id: 'proof001' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('6 confirmation'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('well-anchored'));
    writeSpy.mockRestore();
  });
});
