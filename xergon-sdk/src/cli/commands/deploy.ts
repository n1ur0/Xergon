/**
 * CLI command: deploy
 *
 * Manage deployments on the Xergon Network.
 * Initialize environments, plan deployments, push with health checks,
 * rollback, view history, status, promote across environments, and
 * manage deployment configuration.
 *
 * Usage:
 *   xergon deploy                          Show deployment status summary
 *   xergon deploy init <env>               Initialize deployment config for environment
 *   xergon deploy plan                     Show what would be deployed (dry-run)
 *   xergon deploy push <env>               Push deployment to environment
 *   xergon deploy rollback <env> --to <ver> Roll back to specific version
 *   xergon deploy history <env>            Show deployment history
 *   xergon deploy status <env>             Show current deployment status
 *   xergon deploy promote <from> <to>      Promote from staging to prod
 *   xergon deploy config <env> --set <k=v> Update deployment config
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

interface DeployConfig {
  environment: string;
  version: string;
  relayUrl: string;
  agentUrl: string;
  marketplaceUrl: string;
  healthCheckPath: string;
  healthCheckTimeout: number;
  maxRetries: number;
  rollbackOnFailure: boolean;
  canaryPercent: number;
}

interface DeployRecord {
  id: string;
  env: string;
  version: string;
  status: 'pending' | 'running' | 'success' | 'failed' | 'rolled_back';
  startedAt: string;
  completedAt?: string;
  rollbackFrom?: string;
  deployer: string;
  healthChecks: HealthCheckResult[];
}

interface HealthCheckResult {
  name: string;
  url: string;
  status: 'pass' | 'fail';
  latencyMs: number;
  responseCode?: number;
}

interface DeployPlan {
  currentVersion: string;
  targetVersion: string;
  changes: string[];
  healthChecks: string[];
  estimatedDowntime: string;
  canary: boolean;
}

interface DeployHistory {
  deployments: DeployRecord[];
  env: string;
  summary: { total: number; success: number; failed: number };
}

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

/** Read a line from stdin (non-interactive fallback returns empty string). */
async function readLine(prompt: string): Promise<string> {
  if (!process.stdin.isTTY) return '';
  process.stdout.write(prompt);
  return new Promise<string>((resolve) => {
    const rl = require('node:readline').createInterface({ input: process.stdin, output: process.stdout });
    rl.question('', (answer: string) => {
      rl.close();
      resolve(answer.trim());
    });
  });
}

function getHomeDir(): string {
  return process.env.HOME || process.env.USERPROFILE || '/tmp';
}

function deployDir(): string {
  return `${getHomeDir()}/.xergon/deploy`;
}

function configPath(env: string): string {
  return `${deployDir()}/${env}.json`;
}

function historyPath(env: string): string {
  return `${deployDir()}/${env}-history.json`;
}

function generateId(): string {
  return `deploy_${Date.now()}_${Math.random().toString(36).substring(2, 10)}`;
}

function ensureDir(path: string): void {
  const fs = require('node:fs');
  if (!fs.existsSync(path)) {
    fs.mkdirSync(path, { recursive: true });
  }
}

function readJson<T>(path: string): T | null {
  try {
    const fs = require('node:fs');
    const data = fs.readFileSync(path, 'utf-8');
    return JSON.parse(data) as T;
  } catch {
    return null;
  }
}

function writeJson(path: string, data: unknown): void {
  const fs = require('node:fs');
  ensureDir(require('node:path').dirname(path));
  fs.writeFileSync(path, JSON.stringify(data, null, 2), 'utf-8');
}

function defaultConfig(env: string): DeployConfig {
  return {
    environment: env,
    version: '0.0.0',
    relayUrl: `https://relay.${env}.xergon.network`,
    agentUrl: `https://agent.${env}.xergon.network`,
    marketplaceUrl: `https://marketplace.${env}.xergon.network`,
    healthCheckPath: '/health',
    healthCheckTimeout: 30000,
    maxRetries: 3,
    rollbackOnFailure: true,
    canaryPercent: 10,
  };
}

