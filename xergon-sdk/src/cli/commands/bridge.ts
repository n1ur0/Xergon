//! `xergon bridge` CLI command for cross-chain operations.
//!
//! Provides commands to:
//!   - `bridge status` — show bridge health across all chains
//!   - `bridge transfer` — initiate a new cross-chain transfer
//!   - `bridge history` — list transfer history with filters
//!   - `bridge chains` — list supported chains and their config

import type { Command } from '../mod';
import { Table } from '@cliffy/table';
import { colors } from '@cliffy/colors';

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

const statusOptions = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'string',
  },
  {
    name: 'chain',
    short: '',
    long: '--chain',
    description: 'Filter by chain (ergo, ethereum, cardano, bitcoin)',
    required: false,
    type: 'string',
  },
];

const transferOptions = [
  {
    name: 'source',
    short: '',
    long: '--source',
    description: 'Source chain (ergo, ethereum, cardano, bitcoin)',
    required: true,
    type: 'string',
  },
  {
    name: 'target',
    short: '',
    long: '--target',
    description: 'Target chain (ergo, ethereum, cardano, bitcoin)',
    required: true,
    type: 'string',
  },
  {
    name: 'amount',
    short: '',
    long: '--amount',
    description: 'Amount in nanoERG',
    required: true,
    type: 'number',
  },
  {
    name: 'recipient',
    short: '',
    long: '--recipient',
    description: 'Recipient address on target chain',
    required: true,
    type: 'string',
  },
  {
    name: 'token-id',
    short: '',
    long: '--token-id',
    description: 'Optional token ID to bridge',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
];

const historyOptions = [
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter by status (initiated, locked, committed, completed, expired, fraud)',
    required: false,
    type: 'string',
  },
  {
    name: 'chain',
    short: '',
    long: '--chain',
    description: 'Filter by source chain',
    required: false,
    type: 'string',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Max results (default: 20)',
    required: false,
    type: 'number',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
];

const chainsOptions = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
];

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

