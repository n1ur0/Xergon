#!/usr/bin/env node

/**
 * Xergon CLI -- main entry point.
 *
 * Usage: xergon <command> [options] [arguments]
 *
 * Supports XERGON_API_KEY and XERGON_BASE_URL environment variables,
 * --config flag for config file path, and all standard CLI patterns.
 */

import { ArgumentParser, OutputFormatter, CLIError, type CLIConfig, type ParsedArgs } from './mod';
import type { Command } from './mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// Lazy import commands (avoids loading all at startup)
const commandModules: Record<string, () => Promise<{ [key: string]: Command }>> = {
  chat: () => import('./commands/chat').then(m => ({ chat: m.chatCommand })),
  models: () => import('./commands/models').then(m => ({ models: m.modelsCommand })),
  provider: () => import('./commands/provider').then(m => ({ provider: m.providerCommand })),
  config: () => import('./commands/config').then(m => ({ config: m.configCommand })),
  balance: () => import('./commands/balance').then(m => ({ balance: m.balanceCommand })),
  version: () => import('./commands/version').then(m => ({ version: m.versionCommand })),
  onboard: () => import('./commands/onboard').then(m => ({ onboard: m.onboardCommand })),
  login: () => import('./commands/login').then(m => ({ login: m.loginCommand })),
  logout: () => import('./commands/login').then(m => ({ logout: m.logoutCommand })),
  completion: () => import('./commands/completion').then(m => ({ completion: m.completionCommand })),
  serve: () => import('./commands/serve').then(m => ({ serve: m.serveCommand })),
  validate: () => import('./commands/validate').then(m => ({ validate: m.validateCommand })),
  logs: () => import('./commands/logs').then(m => ({ logs: m.logsCommand })),
  status: () => import('./commands/status').then(m => ({ status: m.statusCommand })),
  embed: () => import('./commands/embed').then(m => ({ embed: m.embedCommand })),
  audio: () => import('./commands/audio').then(m => ({ audio: m.audioCommand })),
  upload: () => import('./commands/upload').then(m => ({ upload: m.uploadCommand })),
  'fine-tune': () => import('./commands/fine-tune').then(m => ({ 'fine-tune': m.fineTuneCommand })),
  deploy: () => import('./commands/deploy').then(m => ({ deploy: m.deployCommand })),
  monitor: () => import('./commands/monitor').then(m => ({ monitor: m.monitorCommand })),
  'edit-config': () => import('./commands/edit-config').then(m => ({ 'edit-config': m.editConfigCommand })),
  plugin: () => import('./commands/plugin').then(m => ({ plugin: m.pluginCommand })),
  bench: () => import('./commands/bench').then(m => ({ bench: m.benchCommand })),
  workspace: () => import('./commands/workspace').then(m => ({ workspace: m.workspaceCommand })),
  template: () => import('./commands/template').then(m => ({ template: m.templateCommand })),
  alias: () => import('./commands/alias').then(m => ({ alias: m.aliasCommand })),
  inspect: () => import('./commands/inspect').then(m => ({ inspect: m.inspectCommand })),
  flow: () => import('./commands/flow').then(m => ({ flow: m.flowCommand })),
  log: () => import('./commands/log').then(m => ({ log: m.logCommand })),
  eval: () => import('./commands/eval').then(m => ({ eval: m.evalCommand })),
  canary: () => import('./commands/canary').then(m => ({ canary: m.canaryCommand })),
  export: () => import('./commands/export').then(m => ({ export: m.exportCommand })),
  team: () => import('./commands/team').then(m => ({ team: m.teamCommand })),
  webhook: () => import('./commands/webhook').then(m => ({ webhook: m.webhookCommand })),
  model: () => import('./commands/model-registry').then(m => ({ model: m.modelRegistryCommand })),
  debug: () => import('./commands/debug').then(m => ({ debug: m.debugCommand })),
  docs: () => import('./commands/docs').then(m => ({ docs: m.docsCommand })),
  gateway: () => import('./commands/gateway').then(m => ({ gateway: m.gatewayCommand })),
  chain: () => import('./commands/chain').then(m => ({ chain: m.chainCommand })),
  price: () => import('./commands/price').then(m => ({ price: m.priceCommand })),
  governance: () => import('./commands/governance').then(m => ({ governance: m.governanceCommand })),
  train: () => import('./commands/train').then(m => ({ train: m.trainCommand })),
  benchmark: () => import('./commands/benchmark').then(m => ({ benchmark: m.benchmarkCommand })),
  proof: () => import('./commands/proof').then(m => ({ proof: m.proofCommand })),
  trust: () => import('./commands/trust').then(m => ({ trust: m.trustCommand })),
  attest: () => import('./commands/attest').then(m => ({ attest: m.attestCommand })),
  verify: () => import('./commands/verify').then(m => ({ verify: m.verifyCommand })),
  update: () => import('./commands/update').then(m => ({ update: m.updateCommand })),
  fleet: () => import('./commands/fleet').then(m => ({ fleet: m.fleetCommand })),
  test: () => import('./commands/test').then(m => ({ test: m.testCommand })),
  settlement: () => import('./commands/settlement').then(m => ({ settlement: m.settlementCommand })),
  auth: () => import('./commands/auth').then(m => ({ auth: m.authCommand })),
  metrics: () => import('./commands/metrics').then(m => ({ metrics: m.metricsCommand })),
  org: () => import('./commands/org').then(m => ({ org: m.orgCommand })),
  rent: () => import('./commands/rent').then(m => ({ rent: m.rentCommand })),
  treasury: () => import('./commands/treasury').then(m => ({ treasury: m.treasuryCommand })),
  ensemble: () => import('./commands/ensemble').then(m => ({ ensemble: m.ensembleCommand })),
};