function statusColor(status: string): 'green' | 'red' | 'yellow' | 'cyan' | 'dim' {
  switch (status) {
    case 'success': case 'healthy': return 'green';
    case 'failed': return 'red';
    case 'pending': case 'running': case 'canary': return 'yellow';
    case 'rolled_back': return 'cyan';
    default: return 'dim';
  }
}

/** Run an HTTP GET health check. */
async function performHealthCheck(url: string, timeoutMs: number): Promise<HealthCheckResult> {
  const start = Date.now();
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    const response = await fetch(url, { signal: controller.signal, method: 'GET' });
    clearTimeout(timer);
    const latency = Date.now() - start;
    const passed = response.status === 200;
    return {
      name: url,
      url,
      status: passed ? 'pass' : 'fail',
      latencyMs: latency,
      responseCode: response.status,
    };
  } catch {
    return {
      name: url,
      url,
      status: 'fail',
      latencyMs: Date.now() - start,
    };
  }
}

// ── Subcommand: (no args) - Status Summary ────────────────────────

async function handleSummary(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const validEnvs = ['dev', 'staging', 'prod'];
  const fs = require('node:fs');

  const summaries: { env: string; version: string; status: string }[] = [];

  for (const env of validEnvs) {
    const cfg = readJson<DeployConfig>(configPath(env));
    const hist = readJson<DeployHistory>(historyPath(env));
    const lastDeploy = hist?.deployments?.[0];
    summaries.push({
      env,
      version: cfg?.version || 'not initialized',
      status: cfg ? (lastDeploy?.status || 'no deployments') : 'not initialized',
    });
  }

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(summaries, null, 2));
    return;
  }

  if (isTableFormat(args)) {
    const tableData = summaries.map(s => ({
      Environment: s.env,
      Version: s.version,
      Status: s.status,
    }));
    ctx.output.write(ctx.output.formatTable(tableData, 'Deployment Status Summary'));
    return;
  }

  ctx.output.write(ctx.output.colorize('Deployment Status Summary', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
  for (const s of summaries) {
    const envColor = s.env === 'prod' ? 'red' : s.env === 'staging' ? 'yellow' : 'green';
    ctx.output.write(`  ${ctx.output.colorize(s.env.toUpperCase(), envColor)}`);
    ctx.output.write(`    Version: ${s.version}`);
    ctx.output.write(`    Status:  ${ctx.output.colorize(s.status, statusColor(s.status))}`);
    ctx.output.write('');
  }
}

// ── Subcommand: init ──────────────────────────────────────────────

async function handleInit(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.positional[0];
  if (!env) {
    ctx.output.writeError('Usage: xergon deploy init <env>');
    ctx.output.write('Environments: dev, staging, prod');
    process.exit(1);
    return;
  }

  if (!['dev', 'staging', 'prod'].includes(env)) {
    ctx.output.writeError(`Invalid environment: ${env}. Must be dev, staging, or prod.`);
    process.exit(1);
    return;
  }

  const existing = readJson<DeployConfig>(configPath(env));
  if (existing) {
    ctx.output.warn(`Environment "${env}" already initialized at ${configPath(env)}`);
    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(existing, null, 2));
    }
    return;
  }

  const cfg = defaultConfig(env);
  writeJson(configPath(env), cfg);

  // Initialize empty history
  const history: DeployHistory = { deployments: [], env, summary: { total: 0, success: 0, failed: 0 } };
  writeJson(historyPath(env), history);

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(cfg, null, 2));
    return;
  }

  ctx.output.success(`Initialized deployment config for "${env}"`);
  ctx.output.write('');
  ctx.output.write(ctx.output.formatText({
    Environment: cfg.environment,
    'Relay URL': cfg.relayUrl,
    'Agent URL': cfg.agentUrl,
    'Marketplace URL': cfg.marketplaceUrl,
    'Health Check Path': cfg.healthCheckPath,
    'Health Check Timeout': `${cfg.healthCheckTimeout}ms`,
    'Max Retries': String(cfg.maxRetries),
    'Rollback on Failure': String(cfg.rollbackOnFailure),
    'Canary Percent': `${cfg.canaryPercent}%`,
  }, `Config: ~/.xergon/deploy/${env}.json`));
}

