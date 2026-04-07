/**
 * Xergon SDK -- Model Alias System.
 *
 * Provides short alias names that resolve to full model identifiers.
 * Ships with built-in aliases for common models and supports custom
 * aliases persisted in ~/.xergon/aliases.json.
 *
 * @example
 * ```ts
 * import { resolveAlias, addAlias, listAliases } from '@xergon/sdk';
 *
 * const model = resolveAlias('code'); // => 'deepseek-coder/DeepSeek-Coder-V2-Instruct'
 * addAlias('fast', 'meta-llama/Meta-Llama-3.1-8B-Instruct');
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export interface ModelAlias {
  alias: string;      // short name
  model: string;      // full model name
  provider?: string;  // optional provider override
}

interface AliasesData {
  aliases: Record<string, ModelAlias>;
}

// ── Storage helpers ────────────────────────────────────────────────

const ALIASES_DIR = () => path.join(os.homedir(), '.xergon');
const ALIASES_FILE = () => path.join(ALIASES_DIR(), 'aliases.json');

function ensureDir(): void {
  const dir = ALIASES_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

function loadAliasesData(): Record<string, ModelAlias> {
  try {
    const data = fs.readFileSync(ALIASES_FILE(), 'utf-8');
    const parsed: AliasesData = JSON.parse(data);
    return parsed.aliases ?? {};
  } catch {
    return {};
  }
}

function saveAliasesData(aliases: Record<string, ModelAlias>): void {
  ensureDir();
  const data: AliasesData = { aliases };
  fs.writeFileSync(ALIASES_FILE(), JSON.stringify(data, null, 2) + '\n');
}

// ── Built-in Aliases ───────────────────────────────────────────────

const builtinAliases: ModelAlias[] = [
  { alias: 'gpt4', model: 'meta-llama/Meta-Llama-3.1-70B-Instruct' },
  { alias: 'gpt35', model: 'meta-llama/Meta-Llama-3.1-8B-Instruct' },
  { alias: 'claude', model: 'anthropic/claude-3-sonnet' },
  { alias: 'code', model: 'deepseek-coder/DeepSeek-Coder-V2-Instruct' },
];

// ── Public API ─────────────────────────────────────────────────────

/**
 * Resolve an alias (or plain model name) to a full model identifier.
 * Checks built-in aliases first, then custom aliases. If no alias
 * matches, returns the input unchanged (assumed to be a full model name).
 */
export function resolveAlias(name: string): { model: string; provider?: string; isAlias: boolean } {
  // Check built-in aliases
  const builtin = builtinAliases.find(a => a.alias === name);
  if (builtin) {
    return { model: builtin.model, provider: builtin.provider, isAlias: true };
  }

  // Check custom aliases
  const custom = loadAliasesData();
  const alias = custom[name];
  if (alias) {
    return { model: alias.model, provider: alias.provider, isAlias: true };
  }

  // No alias found -- return as-is
  return { model: name, isAlias: false };
}

/**
 * Resolve an alias to just the model string (convenience wrapper).
 */
export function resolveModelName(name: string): string {
  return resolveAlias(name).model;
}

/**
 * List all aliases (built-in + custom).
 */
export function listAliases(): ModelAlias[] {
  const custom = loadAliasesData();
  return [
    ...builtinAliases.map(a => ({ ...a, _builtin: true } as ModelAlias & { _builtin: boolean })),
    ...Object.values(custom),
  ];
}

/**
 * Add a custom alias. Persists to ~/.xergon/aliases.json.
 * Cannot overwrite built-in aliases.
 */
export function addAlias(alias: string, model: string, provider?: string): void {
  if (!alias || alias.trim() === '') {
    throw new Error('Alias name is required.');
  }
  if (!model || model.trim() === '') {
    throw new Error('Model name is required.');
  }

  // Check for collision with built-ins
  if (builtinAliases.some(a => a.alias === alias)) {
    throw new Error(`Cannot overwrite built-in alias: ${alias}`);
  }

  const custom = loadAliasesData();
  custom[alias] = { alias, model, provider };
  saveAliasesData(custom);
}

/**
 * Remove a custom alias. Cannot remove built-in aliases.
 * Returns true if the alias was removed, false if it didn't exist.
 */
export function removeAlias(alias: string): boolean {
  if (builtinAliases.some(a => a.alias === alias)) {
    throw new Error(`Cannot remove built-in alias: ${alias}`);
  }

  const custom = loadAliasesData();
  if (!(alias in custom)) {
    return false;
  }

  delete custom[alias];
  saveAliasesData(custom);
  return true;
}

/**
 * Get a single alias by name. Returns undefined if not found.
 */
export function getAlias(name: string): ModelAlias | undefined {
  const builtin = builtinAliases.find(a => a.alias === name);
  if (builtin) return { ...builtin };
  const custom = loadAliasesData();
  return custom[name] ? { ...custom[name] } : undefined;
}
