/**
 * Tests for the benchmark CLI command.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { Command, CLIContext, CLIConfig } from '../../src/cli/mod';
import { OutputFormatter } from '../../src/cli/mod';
// Note: fs.writeFileSync is not spied directly in ESM; the mock client handles results

// ── Mock helpers ───────────────────────────────────────────────────

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    benchmark: {
      run: vi.fn().mockResolvedValue({
        benchmarkId: 'bench001abc123def456abc123def456abc123def456abc123def456abc123def456ab',
        model: 'llama-3.3-70b',
        config: { model: 'llama-3.3-70b', requests: 100, concurrency: 1 },
        latency: {
          p50: 150,
          p75: 280,
          p90: 450,
          p95: 620,
          p99: 1200,
          mean: 200,
          median: 150,
          min: 80,
          max: 2500,
          stdDev: 180,
        },
        throughput: {
          requestsPerSecond: 12.5,
          tokensPerSecond: 1500,
          totalRequests: 100,
          totalTokens: 12000,
          totalDuration: 8000,
        },
        errorRate: 0.5,
        errorCount: 0,
        timestamp: '2026-04-06T00:00:00Z',
        duration: 8000,
      }),
      compare: vi.fn().mockResolvedValue({
        modelA: 'llama-3.3-70b',
        modelB: 'mixtral-8x7b',
        metrics: ['latency', 'throughput', 'accuracy'],
        comparison: [
          {
            name: 'P50 Latency',
            modelAValue: 150,
            modelBValue: 200,
            unit: 'ms',
            difference: 50,
            differencePercent: 25.0,
            winner: 'modelA',
          },
          {
            name: 'Throughput',
            modelAValue: 12.5,
            modelBValue: 10.2,
            unit: 'req/s',
            difference: 2.3,
            differencePercent: 18.4,
            winner: 'modelA',
          },
          {
            name: 'Accuracy (MMLU)',
            modelAValue: 82.5,
            modelBValue: 79.1,
            unit: '%',
            difference: 3.4,
            differencePercent: 4.1,
            winner: 'modelA',
          },
        ],
        winner: 'modelA',
      }),
      history: vi.fn().mockResolvedValue([
        {
          benchmarkId: 'bench001abc123def456abc123def456abc123def456abc123def456abc123def456ab',
          model: 'llama-3.3-70b',
          requestsPerSecond: 12.5,
          p50Latency: 150,
          p99Latency: 1200,
          errorRate: 0.5,
          timestamp: '2026-04-06T00:00:00Z',
        },
        {
          benchmarkId: 'bench002def456abc123def456abc123def456abc123def456abc123def456abc123def4',
          model: 'llama-3.3-70b',
          requestsPerSecond: 11.8,
          p50Latency: 160,
          p99Latency: 1300,
          errorRate: 0.8,
          timestamp: '2026-04-05T00:00:00Z',
        },
      ]),
      exportResults: vi.fn().mockResolvedValue([
        {
          benchmarkId: 'bench001abc123def456abc123def456abc123def456abc123def456abc123def456ab',
          model: 'llama-3.3-70b',
          latency: { p50: 150, p95: 620, p99: 1200 },
          throughput: { requestsPerSecond: 12.5, tokensPerSecond: 1500 },
          errorRate: 0.5,
          timestamp: '2026-04-06T00:00:00Z',
        },
      ]),
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

describe('Benchmark Command', () => {
  let benchmarkCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;
  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/benchmark');
    benchmarkCommand = mod.benchmarkCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(benchmarkCommand.name).toBe('benchmark');
    expect(benchmarkCommand.aliases).toContain('bench');
    expect(benchmarkCommand.aliases).toContain('perf');
  });

  it('has all expected options', () => {
    const optionNames = benchmarkCommand.options.map(o => o.name);
    expect(optionNames).toContain('json');
    expect(optionNames).toContain('format');
    expect(optionNames).toContain('model');
    expect(optionNames).toContain('requests');
    expect(optionNames).toContain('concurrency');
    expect(optionNames).toContain('prompt_file');
    expect(optionNames).toContain('suite');
    expect(optionNames).toContain('warmup');
    expect(optionNames).toContain('timeout');
    expect(optionNames).toContain('model_a');
    expect(optionNames).toContain('model_b');
    expect(optionNames).toContain('metrics');
    expect(optionNames).toContain('output');
    expect(optionNames).toContain('last');
  });

  it('shows usage when no subcommand given', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(benchmarkCommand.action(
      { command: 'benchmark', positional: [], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('run|compare|history|export'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('run subcommand runs a benchmark', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'llama-3.3-70b', requests: 100, concurrency: 1 } },
      ctx
    );
    expect(mockClient.benchmark.run).toHaveBeenCalledWith(
      expect.objectContaining({ model: 'llama-3.3-70b', requests: 100, concurrency: 1 })
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Benchmark Results'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Latency Distribution'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Throughput'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Benchmark completed'));
    writeSpy.mockRestore();
  });

  it('run subcommand requires --model flag', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('--model'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('run subcommand rejects invalid suite', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'test', suite: 'invalid' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('mmlu, humaneval, gsm8k, custom'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('run subcommand rejects both prompt-file and suite', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'test', prompt_file: 'p.txt', suite: 'mmlu' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Cannot specify both'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('run subcommand shows latency percentiles in output', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'llama-3.3-70b' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('150.0ms'));  // P50
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('620.0ms'));  // P95
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('1.20s'));    // P99 (auto-formatted to seconds)
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('P50 (Median)'));
    writeSpy.mockRestore();
  });

  it('run subcommand shows throughput and error rate', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'llama-3.3-70b' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('12.5'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('1,500'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('0.50%'));
    writeSpy.mockRestore();
  });

  it('compare subcommand compares two models', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['compare'], options: { model_a: 'llama-3.3-70b', model_b: 'mixtral-8x7b' } },
      ctx
    );
    expect(mockClient.benchmark.compare).toHaveBeenCalledWith(
      expect.objectContaining({ modelA: 'llama-3.3-70b', modelB: 'mixtral-8x7b' })
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Benchmark Comparison'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Overall winner'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('llama-3.3-70b'));
    writeSpy.mockRestore();
  });

  it('history subcommand shows benchmark history', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['history'], options: { model: 'llama-3.3-70b', last: 10 } },
      ctx
    );
    expect(mockClient.benchmark.history).toHaveBeenCalledWith({ model: 'llama-3.3-70b', last: 10 });
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Benchmark History'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('12.5'));
    writeSpy.mockRestore();
  });

  it('history subcommand with no results shows info message', async () => {
    mockClient.benchmark.history.mockResolvedValueOnce([]);
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['history'], options: { model: 'nonexistent' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('No benchmark history'));
    writeSpy.mockRestore();
  });

  it('export subcommand exports benchmark results as JSON', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['export'], options: { model: 'llama-3.3-70b', format: 'json', output: '/tmp/results.json' } },
      ctx
    );
    expect(mockClient.benchmark.exportResults).toHaveBeenCalledWith(
      expect.objectContaining({ model: 'llama-3.3-70b', format: 'json' })
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Exported 1 benchmark results'));
    writeSpy.mockRestore();
  });

  it('export subcommand exports benchmark results as CSV', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['export'], options: { model: 'llama-3.3-70b', format: 'csv', output: '/tmp/results.csv' } },
      ctx
    );
    expect(mockClient.benchmark.exportResults).toHaveBeenCalledWith(
      expect.objectContaining({ model: 'llama-3.3-70b', format: 'csv' })
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Exported 1 benchmark results'));
    writeSpy.mockRestore();
  });

  it('export subcommand rejects invalid format', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(benchmarkCommand.action(
      { command: 'benchmark', positional: ['export'], options: { model: 'test', format: 'xml', output: '/tmp/out.xml' } },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('csv, json'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on run', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'llama-3.3-70b', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"benchmarkId"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"latency"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"throughput"'));
    writeSpy.mockRestore();
  });

  it('outputs JSON when --json flag is set on compare', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['compare'], options: { model_a: 'llama-3.3-70b', model_b: 'mixtral-8x7b', json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"modelA"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"modelB"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"comparison"'));
    writeSpy.mockRestore();
  });

  it('rejects unknown subcommands', async () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(benchmarkCommand.action(
      { command: 'benchmark', positional: ['foobar'], options: {} },
      ctx
    )).rejects.toThrow('exit');
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('Unknown subcommand'));
    spy.mockRestore();
    exitSpy.mockRestore();
  });

  it('run subcommand shows table format when --format table', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['run'], options: { model: 'llama-3.3-70b', format: 'table' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Metric'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Value'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('P50 (Median)'));
    writeSpy.mockRestore();
  });

  it('compare subcommand shows table format when --format table', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await benchmarkCommand.action(
      { command: 'benchmark', positional: ['compare'], options: { model_a: 'llama-3.3-70b', model_b: 'mixtral-8x7b', format: 'table' } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Metric'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Winner'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('P50 Latency'));
    writeSpy.mockRestore();
  });
});
