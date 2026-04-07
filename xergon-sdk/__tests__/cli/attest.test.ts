/**
 * Tests for the attest CLI command.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';

vi.mock('node:fs', () => ({
  default: {
    existsSync: vi.fn().mockReturnValue(true),
    readFileSync: vi.fn().mockReturnValue(Buffer.from('fake-attestation-report')),
    writeFileSync: vi.fn(),
  },
  existsSync: vi.fn().mockReturnValue(true),
  readFileSync: vi.fn().mockReturnValue(Buffer.from('fake-attestation-report')),
  writeFileSync: vi.fn(),
}));

import * as fs from 'node:fs';

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    attest: {
      submit: vi.fn().mockResolvedValue({
        providerId: 'prov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        teeType: 'sgx',
        status: 'submitted',
        attestationId: 'attest001abc123def456abc123def456abc123def456abc123def456abc123def45',
        submittedAt: '2026-04-06T00:00:00Z',
        expiresAt: '2026-05-06T00:00:00Z',
      }),
      verify: vi.fn().mockResolvedValue({
        providerId: 'prov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        valid: true,
        status: 'valid',
        verifiedAt: '2026-04-06T12:00:00Z',
        details: 'SGX attestation quote verified successfully. MRENCLAVE matches trusted enclave.',
      }),
      status: vi.fn().mockResolvedValue({
        providerId: 'prov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        teeType: 'sgx',
        mrenclave: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
        status: 'valid',
        lastAttested: '2026-04-05T10:00:00Z',
        expiresAt: '2026-05-05T10:00:00Z',
        reportHash: '0xdeadbeef1234567890abcdef1234567890abcdef1234567890abcdef1234567890',
      }),
      providers: vi.fn().mockResolvedValue([
        {
          providerId: 'prov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
          teeType: 'sgx',
          mrenclave: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
          status: 'valid',
          lastAttested: '2026-04-05T10:00:00Z',
          expiresAt: '2026-05-05T10:00:00Z',
        },
        {
          providerId: 'prov002def456abc123def456abc123def456abc123def456abc123def456abc123def4',
          teeType: 'sev',
          mrenclave: 'f1e2d3c4b5a6f1e2d3c4b5a6f1e2d3c4b5a6f1e2d3c4b5a6f1e2d3c4b5a6f1e2',
          status: 'expiring',
          lastAttested: '2026-03-01T10:00:00Z',
          expiresAt: '2026-04-10T10:00:00Z',
        },
        {
          providerId: 'prov003ghi789abc123def456abc123def456abc123def456abc123def456abc123def4',
          teeType: 'tdx',
          mrenclave: '11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff11',
          status: 'expired',
          lastAttested: '2026-01-01T10:00:00Z',
          expiresAt: '2026-02-01T10:00:00Z',
        },
      ]),
      renew: vi.fn().mockResolvedValue({
        providerId: 'prov001abc123def456abc123def456abc123def456abc123def456abc123def456abcd',
        oldExpiresAt: '2026-04-10T10:00:00Z',
        newExpiresAt: '2026-05-10T10:00:00Z',
        renewedAt: '2026-04-06T12:00:00Z',
        status: 'renewed',
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

describe('Attest Command', () => {
  let attestCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/attest');
    attestCommand = mod.attestCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(attestCommand.name).toBe('attest');
    expect(attestCommand.aliases).toContain('attestation');
    expect(attestCommand.aliases).toContain('tee');
  });

  it('has all expected options', () => {
    const optionNames = attestCommand.options.map(o => o.name);
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('provider');
    expect(optionNames).toContain('tee_type');
    expect(optionNames).toContain('report');
    expect(optionNames).toContain('status');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(attestCommand.action(
      { command: 'attest', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('submit|verify|status|providers|types|renew'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('submit subcommand submits attestation', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(Buffer.from('fake-attestation-report'));
    await attestCommand.action(
      { command: 'attest', positional: ['submit'], options: { provider: 'prov001', tee_type: 'sgx', report: '/tmp/report.bin' } },
      ctx
    );
    expect(mockClient.attest.submit).toHaveBeenCalledWith({
      providerId: 'prov001',
      teeType: 'sgx',
      report: expect.any(Buffer),
    });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('submitted successfully'));
    writeSpy.mockRestore();
  });

  it('submit subcommand requires --provider and --tee-type', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(attestCommand.action(
      { command: 'attest', positional: ['submit'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--provider'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('submit subcommand rejects missing report file', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    vi.mocked(fs.existsSync).mockReturnValue(false);
    await expect(attestCommand.action(
      { command: 'attest', positional: ['submit'], options: { provider: 'prov001', tee_type: 'sgx', report: '/tmp/missing.bin' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('not found'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('verify subcommand verifies attestation', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await attestCommand.action(
      { command: 'attest', positional: ['verify'], options: { provider: 'prov001' } },
      ctx
    );
    expect(mockClient.attest.verify).toHaveBeenCalledWith({ providerId: 'prov001' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('VALID'));
    writeSpy.mockRestore();
  });

  it('verify subcommand requires --provider', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(attestCommand.action(
      { command: 'attest', positional: ['verify'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--provider'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('status subcommand shows attestation details', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await attestCommand.action(
      { command: 'attest', positional: ['status'], options: { provider: 'prov001' } },
      ctx
    );
    expect(mockClient.attest.status).toHaveBeenCalledWith({ providerId: 'prov001' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('SGX'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('M R E N C L A V E'));
    writeSpy.mockRestore();
  });

  it('providers subcommand lists attested providers', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await attestCommand.action(
      { command: 'attest', positional: ['providers'], options: {} },
      ctx
    );
    expect(mockClient.attest.providers).toHaveBeenCalledWith({ teeType: 'all', status: 'all' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('3 attested provider'));
    writeSpy.mockRestore();
  });

  it('types subcommand shows supported TEE types', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await attestCommand.action(
      { command: 'attest', positional: ['types'], options: {} },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('SGX'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('SEV'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('TDX'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Software'));
    writeSpy.mockRestore();
  });

  it('renew subcommand renews attestation', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await attestCommand.action(
      { command: 'attest', positional: ['renew'], options: { provider: 'prov001' } },
      ctx
    );
    expect(mockClient.attest.renew).toHaveBeenCalledWith({ providerId: 'prov001' });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('renewed successfully'));
    writeSpy.mockRestore();
  });

  it('unknown subcommand shows error', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(attestCommand.action(
      { command: 'attest', positional: ['unknown'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('submit outputs JSON when --json flag is set', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(Buffer.from('fake-attestation-report'));
    await attestCommand.action(
      { command: 'attest', positional: ['submit'], options: { provider: 'prov001', tee_type: 'sgx', report: '/tmp/report.bin', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"attestationId"'));
    writeSpy.mockRestore();
  });

  it('verify outputs JSON when --json flag is set', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await attestCommand.action(
      { command: 'attest', positional: ['verify'], options: { provider: 'prov001', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"valid"'));
    writeSpy.mockRestore();
  });

  it('handles client not available error', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    const noClientCtx = createMockContext({ attest: undefined });
    await expect(attestCommand.action(
      { command: 'attest', positional: ['verify'], options: { provider: 'prov001' } },
      noClientCtx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('not available'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });
});