// ── Subcommand: plan ──────────────────────────────────────────────

async function handlePlan(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.options.env ? String(args.options.env) : undefined;
  if (!env) {
    ctx.output.writeError('Usage: xergon deploy plan --env <env>');
    process.exit(1);
    return;
  }

  const cfg = readJson<DeployConfig>(configPath(env));
  if (!cfg) {
    ctx.output.writeError(`Environment "${env}" not initialized. Run: xergon deploy init ${env}`);
    process.exit(1);
    return;
  }

  const hist = readJson<DeployHistory>(historyPath(env));
  const lastDeploy = hist?.deployments?.[0];
  const currentVersion = lastDeploy?.version || 'none';

  const targetVersion = args.options.version ? String(args.options.version) : (await readLine('Target version: '));
  if (!targetVersion) {
    ctx.output.writeError('Target version is required. Use --version or enter interactively.');
    process.exit(1);
    return;
  }

  const changes: string[] = [];
  if (currentVersion === 'none') {
    changes.push(`Initial deployment of version ${targetVersion}`);
  } else {
    changes.push(`Upgrade from ${currentVersion} to ${targetVersion}`);
  }
  changes.push(`Deploy to ${cfg.relayUrl}`);
  changes.push(`Deploy to ${cfg.agentUrl}`);
  changes.push(`Deploy to ${cfg.marketplaceUrl}`);

  const healthChecks = [
    `${cfg.relayUrl}${cfg.healthCheckPath}`,
    `${cfg.agentUrl}${cfg.healthCheckPath}`,
    `${cfg.marketplaceUrl}${cfg.healthCheckPath}`,
  ];

  const plan: DeployPlan = {
    currentVersion,
    targetVersion,
    changes,
    healthChecks,
    estimatedDowntime: cfg.canaryPercent > 0 ? '< 1s (canary)' : '~5s (blue-green)',
    canary: cfg.canaryPercent > 0,
  };

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(plan, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Deployment Plan (dry-run)', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
  ctx.output.write('');
  ctx.output.write(`  ${ctx.output.colorize('Current Version:', 'cyan')}  ${plan.currentVersion}`);
  ctx.output.write(`  ${ctx.output.colorize('Target Version:', 'cyan')}   ${plan.targetVersion}`);
  ctx.output.write(`  ${ctx.output.colorize('Canary:', 'cyan')}           ${plan.canary ? `Yes (${cfg.canaryPercent}%)` : 'No'}`);
  ctx.output.write(`  ${ctx.output.colorize('Est. Downtime:', 'cyan')}    ${plan.estimatedDowntime}`);
  ctx.output.write('');
  ctx.output.write(ctx.output.colorize('  Changes:', 'bold'));
  for (const change of plan.changes) {
    ctx.output.write(`    ${ctx.output.colorize('+', 'green')} ${change}`);
  }
  ctx.output.write('');
  ctx.output.write(ctx.output.colorize('  Health Checks:', 'bold'));
  for (const hc of plan.healthChecks) {
    ctx.output.write(`    ${ctx.output.colorize('>', 'cyan')} ${hc}`);
  }
  ctx.output.write('');
  ctx.output.info('This is a dry-run. No changes were made.');
}

// ── Subcommand: push ──────────────────────────────────────────────

async function handlePush(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.positional[0];
  if (!env) {
    ctx.output.writeError('Usage: xergon deploy push <env> [--version <ver>]');
    process.exit(1);
    return;
  }

  const cfg = readJson<DeployConfig>(configPath(env));
  if (!cfg) {
    ctx.output.writeError(`Environment "${env}" not initialized. Run: xergon deploy init ${env}`);
    process.exit(1);
    return;
  }

  const targetVersion = args.options.version ? String(args.options.version) : (await readLine('Version to deploy: '));
  if (!targetVersion) {
    ctx.output.writeError('Version is required. Use --version or enter interactively.');
    process.exit(1);
    return;
  }

  const deployer = args.options.deployer ? String(args.options.deployer) : 'local-cli';
  const deployId = generateId();
  const startedAt = new Date().toISOString();

  ctx.output.info(`Starting deployment ${deployId} to ${env}...`);
  ctx.output.write(`  Version: ${targetVersion}`);
  ctx.output.write(`  Deployer: ${deployer}`);
  ctx.output.write('');

  // Create deploy record
  const record: DeployRecord = {
    id: deployId,
    env,
    version: targetVersion,
    status: 'running',
    startedAt,
    deployer,
    healthChecks: [],
  };

  // Run health checks
  const healthUrls = [
    { name: 'relay', url: `${cfg.relayUrl}${cfg.healthCheckPath}` },
    { name: 'agent', url: `${cfg.agentUrl}${cfg.healthCheckPath}` },
    { name: 'marketplace', url: `${cfg.marketplaceUrl}${cfg.healthCheckPath}` },
  ];

  ctx.output.write(ctx.output.colorize('Running health checks...', 'bold'));
  let allPassed = true;

  for (const hc of healthUrls) {
    const result = await performHealthCheck(hc.url, cfg.healthCheckTimeout);
    record.healthChecks.push(result);

    const icon = result.status === 'pass' ? ctx.output.colorize('✓', 'green') : ctx.output.colorize('✗', 'red');
    ctx.output.write(`  ${icon} ${hc.name}: ${result.url} (${result.latencyMs}ms)`);
    if (result.responseCode) {
      ctx.output.write(`    Status: ${result.responseCode}`);
    }
    if (result.status !== 'pass') {
      allPassed = false;
    }
  }

  ctx.output.write('');

  if (!allPassed && cfg.rollbackOnFailure) {
    record.status = 'failed';
    record.completedAt = new Date().toISOString();

    const hist = readJson<DeployHistory>(historyPath(env)) || { deployments: [], env, summary: { total: 0, success: 0, failed: 0 } };
    hist.deployments.unshift(record);
    hist.summary.total += 1;
    hist.summary.failed += 1;
    writeJson(historyPath(env), hist);

    ctx.output.writeError(`Health checks failed. Deployment ${deployId} aborted.`);
    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(record, null, 2));
    }
    process.exit(1);
    return;
  }

  // Success path
  record.status = 'success';
  record.completedAt = new Date().toISOString();

  // Update config version
  cfg.version = targetVersion;
  writeJson(configPath(env), cfg);

  // Save history
  const hist = readJson<DeployHistory>(historyPath(env)) || { deployments: [], env, summary: { total: 0, success: 0, failed: 0 } };
  hist.deployments.unshift(record);
  hist.summary.total += 1;
  hist.summary.success += 1;
  writeJson(historyPath(env), hist);

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(record, null, 2));
    return;
  }

  ctx.output.success(`Deployment ${deployId} to ${env} completed successfully`);
  ctx.output.write(`  Version: ${targetVersion}`);
  ctx.output.write(`  Duration: ${((Date.now() - new Date(startedAt).getTime()) / 1000).toFixed(1)}s`);
}

