/**
 * CLI command: workspace
 *
 * Manage named workspace contexts with environment variables,
 * default model/provider, and path associations.
 *
 * Usage:
 *   xergon workspace create <name>
 *   xergon workspace list
 *   xergon workspace switch <name>
 *   xergon workspace delete <name>
 *   xergon workspace set <key>=<value>
 *   xergon workspace env
 *   xergon workspace info
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  createWorkspace,
  switchWorkspace,
  listWorkspaces,
  deleteWorkspace,
  setWorkspaceVar,
  getWorkspaceVar,
  getCurrentWorkspace,
} from '../../workspace';

const workspaceOptions: CommandOption[] = [
  {
    name: 'path',
    short: '-p',
    long: '--path',
    description: 'Workspace filesystem path (for create)',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output as JSON',
    required: false,
    type: 'boolean',
  },
];

async function workspaceAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    // Default: show current workspace info
    await handleInfo(ctx, args);
    return;
  }

  switch (sub) {
    case 'create':
      await handleCreate(args, ctx);
      break;
    case 'list':
    case 'ls':
      await handleList(args, ctx);
      break;
    case 'switch':
    case 'use':
      await handleSwitch(args, ctx);
      break;
    case 'delete':
    case 'rm':
    case 'remove':
      await handleDelete(args, ctx);
      break;
    case 'set':
      await handleSet(args, ctx);
      break;
    case 'get':
      await handleGet(args, ctx);
      break;
    case 'env':
    case 'environment':
      await handleEnv(args, ctx);
      break;
    case 'info':
    case 'show':
      await handleInfo(ctx, args);
      break;
    default:
      ctx.output.writeError(`Unknown workspace subcommand: ${sub}`);
      ctx.output.info('Available: create, list, switch, delete, set, get, env, info');
      process.exit(1);
  }
}

// ── create ─────────────────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('Usage: xergon workspace create <name> [--path /some/path]');
    process.exit(1);
    return;
  }

  const workspacePath = args.options.path ? String(args.options.path) : undefined;

  try {
    const ws = createWorkspace(name, workspacePath);
    ctx.output.success(`Workspace "${name}" created`);
    ctx.output.info(`Path: ${ws.path}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const workspaces = listWorkspaces();
  const outputJson = args.options.json === true;

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(workspaces));
    return;
  }

  if (workspaces.length === 0) {
    ctx.output.info('No workspaces. Create one with: xergon workspace create <name>');
    return;
  }

  const output = ctx.output;
  output.write(output.colorize('Workspaces', 'bold'));
  output.write(output.colorize('═══════════════════════════════════════════════════════════════', 'dim'));
  output.write('');

  const tableData = workspaces.map(ws => ({
    Name: ws.active ? `${output.colorize(ws.name, 'green')} ${output.colorize('(active)', 'dim')}` : ws.name,
    Path: ws.path,
    'Default Model': ws.defaultModel ?? '-',
    'Env Vars': String(Object.keys(ws.environment).length),
    Created: ws.createdAt ? new Date(ws.createdAt).toLocaleDateString() : '-',
  }));

  output.write(output.formatTable(tableData));
}

// ── switch ─────────────────────────────────────────────────────────

async function handleSwitch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('Usage: xergon workspace switch <name>');
    process.exit(1);
    return;
  }

  try {
    const ws = switchWorkspace(name);
    ctx.output.success(`Switched to workspace "${name}"`);
    ctx.output.info(`Path: ${ws.path}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── delete ─────────────────────────────────────────────────────────

async function handleDelete(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];

  if (!name) {
    ctx.output.writeError('Usage: xergon workspace delete <name>');
    process.exit(1);
    return;
  }

  try {
    deleteWorkspace(name);
    ctx.output.success(`Workspace "${name}" deleted`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── set ────────────────────────────────────────────────────────────

async function handleSet(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const kvPair = args.positional[1];

  if (!kvPair || !kvPair.includes('=')) {
    ctx.output.writeError('Usage: xergon workspace set <KEY>=<VALUE>');
    ctx.output.info('Sets an environment variable in the current workspace.');
    process.exit(1);
    return;
  }

  const eqIdx = kvPair.indexOf('=');
  const key = kvPair.substring(0, eqIdx).trim();
  const value = kvPair.substring(eqIdx + 1).trim();

  const current = getCurrentWorkspace();
  if (!current) {
    ctx.output.writeError('No active workspace. Create one first: xergon workspace create <name>');
    process.exit(1);
    return;
  }

  try {
    setWorkspaceVar(current.name, key, value);
    ctx.output.success(`Set ${key}=${value} in workspace "${current.name}"`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(message);
    process.exit(1);
  }
}

// ── get ────────────────────────────────────────────────────────────

async function handleGet(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const key = args.positional[1];

  if (!key) {
    ctx.output.writeError('Usage: xergon workspace get <KEY>');
    process.exit(1);
    return;
  }

  const current = getCurrentWorkspace();
  if (!current) {
    ctx.output.writeError('No active workspace.');
    process.exit(1);
    return;
  }

  const value = getWorkspaceVar(current.name, key);
  if (value === undefined) {
    ctx.output.writeError(`Variable "${key}" not set in workspace "${current.name}"`);
    process.exit(1);
    return;
  }

  ctx.output.write(value);
}

// ── env ────────────────────────────────────────────────────────────

async function handleEnv(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const current = getCurrentWorkspace();
  const outputJson = args.options.json === true;

  if (!current) {
    ctx.output.writeError('No active workspace.');
    process.exit(1);
    return;
  }

  const env = current.environment;

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput(env));
    return;
  }

  const output = ctx.output;
  const keys = Object.keys(env);

  if (keys.length === 0) {
    output.info(`No environment variables in workspace "${current.name}".`);
    output.info('Set one with: xergon workspace set KEY=VALUE');
    return;
  }

  output.write(output.colorize(`Environment: ${current.name}`, 'bold'));
  output.write(output.colorize('─────────────────────────────────────────────────────────', 'dim'));
  for (const key of keys.sort()) {
    output.write(`  ${output.colorize(key, 'cyan')}${' '.repeat(Math.max(1, 24 - key.length))} ${env[key]}`);
  }
  output.write('');
}

// ── info ───────────────────────────────────────────────────────────

async function handleInfo(ctx: CLIContext, _args: ParsedArgs): Promise<void> {
  const current = getCurrentWorkspace();

  if (!current) {
    ctx.output.writeError('No active workspace.');
    ctx.output.info('Create one with: xergon workspace create <name>');
    process.exit(1);
    return;
  }

  const output = ctx.output;
  output.write(output.colorize(`Workspace: ${current.name}`, 'bold'));
  output.write(output.colorize('─────────────────────────────────────────────────────────', 'dim'));
  output.write('');
  output.write(`  ${output.colorize('Name:', 'cyan')}          ${current.name}`);
  output.write(`  ${output.colorize('Path:', 'cyan')}          ${current.path}`);
  output.write(`  ${output.colorize('Default Model:', 'cyan')} ${current.defaultModel ?? '(none)'}`);
  output.write(`  ${output.colorize('Provider:', 'cyan')}      ${current.defaultProvider ?? '(none)'}`);
  output.write(`  ${output.colorize('Created:', 'cyan')}       ${current.createdAt ? new Date(current.createdAt).toLocaleString() : '(unknown)'}`);
  output.write('');
  output.write(`  ${output.colorize('Environment Vars:', 'cyan')} ${Object.keys(current.environment).length}`);

  const envKeys = Object.keys(current.environment);
  if (envKeys.length > 0) {
    output.write('');
    for (const key of envKeys.sort()) {
      output.write(`    ${output.colorize(key, 'cyan')}${' '.repeat(Math.max(1, 20 - key.length))} ${current.environment[key]}`);
    }
  }

  output.write('');
}

export const workspaceCommand: Command = {
  name: 'workspace',
  description: 'Manage named workspace contexts',
  aliases: ['ws'],
  options: workspaceOptions,
  action: workspaceAction,
};
