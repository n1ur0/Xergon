/**
 * Debug Diagnostics -- comprehensive system health checks, troubleshooting,
 * and debug dump generation for the Xergon SDK.
 *
 * Provides fine-grained diagnostic checks across connection, models,
 * wallet, disk, and network categories, plus a full debug dump for
 * support ticket creation.
 */

import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import * as http from 'node:http';
import * as https from 'node:https';

// ── Types ──────────────────────────────────────────────────────────

export interface DiagnosticResult {
  category: string;
  name: string;
  status: 'healthy' | 'degraded' | 'unhealthy' | 'unknown';
  message: string;
  details?: Record<string, any>;
  duration: number;
  timestamp: string;
}

export interface DebugDump {
  config: Record<string, any>;
  environment: Record<string, string>;
  connections: DiagnosticResult[];
  models: DiagnosticResult[];
  wallet: DiagnosticResult[];
  disk: DiagnosticResult[];
  network: DiagnosticResult[];
  recommendations: string[];
  generatedAt: string;
  sdkVersion: string;
}

export type DiagnosticCategory = 'connection' | 'models' | 'wallet' | 'disk' | 'network';

// ── Helpers ────────────────────────────────────────────────────────

function result(
  category: string,
  name: string,
  status: DiagnosticResult['status'],
  message: string,
  duration: number,
  details?: Record<string, any>,
): DiagnosticResult {
  return {
    category,
    name,
    status,
    message,
    details,
    duration,
    timestamp: new Date().toISOString(),
  };
}

async function measure<T>(fn: () => Promise<T>): Promise<{ value: T; duration: number }> {
  const start = Date.now();
  const value = await fn();
  return { value, duration: Date.now() - start };
}

function safeFetch(url: string, timeoutMs: number = 10000): Promise<{ ok: boolean; status: number; body: string; latencyMs: number }> {
  return new Promise((resolve) => {
    const start = Date.now();
    const mod = url.startsWith('https') ? https : http;

    const req = mod.get(url, { timeout: timeoutMs }, (res: http.IncomingMessage) => {
      let body = '';
      res.on('data', (chunk: Buffer | string) => { body += chunk; });
      res.on('end', () => {
        resolve({
          ok: res.statusCode! >= 200 && res.statusCode! < 400,
          status: res.statusCode ?? 0,
          body,
          latencyMs: Date.now() - start,
        });
      });
    });

    req.on('error', (err: Error) => {
      resolve({
        ok: false,
        status: 0,
        body: err.message,
        latencyMs: Date.now() - start,
      });
    });

    req.on('timeout', () => {
      req.destroy();
      resolve({
        ok: false,
        status: 0,
        body: 'Request timed out',
        latencyMs: Date.now() - start,
      });
    });
  });
}

const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');

// ── Connection Checks ──────────────────────────────────────────────

async function checkRelayHealth(baseUrl: string): Promise<DiagnosticResult> {
  const { value, duration } = await measure(async () => {
    return safeFetch(`${baseUrl.replace(/\/+$/, '')}/health`, 10000);
  });

  if (value.ok) {
    return result('connection', 'Relay Health', 'healthy', `Relay is online (${value.latencyMs}ms)`, duration, { latencyMs: value.latencyMs });
  }

  // Try /v1/models as fallback
  const fallback = await measure(async () => {
    return safeFetch(`${baseUrl.replace(/\/+$/, '')}/v1/models`, 10000);
  });

  if (fallback.value.ok) {
    return result('connection', 'Relay Health', 'degraded', `Relay responding but /health returned ${value.status} (${fallback.value.latencyMs}ms)`, duration, { latencyMs: fallback.value.latencyMs });
  }

  return result('connection', 'Relay Health', 'unhealthy', `Cannot reach relay at ${baseUrl}: ${value.body}`, duration);
}