// ── Subcommand: rollback ──────────────────────────────────────────

async function handleRollback(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.positional[0];
  const toVersion = args.options.to ? String(args.options.to) : undefined;

  if (!env) {
    ctx.output.writeError('Usage: xergon deploy rollback <env> --to <version>');
    process.exit(1);
    return;
  }
  if (!toVersion) {
    ctx.output.writeError('--to <version> is required for rollback.');
    process.exit(1);
    return;
  }

  const cfg = readJson<DeployConfig>(configPath(env));
  if (!cfg) {
    ctx.output.writeError(`Environment "${env}" not initialized.`);
    process.exit(1);
    return;
  }

  const previousVersion = cfg.version;
  ctx.output.info(`Rolling back ${env} from ${previousVersion} to ${toVersion}...`);

  const deployId = generateId();
  const startedAt = new Date().toISOString();
  const deployer = args.options.deployer ? String(args.options.deployer) : 'local-cli';

  const record: DeployRecord = {
    id: deployId,
    env,
    version: toVersion,
    status: 'rolled_back',
    startedAt,
    completedAt: new Date().toISOString(),
    rollbackFrom: previousVersion,
    deployer,
    healthChecks: [],
  };

  // Update config
  cfg.version = toVersion;
  writeJson(configPath(env), cfg);

  // Save history
  const hist = readJson<DeployHistory>(historyPath(env)) || { deployments: [], env, summary: { total: 0, success: 0, failed: 0 } };
  hist.deployments.unshift(record);
  hist.summary.total += 1;
  writeJson(historyPath(env), hist);

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(record, null, 2));
    return;
  }

  ctx.output.success(`Rollback to ${toVersion} completed`);
  ctx.output.write(`  From: ${previousVersion}`);
  ctx.output.write(`  Deployment ID: ${deployId}`);
}