// Cache for loaded commands
const commandCache: Map<string, Command> = new Map();

async function getCommand(name: string): Promise<Command | undefined> {
  if (commandCache.has(name)) return commandCache.get(name);

  const loader = commandModules[name];
  if (!loader) return undefined;

  const mod = await loader();
  const cmd = Object.values(mod)[0];
  if (cmd) {
    commandCache.set(cmd.name, cmd);
    for (const alias of cmd.aliases) {
      commandCache.set(alias, cmd);
    }
  }
  return cmd;
}

/**
 * Load configuration from file, environment variables, and active profile.
 */
function loadConfig(args: string[]): CLIConfig {
  // Check for --config flag
  let configPath: string | undefined;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--config' && args[i + 1]) {
      configPath = args[i + 1];
      break;
    }
    if (args[i].startsWith('--config=')) {
      configPath = args[i].substring('--config='.length);
      break;
    }
  }

  const defaultConfigPath = path.join(os.homedir(), '.xergon', 'config.json');
  const resolvedPath = configPath || defaultConfigPath;

  let fileConfig: Record<string, string | number | boolean> = {};
  try {
    const data = fs.readFileSync(resolvedPath, 'utf-8');
    fileConfig = JSON.parse(data);
  } catch {
    // No config file -- that's OK
  }

  // Load active profile for defaults
  let profileConfig: Record<string, string | number | boolean> = {};
  try {
    const profilesPath = path.join(os.homedir(), '.xergon', 'profiles.json');
    const data = fs.readFileSync(profilesPath, 'utf-8');
    const parsed = JSON.parse(data);
    const activeName = parsed.activeProfile;
    if (activeName && parsed.profiles?.[activeName]) {
      profileConfig = parsed.profiles[activeName];
    }
  } catch {
    // No profiles file -- that's OK
  }

  // Environment variables take precedence over file config and profiles
  const envApiKey = process.env.XERGON_API_KEY || process.env.XERGON_PUBLIC_KEY;
  const envBaseUrl = process.env.XERGON_BASE_URL;
  const envModel = process.env.XERGON_DEFAULT_MODEL;
  const envFormat = process.env.XERGON_OUTPUT_FORMAT;
  const envTimeout = process.env.XERGON_TIMEOUT;

  // Priority: env var > file config > profile config > hardcoded default
  const resolve = (env: string | undefined, file: string | number | boolean | undefined, profile: string | number | boolean | undefined, fallback: string | number | boolean) =>
    env ?? (file !== undefined && file !== '' ? file : (profile !== undefined && profile !== '' ? profile : fallback));

  return {
    baseUrl: String(resolve(envBaseUrl, fileConfig.baseUrl, profileConfig.baseUrl, 'https://relay.xergon.gg')),
    apiKey: String(resolve(envApiKey, fileConfig.apiKey, profileConfig.apiKey, '')),
    defaultModel: String(resolve(envModel, fileConfig.defaultModel, profileConfig.defaultModel, 'llama-3.3-70b')),
    outputFormat: ((resolve(envFormat, fileConfig.outputFormat, profileConfig.outputFormat, 'text')) as 'text' | 'json' | 'table'),
    color: Boolean(fileConfig.color !== false && !process.env.NO_COLOR),
    timeout: Number(resolve(envTimeout, fileConfig.timeout, profileConfig.timeout, 30000)),
  };
}

