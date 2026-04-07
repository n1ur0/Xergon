/**
 * `xergon treasury` CLI -- Manage treasury funds and multi-sig spending.
 *
 * Interacts with the governance treasury API to check balances, record
 * deposits, propose / sign / execute / fail spends, view history, and
 * manage the multi-signature threshold.
 *
 * Usage:
 *   xergon treasury status
 *   xergon treasury deposit <amount_nanoerg> <tx_id>
 *   xergon treasury propose-spend <proposal_id> <recipient> <amount_nanoerg>
 *   xergon treasury sign <spend_id> <signer_address>
 *   xergon treasury execute <spend_id> <tx_id>
 *   xergon treasury fail <spend_id> <reason>
 *   xergon treasury history [--limit N]
 *   xergon treasury threshold
 *   xergon treasury set-threshold --required K --signatories addr1,addr2,...
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ─── Constants ───────────────────────────────────────────────────────

const NANOERG_PER_ERG = 1_000_000_000;

// ─── Types ───────────────────────────────────────────────────────────

interface TreasuryBalance {
  total_deposits_nanoerg: number;
  total_spent_nanoerg: number;
  available_balance: number;
  locked_balance: number;
  pending_spends: number;
  completed_spends: number;
  failed_spends: number;
}

interface DepositRecord {
  id: string;
  depositor: string;
  amount_nanoerg: number;
  timestamp: string;
  tx_id: string;
}

interface TreasurySpend {
  id: string;
  proposal_id: string;
  recipient: string;
  amount_nanoerg: number;
  status: 'pending' | 'completed' | 'failed' | 'refunded';
  locked_at: string;
  executed_at?: string;
  tx_id?: string;
  signatures_collected: number;
  signatures_required: number;
}

interface ThresholdConfig {
  required_signatures: number;
  total_signatories: number;
  signatory_addresses: string[];
}

interface ApiResponse<T> {
  ok: boolean;
  data?: T;
  error?: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────

/**
 * Convert nanoERG to ERG string with 9 decimal places.
 */
function nanoergToErg(n: number | string): string {
  const value = typeof n === 'string' ? BigInt(n) : BigInt(n);
  const whole = value / BigInt(NANOERG_PER_ERG);
  const frac = value % BigInt(NANOERG_PER_ERG);
  return `${whole}.${frac.toString().padStart(9, '0')}`;
}

/**
 * Check whether --json flag was passed.
 */
function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

/**
 * Return standard headers for API requests.
 */
function apiHeaders(apiKey: string): Record<string, string> {
  return {
    'Authorization': `Bearer ${apiKey}`,
    'Content-Type': 'application/json',
  };
}

/**
 * Perform a GET request against the treasury API.
 */
async function treasuryGet<T>(
  ctx: CLIContext,
  path: string,
): Promise<T> {
  const url = `${ctx.config.baseUrl}${path}`;
  const res = await fetch(url, {
    method: 'GET',
    headers: apiHeaders(ctx.config.apiKey),
  });
  const body: ApiResponse<T> = await res.json();
  if (!body.ok || !body.data) {
    throw new Error(body.error ?? `Request failed with status ${res.status}`);
  }
  return body.data;
}

/**
 * Perform a POST request against the treasury API.
 */
async function treasuryPost<T>(
  ctx: CLIContext,
  path: string,
  payload: Record<string, unknown>,
): Promise<T> {
  const url = `${ctx.config.baseUrl}${path}`;
  const res = await fetch(url, {
    method: 'POST',
    headers: apiHeaders(ctx.config.apiKey),
    body: JSON.stringify(payload),
  });
  const body: ApiResponse<T> = await res.json();
  if (!body.ok || !body.data) {
    throw new Error(body.error ?? `Request failed with status ${res.status}`);
  }
  return body.data;
}

/**
 * Perform a PUT request against the treasury API.
 */
async function treasuryPut<T>(
  ctx: CLIContext,
  path: string,
  payload: Record<string, unknown>,
): Promise<T> {
  const url = `${ctx.config.baseUrl}${path}`;
  const res = await fetch(url, {
    method: 'PUT',
    headers: apiHeaders(ctx.config.apiKey),
    body: JSON.stringify(payload),
  });
  const body: ApiResponse<T> = await res.json();
  if (!body.ok || !body.data) {
    throw new Error(body.error ?? `Request failed with status ${res.status}`);
  }
  return body.data;
}

/**
 * Return a colorized status badge for a spend status.
 */
