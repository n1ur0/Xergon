/**
 * CLI command: benchmark
 *
 * Performance profiling and benchmarking for the Xergon Network.
 * Run benchmarks, compare models, view history, and export results.
 *
 * Usage:
 *   xergon benchmark run --model MODEL --requests N --concurrency N --prompt-file PATH
 *   xergon benchmark run --model MODEL --suite mmlu|humaneval|gsm8k|custom
 *   xergon benchmark compare --model-a MODEL --model-b MODEL --metrics latency,throughput,accuracy
 *   xergon benchmark history --model MODEL --last N
 *   xergon benchmark export --model MODEL --format csv|json --output FILE
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';

// ── Types ──────────────────────────────────────────────────────────

interface BenchmarkConfig {
  model: string;
  requests: number;
  concurrency: number;
  promptFile?: string;
  suite?: BenchmarkSuite;
  warmup?: number;
  timeout?: number;
}

type BenchmarkSuite = 'mmlu' | 'humaneval' | 'gsm8k' | 'custom';

interface LatencyResult {
  p50: number;
  p75: number;
  p90: number;
  p95: number;
  p99: number;
  mean: number;
  median: number;
  min: number;
  max: number;
  stdDev: number;
}

interface ThroughputResult {
  requestsPerSecond: number;
  tokensPerSecond: number;
  totalRequests: number;
  totalTokens: number;
  totalDuration: number;
}

interface BenchmarkResult {
  benchmarkId: string;
  model: string;
  config: BenchmarkConfig;
  latency: LatencyResult;
  throughput: ThroughputResult;
  errorRate: number;
  errorCount: number;
  timestamp: string;
  duration: number;
}

interface BenchmarkComparison {
  modelA: string;
  modelB: string;
  metrics: string[];
  comparison: ComparisonMetric[];
  winner: string;
}

interface ComparisonMetric {
  name: string;
  modelAValue: number;
  modelBValue: number;
  unit: string;
  difference: number;
  differencePercent: number;
  winner: 'modelA' | 'modelB' | 'tie';
}

interface BenchmarkHistoryItem {
  benchmarkId: string;
  model: string;
  requestsPerSecond: number;
  p50Latency: number;
  p99Latency: number;
  errorRate: number;
  timestamp: string;
}

// ── Thresholds for color coding ────────────────────────────────────

const LATENCY_THRESHOLDS = {
  p50: { good: 200, warn: 500 },    // ms
  p95: { good: 500, warn: 1500 },   // ms
  p99: { good: 1000, warn: 3000 },  // ms
};

const THROUGHPUT_THRESHOLDS = {
  requestsPerSecond: { good: 10, warn: 3 },  // higher is better
  tokensPerSecond: { good: 1000, warn: 300 }, // higher is better
};

const ERROR_RATE_THRESHOLDS = {
  good: 1,    // percent
  warn: 5,    // percent
};

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function latencyColor(ms: number, percentile: 'p50' | 'p95' | 'p99'): 'green' | 'yellow' | 'red' {
  const thresholds = LATENCY_THRESHOLDS[percentile];
  if (ms <= thresholds.good) return 'green';
  if (ms <= thresholds.warn) return 'yellow';
  return 'red';
}

function throughputColor(value: number, metric: 'requestsPerSecond' | 'tokensPerSecond'): 'green' | 'yellow' | 'red' {
  const thresholds = THROUGHPUT_THRESHOLDS[metric];
  if (value >= thresholds.good) return 'green';
  if (value >= thresholds.warn) return 'yellow';
  return 'red';
}

function errorRateColor(rate: number): 'green' | 'yellow' | 'red' {
  if (rate <= ERROR_RATE_THRESHOLDS.good) return 'green';
  if (rate <= ERROR_RATE_THRESHOLDS.warn) return 'yellow';
  return 'red';
}

function colorValue(value: number, color: 'green' | 'yellow' | 'red', formatter?: (v: number) => string): string {
  const str = formatter ? formatter(value) : String(value);
  // Return raw string; the caller wraps with colorize
  return str;
}

function formatMs(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
  if (ms < 1000) return `${ms.toFixed(1)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}

function formatNumber(value: number): string {
  return value.toLocaleString('en-US', { maximumFractionDigits: 2 });
}

// ── Subcommand: run ───────────────────────────────────────────────

async function handleRun(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = args.options.model ? String(args.options.model) : undefined;
  const requests = args.options.requests ? Number(args.options.requests) : 100;
  const concurrency = args.options.concurrency ? Number(args.options.concurrency) : 1;
  const promptFile = args.options.prompt_file ? String(args.options.prompt_file) : undefined;
  const suite = args.options.suite ? String(args.options.suite) : undefined;
  const warmup = args.options.warmup ? Number(args.options.warmup) : 5;
  const timeout = args.options.timeout ? Number(args.options.timeout) : 120;

  if (!model) {
    ctx.output.writeError('Usage: xergon benchmark run --model <model> [--requests N] [--concurrency N] [--prompt-file PATH | --suite SUITE]');
    process.exit(1);
    return;
  }

  if (suite && !['mmlu', 'humaneval', 'gsm8k', 'custom'].includes(suite)) {
    ctx.output.writeError('Suite must be one of: mmlu, humaneval, gsm8k, custom');
    process.exit(1);
    return;
  }

  if (promptFile && suite) {
    ctx.output.writeError('Cannot specify both --prompt-file and --suite');
    process.exit(1);
    return;
  }

  const config: BenchmarkConfig = {
    model,
    requests,
    concurrency,
    promptFile,
    suite: suite as BenchmarkSuite | undefined,
    warmup,
    timeout,
  };

  const modeStr = suite ? `suite ${suite}` : promptFile ? `prompt file ${promptFile}` : `${requests} requests`;
  ctx.output.info(`Running benchmark for ${model} (${modeStr}, concurrency=${concurrency})...`);

  try {
    let result: BenchmarkResult;

    if (ctx.client?.benchmark?.run) {
      result = await ctx.client.benchmark.run(config);
    } else {
      throw new Error('Benchmark client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    // Formatted output
    ctx.output.write(ctx.output.colorize('Benchmark Results', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(50), 'dim'));

    // Summary line
    ctx.output.write(ctx.output.formatText({
      'Benchmark ID': result.benchmarkId,
      Model: result.model,
      'Total Duration': formatMs(result.duration),
      'Requests': `${result.throughput.totalRequests} (${formatNumber(result.throughput.requestsPerSecond)} req/s)`,
      Errors: `${result.errorCount} (${formatPercent(result.errorRate)})`,
    }));

    // Latency section
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Latency Distribution:', 'bold'));
    const latencyData = result.latency;
    const latencies = [
      { label: 'Min', value: latencyData.min, percentile: 'p50' as const },
      { label: 'P50 (Median)', value: latencyData.p50, percentile: 'p50' as const },
      { label: 'P75', value: latencyData.p75, percentile: 'p50' as const },
      { label: 'P90', value: latencyData.p90, percentile: 'p95' as const },
      { label: 'P95', value: latencyData.p95, percentile: 'p95' as const },
      { label: 'P99', value: latencyData.p99, percentile: 'p99' as const },
      { label: 'Max', value: latencyData.max, percentile: 'p99' as const },
      { label: 'Mean', value: latencyData.mean, percentile: 'p50' as const },
      { label: 'Std Dev', value: latencyData.stdDev, percentile: 'p50' as const },
    ];

    if (isTableFormat(args)) {
      const tableData = latencies.map(l => ({
        Metric: l.label,
        Value: formatMs(l.value),
      }));
      ctx.output.write(ctx.output.formatTable(tableData));
    } else {
      for (const l of latencies) {
        const color = latencyColor(l.value, l.percentile);
        ctx.output.write(
          `  ${l.label.padEnd(15)} ${ctx.output.colorize(formatMs(l.value).padStart(10), color)}`
        );
      }
    }

    // Throughput section
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Throughput:', 'bold'));
    const tpColor = throughputColor(result.throughput.requestsPerSecond, 'requestsPerSecond');
    const tokColor = throughputColor(result.throughput.tokensPerSecond, 'tokensPerSecond');
    ctx.output.write(
      `  Requests/sec:    ${ctx.output.colorize(formatNumber(result.throughput.requestsPerSecond).padStart(10), tpColor)}`
    );
    ctx.output.write(
      `  Tokens/sec:      ${ctx.output.colorize(formatNumber(result.throughput.tokensPerSecond).padStart(10), tokColor)}`
    );
    ctx.output.write(
      `  Total Requests:  ${String(result.throughput.totalRequests).padStart(10)}`
    );
    ctx.output.write(
      `  Total Tokens:    ${formatNumber(result.throughput.totalTokens).padStart(10)}`
    );

    // Error rate
    ctx.output.write('');
    const errColor = errorRateColor(result.errorRate);
    ctx.output.write(
      `  Error Rate:      ${ctx.output.colorize(formatPercent(result.errorRate).padStart(10), errColor)}`
    );

    ctx.output.success('Benchmark completed');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Benchmark failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: compare ───────────────────────────────────────────

async function handleCompare(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelA = args.options.model_a ? String(args.options.model_a) : undefined;
  const modelB = args.options.model_b ? String(args.options.model_b) : undefined;
  const metricsStr = args.options.metrics ? String(args.options.metrics) : 'latency,throughput,accuracy';

  if (!modelA || !modelB) {
    ctx.output.writeError('Usage: xergon benchmark compare --model-a <model> --model-b <model> [--metrics latency,throughput,accuracy]');
    process.exit(1);
    return;
  }

  const metrics = metricsStr.split(',').map(m => m.trim());

  ctx.output.info(`Comparing ${modelA} vs ${modelB}...`);

  try {
    let result: BenchmarkComparison;

    if (ctx.client?.benchmark?.compare) {
      result = await ctx.client.benchmark.compare({ modelA, modelB, metrics });
    } else {
      throw new Error('Benchmark client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Benchmark Comparison', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(50), 'dim'));
    ctx.output.write('');

    if (isTableFormat(args)) {
      const tableData = result.comparison.map(m => ({
        Metric: m.name,
        [modelA]: `${formatNumber(m.modelAValue)} ${m.unit}`,
        [modelB]: `${formatNumber(m.modelBValue)} ${m.unit}`,
        'Diff': `${m.differencePercent > 0 ? '+' : ''}${m.differencePercent.toFixed(1)}%`,
        Winner: m.winner === 'tie' ? 'Tie' : m.winner === 'modelA' ? modelA : modelB,
      }));
      ctx.output.write(ctx.output.formatTable(tableData));
    } else {
      for (const m of result.comparison) {
        const sign = m.differencePercent > 0 ? '+' : '';
        const winnerStr = m.winner === 'tie'
          ? ctx.output.colorize('Tie', 'dim')
          : m.winner === 'modelA'
            ? ctx.output.colorize(modelA, 'green')
            : ctx.output.colorize(modelB, 'green');

        ctx.output.write(`  ${m.name}`);
        ctx.output.write(
          `    ${modelA}: ${formatNumber(m.modelAValue)} ${m.unit}  vs  ` +
          `${modelB}: ${formatNumber(m.modelBValue)} ${m.unit}`
        );
        ctx.output.write(
          `    Difference: ${sign}${m.differencePercent.toFixed(1)}%  Winner: ${winnerStr}`
        );
        ctx.output.write('');
      }
    }

    // Overall winner
    const overallWinner = result.winner === 'modelA' ? modelA : modelB;
    if (result.winner !== 'tie') {
      ctx.output.success(`Overall winner: ${overallWinner}`);
    } else {
      ctx.output.info('Result: Tie');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Comparison failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: history ───────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = args.options.model ? String(args.options.model) : undefined;
  const last = args.options.last ? Number(args.options.last) : 10;

  try {
    let history: BenchmarkHistoryItem[];

    if (ctx.client?.benchmark?.history) {
      history = await ctx.client.benchmark.history({ model, last });
    } else {
      throw new Error('Benchmark client not available.');
    }

    if (history.length === 0) {
      ctx.output.info('No benchmark history found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(history, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = history.map(h => ({
        ID: h.benchmarkId.substring(0, 12) + '...',
        Model: h.model.length > 25 ? h.model.substring(0, 25) + '...' : h.model,
        'Req/s': formatNumber(h.requestsPerSecond),
        'P50': formatMs(h.p50Latency),
        'P99': formatMs(h.p99Latency),
        'Errors': formatPercent(h.errorRate),
        Date: h.timestamp ? new Date(h.timestamp).toISOString().slice(0, 19) : '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Benchmark History (${history.length} runs)`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Benchmark History for ${model || 'all models'} (${history.length} runs)`, 'bold'));
    ctx.output.write('');
    for (const h of history) {
      const rpsColor = throughputColor(h.requestsPerSecond, 'requestsPerSecond');
      const errColor = errorRateColor(h.errorRate);
      ctx.output.write(
        `  ${ctx.output.colorize(h.benchmarkId.substring(0, 16) + '...', 'cyan')}  ` +
        `${h.model}`
      );
      ctx.output.write(
        `    ${ctx.output.colorize(formatNumber(h.requestsPerSecond) + ' req/s', rpsColor)}  |  ` +
        `P50: ${formatMs(h.p50Latency)}  P99: ${formatMs(h.p99Latency)}  |  ` +
        `Errors: ${ctx.output.colorize(formatPercent(h.errorRate), errColor)}  |  ` +
        `${h.timestamp ? new Date(h.timestamp).toISOString().slice(0, 10) : '-'}`
      );
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get benchmark history: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: export ────────────────────────────────────────────

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = args.options.model ? String(args.options.model) : undefined;
  const format = args.options.format ? String(args.options.format) : 'json';
  const outputFile = args.options.output ? String(args.options.output) : undefined;

  if (!model) {
    ctx.output.writeError('Usage: xergon benchmark export --model <model> --format csv|json --output <file>');
    process.exit(1);
    return;
  }

  if (!['csv', 'json'].includes(format)) {
    ctx.output.writeError('Format must be one of: csv, json');
    process.exit(1);
    return;
  }

  if (!outputFile) {
    ctx.output.writeError('Usage: xergon benchmark export --model <model> --format csv|json --output <file>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Exporting benchmark results for ${model} to ${outputFile}...`);

  try {
    let data: BenchmarkResult[];

    if (ctx.client?.benchmark?.exportResults) {
      data = await ctx.client.benchmark.exportResults({ model, format: format as 'csv' | 'json' });
    } else {
      throw new Error('Benchmark client not available.');
    }

    let content: string;
    if (format === 'json') {
      content = JSON.stringify(data, null, 2);
    } else {
      // CSV format
      const headers = ['benchmarkId', 'model', 'p50', 'p95', 'p99', 'requestsPerSecond', 'tokensPerSecond', 'errorRate', 'timestamp'];
      const rows = data.map(d => [
        d.benchmarkId,
        d.model,
        String(d.latency.p50),
        String(d.latency.p95),
        String(d.latency.p99),
        String(d.throughput.requestsPerSecond),
        String(d.throughput.tokensPerSecond),
        formatPercent(d.errorRate),
        d.timestamp,
      ].join(','));
      content = headers.join(',') + '\n' + rows.join('\n');
    }

    fs.writeFileSync(outputFile, content, 'utf-8');

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({
        model,
        format,
        outputFile,
        records: data.length,
        status: 'exported',
      }, null, 2));
      return;
    }

    ctx.output.success(`Exported ${data.length} benchmark results to ${outputFile}`);
    ctx.output.write(`  Model:  ${model}`);
    ctx.output.write(`  Format: ${format}`);
    ctx.output.write(`  Size:   ${content.length} bytes`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Export failed: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function benchmarkAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon benchmark <run|compare|history|export> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  run       Run a performance benchmark');
    ctx.output.write('  compare   Compare two models on benchmark metrics');
    ctx.output.write('  history   View benchmark history for a model');
    ctx.output.write('  export    Export benchmark results to a file');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'run':
      await handleRun(args, ctx);
      break;
    case 'compare':
      await handleCompare(args, ctx);
      break;
    case 'history':
      await handleHistory(args, ctx);
      break;
    case 'export':
      await handleExport(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: run, compare, history, export');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const benchmarkOptions: CommandOption[] = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Output or export format: text, json, table, csv',
    required: false,
    type: 'string',
  },
  {
    name: 'model',
    short: '',
    long: '--model',
    description: 'Model name to benchmark or query',
    required: false,
    type: 'string',
  },
  {
    name: 'requests',
    short: '',
    long: '--requests',
    description: 'Number of requests for the benchmark (default: 100)',
    required: false,
    default: '100',
    type: 'number',
  },
  {
    name: 'concurrency',
    short: '',
    long: '--concurrency',
    description: 'Number of concurrent requests (default: 1)',
    required: false,
    default: '1',
    type: 'number',
  },
  {
    name: 'prompt_file',
    short: '',
    long: '--prompt-file',
    description: 'Path to prompt file for custom benchmark',
    required: false,
    type: 'string',
  },
  {
    name: 'suite',
    short: '',
    long: '--suite',
    description: 'Evaluation suite: mmlu, humaneval, gsm8k, or custom',
    required: false,
    type: 'string',
  },
  {
    name: 'warmup',
    short: '',
    long: '--warmup',
    description: 'Number of warmup requests (default: 5)',
    required: false,
    default: '5',
    type: 'number',
  },
  {
    name: 'timeout',
    short: '',
    long: '--timeout',
    description: 'Benchmark timeout in seconds (default: 120)',
    required: false,
    default: '120',
    type: 'number',
  },
  {
    name: 'model_a',
    short: '',
    long: '--model-a',
    description: 'First model for comparison',
    required: false,
    type: 'string',
  },
  {
    name: 'model_b',
    short: '',
    long: '--model-b',
    description: 'Second model for comparison',
    required: false,
    type: 'string',
  },
  {
    name: 'metrics',
    short: '',
    long: '--metrics',
    description: 'Comma-separated metrics for comparison: latency,throughput,accuracy',
    required: false,
    default: 'latency,throughput,accuracy',
    type: 'string',
  },
  {
    name: 'output',
    short: '',
    long: '--output',
    description: 'Output file path for export',
    required: false,
    type: 'string',
  },
  {
    name: 'last',
    short: '',
    long: '--last',
    description: 'Number of recent benchmarks to show in history (default: 10)',
    required: false,
    default: '10',
    type: 'number',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const benchmarkCommand: Command = {
  name: 'benchmark',
  description: 'Run performance benchmarks, compare models, and export results',
  aliases: ['bench', 'perf'],
  options: benchmarkOptions,
  action: benchmarkAction,
};
