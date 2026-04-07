/**
 * CLI command: config
 *
 * View and manage CLI configuration and profiles.
 *
 * Usage:
 *   xergon config                    -- show current config (effective)
 *   xergon config list               -- show all config sections
 *   xergon config get <key>          -- get a config value
 *   xergon config set <key> <value>  -- set a config value
 *   xergon config reset [key]        -- reset to default
 *   xergon config path               -- show config file paths
 *   xergon config validate           -- validate current config
 *   xergon config edit               -- launch interactive editor
 *   xergon config profile list       -- list all profiles
 *   xergon config profile use <name> -- switch active profile
 *   xergon config profile set <name> -- create/update a profile
 *   xergon config profile show       -- show active profile
 *   xergon config profile delete <name> -- delete a custom profile
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ──────────────────────────────────────────────────────────

interface ConfigField {
  key: string;
  label: string;
  type: 'string' | 'number' | 'boolean';
  section: string;
  description: string;
  allowed?: string[];
  sensitive?: boolean;
  envOverride?: string;
  minValue?: number;
  maxValue?: number;
  pattern?: string;
}

interface ValidationResult {
  valid: boolean;
  errors: Array<{ key: string; message: string; severity: 'error' | 'warning' }>;
  warnings: Array<{ key: string; message: string }>;
  configSource: 'global' | 'local' | 'env' | 'default';
  configPath: string;
  localConfigPath: string;
}

interface EffectiveConfig {
  key: string;
  label: string;
  value: string | number | boolean | null;
  source: 'config' | 'local' | 'env' | 'default';
  sensitive: boolean;
}

// ── Paths ──────────────────────────────────────────────────────────

const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');
const CONFIG_FILE = () => path.join(CONFIG_DIR(), 'config.json');
const LOCAL_CONFIG_DIR = () => path.join(process.cwd(), '.xergon');
const LOCAL_CONFIG_FILE = () => path.join(LOCAL_CONFIG_DIR(), 'config.json');

// ── Default config ─────────────────────────────────────────────────

const DEFAULT_CONFIG: Record<string, string | number | boolean> = {
  baseUrl: 'https://relay.xergon.gg',
  defaultModel: 'llama-3.3-70b',
  outputFormat: 'text',
  timeout: 30000,
  color: true,
};

// ── Config schema ──────────────────────────────────────────────────

const CONFIG_SCHEMA: ConfigField[] = [
  { key: 'baseUrl', label: 'Base URL', type: 'string', section: 'relay', description: 'Relay endpoint URL', envOverride: 'XERGON_BASE_URL', pattern: '^https?://' },
  { key: 'apiKey', label: 'API Key', type: 'string', section: 'auth', description: 'Public key for authentication', sensitive: true, envOverride: 'XERGON_API_KEY' },
  { key: 'defaultModel', label: 'Default Model', type: 'string', section: 'relay', description: 'Default model for chat completions', envOverride: 'XERGON_DEFAULT_MODEL' },
  { key: 'outputFormat', label: 'Output Format', type: 'string', section: 'cli', description: 'Default CLI output format', allowed: ['text', 'json', 'table'], envOverride: 'XERGON_OUTPUT_FORMAT' },
  { key: 'timeout', label: 'Timeout', type: 'number', section: 'relay', description: 'Request timeout in milliseconds', minValue: 1000, maxValue: 300000, envOverride: 'XERGON_TIMEOUT' },
  { key: 'color', label: 'Color Output', type: 'boolean', section: 'cli', description: 'Enable/disable colored output' },
  { key: 'agentUrl', label: 'Agent URL', type: 'string', section: 'agent', description: 'Local agent endpoint URL', pattern: '^https?://' },
];

// ── Helpers ────────────────────────────────────────────────────────

function loadConfigFile(filePath?: string): Record<string, string | number | boolean> {
  const target = filePath ?? CONFIG_FILE();
  try {
    const data = fs.readFileSync(target, 'utf-8');
    return JSON.parse(data);
  } catch {
    return {};
  }
}

function loadLocalConfig(): Record<string, string | number | boolean> {
  return loadConfigFile(LOCAL_CONFIG_FILE());
}

function saveConfigFile(config: Record<string, string | number | boolean>): void {
  const dir = CONFIG_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(CONFIG_FILE(), JSON.stringify(config, null, 2) + '\n');
}

function backupConfig(): string {
  const backupDir = path.join(CONFIG_DIR(), 'backups');
  if (!fs.existsSync(backupDir)) {
    fs.mkdirSync(backupDir, { recursive: true });
  }
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const backupPath = path.join(backupDir, `config-${timestamp}.json`);
  try {
    fs.copyFileSync(CONFIG_FILE(), backupPath);
  } catch {
    // config may not exist
  }
  return backupPath;
}

function getEnvOverride(key: string): string | number | boolean | undefined {
  const field = CONFIG_SCHEMA.find(f => f.key === key);
  if (!field?.envOverride) return undefined;

  const envVal = process.env[field.envOverride];
  if (envVal === undefined) return undefined;

  if (field.type === 'boolean') {
    return envVal !== '' && envVal !== '0' && envVal.toLowerCase() !== 'false';
  }
  if (field.type === 'number') {
    const num = Number(envVal);
    return isNaN(num) ? undefined : num;
  }
  return envVal;
}

function getEffectiveValue(key: string): { value: string | number | boolean | null; source: 'config' | 'local' | 'env' | 'default' } {
  // Priority: env > local config > global config > default
  const envVal = getEnvOverride(key);
  if (envVal !== undefined) return { value: envVal, source: 'env' };

  const localConfig = loadLocalConfig();
  if (key in localConfig) return { value: localConfig[key], source: 'local' };

  const globalConfig = loadConfigFile();
  if (key in globalConfig) return { value: globalConfig[key], source: 'config' };

  const defaultVal = DEFAULT_CONFIG[key] ?? null;
  return { value: defaultVal, source: 'default' };
}

function validateValue(key: string, value: string): { valid: boolean; parsed: string | number | boolean; error?: string } {
  const schema = CONFIG_SCHEMA.find(f => f.key === key);
  if (!schema) {
    return { valid: true, parsed: value };
  }

  if (schema.type === 'boolean') {
    const lower = value.toLowerCase();
    if (lower === 'true' || lower === '1' || lower === 'yes') return { valid: true, parsed: true };
    if (lower === 'false' || lower === '0' || lower === 'no') return { valid: true, parsed: false };
    return { valid: false, parsed: value, error: 'Must be true/false, yes/no, or 1/0' };
  }

  if (schema.type === 'number') {
    const num = Number(value);
    if (isNaN(num)) return { valid: false, parsed: value, error: 'Must be a number' };
    if (schema.minValue !== undefined && num < schema.minValue) {
      return { valid: false, parsed: value, error: `Must be at least ${schema.minValue}` };
    }
    if (schema.maxValue !== undefined && num > schema.maxValue) {
      return { valid: false, parsed: value, error: `Must be at most ${schema.maxValue}` };
    }
    return { valid: true, parsed: num };
  }

  if (schema.allowed && schema.allowed.length > 0 && !schema.allowed.includes(value)) {
    return { valid: false, parsed: value, error: `Must be one of: ${schema.allowed.join(', ')}` };
  }

  if (schema.pattern) {
    try {
      const regex = new RegExp(schema.pattern);
      if (!regex.test(value)) {
        return { valid: false, parsed: value, error: `Must match pattern: ${schema.pattern}` };
      }
    } catch {
      // Invalid regex in schema -- skip pattern validation
    }
  }

  return { valid: true, parsed: value };
}

function validateFullConfig(): ValidationResult {
  const errors: Array<{ key: string; message: string; severity: 'error' | 'warning' }> = [];
  const warnings: Array<{ key: string; message: string }> = [];

  const globalConfig = loadConfigFile();
  const localConfig = loadLocalConfig();

  // Check all schema fields
  for (const field of CONFIG_SCHEMA) {
    const { value, source } = getEffectiveValue(field.key);

    if (value === null || value === undefined) {
      if (field.key === 'baseUrl') {
        errors.push({ key: field.key, message: 'baseUrl is required', severity: 'error' });
      }
      continue;
    }

    // Type-specific validation
    if (field.type === 'string' && typeof value === 'string') {
      if (field.pattern && value.length > 0) {
        try {
          const regex = new RegExp(field.pattern);
          if (!regex.test(value)) {
            errors.push({ key: field.key, message: `Value "${value}" does not match pattern ${field.pattern}`, severity: 'error' });
          }
        } catch {
          // Skip invalid regex
        }
      }
    }

    if (field.type === 'number' && typeof value === 'number') {
      if (field.minValue !== undefined && value < field.minValue) {
        errors.push({ key: field.key, message: `Value ${value} is below minimum ${field.minValue}`, severity: 'error' });
      }
      if (field.maxValue !== undefined && value > field.maxValue) {
        errors.push({ key: field.key, message: `Value ${value} exceeds maximum ${field.maxValue}`, severity: 'error' });
      }
    }
  }

  // Check for unknown keys in config
  for (const key of Object.keys(globalConfig)) {
    if (!CONFIG_SCHEMA.find(f => f.key === key)) {
      warnings.push({ key, message: `Unknown config key "${key}" in global config` });
    }
  }

  for (const key of Object.keys(localConfig)) {
    if (!CONFIG_SCHEMA.find(f => f.key === key)) {
      warnings.push({ key, message: `Unknown config key "${key}" in local config` });
    }
  }

  // Check config file exists
  if (!fs.existsSync(CONFIG_FILE())) {
    warnings.push({ key: '', message: 'Global config file does not exist (using defaults)' });
  }

  return {
    valid: errors.filter(e => e.severity === 'error').length === 0,
    errors,
    warnings,
    configSource: Object.keys(globalConfig).length > 0 ? 'global' : 'default',
    configPath: CONFIG_FILE(),
    localConfigPath: LOCAL_CONFIG_FILE(),
  };
}

// Friendly key name mapping
const KEY_MAP: Record<string, string> = {
  'base-url': 'baseUrl',
  'base_url': 'baseUrl',
  'url': 'baseUrl',
  'api-key': 'apiKey',
  'api_key': 'apiKey',
  'key': 'apiKey',
  'model': 'defaultModel',
  'default-model': 'defaultModel',
  'format': 'outputFormat',
  'output-format': 'outputFormat',
  'timeout': 'timeout',
  'color': 'color',
  'agent-url': 'agentUrl',
  'agent_url': 'agentUrl',
};

function resolveKey(raw: string): string {
  return KEY_MAP[raw.toLowerCase()] || raw;
}

function maskValue(key: string, value: string | number | boolean): string {
  const field = CONFIG_SCHEMA.find(f => f.key === key);
  if (field?.sensitive && typeof value === 'string' && value.length > 8) {
    return value.substring(0, 8) + '...';
  }
  return String(value);
}

// ── Subcommand: list ───────────────────────────────────────────────

function handleList(ctx: CLIContext, outputJson: boolean): void {
  const sections: Record<string, Array<{ key: string; label: string; value: string | number | boolean | null; source: string; sensitive: boolean }>> = {};

  for (const field of CONFIG_SCHEMA) {
    if (!sections[field.section]) sections[field.section] = [];
    const { value, source } = getEffectiveValue(field.key);
    sections[field.section].push({
      key: field.key,
      label: field.label,
      value,
      source,
      sensitive: field.sensitive ?? false,
    });
  }

  // Add any extra keys not in schema
  const globalConfig = loadConfigFile();
  const localConfig = loadLocalConfig();
  for (const key of Object.keys({ ...globalConfig, ...localConfig })) {
    if (!CONFIG_SCHEMA.find(f => f.key === key)) {
      if (!sections['custom']) sections['custom'] = [];
      sections['custom'].push({
        key,
        label: key,
        value: globalConfig[key] ?? localConfig[key] ?? null,
        source: key in localConfig ? 'local' : 'config',
        sensitive: key.toLowerCase().includes('key') || key.toLowerCase().includes('secret'),
      });
    }
  }

  if (outputJson) {
    ctx.output.write(JSON.stringify({ sections, globalConfig: CONFIG_FILE(), localConfig: LOCAL_CONFIG_FILE() }, null, 2));
    return;
  }

  const sectionNames = Object.keys(sections);
  for (const sectionName of sectionNames) {
    ctx.output.write(ctx.output.colorize(`[${sectionName.toUpperCase()}]`, 'yellow'));
    for (const entry of sections[sectionName]) {
      let displayVal: string;
      if (entry.value === null || entry.value === undefined) {
        displayVal = ctx.output.colorize('(not set)', 'dim');
      } else if (entry.sensitive) {
        displayVal = ctx.output.colorize(maskValue(entry.key, entry.value), 'cyan');
      } else {
        displayVal = ctx.output.colorize(String(entry.value), 'cyan');
      }
      const sourceLabel = entry.source === 'default'
        ? ctx.output.colorize('default', 'dim')
        : entry.source === 'env'
          ? ctx.output.colorize('env', 'green')
          : entry.source === 'local'
            ? ctx.output.colorize('local', 'blue')
            : '';
      ctx.output.write(`  ${entry.label.padEnd(18)} ${displayVal}  ${sourceLabel}`);
    }
    ctx.output.write('');
  }

  ctx.output.info(`Global config: ${CONFIG_FILE()}`);
  ctx.output.info(`Local config:  ${LOCAL_CONFIG_FILE()}`);
}

// ── Subcommand: get ────────────────────────────────────────────────

function handleGet(key: string, ctx: CLIContext, outputJson: boolean): void {
  const configKey = resolveKey(key);
  const { value, source } = getEffectiveValue(configKey);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ key: configKey, value: value ?? null, source }, null, 2));
    return;
  }

  if (value === null || value === undefined) {
    ctx.output.writeError(`Config key "${configKey}" not found`);
    process.exit(1);
    return;
  }

  const display = maskValue(configKey, value);
  ctx.output.write(display + '\n');
}

// ── Subcommand: set ────────────────────────────────────────────────

function handleSet(key: string, value: string, ctx: CLIContext, outputJson: boolean): void {
  const configKey = resolveKey(key);
  const validation = validateValue(configKey, value);

  if (!validation.valid) {
    ctx.output.writeError(`Invalid value for "${configKey}": ${validation.error}`);
    process.exit(1);
    return;
  }

  const backupPath = backupConfig();
  const fileConfig = loadConfigFile();
  fileConfig[configKey] = validation.parsed;
  saveConfigFile(fileConfig);

  if (outputJson) {
    ctx.output.write(JSON.stringify({ key: configKey, value: validation.parsed, backup: backupPath }, null, 2));
  } else {
    ctx.output.success(`Set ${configKey} = ${validation.parsed}`);
    ctx.output.info(`Backup: ${backupPath}`);
  }
}

// ── Subcommand: reset ──────────────────────────────────────────────

function handleReset(keyOrSection: string | undefined, ctx: CLIContext, outputJson: boolean): void {
  const backupPath = backupConfig();

  if (keyOrSection) {
    // Check if it's a specific key
    const configKey = resolveKey(keyOrSection);
    const fileConfig = loadConfigFile();

    if (configKey in fileConfig || configKey in DEFAULT_CONFIG) {
      // Reset single key
      if (configKey in DEFAULT_CONFIG) {
        fileConfig[configKey] = DEFAULT_CONFIG[configKey];
      } else {
        delete fileConfig[configKey];
      }
      saveConfigFile(fileConfig);
      if (outputJson) {
        ctx.output.write(JSON.stringify({ resetKey: configKey, backup: backupPath }, null, 2));
      } else {
        ctx.output.success(`Reset "${configKey}" to default`);
        ctx.output.info(`Backup: ${backupPath}`);
      }
      return;
    }

    // Try as section
    const sectionKeys = CONFIG_SCHEMA.filter(f => f.section === keyOrSection).map(f => f.key);
    if (sectionKeys.length === 0) {
      ctx.output.writeError(`Unknown key or section: "${keyOrSection}"`);
      ctx.output.info(`Valid keys: ${CONFIG_SCHEMA.map(f => f.key).join(', ')}`);
      process.exit(1);
      return;
    }

    for (const k of sectionKeys) {
      if (k in DEFAULT_CONFIG) {
        fileConfig[k] = DEFAULT_CONFIG[k];
      } else {
        delete fileConfig[k];
      }
    }
    saveConfigFile(fileConfig);
    if (outputJson) {
      ctx.output.write(JSON.stringify({ resetSection: keyOrSection, backup: backupPath }, null, 2));
    } else {
      ctx.output.success(`Reset section "${keyOrSection}" to defaults`);
      ctx.output.info(`Backup: ${backupPath}`);
    }
  } else {
    // Reset all
    saveConfigFile({ ...DEFAULT_CONFIG });
    if (outputJson) {
      ctx.output.write(JSON.stringify({ reset: true, backup: backupPath }, null, 2));
    } else {
      ctx.output.success('Config reset to defaults');
      ctx.output.info(`Backup: ${backupPath}`);
    }
  }
}

// ── Subcommand: path ───────────────────────────────────────────────

function handlePath(ctx: CLIContext): void {
  ctx.output.write(ctx.output.colorize('Config file paths:', 'bold') + '\n');
  ctx.output.write(`  Global: ${ctx.output.colorize(CONFIG_FILE(), 'cyan')}\n`);
  ctx.output.write(`  Local:  ${ctx.output.colorize(LOCAL_CONFIG_FILE(), 'cyan')}\n`);
  ctx.output.write(`  Dir:    ${ctx.output.colorize(CONFIG_DIR(), 'dim')}\n`);
}

// ── Subcommand: validate ───────────────────────────────────────────

function handleValidate(ctx: CLIContext, outputJson: boolean): void {
  const result = validateFullConfig();

  if (outputJson) {
    ctx.output.write(JSON.stringify(result, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Config Validation', 'bold') + '\n');
  ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim') + '\n');

  ctx.output.write(`  Config path:   ${result.configPath}\n`);
  ctx.output.write(`  Local path:    ${result.localConfigPath}\n`);
  ctx.output.write(`  Config source: ${result.configSource}\n`);

  if (result.valid) {
    ctx.output.write('\n' + ctx.output.colorize('  \u2713 Configuration is valid', 'green') + '\n');
  } else {
    ctx.output.write('\n' + ctx.output.colorize('  \u2717 Configuration has errors', 'red') + '\n');
  }

  if (result.errors.length > 0) {
    ctx.output.write('\n  ' + ctx.output.colorize('Errors:', 'red') + '\n');
    for (const err of result.errors) {
      const icon = err.severity === 'error' ? ctx.output.colorize('\u2717', 'red') : ctx.output.colorize('\u26A0', 'yellow');
      ctx.output.write(`    ${icon} ${err.key ? `[${err.key}] ` : ''}${err.message}\n`);
    }
  }

  if (result.warnings.length > 0) {
    ctx.output.write('\n  ' + ctx.output.colorize('Warnings:', 'yellow') + '\n');
    for (const warn of result.warnings) {
      ctx.output.write(`    ${ctx.output.colorize('\u26A0', 'yellow')} ${warn.key ? `[${warn.key}] ` : ''}${warn.message}\n`);
    }
  }

  ctx.output.write('');
}

// ── Subcommand: edit (alias to edit-config) ────────────────────────

async function handleEdit(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const { editConfigAction } = await import('./edit-config');
  await editConfigAction(args, ctx);
}

// ── Profile subcommands ────────────────────────────────────────────

async function handleProfileSubcommand(
  subArgs: string[],
  ctx: CLIContext,
  outputJson: boolean,
): Promise<void> {
  const {
    listProfiles,
    useProfile,
    setProfile,
    getCurrentProfile,
    deleteProfile,
  } = await import('../../config/profiles');

  const subcommand = subArgs[0];

  switch (subcommand) {
    case 'list': {
      const profiles = listProfiles();
      if (outputJson) {
        ctx.output.write(ctx.output.formatOutput(profiles));
      } else {
        ctx.output.write(ctx.output.colorize('Config Profiles', 'bold'));
        ctx.output.write('');
        for (const p of profiles) {
          const marker = p.active ? ctx.output.colorize('  * ', 'green') : '    ';
          const name = p.active
            ? ctx.output.colorize(p.name, 'green')
            : ctx.output.colorize(p.name, 'dim');
          ctx.output.write(`${marker}${name}  ${p.config.baseUrl}`);
          if (p.config.defaultModel) {
            ctx.output.write(`      model: ${p.config.defaultModel}`);
          }
          if (p.config.apiKey) {
            ctx.output.write(`      key: ${p.config.apiKey.substring(0, 8)}...`);
          }
          if (p.config.timeout) {
            ctx.output.write(`      timeout: ${p.config.timeout}ms`);
          }
          ctx.output.write('');
        }
      }
      break;
    }

    case 'use': {
      const profileName = subArgs[1];
      if (!profileName) {
        ctx.output.writeError('Usage: xergon config profile use <name>');
        ctx.output.info('Available profiles: default, dev, staging, production');
        process.exit(1);
        return;
      }
      try {
        const profile = useProfile(profileName);
        ctx.output.success(`Switched to profile "${profile.name}"`);
        ctx.output.write(`  Base URL: ${profile.baseUrl}`);
        if (profile.defaultModel) {
          ctx.output.write(`  Model:    ${profile.defaultModel}`);
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        ctx.output.writeError(msg);
        process.exit(1);
        return;
      }
      break;
    }

    case 'set': {
      const profileName = subArgs[1];
      if (!profileName) {
        ctx.output.writeError('Usage: xergon config profile set <name> [--url ...] [--key ...] [--model ...] [--timeout ...]');
        process.exit(1);
        return;
      }

      const profileConfig: Record<string, string> = {};
      const argv = process.argv;

      let foundSet = false;
      let foundProfileName = false;
      for (let j = 0; j < argv.length; j++) {
        if (argv[j] === 'config') continue;
        if (argv[j] === 'profile') continue;
        if (argv[j] === 'set') {
          foundSet = true;
          continue;
        }
        if (foundSet && !foundProfileName) {
          foundProfileName = true;
          continue;
        }
        if (foundSet && foundProfileName) {
          if (argv[j] === '--url' || argv[j] === '--base-url') {
            profileConfig.baseUrl = argv[++j];
          } else if (argv[j] === '--key' || argv[j] === '--api-key') {
            profileConfig.apiKey = argv[++j];
          } else if (argv[j] === '--model') {
            profileConfig.defaultModel = argv[++j];
          } else if (argv[j] === '--timeout') {
            profileConfig.timeout = argv[++j];
          } else if (argv[j].startsWith('--url=')) {
            profileConfig.baseUrl = argv[j].substring('--url='.length);
          } else if (argv[j].startsWith('--key=')) {
            profileConfig.apiKey = argv[j].substring('--key='.length);
          } else if (argv[j].startsWith('--model=')) {
            profileConfig.defaultModel = argv[j].substring('--model='.length);
          } else if (argv[j].startsWith('--timeout=')) {
            profileConfig.timeout = argv[j].substring('--timeout='.length);
          }
        }
      }

      if (Object.keys(profileConfig).length === 0) {
        ctx.output.writeError('No profile options specified. Use --url, --key, --model, or --timeout.');
        process.exit(1);
        return;
      }

      const configToSet: Record<string, unknown> = {};
      if (profileConfig.baseUrl) configToSet.baseUrl = profileConfig.baseUrl;
      if (profileConfig.apiKey) configToSet.apiKey = profileConfig.apiKey;
      if (profileConfig.defaultModel) configToSet.defaultModel = profileConfig.defaultModel;
      if (profileConfig.timeout) configToSet.timeout = Number(profileConfig.timeout);

      const saved = setProfile(profileName, configToSet);
      ctx.output.success(`Profile "${profileName}" updated`);
      ctx.output.write(`  Base URL: ${saved.baseUrl}`);
      if (saved.defaultModel) ctx.output.write(`  Model:    ${saved.defaultModel}`);
      if (saved.apiKey) ctx.output.write(`  API Key:  ${saved.apiKey.substring(0, 8)}...`);
      if (saved.timeout) ctx.output.write(`  Timeout:  ${saved.timeout}ms`);
      break;
    }

    case 'show':
    case 'get': {
      const profile = getCurrentProfile();
      if (outputJson) {
        ctx.output.write(ctx.output.formatOutput(profile));
      } else {
        ctx.output.write(ctx.output.formatText(profile, `Active Profile: ${profile.name}`));
      }
      break;
    }

    case 'delete':
    case 'remove': {
      const profileName = subArgs[1];
      if (!profileName) {
        ctx.output.writeError('Usage: xergon config profile delete <name>');
        process.exit(1);
        return;
      }
      try {
        deleteProfile(profileName);
        ctx.output.success(`Profile "${profileName}" deleted`);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        ctx.output.writeError(msg);
        process.exit(1);
        return;
      }
      break;
    }

    default: {
      ctx.output.writeError(`Unknown profile subcommand: "${subcommand}"`);
      ctx.output.write('');
      ctx.output.write('Available subcommands:');
      ctx.output.write('  xergon config profile list              List all profiles');
      ctx.output.write('  xergon config profile use <name>        Switch active profile');
      ctx.output.write('  xergon config profile set <name> ...    Create/update a profile');
      ctx.output.write('  xergon config profile show              Show active profile');
      ctx.output.write('  xergon config profile delete <name>     Delete a custom profile');
      process.exit(1);
      return;
    }
  }
}

// ── Options ────────────────────────────────────────────────────────

const configOptions: CommandOption[] = [
  {
    name: 'set',
    short: '',
    long: '--set',
    description: 'Set a config value (key=value)',
    required: false,
    type: 'string',
  },
  {
    name: 'get',
    short: '',
    long: '--get',
    description: 'Get a config value',
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
    name: 'section',
    short: '',
    long: '--section',
    description: 'Section to reset (used with --reset)',
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

// ── Command action ─────────────────────────────────────────────────

async function configAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const firstPos = args.positional[0];

  // Delegate to profile subcommand
  if (firstPos === 'profile') {
    await handleProfileSubcommand(args.positional.slice(1), ctx, outputJson);
    return;
  }

  // Delegate to interactive editor
  if (firstPos === 'edit') {
    await handleEdit(args, ctx);
    return;
  }

  // Subcommand: list
  if (firstPos === 'list') {
    handleList(ctx, outputJson);
    return;
  }

  // Subcommand: get
  if (firstPos === 'get') {
    const key = args.positional[1];
    if (!key) {
      ctx.output.writeError('Usage: xergon config get <key>');
      process.exit(1);
      return;
    }
    handleGet(key, ctx, outputJson);
    return;
  }

  // Subcommand: set
  if (firstPos === 'set') {
    const key = args.positional[1];
    const value = args.positional.slice(2).join(' ') || (args.options.set ? String(args.options.set) : undefined);
    if (!key || !value) {
      ctx.output.writeError('Usage: xergon config set <key> <value> or xergon config --set <key>=<value>');
      process.exit(1);
      return;
    }
    handleSet(key, value, ctx, outputJson);
    return;
  }

  // Subcommand: reset
  if (firstPos === 'reset') {
    const keyOrSection = args.positional[1];
    handleReset(keyOrSection, ctx, outputJson);
    return;
  }

  // Subcommand: path
  if (firstPos === 'path') {
    handlePath(ctx);
    return;
  }

  // Subcommand: validate
  if (firstPos === 'validate') {
    handleValidate(ctx, outputJson);
    return;
  }

  // Legacy --set flag (backward compat)
  const setValue = args.options.set ? String(args.options.set) : undefined;
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
      ctx.output.writeError('Usage: xergon config set <key>=<value> or xergon config set <key> <value>');
      process.exit(1);
      return;
    }

    handleSet(key, value, ctx, outputJson);
    return;
  }

  // Default: show current config
  const currentConfig = {
    baseUrl: ctx.config.baseUrl,
    apiKey: ctx.config.apiKey ? `${ctx.config.apiKey.substring(0, 8)}...` : '(not set)',
    defaultModel: ctx.config.defaultModel,
    outputFormat: ctx.config.outputFormat,
    color: ctx.config.color,
    timeout: ctx.config.timeout,
    globalConfig: CONFIG_FILE(),
    localConfig: LOCAL_CONFIG_FILE(),
  };

  if (outputJson) {
    ctx.output.write(ctx.output.formatOutput(currentConfig));
  } else {
    ctx.output.write(ctx.output.formatText(currentConfig, 'Current Configuration'));
  }
}

// ── Command definition ─────────────────────────────────────────────

export const configCommand: Command = {
  name: 'config',
  description: 'View and manage CLI configuration and profiles',
  aliases: ['settings', 'cfg'],
  options: configOptions,
  action: configAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  validateValue,
  validateFullConfig,
  resolveKey,
  maskValue,
  getEffectiveValue,
  getEnvOverride,
  loadConfigFile,
  loadLocalConfig,
  saveConfigFile,
  backupConfig,
  configAction,
  DEFAULT_CONFIG,
  CONFIG_SCHEMA,
  KEY_MAP,
  type ConfigField,
  type ValidationResult,
  type EffectiveConfig,
};