function statusBadge(status: string): string {
  const map: Record<string, { color: 'green' | 'yellow' | 'red' | 'cyan'; label: string }> = {
    completed:  { color: 'green',  label: 'COMPLETED' },
    pending:    { color: 'yellow', label: 'PENDING  ' },
    failed:     { color: 'red',    label: 'FAILED   ' },
    refunded:   { color: 'cyan',   label: 'REFUNDED ' },
  };
  const entry = map[status] ?? { color: 'dim' as const, label: status.toUpperCase().padEnd(9) };
  return `[\x1b[${entry.color === 'green' ? '32' : entry.color === 'yellow' ? '33' : entry.color === 'red' ? '31' : entry.color === 'cyan' ? '36' : '2'}m${entry.label}\x1b[0m]`;
}

// ─── Subcommand: status ──────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  ctx.output.info('Fetching treasury status...');
  const data = await treasuryGet<TreasuryBalance>(ctx, '/api/gov/treasury');

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Treasury Status', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(44), 'dim'));
  ctx.output.write(ctx.output.formatText({
    'Total Deposits':    `${nanoergToErg(data.total_deposits_nanoerg)} ERG`,
    'Total Spent':       `${nanoergToErg(data.total_spent_nanoerg)} ERG`,
    'Available Balance': `${nanoergToErg(data.available_balance)} ERG`,
    'Locked Balance':    `${nanoergToErg(data.locked_balance)} ERG`,
    'Pending Spends':    String(data.pending_spends),
    'Completed Spends':  String(data.completed_spends),
    'Failed Spends':     String(data.failed_spends),
  }));
}

// ─── Subcommand: deposit ─────────────────────────────────────────────

async function handleDeposit(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const amountNanoerg = args.positional[1];
  const txId = args.positional[2];

  if (!amountNanoerg || !txId) {
    ctx.output.writeError('Usage: xergon treasury deposit <amount_nanoerg> <tx_id>');
    process.exit(1);
    return;
  }

  const amount = Number(amountNanoerg);
  if (Number.isNaN(amount) || amount <= 0) {
    ctx.output.writeError('amount_nanoerg must be a positive number');
    process.exit(1);
    return;
  }

  ctx.output.info(`Recording deposit of ${nanoergToErg(amount)} ERG (tx: ${txId})...`);

  const data = await treasuryPost<DepositRecord>(ctx, '/api/gov/treasury/deposit', {
    amount_nanoerg: amount,
    tx_id: txId,
  });

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.success('Deposit recorded');
  ctx.output.write(ctx.output.formatText({
    'Deposit ID':  data.id,
    'Depositor':   data.depositor,
    'Amount':      `${nanoergToErg(data.amount_nanoerg)} ERG`,
    'TX ID':       data.tx_id,
    'Timestamp':   data.timestamp,
  }));
}

// ─── Subcommand: propose-spend ───────────────────────────────────────

async function handleProposeSpend(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proposalId = args.positional[1];
  const recipient = args.positional[2];
  const amountNanoerg = args.positional[3];

  if (!proposalId || !recipient || !amountNanoerg) {
    ctx.output.writeError(
      'Usage: xergon treasury propose-spend <proposal_id> <recipient> <amount_nanoerg>',
    );
    process.exit(1);
    return;
  }

  const amount = Number(amountNanoerg);
  if (Number.isNaN(amount) || amount <= 0) {
    ctx.output.writeError('amount_nanoerg must be a positive number');
    process.exit(1);
    return;
  }

  ctx.output.info(`Proposing spend of ${nanoergToErg(amount)} ERG to ${recipient}...`);

  const data = await treasuryPost<TreasurySpend>(ctx, '/api/gov/treasury/spend/request', {
    proposal_id: proposalId,
    recipient,
    amount_nanoerg: amount,
  });

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.success('Spend proposed');
  ctx.output.write(ctx.output.formatText({
    'Spend ID':     data.id,
    'Proposal ID':  data.proposal_id,
    'Recipient':    data.recipient,
    'Amount':       `${nanoergToErg(data.amount_nanoerg)} ERG`,
    'Status':       data.status,
    'Signatures':   `${data.signatures_collected} / ${data.signatures_required} required`,
    'Locked At':    data.locked_at,
  }));
}

// ─── Subcommand: sign ────────────────────────────────────────────────

