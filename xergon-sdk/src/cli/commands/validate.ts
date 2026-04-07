/**
 * CLI command: validate
 *
 * Validate the current Xergon configuration file.
 * Checks file existence, required fields, relay reachability,
 * API key validity, and config value types.
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');
const CONFIG_FILE = () => path.join(CONFIG_DIR(), 'config.json');

interface CheckResult {
  name: string;
  status: 'pass' | 'fail' | 'warn' | 'skip';
  message: string;
  fixable?: boolean;
}

const validateOptions: CommandOption[] = [
  {
    name: 'fix',
    short: '',
    long: '--fix',
    description: 'Attempt to fix common config issues automatically',
    required: false,
    type: 'boolean',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output results as JSON',
    required: false,
    type: 'boolean',
  },
];

function loadConfigFile(): Record<string, unknown> {
  try {
    const data = fs.readFileSync(CONFIG_FILE(), 'utf-8');
    return JSON.parse(data);
  } catch {
    return {};
  }
}

function saveConfigFile(config: Record<string, unknown>): void {
  const dir = CONFIG_DIR();
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(CONFIG_FILE(), JSON.stringify(config, null, 2) + '\n');
}

function isValidUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

/**
 * Perform an HTTP GET request with a timeout.
 * Returns { ok, status, body } or { ok: false, status: 0, error } on network failure.
 */
async function httpGet(url: string, timeoutMs: number): Promise<{
  ok: boolean;
  status: number;
  body?: string;
  error?: string;
}> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const resp = await fetch(url, { signal: controller.signal });
    const body = await resp.text();
    return { ok: resp.ok, status: resp.status, body };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { ok: false, status: 0, error: msg };
  } finally {
    clearTimeout(timer);
  }
}

