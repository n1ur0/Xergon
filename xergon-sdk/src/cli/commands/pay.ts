//! `xergon pay` CLI command for token fee payment using Babel boxes (EIP-0031).
//!
//! Provides commands to:
//!   - `pay discover`  — discover available Babel boxes for a token
//!   - `pay select`    — find the best Babel box for a given ERG need
//!   - `pay estimate`  — calculate token cost for a given ERG amount
//!   - `pay price`     — get current token price from best Babel box
//!   - `pay verify`    — verify a payment transaction on-chain
//!   - `pay budget`    — show user budget status
//!   - `pay budget set` — set user budget

import { Command } from '@cliffy/command';
import { Table } from '@cliffy/table';
import { colors } from '@cliffy/colors';
import type { ParsedArgs } from '../mod';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const RELAY_URL = process.env.XERGON_RELAY_URL || 'http://localhost:9090';
const AGENT_URL = process.env.XERGON_AGENT_URL || 'http://localhost:9091';

/** Format nanoERG to human-readable ERG string. */
function formatErg(nanoErg: number): string {
  return `${(nanoErg / 1e9).toFixed(4)} ERG`;
}

/** Truncate a box/token ID for display. */
function truncateId(id: string, len = 12): string {
  if (id.length <= len * 2) return id;
  return `${id.slice(0, len)}...${id.slice(-len)}`;
}

/** Color a status string. */
function statusColor(status: string): string {
  const s = status.toLowerCase();
  if (s === 'confirmed' || s === 'active' || s === 'ok' || s === 'recorded') return colors.green(status);
  if (s === 'pending') return colors.yellow(status);
  if (s === 'failed' || s === 'expired' || s === 'rejected') return colors.red(status);
  return status;
}

/** Determine budget alert level. */
function alertLevel(remaining: number, total: number): { level: string; color: (s: string) => string } {
  const pct = total > 0 ? remaining / total : 1;
  if (pct > 0.5) return { level: 'low', color: colors.green };
  if (pct > 0.2) return { level: 'medium', color: colors.yellow };
  return { level: 'high', color: colors.red };
}

// ---------------------------------------------------------------------------
// Options (all subcommand options merged for the parser)
// ---------------------------------------------------------------------------

