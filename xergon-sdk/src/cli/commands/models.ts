/**
 * CLI command: models
 *
 * List, search, and interactively select available models.
 * Also supports model info, pull, and remove subcommands.
 *
 * Usage:
 *   xergon models
 *   xergon models search <query>
 *   xergon models info <model>
 *   xergon models pull <model>
 *   xergon models remove <model>
 *   xergon models -i
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import type { Model } from '../../types';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');
const CONFIG_FILE = () => path.join(CONFIG_DIR(), 'config.json');
const MODELS_DIR = () => path.join(CONFIG_DIR(), 'models');

const modelsOptions: CommandOption[] = [
  {
    name: 'search',
    short: '',
    long: '--search',
    description: 'Filter models by search term',
    required: false,
    type: 'string',
  },
  {
    name: 'interactive',
    short: '-i',
    long: '--interactive',
    description: 'Interactive model picker with arrow-key selection',
    required: false,
    type: 'boolean',
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

async function modelsAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  // If the first positional is a known subcommand, route to it
  if (sub === 'search' || sub === 'info' || sub === 'pull' || sub === 'remove') {
    switch (sub) {
      case 'search':
        await handleSearch(args, ctx);
        break;
      case 'info':
        await handleInfo(args, ctx);
        break;
      case 'pull':
        await handlePull(args, ctx);
        break;
      case 'remove':
        await handleRemove(args, ctx);
        break;
    }
    return;
  }

  // Default: list models (with optional --search filter and -i interactive)
  const searchTerm = args.options.search ? String(args.options.search) : undefined;
  const isInteractive = args.options.interactive === true;
  const outputJson = args.options.json === true;

  try {
    const models: Model[] = await ctx.client.models.list();

    let filtered = models;
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      filtered = models.filter((m: Model) =>
        m.id.toLowerCase().includes(term) ||
        m.ownedBy.toLowerCase().includes(term)
      );
    }

    if (isInteractive) {
      await runInteractivePicker(ctx, filtered, models);
      return;
    }

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(filtered));
    } else if (filtered.length === 0) {
      ctx.output.info(searchTerm ? `No models found matching "${searchTerm}".` : 'No models available.');
    } else {
      const tableData = filtered.map((m: Model) => ({
        ID: m.id,
        Owner: m.ownedBy,
        Pricing: m.pricing ?? '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Models (${filtered.length})`));
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list models: ${message}`);
    process.exit(1);
  }
}

// ── search ─────────────────────────────────────────────────────────

async function handleSearch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const query = args.positional[1];

  if (!query) {
    ctx.output.writeError('Usage: xergon models search <query>');
    process.exit(1);
    return;
  }

  try {
    const models: Model[] = await ctx.client.models.list();
    const term = query.toLowerCase();
    const filtered = models.filter((m: Model) =>
      m.id.toLowerCase().includes(term) ||
      m.ownedBy.toLowerCase().includes(term)
    );

    if (filtered.length === 0) {
      ctx.output.info(`No models found matching "${query}".`);
    } else {
      const tableData = filtered.map((m: Model) => ({
        ID: m.id,
        Owner: m.ownedBy,
        Pricing: m.pricing ?? '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Models matching "${query}" (${filtered.length})`));
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Search failed: ${message}`);
    process.exit(1);
  }
}

// ── info ───────────────────────────────────────────────────────────

async function handleInfo(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[1];

  if (!modelId) {
    ctx.output.writeError('Usage: xergon models info <model>');
    process.exit(1);
    return;
  }

  try {
    const models: Model[] = await ctx.client.models.list();
    const model = models.find((m: Model) => m.id === modelId || m.id.toLowerCase() === modelId.toLowerCase());

    if (!model) {
      ctx.output.writeError(`Model not found: ${modelId}`);
      ctx.output.info('Use "xergon models" to see available models.');
      process.exit(1);
      return;
    }

    ctx.output.write(ctx.output.colorize('Model Details', 'bold'));
    ctx.output.write('');
    ctx.output.write(`  ${ctx.output.colorize('ID:', 'cyan')}     ${model.id}`);
    ctx.output.write(`  ${ctx.output.colorize('Owner:', 'cyan')}   ${model.ownedBy}`);
    ctx.output.write(`  ${ctx.output.colorize('Object:', 'cyan')}  ${model.object}`);

    if (model.pricing) {
      ctx.output.write(`  ${ctx.output.colorize('Pricing:', 'cyan')} ${model.pricing}`);
    }

    // Show any additional properties
    const knownKeys = new Set(['id', 'object', 'created', 'ownedBy', 'pricing']);
    for (const [key, value] of Object.entries(model)) {
      if (!knownKeys.has(key) && value !== undefined) {
        ctx.output.write(`  ${ctx.output.colorize(`${key}:`, 'cyan')} ${String(value)}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get model info: ${message}`);
    process.exit(1);
  }
}

// ── pull ───────────────────────────────────────────────────────────

async function handlePull(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  let modelId: string | undefined = args.positional[1];

  if (!modelId) {
    // Interactive picker: select a model to pull
    try {
      const models: Model[] = await ctx.client.models.list();
      if (models.length === 0) {
        ctx.output.info('No models available to pull.');
        return;
      }
      modelId = await selectModelFromList(ctx, models);
      if (!modelId) return; // cancelled
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      ctx.output.writeError(`Failed to list models: ${message}`);
      process.exit(1);
      return;
    }
  }

  // Ensure models directory exists
  const modelsDir = MODELS_DIR();
  if (!fs.existsSync(modelsDir)) {
    fs.mkdirSync(modelsDir, { recursive: true });
  }

  const modelFile = path.join(modelsDir, `${modelId}.json`);

  if (fs.existsSync(modelFile)) {
    ctx.output.warn(`Model "${modelId}" is already saved locally.`);
    ctx.output.info(`Use "xergon models remove ${modelId}" first, or the file is at ${modelFile}`);
    return;
  }

  const thinkingMsg = ctx.output.colorize(`Pulling model info for "${modelId}"`, 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const models: Model[] = await ctx.client.models.list();
    const model = models.find((m: Model) => m.id === modelId || m.id.toLowerCase() === modelId.toLowerCase());

    if (!model) {
      process.stderr.write(' '.repeat(40) + '\r');
      ctx.output.writeError(`Model not found: ${modelId}`);
      process.exit(1);
      return;
    }

    process.stderr.write(' '.repeat(40) + '\r');

    // Save model metadata locally
    fs.writeFileSync(modelFile, JSON.stringify(model, null, 2) + '\n');
    ctx.output.success(`Model "${modelId}" saved to ${modelFile}`);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to pull model: ${message}`);
    process.exit(1);
  }
}

// ── remove ─────────────────────────────────────────────────────────

async function handleRemove(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[1];

  if (!modelId) {
    ctx.output.writeError('Usage: xergon models remove <model>');
    process.exit(1);
    return;
  }

  const modelFile = path.join(MODELS_DIR(), `${modelId}.json`);

  if (!fs.existsSync(modelFile)) {
    ctx.output.writeError(`Model "${modelId}" not found locally.`);
    ctx.output.info(`Only models saved with "xergon models pull" can be removed.`);
    process.exit(1);
    return;
  }

  try {
    fs.unlinkSync(modelFile);
    ctx.output.success(`Model "${modelId}" removed from local cache.`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to remove model: ${message}`);
    process.exit(1);
  }
}

// ── Interactive Picker ─────────────────────────────────────────────

/**
 * Interactive model picker: numbered list with arrow-key navigation.
 * Falls back to simple numbered selection if the terminal doesn't support raw mode.
 */
