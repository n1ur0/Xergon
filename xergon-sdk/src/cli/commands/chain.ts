/**
 * CLI command: chain
 *
 * On-chain state inspection for the Xergon Network. View UTXO boxes,
 * check balances, inspect transactions, and query blockchain state.
 *
 * Usage:
 *   xergon chain boxes <address>       -- List UTXO boxes for an address
 *   xergon chain box <box-id>          -- Get box details by ID
 *   xergon chain height                -- Current blockchain height
 *   xergon chain balance <address>     -- ERG balance (formatted: X.XXX ERG)
 *   xergon chain tokens <address>      -- Token holdings
 *   xergon chain providers             -- List registered Xergon providers
 *   xergon chain stake <address>       -- Show staking box info
 *   xergon chain tx <tx-id>            -- Transaction details
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Constants ──────────────────────────────────────────────────────

const NANO_ERG_PER_ERG = 1_000_000_000;
const MIN_BOX_VALUE_PER_BYTE = 360;

const DEFAULT_NODES: Record<string, string> = {
  testnet: 'https://node-testnet.ergo.network',
  mainnet: 'https://node.ergo.network',
};

const EXPLORER_URL = 'https://explorer.ergoplatform.com';

// ── Types ──────────────────────────────────────────────────────────

interface ChainBox {
  boxId: string;
  value: string;
  ergoTree: string;
  registers: Record<string, string>;
  tokens: Array<{ tokenId: string; amount: string; name?: string; decimals?: number }>;
  creationHeight: number;
  transactionId: string;
  index: number;
  spent?: boolean;
}

interface AddressBoxes {
  items: ChainBox[];
  total: number;
}

interface BalanceInfo {
  address: string;
  nanoErgs: string;
  ergs: string;
  tokens: Array<{ tokenId: string; amount: string; name?: string; decimals?: number }>;
  boxesCount?: number;
}

interface TokenHolding {
  tokenId: string;
  amount: string;
  name?: string;
  decimals?: number;
}

interface ProviderInfo {
  boxId: string;
  address: string;
  stakeAmount: string;
  stakeErg: string;
  region: string;
  model: string;
  status: 'active' | 'inactive' | 'slashed';
  registeredHeight: number;
}

interface StakeInfo {
  boxId: string;
  stakedAmount: string;
  stakedErg: string;
  rewardAddress: string;
  lockEpochs: number;
  currentEpoch: number;
  status: 'locked' | 'unlocked' | 'withdrawable';
  registeredHeight: number;
  tokens: Array<{ tokenId: string; amount: string; name?: string }>;
}

interface TxDetails {
  txId: string;
  timestamp: number;
  height: number;
  size: number;
  inputsCount: number;
  outputsCount: number;
  inputs: Array<{ boxId: string; value: string; address?: string }>;
  outputs: Array<{ boxId: string; value: string; address?: string; ergoTree?: string }>;
  fee: string;
  feeErg: string;
  status: 'confirmed' | 'pending' | 'unknown';
}

// ── Helpers ────────────────────────────────────────────────────────

function getNodeUrl(args: ParsedArgs): string {
  if (args.options.node) return String(args.options.node);
  const network = String(args.options.network || 'mainnet');
  return DEFAULT_NODES[network] || DEFAULT_NODES.mainnet;
}

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

/**
 * Convert nanoERG to ERG (human-readable).
 */
export function nanoErgToErg(nanoErg: string | number | bigint): string {
  const val = typeof nanoErg === 'string' ? BigInt(nanoErg) : BigInt(nanoErg);
  const ergWhole = val / BigInt(NANO_ERG_PER_ERG);
  const ergFrac = val % BigInt(NANO_ERG_PER_ERG);
  const fracStr = ergFrac.toString().padStart(9, '0').replace(/0+$/, '');
  return `${ergWhole}.${fracStr || '0'}`;
}

/**
 * Format nanoERG as "X.XXX ERG" with 3 decimal places.
 */