async function handleSign(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const spendId = args.positional[1];
  const signerAddress = args.positional[2];

  if (!spendId || !signerAddress) {
    ctx.output.writeError('Usage: xergon treasury sign <spend_id> <signer_address>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Signing spend ${spendId} with ${signerAddress}...`);

  const data = await treasuryPost<TreasurySpend>(ctx, '/api/gov/treasury/spend/sign', {
    spend_id: spendId,
    signer: signerAddress,
  });

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.success('Signature recorded');
  ctx.output.write(ctx.output.formatText({
    'Spend ID':     data.id,
    'Proposal ID':  data.proposal_id,
    'Amount':       `${nanoergToErg(data.amount_nanoerg)} ERG`,
    'Status':       data.status,
    'Signatures':   `${data.signatures_collected} / ${data.signatures_required} required`,
  }));

  if (data.signatures_collected >= data.signatures_required && data.status === 'pending') {
    ctx.output.info('Threshold met -- the spend can now be executed.');
  }
}

// ─── Subcommand: execute ─────────────────────────────────────────────

async function handleExecute(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const spendId = args.positional[1];
  const txId = args.positional[2];

  if (!spendId || !txId) {
    ctx.output.writeError('Usage: xergon treasury execute <spend_id> <tx_id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Executing spend ${spendId} (tx: ${txId})...`);

  const data = await treasuryPost<TreasurySpend>(ctx, '/api/gov/treasury/spend/execute', {
    spend_id: spendId,
    tx_id: txId,
  });

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.success('Spend executed');
  ctx.output.write(ctx.output.formatText({
    'Spend ID':     data.id,
    'Proposal ID':  data.proposal_id,
    'Recipient':    data.recipient,
    'Amount':       `${nanoergToErg(data.amount_nanoerg)} ERG`,
    'Status':       data.status,
    'TX ID':        data.tx_id ?? '-',
    'Executed At':  data.executed_at ?? '-',
  }));
}

// ─── Subcommand: fail ────────────────────────────────────────────────

async function handleFail(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const spendId = args.positional[1];
  const reason = args.positional.slice(2).join(' ');

  if (!spendId || !reason) {
    ctx.output.writeError('Usage: xergon treasury fail <spend_id> <reason>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Marking spend ${spendId} as failed...`);

  const data = await treasuryPost<TreasurySpend>(ctx, '/api/gov/treasury/spend/fail', {
    spend_id: spendId,
    reason,
  });

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.warn('Spend marked as failed');
  ctx.output.write(ctx.output.formatText({
    'Spend ID':     data.id,
    'Proposal ID':  data.proposal_id,
    'Amount':       `${nanoergToErg(data.amount_nanoerg)} ERG`,
    'Status':       data.status,
  }));
}

// ─── Subcommand: history ─────────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const limit = args.options.limit ? Number(args.options.limit) : 20;

  ctx.output.info(`Fetching spend history (limit: ${limit})...`);

  const data = await treasuryGet<TreasurySpend[]>(
    ctx,
    `/api/gov/treasury/spends?limit=${limit}`,
  );

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  if (data.length === 0) {
    ctx.output.info('No spend records found.');
    return;
  }

  // Table format
  const tableData = data.map((s) => ({
    ID: s.id.length > 14 ? s.id.substring(0, 14) + '...' : s.id,
    Proposal: s.proposal_id.length > 14 ? s.proposal_id.substring(0, 14) + '...' : s.proposal_id,
    Amount: `${nanoergToErg(s.amount_nanoerg)} ERG`,
    Status: s.status.toUpperCase(),
    Sigs: `${s.signatures_collected}/${s.signatures_required}`,
    'Locked At': s.locked_at ? new Date(s.locked_at).toISOString().slice(0, 19).replace('T', ' ') : '-',
  }));

  ctx.output.write(ctx.output.formatTable(tableData, `Treasury Spend History (${data.length} records)`));
}

// ─── Subcommand: threshold ───────────────────────────────────────────

async function handleThreshold(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  ctx.output.info('Fetching threshold configuration...');

  const data = await treasuryGet<ThresholdConfig>(ctx, '/api/gov/treasury/threshold');

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.write(ctx.output.colorize('Treasury Threshold Configuration', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
  ctx.output.write(ctx.output.formatText({
    'Required Signatures': String(data.required_signatures),
    'Total Signatories':   String(data.total_signatories),
    'Signatory Addresses': data.signatory_addresses.join('\n    '),
  }));
}

// ─── Subcommand: set-threshold ───────────────────────────────────────

async function handleSetThreshold(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const required = args.options.required ? Number(args.options.required) : undefined;
  const signatoriesRaw = args.options.signatories
    ? String(args.options.signatories)
    : undefined;

  if (required === undefined || !signatoriesRaw) {
    ctx.output.writeError(
      'Usage: xergon treasury set-threshold --required K --signatories addr1,addr2,...',
    );
    process.exit(1);
    return;
  }

  if (Number.isNaN(required) || required < 1) {
    ctx.output.writeError('--required must be a positive integer');
    process.exit(1);
    return;
  }

  const signatoryAddresses = signatoriesRaw
    .split(',')
    .map((a: string) => a.trim())
    .filter((a: string) => a.length > 0);

  if (signatoryAddresses.length === 0) {
    ctx.output.writeError('--signatories must contain at least one address');
    process.exit(1);
    return;
  }

  if (required > signatoryAddresses.length) {
    ctx.output.writeError(
      `--required (${required}) cannot exceed the number of signatories (${signatoryAddresses.length})`,
    );
    process.exit(1);
    return;
  }

  ctx.output.info(
    `Setting threshold to ${required}/${signatoryAddresses.length} signatures...`,
  );

  const data = await treasuryPut<ThresholdConfig>(
    ctx,
    '/api/gov/treasury/threshold',
    {
      required_signatures: required,
      signatory_addresses: signatoryAddresses,
    },
  );

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(data, null, 2));
    return;
  }

  ctx.output.success('Threshold updated');
  ctx.output.write(ctx.output.colorize('New Threshold Configuration', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
  ctx.output.write(ctx.output.formatText({
    'Required Signatures': String(data.required_signatures),
    'Total Signatories':   String(data.total_signatories),
    'Signatory Addresses': data.signatory_addresses.join('\n    '),
  }));
}

// ─── Command action ──────────────────────────────────────────────────

async function treasuryAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon treasury <subcommand> [options] [arguments]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  status          Show treasury balance and spending summary');
    ctx.output.write('  deposit         Record a deposit to the treasury');
    ctx.output.write('  propose-spend   Propose a new treasury spend');
    ctx.output.write('  sign            Sign a pending treasury spend');
    ctx.output.write('  execute         Execute a fully-signed treasury spend');
    ctx.output.write('  fail            Mark a treasury spend as failed');
    ctx.output.write('  history         List treasury spend history');
    ctx.output.write('  threshold       Show current multi-sig threshold');
    ctx.output.write('  set-threshold   Update the multi-sig threshold');
    process.exit(1);
    return;
  }

  try {
    switch (sub) {
      case 'status':
        await handleStatus(args, ctx);
        break;

      case 'deposit':
        await handleDeposit(args, ctx);
        break;

      case 'propose-spend':
        await handleProposeSpend(args, ctx);
        break;

      case 'sign':
        await handleSign(args, ctx);
        break;

      case 'execute':
        await handleExecute(args, ctx);
        break;

      case 'fail':
        await handleFail(args, ctx);
        break;

      case 'history':
        await handleHistory(args, ctx);
        break;

      case 'threshold':
        await handleThreshold(args, ctx);
        break;

      case 'set-threshold':
        await handleSetThreshold(args, ctx);
        break;

      default:
        ctx.output.writeError(`Unknown subcommand: ${sub}`);
        ctx.output.write('Valid subcommands: status, deposit, propose-spend, sign, execute, fail, history, threshold, set-threshold');
        process.exit(1);
        break;
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Treasury operation failed: ${message}`);
    process.exit(1);
  }
}

// ─── Options ─────────────────────────────────────────────────────────

const treasuryOptions: CommandOption[] = [
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output in JSON format',
    required: false,
    type: 'boolean',
  },
  {
    name: 'limit',
    short: '',
    long: '--limit',
    description: 'Maximum number of history records (default: 20)',
    required: false,
    type: 'number',
  },
  {
    name: 'required',
    short: '',
    long: '--required',
    description: 'Number of required signatures for set-threshold',
    required: false,
    type: 'number',
  },
  {
    name: 'signatories',
    short: '',
    long: '--signatories',
    description: 'Comma-separated list of signatory addresses for set-threshold',
    required: false,
    type: 'string',
  },
];

// ─── Command export ──────────────────────────────────────────────────

export const treasuryCommand: Command = {
  name: 'treasury',
  description: 'Manage treasury funds and multi-sig spending',
  aliases: ['treas'],
  options: treasuryOptions,
  action: treasuryAction,
};
