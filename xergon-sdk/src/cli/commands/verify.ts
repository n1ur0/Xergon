/**
 * CLI command: verify
 *
 * On-chain proof verification for the Xergon Network.
 * Verify ZK proofs, commitments, blockchain anchors, batch proofs,
 * and Ergo box register values.
 *
 * Usage:
 *   xergon verify proof --proof-id ID
 *   xergon verify commitment --hash HASH --value FILE
 *   xergon verify anchor --proof-id ID
 *   xergon verify batch --file FILE
 *   xergon verify onchain --box-id ID --register R4|R5|R6
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as crypto from 'node:crypto';

// ── Types ──────────────────────────────────────────────────────────

interface ProofVerifyResult {
  proofId: string;
  valid: boolean;
  proofType: string;
  verifiedAt: string;
  details: string;
  commitment?: string;
  circuit?: string;
}

interface CommitmentVerifyResult {
  hash: string;
  valueHash: string;
  valid: boolean;
  algorithm: string;
  verifiedAt: string;
}

interface AnchorVerifyResult {
  proofId: string;
  anchored: boolean;
  txId?: string;
  blockHeight?: number;
  chainHeight?: number;
  confirmations?: number;
  anchoredAt?: string;
  boxId?: string;
  register?: string;
  details: string;
}

interface BatchVerifyResult {
  total: number;
  passed: number;
  failed: number;
  results: Array<{
    proofId: string;
    valid: boolean;
    message: string;
  }>;
  verifiedAt: string;
}

interface OnchainVerifyResult {
  boxId: string;
  register: string;
  expectedHash: string;
  actualHash: string;
  valid: boolean;
  chainHeight: number;
  boxCreationHeight: number;
  boxValue: string;
  registerValue: string;
  verifiedAt: string;
  explanation: string;
}

interface RegisterInfo {
  name: string;
  index: number;
  purpose: string;
}

// ── Ergo register model ────────────────────────────────────────────

const ERGO_REGISTERS: Record<string, RegisterInfo> = {
  R4: { name: 'R4', index: 4, purpose: 'Custom register -- commonly used for proof commitment hashes' },
  R5: { name: 'R5', index: 5, purpose: 'Custom register -- commonly used for proof metadata or type identifier' },
  R6: { name: 'R6', index: 6, purpose: 'Custom register -- commonly used for additional proof parameters' },
  R7: { name: 'R7', index: 7, purpose: 'Custom register -- extended data storage' },
  R8: { name: 'R8', index: 8, purpose: 'Custom register -- extended data storage' },
  R9: { name: 'R9', index: 9, purpose: 'Custom register -- extended data storage' },
};

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function truncateHash(hash: string, prefixLen = 10, suffixLen = 6): string {
  if (hash.length <= prefixLen + suffixLen + 3) return hash;
  return `${hash.slice(0, prefixLen)}...${hash.slice(-suffixLen)}`;
}

function parseRegister(raw: string | undefined): string | undefined {
  if (!raw) return undefined;
  const upper = raw.toUpperCase();
  if (['R4', 'R5', 'R6', 'R7', 'R8', 'R9'].includes(upper)) {
    return upper;
  }
  return undefined;
}

function validBadge(valid: boolean): string {
  return valid ? '\x1b[32m✓ VALID\x1b[0m' : '\x1b[31m✗ INVALID\x1b[0m';
}

function validBadgePlain(valid: boolean): string {
  return valid ? 'VALID' : 'INVALID';
}

// ── Subcommand: proof ──────────────────────────────────────────────

async function handleProof(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proofId = args.options.proof_id ? String(args.options.proof_id) : undefined;

  if (!proofId) {
    ctx.output.writeError('Usage: xergon verify proof --proof-id <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying ZK proof ${proofId.substring(0, 20)}...`);

  try {
    let result: ProofVerifyResult;

    if (ctx.client?.verify?.proof) {
      result = await ctx.client.verify.proof({ proofId });
    } else {
      throw new Error('Verification client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    if (result.valid) {
      ctx.output.success('ZK proof is VALID');
    } else {
      ctx.output.writeError(`ZK proof is INVALID: ${result.details}`);
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Proof ID': result.proofId,
      'Proof Type': result.proofType,
      Status: validBadgePlain(result.valid),
      Commitment: result.commitment ? truncateHash(result.commitment, 14, 8) : 'N/A',
      Circuit: result.circuit || 'N/A',
      'Verified At': result.verifiedAt,
      Details: result.details,
    }, 'ZK Proof Verification'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify proof: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: commitment ─────────────────────────────────────────

async function handleCommitment(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const hash = args.options.hash ? String(args.options.hash) : undefined;
  const valuePath = args.options.value ? String(args.options.value) : undefined;

  if (!hash) {
    ctx.output.writeError('Usage: xergon verify commitment --hash <hash> --value <file>');
    process.exit(1);
    return;
  }

  if (!valuePath) {
    ctx.output.writeError('Usage: xergon verify commitment --hash <hash> --value <file>');
    process.exit(1);
    return;
  }

  if (!fs.existsSync(valuePath)) {
    ctx.output.writeError(`Value file not found: ${valuePath}`);
    process.exit(1);
    return;
  }

  const valueData = fs.readFileSync(valuePath);
  const valueHash = crypto.createHash('sha256').update(valueData).digest('hex');

  try {
    let result: CommitmentVerifyResult;

    if (ctx.client?.verify?.commitment) {
      result = await ctx.client.verify.commitment({
        hash,
        value: valueData,
      });
    } else {
      // Fallback: local hash comparison
      const normalizedHash = hash.startsWith('0x') ? hash.slice(2) : hash;
      const valid = normalizedHash === valueHash;
      result = {
        hash,
        valueHash: `0x${valueHash}`,
        valid,
        algorithm: 'sha256',
        verifiedAt: new Date().toISOString(),
      };
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    if (result.valid) {
      ctx.output.success('Commitment VERIFIED -- value matches hash');
    } else {
      ctx.output.writeError('Commitment MISMATCH -- value does not match hash');
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Expected Hash': result.hash,
      'Computed Hash': result.valueHash,
      Algorithm: result.algorithm,
      Status: validBadgePlain(result.valid),
      'Verified At': result.verifiedAt,
    }, 'Commitment Verification'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify commitment: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: anchor ─────────────────────────────────────────────

async function handleAnchor(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const proofId = args.options.proof_id ? String(args.options.proof_id) : undefined;

  if (!proofId) {
    ctx.output.writeError('Usage: xergon verify anchor --proof-id <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Checking blockchain anchor for proof ${proofId.substring(0, 20)}...`);

  try {
    let result: AnchorVerifyResult;

    if (ctx.client?.verify?.anchor) {
      result = await ctx.client.verify.anchor({ proofId });
    } else {
      throw new Error('Verification client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    if (result.anchored) {
      ctx.output.success('Proof is ANCHORED on the Ergo blockchain');
    } else {
      ctx.output.writeError('Proof is NOT anchored on the blockchain');
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Proof ID': result.proofId,
      Anchored: result.anchored ? 'YES' : 'NO',
      'Tx ID': result.txId ? truncateHash(result.txId, 14, 8) : 'N/A',
      'Block Height': result.blockHeight != null ? String(result.blockHeight) : 'N/A',
      'Chain Height': result.chainHeight != null ? String(result.chainHeight) : 'N/A',
      Confirmations: result.confirmations != null ? String(result.confirmations) : 'N/A',
      'Anchored At': result.anchoredAt || 'N/A',
      'Box ID': result.boxId ? truncateHash(result.boxId, 14, 8) : 'N/A',
      Register: result.register || 'N/A',
    }, 'Blockchain Anchor Verification'));

    if (result.anchored && result.confirmations != null) {
      ctx.output.write('');
      if (result.confirmations >= 6) {
        ctx.output.success(`Proof has ${result.confirmations} confirmations (well-anchored)`);
      } else {
        ctx.output.write(`Proof has ${result.confirmations} confirmation(s) -- waiting for more confirmations`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify anchor: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: batch ──────────────────────────────────────────────

async function handleBatch(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const filePath = args.options.file ? String(args.options.file) : undefined;

  if (!filePath) {
    ctx.output.writeError('Usage: xergon verify batch --file <file>');
    process.exit(1);
    return;
  }

  if (!fs.existsSync(filePath)) {
    ctx.output.writeError(`Batch file not found: ${filePath}`);
    process.exit(1);
    return;
  }

  const batchData = JSON.parse(fs.readFileSync(filePath, 'utf-8'));

  if (!Array.isArray(batchData)) {
    ctx.output.writeError('Batch file must contain a JSON array of proof objects');
    process.exit(1);
    return;
  }

  ctx.output.info(`Batch verifying ${batchData.length} proof(s)...`);

  try {
    let result: BatchVerifyResult;

    if (ctx.client?.verify?.batch) {
      result = await ctx.client.verify.batch({ proofs: batchData });
    } else {
      throw new Error('Verification client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      Total: String(result.total),
      Passed: String(result.passed),
      Failed: String(result.failed),
      'Verified At': result.verifiedAt,
    }, 'Batch Verification Summary'));

    if (result.results.length > 0) {
      ctx.output.write('');
      ctx.output.write('Individual results:');
      for (const r of result.results) {
        const badge = validBadgePlain(r.valid);
        ctx.output.write(`  ${badge}  ${r.proofId.substring(0, 24)}...  ${r.message}`);
      }
    }

    if (result.failed > 0) {
      ctx.output.write('');
      ctx.output.writeError(`${result.failed} of ${result.total} proof(s) failed verification`);
    } else {
      ctx.output.success(`All ${result.total} proof(s) verified successfully`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to batch verify: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: onchain ────────────────────────────────────────────

async function handleOnchain(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const boxId = args.options.box_id ? String(args.options.box_id) : undefined;
  const registerRaw = args.options.register ? String(args.options.register) : undefined;

  if (!boxId) {
    ctx.output.writeError('Usage: xergon verify onchain --box-id <id> --register <R4|R5|R6>');
    process.exit(1);
    return;
  }

  const register = parseRegister(registerRaw);
  if (!register) {
    ctx.output.writeError('Register must be one of: R4, R5, R6, R7, R8, R9');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying on-chain Ergo box ${boxId.substring(0, 20)} register ${register}...`);

  try {
    let result: OnchainVerifyResult;

    if (ctx.client?.verify?.onchain) {
      result = await ctx.client.verify.onchain({ boxId, register });
    } else {
      throw new Error('Verification client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    // Ergo box register model explanation
    const regInfo = ERGO_REGISTERS[register];
    ctx.output.write('');
    ctx.output.write(`Ergo Box Register Model:`);
    ctx.output.write(`  ${register} (Register ${regInfo.index}): ${regInfo.purpose}`);
    ctx.output.write('');
    ctx.output.write(`  Registers R4-R9 are custom non-mandatory registers on Ergo boxes.`);
    ctx.output.write(`  Xergon uses these to store proof hashes and verification data on-chain.`);
    ctx.output.write(`  Sigma protocols (proveDlog, proveDHTuple) verify correctness at spend time.`);
    ctx.output.write('');

    if (result.valid) {
      ctx.output.success(`On-chain verification PASSED -- ${register} contains expected proof hash`);
    } else {
      ctx.output.writeError(`On-chain verification FAILED -- ${register} does not match expected hash`);
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Box ID': result.boxId,
      Register: result.register,
      'Expected Hash': truncateHash(result.expectedHash, 14, 8),
      'Actual Hash': truncateHash(result.actualHash, 14, 8),
      Match: result.valid ? 'YES' : 'NO',
      'Chain Height': String(result.chainHeight),
      'Box Creation Height': String(result.boxCreationHeight),
      'Box Value': result.boxValue,
      'Register Value': truncateHash(result.registerValue, 14, 8),
      'Verified At': result.verifiedAt,
    }, 'On-chain Register Verification'));

    ctx.output.write('');
    ctx.output.write(`Explanation: ${result.explanation}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify on-chain: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function verifyAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon verify <proof|commitment|anchor|batch|onchain> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  proof       Verify ZK proof validity');
    ctx.output.write('  commitment  Verify value matches commitment hash');
    ctx.output.write('  anchor      Check if proof is anchored on Ergo blockchain');
    ctx.output.write('  batch       Batch verify proofs from JSON file');
    ctx.output.write('  onchain     Verify on-chain Ergo box register contains expected proof hash');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'proof':
      await handleProof(args, ctx);
      break;
    case 'commitment':
      await handleCommitment(args, ctx);
      break;
    case 'anchor':
      await handleAnchor(args, ctx);
      break;
    case 'batch':
      await handleBatch(args, ctx);
      break;
    case 'onchain':
      await handleOnchain(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: proof, commitment, anchor, batch, onchain');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const verifyOptions: CommandOption[] = [
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
    name: 'proof_id',
    short: '',
    long: '--proof-id',
    description: 'Proof ID for verification',
    required: false,
    type: 'string',
  },
  {
    name: 'hash',
    short: '',
    long: '--hash',
    description: 'Commitment hash to verify against',
    required: false,
    type: 'string',
  },
  {
    name: 'value',
    short: '',
    long: '--value',
    description: 'Path to value file for commitment verification',
    required: false,
    type: 'string',
  },
  {
    name: 'file',
    short: '',
    long: '--file',
    description: 'Path to batch proof file (JSON array)',
    required: false,
    type: 'string',
  },
  {
    name: 'box_id',
    short: '',
    long: '--box-id',
    description: 'Ergo box ID for on-chain verification',
    required: false,
    type: 'string',
  },
  {
    name: 'register',
    short: '',
    long: '--register',
    description: 'Ergo box register: R4, R5, R6, R7, R8, or R9',
    required: false,
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const verifyCommand: Command = {
  name: 'verify',
  description: 'On-chain proof verification: proof, commitment, anchor, batch, onchain',
  aliases: ['verification', 'check'],
  options: verifyOptions,
  action: verifyAction,
};
