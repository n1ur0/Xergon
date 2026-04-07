/**
 * CLI command: edit-config
 *
 * Interactive config editor with tree view, inline editing, validation,
 * backup, and diff display.
 *
 * Usage:
 *   xergon config edit              -- launch interactive editor
 *   xergon config edit --section <name>  -- jump to a specific section
 *   xergon config edit --set key=value  -- non-interactive set
 *   xergon config edit --get key        -- non-interactive get
 *   xergon config edit --reset          -- reset config to defaults
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Paths ──────────────────────────────────────────────────────────

function getConfigDir(): string {
  return path.join(os.homedir(), '.xergon');
}

function getConfigFile(): string {
  return path.join(getConfigDir(), 'config.json');
}

function getBackupDir(): string {
  return path.join(getConfigDir(), 'backups');
}

// ── Default config ─────────────────────────────────────────────────

const DEFAULT_CONFIG: Record<string, string | number | boolean> = {
  baseUrl: 'https://relay.xergon.gg',
  defaultModel: 'llama-3.3-70b',
  outputFormat: 'text',
  timeout: 30000,
};

// ── Config schema (for validation) ─────────────────────────────────

interface ConfigField {
  key: string;
  label: string;
  type: 'string' | 'number' | 'boolean';
  section: string;
  description: string;
  allowed?: string[];
}

const CONFIG_SCHEMA: ConfigField[] = [
  { key: 'baseUrl', label: 'Base URL', type: 'string', section: 'relay', description: 'Relay endpoint URL', allowed: [] },
  { key: 'apiKey', label: 'API Key', type: 'string', section: 'auth', description: 'Public key for authentication' },
  { key: 'defaultModel', label: 'Default Model', type: 'string', section: 'relay', description: 'Default model for chat completions' },
  { key: 'outputFormat', label: 'Output Format', type: 'string', section: 'cli', description: 'Default CLI output format', allowed: ['text', 'json', 'table'] },
  { key: 'timeout', label: 'Timeout', type: 'number', section: 'relay', description: 'Request timeout in milliseconds' },
  { key: 'color', label: 'Color Output', type: 'boolean', section: 'cli', description: 'Enable/disable colored output' },
  { key: 'agentUrl', label: 'Agent URL', type: 'string', section: 'agent', description: 'Local agent endpoint URL' },
];

const SECTIONS = [...new Set(CONFIG_SCHEMA.map(f => f.section))];

// ── Helpers ────────────────────────────────────────────────────────

function loadConfig(): Record<string, string | number | boolean> {
  try {
    const data = fs.readFileSync(getConfigFile(), 'utf-8');
    return JSON.parse(data);
  } catch {
    return {};
  }
}

function saveConfig(config: Record<string, string | number | boolean>): void {
  const dir = getConfigDir();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(getConfigFile(), JSON.stringify(config, null, 2) + '\n');
}

function backupConfig(): string {
  const backupDir = getBackupDir();
  if (!fs.existsSync(backupDir)) {
    fs.mkdirSync(backupDir, { recursive: true });
  }
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const backupPath = path.join(backupDir, `config-${timestamp}.json`);

  try {
    const current = fs.readFileSync(getConfigFile(), 'utf-8');
    fs.writeFileSync(backupPath, current);
    return backupPath;
  } catch {
    return backupPath; // file may not exist yet
  }
}

function validateValue(key: string, value: string): { valid: boolean; parsed: string | number | boolean; error?: string } {
  const schema = CONFIG_SCHEMA.find(f => f.key === key);
  if (!schema) {
    // Unknown key -- allow as string
    return { valid: true, parsed: value };
  }

  if (schema.type === 'boolean') {
    const lower = value.toLowerCase();
    if (lower === 'true' || lower === '1' || lower === 'yes') return { valid: true, parsed: true };
    if (lower === 'false' || lower === '0' || lower === 'no') return { valid: true, parsed: false };
    return { valid: false, parsed: value, error: `Must be true/false, yes/no, or 1/0` };
  }

  if (schema.type === 'number') {
    const num = Number(value);
    if (isNaN(num)) return { valid: false, parsed: value, error: `Must be a number` };
    if (schema.key === 'timeout' && num < 1000) return { valid: false, parsed: value, error: `Timeout must be at least 1000ms` };
    return { valid: true, parsed: num };
  }

  if (schema.allowed && schema.allowed.length > 0 && !schema.allowed.includes(value)) {
    return { valid: false, parsed: value, error: `Must be one of: ${schema.allowed.join(', ')}` };
  }

  return { valid: true, parsed: value };
}

function generateDiff(original: Record<string, unknown>, modified: Record<string, unknown>): string {
  const allKeys = new Set([...Object.keys(original), ...Object.keys(modified)]);
  const lines: string[] = [];

  for (const key of allKeys) {
    const oldVal = JSON.stringify(original[key] ?? null);
    const newVal = JSON.stringify(modified[key] ?? null);

    if (oldVal === newVal) continue;

    if (!(key in original)) {
      lines.push(`  + ${key}: ${newVal}`);
    } else if (!(key in modified)) {
      lines.push(`  - ${key}: ${oldVal}`);
    } else {
      lines.push(`  ~ ${key}: ${oldVal} -> ${newVal}`);
    }
  }

  return lines.length > 0 ? lines.join('\n') : '  (no changes)';
}

// ── ANSI helpers ───────────────────────────────────────────────────

const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';
const DIM = '\x1b[2m';
const CYAN = '\x1b[36m';
const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const BLUE = '\x1b[34m';
const INVERT = '\x1b[7m';

function c(text: string, ...codes: string[]): string {
  return codes.join('') + text + RESET;
}

function clearScreen(): void {
  process.stdout.write('\x1b[2J\x1b[H');
}

// ── Interactive editor ─────────────────────────────────────────────

async function runInteractiveEditor(
  ctx: CLIContext,
  startSection?: string,
): Promise<void> {
  const config = loadConfig();
  const original = { ...config };

  // Build editable entries: sections with fields
  interface EditableEntry {
    key: string;
    label: string;
    value: string | number | boolean;
    type: string;
    section: string;
    description: string;
  }

  const entries: EditableEntry[] = [];
  for (const field of CONFIG_SCHEMA) {
    entries.push({
      key: field.key,
      label: field.label,
      value: config[field.key] ?? DEFAULT_CONFIG[field.key] ?? '',
      type: field.type,
      section: field.section,
      description: field.description,
    });
  }

  // Find start index
  let cursorIdx = 0;
  if (startSection) {
    const found = entries.findIndex(e => e.section === startSection);
    if (found >= 0) cursorIdx = found;
  }

  // Enter raw mode
  if (process.stdin.isTTY) {
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.setEncoding('utf-8');
  }

  let running = true;
  let editing = false;
  let editBuffer = '';
  let showDiff = false;

  function renderEditor(): string {
    const w = process.stdout.columns ?? 80;
    const lines: string[] = [];

    clearScreen();
    lines.push(c('  XERGON CONFIG EDITOR', BOLD, CYAN));
    lines.push(c('  ' + '─'.repeat(Math.min(w - 6, 66)), DIM));
    lines.push('');

    // Group by section
    let lastSection = '';
    for (let i = 0; i < entries.length; i++) {
      const entry = entries[i];

      if (entry.section !== lastSection) {
        lastSection = entry.section;
        lines.push('');
        lines.push('  ' + c(`[${entry.section.toUpperCase()}]`, BOLD, YELLOW));
      }

      const isSelected = i === cursorIdx && !editing;
      const isEditing = i === cursorIdx && editing;
      const prefix = isSelected ? c(' > ', GREEN) : '   ';
      const keyStr = isSelected ? c(entry.label, BOLD) : entry.label;

      let valueStr: string;
      if (isEditing) {
        valueStr = c(editBuffer + '_', CYAN, INVERT);
      } else {
        const masked = entry.key.toLowerCase().includes('key') || entry.key.toLowerCase().includes('secret');
        const displayVal = masked && entry.value && String(entry.value).length > 8
          ? String(entry.value).substring(0, 8) + '...'
          : String(entry.value ?? c('(default)', DIM));
        valueStr = entry.value ? c(displayVal, CYAN) : c('(not set)', DIM);
      }

      lines.push(`${prefix}${keyStr.padEnd(18)} ${valueStr}`);

      if (isSelected || isEditing) {
        lines.push('     ' + c(entry.description, DIM) +
          (entry.type === 'number' ? ' ' + c('(number)', DIM) : '') +
          (entry.type === 'boolean' ? ' ' + c('(boolean: true/false)', DIM) : ''));
      }
    }

    // Diff view
    if (showDiff) {
      lines.push('');
      lines.push(c('  ' + '─'.repeat(Math.min(w - 6, 66)), DIM));
      lines.push(c('  CHANGES:', BOLD, YELLOW));
      const diff = generateDiff(original, config);
      if (diff === '  (no changes)') {
        lines.push('  ' + c('(no changes)', DIM));
      } else {
        for (const line of diff.split('\n')) {
          if (line.startsWith('  +')) lines.push(c('  ' + line, GREEN));
          else if (line.startsWith('  -')) lines.push(c('  ' + line, RED));
          else if (line.startsWith('  ~')) lines.push(c('  ' + line, YELLOW));
          else lines.push('  ' + line);
        }
      }
    }

    // Controls
    lines.push('');
    lines.push(c('  ' + '─'.repeat(Math.min(w - 6, 66)), DIM));
    lines.push('  ' +
      c('[↑/↓]', BOLD) + ' navigate  ' +
      c('[Enter]', BOLD) + ' edit  ' +
      c('[Esc]', BOLD) + ' cancel edit  ' +
      c('[s]', BOLD) + ' save  ' +
      c('[d]', BOLD) + ' diff  ' +
      c('[q]', BOLD) + ' quit');

    if (editing) {
      lines.push('  ' + c('EDITING: ', BOLD, GREEN) + c(editBuffer, CYAN) + c('_', INVERT));
    }

    return lines.join('\n');
  }

  // Key handler
  function onKey(key: string) {
    if (key === '\x03') { // Ctrl+C
      running = false;
      return;
    }

    if (editing) {
      if (key === '\r' || key === '\n') {
        // Validate and apply
        const entry = entries[cursorIdx];
        const result = validateValue(entry.key, editBuffer);
        if (result.valid) {
          config[entry.key] = result.parsed;
          entries[cursorIdx].value = result.parsed;
          showDiff = true;
        } else {
          // Flash error -- just continue editing, error shown in next render
        }
        editBuffer = '';
        editing = false;
        return;
      }
      if (key === '\x1b') {
        // Cancel edit
        editBuffer = '';
        editing = false;
        return;
      }
      if (key === '\x7f' || key === '\b') {
        // Backspace
        editBuffer = editBuffer.slice(0, -1);
        return;
      }
      if (key.length === 1 && !key.startsWith('\x1b')) {
        editBuffer += key;
      }
      return;
    }

    // Navigation mode
    if (key === 'q') {
      running = false;
      return;
    }
    if (key === 's') {
      // Save
      const backupPath = backupConfig();
      saveConfig(config);
      clearScreen();
      process.stdout.write(c('  Config saved!', GREEN) + '\n');
      process.stdout.write(c('  Backup: ', DIM) + backupPath + '\n');
      running = false;
      return;
    }
    if (key === 'd') {
      showDiff = !showDiff;
      return;
    }
    if (key === 'A' || key === '\x1b[A') { // Up arrow
      cursorIdx = Math.max(0, cursorIdx - 1);
      return;
    }
    if (key === 'B' || key === '\x1b[B') { // Down arrow
      cursorIdx = Math.min(entries.length - 1, cursorIdx + 1);
      return;
    }
    if (key === '\r' || key === '\n') {
      // Start editing
      editing = true;
      editBuffer = String(entries[cursorIdx].value ?? '');
      return;
    }
  }

  process.stdin.on('data', onKey);

  try {
    while (running) {
      process.stdout.write(renderEditor());
      await new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, 500);
        const check = setInterval(() => {
          if (!running) {
            clearTimeout(timer);
            clearInterval(check);
            resolve();
          }
        }, 100);
      });
    }
  } finally {
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(false);
      process.stdin.pause();
    }
    process.stdin.removeListener('data', onKey);
    clearScreen();
  }
}

// ── Options ────────────────────────────────────────────────────────

const editConfigOptions: CommandOption[] = [
  {
    name: 'section',
    short: '',
    long: '--section',
    description: 'Jump to a specific config section',
    required: false,
    type: 'string',
  },
  {
    name: 'set',
    short: '',
    long: '--set',
    description: 'Set a config value non-interactively (key=value)',
    required: false,
    type: 'string',
  },
  {
    name: 'get',
    short: '',
    long: '--get',
    description: 'Get a config value non-interactively',
    required: false,
    type: 'string',
  },
  {
    name: 'reset',
    short: '',
    long: '--reset',
    description: 'Reset config to defaults',
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

// ── Command action ─────────────────────────────────────────────────

async function editConfigAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const section = args.options.section ? String(args.options.section) : undefined;
  const setValue = args.options.set ? String(args.options.set) : undefined;
  const getValue = args.options.get ? String(args.options.get) : undefined;
  const doReset = args.options.reset === true;
  const outputJson = args.options.json === true;

  // Non-interactive reset
  if (doReset) {
    const backupPath = backupConfig();
    saveConfig({ ...DEFAULT_CONFIG });
    ctx.output.success('Config reset to defaults');
    ctx.output.info(`Backup saved to: ${backupPath}`);
    return;
  }

  // Non-interactive get
  if (getValue) {
    const config = loadConfig();
    const value = config[getValue] ?? DEFAULT_CONFIG[getValue] ?? undefined;
    if (outputJson) {
      ctx.output.write(JSON.stringify({ key: getValue, value: value ?? null }, null, 2));
    } else {
      if (value === undefined) {
        ctx.output.writeError(`Config key "${getValue}" not found`);
        process.exit(1);
        return;
      }
      // Mask sensitive keys
      const masked = getValue.toLowerCase().includes('key') || getValue.toLowerCase().includes('secret');
      const display = masked && String(value).length > 8
        ? String(value).substring(0, 8) + '...'
        : String(value);
      ctx.output.write(display + '\n');
    }
    return;
  }

  // Non-interactive set
  if (setValue) {
    let key: string;
    let value: string;

    if (setValue.includes('=')) {
      const eqIdx = setValue.indexOf('=');
      key = setValue.substring(0, eqIdx).trim();
      value = setValue.substring(eqIdx + 1).trim();
    } else {
      key = setValue.trim();
      value = args.positional.join(' ').trim();
    }

    if (!key) {
      ctx.output.writeError('Usage: xergon config edit --set <key>=<value>');
      process.exit(1);
      return;
    }

    const validation = validateValue(key, value);
    if (!validation.valid) {
      ctx.output.writeError(`Invalid value for "${key}": ${validation.error}`);
      process.exit(1);
      return;
    }

    const original = loadConfig();
    const backupPath = backupConfig();
    const config = { ...original };
    config[key] = validation.parsed;
    saveConfig(config);

    if (outputJson) {
      ctx.output.write(JSON.stringify({ key, value: validation.parsed, backup: backupPath }, null, 2));
    } else {
      ctx.output.success(`Set ${key} = ${validation.parsed}`);
      ctx.output.info(`Backup saved to: ${backupPath}`);
    }
    return;
  }

  // Interactive editor
  await runInteractiveEditor(ctx, section);
}

// ── Command definition ─────────────────────────────────────────────

export const editConfigCommand: Command = {
  name: 'edit-config',
  description: 'Interactive config editor with tree view and validation',
  aliases: ['cfg-edit'],
  options: editConfigOptions,
  action: editConfigAction,
};

// Re-export the action for use by config.ts "edit" subcommand
export { editConfigAction };