async function runInteractivePicker(ctx: CLIContext, filteredModels: Model[], allModels: Model[]): Promise<void> {
  const models = filteredModels.length > 0 ? filteredModels : allModels;

  if (models.length === 0) {
    ctx.output.info('No models available to select from.');
    return;
  }

  const output = ctx.output;
  const currentDefault = ctx.config.defaultModel || '';

  // Find index of current default model
  let selectedIndex = 0;
  const defaultIdx = models.findIndex(m => m.id === currentDefault);
  if (defaultIdx >= 0) {
    selectedIndex = defaultIdx;
  }

  // Check if we can use raw mode for arrow keys
  const canRawMode = process.stdin.isTTY && typeof (process.stdin as any).setRawMode === 'function';

  if (canRawMode) {
    await runArrowKeyPicker(ctx, models, selectedIndex);
  } else {
    await runNumberedPicker(ctx, models, selectedIndex);
  }
}

/**
 * Arrow-key driven model picker using raw terminal mode.
 */
async function runArrowKeyPicker(ctx: CLIContext, models: Model[], initialIndex: number): Promise<void> {
  const output = ctx.output;
  let cursor = initialIndex;

  const readline = await import('node:readline');
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  // Enable raw mode for arrow key capture
  const stdin = process.stdin as any;
  stdin.setRawMode(true);
  stdin.resume();
  stdin.setEncoding('utf-8');

  function render(): void {
    // Move cursor to top of picker area
    const linesToClear = models.length + 4; // header + models + footer
    process.stdout.write(`\x1b[${linesToClear}A`);
    process.stdout.write('\x1b[J'); // clear from cursor to end

    output.write(output.colorize('  Select a Model', 'bold') + '\n');
    output.write(output.colorize('  ─────────────────────────────────────────────────────────', 'dim') + '\n');

    for (let i = 0; i < models.length; i++) {
      const m = models[i];
      const isSelected = i === cursor;
      const isDefault = m.id === ctx.config.defaultModel;

      const prefix = isSelected
        ? output.colorize('  > ', 'cyan')
        : '    ';

      const name = isSelected
        ? output.colorize(m.id, 'bold')
        : m.id;

      const defaultTag = isDefault
        ? output.colorize(' (default)', 'green')
        : '';

      const pricing = m.pricing
        ? output.colorize(`  ${m.pricing}`, 'dim')
        : '';

      output.write(`${prefix}${name}${defaultTag}${pricing}\n`);
    }

    output.write(output.colorize('  ─────────────────────────────────────────────────────────', 'dim') + '\n');
    output.write('  ' + output.colorize('arrow keys navigate', 'dim') + '  ' +
      output.colorize('Enter', 'dim') + ' select  ' +
      output.colorize('q', 'dim') + ' cancel\n');
  }

  // Initial render - move down first to reserve space
  for (let i = 0; i < models.length + 4; i++) {
    process.stdout.write('\n');
  }
  render();

  const cleanup = (): void => {
    stdin.setRawMode(false);
    stdin.pause();
    rl.close();
  };

  return new Promise<void>((resolve) => {
    stdin.on('data', (data: Buffer | string) => {
      const key = typeof data === 'string' ? data : data.toString();

      if (key === '\u001b[A') {
        // Up arrow
        cursor = (cursor - 1 + models.length) % models.length;
        render();
      } else if (key === '\u001b[B') {
        // Down arrow
        cursor = (cursor + 1) % models.length;
        render();
      } else if (key === '\r' || key === '\n') {
        // Enter - select
        const selected = models[cursor];
        cleanup();
        setModelAsDefault(selected.id, ctx);
        output.write('\n');
        output.success(`Selected model: ${selected.id}`);
        output.info('Set as default in config.');
        resolve();
      } else if (key === 'q' || key === '\u0003') {
        // q or Ctrl+C - cancel
        cleanup();
        output.write('\n');
        output.write(output.colorize('Cancelled.\n', 'dim'));
        resolve();
      } else if (key === '\u001b') {
        // Escape - cancel
        cleanup();
        output.write('\n');
        output.write(output.colorize('Cancelled.\n', 'dim'));
        resolve();
      }
      // Ignore other keys
    });
  });
}

