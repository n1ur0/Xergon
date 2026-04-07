/**
 * Tests for the proof CLI command.
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
    proof: {
      verify: vi.fn().mockResolvedValue({
        valid: true,
        proofId: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        commitment: '0xdeadbeef1234567890abcdef1234567890abcdef1234567890abcdef123456',
        message: 'Proof is valid',
        verifiedAt: '2026-04-06T12:00:00Z',
      }),
      batchVerify: vi.fn().mockResolvedValue({
        total: 3,
        passed: 2,
        failed: 1,
        results: [
          { commitment: '0xaaa', valid: true, message: 'Valid proof' },
          { commitment: '0xbbb', valid: true, message: 'Valid proof' },
          { commitment: '0xccc', valid: false, message: 'Invalid commitment' },
        ],
      }),
      submit: vi.fn().mockResolvedValue({
        proofId: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        providerId: 'prov001',
        status: 'submitted',
        submittedAt: '2026-04-06T12:00:00Z',
      }),
      list: vi.fn().mockResolvedValue([
        {
          id: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
          providerId: 'prov001abc123def456',
          commitment: '0xcommit1',
          status: 'verified',
          createdAt: '2026-04-06T10:00:00Z',
          verifiedAt: '2026-04-06T11:00:00Z',
        },
        {
          id: 'proof002def456abc123def456abc123def456abc123def456abc123def456abc123def4',
          providerId: 'prov001abc123def456',
          commitment: '0xcommit2',
          status: 'pending',
          createdAt: '2026-04-06T10:30:00Z',
        },
        {
          id: 'proof003ghi789abc123def456abc123def456abc123def456abc123def456abc123def4',
          providerId: 'prov001abc123def456',
          commitment: '0xcommit3',
          status: 'rejected',
          createdAt: '2026-04-06T09:00:00Z',
        },
      ]),
      status: vi.fn().mockResolvedValue({
        id: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        providerId: 'prov001abc123def456',
        commitment: '0xcommit1',
        status: 'verified',
        createdAt: '2026-04-06T10:00:00Z',
        verifiedAt: '2026-04-06T11:00:00Z',
        anchorTxId: 'tx001abc123def456abc123def456abc123def456abc123def456abc123def4',
      }),
      anchor: vi.fn().mockResolvedValue({
        proofId: 'proof001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        txId: 'tx001abc123def456abc123def456abc123def456abc123def456abc123def4',
        blockHeight: 987654,
        status: 'anchored',
        anchoredAt: '2026-04-06T12:00:00Z',
      }),
      trust: vi.fn().mockResolvedValue({
        providerId: 'prov001abc123def456',
        overallScore: 85.5,
        tee: 90,
        zk: 82,
        uptime: 88,
        ponw: 75,
        reviews: 70,
        lastUpdated: '2026-04-06T11:00:00Z',
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

describe('Proof Command', () => {
  let proofCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/proof');
    proofCommand = mod.proofCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(proofCommand.name).toBe('proof');
    expect(proofCommand.aliases).toContain('zkp');
    expect(proofCommand.aliases).toContain('proofs');
  });

  it('has all expected options', () => {
    const optionNames = proofCommand.options.map(o => o.name);
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('proof');
    expect(optionNames).toContain('commitment');
    expect(optionNames).toContain('batch');
    expect(optionNames).toContain('provider');
    expect(optionNames).toContain('proof_id');
    expect(optionNames).toContain('status');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(proofCommand.action(
      { command: 'proof', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('verify|submit|list|status|anchor|trust'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('verify subcommand verifies a proof', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(Buffer.from('fake-proof-data'));
    await proofCommand.action(
      { command: 'proof', positional: ['verify'], options: { proof: '/tmp/proof.bin', commitment: '0xdeadbeef' } },
      ctx
    );
    expect(mockClient.proof.verify).toHaveBeenCalledWith({
      proof: expect.any(Buffer),
      commitment: '0xdeadbeef',
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('verified successfully'));
    writeSpy.mockRestore();
  });

  it('verify subcommand requires --proof and --commitment', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(proofCommand.action(
      { command: 'proof', positional: ['verify'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--proof'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('verify subcommand rejects missing proof file', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    vi.mocked(fs.existsSync).mockReturnValue(false);
    await expect(proofCommand.action(
      { command: 'proof', positional: ['verify'], options: { proof: '/tmp/missing.bin', commitment: '0xdead' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('not found'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('batch verify works with --batch flag', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const batchData = JSON.stringify([
      { proof: 'base64proof1', commitment: '0xaaa' },
      { proof: 'base64proof2', commitment: '0xbbb' },
    ]);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockImplementation((p: any, enc?: any) => {
      if (enc === 'utf-8') return batchData;
      return Buffer.from('fake');
    });
    await proofCommand.action(
      { command: 'proof', positional: ['verify'], options: { batch: '/tmp/batch.json' } },
      ctx
    );
    expect(mockClient.proof.batchVerify).toHaveBeenCalledWith({
      proofs: expect.any(Array),
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Batch Verification'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Passed'));
    writeSpy.mockRestore();
  });

  it('submit subcommand submits a proof', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(Buffer.from('proof-bytes'));
    await proofCommand.action(
      { command: 'proof', positional: ['submit'], options: { provider: 'prov001', proof: '/tmp/proof.bin' } },
      ctx
    );
    expect(mockClient.proof.submit).toHaveBeenCalledWith({
      providerId: 'prov001',
      proof: expect.any(Buffer),
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('submitted successfully'));
    writeSpy.mockRestore();
  });

  it('list subcommand lists proofs', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await proofCommand.action(
      { command: 'proof', positional: ['list'], options: { provider: 'prov001' } },
      ctx
    );
    expect(mockClient.proof.list).toHaveBeenCalledWith({ providerId: 'prov001', status: 'all' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('proof001'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('VERIFIED'));
    writeSpy.mockRestore();
  });

  it('list subcommand shows table format when --format table', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await proofCommand.action(
      { command: 'proof', positional: ['list'], options: { provider: 'prov001', format: 'table' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Proof ID'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('PENDING'));
    writeSpy.mockRestore();
  });

  it('status subcommand shows proof status', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await proofCommand.action(
      { command: 'proof', positional: ['status'], options: { proof_id: 'proof001' } },
      ctx
    );
    expect(mockClient.proof.status).toHaveBeenCalledWith('proof001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Proof Status'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('tx001'));
    writeSpy.mockRestore();
  });

  it('anchor subcommand anchors proof on chain', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await proofCommand.action(
      { command: 'proof', positional: ['anchor'], options: { proof_id: 'proof001' } },
      ctx
    );
    expect(mockClient.proof.anchor).toHaveBeenCalledWith('proof001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('anchored'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('987654'));
    writeSpy.mockRestore();
  });

  it('trust subcommand shows provider trust breakdown', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await proofCommand.action(
      { command: 'proof', positional: ['trust'], options: { provider: 'prov001' } },
      ctx
    );
    expect(mockClient.proof.trust).toHaveBeenCalledWith('prov001');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Trust Score'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('TEE'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('ZK'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('85.5'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on verify', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(Buffer.from('fake-proof'));
    await proofCommand.action(
      { command: 'proof', positional: ['verify'], options: { proof: '/tmp/p', commitment: '0xdead', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"valid"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"commitment"'));
    writeSpy.mockRestore();
  });

  it('rejects unknown subcommands', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(proofCommand.action(
      { command: 'proof', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });
});
