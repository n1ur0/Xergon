/**
 * CLI command: canary
 *
 * Manage canary deployments for progressive model rollout.
 *
 * Usage:
 *   xergon canary start --model X --canary Y --percentage 10
 *   xergon canary status [id]
 *   xergon canary promote [id]
 *   xergon canary rollback [id]
 *   xergon canary list
 *   xergon canary history
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';

async function canaryAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon canary <start|status|promote|rollback|list|history> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'start':
      await handleStart(args, ctx);
      break;
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'promote':
      await handlePromote(args, ctx);
      break;
    case 'rollback':
      await handleRollback(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'history':
      await handleHistory(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Usage: xergon canary <start|status|promote|rollback|list|history> [options]');
      process.exit(1);
  }
}

// ── start ──────────────────────────────────────────────────────────

async function handleStart(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = String(args.options.model || '');
  const canaryModel = String(args.options.canary || '');
  const percentage = args.options.percentage !== undefined ? Number(args.options.percentage) : 10;
  const successThreshold = args.options.success_threshold !== undefined ? Number(args.options.success_threshold) : 0.95;
  const errorThreshold = args.options.error_threshold !== undefined ? Number(args.options.error_threshold) : 0.05;
  const minRequests = args.options.min_requests !== undefined ? Number(args.options.min_requests) : 100;
  const duration = args.options.duration !== undefined ? Number(args.options.duration) : 60;
  const autoPromote = args.options.auto_promote === true;
  const autoRollback = args.options.auto_rollback !== false; // default true

  if (!model) {
    ctx.output.writeError('Missing required option: --model');
    process.exit(1);
    return;
  }

  if (!canaryModel) {
    ctx.output.writeError('Missing required option: --canary');
    process.exit(1);
    return;
  }

  if (percentage < 0 || percentage > 100) {
    ctx.output.writeError('Percentage must be between 0 and 100');
    process.exit(1);
    return;
  }

  const { startCanary, saveCanaryToHistory } = await import('../../canary');

  const canary = startCanary({
    model,
    canaryModel,
    canaryPercentage: percentage,
    successThreshold,
    errorThreshold,
    minRequests,
    duration,
    autoPromote,
    autoRollback,
  });

  ctx.output.success(`Canary deployment started: ${canary.id}`);
  ctx.output.write('');
  ctx.output.write(ctx.output.formatText({
    ID: canary.id,
    Baseline: canary.model,
    Canary: canary.canaryModel,
    'Traffic %': `${canary.canaryPercentage}%`,
    'Success Threshold': `${(canary.successThreshold * 100).toFixed(0)}%`,
    'Error Threshold': `${(canary.errorThreshold * 100).toFixed(0)}%`,
    'Min Requests': canary.minRequests,
    Duration: `${canary.duration} min`,
    'Auto Promote': canary.autoPromote,
    'Auto Rollback': canary.autoRollback,
    Started: canary.startedAt,
  }, 'Canary Deployment'));
}

// ── status ─────────────────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const canaryId = args.positional[1];

  if (!canaryId) {
    // Show all active canaries
    const { listCanaries } = await import('../../canary');
    const canaries = listCanaries().filter(c => c.metrics.status === 'running');

    if (canaries.length === 0) {
      ctx.output.info('No active canary deployments.');
      return;
    }

    const tableData = canaries.map(c => ({
      ID: c.id,
      Baseline: c.model,
      Canary: c.canaryModel,
      'Traffic %': `${c.canaryPercentage}%`,
      Requests: c.metrics.totalRequests,
      'Canary Rate': c.metrics.canaryRequests,
      'Status': c.metrics.status,
    }));

    ctx.output.write(ctx.output.formatTable(tableData, `Active Canaries (${canaries.length})`));
    return;
  }

  const { checkCanary } = await import('../../canary');

  try {
    const result = checkCanary(canaryId);

    ctx.output.write(ctx.output.formatText({
      ID: result.id,
      Baseline: result.model,
      Canary: result.canaryModel,
      Status: result.status,
      Recommendation: result.recommendation,
      Reason: result.reason,
      'Total Requests': result.metrics.totalRequests,
      'Canary Requests': result.metrics.canaryRequests,
      'Baseline Success': `${(result.metrics.baselineSuccessRate * 100).toFixed(1)}%`,
      'Canary Success': `${(result.metrics.canarySuccessRate * 100).toFixed(1)}%`,
      'Baseline P50': `${result.metrics.baselineLatencyP50.toFixed(0)}ms`,
      'Canary P50': `${result.metrics.canaryLatencyP50.toFixed(0)}ms`,
    }, `Canary Status: ${canaryId}`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── promote ────────────────────────────────────────────────────────

async function handlePromote(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const canaryId = args.positional[1];

  if (!canaryId) {
    ctx.output.writeError('Usage: xergon canary promote <id>');
    process.exit(1);
    return;
  }

  const { promoteCanary, saveCanaryToHistory } = await import('../../canary');

  try {
    const deployment = promoteCanary(canaryId);
    saveCanaryToHistory(deployment);
    ctx.output.success(`Canary ${canaryId} promoted to full deployment`);
    ctx.output.write(`  Model: ${deployment.canaryModel} is now the active model`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── rollback ───────────────────────────────────────────────────────

async function handleRollback(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const canaryId = args.positional[1];

  if (!canaryId) {
    ctx.output.writeError('Usage: xergon canary rollback <id>');
    process.exit(1);
    return;
  }

  const { rollbackCanary, saveCanaryToHistory } = await import('../../canary');

  try {
    const deployment = rollbackCanary(canaryId);
    saveCanaryToHistory(deployment);
    ctx.output.success(`Canary ${canaryId} rolled back to baseline`);
    ctx.output.write(`  Baseline model: ${deployment.model}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { listCanaries } = await import('../../canary');
  const canaries = listCanaries();

  if (canaries.length === 0) {
    ctx.output.info('No canary deployments found.');
    return;
  }

  const tableData = canaries.map(c => ({
    ID: c.id,
    Baseline: c.model,
    Canary: c.canaryModel,
    'Traffic %': `${c.canaryPercentage}%`,
    Requests: c.metrics.totalRequests,
    Status: c.metrics.status,
    Started: new Date(c.startedAt).toISOString().slice(0, 19),
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Canary Deployments (${canaries.length})`));
}

// ── history ────────────────────────────────────────────────────────

async function handleHistory(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { loadCanaryHistory } = await import('../../canary');
  const history = loadCanaryHistory();

  if (history.length === 0) {
    ctx.output.info('No canary history found.');
    return;
  }

  const tableData = history.map(h => ({
    ID: h.id,
    Baseline: h.model,
    Canary: h.canaryModel,
    'Traffic %': `${h.canaryPercentage}%`,
    Requests: h.metrics.totalRequests,
    Status: h.status,
    Started: new Date(h.startedAt).toISOString().slice(0, 19),
    Ended: h.endedAt ? new Date(h.endedAt).toISOString().slice(0, 19) : '-',
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Canary History (${history.length})`));
}

export const canaryCommand: Command = {
  name: 'canary',
  description: 'Manage canary deployments for progressive model rollout',
  aliases: ['canaries'],
  options: [
    {
      name: 'model',
      short: '-m',
      long: '--model',
      description: 'Baseline model',
      required: false,
      type: 'string',
    },
    {
      name: 'canary',
      short: '',
      long: '--canary',
      description: 'Canary model to test',
      required: false,
      type: 'string',
    },
    {
      name: 'percentage',
      short: '-p',
      long: '--percentage',
      description: 'Traffic percentage to canary (0-100, default: 10)',
      required: false,
      type: 'number',
    },
    {
      name: 'success_threshold',
      short: '',
      long: '--success-threshold',
      description: 'Min success rate to promote (default: 0.95)',
      required: false,
      type: 'number',
    },
    {
      name: 'error_threshold',
      short: '',
      long: '--error-threshold',
      description: 'Max error rate before rollback (default: 0.05)',
      required: false,
      type: 'number',
    },
    {
      name: 'min_requests',
      short: '',
      long: '--min-requests',
      description: 'Min requests before evaluation (default: 100)',
      required: false,
      type: 'number',
    },
    {
      name: 'duration',
      short: '',
      long: '--duration',
      description: 'Max canary duration in minutes (default: 60)',
      required: false,
      type: 'number',
    },
    {
      name: 'auto_promote',
      short: '',
      long: '--auto-promote',
      description: 'Auto-promote if thresholds met',
      required: false,
      type: 'boolean',
    },
    {
      name: 'auto_rollback',
      short: '',
      long: '--auto-rollback',
      description: 'Auto-rollback on error threshold (default: true)',
      required: false,
      type: 'boolean',
    },
  ],
  action: canaryAction,
};
