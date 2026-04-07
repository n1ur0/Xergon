#!/usr/bin/env node

/**
 * CLI command: onboard
 *
 * Interactive and non-interactive provider onboarding.
 * Calls POST /v1/providers/onboard via the API client.
 */

import * as readline from 'node:readline';
import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

const onboardOptions: CommandOption[] = [
  {
    name: 'endpoint',
    short: '-e',
    long: '--endpoint',
    description: 'Provider agent endpoint URL (e.g. http://1.2.3.4:9099)',
    required: false,
    type: 'string',
  },
  {
    name: 'region',
    short: '-r',
    long: '--region',
    description: 'Provider region (e.g. us-east, eu-west)',
    required: false,
    type: 'string',
  },
  {
    name: 'authToken',
    short: '',
    long: '--auth-token',
    description: 'Onboarding auth token (if relay requires one)',
    required: false,
    type: 'string',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Check onboarding status for a provider public key',
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

/**
 * Create a readline interface for interactive prompts.
 */
function createReadline(): readline.Interface {
  return readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });
}

/**
 * Prompt the user for input with a default value.
 */
function prompt(rl: readline.Interface, question: string, defaultValue?: string): Promise<string> {
  return new Promise((resolve) => {
    const display = defaultValue ? `${question} [${defaultValue}]: ` : `${question}: `;
    rl.question(display, (answer: string) => {
      resolve(answer.trim() || defaultValue || '');
    });
  });
}

/**
 * Validate a URL string.
 */
function isValidUrl(str: string): boolean {
  try {
    const url = new URL(str);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

/**
 * Onboard a provider (non-interactive mode).
 */
async function onboardNonInteractive(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const endpoint = args.options.endpoint ? String(args.options.endpoint) : '';
  const region = args.options.region ? String(args.options.region) : undefined;
  const authToken = args.options.authToken ? String(args.options.authToken) : undefined;

  if (!endpoint) {
    ctx.output.writeError('Missing required option: --endpoint');
    ctx.output.writeError('Usage: xergon onboard --endpoint <url> [--region <region>] [--auth-token <token>]');
    process.exit(1);
  }

  if (!isValidUrl(endpoint)) {
    ctx.output.writeError(`Invalid endpoint URL: ${endpoint}`);
    process.exit(1);
  }

  const body: Record<string, unknown> = { endpoint, region };
  if (authToken) {
    body.auth_token = authToken;
  }

  try {
    ctx.output.info(`Onboarding provider at ${endpoint}...`);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const response: any = await ctx.client.post('/v1/providers/onboard', body, { skipAuth: true });

    if (args.options.json) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(response));
    } else {
      ctx.output.success('Provider onboarded successfully!');
      ctx.output.write(ctx.output.formatText({
        'Provider ID': response.provider_pk,
        'Models': response.models.join(', ') || '(none detected)',
        'Status': response.status,
      }));
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Onboarding failed: ${message}`);
    process.exit(1);
  }
}

/**
 * Check onboarding status for a provider.
 */
async function checkOnboardingStatus(providerPk: string, ctx: CLIContext, outputJson: boolean): Promise<void> {
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const response: any = await ctx.client.get(`/v1/providers/onboard/${providerPk}`, { skipAuth: true });

    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(response));
    } else {
      ctx.output.info(`Onboarding status for ${providerPk}:`);
      ctx.output.write(ctx.output.formatText({
        'Status': response.status,
        'Steps Completed': response.steps_completed.join(', ') || '(none)',
        'Steps Remaining': response.steps_remaining.join(', ') || '(none)',
      }));
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get onboarding status: ${message}`);
    process.exit(1);
  }
}

/**
 * Interactive onboarding flow.
 */
async function onboardInteractive(ctx: CLIContext): Promise<void> {
  const rl = createReadline();

  try {
    ctx.output.info('Xergon Provider Onboarding');
    ctx.output.write('This will register your agent endpoint with the Xergon relay.\n');

    // Prompt for endpoint
    let endpoint: string;
    while (true) {
      endpoint = await prompt(rl, 'Provider agent endpoint URL');
      if (isValidUrl(endpoint)) break;
      ctx.output.writeError('Invalid URL. Must start with http:// or https://');
    }

    // Prompt for region
    const region = await prompt(rl, 'Region (e.g. us-east, eu-west, ap-south)');

    // Prompt for auth token (optional)
    const authToken = await prompt(rl, 'Auth token (leave empty if not required)');

    rl.close();

    // Build request body
    const body: Record<string, unknown> = {
      endpoint: endpoint.trim(),
      region: region.trim() || undefined,
    };
    if (authToken.trim()) {
      body.auth_token = authToken.trim();
    }

    ctx.output.info(`Onboarding provider at ${endpoint.trim()}...`);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const response: any = await ctx.client.post('/v1/providers/onboard', body, { skipAuth: true });

    ctx.output.success('Provider onboarded successfully!');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': response.provider_pk,
      'Models': response.models.join(', ') || '(none detected)',
      'Status': response.status,
    }));

    ctx.output.info('Use "xergon onboard --status <provider_pk>" to check onboarding progress.');
  } catch (err) {
    rl.close();
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Onboarding failed: ${message}`);
    process.exit(1);
  }
}

async function onboardAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const outputJson = args.options.json === true;
  const statusPk = args.options.status ? String(args.options.status) : undefined;

  // Check status mode
  if (statusPk) {
    await checkOnboardingStatus(statusPk, ctx, outputJson);
    return;
  }

  // Non-interactive mode if endpoint is provided
  if (args.options.endpoint) {
    await onboardNonInteractive(args, ctx);
    return;
  }

  // Interactive mode
  await onboardInteractive(ctx);
}

export const onboardCommand: Command = {
  name: 'onboard',
  description: 'Register a provider with the Xergon relay',
  aliases: ['register'],
  options: onboardOptions,
  action: onboardAction,
};
