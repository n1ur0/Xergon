/**
 * CLI command: attest
 *
 * TEE (Trusted Execution Environment) attestation management for the Xergon Network.
 * Submit, verify, and manage hardware attestation reports from providers
 * running in SGX, SEV, TDX, or software-based TEE environments.
 *
 * Usage:
 *   xergon attest submit --provider ID --tee-type sgx|sev|tdx|software --report FILE
 *   xergon attest verify --provider ID
 *   xergon attest status --provider ID
 *   xergon attest providers [--tee-type sgx|sev|tdx|all] [--status valid|expired|all]
 *   xergon attest types
 *   xergon attest renew --provider ID
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ── Types ──────────────────────────────────────────────────────────

type TEEType = 'sgx' | 'sev' | 'tdx' | 'software';
type AttestationStatus = 'valid' | 'expiring' | 'expired' | 'pending';

interface AttestationInfo {
  providerId: string;
  teeType: TEEType;
  mrenclave: string;
  status: AttestationStatus;
  lastAttested: string;
  expiresAt: string;
  reportHash: string;
}

interface AttestationVerifyResult {
  providerId: string;
  valid: boolean;
  status: AttestationStatus;
  verifiedAt: string;
  details: string;
}

interface AttestedProvider {
  providerId: string;
  teeType: TEEType;
  mrenclave: string;
  status: AttestationStatus;
  lastAttested: string;
  expiresAt: string;
}

interface AttestationSubmitResult {
  providerId: string;
  teeType: TEEType;
  status: string;
  attestationId: string;
  submittedAt: string;
  expiresAt: string;
}

interface AttestationRenewResult {
  providerId: string;
  oldExpiresAt: string;
  newExpiresAt: string;
  renewedAt: string;
  status: string;
}

// ── TEE Type metadata ──────────────────────────────────────────────

const TEE_TYPE_INFO: Record<TEEType, { label: string; vendor: string; description: string }> = {
  sgx: {
    label: 'SGX',
    vendor: 'Intel',
    description: 'Intel Software Guard Extensions -- hardware-isolated enclaves with CPU-level memory encryption',
  },
  sev: {
    label: 'SEV',
    vendor: 'AMD',
    description: 'AMD Secure Encrypted Virtualization -- VM memory encryption with SEV-SNP attestation',
  },
  tdx: {
    label: 'TDX',
    vendor: 'Intel',
    description: 'Intel Trust Domain Extensions -- confidential VMs with hardware TCB isolation',
  },
  software: {
    label: 'Software',
    vendor: 'Mock',
    description: 'Software-based attestation for development and testing (no hardware TEE)',
  },
};

// ── Helpers ────────────────────────────────────────────────────────

function isJsonOutput(args: ParsedArgs): boolean {
  return args.options.json === true;
}

function isTableFormat(args: ParsedArgs): boolean {
  return args.options.format === 'table';
}

function parseTEEType(raw: string | undefined): TEEType | undefined {
  if (!raw) return undefined;
  const normalized = raw.toLowerCase();
  if (['sgx', 'sev', 'tdx', 'software'].includes(normalized)) {
    return normalized as TEEType;
  }
  return undefined;
}

function statusBadge(status: AttestationStatus): string {
  switch (status) {
    case 'valid': return '\x1b[32m● valid\x1b[0m';
    case 'expiring': return '\x1b[33m● expiring\x1b[0m';
    case 'expired': return '\x1b[31m● expired\x1b[0m';
    case 'pending': return '\x1b[36m● pending\x1b[0m';
    default: return `\x1b[2m● ${status}\x1b[0m`;
  }
}

function statusBadgePlain(status: AttestationStatus): string {
  return status.toUpperCase();
}

function teeBadge(teeType: TEEType): string {
  const info = TEE_TYPE_INFO[teeType];
  return `\x1b[36m[${info.label}]\x1b[0m`;
}

function teeBadgePlain(teeType: TEEType): string {
  return `[${TEE_TYPE_INFO[teeType].label}]`;
}

function truncateHash(hash: string, prefixLen = 8, suffixLen = 6): string {
  if (hash.length <= prefixLen + suffixLen + 3) return hash;
  return `${hash.slice(0, prefixLen)}...${hash.slice(-suffixLen)}`;
}

function formatExpiresIn(expiresAt: string): string {
  const now = new Date();
  const expiry = new Date(expiresAt);
  const diffMs = expiry.getTime() - now.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  const diffHours = Math.floor((diffMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));

  if (diffMs < 0) {
    const absDays = Math.abs(diffDays);
    return `expired ${absDays}d ago`;
  }
  if (diffDays > 0) return `${diffDays}d ${diffHours}h`;
  return `${diffHours}h`;
}

// ── Subcommand: submit ─────────────────────────────────────────────

async function handleSubmit(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;
  const teeTypeRaw = args.options.tee_type ? String(args.options.tee_type) : undefined;
  const reportPath = args.options.report ? String(args.options.report) : undefined;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon attest submit --provider <id> --tee-type <sgx|sev|tdx|software> --report <file>');
    process.exit(1);
    return;
  }

  const teeType = parseTEEType(teeTypeRaw);
  if (!teeType) {
    ctx.output.writeError('TEE type must be one of: sgx, sev, tdx, software');
    process.exit(1);
    return;
  }

  if (!reportPath) {
    ctx.output.writeError('Usage: xergon attest submit --provider <id> --tee-type <sgx|sev|tdx|software> --report <file>');
    process.exit(1);
    return;
  }

  if (!fs.existsSync(reportPath)) {
    ctx.output.writeError(`Report file not found: ${reportPath}`);
    process.exit(1);
    return;
  }

  const reportData = fs.readFileSync(reportPath);

  ctx.output.info(`Submitting ${teeType.toUpperCase()} attestation for provider ${providerId.substring(0, 16)}...`);

  try {
    let result: AttestationSubmitResult;

    if (ctx.client?.attest?.submit) {
      result = await ctx.client.attest.submit({
        providerId,
        teeType,
        report: reportData,
      });
    } else {
      throw new Error('Attestation client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Attestation submitted successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': result.providerId,
      'TEE Type': result.teeType.toUpperCase(),
      'Attestation ID': result.attestationId,
      Status: result.status,
      'Submitted At': result.submittedAt,
      'Expires At': result.expiresAt,
    }, 'TEE Attestation Submitted'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to submit attestation: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: verify ─────────────────────────────────────────────

async function handleVerify(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon attest verify --provider <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying attestation for provider ${providerId.substring(0, 16)}...`);

  try {
    let result: AttestationVerifyResult;

    if (ctx.client?.attest?.verify) {
      result = await ctx.client.attest.verify({ providerId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    if (result.valid) {
      ctx.output.success('Attestation is VALID');
    } else {
      ctx.output.writeError(`Attestation is INVALID: ${result.details}`);
    }

    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': result.providerId,
      'Verified At': result.verifiedAt,
      Status: result.valid ? 'VALID' : 'INVALID',
      Details: result.details,
    }, 'Attestation Verification Result'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify attestation: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: status ─────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon attest status --provider <id>');
    process.exit(1);
    return;
  }

  try {
    let info: AttestationInfo;

    if (ctx.client?.attest?.status) {
      info = await ctx.client.attest.status({ providerId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(info, null, 2));
      return;
    }

    const expiresIn = formatExpiresIn(info.expiresAt);

    ctx.output.write(ctx.output.formatText({
      'Provider ID': info.providerId,
      'TEE Type': `${TEE_TYPE_INFO[info.teeType].label} (${TEE_TYPE_INFO[info.teeType].vendor})`,
      MRENCLAVE: info.mrenclave,
      Status: statusBadgePlain(info.status),
      'Last Attested': info.lastAttested,
      'Expires At': info.expiresAt,
      'Expires In': expiresIn,
      'Report Hash': truncateHash(info.reportHash, 12, 8),
    }, 'Provider Attestation Status'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get attestation status: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: providers ──────────────────────────────────────────

async function handleProviders(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const teeTypeFilter = args.options.tee_type ? String(args.options.tee_type) : 'all';
  const statusFilter = args.options.status ? String(args.options.status) : 'all';

  try {
    let providers: AttestedProvider[];

    if (ctx.client?.attest?.providers) {
      providers = await ctx.client.attest.providers({
        teeType: teeTypeFilter,
        status: statusFilter,
      });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(providers, null, 2));
      return;
    }

    if (providers.length === 0) {
      ctx.output.write('No attested providers found matching the given filters.');
      return;
    }

    if (isTableFormat(args)) {
      const header = 'PROVIDER                              TEE TYPE     MRENCLAVE                    STATUS        LAST ATTESTED        EXPIRES IN';
      const separator = '────────────────────────────────────── ──────────── ──────────────────────────── ──────────── ───────────────────── ───────────';
      ctx.output.write(header);
      ctx.output.write(separator);

      for (const p of providers) {
        const pid = p.providerId.padEnd(38);
        const tee = teeBadgePlain(p.teeType).padEnd(13);
        const mrenclave = truncateHash(p.mrenclave, 12, 8).padEnd(28);
        const status = statusBadgePlain(p.status).padEnd(13);
        const lastAtt = p.lastAttested.substring(0, 19).padEnd(21);
        const expiresIn = formatExpiresIn(p.expiresAt).padEnd(11);
        ctx.output.write(`${pid} ${tee} ${mrenclave} ${status} ${lastAtt} ${expiresIn}`);
      }
    } else {
      ctx.output.write(`Found ${providers.length} attested provider(s):\n`);

      for (const p of providers) {
        const expiresIn = formatExpiresIn(p.expiresAt);
        ctx.output.write(`  ${teeBadge(p.teeType)} ${p.providerId.substring(0, 20)}...`);
        ctx.output.write(`    MRENCLAVE:  ${truncateHash(p.mrenclave, 12, 8)}`);
        ctx.output.write(`    Status:    ${statusBadge(p.status)}`);
        ctx.output.write(`    Attested:  ${p.lastAttested.substring(0, 19)}`);
        ctx.output.write(`    Expires:   ${expiresIn}`);
        ctx.output.write('');
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list providers: ${message}`);
    process.exit(1);
  }
}

// ── Subcommand: types ──────────────────────────────────────────────

async function handleTypes(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const types: Array<{ type: TEEType; label: string; vendor: string; description: string }> = Object.entries(TEE_TYPE_INFO).map(([key, info]) => ({
    type: key as TEEType,
    label: info.label,
    vendor: info.vendor,
    description: info.description,
  }));

  if (isJsonOutput(args)) {
    ctx.output.write(JSON.stringify(types, null, 2));
    return;
  }

  ctx.output.write('Supported TEE Types:\n');

  for (const t of types) {
    ctx.output.write(`  ${t.label} (${t.vendor})`);
    ctx.output.write(`    Type:        ${t.type}`);
    ctx.output.write(`    Description: ${t.description}`);
    ctx.output.write('');
  }

  ctx.output.write('Use --tee-type <type> with submit or providers commands to filter by TEE type.');
}

// ── Subcommand: renew ──────────────────────────────────────────────

async function handleRenew(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.options.provider ? String(args.options.provider) : undefined;

  if (!providerId) {
    ctx.output.writeError('Usage: xergon attest renew --provider <id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Renewing attestation for provider ${providerId.substring(0, 16)}...`);

  try {
    let result: AttestationRenewResult;

    if (ctx.client?.attest?.renew) {
      result = await ctx.client.attest.renew({ providerId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.success('Attestation renewed successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': result.providerId,
      'Old Expires At': result.oldExpiresAt,
      'New Expires At': result.newExpiresAt,
      'Renewed At': result.renewedAt,
      Status: result.status,
    }, 'Attestation Renewed'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to renew attestation: ${message}`);
    process.exit(1);
  }
}

// ── Command action ─────────────────────────────────────────────────

async function attestAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon attest <submit|verify|status|providers|types|renew> [options]');
    ctx.output.write('');
    ctx.output.write('Subcommands:');
    ctx.output.write('  submit     Submit TEE attestation report');
    ctx.output.write('  verify     Verify provider attestation status');
    ctx.output.write('  status     Show detailed attestation info for a provider');
    ctx.output.write('  providers  List attested providers with optional filters');
    ctx.output.write('  types      Show supported TEE types with descriptions');
    ctx.output.write('  renew      Renew an expiring attestation');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'submit':
      await handleSubmit(args, ctx);
      break;
    case 'verify':
      await handleVerify(args, ctx);
      break;
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'providers':
      await handleProviders(args, ctx);
      break;
    case 'types':
      await handleTypes(args, ctx);
      break;
    case 'renew':
      await handleRenew(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Valid subcommands: submit, verify, status, providers, types, renew');
      process.exit(1);
      break;
  }
}

// ── Options ────────────────────────────────────────────────────────

const attestOptions: CommandOption[] = [
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
    name: 'provider',
    short: '',
    long: '--provider',
    description: 'Provider ID for attestation operations',
    required: false,
    type: 'string',
  },
  {
    name: 'tee_type',
    short: '',
    long: '--tee-type',
    description: 'TEE type: sgx, sev, tdx, or software',
    required: false,
    type: 'string',
  },
  {
    name: 'report',
    short: '',
    long: '--report',
    description: 'Path to TEE attestation report file',
    required: false,
    type: 'string',
  },
  {
    name: 'status',
    short: '',
    long: '--status',
    description: 'Filter by attestation status: valid, expired, or all (default: all)',
    required: false,
    default: 'all',
    type: 'string',
  },
];

// ── Command export ─────────────────────────────────────────────────

export const attestCommand: Command = {
  name: 'attest',
  description: 'Manage TEE attestation: submit, verify, status, providers, types, renew',
  aliases: ['attestation', 'tee'],
  options: attestOptions,
  action: attestAction,
};
