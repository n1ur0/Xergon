/**
 * CLI command: trust
 *
 * Trust-score management for the Xergon Network.
 * View, compare, export, and administratively adjust provider trust scores.
 *
 * Usage:
 *   xergon trust score --provider ID
 *   xergon trust providers --min-score N --sort score|name|tee|zk
 *   xergon trust history --provider ID --last N
 *   xergon trust export --format csv|json --output FILE
 *   xergon trust compare --provider-a ID --provider-b ID
 *   xergon trust boost --provider ID --reason REASON
 *   xergon trust slash --provider ID --amount N --reason REASON
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ── Types ──────────────────────────────────────────────────────────

interface TrustScore {
  providerId: string;
  providerName: string;
  overallScore: number;
  tee: number;
  zk: number;
  uptime: number;
  ponw: number;
  reviews: number;
  lastUpdated: string;
}

interface TrustHistoryEntry {
  timestamp: string;
  overallScore: number;
  event: string;
}

interface TrustExportData {
  generatedAt: string;
  providers: TrustScore[];
}

type SortField = 'score' | 'name' | 'tee' | 'zk';

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function trustScoreColor(score: number): 'green' | 'yellow' | 'red' {
  if (score >= 80) return 'green';
  if (score >= 50) return 'yellow';
  return 'red';
}

function renderTrustBar(score: number, width: number = 30): string {
  const pct = Math.min(Math.max(score, 0), 100);
  const filled = Math.round((pct / 100) * width);
  const empty = width - filled;
  return '[' + '█'.repeat(filled) + '░'.repeat(empty) + '] ' + pct.toFixed(1);
}

function formatTimestamp(iso: string | undefined): string {
  if (!iso) return '-';
  return new Date(iso).toISOString().slice(0, 19).replace('T', ' ');
}

function sortProviders(providers: TrustScore[], sort: SortField): TrustScore[] {
  return [...providers].sort((a, b) => {
    switch (sort) {
      case 'score':
        return b.overallScore - a.overallScore;
      case 'name':
        return a.providerName.localeCompare(b.providerName);
      case 'tee':
        return b.tee - a.tee;
      case 'zk':
        return b.zk - a.zk;
      default:
        return b.overallScore - a.overallScore;
    }
  });
}

function renderScoreBreakdown(score: TrustScore, ctx: CLIContext, label?: string): void {
  const overallColor = trustScoreColor(score.overallScore);

  if (label) {
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize(label, 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(label.length), 'dim'));
  }

  ctx.output.write(ctx.output.formatText({
    'Provider': score.providerName,
    'Provider ID': score.providerId.substring(0, 20) + '...',
    'Overall Score': ctx.output.colorize(score.overallScore.toFixed(1) + ' / 100', overallColor),
    'Last Updated': formatTimestamp(score.lastUpdated),
  }));
  ctx.output.write('');

  const components = [
    { name: 'TEE Attestation', score: score.tee, weight: '30%', bar: renderTrustBar(score.tee, 25) },
    { name: 'ZK Proof Score', score: score.zk, weight: '25%', bar: renderTrustBar(score.zk, 25) },
    { name: 'Uptime', score: score.uptime, weight: '20%', bar: renderTrustBar(score.uptime, 25) },
    { name: 'Proof of Node Work', score: score.ponw, weight: '15%', bar: renderTrustBar(score.ponw, 25) },
    { name: 'Reviews', score: score.reviews, weight: '10%', bar: renderTrustBar(score.reviews, 25) },
  ];

  ctx.output.write(ctx.output.colorize('Component Breakdown:', 'bold'));
  for (const comp of components) {
    const color = trustScoreColor(comp.score);
    ctx.output.write(
      `  ${comp.name.padEnd(20)} (${comp.weight})  ` +
      `${ctx.output.colorize(comp.bar, color)}`
    );
  }
}

function exportCSV(providers: TrustScore[]): string {
  const headers = ['provider_id', 'provider_name', 'overall_score', 'tee', 'zk', 'uptime', 'ponw', 'reviews', 'last_updated'];
  const rows = providers.map(p =>
    [p.providerId, p.providerName, p.overallScore, p.tee, p.zk, p.uptime, p.ponw, p.reviews, p.lastUpdated].join(',')
  );
  return [headers.join(','), ...rows].join('\n');
}

// ── Subcommand: score ─────────────────────────────────────────────

async function handleScore(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon trust score --provider <id>');
    process.exit(1);
    return;
  }

  try {
    let score: TrustScore;

    if (ctx.client?.trust?.score) {
      score = await ctx.client.trust.score(providerId);
    } else {
      throw new Error('Trust client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(score, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Provider Trust Score', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    renderScoreBreakdown(score, ctx);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get trust score: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: providers ─────────────────────────────────────────

async function handleProviders(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const minScore = args.options.min_score ? Number(args.options.min_score) : 0;
  const sortBy = (args.options.sort ? String(args.options.sort) : 'score') as SortField;

  try {
    let providers: TrustScore[];

    if (ctx.client?.trust?.providers) {
      providers = await ctx.client.trust.providers({ minScore });
    } else {
      throw new Error('Trust client not available.');
    }

    // Apply minimum score filter
    providers = providers.filter(p => p.overallScore >= minScore);

    // Sort
    providers = sortProviders(providers, sortBy);

    if (providers.length === 0) {
      ctx.output.info('No providers found matching the criteria.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(providers, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = providers.map(p => {
        const color = trustScoreColor(p.overallScore);
        const badge = p.overallScore >= 80 ? 'HIGH' : p.overallScore >= 50 ? 'MED' : 'LOW';
        return {
          Provider: p.providerName.length > 20 ? p.providerName.substring(0, 20) + '...' : p.providerName,
          Score: p.overallScore.toFixed(1),
          TEE: p.tee.toFixed(1),
          ZK: p.zk.toFixed(1),
          Uptime: p.uptime.toFixed(1),
          Badge: badge,
        };
      });
      ctx.output.write(ctx.output.formatTable(tableData, `Providers (${providers.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Trusted Providers (${providers.length})`, 'bold'));
    ctx.output.write('');
    for (const p of providers) {
      const color = trustScoreColor(p.overallScore);
      const bar = renderTrustBar(p.overallScore, 20);
      ctx.output.write(
        `  ${ctx.output.colorize(p.overallScore.toFixed(1).padStart(5), color)}  ` +
        `${ctx.output.colorize(bar, color)}  ` +
        `${p.providerName}`
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list providers: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: history ───────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const lastN = args.options.last ? Number(args.options.last) : 20;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon trust history --provider <id> [--last N]');
    process.exit(1);
    return;
  }

  try {
    let history: TrustHistoryEntry[];

    if (ctx.client?.trust?.history) {
      history = await ctx.client.trust.history(providerId, { last: lastN });
    } else {
      throw new Error('Trust client not available.');
    }

    if (history.length === 0) {
      ctx.output.info('No trust score history found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(history, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Trust Score History (${history.length} entries)`, 'bold'));
    ctx.output.write('');

    if (isTableFormat(args)) {
      const tableData = history.map(h => ({
        Timestamp: formatTimestamp(h.timestamp),
        Score: h.overallScore.toFixed(1),
        Event: h.event,
      }));
      ctx.output.write(ctx.output.formatTable(tableData));
      return;
    }

    for (const entry of history) {
      const color = trustScoreColor(entry.overallScore);
      const bar = renderTrustBar(entry.overallScore, 15);
      ctx.output.write(
        `  ${formatTimestamp(entry.timestamp)}  ` +
        `${ctx.output.colorize(bar, color)}  ` +
        `${entry.event}`
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get trust history: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: export ────────────────────────────────────────────

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const format = args.options.format ? String(args.options.format) : 'json';
  const outputFile = args.options.output ? String(args.options.output) : undefined;

  if (!['json', 'csv'].includes(format)) {
    ctx.output.writeError('Export format must be json or csv');
    process.exit(1);
    return;
  }

  try {
    let providers: TrustScore[];

    if (ctx.client?.trust?.providers) {
      providers = await ctx.client.trust.providers({ minScore: 0 });
    } else {
      throw new Error('Trust client not available.');
    }

    const exportData: TrustExportData = {
      generatedAt: new Date().toISOString(),
      providers,
    };

    let content: string;
    if (format === 'csv') {
      content = exportCSV(providers);
    } else {
      content = JSON.stringify(exportData, null, 2);
    }

    if (outputFile) {
      const resolvedPath = path.resolve(outputFile);
      fs.writeFileSync(resolvedPath, content, 'utf-8');
      ctx.output.success(`Trust data exported to ${resolvedPath} (${content.length} bytes)`);
    } else {
      ctx.output.write(content);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to export trust data: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: compare ───────────────────────────────────────────

async function handleCompare(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerA = args.options.provider_a ? String(args.options.provider_a) : undefined;
  const providerB = args.options.provider_b ? String(args.options.provider_b) : undefined;

  if (!providerA || !providerB) {
    ctx.output.writeError('Usage: xergon trust compare --provider-a <id> --provider-b <id>');
    process.exit(1);
    return;
  }

  try {
    let scoreA: TrustScore;
    let scoreB: TrustScore;

    if (ctx.client?.trust?.score) {
      [scoreA, scoreB] = await Promise.all([
        ctx.client.trust.score(providerA),
        ctx.client.trust.score(providerB),
      ]);
    } else {
      throw new Error('Trust client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ providerA: scoreA, providerB: scoreB }, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Trust Score Comparison', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(50), 'dim'));
    ctx.output.write('');

    const maxNameLen = Math.max(scoreA.providerName.length, scoreB.providerName.length);
    const colA = scoreA.providerName.padEnd(maxNameLen);
    const colB = scoreB.providerName.padEnd(maxNameLen);
    const headerWidth = maxNameLen + 4;

    ctx.output.write(
      '  ' + 'Metric'.padEnd(22) +
      ctx.output.colorize(colA, trustScoreColor(scoreA.overallScore)) + '  ' +
      ctx.output.colorize(colB, trustScoreColor(scoreB.overallScore))
    );
    ctx.output.write('  ' + '─'.repeat(headerWidth * 2 + 22));

    const metrics = [
      { label: 'Overall Score', a: scoreA.overallScore, b: scoreB.overallScore },
      { label: 'TEE Attestation', a: scoreA.tee, b: scoreB.tee },
      { label: 'ZK Proof Score', a: scoreA.zk, b: scoreB.zk },
      { label: 'Uptime', a: scoreA.uptime, b: scoreB.uptime },
      { label: 'Proof of Node Work', a: scoreA.ponw, b: scoreB.ponw },
      { label: 'Reviews', a: scoreA.reviews, b: scoreB.reviews },
    ];

    for (const m of metrics) {
      const aColor = trustScoreColor(m.a);
      const bColor = trustScoreColor(m.b);
      const aStr = m.a.toFixed(1).padEnd(maxNameLen);
      const bStr = m.b.toFixed(1).padEnd(maxNameLen);
      ctx.output.write(
        '  ' + m.label.padEnd(22) +
        ctx.output.colorize(aStr, aColor) + '  ' +
        ctx.output.colorize(bStr, bColor)
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to compare trust scores: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: boost ─────────────────────────────────────────────

async function handleBoost(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const reason = args.options.reason ? String(args.options.reason) : undefined;

  if (!providerId || !reason) {
    ctx.output.writeError('Usage: xergon trust boost --provider <id> --reason <reason>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Boosting trust for provider ${providerId.substring(0, 16)}...`);

  try {
    let result: { providerId: string; previousScore: number; newScore: number; reason: string; timestamp: string };

    if (ctx.client?.trust?.boost) {
      result = await ctx.client.trust.boost({ providerId, reason });
    } else {
      throw new Error('Trust client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success(`Trust boosted for provider ${providerId.substring(0, 16)}...`);
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': result.providerId,
      'Previous Score': result.previousScore.toFixed(1),
      'New Score': ctx.output.colorize(result.newScore.toFixed(1), trustScoreColor(result.newScore)),
      Reason: result.reason,
      Timestamp: formatTimestamp(result.timestamp),
    }, 'Trust Boost'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to boost trust: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: slash ─────────────────────────────────────────────

async function handleSlash(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const amount = args.options.amount ? Number(args.options.amount) : undefined;
  const reason = args.options.reason ? String(args.options.reason) : undefined;

  if (!providerId || !amount || !reason) {
    ctx.output.writeError('Usage: xergon trust slash --provider <id> --amount <n> --reason <reason>');
    process.exit(1);
    return;
  }

  if (amount <= 0) {
    ctx.output.writeError('Slash amount must be a positive number');
    process.exit(1);
    return;
  }

  ctx.output.warn(`Slashing ${amount} trust points from provider ${providerId.substring(0, 16)}...`);

  try {
    let result: { providerId: string; previousScore: number; newScore: number; slashed: number; reason: string; timestamp: string };

    if (ctx.client?.trust?.slash) {
      result = await ctx.client.trust.slash({ providerId, amount, reason });
    } else {
      throw new Error('Trust client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Slashed ${result.slashed} points from provider ${providerId.substring(0, 16)}...`, 'red'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': result.providerId,
      'Previous Score': result.previousScore.toFixed(1),
      'New Score': ctx.output.colorize(result.newScore.toFixed(1), trustScoreColor(result.newScore)),
      Slashed: ctx.output.colorize(String(result.slashed), 'red'),
      Reason: result.reason,
      Timestamp: formatTimestamp(result.timestamp),
    }, 'Trust Slash'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to slash trust: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function trustAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon trust <score|providers|history|export|compare|boost|slash> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  score      Show trust score with component breakdown');
    ctx.output.write('  providers  List providers sorted by trust score');
    ctx.output.write('  history    Trust score history over time');
    ctx.output.write('  export     Export trust data (CSV or JSON)');
    ctx.output.write('  compare    Side-by-side trust comparison');
    ctx.output.write('  boost      Boost provider trust score (admin)');
    ctx.output.write('  slash      Slash provider trust score (admin)');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'score':
      await handleScore(args, ctx);
      break;
    case 'providers':
      await handleProviders(args, ctx);
      break;
    case 'history':
      await handleHistory(args, ctx);
      break;
    case 'export':
      await handleExport(args, ctx);
      break;
    case 'compare':
      await handleCompare(args, ctx);
      break;
    case 'boost':
      await handleBoost(args, ctx);
      break;
    case 'slash':
      await handleSlash(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: score, providers, history, export, compare, boost, slash');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const trustOptions: CommandOption[] = [
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
    description: 'Output format: text, json, table, csv',
    required: false,
    type: 'string',
  },
  {
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Provider ID',
    required: false,
    type: 'string',
  },
  {
    name: 'provider_a',
    short: '',
    long: '--provider-a',
    description: 'First provider ID for comparison',
    required: false,
    type: 'string',
  },
  {
    name: 'provider_b',
    short: '',
    long: '--provider-b',
    description: 'Second provider ID for comparison',
    required: false,
    type: 'string',
  },
  {
    name: 'min_score',
    short: '',
    long: '--min-score',
    description: 'Minimum trust score to list providers (default: 0)',
    required: false,
    type: 'number',
  },
  {
    name: 'sort',
    short: '',
    long: '--sort',
    description: 'Sort providers by: score, name, tee, zk (default: score)',
    required: false,
    default: 'score',
    type: 'string',
  },
  {
    name: 'last',
    short: '',
    long: '--last',
    description: 'Number of history entries to show (default: 20)',
    required: false,
    type: 'number',
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
    name: 'amount',
    short: '',
    long: '--amount',
    description: 'Amount to slash from trust score',
    required: false,
    type: 'number',
  },
  {
    name: 'reason',
    short: '',
    long: '--reason',
    description: 'Reason for boost or slash action',
    required: false,
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const trustCommand: Command = {
  name: 'trust',
  description: 'Trust-score management: view, compare, export, boost, and slash provider trust scores',
  aliases: ['reputation', 'score'],
  options: trustOptions,
  action: trustAction,
};