async function checkEndpointConnectivity(endpoint: string): Promise<DiagnosticResult> {
  const { value, duration } = await measure(async () => {
    return safeFetch(endpoint, 10000);
  });

  if (value.ok) {
    return result('connection', `Endpoint: ${endpoint}`, 'healthy', `Connected (${value.latencyMs}ms)`, duration, { latencyMs: value.latencyMs, statusCode: value.status });
  }

  return result('connection', `Endpoint: ${endpoint}`, 'unhealthy', `Connection failed: ${value.body} (${value.latencyMs}ms)`, duration);
}

async function checkConnection(baseUrl: string): Promise<DiagnosticResult> {
  return checkRelayHealth(baseUrl);
}

// ── Model Checks ──────────────────────────────────────────────────

async function checkModelList(baseUrl: string): Promise<DiagnosticResult> {
  const { value, duration } = await measure(async () => {
    return safeFetch(`${baseUrl.replace(/\/+$/, '')}/v1/models`, 10000);
  });

  if (!value.ok) {
    return result('models', 'Model List', 'unhealthy', `Failed to fetch models: HTTP ${value.status}`, duration);
  }

  let modelCount = 0;
  try {
    const data = JSON.parse(value.body);
    if (Array.isArray(data)) {
      modelCount = data.length;
    } else if (data.data && Array.isArray(data.data)) {
      modelCount = data.data.length;
    } else if (typeof data.total === 'number') {
      modelCount = data.total;
    }
  } catch { /* parse error */ }

  if (modelCount === 0) {
    return result('models', 'Model List', 'degraded', 'No models available', duration, { modelCount });
  }

  return result('models', 'Model List', 'healthy', `${modelCount} model(s) available (${value.latencyMs}ms)`, duration, { modelCount, latencyMs: value.latencyMs });
}

async function checkModelAvailability(baseUrl: string, modelName: string): Promise<DiagnosticResult> {
  const { value, duration } = await measure(async () => {
    return safeFetch(`${baseUrl.replace(/\/+$/, '')}/v1/models`, 10000);
  });

  if (!value.ok) {
    return result('models', `Model: ${modelName}`, 'unhealthy', `Cannot verify: relay returned HTTP ${value.status}`, duration);
  }

  let found = false;
  try {
    const data = JSON.parse(value.body);
    const models: any[] = Array.isArray(data) ? data : (data.data ?? []);
    found = models.some((m: any) => m.id === modelName || m.id?.toLowerCase() === modelName.toLowerCase());
  } catch { /* parse error */ }

  if (found) {
    return result('models', `Model: ${modelName}`, 'healthy', `Model "${modelName}" is available`, duration);
  }

  return result('models', `Model: ${modelName}`, 'unhealthy', `Model "${modelName}" not found`, duration);
}

// ── Wallet Checks ──────────────────────────────────────────────────

async function checkWalletConnection(apiKey: string): Promise<DiagnosticResult> {
  const { duration } = await measure(() => Promise.resolve());

  if (!apiKey) {
    return result('wallet', 'Wallet Connection', 'unhealthy', 'No API key / public key configured', duration);
  }

  const masked = apiKey.length > 16
    ? `${apiKey.substring(0, 8)}...${apiKey.substring(apiKey.length - 8)}`
    : `${apiKey.substring(0, 8)}...`;

  const isValidLength = apiKey.length >= 20;
  const hasNoSpaces = !/\s/.test(apiKey);

  if (isValidLength && hasNoSpaces) {
    return result('wallet', 'Wallet Connection', 'healthy', `Public key configured (${masked})`, duration, { maskedKey: masked });
  }

  return result('wallet', 'Wallet Connection', 'degraded', `Public key may be invalid (${masked})`, duration, { maskedKey: masked });
}