/**
 * Simple numbered model picker (fallback for non-TTY environments).
 */
async function runNumberedPicker(ctx: CLIContext, models: Model[], selectedIndex: number): Promise<void> {
  const output = ctx.output;

  output.write(output.colorize('Available Models:', 'bold') + '\n');
  output.write(output.colorize('─────────────────────────────────────────────────────────', 'dim') + '\n');

  for (let i = 0; i < models.length; i++) {
    const m = models[i];
    const num = output.colorize(`  [${(i + 1).toString().padStart(2)}]`, 'cyan');
    const isDefault = m.id === ctx.config.defaultModel;
    const defaultTag = isDefault ? output.colorize(' (default)', 'green') : '';
    const pricing = m.pricing ? output.colorize(`  ${m.pricing}`, 'dim') : '';
    output.write(`${num} ${m.id}${defaultTag}${pricing}\n`);
  }

  output.write(output.colorize('─────────────────────────────────────────────────────────', 'dim') + '\n');

  // Read selection
  const readline = await import('node:readline');
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const choice = await new Promise<string>((resolve) => {
    rl.question(output.colorize('  Enter model number (or q to cancel): ', 'yellow'), (answer: string) => {
      rl.close();
      resolve(answer.trim());
    });
  });

  if (choice.toLowerCase() === 'q' || choice === '') {
    output.write(output.colorize('Cancelled.\n', 'dim'));
    return;
  }

  const num = parseInt(choice, 10);
  if (isNaN(num) || num < 1 || num > models.length) {
    output.writeError(`Invalid selection. Please enter a number between 1 and ${models.length}.`);
    return;
  }

  const selected = models[num - 1];
  setModelAsDefault(selected.id, ctx);
  output.success(`Selected model: ${selected.id}`);
  output.info('Set as default in config.');
}

