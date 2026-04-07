/**
 * Tests for CLI command: test
 */

import { describe, it, expect, vi } from 'vitest';
import {
  statusIcon,
  statusColor,
  formatDuration,
  formatPassRate,
  TestService,
  testAction,
  testCommand,
  type TestStatus,
} from './test';

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

describe('test command', () => {
  describe('statusIcon', () => {
    it('returns correct icons for each status', () => {
      expect(statusIcon('pass')).toBe('+');
      expect(statusIcon('fail')).toBe('X');
      expect(statusIcon('skip')).toBe('-');
      expect(statusIcon('running')).toBe('*');
      expect(statusIcon('pending')).toBe('?');
    });
  });

  describe('statusColor', () => {
    it('returns green for pass', () => {
      expect(statusColor('pass')).toBe('green');
    });
    it('returns red for fail', () => {
      expect(statusColor('fail')).toBe('red');
    });
    it('returns yellow for skip', () => {
      expect(statusColor('skip')).toBe('yellow');
    });
    it('returns cyan for running', () => {
      expect(statusColor('running')).toBe('cyan');
    });
    it('returns dim for unknown', () => {
      expect(statusColor('pending' as TestStatus)).toBe('dim');
    });
  });

  describe('formatDuration', () => {
    it('formats milliseconds', () => {
      expect(formatDuration(500)).toBe('500ms');
    });
    it('formats seconds', () => {
      expect(formatDuration(2500)).toBe('2.5s');
    });
    it('formats minutes and seconds', () => {
      expect(formatDuration(90000)).toBe('1m 30s');
    });
  });

  describe('formatPassRate', () => {
    it('formats pass rate with one decimal', () => {
      expect(formatPassRate(95.5)).toBe('95.5%');
    });
    it('handles zero', () => {
      expect(formatPassRate(0)).toBe('0.0%');
    });
    it('handles 100', () => {
      expect(formatPassRate(100)).toBe('100.0%');
    });
  });

  describe('testCommand', () => {
    it('has correct name', () => {
      expect(testCommand.name).toBe('test');
    });
    it('has description', () => {
      expect(testCommand.description).toContain('integration test');
    });
    it('has aliases', () => {
      expect(testCommand.aliases).toContain('tests');
      expect(testCommand.aliases).toContain('spec');
    });
    it('has options', () => {
      expect(testCommand.options.length).toBeGreaterThan(0);
      expect(testCommand.options.some(o => o.name === 'json')).toBe(true);
      expect(testCommand.options.some(o => o.name === 'timeout')).toBe(true);
      expect(testCommand.options.some(o => o.name === 'parallel')).toBe(true);
    });
  });

  describe('testAction dispatch', () => {
    const mockOutput = createMockOutput();
    const mockCtx: any = {
      client: null,
      config: { baseUrl: 'https://test.xergon.gg' } as any,
      output: mockOutput as any,
    };

    it('shows usage when no subcommand provided', async () => {
      const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
      await expect(
        testAction({ command: 'test', positional: [], options: {} }, mockCtx)
      ).rejects.toThrow('exit');
      expect(mockOutput.writeError).toHaveBeenCalled();
      exitSpy.mockRestore();
    });

    it('shows error for unknown subcommand', async () => {
      const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
      await expect(
        testAction({ command: 'test', positional: ['foobar'], options: {} }, mockCtx)
      ).rejects.toThrow('exit');
      expect(mockOutput.writeError).toHaveBeenCalledWith(expect.stringContaining('foobar'));
      exitSpy.mockRestore();
    });
  });

  describe('TestService', () => {
    it('listSuites returns suite info', async () => {
      const service = new TestService('https://nonexistent.local:1');
      const suites = await service.listSuites();
      expect(suites.length).toBeGreaterThan(0);
      expect(suites[0]).toHaveProperty('name');
      expect(suites[0]).toHaveProperty('description');
      expect(suites[0]).toHaveProperty('testCount');
    });

    it('runSuite returns a valid result', async () => {
      const service = new TestService('https://nonexistent.local:1');
      const result = await service.runSuite('contract', 5000, false);
      expect(result).toHaveProperty('runId');
      expect(result).toHaveProperty('totalTests');
      expect(result).toHaveProperty('totalPass');
      expect(result).toHaveProperty('totalFail');
      expect(result).toHaveProperty('totalSkip');
      expect(result.totalTests).toBeGreaterThan(0);
    });

    it('getHistory returns history items', async () => {
      const service = new TestService('https://nonexistent.local:1');
      const history = await service.getHistory(5);
      expect(history.length).toBeLessThanOrEqual(5);
      expect(history[0]).toHaveProperty('runId');
      expect(history[0]).toHaveProperty('passRate');
    });

    it('verifyTransaction returns verification result', async () => {
      const service = new TestService('https://nonexistent.local:1');
      const result = await service.verifyTransaction('test-tx-id');
      expect(result).toHaveProperty('txId', 'test-tx-id');
      expect(result).toHaveProperty('valid');
      expect(result).toHaveProperty('confirmed');
    });

    it('probeProvider handles unreachable provider', async () => {
      const service = new TestService('https://nonexistent.local:1');
      const result = await service.probeProvider('https://nonexistent.invalid:1', 2000);
      expect(result.reachable).toBe(false);
      expect(result.error).toBeDefined();
    });
  });
});