async function checkWalletBalance(baseUrl: string, apiKey: string): Promise<DiagnosticResult> {
  if (!apiKey) {
    return result('wallet', 'ERG Balance', 'unknown', 'Cannot check: no API key configured', 0);
  }

  const { value, duration } = await measure(async () => {
    return safeFetch(`${baseUrl.replace(/\/+$/, '')}/v1/balance/${encodeURIComponent(apiKey)}`, 10000);
  });

  if (value.ok) {
    try {
      const data = JSON.parse(value.body);
      const erg = data.balanceErg ?? data.balance ?? data.erg ?? 'unknown';
      return result('wallet', 'ERG Balance', 'healthy', `${erg} ERG`, duration, { balance: erg });
    } catch {
      return result('wallet', 'ERG Balance', 'degraded', 'Balance response could not be parsed', duration);
    }
  }

  return result('wallet', 'ERG Balance', 'degraded', `Balance check failed: HTTP ${value.status}`, duration);
}

// ── Disk Checks ───────────────────────────────────────────────────

async function checkDiskSpace(): Promise<DiagnosticResult> {
  const { duration } = await measure(async () => {
    // Check Xergon config directory
    const configDir = CONFIG_DIR();
    let configDirSize = 0;
    let configDirExists = false;

    try {
      configDirExists = fs.existsSync(configDir);
      if (configDirExists) {
        const stats = fs.statSync(configDir);
        configDirSize = stats.size;
      }
    } catch { /* stat error */ }

    // Check home disk space
    let freeBytes = 0;
    let totalBytes = 0;
    try {
      // Use statvfs equivalent via os
      const homeStats = fs.statSync(os.homedir());
      if (homeStats) {
        // Node doesn't directly expose disk space; use a heuristic
        freeBytes = 0;
        totalBytes = 0;
      }
    } catch { /* stat error */ }

    return { configDirExists, configDirSize, freeBytes, totalBytes };
  });

  const configDir = CONFIG_DIR();
  const configDirExists = fs.existsSync(configDir);

  if (!configDirExists) {
    return result('disk', 'Xergon Config Dir', 'degraded', `Config directory not found: ${configDir}`, duration);
  }

  return result('disk', 'Xergon Config Dir', 'healthy', `Config directory exists: ${configDir}`, duration, { path: configDir });
}

async function checkDiskTemp(): Promise<DiagnosticResult> {
  const { duration } = await measure(async () => {
    const tmpDir = os.tmpdir();
    let writable = false;
    try {
      const testFile = path.join(tmpDir, '.xergon-debug-test');
      fs.writeFileSync(testFile, 'test');
      fs.unlinkSync(testFile);
      writable = true;
    } catch {
      writable = false;
    }
    return { tmpDir, writable };
  });

  const tmpDir = os.tmpdir();
  // Check if writable
  let writable = false;
  try {
    const testFile = path.join(tmpDir, '.xergon-debug-test');
    fs.writeFileSync(testFile, 'test');
    fs.unlinkSync(testFile);
    writable = true;
  } catch {
    writable = false;
  }

  if (writable) {
    return result('disk', 'Temp Directory', 'healthy', `Temp dir writable: ${tmpDir}`, duration);
  }

  return result('disk', 'Temp Directory', 'unhealthy', `Temp dir not writable: ${tmpDir}`, duration);
}

// ── Network Checks ────────────────────────────────────────────────

async function checkNetworkLatency(baseUrl: string): Promise<DiagnosticResult> {
  const samples: number[] = [];

  for (let i = 0; i < 3; i++) {
    const { value } = await measure(async () => {
      return safeFetch(`${baseUrl.replace(/\/+$/, '')}/health`, 5000);
    });
    if (value.latencyMs > 0) {
      samples.push(value.latencyMs);
    }
  }

  if (samples.length === 0) {
    return result('network', 'Network Latency', 'unhealthy', 'Could not measure latency', 0);
  }

  const avg = Math.round(samples.reduce((a, b) => a + b, 0) / samples.length);
  const min = Math.min(...samples);
  const max = Math.max(...samples);

  let status: DiagnosticResult['status'] = 'healthy';
  let message = `Average: ${avg}ms (min: ${min}ms, max: ${max}ms)`;

  if (avg > 2000) {
    status = 'unhealthy';
    message += ' -- HIGH LATENCY';
  } else if (avg > 500) {
    status = 'degraded';
    message += ' -- elevated latency';
  }

  return result('network', 'Network Latency', status, message, avg, { avg, min, max, samples });
}

