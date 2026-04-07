/**
 * CLI command: model-registry
 *
 * Enhanced model management: list, info, search, versions, compare,
 * recommend, popular, and lineage subcommands.
 *
 * Usage:
 *   xergon model list --task code --sort rating --limit 20
 *   xergon model info <id>
 *   xergon model search <query>
 *   xergon model versions <id>
 *   xergon model compare <id1> <id2>
 *   xergon model recommend --task code --budget 10
 *   xergon model popular
 *   xergon model lineage <id>
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  listModels,
  getModel,
  searchModels,
  getModelVersions,
  compareModels,
  getRecommended,
  getPopularModels,
  getModelLineage,
  getDeprecationNotice,
  type ModelInfo,
  type ModelComparison,
  type ModelRecommendation,
} from '../../model-registry';

// ── Options ────────────────────────────────────────────────────────

const modelRegistryOptions: CommandOption[] = [
  {
    name: 'task',
    short: '',
    long: '--task',
    description: 'Filter by task type (code, chat, embedding, vision, etc.)',
    required: false,
    type: 'string',
  },
  {
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Filter by provider name',
    required: false,
    type: 'string',
  },
  {
    name: 'sort',
    short: '',
    long: '--sort',
    description: 'Sort field (id, name, pricing, contextLength)',
    required: false,
    default: 'id',
    type: 'string',
  },
  {
    name: 'direction',
    short: '',
    long: '--direction',
    description: 'Sort direction (asc or desc)',
    required: false,
    default: 'asc',
    type: 'string',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Maximum number of results',
    required: false,
    default: '20',
    type: 'number',
  },
  {
    name: 'offset',
    short: '',
    long: '--offset',
    description: 'Pagination offset',
    required: false,
    type: 'number',
  },
  {
    name: 'budget',
    short: '',
    long: '--budget',
    description: 'Maximum budget per token for recommendations',
    required: false,
    type: 'number',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter by status (active, inactive, deprecated)',
    required: false,
    type: 'string',
  },
  {
    name: 'quantization',
    short: '',
    long: '--quantization',
    description: 'Filter by quantization (4-bit, 8-bit, fp16, etc.)',
    required: false,
    type: 'string',
  },
];

// ── Helpers ────────────────────────────────────────────────────────

function formatModelTable(models: ModelInfo[], output: any): string {
  if (models.length === 0) return 'No models found.\n';

  const tableData = models.map(m => ({
    ID: m.id.length > 30 ? m.id.substring(0, 27) + '...' : m.id,
    Task: m.task,
    Provider: m.provider,
    Context: String(m.contextLength),
    Price: `${m.pricing.perToken}`,
    Status: m.status,
  }));

  return output.formatTable(tableData, `Models (${models.length})`);
}

function formatModelDetail(model: ModelInfo, output: any): string {
  const lines: string[] = [];
  lines.push(output.colorize('Model Details', 'bold'));
  lines.push('');

  const fields: Array<[string, string]> = [
    ['ID', model.id],
    ['Name', model.name],
    ['Provider', model.provider],
    ['Task', model.task],
    ['Status', model.status === 'active'
      ? output.colorize(model.status, 'green')
      : model.status === 'deprecated'
        ? output.colorize(model.status, 'red')
        : output.colorize(model.status, 'yellow')],
    ['Context Length', `${model.contextLength.toLocaleString()} tokens`],
    ['Max Output', `${model.maxOutputTokens.toLocaleString()} tokens`],
    ['Pricing', `${model.pricing.perToken} ${model.pricing.currency}/token`],
  ];

  if (model.quantization) {
    fields.push(['Quantization', model.quantization]);
  }

  if (model.tags.length > 0) {
    fields.push(['Tags', model.tags.join(', ')]);
  }

  if (model.description) {
    fields.push(['Description', model.description]);
  }

  if (Object.keys(model.benchmarks).length > 0) {
    fields.push(['Benchmarks', Object.entries(model.benchmarks)
      .map(([k, v]) => `${k}: ${v}`)
      .join(', ')]);
  }

  const maxLabel = Math.max(...fields.map(([l]) => l.length));
  for (const [label, value] of fields) {
    const labelStr = output.colorize(`${label}:`.padEnd(maxLabel + 2), 'cyan');
    lines.push(`  ${labelStr} ${value}`);
  }

  lines.push('');
  return lines.join('\n');
}

function formatComparison(comp: ModelComparison, output: any): string {
  const lines: string[] = [];
  lines.push(output.colorize('Model Comparison', 'bold'));
  lines.push('');

  // Header
  const m1Name = output.colorize(comp.model1.id ?? 'N/A', 'cyan');
  const m2Name = output.colorize(comp.model2.id ?? 'N/A', 'cyan');
  lines.push(`  ${m1Name}  vs  ${m2Name}`);
  lines.push('');

  if (comp.differences.length === 0) {
    lines.push('  These models are identical in all tracked attributes.');
  } else {
    lines.push(output.colorize('Differences:', 'bold'));
    for (const diff of comp.differences) {
      const field = output.colorize(`  ${diff.field}:`, 'yellow');
      const left = String(diff.left ?? 'N/A');
      const right = String(diff.right ?? 'N/A');
      lines.push(`${field} ${left}  <->  ${right}`);
    }
  }

  if (comp.recommendation) {
    lines.push('');
    lines.push(output.colorize('Recommendation:', 'green'));
    lines.push(`  ${comp.recommendation}`);
  }

  lines.push('');
  return lines.join('\n');
}

function formatRecommendations(recs: ModelRecommendation[], output: any): string {
  if (recs.length === 0) return 'No recommendations found.\n';

  const tableData = recs.map(r => ({
    Model: r.modelId,
    Score: String(r.score),
    Reason: r.reason.length > 60 ? r.reason.substring(0, 57) + '...' : r.reason,
    Cost: r.estimatedCost !== undefined ? String(r.estimatedCost) : '-',
  }));

  return output.formatTable(tableData, 'Recommended Models');
}

// ── Subcommand Handlers ───────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { models } = await listModels(
    ctx.client.models as any,
    {
      task: args.options.task as string | undefined,
      provider: args.options.provider as string | undefined,
      status: args.options.status as any,
      quantization: args.options.quantization as string | undefined,
    },
    {
      field: String(args.options.sort ?? 'id'),
      direction: (String(args.options.direction ?? 'asc') as 'asc' | 'desc'),
    },
    {
      offset: args.options.offset as number | undefined,
      limit: args.options.limit as number | undefined,
    },
  );

  ctx.output.write(formatModelTable(models, ctx.output));
}

async function handleInfo(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[0];
  if (!modelId) {
    ctx.output.writeError('Usage: xergon model info <model-id>');
    process.exit(1);
    return;
  }

  const model = await getModel(ctx.client.models as any, modelId);
  if (!model) {
    ctx.output.writeError(`Model not found: ${modelId}`);
    process.exit(1);
    return;
  }

  ctx.output.write(formatModelDetail(model, ctx.output));
}

async function handleSearch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const query = args.positional[0];
  if (!query) {
    ctx.output.writeError('Usage: xergon model search <query>');
    process.exit(1);
    return;
  }

  const models = await searchModels(ctx.client.models as any, query, {
    limit: args.options.limit as number | undefined,
  });

  ctx.output.write(formatModelTable(models, ctx.output));
}

async function handleVersions(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[0];
  if (!modelId) {
    ctx.output.writeError('Usage: xergon model versions <model-id>');
    process.exit(1);
    return;
  }

  const versions = await getModelVersions(ctx.client.models as any, modelId);
  if (versions.length === 0) {
    ctx.output.info(`No version history available for "${modelId}".`);
    return;
  }

  const tableData = versions.map(v => ({
    Version: v.version,
    Published: v.publishedAt,
    Deprecated: v.deprecated ? 'Yes' : 'No',
    Changelog: v.changelog.length > 50 ? v.changelog.substring(0, 47) + '...' : v.changelog,
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Version History: ${modelId}`));
}

async function handleCompare(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const id1 = args.positional[0];
  const id2 = args.positional[1];
  if (!id1 || !id2) {
    ctx.output.writeError('Usage: xergon model compare <model-id-1> <model-id-2>');
    process.exit(1);
    return;
  }

  const comp = await compareModels(ctx.client.models as any, id1, id2);
  if (!comp) {
    ctx.output.writeError(`Could not find one or both models: ${id1}, ${id2}`);
    process.exit(1);
    return;
  }

  ctx.output.write(formatComparison(comp, ctx.output));
}

async function handleRecommend(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const task = args.options.task as string;
  if (!task) {
    ctx.output.writeError('Usage: xergon model recommend --task <task-type> [--budget <amount>]');
    process.exit(1);
    return;
  }

  const recs = await getRecommended(
    ctx.client.models as any,
    task,
    args.options.budget as number | undefined,
  );

  ctx.output.write(formatRecommendations(recs, ctx.output));
}

async function handlePopular(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const limit = args.options.limit as number ?? 10;
  const models = await getPopularModels(ctx.client.models as any, limit);
  ctx.output.write(formatModelTable(models, ctx.output));
}

async function handleLineage(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[0];
  if (!modelId) {
    ctx.output.writeError('Usage: xergon model lineage <model-id>');
    process.exit(1);
    return;
  }

  const lineage = await getModelLineage(ctx.client.models as any, modelId);
  if (lineage.length === 0) {
    ctx.output.info(`No lineage information available for "${modelId}".`);
    return;
  }

  const tableData = lineage.map(n => ({
    Model: n.modelId,
    Relationship: n.relationship,
    Version: n.version ?? '-',
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Lineage: ${modelId}`));
}

async function handleDeprecation(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[0];
  if (!modelId) {
    ctx.output.writeError('Usage: xergon model deprecation <model-id>');
    process.exit(1);
    return;
  }

  const result = await getDeprecationNotice(ctx.client.models as any, modelId);
  if (!result) {
    ctx.output.writeError(`Model not found: ${modelId}`);
    process.exit(1);
    return;
  }

  if (result.deprecated) {
    ctx.output.warn(result.notice ?? 'This model has been deprecated.');
    if (result.migration) {
      ctx.output.info(`Migration: ${result.migration}`);
    }
  } else {
    ctx.output.success(`Model "${modelId}" is active and not deprecated.`);
  }
}

// ── Command Definition ─────────────────────────────────────────────

async function modelRegistryAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  switch (sub) {
    case 'list':
      await handleList(args, ctx);
      break;
    case 'info':
      await handleInfo(args, ctx);
      break;
    case 'search':
      await handleSearch(args, ctx);
      break;
    case 'versions':
      await handleVersions(args, ctx);
      break;
    case 'compare':
      await handleCompare(args, ctx);
      break;
    case 'recommend':
      await handleRecommend(args, ctx);
      break;
    case 'popular':
      await handlePopular(args, ctx);
      break;
    case 'lineage':
      await handleLineage(args, ctx);
      break;
    case 'deprecation':
      await handleDeprecation(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub ?? '(none)'}`);
      ctx.output.info('Available subcommands: list, info, search, versions, compare, recommend, popular, lineage, deprecation');
      process.exit(1);
      break;
  }
}

export const modelRegistryCommand: Command = {
  name: 'model',
  description: 'Enhanced model registry: search, compare, recommend, versioning',
  aliases: ['model-registry'],
  options: modelRegistryOptions,
  action: modelRegistryAction,
};