export function formatErg(nanoErg: string | number | bigint): string {
  const val = typeof nanoErg === 'string' ? BigInt(nanoErg) : BigInt(nanoErg);
  const ergWhole = val / BigInt(NANO_ERG_PER_ERG);
  const ergFrac = val % BigInt(NANO_ERG_PER_ERG);
  // Take first 3 digits of fractional part
  const fracDigits = ergFrac.toString().padStart(9, '0').substring(0, 3);
  return `${ergWhole}.${fracDigits} ERG`;
}

/**
 * Truncate an ID for display (show first N + last 4 chars).
 */
export function truncateId(id: string, prefixLen: number = 8): string {
  if (id.length <= prefixLen + 8) return id;
  return `${id.substring(0, prefixLen)}...${id.substring(id.length - 4)}`;
}

/**
 * Color a status string for terminal output.
 */
export function boxStatusColor(spent: boolean | undefined, useColor: boolean): string {
  if (!useColor) return spent ? 'spent' : 'unspent';
  return spent ? '\x1b[31mspent\x1b[0m' : '\x1b[32munspent\x1b[0m';
}

/**
 * Decode a register value from base64 or raw string.
 */
export function decodeRegisterValue(reg: any): string {
  if (!reg) return '';
  if (typeof reg === 'string') {
    try {
      const decoded = Buffer.from(reg, 'base64').toString('utf-8').replace(/\0/g, '');
      // Check if the decoded result looks reasonable (printable ASCII)
      if (/^[\x20-\x7E]+$/.test(decoded) && decoded.length > 0) {
        return decoded;
      }
    } catch {
      // Not valid base64, return as-is
    }
    return reg;
  }
  if (typeof reg === 'object' && reg.serializedValue) {
    try {
      const decoded = Buffer.from(reg.serializedValue, 'base64').toString('utf-8').replace(/\0/g, '');
      if (/^[\x20-\x7E]+$/.test(decoded) && decoded.length > 0) {
        return decoded;
      }
    } catch {
      // fall through
    }
    return reg.serializedValue;
  }
  return String(reg);
}

/**
 * Extract registers R4-R9 from a box and decode them.
 */
export function extractRegisters(box: any): Record<string, string> {
  const regs: Record<string, string> = {};
  const rawRegs = box.additionalRegisters || box.registers || {};
  for (const key of ['R4', 'R5', 'R6', 'R7', 'R8', 'R9']) {
    if (rawRegs[key]) {
      const decoded = decodeRegisterValue(rawRegs[key]);
      if (decoded) regs[key] = decoded;
    }
  }
  return regs;
}

/**
 * Compute minimum box value based on box size estimate.
 */
export function minBoxValue(boxSizeBytes: number): bigint {
  return BigInt(boxSizeBytes) * BigInt(MIN_BOX_VALUE_PER_BYTE);
}

/**
 * Generate an explorer URL for a transaction or box.
 */
export function explorerLink(id: string, type: 'tx' | 'box' | 'address' = 'tx'): string {
  return `${EXPLORER_URL}/${type}/${id}`;
}

/**
 * Fetch from Ergo node REST API with error handling.
 */
async function fetchNode<T>(nodeUrl: string, path: string): Promise<T> {
  const url = `${nodeUrl.replace(/\/+$/, '')}${path}`;
  const res = await fetch(url, {
    signal: AbortSignal.timeout(30000),
    headers: { 'Content-Type': 'application/json' },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`Node returned ${res.status}: ${body || res.statusText}`);
  }
  return res.json() as Promise<T>;
}

/**
 * Map raw node box to ChainBox.
 */
function mapBoxToChainBox(raw: any): ChainBox {
  return {
    boxId: raw.boxId,
    value: String(raw.value || raw.nanoErgs || 0),
    ergoTree: raw.ergoTree || '',
    registers: raw.additionalRegisters || raw.registers || {},
    tokens: (raw.assets || []).map((a: any) => ({
      tokenId: a.tokenId,
      amount: String(a.amount || a.value || 0),
      name: a.name,
      decimals: a.decimals,
    })),
    creationHeight: raw.creationHeight || 0,
    transactionId: raw.transactionId || '',
    index: raw.index || 0,
    spent: raw.spent,
  };
}

// ── Subcommand: boxes ──────────────────────────────────────────────

