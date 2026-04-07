/**
 * CLI command: ensemble
 *
 * Manage ensemble model groups on the Xergon Network.
 * List, create, get, update, delete ensemble groups, route prompts
 * through ensembles, view history and stats, and manage A/B weights.
 *
 * Usage:
 *   xergon ensemble list [--strategy <type>] [--enabled true|false]
 *   xergon ensemble create --name <name> --models <id1,id2,...> [--strategy <type>] [--weights <w1,w2,...>]
 *   xergon ensemble get --id <group-id>
 *   xergon ensemble update --id <group-id> [--name <name>] [--models <ids>] [--strategy <type>] [--weights <w1,w2,...>] [--enable|--disable]
 *   xergon ensemble delete --id <group-id>
 *   xergon ensemble route --id <group-id> --prompt <text> [--temperature <n>] [--max-tokens <n>]
 *   xergon ensemble history [--limit <n>]
 *   xergon ensemble stats
 *   xergon ensemble weights --id <group-id> --model <model-id> --weight <n>
 *   xergon ensemble config --timeout <ms> --max-fanout <n> --strategy <type> --threshold <f> --fallback true|false
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

interface EnsembleGroup {
  id: string;
  name: string;
  model_ids: string[];
  strategy: string;
  weights: Record<string, number>;
  enabled: boolean;
  created_at: string;
}

interface AggregatedResponse {
  request_id: string;
  final_text: string;
  confidence: number;
  individual_responses: Array<{
    model_id: string;
    text: string;
    latency_ms: number;
    tokens: number;
  }>;
  aggregation_method: string;
  total_latency_ms: number;
  total_tokens: number;
  fallback_used: boolean;
  created_at: string;
}

interface EnsembleStats {
  total_groups: number;
  active_groups: number;
  total_requests: number;
  avg_confidence: number;
  avg_latency_ms: number;
  fallback_rate: number;
  top_strategy: string;
}

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

// ── Subcommand: list ───────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const strategyFilter = args.options.strategy ? String(args.options.strategy) : undefined;
  const enabledFilter = args.options.enabled !== undefined ? String(args.options.enabled) === 'true' : undefined;

  try {
    let groups: EnsembleGroup[];

    if (ctx.client?.ensemble?.list) {
      groups = await ctx.client.ensemble.list({ strategy: strategyFilter, enabled: enabledFilter });
    } else {
      groups = [];
    }

    if (strategyFilter && groups.length > 0) {
      groups = groups.filter(g => g.strategy === strategyFilter);
    }
    if (enabledFilter !== undefined && groups.length > 0) {
      groups = groups.filter(g => g.enabled === enabledFilter);
    }

    if (groups.length === 0) {
      ctx.output.info('No ensemble groups found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(groups, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = groups.map(g => ({
        ID: g.id.substring(0, 12) + '...',
        Name: g.name,
        Models: g.model_ids.join(', '),
        Strategy: g.strategy,
        Enabled: g.enabled ? 'Yes' : 'No',
        Created: g.created_at ? new Date(g.created_at).toISOString().slice(0, 10) : '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Ensemble Groups (${groups.length})`));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Ensemble Groups (${groups.length})`, 'bold'));
    ctx.output.write('');
    for (const g of groups) {
      const statusColor = g.enabled ? 'green' : 'dim';
      ctx.output.write(`  ${ctx.output.colorize(g.id.substring(0, 16) + '...', 'cyan')}  ${ctx.output.colorize(g.enabled ? 'ACTIVE' : 'DISABLED', statusColor)}`);
      ctx.output.write(`    ${g.name}  |  Strategy: ${g.strategy}`);
      ctx.output.write(`    Models: ${g.model_ids.join(', ')}`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list ensemble groups: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: create ─────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.options.name ? String(args.options.name) : undefined;
  const modelsStr = args.options.models ? String(args.options.models) : undefined;
  const strategy = args.options.strategy ? String(args.options.strategy) : undefined;
  const weightsStr = args.options.weights ? String(args.options.weights) : undefined;

  if (!name) {
    ctx.output.writeError('Usage: xergon ensemble create --name <name> --models <id1,id2,...> [--strategy <type>] [--weights <w1,w2,...>]');
    process.exit(1);
    return;
  }
  if (!modelsStr) {
    ctx.output.writeError('--models is required. Provide comma-separated model IDs.');
    process.exit(1);
    return;
  }

  const model_ids = modelsStr.split(',').map(m => m.trim()).filter(Boolean);
  if (model_ids.length < 2) {
    ctx.output.writeError('At least 2 models are required for an ensemble group.');
    process.exit(1);
    return;
  }

  let weights: Record<string, number> | undefined;
  if (weightsStr) {
    const weightValues = weightsStr.split(',').map(w => parseFloat(w.trim()));
    if (weightValues.length !== model_ids.length) {
      ctx.output.writeError(`Number of weights (${weightValues.length}) must match number of models (${model_ids.length}).`);
      process.exit(1);
      return;
    }
    weights = {};
    for (let i = 0; i < model_ids.length; i++) {
      weights[model_ids[i]] = weightValues[i];
    }
  }

  const input = {
    name,
    model_ids,
    strategy: strategy || 'weighted_average',
    weights,
  };

  ctx.output.info('Creating ensemble group...');

  try {
    let group: EnsembleGroup;

    if (ctx.client?.ensemble?.create) {
      group = await ctx.client.ensemble.create(input);
    } else {
      throw new Error('Ensemble client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(group, null, 2));
      return;
    }

    ctx.output.success('Ensemble group created successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Group ID': group.id,
      Name: group.name,
      Models: group.model_ids.join(', '),
      Strategy: group.strategy,
      Weights: group.weights ? JSON.stringify(group.weights) : 'uniform',
      Enabled: String(group.enabled),
      'Created At': group.created_at,
    }, 'New Ensemble Group'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create ensemble group: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: get ────────────────────────────────────────────────

async function handleGet(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.options.id ? String(args.options.id) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon ensemble get --id <group-id>');
    process.exit(1);
    return;
  }

  try {
    let group: EnsembleGroup;

    if (ctx.client?.ensemble?.get) {
      group = await ctx.client.ensemble.get(id);
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(group, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Ensemble Group Details', 'bold'));
    ctx.output.write(ctx.output.colorize('-'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Group ID': group.id,
      Name: group.name,
      Models: group.model_ids.join(', '),
      Strategy: group.strategy,
      Weights: group.weights ? JSON.stringify(group.weights) : 'uniform',
      Enabled: group.enabled ? 'Yes' : 'No',
      'Created At': group.created_at,
    }));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get ensemble group: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: update ─────────────────────────────────────────────

async function handleUpdate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.options.id ? String(args.options.id) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon ensemble update --id <group-id> [--name <name>] [--models <ids>] [--strategy <type>] [--weights <w1,w2,...>] [--enable|--disable]');
    process.exit(1);
    return;
  }

  const updates: Record<string, unknown> = {};

  if (args.options.name) updates.name = String(args.options.name);
  if (args.options.models) updates.model_ids = String(args.options.models).split(',').map(m => m.trim()).filter(Boolean);
  if (args.options.strategy) updates.strategy = String(args.options.strategy);
  if (args.options.weights) {
    const weightsStr = String(args.options.weights);
    updates.weights = weightsStr.split(',').map(w => parseFloat(w.trim()));
  }
  if (args.options.enable) updates.enabled = true;
  if (args.options.disable) updates.enabled = false;

  if (Object.keys(updates).length === 0) {
    ctx.output.writeError('No updates specified. Provide at least one of: --name, --models, --strategy, --weights, --enable, --disable');
    process.exit(1);
    return;
  }

  ctx.output.info(`Updating ensemble group ${id.substring(0, 16)}...`);

  try {
    let group: EnsembleGroup;

    if (ctx.client?.ensemble?.update) {
      group = await ctx.client.ensemble.update(id, updates);
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(group, null, 2));
      return;
    }

    ctx.output.success('Ensemble group updated successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Group ID': group.id,
      Name: group.name,
      Models: group.model_ids.join(', '),
      Strategy: group.strategy,
      Enabled: group.enabled ? 'Yes' : 'No',
    }, 'Updated Ensemble Group'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to update ensemble group: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: delete ─────────────────────────────────────────────

async function handleDelete(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.options.id ? String(args.options.id) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon ensemble delete --id <group-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Deleting ensemble group ${id.substring(0, 16)}...`);

  try {
    if (ctx.client?.ensemble?.delete) {
      await ctx.client.ensemble.delete(id);
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ id, deleted: true }, null, 2));
      return;
    }

    ctx.output.success(`Ensemble group ${id.substring(0, 16)}... deleted successfully`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to delete ensemble group: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: route ──────────────────────────────────────────────

async function handleRoute(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.options.id ? String(args.options.id) : undefined;
  const prompt = args.options.prompt ? String(args.options.prompt) : undefined;
  const temperature = args.options.temperature !== undefined ? Number(args.options.temperature) : undefined;
  const maxTokens = args.options.max_tokens !== undefined ? Number(args.options.max_tokens) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon ensemble route --id <group-id> --prompt <text> [--temperature <n>] [--max-tokens <n>]');
    process.exit(1);
    return;
  }
  if (!prompt) {
    ctx.output.writeError('--prompt is required.');
    process.exit(1);
    return;
  }

  const input = {
    prompt,
    temperature,
    max_tokens: maxTokens,
  };

  ctx.output.info(`Routing prompt through ensemble ${id.substring(0, 16)}...`);

  try {
    let response: AggregatedResponse;

    if (ctx.client?.ensemble?.route) {
      response = await ctx.client.ensemble.route(id, input);
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(response, null, 2));
      return;
    }

    ctx.output.success('Ensemble response received');
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Response', 'bold'));
    ctx.output.write(ctx.output.colorize('-'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Request ID': response.request_id,
      'Final Text': response.final_text.length > 200 ? response.final_text.substring(0, 200) + '...' : response.final_text,
      Confidence: `${(response.confidence * 100).toFixed(1)}%`,
      'Aggregation': response.aggregation_method,
      'Latency': `${response.total_latency_ms}ms`,
      Tokens: String(response.total_tokens),
      'Fallback Used': response.fallback_used ? 'Yes' : 'No',
    }));

    if (response.individual_responses && response.individual_responses.length > 0) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize('Individual Responses', 'bold'));
      ctx.output.write(ctx.output.colorize('-'.repeat(40), 'dim'));
      for (const r of response.individual_responses) {
        ctx.output.write(`  ${ctx.output.colorize(r.model_id, 'cyan')}  ${r.latency_ms}ms  ${r.tokens} tokens`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to route prompt: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: history ────────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const limit = args.options.limit ? Number(args.options.limit) : 20;

  try {
    let history: AggregatedResponse[];

    if (ctx.client?.ensemble?.history) {
      history = await ctx.client.ensemble.history({ limit });
    } else {
      history = [];
    }

    if (history.length === 0) {
      ctx.output.info('No ensemble request history found.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(history, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = history.map(h => ({
        'Request ID': h.request_id.substring(0, 12) + '...',
        Confidence: `${(h.confidence * 100).toFixed(1)}%`,
        Method: h.aggregation_method,
        Latency: `${h.total_latency_ms}ms`,
        Tokens: String(h.total_tokens),
        Fallback: h.fallback_used ? 'Yes' : 'No',
        Time: h.created_at ? new Date(h.created_at).toISOString().slice(0, 19) : '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Ensemble History (${history.length})`));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Ensemble Request History (${history.length})`, 'bold'));
    ctx.output.write('');
    for (const h of history) {
      ctx.output.write(`  ${ctx.output.colorize(h.request_id.substring(0, 16) + '...', 'cyan')}  ${(h.confidence * 100).toFixed(1)}%  ${h.total_latency_ms}ms`);
      ctx.output.write(`    Method: ${h.aggregation_method}  |  Tokens: ${h.total_tokens}  |  Fallback: ${h.fallback_used ? 'Yes' : 'No'}`);
      const preview = h.final_text.length > 80 ? h.final_text.substring(0, 80) + '...' : h.final_text;
      ctx.output.write(`    ${preview}`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get ensemble history: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: stats ──────────────────────────────────────────────

async function handleStats(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let stats: EnsembleStats;

    if (ctx.client?.ensemble?.stats) {
      stats = await ctx.client.ensemble.stats();
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(stats, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Ensemble Statistics', 'bold'));
    ctx.output.write(ctx.output.colorize('-'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Total Groups': String(stats.total_groups),
      'Active Groups': String(stats.active_groups),
      'Total Requests': String(stats.total_requests),
      'Avg Confidence': `${(stats.avg_confidence * 100).toFixed(1)}%`,
      'Avg Latency': `${stats.avg_latency_ms.toFixed(0)}ms`,
      'Fallback Rate': `${(stats.fallback_rate * 100).toFixed(1)}%`,
      'Top Strategy': stats.top_strategy,
    }));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get ensemble stats: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: weights ────────────────────────────────────────────

async function handleWeights(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id = args.options.id ? String(args.options.id) : undefined;
  const model = args.options.model ? String(args.options.model) : undefined;
  const weight = args.options.weight !== undefined ? Number(args.options.weight) : undefined;

  if (!id) {
    ctx.output.writeError('Usage: xergon ensemble weights --id <group-id> --model <model-id> --weight <n>');
    process.exit(1);
    return;
  }
  if (!model) {
    ctx.output.writeError('--model is required.');
    process.exit(1);
    return;
  }
  if (weight === undefined || isNaN(weight)) {
    ctx.output.writeError('--weight is required and must be a number.');
    process.exit(1);
    return;
  }

  ctx.output.info(`Setting weight ${weight} for model ${model} in ensemble ${id.substring(0, 16)}...`);

  try {
    let result: EnsembleGroup;

    if (ctx.client?.ensemble?.setWeight) {
      result = await ctx.client.ensemble.setWeight(id, model, weight);
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success(`Weight updated successfully`);
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Group ID': result.id,
      Name: result.name,
      'Updated Model': model,
      'New Weight': String(weight),
      'All Weights': JSON.stringify(result.weights),
    }, 'A/B Weight Configuration'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to update weight: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: config ─────────────────────────────────────────────

async function handleConfig(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const config: Record<string, unknown> = {};

  if (args.options.timeout !== undefined) config.timeout = Number(args.options.timeout);
  if (args.options.max_fanout !== undefined) config.max_fanout = Number(args.options.max_fanout);
  if (args.options.strategy !== undefined) config.strategy = String(args.options.strategy);
  if (args.options.threshold !== undefined) config.threshold = Number(args.options.threshold);
  if (args.options.fallback !== undefined) config.fallback = String(args.options.fallback) === 'true';

  if (Object.keys(config).length === 0) {
    ctx.output.writeError('Usage: xergon ensemble config --timeout <ms> --max-fanout <n> --strategy <type> --threshold <f> --fallback true|false');
    ctx.output.writeError('Provide at least one configuration option.');
    process.exit(1);
    return;
  }

  ctx.output.info('Updating ensemble configuration...');

  try {
    let result: Record<string, unknown>;

    if (ctx.client?.ensemble?.config) {
      result = await ctx.client.ensemble.config(config);
    } else {
      throw new Error('Ensemble client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Ensemble configuration updated successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      ...Object.fromEntries(Object.entries(config).map(([k, v]) => [k, String(v)])),
      ...(typeof result === 'object' && result !== null ? Object.fromEntries(Object.entries(result).map(([k, v]) => [k, String(v)])) : {}),
    }, 'Ensemble Configuration'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to update ensemble config: ${message}`);
    process.exit(1);
  }
}

// ── Action dispatcher ──────────────────────────────────────────────

async function ensembleAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon ensemble <subcommand> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  list       List ensemble groups');
    ctx.output.write('  create     Create a new ensemble group');
    ctx.output.write('  get        Get details of an ensemble group');
    ctx.output.write('  update     Update an ensemble group');
    ctx.output.write('  delete     Delete an ensemble group');
    ctx.output.write('  route      Route a prompt through an ensemble');
    ctx.output.write('  history    View ensemble request history');
    ctx.output.write('  stats      View ensemble statistics');
    ctx.output.write('  weights    Set A/B weights for a model in a group');
    ctx.output.write('  config     Update ensemble configuration');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'create':
      await handleCreate(args, ctx);
      break;
    case 'get':
      await handleGet(args, ctx);
      break;
    case 'update':
      await handleUpdate(args, ctx);
      break;
    case 'delete':
      await handleDelete(args, ctx);
      break;
    case 'route':
      await handleRoute(args, ctx);
      break;
    case 'history':
      await handleHistory(args, ctx);
      break;
    case 'stats':
      await handleStats(args, ctx);
      break;
    case 'weights':
      await handleWeights(args, ctx);
      break;
    case 'config':
      await handleConfig(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: list, create, get, update, delete, route, history, stats, weights, config');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const ensembleOptions: CommandOption[] = [
  {
    name: 'id',
    short: '',
    long: '--id',
    description: 'Ensemble group ID',
    required: false,
    type: 'string',
  },
  {
    name: 'name',
    short: '',
    long: '--name',
    description: 'Ensemble group name',
    required: false,
    type: 'string',
  },
  {
    name: 'models',
    short: '',
    long: '--models',
    description: 'Comma-separated model IDs for the ensemble',
    required: false,
    type: 'string',
  },
  {
    name: 'strategy',
    short: '',
    long: '--strategy',
    description: 'Aggregation strategy: weighted_average, majority_vote, confidence_routing, random',
    required: false,
    type: 'string',
  },
  {
    name: 'weights',
    short: '',
    long: '--weights',
    description: 'Comma-separated weights corresponding to model IDs',
    required: false,
    type: 'string',
  },
  {
    name: 'prompt',
    short: '',
    long: '--prompt',
    description: 'Prompt text to route through the ensemble',
    required: false,
    type: 'string',
  },
  {
    name: 'temperature',
    short: '',
    long: '--temperature',
    description: 'Sampling temperature for generation',
    required: false,
    type: 'number',
  },
  {
    name: 'max_tokens',
    short: '',
    long: '--max-tokens',
    description: 'Maximum tokens to generate',
    required: false,
    type: 'number',
  },
  {
    name: 'enable',
    short: '',
    long: '--enable',
    description: 'Enable the ensemble group',
    required: false,
    type: 'boolean',
  },
  {
    name: 'disable',
    short: '',
    long: '--disable',
    description: 'Disable the ensemble group',
    required: false,
    type: 'boolean',
  },
  {
    name: 'timeout',
    short: '',
    long: '--timeout',
    description: 'Ensemble timeout in milliseconds',
    required: false,
    type: 'number',
  },
  {
    name: 'max_fanout',
    short: '',
    long: '--max-fanout',
    description: 'Maximum number of concurrent model requests',
    required: false,
    type: 'number',
  },
  {
    name: 'threshold',
    short: '',
    long: '--threshold',
    description: 'Confidence threshold for fallback activation',
    required: false,
    type: 'number',
  },
  {
    name: 'fallback',
    short: '',
    long: '--fallback',
    description: 'Enable or disable fallback mode: true or false',
    required: false,
    type: 'string',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Maximum number of history entries to return',
    required: false,
    type: 'number',
  },
  {
    name: 'model',
    short: '',
    long: '--model',
    description: 'Model ID for weight configuration',
    required: false,
    type: 'string',
  },
  {
    name: 'weight',
    short: '',
    long: '--weight',
    description: 'Weight value for A/B testing',
    required: false,
    type: 'number',
  },
  {
    name: 'enabled',
    short: '',
    long: '--enabled',
    description: 'Filter groups by enabled status: true or false',
    required: false,
    type: 'string',
  },
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
];

// ── Command export ─────────────────────────────────────────────────

export const ensembleCommand: Command = {
  name: 'ensemble',
  description: 'Manage ensemble model groups for multi-model routing',
  aliases: ['ens', 'ensembles'],
  options: ensembleOptions,
  action: ensembleAction,
};