async function validateAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const shouldFix = args.options.fix === true;
  const outputJson = args.options.json === true;
  const checks: CheckResult[] = [];
  const config = loadConfigFile();
  let fixed = false;

  // ── Check 1: Config file exists ──
  const configPath = CONFIG_FILE();
  if (fs.existsSync(configPath)) {
    checks.push({
      name: 'Config file exists',
      status: 'pass',
      message: `Found at ${configPath}`,
    });
  } else {
    checks.push({
      name: 'Config file exists',
      status: 'fail',
      message: `Not found at ${configPath}`,
      fixable: true,
    });
    if (shouldFix) {
      saveConfigFile({
        baseUrl: 'https://relay.xergon.gg',
        apiKey: '',
        defaultModel: 'llama-3.3-70b',
        outputFormat: 'text',
        color: true,
      });
      checks[checks.length - 1].status = 'pass';
      checks[checks.length - 1].message = `Created at ${configPath}`;
      fixed = true;
    }
  }

  // ── Check 2: Config is valid JSON ──
  if (fs.existsSync(configPath)) {
    try {
      const raw = fs.readFileSync(configPath, 'utf-8');
      JSON.parse(raw);
      checks.push({
        name: 'Config is valid JSON',
        status: 'pass',
        message: 'Parses correctly',
      });
    } catch (err) {
      checks.push({
        name: 'Config is valid JSON',
        status: 'fail',
        message: `JSON parse error: ${err instanceof Error ? err.message : String(err)}`,
      });
    }
  } else {
    checks.push({
      name: 'Config is valid JSON',
      status: 'skip',
      message: 'No config file to validate',
    });
  }

  // ── Check 3: Required field - baseUrl/relay_url ──
  const baseUrl = String(config.baseUrl || config.relay_url || ctx.config.baseUrl || '');
  if (baseUrl) {
    checks.push({
      name: 'baseUrl / relay_url set',
      status: 'pass',
      message: `Relay URL: ${baseUrl}`,
    });
  } else {
    checks.push({
      name: 'baseUrl / relay_url set',
      status: 'fail',
      message: 'Missing baseUrl or relay_url in config',
      fixable: true,
    });
    if (shouldFix) {
      config.baseUrl = 'https://relay.xergon.gg';
      saveConfigFile(config);
      checks[checks.length - 1].status = 'pass';
      checks[checks.length - 1].message = 'Set to https://relay.xergon.gg';
      fixed = true;
    }
  }

  // ── Check 4: Required field - apiKey or public_key ──
  const apiKey = String(config.apiKey || config.public_key || ctx.config.apiKey || '');
  if (apiKey) {
    checks.push({
      name: 'API key / public key set',
      status: 'pass',
      message: `Key: ${apiKey.substring(0, 8)}...`,
    });
  } else {
    checks.push({
      name: 'API key / public key set',
      status: 'warn',
      message: 'No API key set. Some endpoints may be unavailable.',
    });
  }

  // ── Check 5: baseUrl is valid URL ──
  if (baseUrl) {
    if (isValidUrl(baseUrl)) {
      checks.push({
        name: 'baseUrl is valid URL',
        status: 'pass',
        message: 'Well-formed URL with http(s) protocol',
      });
    } else {
      checks.push({
        name: 'baseUrl is valid URL',
        status: 'fail',
        message: `"${baseUrl}" is not a valid http(s) URL`,
        fixable: true,
      });
      if (shouldFix) {
        config.baseUrl = 'https://relay.xergon.gg';
        saveConfigFile(config);
        checks[checks.length - 1].status = 'pass';
        checks[checks.length - 1].message = 'Reset to https://relay.xergon.gg';
        fixed = true;
      }
    }
  } else {
    checks.push({
      name: 'baseUrl is valid URL',
      status: 'skip',
      message: 'No baseUrl to validate',
    });
  }

  // ── Check 6: timeout is valid number ──
  if (config.timeout !== undefined) {
    const timeout = Number(config.timeout);
    if (!isNaN(timeout) && timeout > 0) {
      checks.push({
        name: 'timeout is valid number',
        status: 'pass',
        message: `Timeout: ${timeout}ms`,
      });
    } else {
      checks.push({
        name: 'timeout is valid number',
        status: 'fail',
        message: `Invalid timeout: ${config.timeout}`,
        fixable: true,
      });
      if (shouldFix) {
        config.timeout = 30000;
        saveConfigFile(config);
        checks[checks.length - 1].status = 'pass';
        checks[checks.length - 1].message = 'Reset to 30000ms';
        fixed = true;
      }
    }
  } else {
    checks.push({
      name: 'timeout is valid number',
      status: 'pass',
      message: 'Using default: 30000ms',
    });
  }

  // ── Check 7: outputFormat is valid ──
  if (config.outputFormat !== undefined) {
    const validFormats = ['text', 'json', 'table'];
    if (validFormats.includes(String(config.outputFormat))) {
      checks.push({
        name: 'outputFormat is valid',
        status: 'pass',
        message: `Format: ${config.outputFormat}`,
      });
    } else {
      checks.push({
        name: 'outputFormat is valid',
        status: 'fail',
        message: `Invalid format: "${config.outputFormat}" (expected: ${validFormats.join(', ')})`,
        fixable: true,
      });
      if (shouldFix) {
        config.outputFormat = 'text';
        saveConfigFile(config);
        checks[checks.length - 1].status = 'pass';
        checks[checks.length - 1].message = 'Reset to "text"';
        fixed = true;
      }
    }
  } else {
    checks.push({
      name: 'outputFormat is valid',
      status: 'pass',
      message: 'Using default: text',
    });
  }

  // ── Check 8: Relay is reachable (GET /health) ──
  const relayUrl = baseUrl || ctx.config.baseUrl;
  if (relayUrl) {
    ctx.output.write(ctx.output.colorize('Checking relay health...', 'dim') + '\n');
    const healthResult = await httpGet(`${relayUrl}/health`, 5000);
    if (healthResult.ok) {
      checks.push({
        name: 'Relay reachable (/health)',
        status: 'pass',
        message: `HTTP ${healthResult.status} - ${healthResult.body?.substring(0, 60) || 'OK'}`,
      });
    } else if (healthResult.status === 404) {
      // /health endpoint might not exist, but server responded
      checks.push({
        name: 'Relay reachable (/health)',
        status: 'warn',
        message: `Server responded (HTTP 404) - /health endpoint may not exist`,
      });
    } else if (healthResult.status === 0) {
      checks.push({
        name: 'Relay reachable (/health)',
        status: 'fail',
        message: `Connection failed: ${healthResult.error || 'unknown error'}`,
      });
    } else {
      checks.push({
        name: 'Relay reachable (/health)',
        status: 'fail',
        message: `HTTP ${healthResult.status}`,
      });
    }
  } else {
    checks.push({
      name: 'Relay reachable (/health)',
      status: 'skip',
      message: 'No relay URL configured',
    });
  }

  // ── Check 9: API key is valid (GET /v1/models returns 200) ──
  if (apiKey && relayUrl) {
    ctx.output.write(ctx.output.colorize('Validating API key...', 'dim') + '\n');
    try {
      const headers: Record<string, string> = {};
      if (apiKey) {
        headers['Authorization'] = `Bearer ${apiKey}`;
      }
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 10000);
      const resp = await fetch(`${relayUrl}/v1/models`, { headers, signal: controller.signal });
      clearTimeout(timer);

      if (resp.ok) {
        const data = await resp.json();
        const modelCount = data?.data?.length ?? 0;
        checks.push({
          name: 'API key valid (/v1/models)',
          status: 'pass',
          message: `Authenticated successfully - ${modelCount} model(s) available`,
        });
      } else if (resp.status === 401 || resp.status === 403) {
        checks.push({
          name: 'API key valid (/v1/models)',
          status: 'fail',
          message: `Authentication failed (HTTP ${resp.status})`,
        });
      } else {
        checks.push({
          name: 'API key valid (/v1/models)',
          status: 'warn',
          message: `Unexpected response: HTTP ${resp.status}`,
        });
      }
    } catch (err) {
      checks.push({
        name: 'API key valid (/v1/models)',
        status: 'skip',
        message: `Could not verify: ${err instanceof Error ? err.message : String(err)}`,
      });
    }
  } else if (!apiKey) {
    checks.push({
      name: 'API key valid (/v1/models)',
      status: 'skip',
      message: 'No API key to validate',
    });
  } else {
    checks.push({
      name: 'API key valid (/v1/models)',
      status: 'skip',
      message: 'No relay URL to validate against',
    });
  }

  // ── Output results ──
  const hasFail = checks.some(c => c.status === 'fail');

  if (outputJson) {
    ctx.output.setFormat('json');
    ctx.output.write(ctx.output.formatOutput({
      checks,
      summary: {
        total: checks.length,
        pass: checks.filter(c => c.status === 'pass').length,
        fail: checks.filter(c => c.status === 'fail').length,
        warn: checks.filter(c => c.status === 'warn').length,
        skip: checks.filter(c => c.status === 'skip').length,
      },
      fixed: shouldFix ? fixed : undefined,
    }));
  } else {
    // Build table
    const tableData = checks.map(c => {
      let icon: string;
      let label: string;
      switch (c.status) {
        case 'pass': icon = ctx.output.colorize('PASS', 'green'); break;
        case 'fail': icon = ctx.output.colorize('FAIL', 'red'); break;
        case 'warn': icon = ctx.output.colorize('WARN', 'yellow'); break;
        case 'skip': icon = ctx.output.colorize('SKIP', 'dim'); break;
      }
      return {
        Status: icon,
        Check: c.name,
        Detail: c.message,
      };
    });

    ctx.output.write('\n');
    ctx.output.write(ctx.output.formatTable(tableData, 'Config Validation Results'));

    // Summary
    const passCount = checks.filter(c => c.status === 'pass').length;
    const failCount = checks.filter(c => c.status === 'fail').length;
    const warnCount = checks.filter(c => c.status === 'warn').length;
    const skipCount = checks.filter(c => c.status === 'skip').length;

    ctx.output.write(ctx.output.colorize(`  ${passCount} passed`, 'green'));
    if (failCount) ctx.output.write(', ' + ctx.output.colorize(`${failCount} failed`, 'red'));
    if (warnCount) ctx.output.write(', ' + ctx.output.colorize(`${warnCount} warnings`, 'yellow'));
    if (skipCount) ctx.output.write(', ' + ctx.output.colorize(`${skipCount} skipped`, 'dim'));
    ctx.output.write('\n');

    if (hasFail) {
      if (shouldFix) {
        if (fixed) {
          ctx.output.success('Some issues were fixed automatically.');
        } else {
          ctx.output.warn('No fixable issues remaining.');
        }
      } else {
        ctx.output.info('Run with --fix to attempt automatic fixes.');
      }
    }
  }

  if (hasFail) {
    process.exit(1);
  }
}

export const validateCommand: Command = {
  name: 'validate',
  description: 'Validate the current configuration',
  aliases: ['check', 'doctor'],
  options: validateOptions,
  action: validateAction,
};