const commonOptions = [
  { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' as const },
];

const discoverOptions = [
  { name: 'limit', short: '', long: '--limit', description: 'Max boxes to show (default: 10)', required: false, type: 'number' as const },
  ...commonOptions,
];

const selectOptions = [...commonOptions];
const estimateOptions = [...commonOptions];

const priceOptions = [
  { name: 'decimals', short: '', long: '--decimals', description: 'Decimal places for price (default: 4)', required: false, type: 'number' as const },
  ...commonOptions,
];

const verifyOptions = [...commonOptions];

const budgetOptions = [...commonOptions];

const budgetSetOptions = [
  { name: 'daily-limit', short: '', long: '--daily-limit', description: 'Daily spending limit in ERG', required: false, type: 'number' as const },
  ...commonOptions,
];

// ---------------------------------------------------------------------------
// Subcommand action handlers
// ---------------------------------------------------------------------------

async function discoverAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const tokenId = positional[0];
  if (!tokenId) {
    console.error(colors.red('Missing required argument: <token-id>'));
    console.error(colors.gray('Usage: xergon pay discover <token-id> [--limit N] [--json]'));
    process.exit(1);
  }

  const limit = Number(options.limit) || 10;
  const json = options.json as boolean;

  try {
    const resp = await fetch(`${RELAY_URL}/api/v1/babel/discover/${encodeURIComponent(tokenId)}?limit=${limit}`);
    if (!resp.ok) {
      const body = await resp.json().catch(() => ({}));
      console.error(colors.red(`Discovery failed: ${(body as Record<string, unknown>).error ?? resp.statusText}`));
      process.exit(1);
    }

    const data = await resp.json() as Record<string, unknown>;
    const boxes = (data.boxes ?? data.results ?? []) as Record<string, unknown>[];

    if (json) {
      console.log(JSON.stringify({ token_id: tokenId, boxes }, null, 2));
      return;
    }

    if (boxes.length === 0) {
      console.log(colors.gray(`  No Babel boxes found for token ${truncateId(tokenId, 16)}`));
      return;
    }

    console.log(colors.bold(colors.cyan(`\n  Babel Boxes for ${truncateId(tokenId, 16)}\n`)));

    new Table()
      .header(['Box ID', 'ERG Value', 'Token Price', 'Liquidity Score'])
      .rows(boxes.slice(0, limit).map((box) => [
        colors.bold(truncateId(String(box.box_id ?? box.id ?? ''), 14)),
        formatErg(Number(box.erg_value ?? box.erg ?? 0)),
        String(box.token_price ?? box.price ?? 'N/A'),
        String(box.liquidity_score ?? box.score ?? 'N/A'),
      ]))
      .border(true)
      .render();
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to discover Babel boxes'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function selectAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const tokenId = positional[0];
  const requiredErg = positional[1];
  if (!tokenId || !requiredErg) {
    console.error(colors.red('Missing required arguments'));
    console.error(colors.gray('Usage: xergon pay select <token-id> <required-erg> [--json]'));
    process.exit(1);
  }

  const ergAmount = Number(requiredErg);
  if (isNaN(ergAmount) || ergAmount <= 0) {
    console.error(colors.red('Invalid ERG amount — must be a positive number'));
    process.exit(1);
  }

  const json = options.json as boolean;

  try {
    const resp = await fetch(`${RELAY_URL}/api/v1/babel/select/${encodeURIComponent(tokenId)}/${ergAmount}`);
    if (!resp.ok) {
      const body = await resp.json().catch(() => ({}));
      console.error(colors.red(`Select failed: ${(body as Record<string, unknown>).error ?? resp.statusText}`));
      process.exit(1);
    }

    const data = await resp.json() as Record<string, unknown>;

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    console.log(colors.green(colors.bold('\n  Best Babel Box Selected\n')));
    for (const [key, label] of [
      ['box_id', 'Box ID'],
      ['erg_value', 'ERG Value'],
      ['token_price', 'Token Price'],
      ['estimated_token_cost', 'Est. Token Cost'],
      ['swap_calc', 'Swap Calculation'],
      ['liquidity_score', 'Liquidity Score'],
    ] as [string, string][]) {
      const val = data[key];
      if (val !== undefined) {
        const display = key === 'box_id' ? truncateId(String(val), 14) : key.includes('erg') || key.includes('value') ? formatErg(Number(val)) : String(val);
        console.log(`  ${colors.bold(label.padEnd(22))} ${display}`);
      }
    }
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to select Babel box'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function estimateAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const tokenId = positional[0];
  const ergAmount = positional[1];
  if (!tokenId || !ergAmount) {
    console.error(colors.red('Missing required arguments'));
    console.error(colors.gray('Usage: xergon pay estimate <token-id> <erg-amount> [--json]'));
    process.exit(1);
  }

  const ergNeeded = Number(ergAmount);
  if (isNaN(ergNeeded) || ergNeeded <= 0) {
    console.error(colors.red('Invalid ERG amount — must be a positive number'));
    process.exit(1);
  }

  const json = options.json as boolean;

  try {
    // Step 1: Discover best box to get token_price
    const discResp = await fetch(`${RELAY_URL}/api/v1/babel/discover/${encodeURIComponent(tokenId)}?limit=1`);
    if (!discResp.ok) {
      const body = await discResp.json().catch(() => ({}));
      console.error(colors.red(`Discovery failed: ${(body as Record<string, unknown>).error ?? discResp.statusText}`));
      process.exit(1);
    }

    const discData = await discResp.json() as Record<string, unknown>;
    const boxes = (discData.boxes ?? discData.results ?? []) as Record<string, unknown>[];
    if (boxes.length === 0) {
      console.error(colors.red(`No Babel boxes found for token ${truncateId(tokenId, 16)}`));
      process.exit(1);
    }

    const bestBox = boxes[0];
    const tokenPrice = Number(bestBox.token_price ?? bestBox.price ?? 0);

    // Step 2: Calculate swap
    const calcResp = await fetch(`${RELAY_URL}/api/v1/babel/calculate-swap`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        token_id: tokenId,
        token_price: tokenPrice,
        erg_needed: ergNeeded,
      }),
    });

    if (!calcResp.ok) {
      const body = await calcResp.json().catch(() => ({}));
      console.error(colors.red(`Calculation failed: ${(body as Record<string, unknown>).error ?? calcResp.statusText}`));
      process.exit(1);
    }

    const calcData = await calcResp.json() as Record<string, unknown>;

    if (json) {
      console.log(JSON.stringify({
        token_id: tokenId,
        erg_needed: ergNeeded,
        token_cost: calcData.token_cost ?? calcData.tokens_required,
        token_price: tokenPrice,
        usd_equivalent: calcData.usd_equivalent ?? null,
        box_id: bestBox.box_id ?? bestBox.id,
      }, null, 2));
      return;
    }

    console.log(colors.bold(colors.cyan('\n  Payment Estimate\n')));
    for (const [key, label] of [
      ['erg_needed', 'ERG Needed'],
      ['token_cost', 'Token Cost'],
      ['token_price', 'Token Price'],
      ['usd_equivalent', 'USD Equivalent'],
      ['box_id', 'Source Box'],
    ] as [string, string][]) {
      const val = key === 'erg_needed' ? ergNeeded : key === 'token_price' ? tokenPrice : key === 'box_id' ? String(bestBox.box_id ?? bestBox.id ?? '') : calcData[key];
      if (val !== undefined && val !== null) {
        const display = key === 'box_id' ? truncateId(String(val), 14) : key.includes('erg') || key === 'usd_equivalent' ? `$${Number(val).toFixed(2)}` : String(val);
        console.log(`  ${colors.bold(label.padEnd(22))} ${display}`);
      }
    }
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to calculate payment estimate'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function priceAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const tokenId = positional[0];
  if (!tokenId) {
    console.error(colors.red('Missing required argument: <token-id>'));
    console.error(colors.gray('Usage: xergon pay price <token-id> [--decimals N] [--json]'));
    process.exit(1);
  }

  const decimals = Number(options.decimals) || 4;
  const json = options.json as boolean;

  try {
    const resp = await fetch(`${RELAY_URL}/api/v1/babel/price/${encodeURIComponent(tokenId)}`);
    if (!resp.ok) {
      const body = await resp.json().catch(() => ({}));
      console.error(colors.red(`Price lookup failed: ${(body as Record<string, unknown>).error ?? resp.statusText}`));
      process.exit(1);
    }

    const data = await resp.json() as Record<string, unknown>;

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    const nanoErgPerToken = Number(data.nanoerg_per_token ?? data.price_raw ?? data.price ?? 0);
    const ergPerToken = nanoErgPerToken / 1e9;

    console.log(colors.bold(colors.cyan(`\n  Token Price: ${truncateId(tokenId, 16)}\n`)));
    console.log(`  ${colors.bold('Raw (nanoERG)'.padEnd(22))} ${nanoErgPerToken.toLocaleString()}`);
    console.log(`  ${colors.bold('ERG / token'.padEnd(22))} ${ergPerToken.toFixed(decimals)}`);
    if (data.box_id) {
      console.log(`  ${colors.bold('Source Box'.padEnd(22))} ${truncateId(String(data.box_id), 14)}`);
    }
    if (data.oracle_price_usd !== undefined) {
      console.log(`  ${colors.bold('Oracle USD'.padEnd(22))} $${Number(data.oracle_price_usd).toFixed(2)}`);
    }
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to fetch token price'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function verifyAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const txId = positional[0];
  if (!txId) {
    console.error(colors.red('Missing required argument: <tx-id>'));
    console.error(colors.gray('Usage: xergon pay verify <tx-id> [--json]'));
    process.exit(1);
  }

  const json = options.json as boolean;

  try {
    // POST to record-usage endpoint to check if tx was recorded
    const resp = await fetch(`${RELAY_URL}/api/v1/cost/record-usage`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tx_id: txId }),
    });

    if (!resp.ok) {
      const body = await resp.json().catch(() => ({}));
      console.error(colors.red(`Verification failed: ${(body as Record<string, unknown>).error ?? resp.statusText}`));
      process.exit(1);
    }

    const data = await resp.json() as Record<string, unknown>;

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    console.log(colors.bold(colors.cyan('\n  Payment Verification\n')));
    for (const [key, label] of [
      ['tx_id', 'Transaction ID'],
      ['status', 'Status'],
      ['amount_paid', 'Amount Paid'],
      ['erg_value', 'ERG Value'],
      ['confirmed_at', 'Confirmed At'],
      ['confirmations', 'Confirmations'],
      ['recorded', 'Recorded'],
    ] as [string, string][]) {
      const val = data[key];
      if (val !== undefined && val !== null) {
        const display = key === 'status' ? statusColor(String(val)) : key.includes('amount') || key.includes('erg') ? formatErg(Number(val)) : String(val);
        console.log(`  ${colors.bold(label.padEnd(22))} ${display}`);
      }
    }
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to verify payment'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function budgetAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const userId = positional[0];
  if (!userId) {
    console.error(colors.red('Missing required argument: <user-id>'));
    console.error(colors.gray('Usage: xergon pay budget <user-id> [--json]'));
    process.exit(1);
  }

  const json = options.json as boolean;

  try {
    const resp = await fetch(`${RELAY_URL}/api/v1/budget/${encodeURIComponent(userId)}`);
    if (!resp.ok) {
      const body = await resp.json().catch(() => ({}));
      console.error(colors.red(`Budget lookup failed: ${(body as Record<string, unknown>).error ?? resp.statusText}`));
      process.exit(1);
    }

    const data = await resp.json() as Record<string, unknown>;

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    const totalBudget = Number(data.total_budget ?? data.budget ?? 0);
    const spent = Number(data.spent ?? data.used ?? 0);
    const remaining = Number(data.remaining ?? data.balance ?? totalBudget - spent);
    const dailyLimit = Number(data.daily_limit ?? 0);
    const dailySpent = Number(data.daily_spent ?? 0);
    const alert = alertLevel(remaining, totalBudget);

    console.log(colors.bold(colors.cyan(`\n  Budget Status: ${userId}\n`)));
    console.log(`  ${colors.bold('Total Budget'.padEnd(22))} ${formatErg(totalBudget)}`);
    console.log(`  ${colors.bold('Spent'.padEnd(22))} ${formatErg(spent)}`);
    console.log(`  ${colors.bold('Remaining'.padEnd(22))} ${formatErg(remaining)}`);
    if (dailyLimit > 0) {
      console.log(`  ${colors.bold('Daily Limit'.padEnd(22))} ${formatErg(dailyLimit)}`);
      console.log(`  ${colors.bold('Daily Spent'.padEnd(22))} ${formatErg(dailySpent)}`);
      console.log(`  ${colors.bold('Daily Remaining'.padEnd(22))} ${formatErg(Math.max(0, dailyLimit - dailySpent))}`);
    }
    console.log(`  ${colors.bold('Alert Level'.padEnd(22))} ${alert.color(alert.level)}`);
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to fetch budget'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function budgetSetAction(positional: string[], options: Record<string, unknown>): Promise<void> {
  const userId = positional[0];
  const budgetErg = positional[1];
  if (!userId || !budgetErg) {
    console.error(colors.red('Missing required arguments'));
    console.error(colors.gray('Usage: xergon pay budget set <user-id> <budget-erg> [--daily-limit <erg>] [--json]'));
    process.exit(1);
  }

  const budget = Number(budgetErg);
  if (isNaN(budget) || budget <= 0) {
    console.error(colors.red('Invalid budget amount — must be a positive number'));
    process.exit(1);
  }

  const dailyLimit = options['daily-limit'] as number | undefined;
  const json = options.json as boolean;

  const body: Record<string, unknown> = { user_id: userId, budget_erg: budget };
  if (dailyLimit !== undefined) body.daily_limit = dailyLimit;

  try {
    const resp = await fetch(`${RELAY_URL}/api/v1/budget/set`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    if (!resp.ok) {
      const respBody = await resp.json().catch(() => ({}));
      console.error(colors.red(`Budget set failed: ${(respBody as Record<string, unknown>).error ?? resp.statusText}`));
      process.exit(1);
    }

    const data = await resp.json() as Record<string, unknown>;

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    console.log(colors.green(colors.bold('\n  Budget Updated\n')));
    console.log(`  ${colors.bold('User ID'.padEnd(22))} ${userId}`);
    console.log(`  ${colors.bold('Total Budget'.padEnd(22))} ${formatErg(budget)}`);
    if (dailyLimit !== undefined) {
      console.log(`  ${colors.bold('Daily Limit'.padEnd(22))} ${formatErg(dailyLimit)}`);
    }
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to set budget'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

// ---------------------------------------------------------------------------
// Command export
// ---------------------------------------------------------------------------

export const payCommand: Command = {
  name: 'pay',
  description: 'Token fee payments via Babel boxes (EIP-0031)',
  aliases: ['fee'],
  options: [
    { name: 'limit', short: '', long: '--limit', description: 'Max boxes to show (default: 10)', required: false, type: 'number' },
    { name: 'decimals', short: '', long: '--decimals', description: 'Decimal places for price (default: 4)', required: false, type: 'number' },
    { name: 'daily-limit', short: '', long: '--daily-limit', description: 'Daily spending limit in ERG', required: false, type: 'number' },
    { name: 'json', short: '', long: '--json', description: 'Output in JSON format', required: false, type: 'boolean' },
  ],
  action: async (args: ParsedArgs) => {
    const sub = args.positional[0];

    // Shift subcommand off positional so handlers see args[0..]
    const subPositional = args.positional.slice(1);

    switch (sub) {
      case 'discover':
        return discoverAction(subPositional, args.options);
      case 'select':
        return selectAction(subPositional, args.options);
      case 'estimate':
        return estimateAction(subPositional, args.options);
      case 'price':
        return priceAction(subPositional, args.options);
      case 'verify':
        return verifyAction(subPositional, args.options);
      case 'budget': {
        // `budget` or `budget set`
        if (subPositional[0] === 'set') {
          return budgetSetAction(subPositional.slice(1), args.options);
        }
        return budgetAction(subPositional, args.options);
      }
      default:
        console.error(colors.red(sub ? `Unknown subcommand: ${sub}` : 'Missing subcommand'));
        console.error(colors.gray('Available: discover, select, estimate, price, verify, budget, budget set'));
        process.exit(1);
    }
  },
  subcommands: [
    { name: 'discover', description: 'Discover available Babel boxes for a token', options: discoverOptions, action: () => {} },
    { name: 'select', description: 'Find the best Babel box for a given ERG need', options: selectOptions, action: () => {} },
    { name: 'estimate', description: 'Calculate token cost for a given ERG amount', options: estimateOptions, action: () => {} },
    { name: 'price', description: 'Get current token price from best Babel box', options: priceOptions, action: () => {} },
    { name: 'verify', description: 'Verify a payment transaction on-chain', options: verifyOptions, action: () => {} },
    { name: 'budget', description: 'Show or set user budget status', options: budgetOptions, action: () => {} },
    { name: 'budget-set', description: 'Set user budget (xergon pay budget set <user-id> <budget-erg>)', options: budgetSetOptions, action: () => {} },
  ],
};
