/**
 * Tests for CLI command: settlement
 */

import { describe, it, expect, vi } from 'vitest';
import {
  settlementStatusColor,
  disputeStatusColor,
  formatErgAmount,
  explorerUrl,
  SettlementService,
  settlementAction,
  settlementCommand,
} from './settlement';

// Mock output formatter
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

describe('settlement command', () => {
  describe('settlementStatusColor', () => {
    it('returns green for confirmed', () => {
      expect(settlementStatusColor('confirmed')).toBe('green');
    });
    it('returns yellow for pending', () => {
      expect(settlementStatusColor('pending')).toBe('yellow');
    });
    it('returns red for failed', () => {
      expect(settlementStatusColor('failed')).toBe('red');
    });
    it('returns cyan for disputed', () => {
      expect(settlementStatusColor('disputed')).toBe('cyan');
    });
    it('returns dim for refunded', () => {
      expect(settlementStatusColor('refunded')).toBe('dim');
    });
  });

  describe('disputeStatusColor', () => {
    it('returns green for resolved', () => {
      expect(disputeStatusColor('resolved')).toBe('green');
    });
    it('returns yellow for open', () => {
      expect(disputeStatusColor('open')).toBe('yellow');
    });
    it('returns red for rejected', () => {
      expect(disputeStatusColor('rejected')).toBe('red');
    });
    it('returns cyan for escalated', () => {
      expect(disputeStatusColor('escalated')).toBe('cyan');
    });
  });

  describe('formatErgAmount', () => {
    it('passes through already formatted ERG amounts', () => {
      expect(formatErgAmount('12.5 ERG')).toBe('12.5 ERG');
    });
    it('converts nanoERG to ERG', () => {
      expect(formatErgAmount('12500000000')).toContain('ERG');
      expect(formatErgAmount('12500000000')).toContain('12.5');
    });
  });

  describe('explorerUrl', () => {
    it('generates correct explorer URL', () => {
      const url = explorerUrl('abc123');
      expect(url).toContain('explorer.ergoplatform.com');
      expect(url).toContain('abc123');
    });
  });

  describe('settlementCommand', () => {
    it('has correct name', () => {
      expect(settlementCommand.name).toBe('settlement');
    });
    it('has description', () => {
      expect(settlementCommand.description).toContain('settlement');
    });
    it('has aliases', () => {
      expect(settlementCommand.aliases).toContain('settle');
      expect(settlementCommand.aliases).toContain('pay');
    });
    it('has options', () => {
      expect(settlementCommand.options.length).toBeGreaterThan(0);
      expect(settlementCommand.options.some(o => o.name === 'json')).toBe(true);
      expect(settlementCommand.options.some(o => o.name === 'format')).toBe(true);
      expect(settlementCommand.options.some(o => o.name === 'status')).toBe(true);
      expect(settlementCommand.options.some(o => o.name === 'period')).toBe(true);
    });
  });

  describe('settlementAction dispatch', () => {
    const mockOutput = createMockOutput();
    const mockCtx: any = {
      client: null,
      config: { baseUrl: 'https://test.xergon.gg' } as any,
      output: mockOutput as any,
    };

    it('shows usage when no subcommand provided', async () => {
      const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
      await expect(
        settlementAction({ command: 'settlement', positional: [], options: {} }, mockCtx)
      ).rejects.toThrow('exit');
      expect(mockOutput.writeError).toHaveBeenCalled();
      exitSpy.mockRestore();
    });

    it('shows error for unknown subcommand', async () => {
      const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
      await expect(
        settlementAction({ command: 'settlement', positional: ['bogus'], options: {} }, mockCtx)
      ).rejects.toThrow('exit');
      expect(mockOutput.writeError).toHaveBeenCalledWith(expect.stringContaining('bogus'));
      exitSpy.mockRestore();
    });
  });

  describe('SettlementService', () => {
    it('getStatus returns status info', async () => {
      const service = new SettlementService('https://nonexistent.local:1');
      const status = await service.getStatus();
      expect(status).toHaveProperty('pendingCount');
      expect(status).toHaveProperty('network');
      expect(status.network).toBe('mainnet');
    });

    it('getHistory returns transactions', async () => {
      const service = new SettlementService('https://nonexistent.local:1');
      const history = await service.getHistory({ last: 5 });
      expect(history.length).toBeLessThanOrEqual(5);
      expect(history[0]).toHaveProperty('txId');
      expect(history[0]).toHaveProperty('amount');
      expect(history[0]).toHaveProperty('status');
    });

    it('verifyTransaction returns verification result', async () => {
      const service = new SettlementService('https://nonexistent.local:1');
      const result = await service.verifyTransaction('test-tx-123');
      expect(result).toHaveProperty('txId', 'test-tx-123');
      expect(result).toHaveProperty('valid');
      expect(result).toHaveProperty('amount');
    });

    it('openDispute returns dispute result', async () => {
      const service = new SettlementService('https://nonexistent.local:1');
      const result = await service.openDispute('req-123', 'incorrect_result');
      expect(result).toHaveProperty('disputeId');
      expect(result).toHaveProperty('requestId', 'req-123');
      expect(result.status).toBe('open');
    });

    it('resolveDispute returns resolve result', async () => {
      const service = new SettlementService('https://nonexistent.local:1');
      const result = await service.resolveDispute('disp-123', 'refunded');
      expect(result).toHaveProperty('disputeId', 'disp-123');
      expect(result.status).toBe('resolved');
    });

    it('getSummary returns summary stats', async () => {
      const service = new SettlementService('https://nonexistent.local:1');
      const summary = await service.getSummary('30d');
      expect(summary).toHaveProperty('totalSettled');
      expect(summary).toHaveProperty('totalAmount');
      expect(summary).toHaveProperty('successRate');
      expect(summary.period).toBe('30d');
    });
  });
});
