/**
 * CLI command: login / logout
 *
 * Authenticate with the Xergon relay.
 *
 *   xergon login              Interactive: prompts for API key, validates, saves
 *   xergon login --key <key>  Non-interactive: validates and saves the key
 *   xergon login --wallet     Prints ErgoAuth deep link for wallet-based auth
 *   xergon logout             Clears stored credentials
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

const CONFIG_DIR = () => path.join(os.homedir(), '.xergon');
const CONFIG_FILE = () => path.join(CONFIG_DIR(), 'config.json');

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

/**
 * Validate an API key by making a lightweight request to the relay.
 * Tries /v1/auth/status first, then falls back to /v1/models.
 */
async function validateApiKey(baseUrl: string, apiKey: string): Promise<{ valid: boolean; message: string }> {
  try {
    // Dynamic import to keep CLI lightweight
    const { XergonClient } = await import('../../index');
    const client = new XergonClient({ baseUrl });
    client.setPublicKey(apiKey);

    // Try fetching models as a validation check (lightweight, always available)
    try {
      const models = await client.models.list();
      return {
        valid: true,
        message: `Authenticated successfully. ${Array.isArray(models) ? models.length : 0} model(s) available.`,
      };
    } catch {
      // If models fails, try auth/status
      try {
        const resp = await fetch(`${baseUrl}/v1/auth/status`, {
          headers: { Authorization: `Bearer ${apiKey}` },
          signal: AbortSignal.timeout(10000),
        });
        if (resp.ok) {
          return { valid: true, message: 'Authenticated successfully.' };
        }
        return { valid: false, message: `Authentication failed: ${resp.status} ${resp.statusText}` };
      } catch {
        return { valid: false, message: 'Could not reach relay for validation. Key saved but not verified.' };
      }
    }
  } catch (err) {
    return {
      valid: false,
      message: `Validation error: ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}

// ── Login command options ──────────────────────────────────────

const loginOptions: CommandOption[] = [
  {
    name: 'key',
    short: '',
    long: '--key',
    description: 'API key for non-interactive login',
    required: false,
    type: 'string',
  },
  {
    name: 'wallet',
    short: '',
    long: '--wallet',
    description: 'Open ErgoAuth wallet-based login flow',
    required: false,
    type: 'boolean',
  },
];

async function loginAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providedKey = args.options.key ? String(args.options.key) : undefined;
  const walletMode = args.options.wallet === true;
  const baseUrl = ctx.config.baseUrl;

  // ── Wallet mode ─────────────────────────────────────────────
  if (walletMode) {
    const deepLink = `ergonauth://login?relay=${encodeURIComponent(baseUrl)}&callback=xergon`;
    ctx.output.write(ctx.output.colorize('ErgoAuth Wallet Login', 'bold') + '\n\n');
    ctx.output.write('Open the following link on your mobile device or in a wallet-aware browser:\n\n');
    ctx.output.write(`  ${ctx.output.colorize(deepLink, 'cyan')}\n\n`);
    ctx.output.write('After authentication, your API key will be stored automatically.\n');
    ctx.output.info('If your wallet does not open automatically, copy the link above.');
    return;
  }

  // ── Determine the API key ───────────────────────────────────
  let apiKey = providedKey;

  if (!apiKey) {
    // Interactive mode: prompt for API key
    const { createInterface } = await import('node:readline');

    // Check if stdin is a TTY before prompting
    if (!process.stdin.isTTY) {
      ctx.output.writeError('No API key provided. Use --key flag when piping input.');
      ctx.output.info('Usage: xergon login --key <api_key>');
      process.exit(1);
      return; // unreachable
    }

    const rl = createInterface({
      input: process.stdin,
      output: process.stdout,
    });

    apiKey = await new Promise<string>((resolve) => {
      process.stdout.write('Enter your API key: ');
      rl.question('', (answer: string) => {
        resolve(answer.trim());
      });
    });

    rl.close();

    if (!apiKey) {
      ctx.output.writeError('No API key provided.');
      process.exit(1);
      return; // unreachable
    }
  }

  // ── Validate the key ────────────────────────────────────────
  ctx.output.write('Validating API key...');
  const result = await validateApiKey(baseUrl, apiKey);

  // Overwrite the "Validating..." line
  process.stdout.write('\r\x1b[K'); // clear line

  if (!result.valid) {
    ctx.output.writeError(result.message);
    ctx.output.warn('Key was NOT saved. Please check your API key and try again.');
    process.exit(1);
    return; // unreachable
  }

  // ── Save the key ────────────────────────────────────────────
  const fileConfig = loadConfigFile();
  fileConfig.apiKey = apiKey;
  saveConfigFile(fileConfig);

  ctx.output.success(result.message);
  ctx.output.info(`Key saved to ${CONFIG_FILE()}`);
}

export const loginCommand: Command = {
  name: 'login',
  description: 'Authenticate with Xergon relay',
  aliases: ['auth'],
  options: loginOptions,
  action: loginAction,
};

// ── Logout command ─────────────────────────────────────────────

const logoutOptions: CommandOption[] = [];

async function logoutAction(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const fileConfig = loadConfigFile();

  if (!fileConfig.apiKey) {
    ctx.output.info('No credentials stored. Nothing to log out from.');
    return;
  }

  // Clear the API key
  delete fileConfig.apiKey;
  saveConfigFile(fileConfig);

  ctx.output.success('Credentials cleared.');
  ctx.output.info(`Config saved to ${CONFIG_FILE()}`);
}

export const logoutCommand: Command = {
  name: 'logout',
  description: 'Clear stored credentials',
  aliases: [],
  options: logoutOptions,
  action: logoutAction,
};