async function handleBoxes(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);
  const address = args.positional[1];

  if (!address) {
    ctx.output.writeError('Usage: xergon chain boxes <address>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Fetching boxes for ${truncateId(address, 12)}...`);

  try {
    let boxes: ChainBox[];

    if (ctx.client?.chain?.getBoxesByAddress) {
      const result = await ctx.client.chain.getBoxesByAddress(address);
      boxes = Array.isArray(result) ? result : (result as AddressBoxes).items || [];
    } else {
      const data = await fetchNode<any>(nodeUrl, `/utxo/byAddress/${address}`);
      if (Array.isArray(data)) {
        boxes = data.map(mapBoxToChainBox);
      } else if (data && Array.isArray(data.items)) {
        boxes = data.items.map(mapBoxToChainBox);
      } else {
        boxes = [];
      }
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ address, totalBoxes: boxes.length, boxes }, null, 2));
      return;
    }

    const useColor = ctx.config.color;

    ctx.output.write(ctx.output.colorize(`UTXO Boxes for ${truncateId(address, 12)}`, 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(55), 'dim'));
    ctx.output.write(`  Total boxes: ${boxes.length}`);
    ctx.output.write('');

    if (boxes.length === 0) {
      ctx.output.info('  No unspent boxes found.');
      return;
    }

    for (const box of boxes) {
      const status = boxStatusColor(box.spent, useColor);
      const valueStr = formatErg(box.value);
      const tokenCount = box.tokens.length;

      ctx.output.write(`  ${ctx.output.colorize(truncateId(box.boxId), 'cyan')}  ${valueStr}  ${status}`);
      ctx.output.write(`    Height: ${box.creationHeight}  |  Tokens: ${tokenCount}  |  Tx: ${truncateId(box.transactionId)}`);

      // Show registers R4-R9
      const regs = extractRegisters(box);
      const regKeys = Object.keys(regs);
      if (regKeys.length > 0) {
        const regStr = regKeys.map(k => `${k}=${truncateId(regs[k], 12)}`).join('  ');
        ctx.output.write(`    Regs: ${regStr}`);
      }

      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to fetch boxes: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: box ────────────────────────────────────────────────

async function handleBox(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);
  const boxId = args.positional[1];

  if (!boxId) {
    ctx.output.writeError('Usage: xergon chain box <box-id>');
    process.exit(1);
    return;
  }

  try {
    let box: ChainBox;

    if (ctx.client?.chain?.getBox) {
      box = await ctx.client.chain.getBox(boxId);
    } else {
      const raw = await fetchNode<any>(nodeUrl, `/utxo/byId/${boxId}`);
      box = mapBoxToChainBox(raw);
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(box, null, 2));
      return;
    }

    const useColor = ctx.config.color;
    const status = boxStatusColor(box.spent, useColor);

    ctx.output.write(ctx.output.colorize('Box Details', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(55), 'dim'));
    ctx.output.write(`  Box ID:        ${ctx.output.colorize(box.boxId, 'cyan')}`);
    ctx.output.write(`  Status:        ${status}`);
    ctx.output.write(`  Value:         ${formatErg(box.value)}`);
    ctx.output.write(`  Creation:      Height ${box.creationHeight}`);
    ctx.output.write(`  Transaction:   ${box.transactionId}`);
    ctx.output.write(`  Index:         ${box.index}`);
    ctx.output.write(`  Explorer:      ${explorerLink(box.boxId, 'box')}`);
    ctx.output.write('');
    ctx.output.write(`  ErgoTree:      ${box.ergoTree.substring(0, 60)}...`);

    // Tokens
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize(`  Tokens (${box.tokens.length}):`, 'yellow'));
    if (box.tokens.length === 0) {
      ctx.output.write('    None');
    } else {
      for (const t of box.tokens) {
        const nameStr = t.name ? ` (${t.name})` : '';
        ctx.output.write(`    ${truncateId(t.tokenId)}: ${t.amount}${nameStr}`);
      }
    }

    // Registers R4-R9
    const regs = extractRegisters(box);
    const regKeys = Object.keys(regs);
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('  Registers:', 'yellow'));
    if (regKeys.length === 0) {
      ctx.output.write('    None');
    } else {
      for (const key of regKeys) {
        const val = regs[key];
        // Show hex for long values, decoded for short
        const display = val.length > 40 ? truncateId(val, 16) : val;
        ctx.output.write(`    ${key}: ${display}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get box: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: height ─────────────────────────────────────────────

async function handleHeight(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);

  try {
    let height: number;

    if (ctx.client?.chain?.getHeight) {
      height = await ctx.client.chain.getHeight();
    } else {
      const info = await fetchNode<Array<{ height?: number }>>(nodeUrl, '/blocks/lastHeaders/1');
      height = info?.[0]?.height ?? 0;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ height, network: args.options.network || 'mainnet', timestamp: new Date().toISOString() }));
      return;
    }

    const useColor = ctx.config.color;
    const network = String(args.options.network || 'mainnet');

    ctx.output.write(ctx.output.colorize('Blockchain Height', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(40), 'dim'));
    ctx.output.write(`  Network: ${ctx.output.colorize(network.toUpperCase(), 'cyan')}`);
    ctx.output.write(`  Height:  ${useColor ? '\x1b[1m' : ''}${height}${'\x1b[0m'}`);
    ctx.output.write(`  Time:    ${new Date().toISOString()}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get height: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: balance ────────────────────────────────────────────

async function handleBalance(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);
  const address = args.positional[1];

  if (!address) {
    ctx.output.writeError('Usage: xergon chain balance <address>');
    process.exit(1);
    return;
  }

  try {
    let balance: BalanceInfo;

    if (ctx.client?.chain?.getBalance) {
      balance = await ctx.client.chain.getBalance(address);
    } else {
      const data = await fetchNode<any>(nodeUrl, `/utils/balance/${address}`);
      balance = {
        address,
        nanoErgs: String(data.nanoErgs || 0),
        ergs: formatErg(String(data.nanoErgs || 0)),
        tokens: (data.tokens || []).map((t: any) => ({
          tokenId: t.tokenId,
          amount: String(t.amount || t.value || 0),
          name: t.name,
          decimals: t.decimals,
        })),
        boxesCount: data.numberOfBoxes,
      };
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(balance, null, 2));
      return;
    }

    const useColor = ctx.config.color;

    ctx.output.write(ctx.output.colorize('ERG Balance', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(40), 'dim'));
    ctx.output.write(`  Address: ${truncateId(address, 16)}`);
    ctx.output.write(`  Balance: ${useColor ? '\x1b[1m\x1b[32m' : ''}${balance.ergs}${'\x1b[0m'}`);
    ctx.output.write(`  Boxes:   ${balance.boxesCount ?? 'N/A'}`);
    ctx.output.write(`  Explorer: ${explorerLink(address, 'address')}`);

    if (balance.tokens.length > 0) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize(`  Tokens (${balance.tokens.length}):`, 'yellow'));
      for (const t of balance.tokens) {
        const nameStr = t.name ? ` (${t.name})` : '';
        ctx.output.write(`    ${truncateId(t.tokenId)}: ${t.amount}${nameStr}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get balance: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: tokens ─────────────────────────────────────────────

async function handleTokens(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);
  const address = args.positional[1];

  if (!address) {
    ctx.output.writeError('Usage: xergon chain tokens <address>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Fetching token holdings for ${truncateId(address, 12)}...`);

  try {
    let tokens: TokenHolding[];

    if (ctx.client?.chain?.getTokens) {
      tokens = await ctx.client.chain.getTokens(address);
    } else {
      // Get balance which includes tokens
      const data = await fetchNode<any>(nodeUrl, `/utils/balance/${address}`);
      tokens = (data.tokens || []).map((t: any) => ({
        tokenId: t.tokenId,
        amount: String(t.amount || t.value || 0),
        name: t.name,
        decimals: t.decimals,
      }));
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ address, totalTokens: tokens.length, tokens }, null, 2));
      return;
    }

    const useColor = ctx.config.color;

    ctx.output.write(ctx.output.colorize(`Token Holdings for ${truncateId(address, 12)}`, 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(55), 'dim'));
    ctx.output.write(`  Total tokens: ${tokens.length}`);
    ctx.output.write('');

    if (tokens.length === 0) {
      ctx.output.info('  No tokens found for this address.');
      return;
    }

    for (const t of tokens) {
      const nameStr = t.name || 'Unknown Token';
      const amountStr = t.decimals
        ? (Number(t.amount) / Math.pow(10, t.decimals)).toFixed(t.decimals)
        : t.amount;

      ctx.output.write(`  ${ctx.output.colorize(nameStr, 'yellow')} (${truncateId(t.tokenId)})`);
      ctx.output.write(`    Amount: ${amountStr}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to fetch tokens: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: providers ──────────────────────────────────────────

async function handleProviders(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);

  ctx.output.info('Scanning UTXO set for registered providers...');

  try {
    let providers: ProviderInfo[];

    if (ctx.client?.chain?.getProviders) {
      providers = await ctx.client.chain.getProviders();
    } else {
      // Scan UTXO set and filter for provider boxes
      const rawBoxes = await fetchNode<any[]>(nodeUrl, '/utxo/scan');
      const providerBoxes = (rawBoxes || []).filter((box: any) => {
        const regs = box.additionalRegisters || box.registers || {};
        const r4 = decodeRegisterValue(regs['R4']);
        return r4 === 'provider' || (box.ergoTree || '').includes('provider');
      });

      providers = providerBoxes.map((box: any) => ({
        boxId: box.boxId,
        address: box.address || 'unknown',
        stakeAmount: String(box.value || 0),
        stakeErg: formatErg(String(box.value || 0)),
        region: decodeRegisterValue((box.additionalRegisters || box.registers || {})['R5']) || 'unknown',
        model: decodeRegisterValue((box.additionalRegisters || box.registers || {})['R6']) || 'unknown',
        status: 'active' as const,
        registeredHeight: box.creationHeight || 0,
      }));
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ totalProviders: providers.length, providers }, null, 2));
      return;
    }

    const useColor = ctx.config.color;

    ctx.output.write(ctx.output.colorize('Xergon Providers', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(60), 'dim'));
    ctx.output.write(`  Registered providers: ${providers.length}`);
    ctx.output.write('');

    if (providers.length === 0) {
      ctx.output.info('  No registered providers found.');
      return;
    }

    for (const p of providers) {
      const statusColor = p.status === 'active'
        ? (useColor ? '\x1b[32m' : '')
        : p.status === 'slashed'
          ? (useColor ? '\x1b[31m' : '')
          : (useColor ? '\x1b[33m' : '');

      ctx.output.write(
        `  ${ctx.output.colorize(truncateId(p.boxId), 'cyan')}  ${p.stakeErg}  ${statusColor}${p.status.toUpperCase()}\x1b[0m`
      );
      ctx.output.write(`    Region: ${p.region}  |  Model: ${p.model}  |  Since: Height ${p.registeredHeight}`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to scan providers: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: stake ──────────────────────────────────────────────

async function handleStake(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);
  const address = args.positional[1];

  if (!address) {
    ctx.output.writeError('Usage: xergon chain stake <address>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Checking staking box for ${truncateId(address, 12)}...`);

  try {
    let stakeInfo: StakeInfo | null;

    if (ctx.client?.chain?.getStake) {
      stakeInfo = await ctx.client.chain.getStake(address);
    } else {
      // Look for staking boxes at this address
      const boxes = await fetchNode<any[]>(nodeUrl, `/utxo/byAddress/${address}`);
      const stakeBox = (boxes || []).find((box: any) => {
        const regs = box.additionalRegisters || box.registers || {};
        const r4 = decodeRegisterValue(regs['R4']);
        return r4 === 'staking' || (box.ergoTree || '').includes('staking');
      });

      if (stakeBox) {
        const regs = stakeBox.additionalRegisters || stakeBox.registers || {};
        stakeInfo = {
          boxId: stakeBox.boxId,
          stakedAmount: String(stakeBox.value || 0),
          stakedErg: formatErg(String(stakeBox.value || 0)),
          rewardAddress: decodeRegisterValue(regs['R5']) || address,
          lockEpochs: Number(decodeRegisterValue(regs['R6'])) || 0,
          currentEpoch: 0,
          status: 'locked' as const,
          registeredHeight: stakeBox.creationHeight || 0,
          tokens: (stakeBox.assets || []).map((a: any) => ({
            tokenId: a.tokenId,
            amount: String(a.amount || a.value || 0),
            name: a.name,
          })),
        };
      } else {
        stakeInfo = null;
      }
    }

    if (!stakeInfo) {
      ctx.output.info('  No staking box found for this address.');
      if (isJsonOutput(args)) {
        ctx.output.write(JSON.stringify({ address, staked: false }));
      }
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify({ address, staked: true, ...stakeInfo }, null, 2));
      return;
    }

    const useColor = ctx.config.color;
    const statusColor = stakeInfo.status === 'locked'
      ? (useColor ? '\x1b[33m' : '')
      : stakeInfo.status === 'withdrawable'
        ? (useColor ? '\x1b[32m' : '')
        : (useColor ? '\x1b[36m' : '');

    ctx.output.write(ctx.output.colorize('Staking Box', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(50), 'dim'));
    ctx.output.write(`  Box ID:        ${ctx.output.colorize(stakeInfo.boxId, 'cyan')}`);
    ctx.output.write(`  Status:        ${statusColor}${stakeInfo.status.toUpperCase()}\x1b[0m`);
    ctx.output.write(`  Staked:        ${useColor ? '\x1b[1m\x1b[32m' : ''}${stakeInfo.stakedErg}\x1b[0m`);
    ctx.output.write(`  Reward Addr:   ${truncateId(stakeInfo.rewardAddress, 16)}`);
    ctx.output.write(`  Lock Epochs:   ${stakeInfo.lockEpochs}`);
    ctx.output.write(`  Current Epoch: ${stakeInfo.currentEpoch}`);
    ctx.output.write(`  Registered:    Height ${stakeInfo.registeredHeight}`);
    ctx.output.write(`  Explorer:      ${explorerLink(stakeInfo.boxId, 'box')}`);

    if (stakeInfo.tokens.length > 0) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize('  Staking Tokens:', 'yellow'));
      for (const t of stakeInfo.tokens) {
        const nameStr = t.name ? ` (${t.name})` : '';
        ctx.output.write(`    ${truncateId(t.tokenId)}: ${t.amount}${nameStr}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get stake info: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: tx ─────────────────────────────────────────────────

async function handleTx(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const nodeUrl = getNodeUrl(args);
  const txId = args.positional[1];

  if (!txId) {
    ctx.output.writeError('Usage: xergon chain tx <tx-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Fetching transaction ${truncateId(txId)}...`);

  try {
    let tx: TxDetails;

    if (ctx.client?.chain?.getTx) {
      tx = await ctx.client.chain.getTx(txId);
    } else {
      const raw = await fetchNode<any>(nodeUrl, `/transactions/${txId}`);
      const inputs = (raw.inputs || []).map((inp: any) => ({
        boxId: inp.boxId,
        value: String(inp.value || 0),
        address: inp.address,
      }));
      const outputs = (raw.outputs || []).map((out: any) => ({
        boxId: out.boxId,
        value: String(out.value || 0),
        address: out.address,
        ergoTree: out.ergoTree,
      }));

      tx = {
        txId: raw.id || txId,
        timestamp: raw.timestamp || 0,
        height: raw.inclusionHeight || 0,
        size: raw.size || 0,
        inputsCount: inputs.length,
        outputsCount: outputs.length,
        inputs,
        outputs,
        fee: String(raw.fee || 0),
        feeErg: formatErg(String(raw.fee || 0)),
        status: raw.inclusionHeight ? 'confirmed' : 'pending',
      };
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(tx, null, 2));
      return;
    }

    const useColor = ctx.config.color;
    const statusColor = tx.status === 'confirmed'
      ? (useColor ? '\x1b[32m' : '')
      : tx.status === 'pending'
        ? (useColor ? '\x1b[33m' : '')
        : '';

    ctx.output.write(ctx.output.colorize('Transaction Details', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(55), 'dim'));
    ctx.output.write(`  TX ID:      ${ctx.output.colorize(tx.txId, 'cyan')}`);
    ctx.output.write(`  Status:     ${statusColor}${tx.status.toUpperCase()}\x1b[0m`);
    ctx.output.write(`  Block:      ${tx.height > 0 ? tx.height : 'pending'}`);
    ctx.output.write(`  Size:       ${tx.size} bytes`);
    ctx.output.write(`  Fee:        ${tx.feeErg}`);
    ctx.output.write(`  Inputs:     ${tx.inputsCount}`);
    ctx.output.write(`  Outputs:    ${tx.outputsCount}`);
    ctx.output.write(`  Explorer:   ${explorerLink(tx.txId, 'tx')}`);

    // Input boxes
    if (tx.inputs.length > 0) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize('  Inputs:', 'yellow'));
      for (const inp of tx.inputs) {
        const addr = inp.address ? ` (${truncateId(inp.address, 10)})` : '';
        ctx.output.write(`    ${truncateId(inp.boxId)}: ${formatErg(inp.value)}${addr}`);
      }
    }

    // Output boxes
    if (tx.outputs.length > 0) {
      ctx.output.write('');
      ctx.output.write(ctx.output.colorize('  Outputs:', 'yellow'));
      for (const out of tx.outputs) {
        const addr = out.address ? ` (${truncateId(out.address, 10)})` : '';
        ctx.output.write(`    ${truncateId(out.boxId)}: ${formatErg(out.value)}${addr}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get transaction: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

export async function chainAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon chain <boxes|box|height|balance|tokens|providers|stake|tx> [args]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  boxes <address>     List UTXO boxes for an address');
    ctx.output.write('  box <box-id>        Get box details by ID');
    ctx.output.write('  height              Current blockchain height');
    ctx.output.write('  balance <address>   ERG balance (X.XXX ERG)');
    ctx.output.write('  tokens <address>    Token holdings');
    ctx.output.write('  providers           List registered Xergon providers');
    ctx.output.write('  stake <address>     Show staking box info');
    ctx.output.write('  tx <tx-id>          Transaction details');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'boxes':
      await handleBoxes(args, ctx);
      break;
    case 'box':
      await handleBox(args, ctx);
      break;
    case 'height':
      await handleHeight(args, ctx);
      break;
    case 'balance':
      await handleBalance(args, ctx);
      break;
    case 'tokens':
      await handleTokens(args, ctx);
      break;
    case 'providers':
      await handleProviders(args, ctx);
      break;
    case 'stake':
      await handleStake(args, ctx);
      break;
    case 'tx':
      await handleTx(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: boxes, box, height, balance, tokens, providers, stake, tx');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const chainOptions: CommandOption[] = [
  {
    name: 'node',
    short: '',
    long: '--node',
    description: 'Ergo node URL (default: https://node.ergo.network)',
    required: false,
    type: 'string',
  },
  {
    name: 'network',
    short: '',
    long: '--network',
    description: 'Network: testnet or mainnet (default: mainnet)',
    required: false,
    default: 'mainnet',
    type: 'string',
  },
  {
    name: 'json',
    short: '-j',
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
];

// ── Command export ─────────────────────────────────────────────────

export const chainCommand: Command = {
  name: 'chain',
  description: 'On-chain state inspection: boxes, balances, transactions, providers, staking',
  aliases: ['onchain', 'utxo'],
  options: chainOptions,
  action: chainAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  nanoErgToErg as _nanoErgToErg,
  formatErg as _formatErg,
  truncateId as _truncateId,
  boxStatusColor as _boxStatusColor,
  decodeRegisterValue as _decodeRegisterValue,
  extractRegisters as _extractRegisters,
  minBoxValue as _minBoxValue,
  explorerLink as _explorerLink,
  handleBoxes as _handleBoxes,
  handleBox as _handleBox,
  handleHeight as _handleHeight,
  handleBalance as _handleBalance,
  handleTokens as _handleTokens,
  handleProviders as _handleProviders,
  handleStake as _handleStake,
  handleTx as _handleTx,
  NANO_ERG_PER_ERG,
  MIN_BOX_VALUE_PER_BYTE,
  DEFAULT_NODES,
  EXPLORER_URL,
  mapBoxToChainBox,
  type ChainBox,
  type AddressBoxes,
  type BalanceInfo,
  type TokenHolding,
  type ProviderInfo,
  type StakeInfo,
  type TxDetails,
};
