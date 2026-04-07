/**
 * CLI command: eval
 *
 * Run evaluation benchmarks against models on the Xergon Network.
 *
 * Usage:
 *   xergon eval run <benchmark> --model X
 *   xergon eval list
 *   xergon eval compare <model1> <model2> --benchmark X
 *   xergon eval history
 *   xergon eval export --format json|csv|markdown
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';

async function evalAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon eval <run|list|compare|history|export> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'run':
      await handleRun(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
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
      ctx.output.write('Usage: xergon eval <run|list|compare|history|export> [options]');
      process.exit(1);
  }
}

// ── run ────────────────────────────────────────────────────────────

async function handleRun(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const benchmark = args.positional[1];
  const model = String(args.options.model || ctx.config.defaultModel);
  const maxTokens = args.options.max_tokens !== undefined ? Number(args.options.max_tokens) : 256;
  const temperature = args.options.temperature !== undefined ? Number(args.options.temperature) : 0.0;

  if (!benchmark) {
    ctx.output.writeError('Usage: xergon eval run <benchmark> --model X');
    ctx.output.write('Run "xergon eval list" to see available benchmarks.');
    process.exit(1);
    return;
  }

  const {
    runBenchmark,
    saveToHistory,
  } = await import('../../eval');

  ctx.output.info(`Running "${benchmark}" benchmark with model "${model}"...`);
  ctx.output.write('');

  const result = await runBenchmark(benchmark, model, {
    baseUrl: ctx.config.baseUrl,
    apiKey: ctx.config.apiKey,
    maxTokens,
    temperature,
  });

  // Save to history
  saveToHistory(result);

  // Display results
  const scorePct = (result.score * 100).toFixed(1);
  const duration = (result.duration / 1000).toFixed(1);

  ctx.output.success(`Benchmark "${benchmark}" complete`);
  ctx.output.write('');

  const tableData = [{
    Benchmark: result.benchmark,
    Model: result.model,
    Score: `${scorePct}%`,
    Correct: `${result.correct}/${result.total}`,
    Duration: `${duration}s`,
  }];
  ctx.output.write(ctx.output.formatTable(tableData, 'Benchmark Results'));

  // Show details if JSON output
  if (args.options.verbose) {
    ctx.output.write(ctx.output.formatText(result.details, 'Detailed Results'));
  }

  // Show failures
  if (result.details) {
    const failures = result.details.filter(d => !d.correct);
    if (failures.length > 0) {
      ctx.output.write('');
      ctx.output.writeError(`Failures (${failures.length}):`);
      for (const f of failures) {
        ctx.output.write(`  Expected: ${f.expected}`);
        ctx.output.write(`  Got:      ${f.actual.slice(0, 100)}${f.actual.length > 100 ? '...' : ''}`);
        ctx.output.write('');
      }
    }
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { listBenchmarks } = await import('../../eval');
  const benchmarks = listBenchmarks();

  const tableData = benchmarks.map(b => ({
    Name: b.name,
    Category: b.category,
    Examples: b.num_examples,
    Metric: b.metric,
    Description: b.description,
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Available Benchmarks (${benchmarks.length})`));
}

// ── compare ────────────────────────────────────────────────────────

async function handleCompare(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model1 = args.positional[1];
  const model2 = args.positional[2];
  const benchmark = String(args.options.benchmark || 'gsm8k');

  if (!model1 || !model2) {
    ctx.output.writeError('Usage: xergon eval compare <model1> <model2> --benchmark X');
    process.exit(1);
    return;
  }

  const { compareBenchmarks } = await import('../../eval');

  ctx.output.info(`Comparing "${model1}" vs "${model2}" on "${benchmark}"...`);

  const result = await compareBenchmarks(model1, model2, benchmark, {
    baseUrl: ctx.config.baseUrl,
    apiKey: ctx.config.apiKey,
  });

  ctx.output.success('Comparison complete');
  ctx.output.write('');

  const tableData = [{
    Benchmark: result.benchmark,
    Winner: result.winner,
    [model1]: `${(result.score1 * 100).toFixed(1)}%`,
    [model2]: `${(result.score2 * 100).toFixed(1)}%`,
    Diff: `${result.diff > 0 ? '+' : ''}${(result.diff * 100).toFixed(1)}%`,
  }];

  ctx.output.write(ctx.output.formatTable(tableData, 'Model Comparison'));
}

// ── history ────────────────────────────────────────────────────────

async function handleHistory(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { loadHistory } = await import('../../eval');
  const history = loadHistory();

  if (history.length === 0) {
    ctx.output.info('No evaluation history found.');
    return;
  }

  const tableData = history.map(h => ({
    Date: new Date(h.timestamp).toISOString().slice(0, 19),
    Benchmark: h.benchmark,
    Model: h.model,
    Score: `${(h.score * 100).toFixed(1)}%`,
    Correct: `${h.correct}/${h.total}`,
    Duration: `${(h.duration / 1000).toFixed(1)}s`,
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Eval History (${history.length} runs)`));
}

// ── export ─────────────────────────────────────────────────────────

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const format = String(args.options.format || 'json') as 'json' | 'csv' | 'markdown';

  if (!['json', 'csv', 'markdown'].includes(format)) {
    ctx.output.writeError(`Invalid format: ${format}. Must be json, csv, or markdown.`);
    process.exit(1);
    return;
  }

  const { loadHistory, exportResults } = await import('../../eval');
  const history = loadHistory();

  if (history.length === 0) {
    ctx.output.info('No evaluation history to export.');
    return;
  }

  const { runBenchmark: _, ...resultFields } = history[0] as any;
  const results = history.map(h => ({
    benchmark: h.benchmark,
    model: h.model,
    score: h.score,
    total: h.total,
    correct: h.correct,
    duration: h.duration,
  }));

  const output = exportResults(results, format);

  ctx.output.write(output);
}

export const evalCommand: Command = {
  name: 'eval',
  description: 'Run evaluation benchmarks against models',
  aliases: ['benchmark', 'evals'],
  options: [
    {
      name: 'model',
      short: '-m',
      long: '--model',
      description: 'Model to evaluate',
      required: false,
      type: 'string',
    },
    {
      name: 'benchmark',
      short: '',
      long: '--benchmark',
      description: 'Benchmark to run (for compare)',
      required: false,
      type: 'string',
    },
    {
      name: 'max_tokens',
      short: '',
      long: '--max-tokens',
      description: 'Max tokens per response (default: 256)',
      required: false,
      type: 'number',
    },
    {
      name: 'temperature',
      short: '',
      long: '--temperature',
      description: 'Sampling temperature (default: 0.0)',
      required: false,
      type: 'number',
    },
    {
      name: 'format',
      short: '-f',
      long: '--format',
      description: 'Export format: json, csv, markdown (default: json)',
      required: false,
      type: 'string',
    },
    {
      name: 'verbose',
      short: '-v',
      long: '--verbose',
      description: 'Show detailed results',
      required: false,
      type: 'boolean',
    },
  ],
  action: evalAction,
};