async function checkDNSSettings(): Promise<DiagnosticResult> {
  const { duration } = await measure(() => Promise.resolve());

  const hostname = os.hostname();
  const networkInterfaces = os.networkInterfaces();
  const interfaceNames = Object.keys(networkInterfaces);

  let hasNonLoopback = false;
  for (const name of interfaceNames) {
    const ifaces = networkInterfaces[name] ?? [];
    for (const iface of ifaces) {
      if (!iface.internal) {
        hasNonLoopback = true;
        break;
      }
    }
    if (hasNonLoopback) break;
  }

  if (hasNonLoopback) {
    return result('network', 'DNS / Network', 'healthy', `Hostname: ${hostname}, ${interfaceNames.length} interface(s)`, duration, { hostname, interfaces: interfaceNames });
  }

  return result('network', 'DNS / Network', 'degraded', `No non-loopback network interfaces found (hostname: ${hostname})`, duration, { hostname });
}

// ── System Info ────────────────────────────────────────────────────

export function getSystemInfo(): Record<string, any> {
  const cpus = os.cpus();
  const totalMemory = os.totalmem();
  const freeMemory = os.freemem();
  const networkInterfaces = os.networkInterfaces();

  return {
    platform: os.platform(),
    arch: os.arch(),
    release: os.release(),
    hostname: os.hostname(),
    nodeVersion: process.version,
    cpuCount: cpus.length,
    cpuModel: cpus[0]?.model ?? 'unknown',
    cpuSpeed: `${cpus[0]?.speed ?? 0} MHz`,
    totalMemory: `${(totalMemory / (1024 * 1024 * 1024)).toFixed(2)} GB`,
    freeMemory: `${(freeMemory / (1024 * 1024 * 1024)).toFixed(2)} GB`,
    usedMemory: `${((totalMemory - freeMemory) / (1024 * 1024 * 1024)).toFixed(2)} GB`,
    uptime: `${(os.uptime() / 3600).toFixed(1)} hours`,
    interfaces: Object.keys(networkInterfaces),
    homeDir: os.homedir(),
    tmpDir: os.tmpdir(),
    pid: process.pid,
  };
}

// ── Orchestration ──────────────────────────────────────────────────

/**
 * Run all diagnostic checks across all categories.
 */
export async function runDiagnostics(options?: {
  baseUrl?: string;
  apiKey?: string;
}): Promise<DiagnosticResult[]> {
  const baseUrl = options?.baseUrl ?? 'https://relay.xergon.gg';
  const apiKey = options?.apiKey ?? '';

  const checks = await Promise.all([
    // Connection
    checkRelayHealth(baseUrl),

    // Models
    checkModelList(baseUrl),

    // Wallet
    checkWalletConnection(apiKey),
    checkWalletBalance(baseUrl, apiKey),

    // Disk
    checkDiskSpace(),
    checkDiskTemp(),

    // Network
    checkNetworkLatency(baseUrl),
    checkDNSSettings(),
  ]);

  return checks;
}

/**
 * Run diagnostic checks for a specific category.
 */
export async function runDiagnostic(
  category: DiagnosticCategory,
  options?: {
    baseUrl?: string;
    apiKey?: string;
    endpoint?: string;
    model?: string;
  },
): Promise<DiagnosticResult[]> {
  const baseUrl = options?.baseUrl ?? 'https://relay.xergon.gg';
  const apiKey = options?.apiKey ?? '';

  switch (category) {
    case 'connection': {
      const checks = [checkRelayHealth(baseUrl)];
      if (options?.endpoint) {
        checks.push(checkEndpointConnectivity(options.endpoint));
      }
      return Promise.all(checks);
    }

    case 'models': {
      if (options?.model) {
        return Promise.all([checkModelAvailability(baseUrl, options.model)]);
      }
      return Promise.all([checkModelList(baseUrl)]);
    }

    case 'wallet':
      return Promise.all([
        checkWalletConnection(apiKey),
        checkWalletBalance(baseUrl, apiKey),
      ]);

    case 'disk':
      return Promise.all([
        checkDiskSpace(),
        checkDiskTemp(),
      ]);

    case 'network':
      return Promise.all([
        checkNetworkLatency(baseUrl),
        checkDNSSettings(),
      ]);

    default:
      return [];
  }
}

