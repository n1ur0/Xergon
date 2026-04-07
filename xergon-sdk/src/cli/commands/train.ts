/**
 * CLI command: train
 *
 * Federated training management for the Xergon Network.
 * Start, join, monitor, and aggregate federated training rounds,
 * as well as perform knowledge distillation.
 *
 * Usage:
 *   xergon train start --model MODEL --rounds N --strategy fedavg|fedprox --min-providers N
 *   xergon train join --round-id ID --provider-id ID
 *   xergon train status --round-id ID
 *   xergon train submit --round-id ID --delta-file PATH
 *   xergon train list [--status all|collecting|training|aggregating|complete]
 *   xergon train cancel --round-id ID --reason REASON
 *   xergon train aggregate --round-id ID
 *   xergon train distill --teacher MODEL --student MODEL --temperature N --alpha N
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ── Types ──────────────────────────────────────────────────────────

type TrainingPhase = 'collecting' | 'training' | 'aggregating' | 'complete' | 'cancelled';
type FederatedStrategy = 'fedavg' | 'fedprox';

interface TrainingRound {
  id: string;
  model: string;
  strategy: FederatedStrategy;
  totalRounds: number;
  currentRound: number;
  phase: TrainingPhase;
  minProviders: number;
  participants: TrainingParticipant[];
  createdAt: string;
  updatedAt: string;
}

interface TrainingParticipant {
  providerId: string;
  status: 'joined' | 'training' | 'submitted' | 'failed';
  deltaSize?: number;
  submittedAt?: string;
}

interface TrainingRoundSummary {
  id: string;
  model: string;
  strategy: FederatedStrategy;
  phase: TrainingPhase;
  currentRound: number;
  totalRounds: number;
  participants: number;
  createdAt: string;
}

interface StartTrainingInput {
  model: string;
  rounds: number;
  strategy: FederatedStrategy;
  minProviders: number;
}

interface JoinRoundInput {
  roundId: string;
  providerId: string;
}

interface SubmitDeltaInput {
  roundId: string;
  deltaFile: string;
  deltaData: Buffer;
}

interface DistillInput {
  teacher: string;
  student: string;
  temperature: number;
  alpha: number;
}

interface AggregateResult {
  roundId: string;
  aggregatedWeightsUrl: string;
  participantsIncluded: number;
  averageLoss: number;
  timestamp: string;
}

interface DistillResult {
  jobId: string;
  teacher: string;
  student: string;
  temperature: number;
  alpha: number;
  status: string;
  estimatedTime: string;
}

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function phaseColor(phase: TrainingPhase): 'green' | 'cyan' | 'yellow' | 'dim' | 'red' {
  switch (phase) {
    case 'collecting': return 'cyan';
    case 'training': return 'yellow';
    case 'aggregating': return 'yellow';
    case 'complete': return 'green';
    case 'cancelled': return 'red';
  }
}

function participantStatusColor(status: string): 'green' | 'cyan' | 'yellow' | 'red' {
  switch (status) {
    case 'submitted': return 'green';
    case 'training': return 'yellow';
    case 'joined': return 'cyan';
    case 'failed': return 'red';
    default: return 'yellow';
  }
}

function renderProgressBar(current: number, total: number, width: number = 30): string {
  if (total <= 0) return '[  ] 0%';
  const pct = Math.min(Math.round((current / total) * 100), 100);
  const filled = Math.round((current / total) * width);
  const empty = width - filled;
  const bar = '█'.repeat(filled) + '░'.repeat(empty);
  return `[${bar}] ${pct}%`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

// ── Subcommand: start ─────────────────────────────────────────────

async function handleStart(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = args.options.model ? String(args.options.model) : undefined;
  const rounds = args.options.rounds ? Number(args.options.rounds) : undefined;
  const strategyStr = args.options.strategy ? String(args.options.strategy) : undefined;
  const minProviders = args.options.min_providers ? Number(args.options.min_providers) : undefined;

  if (!model) {
    ctx.output.writeError('Usage: xergon train start --model <model> --rounds <n> --strategy fedavg|fedprox --min-providers <n>');
    process.exit(1);
    return;
  }

  const strategy = (strategyStr || 'fedavg') as FederatedStrategy;
  if (!['fedavg', 'fedprox'].includes(strategy)) {
    ctx.output.writeError('Strategy must be one of: fedavg, fedprox');
    process.exit(1);
    return;
  }

  const input: StartTrainingInput = {
    model,
    rounds: rounds || 10,
    strategy,
    minProviders: minProviders || 3,
  };

  ctx.output.info(`Starting federated training round for ${model}...`);

  try {
    let round: TrainingRound;

    if (ctx.client?.train?.start) {
      round = await ctx.client.train.start(input);
    } else {
      throw new Error('Training client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(round, null, 2));
      return;
    }

    ctx.output.success('Training round started successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Round ID': round.id,
      Model: round.model,
      Strategy: round.strategy.toUpperCase(),
      'Total Rounds': String(round.totalRounds),
      'Min Providers': String(round.minProviders),
      Phase: round.phase.toUpperCase(),
      'Created At': round.createdAt,
    }, 'Federated Training Round'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to start training round: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: join ──────────────────────────────────────────────

async function handleJoin(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const roundId = args.options.round_id ? String(args.options.round_id) : undefined;
  const providerId = args.options.provider_id ? String(args.options.provider_id) : undefined;

  if (!roundId || !providerId) {
    ctx.output.writeError('Usage: xergon train join --round-id <id> --provider-id <id>');
    process.exit(1);
    return;
  }

  const input: JoinRoundInput = { roundId, providerId };

  ctx.output.info(`Joining training round ${roundId.substring(0, 16)}...`);

  try {
    if (ctx.client?.train?.join) {
      await ctx.client.train.join(input);
    } else {
      throw new Error('Training client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ roundId, providerId, status: 'joined' }, null, 2));
      return;
    }

    ctx.output.success(`Provider ${providerId.substring(0, 16)}... joined round ${roundId.substring(0, 16)}...`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to join training round: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: status ────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const roundId = args.options.round_id ? String(args.options.round_id) : undefined;

  if (!roundId) {
    ctx.output.writeError('Usage: xergon train status --round-id <id>');
    process.exit(1);
    return;
  }

  try {
    let round: TrainingRound;

    if (ctx.client?.train?.status) {
      round = await ctx.client.train.status(roundId);
    } else {
      throw new Error('Training client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(round, null, 2));
      return;
    }

    // Text output with progress bar and participant table
    ctx.output.write(ctx.output.colorize('Training Round Status', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Round ID': round.id,
      Model: round.model,
      Strategy: round.strategy.toUpperCase(),
      Phase: ctx.output.colorize(round.phase.toUpperCase(), phaseColor(round.phase)),
      Progress: renderProgressBar(round.currentRound, round.totalRounds),
      'Current Round': `${round.currentRound} / ${round.totalRounds}`,
      'Min Providers': String(round.minProviders),
      Participants: String(round.participants.length),
      'Created At': round.createdAt,
      'Updated At': round.updatedAt,
    }));

    // Participants table
    if (round.participants.length > 0) {
      ctx.output.write('');
      if (isTableFormat(args)) {
        const tableData = round.participants.map(p => ({
          Provider: p.providerId.substring(0, 16) + '...',
          Status: p.status.toUpperCase(),
          'Delta Size': p.deltaSize ? formatBytes(p.deltaSize) : '-',
          'Submitted At': p.submittedAt ? new Date(p.submittedAt).toISOString().slice(0, 19) : '-',
        }));
        ctx.output.write(ctx.output.formatTable(tableData, `Participants (${round.participants.length})`));
      } else {
        ctx.output.write(ctx.output.colorize(`Participants (${round.participants.length}):`, 'bold'));
        for (const p of round.participants) {
          const color = participantStatusColor(p.status);
          const deltaStr = p.deltaSize ? `  Delta: ${formatBytes(p.deltaSize)}` : '';
          ctx.output.write(
            `  ${ctx.output.colorize(p.providerId.substring(0, 20) + '...', 'cyan')}  ` +
            `${ctx.output.colorize(p.status.toUpperCase(), color)}${deltaStr}`
          );
        }
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get training round status: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: submit ────────────────────────────────────────────

async function handleSubmit(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const roundId = args.options.round_id ? String(args.options.round_id) : undefined;
  const deltaFile = args.options.delta_file ? String(args.options.delta_file) : undefined;

  if (!roundId || !deltaFile) {
    ctx.output.writeError('Usage: xergon train submit --round-id <id> --delta-file <path>');
    process.exit(1);
    return;
  }

  const resolvedPath = path.resolve(deltaFile);
  if (!fs.existsSync(resolvedPath)) {
    ctx.output.writeError(`Delta file not found: ${resolvedPath}`);
    process.exit(1);
    return;
  }

  const deltaData = fs.readFileSync(resolvedPath);
  ctx.output.info(`Submitting weight delta for round ${roundId.substring(0, 16)}... (${formatBytes(deltaData.length)})`);

  try {
    const input: SubmitDeltaInput = { roundId, deltaFile: resolvedPath, deltaData };

    if (ctx.client?.train?.submit) {
      await ctx.client.train.submit(input);
    } else {
      throw new Error('Training client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ roundId, deltaFile: resolvedPath, size: deltaData.length, status: 'submitted' }, null, 2));
      return;
    }

    ctx.output.success(`Weight delta submitted successfully (${formatBytes(deltaData.length)})`);
    ctx.output.write(`  Round: ${ctx.output.colorize(roundId.substring(0, 16) + '...', 'cyan')}`);
    ctx.output.write(`  File:  ${resolvedPath}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to submit weight delta: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: list ──────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const statusFilter = args.options.status ? String(args.options.status) : 'all';

  try {
    let rounds: TrainingRoundSummary[];

    if (ctx.client?.train?.list) {
      rounds = await ctx.client.train.list({ status: statusFilter });
    } else {
      throw new Error('Training client not available.');
    }

    // Apply local filter if not done server-side
    if (statusFilter !== 'all' && rounds.length > 0) {
      rounds = rounds.filter(r => r.phase === statusFilter);
    }

    if (rounds.length === 0) {
      ctx.output.info('No training rounds found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(rounds, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = rounds.map(r => ({
        ID: r.id.substring(0, 12) + '...',
        Model: r.model.length > 25 ? r.model.substring(0, 25) + '...' : r.model,
        Strategy: r.strategy.toUpperCase(),
        Phase: r.phase.toUpperCase(),
        Progress: `${r.currentRound}/${r.totalRounds}`,
        Providers: String(r.participants),
        Created: r.createdAt ? new Date(r.createdAt).toISOString().slice(0, 10) : '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Training Rounds (${rounds.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Training Rounds (${rounds.length})`, 'bold'));
    ctx.output.write('');
    for (const r of rounds) {
      const color = phaseColor(r.phase);
      const progress = renderProgressBar(r.currentRound, r.totalRounds, 20);
      ctx.output.write(
        `  ${ctx.output.colorize(r.id.substring(0, 16) + '...', 'cyan')}  ` +
        `${ctx.output.colorize(r.phase.toUpperCase(), color)}  ` +
        `${r.model}`
      );
      ctx.output.write(`    ${progress}  ${r.currentRound}/${r.totalRounds} rounds  |  ${r.participants} providers`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list training rounds: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: cancel ────────────────────────────────────────────

async function handleCancel(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const roundId = args.options.round_id ? String(args.options.round_id) : undefined;
  const reason = args.options.reason ? String(args.options.reason) : 'Cancelled by user';

  if (!roundId) {
    ctx.output.writeError('Usage: xergon train cancel --round-id <id> --reason <reason>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Cancelling training round ${roundId.substring(0, 16)}...`);

  try {
    if (ctx.client?.train?.cancel) {
      await ctx.client.train.cancel(roundId, reason);
    } else {
      throw new Error('Training client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ roundId, reason, status: 'cancelled' }, null, 2));
      return;
    }

    ctx.output.success(`Training round ${roundId.substring(0, 16)}... cancelled`);
    ctx.output.write(`  Reason: ${reason}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to cancel training round: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: aggregate ─────────────────────────────────────────

async function handleAggregate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const roundId = args.options.round_id ? String(args.options.round_id) : undefined;

  if (!roundId) {
    ctx.output.writeError('Usage: xergon train aggregate --round-id <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Triggering manual aggregation for round ${roundId.substring(0, 16)}...`);

  try {
    let result: AggregateResult;

    if (ctx.client?.train?.aggregate) {
      result = await ctx.client.train.aggregate(roundId);
    } else {
      throw new Error('Training client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Aggregation completed successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Round ID': result.roundId,
      'Participants Included': String(result.participantsIncluded),
      'Average Loss': result.averageLoss.toFixed(6),
      'Aggregated Weights': result.aggregatedWeightsUrl,
      'Timestamp': result.timestamp,
    }, 'Aggregation Result'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to aggregate training round: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: distill ───────────────────────────────────────────

async function handleDistill(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const teacher = args.options.teacher ? String(args.options.teacher) : undefined;
  const student = args.options.student ? String(args.options.student) : undefined;
  const temperature = args.options.temperature ? Number(args.options.temperature) : 4.0;
  const alpha = args.options.alpha ? Number(args.options.alpha) : 0.5;

  if (!teacher || !student) {
    ctx.output.writeError('Usage: xergon train distill --teacher <model> --student <model> [--temperature N] [--alpha N]');
    process.exit(1);
    return;
  }

  const input: DistillInput = { teacher, student, temperature, alpha };

  ctx.output.info(`Starting knowledge distillation: ${teacher} -> ${student}`);

  try {
    let result: DistillResult;

    if (ctx.client?.train?.distill) {
      result = await ctx.client.train.distill(input);
    } else {
      throw new Error('Training client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Knowledge distillation job started');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Job ID': result.jobId,
      'Teacher Model': result.teacher,
      'Student Model': result.student,
      Temperature: String(result.temperature),
      Alpha: String(result.alpha),
      Status: result.status,
      'Estimated Time': result.estimatedTime,
    }, 'Distillation Job'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to start knowledge distillation: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function trainAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon train <start|join|status|submit|list|cancel|aggregate|distill> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  start      Start a new federated training round');
    ctx.output.write('  join       Join a training round as a provider');
    ctx.output.write('  status     Show training round status and participants');
    ctx.output.write('  submit     Submit weight delta after local training');
    ctx.output.write('  list       List training rounds');
    ctx.output.write('  cancel     Cancel a training round');
    ctx.output.write('  aggregate  Trigger manual aggregation of a round');
    ctx.output.write('  distill    Start knowledge distillation between models');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'start':
      await handleStart(args, ctx);
      break;
    case 'join':
      await handleJoin(args, ctx);
      break;
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'submit':
      await handleSubmit(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'cancel':
      await handleCancel(args, ctx);
      break;
    case 'aggregate':
      await handleAggregate(args, ctx);
      break;
    case 'distill':
      await handleDistill(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: start, join, status, submit, list, cancel, aggregate, distill');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const trainOptions: CommandOption[] = [
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
    description: 'Output format: text, json, or table',
    required: false,
    type: 'string',
  },
  {
    name: 'round_id',
    short: '',
    long: '--round-id',
    description: 'Training round ID for status, submit, cancel, aggregate',
    required: false,
    type: 'string',
  },
  {
    name: 'model',
    short: '',
    long: '--model',
    description: 'Model name for training round',
    required: false,
    type: 'string',
  },
  {
    name: 'strategy',
    short: '',
    long: '--strategy',
    description: 'Federated learning strategy: fedavg or fedprox (default: fedavg)',
    required: false,
    default: 'fedavg',
    type: 'string',
  },
  {
    name: 'rounds',
    short: '',
    long: '--rounds',
    description: 'Number of training rounds (default: 10)',
    required: false,
    default: '10',
    type: 'number',
  },
  {
    name: 'min_providers',
    short: '',
    long: '--min-providers',
    description: 'Minimum number of providers to start (default: 3)',
    required: false,
    default: '3',
    type: 'number',
  },
  {
    name: 'provider_id',
    short: '',
    long: '--provider-id',
    description: 'Provider ID to join a training round',
    required: false,
    type: 'string',
  },
  {
    name: 'delta_file',
    short: '',
    long: '--delta-file',
    description: 'Path to weight delta file for submission',
    required: false,
    type: 'string',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter rounds by status: all, collecting, training, aggregating, complete',
    required: false,
    default: 'all',
    type: 'string',
  },
  {
    name: 'reason',
    short: '',
    long: '--reason',
    description: 'Reason for cancelling a training round',
    required: false,
    type: 'string',
  },
  {
    name: 'teacher',
    short: '',
    long: '--teacher',
    description: 'Teacher model for knowledge distillation',
    required: false,
    type: 'string',
  },
  {
    name: 'student',
    short: '',
    long: '--student',
    description: 'Student model for knowledge distillation',
    required: false,
    type: 'string',
  },
  {
    name: 'temperature',
    short: '',
    long: '--temperature',
    description: 'Temperature for knowledge distillation (default: 4.0)',
    required: false,
    default: '4.0',
    type: 'number',
  },
  {
    name: 'alpha',
    short: '',
    long: '--alpha',
    description: 'Distillation loss weight alpha (default: 0.5)',
    required: false,
    default: '0.5',
    type: 'number',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const trainCommand: Command = {
  name: 'train',
  description: 'Manage federated training rounds: start, join, monitor, aggregate, and distill',
  aliases: ['federated', 'fl'],
  options: trainOptions,
  action: trainAction,
};
