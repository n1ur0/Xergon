/**
 * CLI command: alias
 *
 * Manage model aliases -- short names that resolve to full model identifiers.
 *
 * Usage:
 *   xergon alias list
 *   xergon alias add <alias> <model> [--provider X]
 *   xergon alias remove <alias>
 *   xergon alias resolve <alias>
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  listAliases,
  addAlias,
  removeAlias,
  resolveAlias,
} from '../../model-alias';

const aliasOptions: CommandOption[] = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
  {
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Provider override for the alias',
    required: false,
    type: 'string',
  },
];

async function aliasAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    // Default: list aliases
    await handleList(args, ctx);
    return;
  }

  switch (sub) {
    case 'list':
    case 'ls':
      await handleList(args, ctx);
      break;
    case 'add':
    case 'create':
    case 'set':
      await handleAdd(args, ctx);
      break;
    case 'remove':
    case 'rm':
    case 'delete':
    case 'unset':
      await handleRemove(args, ctx);
      break;
    case 'resolve':
    case 'get':
    case 'show':
      await handleResolve(args, ctx);
      break;
    default:
      // Could be a resolve shorthand: xergon alias <name>
      await handleResolveDirect(args, ctx);
      break;
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const aliases = listAliases();

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(aliases));
    return;
  }

  const o = ctx.output;

  if (aliases.length === 0) {
    o.info('No aliases configured.');
    o.info('Create one with: xergon alias add <alias> <model>');
    return;
  }

  o.write(o.colorize('Model Aliases', 'bold'));
  o.write(o.colorize('═══════════════════════════════════════════════════════════════════', 'dim'));
  o.write('');

  // Separate built-in and custom
  const builtins = aliases.filter(a => (a as any)._builtin);
  const customs = aliases.filter(a => !(a as any)._builtin);

  if (builtins.length > 0) {
    o.write(o.colorize('  Built-in', 'cyan'));
    for (const a of builtins) {
      const provider = a.provider ? o.colorize(` [${a.provider}]`, 'dim') : '';
      o.write(`    ${o.colorize(a.alias.padEnd(10), 'green')} -> ${a.model}${provider}`);
    }
    o.write('');
  }

  if (customs.length > 0) {
    o.write(o.colorize('  Custom', 'cyan'));
    for (const a of customs) {
      const provider = a.provider ? o.colorize(` [${a.provider}]`, 'dim') : '';
      o.write(`    ${o.colorize(a.alias.padEnd(10), 'yellow')} -> ${a.model}${provider}`);
    }
    o.write('');
  }

  o.info(`${aliases.length} alias(es) configured.`);
}

// ── add ────────────────────────────────────────────────────────────

async function handleAdd(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const alias = args.positional[1];
  const model = args.positional[2];
  const provider = args.options.provider ? String(args.options.provider) : undefined;

  if (!alias || !model) {
    ctx.output.writeError('Usage: xergon alias add <alias> <model> [--provider X]');
    process.exit(1);
    return;
  }

  try {
    addAlias(alias, model, provider);
    const providerStr = provider ? ` with provider "${provider}"` : '';
    ctx.output.success(`Alias "${alias}" -> "${model}"${providerStr} created.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── remove ─────────────────────────────────────────────────────────

async function handleRemove(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const alias = args.positional[1];

  if (!alias) {
    ctx.output.writeError('Usage: xergon alias remove <alias>');
    process.exit(1);
    return;
  }

  try {
    const removed = removeAlias(alias);
    if (removed) {
      ctx.output.success(`Alias "${alias}" removed.`);
    } else {
      ctx.output.writeError(`Alias not found: ${alias}`);
      process.exit(1);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── resolve ────────────────────────────────────────────────────────

async function handleResolve(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const alias = args.positional[1];
  const outputJson = args.options.json === true;

  if (!alias) {
    ctx.output.writeError('Usage: xergon alias resolve <alias>');
    process.exit(1);
    return;
  }

  const resolved = resolveAlias(alias);

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(resolved));
    return;
  }

  const o = ctx.output;
  o.write(o.colorize('Alias Resolution', 'bold'));
  o.write(o.colorize('─────────────────────────────────────────────────────────', 'dim'));
  o.write(`  ${o.colorize('Input:', 'cyan')}    ${alias}`);
  o.write(`  ${o.colorize('Resolved:', 'cyan')} ${resolved.model}`);
  if (resolved.provider) {
    o.write(`  ${o.colorize('Provider:', 'cyan')} ${resolved.provider}`);
  }
  o.write(`  ${o.colorize('Type:', 'cyan')}    ${resolved.isAlias ? 'alias' : 'direct model name'}`);
  o.write('');
}

// ── resolve direct (shorthand: xergon alias <name>) ───────────────

async function handleResolveDirect(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  // Treat first positional as alias name, re-run resolve
  const name = args.positional[0];
  const resolved = resolveAlias(name);
  ctx.output.write(`${resolved.model}\n`);
}

export const aliasCommand: Command = {
  name: 'alias',
  description: 'Manage model aliases for quick model selection',
  aliases: ['aliases'],
  options: aliasOptions,
  action: aliasAction,
};