// ── Subcommand: history ───────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.positional[0];
  if (!env) {
    ctx.output.writeError('Usage: xergon deploy history <env>');
    process.exit(1);
    return;
  }

  const hist = readJson<DeployHistory>(historyPath(env));
  if (!hist || hist.deployments.length === 0) {
    ctx.output.info(`No deployment history for "${env}".`);
    return;
  }

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(hist, null, 2));
    return;
  }

  if (isTableFormat(args)) {
    const tableData = hist.deployments.map(d => ({
      ID: d.id.substring(0, 20) + '...',
      Version: d.version,
      Status: d.status,
      Deployer: d.deployer,
      Date: new Date(d.startedAt).toISOString().slice(0, 10),
      Duration: d.completedAt ? `${((new Date(d.completedAt).getTime() - new Date(d.startedAt).getTime()) / 1000).toFixed(0)}s` : '-',
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Deployment History: ${env}`));
    return;
  }

  ctx.output.write(ctx.output.colorize(`Deployment History: ${env}`, 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
  ctx.output.write('');
  ctx.output.write(`  Total: ${hist.summary.total}  |  Success: ${ctx.output.colorize(String(hist.summary.success), 'green')}  |  Failed: ${ctx.output.colorize(String(hist.summary.failed), 'red')}`);
  ctx.output.write('');

  for (const d of hist.deployments.slice(0, 20)) {
    const sc = statusColor(d.status);
    ctx.output.write(`  ${ctx.output.colorize(d.id.substring(0, 24), 'cyan')}  ${ctx.output.colorize(d.status.toUpperCase(), sc)}`);
    ctx.output.write(`    Version: ${d.version}  |  Deployer: ${d.deployer}`);
    if (d.rollbackFrom) {
      ctx.output.write(`    Rollback from: ${d.rollbackFrom}`);
    }
    ctx.output.write(`    ${d.startedAt}`);
    ctx.output.write('');
  }

  if (hist.deployments.length > 20) {
    ctx.output.info(`Showing 20 of ${hist.deployments.length} deployments. Use --json for full history.`);
  }
}

// ── Subcommand: status ────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.positional[0];
  if (!env) {
    ctx.output.writeError('Usage: xergon deploy status <env>');
    process.exit(1);
    return;
  }

  const cfg = readJson<DeployConfig>(configPath(env));
  if (!cfg) {
    ctx.output.writeError(`Environment "${env}" not initialized. Run: xergon deploy init ${env}`);
    process.exit(1);
    return;
  }

  const hist = readJson<DeployHistory>(historyPath(env));
  const lastDeploy = hist?.deployments?.[0];

  const statusData: Record<string, string | number | boolean> = {
    Environment: cfg.environment,
    Version: cfg.version,
    'Relay URL': cfg.relayUrl,
    'Agent URL': cfg.agentUrl,
    'Marketplace URL': cfg.marketplaceUrl,
    'Health Check Path': cfg.healthCheckPath,
    'Health Check Timeout': `${cfg.healthCheckTimeout}ms`,
    'Max Retries': cfg.maxRetries,
    'Rollback on Failure': cfg.rollbackOnFailure,
    'Canary Percent': `${cfg.canaryPercent}%`,
  };

  if (lastDeploy) {
    statusData['Last Deploy ID'] = lastDeploy.id;
    statusData['Last Deploy Status'] = lastDeploy.status;
    statusData['Last Deploy Time'] = lastDeploy.startedAt;
    if (lastDeploy.healthChecks.length > 0) {
      statusData['Last Health Checks'] = `${lastDeploy.healthChecks.filter(h => h.status === 'pass').length}/${lastDeploy.healthChecks.length} passed`;
    }
  }

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(statusData, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize(`Deployment Status: ${env}`, 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
  ctx.output.write(ctx.output.formatText(statusData));
}

// ── Subcommand: promote ───────────────────────────────────────────

async function handlePromote(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const fromEnv = args.positional[0];
  const toEnv = args.positional[1];

  if (!fromEnv || !toEnv) {
    ctx.output.writeError('Usage: xergon deploy promote <from-env> <to-env>');
    ctx.output.write('Example: xergon deploy promote staging prod');
    process.exit(1);
    return;
  }

  const fromCfg = readJson<DeployConfig>(configPath(fromEnv));
  if (!fromCfg) {
    ctx.output.writeError(`Source environment "${fromEnv}" not initialized.`);
    process.exit(1);
    return;
  }

  const toCfg = readJson<DeployConfig>(configPath(toEnv));
  if (!toCfg) {
    ctx.output.writeError(`Target environment "${toEnv}" not initialized.`);
    process.exit(1);
    return;
  }

  ctx.output.info(`Promoting ${fromEnv} -> ${toEnv}`);
  ctx.output.write(`  Source version: ${fromCfg.version}`);
  ctx.output.write(`  Target version: ${toCfg.version}`);

  const deployer = args.options.deployer ? String(args.options.deployer) : 'local-cli';
  const deployId = generateId();
  const startedAt = new Date().toISOString();

  // Create promotion record
  const record: DeployRecord = {
    id: deployId,
    env: toEnv,
    version: fromCfg.version,
    status: 'success',
    startedAt,
    completedAt: new Date().toISOString(),
    deployer,
    healthChecks: [],
  };

  // Update target config
  toCfg.version = fromCfg.version;
  writeJson(configPath(toEnv), toCfg);

  // Save history in target env
  const hist = readJson<DeployHistory>(historyPath(toEnv)) || { deployments: [], env: toEnv, summary: { total: 0, success: 0, failed: 0 } };
  hist.deployments.unshift(record);
  hist.summary.total += 1;
  hist.summary.success += 1;
  writeJson(historyPath(toEnv), hist);

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify({
      from: fromEnv,
      to: toEnv,
      version: fromCfg.version,
      deploymentId: deployId,
      status: 'promoted',
    }, null, 2));
    return;
  }

  ctx.output.success(`Promoted version ${fromCfg.version} from ${fromEnv} to ${toEnv}`);
  ctx.output.write(`  Deployment ID: ${deployId}`);
}

// ── Subcommand: config ────────────────────────────────────────────

async function handleConfig(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const env = args.positional[0];
  if (!env) {
    ctx.output.writeError('Usage: xergon deploy config <env> --set <key=value>');
    process.exit(1);
    return;
  }

  const cfg = readJson<DeployConfig>(configPath(env));
  if (!cfg) {
    ctx.output.writeError(`Environment "${env}" not initialized. Run: xergon deploy init ${env}`);
    process.exit(1);
    return;
  }

  const setValue = args.options.set ? String(args.options.set) : undefined;

  if (!setValue) {
    // Show current config
    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(cfg, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Config: ${env}`, 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText(cfg as unknown as Record<string, unknown>));
    return;
  }

  // Parse key=value
  const eqIdx = setValue.indexOf('=');
  if (eqIdx === -1) {
    ctx.output.writeError('Invalid --set format. Use key=value.');
    process.exit(1);
    return;
  }

  const key = setValue.substring(0, eqIdx).trim();
  const value = setValue.substring(eqIdx + 1).trim();

  const validKeys = new Set<string>([
    'relayUrl', 'agentUrl', 'marketplaceUrl',
    'healthCheckPath', 'healthCheckTimeout', 'maxRetries',
    'rollbackOnFailure', 'canaryPercent', 'version',
  ]);

  if (!validKeys.has(key)) {
    ctx.output.writeError(`Unknown config key: ${key}. Valid keys: ${Array.from(validKeys).join(', ')}`);
    process.exit(1);
    return;
  }

  // Type coercion
  const cfgAny = cfg as unknown as Record<string, unknown>;
  if (key === 'healthCheckTimeout' || key === 'maxRetries') {
    cfgAny[key] = Number(value);
  } else if (key === 'rollbackOnFailure') {
    cfgAny[key] = value === 'true';
  } else if (key === 'canaryPercent') {
    cfgAny[key] = Number(value);
  } else {
    cfgAny[key] = value;
  }

  writeJson(configPath(env), cfg);

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify({ key, value: cfgAny[key], config: cfg }, null, 2));
    return;
  }

  ctx.output.success(`Updated ${key} = ${cfgAny[key]} for ${env}`);
}