/**
 * Prompt user to select a model from a list. Returns the model ID or undefined if cancelled.
 */
async function selectModelFromList(ctx: CLIContext, models: Model[]): Promise<string | undefined> {
  const output = ctx.output;

  output.write(output.colorize('Select a model:', 'bold') + '\n');
  output.write(output.colorize('─────────────────────────────────────────────────────────', 'dim') + '\n');

  for (let i = 0; i < models.length; i++) {
    const m = models[i];
    const num = output.colorize(`  [${(i + 1).toString().padStart(2)}]`, 'cyan');
    output.write(`${num} ${m.id}\n`);
  }

  output.write(output.colorize('─────────────────────────────────────────────────────────', 'dim') + '\n');

  const readline = await import('node:readline');
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const choice = await new Promise<string>((resolve) => {
    rl.question(output.colorize('  Enter model number (or q to cancel): ', 'yellow'), (answer: string) => {
      rl.close();
      resolve(answer.trim());
    });
  });

  if (choice.toLowerCase() === 'q' || choice === '') {
    output.write(output.colorize('Cancelled.\n', 'dim'));
    return undefined;
  }

  const num = parseInt(choice, 10);
  if (isNaN(num) || num < 1 || num > models.length) {
    output.writeError(`Invalid selection.`);
    return undefined;
  }

  return models[num - 1].id;
}

/**
 * Set a model as the default in the user's config file.
 */
function setModelAsDefault(modelId: string, ctx: CLIContext): void {
  try {
    const configPath = CONFIG_FILE();
    let fileConfig: Record<string, unknown> = {};
    try {
      const data = fs.readFileSync(configPath, 'utf-8');
      fileConfig = JSON.parse(data);
    } catch {
      // Config doesn't exist yet
    }
    fileConfig.defaultModel = modelId;
    const dir = CONFIG_DIR();
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.writeFileSync(configPath, JSON.stringify(fileConfig, null, 2) + '\n');
  } catch (err) {
    ctx.output.warn(`Could not save default model to config: ${err instanceof Error ? err.message : String(err)}`);
  }
}

export const modelsCommand: Command = {
  name: 'models',
  description: 'List, search, and manage available models',
  aliases: ['model', 'list-models'],
  options: modelsOptions,
  action: modelsAction,
};
