/**
 * CLI command: proof
 *
 * ZKP (Zero-Knowledge Proof) verification management for the Xergon Network.
 * Verify, submit, list, and anchor proofs, plus inspect provider trust scores.
 *
 * Usage:
 *   xergon proof verify --proof FILE --commitment HASH
 *   xergon proof verify --batch FILE
 *   xergon proof submit --provider ID --proof FILE
 *   xergon proof list --provider ID --status all|pending|verified|rejected
 *   xergon proof status --proof-id ID
 *   xergon proof anchor --proof-id ID
 *   xergon proof trust --provider ID
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ── Types ──────────────────────────────────────────────────────────

type ProofStatus = 'pending' | 'verified' | 'rejected';

interface ProofRecord {
  id: string;
  providerId: string;
  commitment: string;
  status: ProofStatus;
  createdAt: string;
  verifiedAt?: string;
  anchorTxId?: string;
}

interface ProofVerifyResult {
  valid: boolean;
  proofId?: string;
  commitment?: string;
  message: string;
  verifiedAt: string;
}

interface BatchVerifyResult {
  total: number;
  passed: number;
  failed: number;
  results: Array<{
    commitment: string;
    valid: boolean;
    message: string;
  }>;
}

interface ProofSubmitResult {
  proofId: string;
  providerId: string;
  status: 'submitted';
  submittedAt: string;
}

interface AnchorResult {
  proofId: string;
  txId: string;
  blockHeight: number;
  status: 'anchored';
  anchoredAt: string;
}

interface TrustScoreBreakdown {
  providerId: string;
  overallScore: number;
  tee: number;
  zk: number;
  uptime: number;
  ponw: number;
  reviews: number;
  lastUpdated: string;
}

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function statusBadge(status: ProofStatus): string {
  switch (status) {
    case 'verified':
      return 'VERIFIED';
    case 'pending':
      return 'PENDING';
    case 'rejected':
      return 'REJECTED';
    default: {
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

function statusColor(status: ProofStatus): 'green' | 'yellow' | 'red' {
  switch (status) {
    case 'verified': return 'green';
    case 'pending': return 'yellow';
    case 'rejected': return 'red';
  }
}

function formatTimestamp(iso: string | undefined): string {
  if (!iso) return '-';
  return new Date(iso).toISOString().slice(0, 19).replace('T', ' ');
}

function renderTrustBar(score: number, width: number = 30): string {
  const pct = Math.min(Math.max(score, 0), 100);
  const filled = Math.round((pct / 100) * width);
  const empty = width - filled;
  return '[' + '█'.repeat(filled) + '░'.repeat(empty) + '] ' + pct.toFixed(1);
}

function trustScoreColor(score: number): 'green' | 'yellow' | 'red' {
  if (score >= 80) return 'green';
  if (score >= 50) return 'yellow';
  return 'red';
}

// ── Subcommand: verify ─────────────────────────────────────────────

async function handleVerify(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proofFile = args.options.proof ? String(args.options.proof) : undefined;
  const commitment = args.options.commitment ? String(args.options.commitment) : undefined;
  const batchFile = args.options.batch ? String(args.options.batch) : undefined;

  // Batch mode
  if (batchFile) {
    await handleBatchVerify(args, ctx, batchFile);
    return;
  }

  // Single proof mode
  if (!proofFile || !commitment) {
    ctx.output.writeError('Usage: xergon proof verify --proof <file> --commitment <hash>');
    ctx.output.writeError('       xergon proof verify --batch <file>');
    process.exit(1);
    return;
  }

  const resolvedPath = path.resolve(proofFile);
  if (!fs.existsSync(resolvedPath)) {
    ctx.output.writeError(`Proof file not found: ${resolvedPath}`);
    process.exit(1);
    return;
  }

  const proofData = fs.readFileSync(resolvedPath);
  ctx.output.info(`Verifying ZK proof against commitment ${commitment.substring(0, 16)}...`);

  try {
    let result: ProofVerifyResult;

    if (ctx.client?.proof?.verify) {
      result = await ctx.client.proof.verify({
        proof: proofData,
        commitment,
      });
    } else {
      throw new Error('Proof client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    if (result.valid) {
      ctx.output.success('Proof verified successfully');
      ctx.output.write('');
      ctx.output.write(ctx.output.formatText({
        'Proof ID': result.proofId || '-',
        'Commitment': commitment,
        Status: ctx.output.colorize('VALID', 'green'),
        Message: result.message,
        'Verified At': formatTimestamp(result.verifiedAt),
      }, 'ZK Proof Verification'));
    } else {
      ctx.output.writeError('Proof verification failed');
      ctx.output.write('');
      ctx.output.write(ctx.output.formatText({
        'Commitment': commitment,
        Status: ctx.output.colorize('INVALID', 'red'),
        Message: result.message,
      }, 'ZK Proof Verification'));
      process.exit(1);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify proof: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: batch verify ──────────────────────────────────────

async function handleBatchVerify(args: ParsedArgs, ctx: CLIContext, batchFile: string): Promise<void> {
  const resolvedPath = path.resolve(batchFile);
  if (!fs.existsSync(resolvedPath)) {
    ctx.output.writeError(`Batch file not found: ${resolvedPath}`);
    process.exit(1);
    return;
  }

  let batchData: Array<{ proof: string; commitment: string }>;
  try {
    const raw = fs.readFileSync(resolvedPath, 'utf-8');
    batchData = JSON.parse(raw);
    if (!Array.isArray(batchData)) {
      throw new Error('Batch file must contain a JSON array');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to parse batch file: ${message}`);
    process.exit(1);
    return;
  }

  ctx.output.info(`Batch verifying ${batchData.length} proof(s)...`);

  try {
    let result: BatchVerifyResult;

    if (ctx.client?.proof?.batchVerify) {
      result = await ctx.client.proof.batchVerify({ proofs: batchData });
    } else {
      throw new Error('Proof client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Batch Verification Results', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(30), 'dim'));
    ctx.output.write(ctx.output.formatText({
      Total: String(result.total),
      Passed: ctx.output.colorize(String(result.passed), 'green'),
      Failed: ctx.output.colorize(String(result.failed), result.failed > 0 ? 'red' : 'green'),
    }));

    if (result.results.length > 0) {
      if (isTableFormat(args)) {
        const tableData = result.results.map(r => ({
          Commitment: r.commitment.substring(0, 16) + '...',
          Valid: r.valid ? 'YES' : 'NO',
          Message: r.message.length > 50 ? r.message.substring(0, 50) + '...' : r.message,
        }));
        ctx.output.write(ctx.output.formatTable(tableData));
      } else {
        ctx.output.write('');
        for (const r of result.results) {
          const icon = r.valid ? '✓' : '✗';
          const color = r.valid ? 'green' : 'red';
          ctx.output.write(
            `  ${ctx.output.colorize(icon, color)} ${r.commitment.substring(0, 16)}...  ${r.message}`
          );
        }
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to batch verify proofs: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: submit ────────────────────────────────────────────

async function handleSubmit(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const proofFile = args.options.proof ? String(args.options.proof) : undefined;

  if (!providerId || !proofFile) {
    ctx.output.writeError('Usage: xergon proof submit --provider <id> --proof <file>');
    process.exit(1);
    return;
  }

  const resolvedPath = path.resolve(proofFile);
  if (!fs.existsSync(resolvedPath)) {
    ctx.output.writeError(`Proof file not found: ${resolvedPath}`);
    process.exit(1);
    return;
  }

  const proofData = fs.readFileSync(resolvedPath);
  ctx.output.info(`Submitting proof for provider ${providerId.substring(0, 16)}... (${proofData.length} bytes)`);

  try {
    let result: ProofSubmitResult;

    if (ctx.client?.proof?.submit) {
      result = await ctx.client.proof.submit({
        providerId,
        proof: proofData,
      });
    } else {
      throw new Error('Proof client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Proof submitted successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Proof ID': result.proofId,
      'Provider ID': providerId,
      Status: ctx.output.colorize(result.status.toUpperCase(), 'green'),
      'Submitted At': formatTimestamp(result.submittedAt),
    }, 'Proof Submission'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to submit proof: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: list ──────────────────────────────────────────────

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const statusFilter = args.options.status ? String(args.options.status) : 'all';

  if (!providerId) {
    ctx.output.writeError('Usage: xergon proof list --provider <id> --status all|pending|verified|rejected');
    process.exit(1);
    return;
  }

  try {
    let proofs: ProofRecord[];

    if (ctx.client?.proof?.list) {
      proofs = await ctx.client.proof.list({ providerId, status: statusFilter });
    } else {
      throw new Error('Proof client not available.');
    }

    // Local filter
    if (statusFilter !== 'all' && proofs.length > 0) {
      proofs = proofs.filter(p => p.status === statusFilter);
    }

    if (proofs.length === 0) {
      ctx.output.info('No proofs found for the given criteria.');
      return;
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(proofs, null, 2));
      return;
    }

    if (isTableFormat(args)) {
      const tableData = proofs.map(p => ({
        'Proof ID': p.id.substring(0, 16) + '...',
        Provider: p.providerId.substring(0, 12) + '...',
        Status: statusBadge(p.status),
        Created: formatTimestamp(p.createdAt),
        'Verified At': formatTimestamp(p.verifiedAt),
      }));
      ctx.output.write(ctx.output.formatTable(tableData, `Proofs (${proofs.length})`));
      return;
    }

    // Text output
    ctx.output.write(ctx.output.colorize(`Proofs (${proofs.length})`, 'bold'));
    ctx.output.write('');
    for (const p of proofs) {
      const color = statusColor(p.status);
      const badge = statusBadge(p.status);
      const anchorInfo = p.anchorTxId ? `  Anchored: ${p.anchorTxId.substring(0, 16)}...` : '';
      ctx.output.write(
        `  ${ctx.output.colorize(p.id.substring(0, 20) + '...', 'cyan')}  ` +
        `${ctx.output.colorize(badge, color)}  ` +
        `${formatTimestamp(p.createdAt)}${anchorInfo}`
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list proofs: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: status ────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proofId = args.options.proof_id ? String(args.options.proof_id) : undefined;

  if (!proofId) {
    ctx.output.writeError('Usage: xergon proof status --proof-id <id>');
    process.exit(1);
    return;
  }

  try {
    let proof: ProofRecord;

    if (ctx.client?.proof?.status) {
      proof = await ctx.client.proof.status(proofId);
    } else {
      throw new Error('Proof client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(proof, null, 2));
      return;
    }

    const color = statusColor(proof.status);
    ctx.output.write(ctx.output.colorize('Proof Status', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write(ctx.output.formatText({
      'Proof ID': proof.id,
      'Provider ID': proof.providerId,
      'Commitment': proof.commitment || '-',
      Status: ctx.output.colorize(statusBadge(proof.status), color),
      'Created At': formatTimestamp(proof.createdAt),
      'Verified At': formatTimestamp(proof.verifiedAt),
      'Anchor Tx': proof.anchorTxId || '-',
    }));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get proof status: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: anchor ────────────────────────────────────────────

async function handleAnchor(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proofId = args.options.proof_id ? String(args.options.proof_id) : undefined;

  if (!proofId) {
    ctx.output.writeError('Usage: xergon proof anchor --proof-id <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Anchoring proof ${proofId.substring(0, 16)}... on Ergo blockchain`);

  try {
    let result: AnchorResult;

    if (ctx.client?.proof?.anchor) {
      result = await ctx.client.proof.anchor(proofId);
    } else {
      throw new Error('Proof client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Proof anchored on Ergo blockchain');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Proof ID': result.proofId,
      'Transaction ID': result.txId,
      'Block Height': String(result.blockHeight),
      Status: ctx.output.colorize(result.status.toUpperCase(), 'green'),
      'Anchored At': formatTimestamp(result.anchoredAt),
    }, 'Blockchain Anchor'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to anchor proof: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: trust ─────────────────────────────────────────────

async function handleTrust(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon proof trust --provider <id>');
    process.exit(1);
    return;
  }

  try {
    let trust: TrustScoreBreakdown;

    if (ctx.client?.proof?.trust) {
      trust = await ctx.client.proof.trust(providerId);
    } else {
      throw new Error('Proof client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(trust, null, 2));
      return;
    }

    const overallColor = trustScoreColor(trust.overallScore);
    ctx.output.write(ctx.output.colorize('Provider Trust Score', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': providerId,
      'Overall Score': ctx.output.colorize(trust.overallScore.toFixed(1) + ' / 100', overallColor),
      'Last Updated': formatTimestamp(trust.lastUpdated),
    }));
    ctx.output.write('');

    // Component breakdown
    const components = [
      { name: 'TEE Attestation', score: trust.tee, weight: '30%', bar: renderTrustBar(trust.tee, 25) },
      { name: 'ZK Proof Score', score: trust.zk, weight: '25%', bar: renderTrustBar(trust.zk, 25) },
      { name: 'Uptime', score: trust.uptime, weight: '20%', bar: renderTrustBar(trust.uptime, 25) },
      { name: 'Proof of Node Work', score: trust.ponw, weight: '15%', bar: renderTrustBar(trust.ponw, 25) },
      { name: 'Reviews', score: trust.reviews, weight: '10%', bar: renderTrustBar(trust.reviews, 25) },
    ];

    ctx.output.write(ctx.output.colorize('Component Breakdown:', 'bold'));
    for (const comp of components) {
      const color = trustScoreColor(comp.score);
      ctx.output.write(
        `  ${comp.name.padEnd(20)} (${comp.weight})  ` +
        `${ctx.output.colorize(comp.bar, color)}`
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get trust score: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function proofAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon proof <verify|submit|list|status|anchor|trust> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  verify    Verify a ZK proof against a commitment');
    ctx.output.write('  submit    Submit a proof to the relay');
    ctx.output.write('  list      List proofs for a provider');
    ctx.output.write('  status    Check proof verification status');
    ctx.output.write('  anchor    Anchor a verified proof on Ergo blockchain');
    ctx.output.write('  trust     Show provider trust score breakdown');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'verify':
      await handleVerify(args, ctx);
      break;
    case 'submit':
      await handleSubmit(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'anchor':
      await handleAnchor(args, ctx);
      break;
    case 'trust':
      await handleTrust(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: verify, submit, list, status, anchor, trust');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const proofOptions: CommandOption[] = [
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
    name: 'proof',
    short: '',
    long: '--proof',
    description: 'Path to ZK proof file',
    required: false,
    type: 'string',
  },
  {
    name: 'commitment',
    short: '',
    long: '--commitment',
    description: 'Commitment hash to verify proof against',
    required: false,
    type: 'string',
  },
  {
    name: 'batch',
    short: '',
    long: '--batch',
    description: 'Path to JSON file with batch proof data',
    required: false,
    type: 'string',
  },
  {
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Provider ID',
    required: false,
    type: 'string',
  },
  {
    name: 'proof_id',
    short: '',
    long: '--proof-id',
    description: 'Proof ID for status or anchor operations',
    required: false,
    type: 'string',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter proofs by status: all, pending, verified, rejected',
    required: false,
    default: 'all',
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const proofCommand: Command = {
  name: 'proof',
  description: 'ZKP verification: verify, submit, list, anchor proofs, and inspect trust scores',
  aliases: ['zkp', 'proofs'],
  options: proofOptions,
  action: proofAction,
};