// ── Command action ─────────────────────────────────────────────────

async function deployAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    await handleSummary(args, ctx);
    return;
  }

  // Shift positional args so subcommand handlers see their own args
  const shiftedArgs = { ...args, positional: args.positional.slice(1) };

  switch (sub) {
    case 'init':
      await handleInit(shiftedArgs, ctx);
      break;
    case 'plan':
      await handlePlan(args, ctx);
      break;
    case 'push':
      await handlePush(shiftedArgs, ctx);
      break;
    case 'rollback':
      await handleRollback(shiftedArgs, ctx);
      break;
    case 'history':
      await handleHistory(shiftedArgs, ctx);
      break;
    case 'status':
      await handleStatus(shiftedArgs, ctx);
      break;
    case 'promote':
      await handlePromote(shiftedArgs, ctx);
      break;
    case 'config':
      await handleConfig(shiftedArgs, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('');
      ctx.output.write('Subcommands:');
      ctx.output.write('  (no args)    Show deployment status summary');
      ctx.output.write('  init         Initialize deployment config for environment');
      ctx.output.write('  plan         Show what would be deployed (dry-run)');
      ctx.output.write('  push         Push deployment to environment');
      ctx.output.write('  rollback     Roll back to specific version');
      ctx.output.write('  history      Show deployment history');
      ctx.output.write('  status       Show current deployment status');
      ctx.output.write('  promote      Promote from one environment to another');
      ctx.output.write('  config       View or update deployment config');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const deployOptions: CommandOption[] = [
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
  {
    name: 'env',
    short: '',
    long: '--env',
    description: 'Target environment: dev, staging, prod (for plan subcommand)',
    required: false,
    type: 'string',
  },
  {
    name: 'version',
    short: '',
    long: '--version',
    description: 'Version to deploy (for plan/push subcommands)',
    required: false,
    type: 'string',
  },
  {
    name: 'to',
    short: '',
    long: '--to',
    description: 'Target version for rollback',
    required: false,
    type: 'string',
  },
  {
    name: 'set',
    short: '',
    long: '--set',
    description: 'Set config key=value (for config subcommand)',
    required: false,
    type: 'string',
  },
  {
    name: 'deployer',
    short: '',
    long: '--deployer',
    description: 'Deployer identity for audit trail',
    required: false,
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const deployCommand: Command = {
  name: 'deploy',
  description: 'Manage deployments on the Xergon Network',
  aliases: ['deployment'],
  options: deployOptions,
  action: deployAction,
};
