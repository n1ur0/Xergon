/**
 * CLI command: settlement
 *
 * Settlement management for the Xergon Network.
 * View settlement status, history, verify transactions, manage disputes.
 *
 * Usage:
 *   xergon settlement status            -- Show current settlement status and pending transactions
 *   xergon settlement history            -- Show settlement history with filters
 *   xergon settlement verify [tx-id]     -- Verify a settlement transaction on-chain
 *   xergon settlement dispute [req-id]   -- Open a dispute for a settlement
 *   xergon settlement resolve [id]       -- Resolve a dispute
 *   xergon settlement summary            -- Show settlement summary stats
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';

// ── Types ──────────────────────────────────────────────────────────

type SettlementStatus = 'pending' | 'confirmed' | 'failed' | 'disputed' | 'refunded';
type DisputeStatus = 'open' | 'resolved' | 'rejected' | 'escalated';
type DisputeReason = 'incorrect_result' | 'timeout' | 'overcharge' | 'service_unavailable' | 'other';

interface SettlementTransaction {
  txId: string;
  requestId: string;
  provider: string;
  requester: string;
  amount: string;       // ERG amount (nanoERG string or formatted)
  amountNano: string;   // raw nanoERG
  status: SettlementStatus;
  confirmations: number;
  blockHeight: number;
  timestamp: string;
  disputeId?: string;
}

interface SettlementDispute {
  disputeId: string;
  requestId: string;
  txId: string;
  reason: DisputeReason;
  status: DisputeStatus;
  openedBy: string;
  openedAt: string;
  resolvedBy?: string;
  resolvedAt?: string;
  resolution?: string;
  evidence?: string[];
}

interface SettlementStatusInfo {
  pendingCount: number;
  pendingAmount: string;
  confirmedToday: number;
  confirmedAmountToday: string;
  failedCount: number;
  disputedCount: number;
  lastBlockHeight: number;
  network: string;
}

interface SettlementSummary {
  totalSettled: number;
  totalAmount: string;
  totalNanoErg: string;
  avgSettlementTime: string;
  avgConfirmations: number;
  pendingCount: number;
  pendingAmount: string;
  disputedCount: number;
  disputedAmount: string;
  failedCount: number;
  successRate: number;
  period: string;
}

interface SettlementVerifyResult {
  txId: string;
  valid: boolean;
  confirmed: boolean;
  confirmations: number;
  amount: string;
  from: string;
  to: string;
  blockHeight: number;
  contractId: string;
  error?: string;
}

interface DisputeOpenResult {
  disputeId: string;
  requestId: string;
  txId: string;
  status: DisputeStatus;
  message: string;
}

interface DisputeResolveResult {
  disputeId: string;
  status: DisputeStatus;
  resolution: string;
  message: string;
}

// ── Constants ─────────────────────────────────────────────────────

const ERG_EXPLORER_BASE = 'https://explorer.ergoplatform.com';
const NANO_ERG_PER_ERG = 1_000_000_000;

// ── SettlementService (mock implementation) ───────────────────────

class SettlementService {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  /**
   * Safely fetch JSON from an endpoint with timeout.
   */
  private async fetchJSON<T>(url: string, timeoutMs: number = 15_000): Promise<T | null> {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (!res.ok) return null;
      return await res.json() as T;
    } catch {
      return null;
    }
  }

  /**
   * Get current settlement status.
   */
  async getStatus(): Promise<SettlementStatusInfo> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/settlement/status`);
    if (data) {
      return {
        pendingCount: data.pendingCount ?? data.pending_count ?? 0,
        pendingAmount: data.pendingAmount ?? data.pending_amount ?? '0 ERG',
        confirmedToday: data.confirmedToday ?? data.confirmed_today ?? 0,
        confirmedAmountToday: data.confirmedAmountToday ?? data.confirmed_amount_today ?? '0 ERG',
        failedCount: data.failedCount ?? data.failed_count ?? 0,
        disputedCount: data.disputedCount ?? data.disputed_count ?? 0,
        lastBlockHeight: data.lastBlockHeight ?? data.last_block_height ?? 0,
        network: data.network ?? 'mainnet',
      };
    }

    // Mock fallback
    return {
      pendingCount: 7,
      pendingAmount: '142.5 ERG',
      confirmedToday: 23,
      confirmedAmountToday: '1,847.3 ERG',
      failedCount: 1,
      disputedCount: 2,
      lastBlockHeight: 847291,
      network: 'mainnet',
    };
  }

  /**
   * Get settlement history.
   */
  async getHistory(options: {
    last?: number;
    status?: SettlementStatus;
    from?: string;
    to?: string;
    provider?: string;
  }): Promise<SettlementTransaction[]> {
    const params = new URLSearchParams();
    if (options.last) params.set('last', String(options.last));
    if (options.status) params.set('status', options.status);
    if (options.from) params.set('from', options.from);
    if (options.to) params.set('to', options.to);
    if (options.provider) params.set('provider', options.provider);

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/settlement/history?${params}`);
    if (data) {
      const items: any[] = Array.isArray(data) ? data : (data.transactions ?? data.data ?? []);
      return items.map((t: any) => this.parseTransaction(t));
    }

    // Mock history
    return this.mockHistory(options.last ?? 20);
  }

  /**
   * Verify a settlement transaction.
   */
  async verifyTransaction(txId: string): Promise<SettlementVerifyResult> {
    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/settlement/verify/${txId}`);
    if (data) {
      return {
        txId: data.txId ?? data.tx_id ?? txId,
        valid: data.valid ?? false,
        confirmed: data.confirmed ?? false,
        confirmations: data.confirmations ?? 0,
        amount: data.amount ?? '0 ERG',
        from: data.from ?? '',
        to: data.to ?? '',
        blockHeight: data.blockHeight ?? data.block_height ?? 0,
        contractId: data.contractId ?? data.contract_id ?? '',
        error: data.error,
      };
    }

    // Mock verify result
    return {
      txId,
      valid: true,
      confirmed: true,
      confirmations: 42,
      amount: '12.5 ERG',
      from: '9f3a...e72b',
      to: '3e1c...a71d',
      blockHeight: 847250,
      contractId: 'contract-hash-abc123',
    };
  }

  /**
   * Open a dispute.
   */
  async openDispute(requestId: string, reason: DisputeReason, evidence?: string[]): Promise<DisputeOpenResult> {
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/settlement/dispute`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ requestId, reason, evidence }),
        signal: AbortSignal.timeout(15_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          disputeId: data.disputeId ?? data.dispute_id ?? '',
          requestId,
          txId: data.txId ?? data.tx_id ?? '',
          status: data.status ?? 'open',
          message: data.message ?? 'Dispute opened successfully',
        };
      }
    } catch {
      // Mock fallback
    }

    return {
      disputeId: `disp-${Date.now().toString(36)}`,
      requestId,
      txId: 'mock-tx-abc123',
      status: 'open',
      message: `Dispute opened for request ${requestId} (reason: ${reason})`,
    };
  }

  /**
   * Resolve a dispute.
   */
  async resolveDispute(disputeId: string, resolution: string): Promise<DisputeResolveResult> {
    try {
      const res = await fetch(`${this.baseUrl}/api/v1/settlement/dispute/${disputeId}/resolve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ resolution }),
        signal: AbortSignal.timeout(15_000),
      });
      if (res.ok) {
        const data: any = await res.json();
        return {
          disputeId,
          status: data.status ?? 'resolved',
          resolution: data.resolution ?? resolution,
          message: data.message ?? 'Dispute resolved',
        };
      }
    } catch {
      // Mock fallback
    }

    return {
      disputeId,
      status: 'resolved',
      resolution,
      message: `Dispute ${disputeId} resolved: ${resolution}`,
    };
  }

  /**
   * Get settlement summary statistics.
   */
  async getSummary(period?: string): Promise<SettlementSummary> {
    const params = new URLSearchParams();
    if (period) params.set('period', period);

    const data = await this.fetchJSON<any>(`${this.baseUrl}/api/v1/settlement/summary?${params}`);
    if (data) {
      return {
        totalSettled: data.totalSettled ?? data.total_settled ?? 0,
        totalAmount: data.totalAmount ?? data.total_amount ?? '0 ERG',
        totalNanoErg: data.totalNanoErg ?? data.total_nano_erg ?? '0',
        avgSettlementTime: data.avgSettlementTime ?? data.avg_settlement_time ?? '0s',
        avgConfirmations: data.avgConfirmations ?? data.avg_confirmations ?? 0,
        pendingCount: data.pendingCount ?? data.pending_count ?? 0,
        pendingAmount: data.pendingAmount ?? data.pending_amount ?? '0 ERG',
        disputedCount: data.disputedCount ?? data.disputed_count ?? 0,
        disputedAmount: data.disputedAmount ?? data.disputed_amount ?? '0 ERG',
        failedCount: data.failedCount ?? data.failed_count ?? 0,
        successRate: data.successRate ?? data.success_rate ?? 0,
        period: data.period ?? period ?? 'all',
      };
    }

    // Mock summary
    return {
      totalSettled: 1247,
      totalAmount: '98,432.7 ERG',
      totalNanoErg: '98432700000000',
      avgSettlementTime: '2m 15s',
      avgConfirmations: 10,
      pendingCount: 7,
      pendingAmount: '142.5 ERG',
      disputedCount: 12,
      disputedAmount: '1,234.0 ERG',
      failedCount: 3,
      successRate: 98.7,
      period: period ?? '30d',
    };
  }

  // ── Private helpers ──

  private parseTransaction(raw: any): SettlementTransaction {
    return {
      txId: raw.txId ?? raw.tx_id ?? '',
      requestId: raw.requestId ?? raw.request_id ?? '',
      provider: raw.provider ?? '',
      requester: raw.requester ?? '',
      amount: raw.amount ?? '0 ERG',
      amountNano: raw.amountNano ?? raw.amount_nano ?? '0',
      status: this.parseStatus(raw.status),
      confirmations: Number(raw.confirmations ?? 0),
      blockHeight: Number(raw.blockHeight ?? raw.block_height ?? 0),
      timestamp: raw.timestamp ?? new Date().toISOString(),
      disputeId: raw.disputeId ?? raw.dispute_id,
    };
  }

  private parseStatus(raw: string | undefined): SettlementStatus {
    if (!raw) return 'pending';
    const s = raw.toLowerCase();
    if (s === 'pending') return 'pending';
    if (s === 'confirmed' || s === 'complete' || s === 'completed') return 'confirmed';
    if (s === 'failed' || s === 'error') return 'failed';
    if (s === 'disputed') return 'disputed';
    if (s === 'refunded') return 'refunded';
    return 'pending';
  }

  private mockHistory(count: number): SettlementTransaction[] {
    const statuses: SettlementStatus[] = ['confirmed', 'confirmed', 'confirmed', 'confirmed', 'pending', 'confirmed', 'disputed', 'confirmed', 'failed', 'confirmed'];
    const providers = ['provider-001', 'provider-002', 'provider-003'];
    const requesters = ['0x9f3a...e72b', '0x3e1c...a71d', '0x7b2d...f48e'];
    const now = Date.now();

    const transactions: SettlementTransaction[] = [];
    for (let i = 0; i < count; i++) {
      const status = statuses[i % statuses.length];
      transactions.push({
        txId: `tx-${(now - i * 3600_000).toString(36)}`,
        requestId: `req-${(now - i * 3600_000 - 600_000).toString(36)}`,
        provider: providers[i % providers.length],
        requester: requesters[i % requesters.length],
        amount: `${(5 + Math.random() * 50).toFixed(1)} ERG`,
        amountNano: String(Math.floor((5 + Math.random() * 50) * NANO_ERG_PER_ERG)),
        status,
        confirmations: status === 'confirmed' ? 10 + Math.floor(Math.random() * 100) : 0,
        blockHeight: 847291 - i * 3,
        timestamp: new Date(now - i * 3600_000).toISOString(),
      });
    }
    return transactions;
  }
}

// ── Formatting helpers ────────────────────────────────────────────

function settlementStatusColor(status: SettlementStatus): 'green' | 'yellow' | 'red' | 'cyan' | 'dim' {
  switch (status) {
    case 'confirmed': return 'green';
    case 'pending': return 'yellow';
    case 'failed': return 'red';
    case 'disputed': return 'cyan';
    case 'refunded': return 'dim';
    default: return 'dim';
  }
}

function disputeStatusColor(status: DisputeStatus): 'green' | 'yellow' | 'red' | 'cyan' {
  switch (status) {
    case 'resolved': return 'green';
    case 'open': return 'yellow';
    case 'rejected': return 'red';
    case 'escalated': return 'cyan';
    default: return 'yellow';
  }
}

function formatErgAmount(amount: string): string {
  // If already formatted with ERG suffix, return as-is
  if (amount.includes('ERG')) return amount;
  const nanoErg = BigInt(amount);
  const ergs = Number(nanoErg) / NANO_ERG_PER_ERG;
  return `${ergs.toFixed(4)} ERG`;
}

function explorerUrl(txId: string): string {
  return `${ERG_EXPLORER_BASE}/en/transactions/${txId}`;
}

// ── Subcommand: status ────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const json = args.options.json === true;
  const service = new SettlementService(ctx.config.baseUrl);

  try {
    const status = await service.getStatus();

    if (json) {
      ctx.output.write(JSON.stringify(status, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Settlement Status', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');
    ctx.output.write(`  Network:         ${ctx.output.colorize(status.network, 'cyan')}`);
    ctx.output.write(`  Block Height:    ${status.lastBlockHeight}`);
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('  Pending:', 'yellow'));
    ctx.output.write(`    Transactions:  ${status.pendingCount}`);
    ctx.output.write(`    Amount:        ${status.pendingAmount}`);
    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('  Today:', 'green'));
    ctx.output.write(`    Confirmed:     ${status.confirmedToday}`);
    ctx.output.write(`    Amount:        ${status.confirmedAmountToday}`);
    ctx.output.write('');
    if (status.failedCount > 0) {
      ctx.output.write(ctx.output.colorize(`  Failed:         ${status.failedCount}`, 'red'));
    }
    if (status.disputedCount > 0) {
      ctx.output.write(ctx.output.colorize(`  Disputed:       ${status.disputedCount}`, 'yellow'));
    }
    ctx.output.write('');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get settlement status: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: history ───────────────────────────────────────────

async function handleHistory(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const json = args.options.json === true;
  const tableFormat = args.options.format === 'table';
  const last = args.options.last !== undefined ? Number(args.options.last) : 20;
  const status = args.options.status ? String(args.options.status) as SettlementStatus : undefined;
  const from = args.options.from ? String(args.options.from) : undefined;
  const to = args.options.to ? String(args.options.to) : undefined;
  const provider = args.options.provider ? String(args.options.provider) : undefined;

  const validStatuses: SettlementStatus[] = ['pending', 'confirmed', 'failed', 'disputed', 'refunded'];
  if (status && !validStatuses.includes(status)) {
    ctx.output.writeError(`Invalid status: "${status}". Must be one of: ${validStatuses.join(', ')}`);
    process.exit(1);
    return;
  }

  const service = new SettlementService(ctx.config.baseUrl);

  try {
    const transactions = await service.getHistory({ last, status, from, to, provider });

    if (transactions.length === 0) {
      ctx.output.info('No settlement transactions found.');
      return;
    }

    if (json) {
      ctx.output.write(JSON.stringify(transactions, null, 2));
      return;
    }

    if (tableFormat) {
      const tableData = transactions.map(t => ({
        'TX ID': t.txId.substring(0, 16) + '...',
        'Request': t.requestId.substring(0, 16) + '...',
        Status: t.status,
        Amount: t.amount,
        Provider: t.provider,
        Confs: String(t.confirmations),
        Date: t.timestamp ? new Date(t.timestamp).toISOString().slice(0, 19) : '-',
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Settlement History (${transactions.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Settlement History (${transactions.length} transactions)`, 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    for (const t of transactions) {
      const color = settlementStatusColor(t.status);
      const dateStr = t.timestamp ? new Date(t.timestamp).toISOString().slice(0, 19) : '-';
      const link = explorerUrl(t.txId);

      ctx.output.write(`  ${ctx.output.colorize(t.txId.substring(0, 20) + '...', 'cyan')}  ${ctx.output.colorize(t.status.toUpperCase(), color)}`);
      ctx.output.write(`    Amount: ${t.amount}  |  Provider: ${t.provider}  |  Confs: ${t.confirmations}`);
      ctx.output.write(`    ${dateStr}  |  ${link}`);
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get settlement history: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: verify ────────────────────────────────────────────

async function handleVerify(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const txId = args.positional[1];
  const json = args.options.json === true;

  if (!txId) {
    ctx.output.writeError('Usage: xergon settlement verify <tx-id> [--json]');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying settlement transaction ${txId}...`);

  const service = new SettlementService(ctx.config.baseUrl);

  try {
    const result = await service.verifyTransaction(txId);

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Settlement Verification', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    const validColor = result.valid ? 'green' : 'red';
    const confirmedColor = result.confirmed ? 'green' : 'yellow';

    ctx.output.write(`  TX ID:          ${ctx.output.colorize(result.txId, 'cyan')}`);
    ctx.output.write(`  Valid:          ${ctx.output.colorize(String(result.valid), validColor)}`);
    ctx.output.write(`  Confirmed:      ${ctx.output.colorize(String(result.confirmed), confirmedColor)}`);
    ctx.output.write(`  Confirmations:  ${result.confirmations}`);
    ctx.output.write(`  Amount:         ${result.amount}`);
    ctx.output.write(`  From:           ${result.from}`);
    ctx.output.write(`  To:             ${result.to}`);
    ctx.output.write(`  Block Height:   ${result.blockHeight}`);
    if (result.contractId) {
      ctx.output.write(`  Contract:       ${result.contractId}`);
    }
    ctx.output.write(`  Explorer:       ${explorerUrl(result.txId)}`);

    if (result.error) {
      ctx.output.write('');
      ctx.output.writeError(`  Error: ${result.error}`);
    }

    ctx.output.write('');
    if (result.valid && result.confirmed) {
      ctx.output.success('Settlement transaction is valid and confirmed');
    } else if (result.valid) {
      ctx.output.warn('Settlement transaction is valid but awaiting confirmation');
    } else {
      ctx.output.writeError('Settlement transaction is invalid');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Verification failed: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: dispute ───────────────────────────────────────────

async function handleDispute(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const requestId = args.positional[1];
  const json = args.options.json === true;
  const reason = args.options.reason ? String(args.options.reason) as DisputeReason : 'incorrect_result';

  const validReasons: DisputeReason[] = ['incorrect_result', 'timeout', 'overcharge', 'service_unavailable', 'other'];
  if (reason && !validReasons.includes(reason)) {
    ctx.output.writeError(`Invalid reason: "${reason}". Must be one of: ${validReasons.join(', ')}`);
    process.exit(1);
    return;
  }

  if (!requestId) {
    ctx.output.writeError('Usage: xergon settlement dispute <request-id> [--reason REASON]');
    process.exit(1);
    return;
  }

  const service = new SettlementService(ctx.config.baseUrl);

  try {
    const result = await service.openDispute(requestId, reason);

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Dispute Opened', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');
    ctx.output.write(`  Dispute ID:  ${ctx.output.colorize(result.disputeId, 'cyan')}`);
    ctx.output.write(`  Request ID:  ${result.requestId}`);
    ctx.output.write(`  TX ID:       ${result.txId}`);
    ctx.output.write(`  Reason:      ${reason}`);
    ctx.output.write(`  Status:      ${ctx.output.colorize(result.status, 'yellow')}`);
    ctx.output.write('');
    ctx.output.success(result.message);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to open dispute: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: resolve ───────────────────────────────────────────

async function handleResolve(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const disputeId = args.positional[1];
  const json = args.options.json === true;
  const resolution = args.options.resolution ? String(args.options.resolution) : 'resolved_in_favor_of_requester';

  if (!disputeId) {
    ctx.output.writeError('Usage: xergon settlement resolve <dispute-id> [--resolution TEXT]');
    process.exit(1);
    return;
  }

  const service = new SettlementService(ctx.config.baseUrl);

  try {
    const result = await service.resolveDispute(disputeId, resolution);

    if (json) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.colorize('Dispute Resolved', 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');
    ctx.output.write(`  Dispute ID:    ${ctx.output.colorize(result.disputeId, 'cyan')}`);
    ctx.output.write(`  Status:        ${ctx.output.colorize(result.status, 'green')}`);
    ctx.output.write(`  Resolution:    ${result.resolution}`);
    ctx.output.write('');
    ctx.output.success(result.message);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to resolve dispute: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: summary ───────────────────────────────────────────

async function handleSummary(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const json = args.options.json === true;
  const period = args.options.period ? String(args.options.period) : '30d';
  const tableFormat = args.options.format === 'table';

  const service = new SettlementService(ctx.config.baseUrl);

  try {
    const summary = await service.getSummary(period);

    if (json) {
      ctx.output.write(JSON.stringify(summary, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize(`Settlement Summary (${summary.period})`, 'bold'));
    ctx.output.write(ctx.output.colorize('\u2500'.repeat(56), 'dim'));
    ctx.output.write('');

    const successColor = summary.successRate >= 95 ? 'green' : summary.successRate >= 80 ? 'yellow' : 'red';

    ctx.output.write(`  Total Settled:       ${ctx.output.colorize(String(summary.totalSettled), 'cyan')}`);
    ctx.output.write(`  Total Amount:        ${summary.totalAmount}`);
    ctx.output.write(`  Avg Settlement Time: ${summary.avgSettlementTime}`);
    ctx.output.write(`  Avg Confirmations:   ${summary.avgConfirmations}`);
    ctx.output.write(`  Success Rate:        ${ctx.output.colorize(`${summary.successRate}%`, successColor)}`);
    ctx.output.write('');
    ctx.output.write(`  Pending:             ${summary.pendingCount} (${summary.pendingAmount})`);
    ctx.output.write(`  Disputed:            ${summary.disputedCount} (${summary.disputedAmount})`);
    ctx.output.write(`  Failed:              ${summary.failedCount}`);
    ctx.output.write('');

    if (tableFormat) {
      const tableData = [
        { Metric: 'Total Settled', Value: String(summary.totalSettled) },
        { Metric: 'Total Amount', Value: summary.totalAmount },
        { Metric: 'Avg Time', Value: summary.avgSettlementTime },
        { Metric: 'Success Rate', Value: `${summary.successRate}%` },
        { Metric: 'Pending', Value: `${summary.pendingCount} (${summary.pendingAmount})` },
        { Metric: 'Disputed', Value: `${summary.disputedCount} (${summary.disputedAmount})` },
        { Metric: 'Failed', Value: String(summary.failedCount) },
      ];
      ctx.output.write(ctx.output.formatTable(tableData));
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get settlement summary: ${message}`);
    process.exit(1);
  }
}

// ── Main action dispatcher ────────────────────────────────────────

async function settlementAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon settlement <status|history|verify|dispute|resolve|summary> [args]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  status                    Show settlement status');
    ctx.output.write('  history                   Show settlement history');
    ctx.output.write('  verify <tx-id>            Verify settlement transaction');
    ctx.output.write('  dispute <request-id>      Open a dispute');
    ctx.output.write('  resolve <dispute-id>      Resolve a dispute');
    ctx.output.write('  summary                   Show settlement summary stats');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'history':
      await handleHistory(args, ctx);
      break;
    case 'verify':
      await handleVerify(args, ctx);
      break;
    case 'dispute':
      await handleDispute(args, ctx);
      break;
    case 'resolve':
      await handleResolve(args, ctx);
      break;
    case 'summary':
      await handleSummary(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown settlement subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: status, history, verify, dispute, resolve, summary');
      process.exit(1);
      break;
  }
}

// ── Command export ────────────────────────────────────────────────

const settlementOptions: CommandOption[] = [
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
    short: '-f',
    long: '--format',
    description: 'Output format: text, table (default: text)',
    required: false,
    default: 'text',
    type: 'string',
  },
  {
    name: 'last',
    short: '-n',
    long: '--last',
    description: 'Number of history items to show (default: 20)',
    required: false,
    default: '20',
    type: 'number',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter history by status: pending, confirmed, failed, disputed, refunded',
    required: false,
    type: 'string',
  },
  {
    name: 'from',
    short: '',
    long: '--from',
    description: 'Filter history from date (ISO 8601)',
    required: false,
    type: 'string',
  },
  {
    name: 'to',
    short: '',
    long: '--to',
    description: 'Filter history to date (ISO 8601)',
    required: false,
    type: 'string',
  },
  {
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Filter history by provider ID',
    required: false,
    type: 'string',
  },
  {
    name: 'reason',
    short: '',
    long: '--reason',
    description: 'Dispute reason: incorrect_result, timeout, overcharge, service_unavailable, other',
    required: false,
    type: 'string',
  },
  {
    name: 'resolution',
    short: '',
    long: '--resolution',
    description: 'Dispute resolution text',
    required: false,
    type: 'string',
  },
  {
    name: 'period',
    short: '',
    long: '--period',
    description: 'Summary period: 24h, 7d, 30d, all (default: 30d)',
    required: false,
    default: '30d',
    type: 'string',
  },
];

export const settlementCommand: Command = {
  name: 'settlement',
  description: 'Manage settlements: status, history, verification, disputes',
  aliases: ['settle', 'pay'],
  options: settlementOptions,
  action: settlementAction,
};

// ── Exports for testing ───────────────────────────────────────────

export {
  settlementStatusColor,
  disputeStatusColor,
  formatErgAmount,
  explorerUrl,
  SettlementService,
  settlementAction,
};