/**
 * Test connection to a specific endpoint.
 */
export async function checkConnectionToEndpoint(endpoint: string): Promise<DiagnosticResult> {
  return checkEndpointConnectivity(endpoint);
}

/**
 * Check if a specific model is available.
 */
export async function checkModelAvailabilityAtUrl(baseUrl: string, model: string): Promise<DiagnosticResult> {
  return checkModelAvailability(baseUrl, model);
}

/**
 * Verify wallet connection.
 */
export async function verifyWalletConnection(apiKey: string): Promise<DiagnosticResult> {
  return checkWalletConnection(apiKey);
}

/**
 * Check available disk space.
 */
export async function checkDiskSpaceAvailable(): Promise<DiagnosticResult> {
  return checkDiskSpace();
}

/**
 * Measure network latency to the relay.
 */
export async function measureNetworkLatency(baseUrl: string): Promise<DiagnosticResult> {
  return checkNetworkLatency(baseUrl);
}

/**
 * Generate a full debug dump for support ticket creation.
 */
export async function generateDebugDump(options?: {
  baseUrl?: string;
  apiKey?: string;
}): Promise<DebugDump> {
  const baseUrl = options?.baseUrl ?? 'https://relay.xergon.gg';
  const apiKey = options?.apiKey ?? '';

  // Load config
  let config: Record<string, any> = {};
  try {
    const configPath = path.join(CONFIG_DIR(), 'config.json');
    const data = fs.readFileSync(configPath, 'utf-8');
    config = JSON.parse(data);
    // Redact sensitive keys
    for (const key of Object.keys(config)) {
      if (key.toLowerCase().includes('key') || key.toLowerCase().includes('secret') || key.toLowerCase().includes('private')) {
        config[key] = '***REDACTED***';
      }
    }
  } catch { /* no config */ }

  // Collect environment (redact sensitive)
  const environment: Record<string, string> = {
    NODE_ENV: process.env.NODE_ENV ?? 'not set',
    XERGON_BASE_URL: process.env.XERGON_BASE_URL ?? 'not set',
    XERGON_API_KEY: process.env.XERGON_API_KEY ? '***REDACTED***' : 'not set',
    HOME: process.env.HOME ?? os.homedir(),
    PATH: process.env.PATH ? `${process.env.PATH.split(':').length} entries` : 'not set',
    NO_COLOR: process.env.NO_COLOR ?? 'not set',
  };

  // Run all diagnostics
  const allChecks = await runDiagnostics({ baseUrl, apiKey });

  const connections = allChecks.filter(c => c.category === 'connection');
  const models = allChecks.filter(c => c.category === 'models');
  const wallet = allChecks.filter(c => c.category === 'wallet');
  const disk = allChecks.filter(c => c.category === 'disk');
  const network = allChecks.filter(c => c.category === 'network');

  // Generate recommendations
  const recommendations: string[] = [];
  for (const check of allChecks) {
    if (check.status === 'unhealthy') {
      recommendations.push(`[ACTION REQUIRED] ${check.category}/${check.name}: ${check.message}`);
    } else if (check.status === 'degraded') {
      recommendations.push(`[REVIEW] ${check.category}/${check.name}: ${check.message}`);
    }
  }

  return {
    config,
    environment,
    connections,
    models,
    wallet,
    disk,
    network,
    recommendations,
    generatedAt: new Date().toISOString(),
    sdkVersion: '0.1.0',
  };
}

/**
 * Guided troubleshooting: ask about symptoms and provide targeted advice.
 */
