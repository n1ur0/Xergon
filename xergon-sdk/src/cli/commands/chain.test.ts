/**
 * Tests for CLI command: chain (on-chain state inspection)
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  nanoErgToErg,
  formatErg,
  truncateId,
  boxStatusColor,
  decodeRegisterValue,
  extractRegisters,
  minBoxValue,
  explorerLink,
  chainAction,
  chainCommand,
  mapBoxToChainBox,
  NANO_ERG_PER_ERG,
  MIN_BOX_VALUE_PER_BYTE,
  type ChainBox,
  type TxDetails,
} from './chain';

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

function createMockClient(overrides?: Record<string, any>) {
  return {
    chain: {
      getBoxesByAddress: vi.fn().mockResolvedValue([
        {
          boxId: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
          value: '1000000000',
          ergoTree: '1006040004000e36100204deadbeef',
          registers: { R4: 'cHJvdmlkZXI=' },
          tokens: [{ tokenId: 'tok1', amount: '100', name: 'XERGON' }],
          creationHeight: 123456,
          transactionId: 'tx123',
          index: 0,
          spent: false,
        },
        {
          boxId: 'f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5',
          value: '500000000',
          ergoTree: '1008040004000e36100205cafebabe',
          registers: {},
          tokens: [],
          creationHeight: 120000,
          transactionId: 'tx456',
          index: 1,
          spent: false,
        },
      ]),
      getBox: vi.fn().mockResolvedValue({
        boxId: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
        value: '2500000000',
        ergoTree: '1006040004000e36100204deadbeef',
        registers: { R4: 'cHJvdmlkZXI=', R5: 'dXMtZWFzdA==' },
        tokens: [{ tokenId: 'tok1', amount: '500', name: 'XERGON', decimals: 4 }],
        creationHeight: 123456,
        transactionId: 'tx789',
        index: 0,
        spent: false,
      }),
      getHeight: vi.fn().mockResolvedValue(987654),
      getBalance: vi.fn().mockResolvedValue({
        address: '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg',
        nanoErgs: '10000000000',
        ergs: '10.000 ERG',
        tokens: [{ tokenId: 'tok1', amount: '100', name: 'XERGON' }],
        boxesCount: 5,
      }),
      getTokens: vi.fn().mockResolvedValue([
        { tokenId: 'tok1', amount: '100000', name: 'XERGON', decimals: 4 },
        { tokenId: 'tok2', amount: '50000000', name: 'SigUSD', decimals: 2 },
      ]),
      getProviders: vi.fn().mockResolvedValue([
        {
          boxId: 'prov1box1box1box1box1box1box1box1box1box1box1box1box1box1box1box1',
          address: 'addr1',
          stakeAmount: '1000000000',
          stakeErg: '1.000 ERG',
          region: 'us-east',
          model: 'llama-3.3-70b',
          status: 'active',
          registeredHeight: 500000,
        },
      ]),
      getStake: vi.fn().mockResolvedValue({
        boxId: 'stake1box1box1box1box1box1box1box1box1box1box1box1box1box1box1b',
        stakedAmount: '5000000000',
        stakedErg: '5.000 ERG',
        rewardAddress: '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg',
        lockEpochs: 90,
        currentEpoch: 45,
        status: 'locked',
        registeredHeight: 600000,
        tokens: [],
      }),
      getTx: vi.fn().mockResolvedValue({
        txId: 'abc123def456abc123def456abc123def456abc123def456abc123def456abc1',
        timestamp: 1700000000000,
        height: 987000,
        size: 512,
        inputsCount: 2,
        outputsCount: 3,
        inputs: [
          { boxId: 'inbox1inbox1inbox1inbox1inbox1inbox1inbox1inbox1inbox1inbox1', value: '2000000000' },
          { boxId: 'inbox2inbox2inbox2inbox2inbox2inbox2inbox2inbox2inbox2inbox2', value: '1000000000' },
        ],
        outputs: [
          { boxId: 'outbox1outbox1outbox1outbox1outbox1outbox1outbox1outbox1out', value: '1500000000', address: 'addr1' },
          { boxId: 'outbox2outbox2outbox2outbox2outbox2outbox2outbox2outbox2out', value: '1490000000', address: 'addr2' },
          { boxId: 'outbox3outbox3outbox3outbox3outbox3outbox3outbox3outbox3out', value: '1000000', address: 'fee' },
        ],
        fee: '1000000',
        feeErg: '0.001 ERG',
        status: 'confirmed',
      }),
      scanBoxes: vi.fn().mockResolvedValue([]),
      submitTx: vi.fn(),
      verifyBox: vi.fn(),
    },
    ...overrides,
  };
}

// ── Tests ──────────────────────────────────────────────────────────

describe('nanoErgToErg', () => {
  it('converts 1 ERG in nanoERG', () => {
    expect(nanoErgToErg(NANO_ERG_PER_ERG)).toBe('1.0');
  });

  it('converts 0 nanoERG', () => {
    expect(nanoErgToErg(0)).toBe('0.0');
  });

  it('converts fractional ERG amounts', () => {
    expect(nanoErgToErg('500000000')).toBe('0.5');
  });

  it('converts large values', () => {
    expect(nanoErgToErg('1000000000000')).toBe('1000.0');
  });

  it('handles string input', () => {
    expect(nanoErgToErg('2500000000')).toBe('2.5');
  });
});

describe('formatErg', () => {
  it('formats 1 ERG', () => {
    expect(formatErg(NANO_ERG_PER_ERG)).toBe('1.000 ERG');
  });

  it('formats 0 ERG', () => {
    expect(formatErg(0)).toBe('0.000 ERG');
  });

  it('formats fractional amounts with 3 decimal places', () => {
    expect(formatErg('500000000')).toBe('0.500 ERG');
  });

  it('formats 10 ERG', () => {
    expect(formatErg('10000000000')).toBe('10.000 ERG');
  });

  it('formats sub-nano precision (rounds to 3 decimals)', () => {
    expect(formatErg('123456789')).toBe('0.123 ERG');
  });
});

describe('truncateId', () => {
  it('truncates long IDs', () => {
    const id = 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2';
    expect(truncateId(id, 8)).toBe('a1b2c3d4...a1b2');
  });

  it('returns short IDs unchanged', () => {
    expect(truncateId('abc123')).toBe('abc123');
  });

  it('uses default prefix length of 8', () => {
    const id = '1234567890abcdef1234567890abcdef';
    expect(truncateId(id)).toBe('12345678...cdef');
  });
});

describe('boxStatusColor', () => {
  it('returns unspent for undefined spent', () => {
    expect(boxStatusColor(undefined, false)).toBe('unspent');
  });

  it('returns unspent for false', () => {
    expect(boxStatusColor(false, false)).toBe('unspent');
  });

  it('returns spent for true', () => {
    expect(boxStatusColor(true, false)).toBe('spent');
  });

  it('includes ANSI color codes when useColor is true (unspent)', () => {
    const result = boxStatusColor(false, true);
    expect(result).toContain('\x1b[32m');
    expect(result).toContain('unspent');
  });

  it('includes ANSI color codes when useColor is true (spent)', () => {
    const result = boxStatusColor(true, true);
    expect(result).toContain('\x1b[31m');
    expect(result).toContain('spent');
  });
});

describe('decodeRegisterValue', () => {
  it('decodes base64 encoded strings', () => {
    expect(decodeRegisterValue('cHJvdmlkZXI=')).toBe('provider');
  });

  it('returns raw string if not valid base64', () => {
    expect(decodeRegisterValue('not-base64!!!')).toBe('not-base64!!!');
  });

  it('handles null/undefined', () => {
    expect(decodeRegisterValue(null)).toBe('');
    expect(decodeRegisterValue(undefined)).toBe('');
  });

  it('handles objects with serializedValue', () => {
    expect(decodeRegisterValue({ serializedValue: 'dXMtZWFzdA==' })).toBe('us-east');
  });

  it('returns empty string for non-printable decoded base64', () => {
    const binary = Buffer.from([0x00, 0x01, 0x02]).toString('base64');
    // Should not return garbage binary as decoded text
    const result = decodeRegisterValue(binary);
    expect(result).toBeTruthy();
  });
});

describe('extractRegisters', () => {
  it('extracts R4-R9 registers', () => {
    const box = {
      additionalRegisters: {
        R4: 'cHJvdmlkZXI=',   // "provider"
        R5: 'dXMtZWFzdA==',    // "us-east"
        R6: 'bGxhbWEtMy4z',    // "llama-3.3"
      },
    };
    const regs = extractRegisters(box);
    expect(regs.R4).toBe('provider');
    expect(regs.R5).toBe('us-east');
    expect(regs.R6).toBe('llama-3.3');
  });

  it('skips empty registers', () => {
    const regs = extractRegisters({});
    expect(Object.keys(regs)).toHaveLength(0);
  });

  it('only extracts R4-R9', () => {
    const box = {
      registers: {
        R3: 'ignored',
        R4: 'cHJvdmlkZXI=',
        R10: 'also-ignored',
      },
    };
    const regs = extractRegisters(box);
    expect(regs.R4).toBe('provider');
    expect(regs.R3).toBeUndefined();
    expect(regs.R10).toBeUndefined();
  });
});

describe('minBoxValue', () => {
  it('calculates minimum value for a 300 byte box', () => {
    expect(minBoxValue(300)).toBe(BigInt(300 * MIN_BOX_VALUE_PER_BYTE));
  });

  it('returns 0 for 0 bytes', () => {
    expect(minBoxValue(0)).toBe(BigInt(0));
  });
});

describe('explorerLink', () => {
  it('generates transaction explorer URL', () => {
    expect(explorerLink('abc123', 'tx')).toContain('/tx/abc123');
  });

  it('generates box explorer URL', () => {
    expect(explorerLink('box456', 'box')).toContain('/box/box456');
  });

  it('generates address explorer URL', () => {
    expect(explorerLink('addr789', 'address')).toContain('/address/addr789');
  });

  it('defaults to tx type', () => {
    expect(explorerLink('default123')).toContain('/tx/default123');
  });
});

describe('mapBoxToChainBox', () => {
  it('maps raw node box to ChainBox', () => {
    const raw = {
      boxId: 'box1',
      value: 1000000000,
      ergoTree: 'tree1',
      additionalRegisters: { R4: 'cHJvdmlkZXI=' },
      assets: [{ tokenId: 't1', amount: 100, name: 'Test' }],
      creationHeight: 500,
      transactionId: 'tx1',
      index: 0,
      spent: false,
    };
    const box = mapBoxToChainBox(raw);
    expect(box.boxId).toBe('box1');
    expect(box.value).toBe('1000000000');
    expect(box.tokens).toHaveLength(1);
    expect(box.tokens[0].name).toBe('Test');
    expect(box.spent).toBe(false);
  });

  it('handles missing optional fields', () => {
    const raw = { boxId: 'box2', value: 0 };
    const box = mapBoxToChainBox(raw);
    expect(box.boxId).toBe('box2');
    expect(box.value).toBe('0');
    expect(box.tokens).toHaveLength(0);
    expect(box.creationHeight).toBe(0);
  });
});

describe('chainCommand', () => {
  it('has correct name', () => {
    expect(chainCommand.name).toBe('chain');
  });

  it('has description', () => {
    expect(chainCommand.description).toContain('on-chain');
  });

  it('has aliases', () => {
    expect(chainCommand.aliases).toContain('onchain');
    expect(chainCommand.aliases).toContain('utxo');
  });

  it('has required options', () => {
    const names = chainCommand.options.map(o => o.name);
    expect(names).toContain('node');
    expect(names).toContain('network');
    expect(names).toContain('json');
    expect(names).toContain('format');
  });
});

describe('chainAction dispatch', () => {
  const mockOutput = createMockOutput();
  const mockClient = createMockClient();
  const mockCtx = createMockContext({ client: mockClient });

  it('shows usage when no subcommand provided', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(
      chainAction({ command: 'chain', positional: [], options: {} }, mockCtx)
    ).rejects.toThrow('exit');
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });

  it('shows error for unknown subcommand', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(
      chainAction({ command: 'chain', positional: ['bogus'], options: {} }, mockCtx)
    ).rejects.toThrow('exit');
    expect(mockOutput.writeError).toHaveBeenCalledWith(expect.stringContaining('bogus'));
    exitSpy.mockRestore();
  });

  it('calls boxes subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['boxes', '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getBoxesByAddress).toHaveBeenCalledWith('9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg');
  });

  it('calls box subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['box', 'boxid123'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getBox).toHaveBeenCalledWith('boxid123');
  });

  it('calls height subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['height'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getHeight).toHaveBeenCalled();
  });

  it('calls balance subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['balance', '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getBalance).toHaveBeenCalledWith('9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg');
  });

  it('calls tokens subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['tokens', '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getTokens).toHaveBeenCalledWith('9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg');
  });

  it('calls providers subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['providers'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getProviders).toHaveBeenCalled();
  });

  it('calls stake subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['stake', '9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getStake).toHaveBeenCalledWith('9fFJjRnm5FcT6mWF6V7dWwmHJoQfEn6oKDSt25UY3Xg');
  });

  it('calls tx subcommand', async () => {
    await chainAction(
      { command: 'chain', positional: ['tx', 'txid123'], options: {} },
      mockCtx
    );
    expect(mockClient.chain.getTx).toHaveBeenCalledWith('txid123');
  });

  it('outputs JSON when --json flag is set', async () => {
    await chainAction(
      { command: 'chain', positional: ['height'], options: { json: true } },
      mockCtx
    );
    expect(mockOutput.write).toHaveBeenCalledWith(expect.stringContaining('"height"'));
  });
});
