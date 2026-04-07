/**
 * CLI command: bench
 *
 * Run a performance benchmark against a model on the Xergon relay.
 * Measures latency percentiles, throughput, and token usage.
 *
 * Usage:
 *   xergon bench <model>
 *   xergon bench <model> --requests 50 --concurrent 5
 *   xergon bench <model> --json
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import { runBench, type BenchResult } from '../../bench';

const benchOptions: CommandOption[] = [
  {
    name: 'concurrent',
    short: '-c',
    long: '--concurrent',
    description: 'Concurrent requests (1-10, default: 1)',
    required: false,
    type: 'number',
  },
  {
    name: 'requests',
    short: '-n',
    long: '--requests',
    description: 'Total number of requests (default: 10)',
    required: false,
    type: 'number',
  },
  {
    name: 'prompt',
    short: '-p',
    long: '--prompt',
    description: 'Custom prompt text',
    required: false,
    type: 'string',
  },
  {
    name: 'maxTokens',
    short: '',
    long: '--max-tokens',
    description: 'Max tokens in response (default: 64)',
    required: false,
    type: 'number',
  },
  {
    name: 'warmup',
    short: '-w',
    long: '--warmup',
    description: 'Warmup rounds to discard (default: 2)',
    required: false,
    type: 'number',
  },
  {
    name: 'timeout',
    short: '',
    long: '--timeout',
    description: 'Per-request timeout in ms (default: 30000)',
    required: false,
    type: 'number',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output results as JSON',
    required: false,
    type: 'boolean',
  },
];

async function benchAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = args.positional[0];

  if (!model) {
    ctx.output.writeError('Usage: xergon bench <model>');
    ctx.output.info('Example: xergon bench llama-3.3-70b');
    process.exit(1);
    return;
  }

  // Resolve concurrent (clamp to 1-10)
  const concurrentRaw = args.options.concurrent !== undefined
    ? Number(args.options.concurrent)
    : 1;
  const concurrent = Math.max(1, Math.min(10, concurrentRaw));

  const requests = args.options.requests !== undefined
    ? Number(args.options.requests)
    : 10;

  const prompt = args.options.prompt ? String(args.options.prompt) : undefined;
  const maxTokens = args.options.maxTokens !== undefined
    ? Number(args.options.maxTokens)
    : undefined;
  const warmup = args.options.warmup !== undefined
    ? Number(args.options.warmup)
    : undefined;
  const timeout = args.options.timeout !== undefined
    ? Number(args.options.timeout)
    : undefined;
  const outputJson = args.options.json === true;

  // Show start message
  ctx.output.info(
    `Benchmarking ${model} (${requests} requests, ${concurrent} concurrent, ${warmup ?? 2} warmup)...`,
  );
  ctx.output.write('');

  const result: BenchResult = await runBench({
    model,
    prompt,
    maxTokens,
    concurrent,
    requests,
    warmup,
    timeout,
    baseUrl: ctx.config.baseUrl,
    apiKey: ctx.config.apiKey,
  });

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(result));
    return;
  }

  // ── Human-readable table output ───────────────────────────────
  const output = ctx.output;
  const bar = (value: number, max: number, width: number = 30): string => {
    if (max === 0) return ' '.repeat(width);
    const filled = Math.round((value / max) * width);
    return output.colorize('█'.repeat(filled), 'cyan') + ' '.repeat(width - filled);
  };

  output.write(output.colorize('Benchmark Results', 'bold'));
  output.write(output.colorize('═══════════════════════════════════════════════', 'dim'));
  output.write('');

  // Model info
  output.write(`  ${output.colorize('Model:', 'cyan')}          ${result.model}`);
  output.write(`  ${output.colorize('Requests:', 'cyan')}       ${result.totalRequests} (${output.colorize(`${result.successful} ok`, 'green')}, ${output.colorize(`${result.failed} failed`, result.failed > 0 ? 'red' : 'dim')})`);
  output.write(`  ${output.colorize('Total Time:', 'cyan')}     ${result.totalDuration}ms`);
  output.write('');

  // Throughput
  output.write(output.colorize('  Throughput', 'bold'));
  const rpsBar = bar(result.requestsPerSecond, 50);
  output.write(`  ${output.colorize('Req/s:', 'cyan')}          ${result.requestsPerSecond.toFixed(2).padStart(8)}  ${rpsBar}`);
  const tpsBar = bar(result.tokensPerSecond, 5000);
  output.write(`  ${output.colorize('Tokens/s:', 'cyan')}       ${result.tokensPerSecond.toFixed(2).padStart(8)}  ${tpsBar}`);
  output.write('');

  // Latency
  output.write(output.colorize('  Latency', 'bold'));
  const maxLat = result.maxLatency || 1;
  output.write(`  ${output.colorize('Avg:', 'cyan')}            ${String(result.avgLatency).padStart(6)}ms  ${bar(result.avgLatency, maxLat)}`);
  output.write(`  ${output.colorize('Min:', 'cyan')}            ${String(result.minLatency).padStart(6)}ms  ${bar(result.minLatency, maxLat)}`);
  output.write(`  ${output.colorize('Max:', 'cyan')}            ${String(result.maxLatency).padStart(6)}ms  ${bar(result.maxLatency, maxLat)}`);
  output.write('');
  output.write(`  ${output.colorize('p50:', 'cyan')}            ${String(result.p50Latency).padStart(6)}ms  ${bar(result.p50Latency, maxLat)}`);
  output.write(`  ${output.colorize('p90:', 'cyan')}            ${String(result.p90Latency).padStart(6)}ms  ${bar(result.p90Latency, maxLat)}`);
  output.write(`  ${output.colorize('p99:', 'cyan')}            ${String(result.p99Latency).padStart(6)}ms  ${bar(result.p99Latency, maxLat)}`);
  output.write('');

  // Tokens
  output.write(output.colorize('  Tokens', 'bold'));
  output.write(`  ${output.colorize('Total:', 'cyan')}          ${result.totalTokens}`);
  output.write('');

  // Errors
  if (result.errors.length > 0) {
    output.write(output.colorize('  Errors', 'red'));
    const uniqueErrors = [...new Set(result.errors)];
    for (const err of uniqueErrors.slice(0, 5)) {
      output.write(`    ${output.colorize('•', 'red')} ${err.slice(0, 80)}`);
    }
    if (uniqueErrors.length > 5) {
      output.write(`    ${output.colorize(`... and ${uniqueErrors.length - 5} more`, 'dim')}`);
    }
    output.write('');
  }

  output.write(output.colorize('═══════════════════════════════════════════════', 'dim'));

  // Exit with error code if all requests failed
  if (result.successful === 0) {
    process.exit(1);
  }
}

export const benchCommand: Command = {
  name: 'bench',
  description: 'Benchmark model latency and throughput',
  aliases: ['benchmark', 'perf'],
  options: benchOptions,
  action: benchAction,
};