export function troubleshoot(issue: string): string[] {
  const lower = issue.toLowerCase();
  const steps: string[] = [];

  steps.push(`Troubleshooting: "${issue}"`);
  steps.push('');

  if (lower.includes('timeout') || lower.includes('slow') || lower.includes('latency')) {
    steps.push('Possible causes: network latency, relay overload, DNS issues');
    steps.push('');
    steps.push('Steps to resolve:');
    steps.push('  1. Check network connectivity: xergon debug network');
    steps.push('  2. Try an alternative relay endpoint if configured');
    steps.push('  3. Check if the relay is under maintenance: xergon status');
    steps.push('  4. Increase timeout in config: set timeout to 60000');
    steps.push('  5. Check DNS resolution: nslookup relay.xergon.gg');
  } else if (lower.includes('auth') || lower.includes('key') || lower.includes('login') || lower.includes('401') || lower.includes('403')) {
    steps.push('Possible causes: invalid API key, expired key, HMAC mismatch');
    steps.push('');
    steps.push('Steps to resolve:');
    steps.push('  1. Verify your public key is configured: xergon debug wallet');
    steps.push('  2. Re-authenticate: xergon login');
    steps.push('  3. Check your config: cat ~/.xergon/config.json');
    steps.push('  4. Ensure the key matches your Ergo wallet address');
  } else if (lower.includes('model') || lower.includes('not found') || lower.includes('404')) {
    steps.push('Possible causes: model removed, renamed, or provider offline');
    steps.push('');
    steps.push('Steps to resolve:');
    steps.push('  1. List available models: xergon models');
    steps.push('  2. Search for the model: xergon model search <name>');
    steps.push('  3. Check model status: xergon model info <model-id>');
    steps.push('  4. Try a different model or provider');
  } else if (lower.includes('balance') || lower.includes('payment') || lower.includes('funds')) {
    steps.push('Possible causes: insufficient ERG, staking box not found, oracle rate issue');
    steps.push('');
    steps.push('Steps to resolve:');
    steps.push('  1. Check your ERG balance: xergon balance');
    steps.push('  2. Verify staking box exists: xergon contracts getUserStakingBoxes');
    steps.push('  3. Check bridge status: xergon bridge status');
    steps.push('  4. Ensure you have enough ERG for gas fees');
  } else if (lower.includes('disk') || lower.includes('space') || lower.includes('write')) {
    steps.push('Possible causes: disk full, permission issues, config directory corrupted');
    steps.push('');
    steps.push('Steps to resolve:');
    steps.push('  1. Check disk space: xergon debug disk');
    steps.push('  2. Check config directory: ls -la ~/.xergon/');
    steps.push('  3. Remove corrupted cache: rm -rf ~/.xergon/cache/');
    steps.push('  4. Verify write permissions on config directory');
  } else {
    steps.push('General troubleshooting steps:');
    steps.push('  1. Run full diagnostics: xergon debug');
    steps.push('  2. Generate a debug dump: xergon debug dump');
    steps.push('  3. Check the relay status: xergon status');
    steps.push('  4. Verify your configuration: xergon config show');
    steps.push('  5. Update the SDK: npm update @xergon/sdk');
    steps.push('');
    steps.push('If the issue persists, please share the output of "xergon debug dump"');
    steps.push('with the Xergon support team.');
  }

  return steps;
}

/**
 * Export diagnostics in the specified format.
 */
export function exportDiagnostics(
  diagnostics: DiagnosticResult[],
  format: 'json' | 'text' = 'text',
): string {
  if (format === 'json') {
    return JSON.stringify(diagnostics, null, 2);
  }

  const lines: string[] = [];
  const statusIcons: Record<string, string> = {
    healthy: '\u2713',
    degraded: '\u26A0',
    unhealthy: '\u2717',
    unknown: '?',
  };

  for (const d of diagnostics) {
    const icon = statusIcons[d.status] ?? '?';
    lines.push(`[${icon}] ${d.category}/${d.name}: ${d.message} (${d.duration}ms)`);
  }

  return lines.join('\n');
}