/**
 * Create an XergonClient from CLI config.
 */
async function createClient(config: CLIConfig): Promise<any> {
  // Dynamic import to keep CLI entry lightweight
  const { XergonClient } = await import('../index');
  const client = new XergonClient({ baseUrl: config.baseUrl });
  if (config.apiKey) {
    client.setPublicKey(config.apiKey);
  }
  return client;
}

/**
 * Handle the 'help' command.
 */
async function handleHelp(parser: ArgumentParser, args: ParsedArgs): Promise<void> {
  const formatter = new OutputFormatter();
  const subCommand = args.positional[0];
  if (subCommand) {
    const cmd = await getCommand(subCommand);
    if (cmd) {
      formatter.write(parser.generateHelp(cmd.name));
    } else {
      formatter.writeError(`Unknown command: ${subCommand}`);
      formatter.write('\n' + parser.generateHelp());
    }
  } else {
    formatter.write(parser.generateHelp());
  }
}

/**
 * Handle the 'version' command.
 */
async function handleVersion(): Promise<void> {
  const formatter = new OutputFormatter();
  const cmd = await getCommand('version');
  if (cmd) {
    await cmd.action({ command: 'version', positional: [], options: {} }, {
      client: null,
      config: {} as CLIConfig,
      output: formatter,
    });
  } else {
    formatter.write('xergon-cli v0.1.0\n');
  }
}

/**
 * Main entry point.
 */
async function main(): Promise<void> {
  const args = process.argv;
  const config = loadConfig(args);
  const formatter = new OutputFormatter(config.outputFormat, config.color);

  // Handle --json global flag
  for (const arg of args) {
    if (arg === '--json' || arg === '-j') {
      formatter.setFormat('json');
      break;
    }
  }

  const parser = new ArgumentParser('xergon', '0.1.0');

  // Register global options
  parser.addGlobalOption({
    name: 'json',
    short: '-j',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  });
  parser.addGlobalOption({
    name: 'config',
    short: '',
    long: '--config',
    description: 'Path to config file',
    required: false,
    type: 'string',
  });
  parser.addGlobalOption({
    name: 'help',
    short: '-h',
    long: '--help',
    description: 'Show help',
    required: false,
    type: 'boolean',
  });

  try {
    const parsed = parser.parse(args);

    switch (parsed.command) {
      case 'help':
        await handleHelp(parser, parsed);
        break;

      case 'version':
        await handleVersion();
        break;

      case 'unknown':
        formatter.writeError(`Unknown command: ${parsed.positional[0]}`);
        formatter.write('\n' + parser.generateHelp());
        process.exit(1);
        break;

      default: {
        const cmd = await getCommand(parsed.command);
        if (!cmd) {
          formatter.writeError(`Unknown command: ${parsed.command}`);
          formatter.write('\n' + parser.generateHelp());
          process.exit(1);
          return; // unreachable, satisfies type narrowing
        }

        // Check for --help on the command
        if (parsed.options.help) {
          formatter.write(parser.generateHelp(cmd.name));
          break;
        }

        const client = await createClient(config);
        const ctx = { client, config, output: formatter };

        await cmd.action(parsed, ctx);
        break;
      }
    }
  } catch (err) {
    if (err instanceof CLIError) {
      formatter.writeError(err.message);
      process.exit(err.exitCode);
    }

    const message = err instanceof Error ? err.message : String(err);
    formatter.writeError(message);
    process.exit(1);
  }
}

// Run
main().catch((err) => {
  const message = err instanceof Error ? err.message : String(err);
  process.stderr.write(`Error: ${message}\n`);
  process.exit(1);
});