async function statusAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const chain = options.chain as string | undefined;

  try {
    const resp = await fetch('http://127.0.0.1:9090/api/bridge/health');
    const health = await resp.json();

    if (json) {
      console.log(JSON.stringify(health, null, 2));
      return;
    }

    console.log(colors.bold(colors.cyan('\n  Xergon Cross-Chain Bridge Status\n')));

    const rows = [
      [colors.bold('Active Chains'), String(health.active_chains ?? 0)],
      [colors.bold('Pending Transfers'), String(health.pending_transfers ?? 0)],
      [colors.bold('Total Transfers'), String(health.total_transfers ?? 0)],
      [colors.bold('Total Bridged'), `${((health.total_bridged_nanoerg ?? 0) / 1e9).toFixed(4)} ERG`],
      [colors.bold('Fraud Reports'), String(health.fraud_reports ?? 0)],
      [colors.bold('Active Watchers'), String(health.active_watchers ?? 0)],
      [colors.bold('Completion Rate'), `${(health.completion_rate_percent ?? 0).toFixed(1)}%`],
      [colors.bold('Lock Events (24h)'), String(health.lock_events_24h ?? 0)],
      [colors.bold('Last Event'), health.last_event_secs_ago > 0 ? `${health.last_event_secs_ago}s ago` : 'Never'],
    ];

    for (const [key, val] of rows) {
      console.log(`  ${key.padEnd(22)} ${val}`);
    }
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to fetch bridge status'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function transferAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const source = options.source as string;
  const target = options.target as string;
  const amount = Number(options.amount);
  const recipient = options.recipient as string;
  const tokenId = options['token-id'] as string | undefined;

  if (source === target) {
    console.error(colors.red('Source and target chains must differ'));
    process.exit(1);
  }
  if (isNaN(amount) || amount <= 0) {
    console.error(colors.red('Invalid amount'));
    process.exit(1);
  }

  const body: Record<string, unknown> = { source_chain: source, target_chain: target, amount, recipient };
  if (tokenId) body.token_id = tokenId;

  try {
    const resp = await fetch('http://127.0.0.1:9090/api/bridge/transfer', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const data = await resp.json();

    if (!resp.ok) {
      console.error(colors.red(`Transfer failed: ${data.error ?? resp.statusText}`));
      process.exit(1);
    }

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    console.log(colors.green(colors.bold('\n  Transfer Initiated\n')));
    console.log(`  ${colors.bold('Transfer ID')}  ${data.transfer_id}`);
    console.log(`  ${colors.bold('Source')}       ${source} -> ${target}`);
    console.log(`  ${colors.bold('Amount')}       ${(amount / 1e9).toFixed(4)} ERG`);
    console.log(`  ${colors.bold('Recipient')}    ${recipient}`);
    console.log(`  ${colors.bold('Fee')}          ${((data.fee ?? 0) / 1e9).toFixed(6)} ERG`);
    console.log(`  ${colors.bold('Status')}       ${data.status}`);
    console.log();
  } catch (err) {
    console.error(colors.red('Failed to initiate transfer'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function historyAction(options: Record<string, unknown>) {
  const json = options.json as boolean;
  const status = options.status as string | undefined;
  const chain = options.chain as string | undefined;
  const limit = Number(options.limit) || 20;

  const params = new URLSearchParams();
  if (status) params.set('status', status);
  if (chain) params.set('chain', chain);
  params.set('limit', String(limit));

  try {
    const resp = await fetch(`http://127.0.0.1:9090/api/bridge/transfers?${params}`);
    const data = await resp.json();

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    const transfers = data.transfers ?? [];
    if (transfers.length === 0) {
      console.log(colors.gray('  No transfers found'));
      return;
    }

    console.log(colors.bold(colors.cyan(`\n  Transfer History (${transfers.length})\n`)));

    for (const t of transfers) {
      const statusColor = {
        completed: colors.green,
        locked: colors.yellow,
        committed: colors.cyan,
        initiated: colors.blue,
        expired: colors.red,
        fraud_reported: colors.red,
        refunded: colors.gray,
      }[t.status as string] ?? colors.white;

      console.log(`  ${colors.bold(t.transfer_id)}  ${statusColor(t.status)}`);
      console.log(`    ${t.source_chain} -> ${t.target_chain}  |  ${(t.amount / 1e9).toFixed(4)} ERG  |  fee: ${(t.fee / 1e9).toFixed(6)} ERG`);
      console.log(`    sender: ${t.sender}  ->  ${t.recipient}`);
      console.log();
    }
  } catch (err) {
    console.error(colors.red('Failed to fetch transfer history'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

async function chainsAction(options: Record<string, unknown>) {
  const json = options.json as boolean;

  try {
    const resp = await fetch('http://127.0.0.1:9090/api/bridge/chains');
    const data = await resp.json();

    if (json) {
      console.log(JSON.stringify(data, null, 2));
      return;
    }

    const chains = data.chains ?? [];
    if (chains.length === 0) {
      console.log(colors.gray('  No chains configured'));
      return;
    }

    console.log(colors.bold(colors.cyan('\n  Supported Chains\n')));

    new Table()
      .header(['Chain', 'Status', 'Min Confirm', 'Timeout (blocks)', 'Fee (bps)', 'Min Amount', 'Max Amount'])
      .rows(chains.map((c: Record<string, unknown>) => [
        colors.bold(String(c.chain)),
        c.enabled ? colors.green('Enabled') : colors.red('Disabled'),
        String(c.min_confirmations),
        String(c.lock_timeout_blocks),
        String(c.bridge_fee_bps),
        `${((c.min_transfer_amount as number) / 1e9).toFixed(4)} ERG`,
        `${((c.max_transfer_amount as number) / 1e9).toFixed(2)} ERG`,
      ]))
      .border(true)
      .render();

    console.log();
  } catch (err) {
    console.error(colors.red('Failed to fetch chains'));
    console.error(colors.gray(String(err)));
    process.exit(1);
  }
}

// ---------------------------------------------------------------------------
// Command export
// ---------------------------------------------------------------------------

export const bridgeCommand: Command = {
  name: 'bridge',
  description: 'Cross-chain bridge operations (status, transfer, history, chains)',
  aliases: ['xbr'],
  options: [],
  action: () => {},
  subcommands: [
    { name: 'status', description: 'Show bridge health and status', options: statusOptions, action: statusAction },
    { name: 'transfer', description: 'Initiate a cross-chain transfer', options: transferOptions, action: transferAction },
    { name: 'history', description: 'View transfer history', options: historyOptions, action: historyAction },
    { name: 'chains', description: 'List supported chains', options: chainsOptions, action: chainsAction },
  ],
};
