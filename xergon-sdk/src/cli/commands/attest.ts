/**
 * CLI command: attest
 *
 * TEE attestation and model provenance verification for the Xergon Network.
 * Submit, verify, and manage hardware attestation reports from providers
 * running in SGX, SEV, TDX, or software-based TEE environments.
 * Also verify model attestation chains, trust scores, and provenance.
 *
 * Usage:
 *   xergon attest submit --provider ID --tee-type sgx|sev|tdx|software --report FILE
 *   xergon attest verify --provider ID
 *   xergon attest status --provider ID
 *   xergon attest providers [--tee-type sgx|sev|tdx|all] [--status valid|expired|all]
 *   xergon attest types
 *   xergon attest renew --provider ID
 *   xergon attest model <model-id>           Verify model attestation chain
 *   xergon attest provider-attest <id>       Verify provider attestation status
 *   xergon attest artifact <hash>            Verify artifact hash chain entry
 *   xergon attest chain <model-id>           Verify full hash chain integrity
 *   xergon attest list --model <id>          List attestations for a model
 *   xergon attest score <model-id>           Show model trust score
 *   xergon attest export <model-id> --format json|yaml
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';
import * as path from 'node:path';

// ══════════════════════════════════════════════════════════════════
// Types -- TEE Attestation
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Types -- Model Provenance Attestation
// ══════════════════════════════════════════════════════════════════

type TrustLevel = 'Trusted' | 'Provisional' | 'Untrusted' | 'Unknown';

interface AttestationRecord {
  id: string;
  modelId: string;
  providerId: string;
  artifactType: string;
  artifactHash: string;
  chainIndex: number;
  chainHash: string;
  previousHash: string;
  verified: boolean;
  verifiedAt: string;
  trustLevel: TrustLevel;
  checks: string[];
}

interface ChainVerificationResult {
  valid: boolean;
  entriesChecked: number;
  firstInvalidIndex?: number;
  mismatchAt?: string;
}

interface ModelTrustScore {
  modelId: string;
  overallScore: number;
  attestationCount: number;
  trustLevel: TrustLevel;
  latestAttestation: string;
  artifactTypes: string[];
  riskFactors: string[];
}

interface AttestationReport {
  modelId: string;
  generatedAt: string;
  attestations: AttestationRecord[];
  chainValid: boolean;
  trustScore: ModelTrustScore;
  recommendations: string[];
}

interface AttestationStatusSummary {
  totalModels: number;
  verified: number;
  provisional: number;
  untrusted: number;
  pending: number;
  lastChecked: string;
}

// ══════════════════════════════════════════════════════════════════
// TEE Type metadata
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════

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

function formatTimestamp(iso: string | undefined): string {
  if (!iso) return '-';
  return new Date(iso).toISOString().slice(0, 19).replace('T', ' ');
}

function trustLevelColor(level: TrustLevel): 'green' | 'yellow' | 'red' | 'dim' {
  switch (level) {
    case 'Trusted': return 'green';
    case 'Provisional': return 'yellow';
    case 'Untrusted': return 'red';
    case 'Unknown': return 'dim';
  }
}

function trustLevelBadge(level: TrustLevel): string {
  switch (level) {
    case 'Trusted': return '\x1b[32m● Trusted\x1b[0m';
    case 'Provisional': return '\x1b[33m● Provisional\x1b[0m';
    case 'Untrusted': return '\x1b[31m● Untrusted\x1b[0m';
    case 'Unknown': return '\x1b[2m● Unknown\x1b[0m';
  }
}

function trustLevelBadgePlain(level: TrustLevel): string {
  return level.toUpperCase();
}

function renderTrustBar(score: number, width: number = 30): string {
  const pct = Math.min(Math.max(score, 0), 100);
  const filled = Math.round((pct / 100) * width);
  const empty = width - filled;
  return '[' + '█'.repeat(filled) + '░'.repeat(empty) + '] ' + pct.toFixed(1);
}

function renderChainVisualization(records: AttestationRecord[]): string {
  if (records.length === 0) return '  (empty chain)';
  if (records.length === 1) {
    return `  ${truncateHash(records[0].artifactHash, 12, 8)}`;
  }

  const lines: string[] = [];
  for (let i = 0; i < records.length; i++) {
    const r = records[i];
    const arrow = i < records.length - 1 ? ' -> ' : '';
    const hash = truncateHash(r.artifactHash, 10, 6);
    const verified = r.verified ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
    lines.push(`  ${verified} ${hash}${arrow}`);
  }
  return lines.join('\n');
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: submit (TEE)
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Subcommand: verify (TEE)
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Subcommand: status (TEE)
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Subcommand: providers (TEE)
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Subcommand: types (TEE)
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Subcommand: renew (TEE)
// ══════════════════════════════════════════════════════════════════

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

// ══════════════════════════════════════════════════════════════════
// Subcommand: model (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleModel(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[1];

  if (!modelId) {
    ctx.output.writeError('Usage: xergon attest model <model-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying model attestation chain for ${modelId}...`);

  try {
    let records: AttestationRecord[];

    if (ctx.client?.attest?.model) {
      records = await ctx.client.attest.model({ modelId });
    } else {
      throw new Error('Attestation client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(records, null, 2));
      return;
    }

    if (records.length === 0) {
      ctx.output.write(`No attestation records found for model ${modelId}.`);
      return;
    }

    ctx.output.write(ctx.output.colorize(`Model Attestation Chain (${records.length} entries)`, 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(50), 'dim'));
    ctx.output.write('');

    // Show chain visualization
    ctx.output.write(ctx.output.colorize('Hash Chain:', 'cyan'));
    ctx.output.write(renderChainVisualization(records));
    ctx.output.write('');

    // Show individual records
    for (const r of records) {
      const levelColor = trustLevelColor(r.trustLevel);
      const verified = r.verified ? 'VERIFIED' : 'UNVERIFIED';
      ctx.output.write(ctx.output.formatText({
        'Attestation ID': r.id,
        'Artifact Type': r.artifactType,
        'Artifact Hash': truncateHash(r.artifactHash, 14, 8),
        'Chain Index': String(r.chainIndex),
        'Trust Level': ctx.output.colorize(trustLevelBadgePlain(r.trustLevel), levelColor),
        Status: verified,
        'Verified At': formatTimestamp(r.verifiedAt),
        Checks: r.checks.length > 0 ? r.checks.join(', ') : 'none',
      }));
      ctx.output.write('');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify model attestation: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: provider-attest (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleProviderAttest(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const providerId = args.positional[1];

  if (!providerId) {
    ctx.output.writeError('Usage: xergon attest provider-attest <provider-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying provider attestation status for ${providerId}...`);

  try {
    let status: { providerId: string; attestationCount: number; trustLevel: TrustLevel; lastAttestation: string; details: string };

    if (ctx.client?.attest?.providerAttest) {
      status = await ctx.client.attest.providerAttest({ providerId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(status, null, 2));
      return;
    }

    const levelColor = trustLevelColor(status.trustLevel);
    ctx.output.write(ctx.output.colorize('Provider Attestation Status', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Provider ID': status.providerId,
      'Trust Level': ctx.output.colorize(trustLevelBadgePlain(status.trustLevel), levelColor),
      'Attestation Count': String(status.attestationCount),
      'Last Attestation': formatTimestamp(status.lastAttestation),
      Details: status.details,
    }));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify provider attestation: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: artifact (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleArtifact(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const hash = args.positional[1];

  if (!hash) {
    ctx.output.writeError('Usage: xergon attest artifact <hash>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying artifact hash chain entry ${truncateHash(hash, 14, 8)}...`);

  try {
    let record: AttestationRecord;

    if (ctx.client?.attest?.artifact) {
      record = await ctx.client.attest.artifact({ hash });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(record, null, 2));
      return;
    }

    const levelColor = trustLevelColor(record.trustLevel);
    ctx.output.write(ctx.output.colorize('Artifact Attestation', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Attestation ID': record.id,
      'Model ID': record.modelId,
      'Provider ID': record.providerId,
      'Artifact Type': record.artifactType,
      'Artifact Hash': record.artifactHash,
      'Chain Index': String(record.chainIndex),
      'Chain Hash': truncateHash(record.chainHash, 14, 8),
      'Previous Hash': truncateHash(record.previousHash, 14, 8),
      'Trust Level': ctx.output.colorize(trustLevelBadgePlain(record.trustLevel), levelColor),
      Verified: record.verified ? 'YES' : 'NO',
      'Verified At': formatTimestamp(record.verifiedAt),
      Checks: record.checks.length > 0 ? record.checks.join(', ') : 'none',
    }));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify artifact: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: chain (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleChain(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[1];

  if (!modelId) {
    ctx.output.writeError('Usage: xergon attest chain <model-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Verifying full hash chain integrity for model ${modelId}...`);

  try {
    let result: ChainVerificationResult;

    if (ctx.client?.attest?.chain) {
      result = await ctx.client.attest.chain({ modelId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(result, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Hash Chain Integrity Check', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Model ID': modelId,
      Valid: result.valid ? ctx.output.colorize('YES', 'green') : ctx.output.colorize('NO', 'red'),
      'Entries Checked': String(result.entriesChecked),
      'First Invalid Index': result.firstInvalidIndex != null ? String(result.firstInvalidIndex) : 'N/A',
      'Mismatch At': result.mismatchAt || 'N/A',
    }));

    if (result.valid) {
      ctx.output.success('Hash chain integrity verified -- all entries are consistent');
    } else {
      ctx.output.writeError('Hash chain integrity FAILED');
      if (result.firstInvalidIndex != null) {
        ctx.output.write(`  First invalid entry at index ${result.firstInvalidIndex}`);
      }
      if (result.mismatchAt) {
        ctx.output.write(`  Mismatch detected: ${result.mismatchAt}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to verify hash chain: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: list (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleList(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.options.model ? String(args.options.model) : undefined;

  if (!modelId) {
    ctx.output.writeError('Usage: xergon attest list --model <id>');
    process.exit(1);
    return;
  }

  try {
    let records: AttestationRecord[];

    if (ctx.client?.attest?.list) {
      records = await ctx.client.attest.list({ modelId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(records, null, 2));
      return;
    }

    if (records.length === 0) {
      ctx.output.write(`No attestations found for model ${modelId}.`);
      return;
    }

    ctx.output.write(ctx.output.colorize(`Attestations for ${modelId} (${records.length})`, 'bold'));
    ctx.output.write('');

    if (isTableFormat(args)) {
      const tableData = records.map(r => ({
        Index: String(r.chainIndex),
        Type: r.artifactType,
        Hash: truncateHash(r.artifactHash, 12, 6),
        Trust: trustLevelBadgePlain(r.trustLevel),
        Verified: r.verified ? 'YES' : 'NO',
        'At': formatTimestamp(r.verifiedAt),
      }));
      ctx.output.write(ctx.output.formatTable(tableData));
    } else {
      for (const r of records) {
        const levelColor = trustLevelColor(r.trustLevel);
        const verified = r.verified ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
        ctx.output.write(
          `  ${String(r.chainIndex).padStart(3)}  ` +
          `${r.artifactType.padEnd(14)}  ` +
          `${truncateHash(r.artifactHash, 12, 6)}  ` +
          `${verified}  ` +
          ctx.output.colorize(trustLevelBadgePlain(r.trustLevel).padEnd(12), levelColor) +
          `  ${formatTimestamp(r.verifiedAt)}`
        );
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list attestations: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: score (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleScore(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[1];

  if (!modelId) {
    ctx.output.writeError('Usage: xergon attest score <model-id>');
    process.exit(1);
    return;
  }

  ctx.output.info(`Computing trust score for model ${modelId}...`);

  try {
    let score: ModelTrustScore;

    if (ctx.client?.attest?.score) {
      score = await ctx.client.attest.score({ modelId });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(score, null, 2));
      return;
    }

    const levelColor = trustLevelColor(score.trustLevel);

    ctx.output.write(ctx.output.colorize('Model Trust Score', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Model ID': score.modelId,
      'Overall Score': ctx.output.colorize(renderTrustBar(score.overallScore), levelColor),
      'Trust Level': ctx.output.colorize(trustLevelBadgePlain(score.trustLevel), levelColor),
      'Attestation Count': String(score.attestationCount),
      'Latest Attestation': formatTimestamp(score.latestAttestation),
      'Artifact Types': score.artifactTypes.join(', ') || 'none',
      'Risk Factors': score.riskFactors.length > 0 ? score.riskFactors.join(', ') : 'none',
    }));

    if (score.riskFactors.length > 0) {
      ctx.output.write('');
      ctx.output.warn(`${score.riskFactors.length} risk factor(s) identified`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get trust score: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Subcommand: export (Provenance)
// ══════════════════════════════════════════════════════════════════

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const modelId = args.positional[1];
  const format = args.options.format ? String(args.options.format) : 'json';
  const outputPath = args.options.output ? String(args.options.output) : undefined;

  if (!modelId) {
    ctx.output.writeError('Usage: xergon attest export <model-id> --format json|yaml');
    process.exit(1);
    return;
  }

  if (!['json', 'yaml'].includes(format)) {
    ctx.output.writeError('Export format must be json or yaml');
    process.exit(1);
    return;
  }

  ctx.output.info(`Generating attestation report for model ${modelId}...`);

  try {
    let report: AttestationReport;

    if (ctx.client?.attest?.export) {
      report = await ctx.client.attest.export({ modelId, format });
    } else {
      throw new Error('Attestation client not available.');
    }

    if (isJsonOutput(args) && format === 'json') {
      ctx.output.write(JSON.stringify(report, null, 2));
      return;
    }

    let content: string;
    if (format === 'yaml') {
      // Simple YAML-like rendering
      content = renderYamlReport(report);
    } else {
      content = JSON.stringify(report, null, 2);
    }

    if (outputPath) {
      const resolvedPath = path.resolve(outputPath);
      fs.writeFileSync(resolvedPath, content, 'utf-8');
      ctx.output.success(`Attestation report exported to ${resolvedPath} (${content.length} bytes)`);
    } else {
      ctx.output.write(content);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to export attestation report: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// Default action (no subcommand)
// ══════════════════════════════════════════════════════════════════

async function handleDefault(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    let summary: AttestationStatusSummary;

    if (ctx.client?.attest?.summary) {
      summary = await ctx.client.attest.summary({});
    } else {
      throw new Error('Attestation client not available. Ensure you are connected to the Xergon network.');
    }

    if (isJsonOutput(args)) {
      ctx.output.write(JSON.stringify(summary, null, 2));
      return;
    }

    ctx.output.write(ctx.output.colorize('Attestation Status Summary', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40), 'dim'));
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      'Total Models': String(summary.totalModels),
      Verified: ctx.output.colorize(String(summary.verified), 'green'),
      Provisional: ctx.output.colorize(String(summary.provisional), 'yellow'),
      Untrusted: ctx.output.colorize(String(summary.untrusted), 'red'),
      Pending: String(summary.pending),
      'Last Checked': formatTimestamp(summary.lastChecked),
    }));

    ctx.output.write('');
    ctx.output.write('Use a subcommand for details:');
    ctx.output.write('  model <id>           Verify model attestation chain');
    ctx.output.write('  provider-attest <id> Verify provider attestation');
    ctx.output.write('  artifact <hash>      Verify artifact hash chain entry');
    ctx.output.write('  chain <id>           Verify full hash chain integrity');
    ctx.output.write('  list --model <id>    List attestations for a model');
    ctx.output.write('  score <id>           Show model trust score');
    ctx.output.write('  export <id>          Export attestation report');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get attestation summary: ${message}`);
    process.exit(1);
  }
}

// ══════════════════════════════════════════════════════════════════
// YAML rendering helper
// ══════════════════════════════════════════════════════════════════

function renderYamlReport(report: AttestationReport): string {
  const lines: string[] = [];
  lines.push(`model_id: ${report.modelId}`);
  lines.push(`generated_at: ${report.generatedAt}`);
  lines.push(`chain_valid: ${report.chainValid}`);
  lines.push('');

  lines.push('trust_score:');
  lines.push(`  model_id: ${report.trustScore.modelId}`);
  lines.push(`  overall_score: ${report.trustScore.overallScore}`);
  lines.push(`  attestation_count: ${report.trustScore.attestationCount}`);
  lines.push(`  trust_level: ${report.trustScore.trustLevel}`);
  lines.push(`  latest_attestation: ${report.trustScore.latestAttestation}`);
  lines.push(`  artifact_types: [${report.trustScore.artifactTypes.map(t => `"${t}"`).join(', ')}]`);
  if (report.trustScore.riskFactors.length > 0) {
    lines.push(`  risk_factors:`);
    for (const rf of report.trustScore.riskFactors) {
      lines.push(`    - "${rf}"`);
    }
  }

  lines.push('');
  lines.push(`attestations_count: ${report.attestations.length}`);
  lines.push('attestations:');
  for (const a of report.attestations) {
    lines.push(`  - id: ${a.id}`);
    lines.push(`    model_id: ${a.modelId}`);
    lines.push(`    artifact_type: ${a.artifactType}`);
    lines.push(`    artifact_hash: ${a.artifactHash}`);
    lines.push(`    chain_index: ${a.chainIndex}`);
    lines.push(`    verified: ${a.verified}`);
    lines.push(`    trust_level: ${a.trustLevel}`);
    lines.push('');
  }

  if (report.recommendations.length > 0) {
    lines.push('recommendations:');
    for (const r of report.recommendations) {
      lines.push(`  - "${r}"`);
    }
  }

  return lines.join('\n') + '\n';
}

// ══════════════════════════════════════════════════════════════════
// Command action
// ══════════════════════════════════════════════════════════════════

async function attestAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    await handleDefault(args, ctx);
    return;
  }

  switch (sub) {
    // TEE attestation commands
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
    // Model provenance attestation commands
    case 'model':
      await handleModel(args, ctx);
      break;
    case 'provider-attest':
      await handleProviderAttest(args, ctx);
      break;
    case 'artifact':
      await handleArtifact(args, ctx);
      break;
    case 'chain':
      await handleChain(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'score':
      await handleScore(args, ctx);
      break;
    case 'export':
      await handleExport(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('');
      ctx.output.write('TEE Attestation:');
      ctx.output.write('  submit      Submit TEE attestation report');
      ctx.output.write('  verify      Verify provider TEE attestation status');
      ctx.output.write('  status      Show detailed TEE attestation info');
      ctx.output.write('  providers   List attested providers with optional filters');
      ctx.output.write('  types       Show supported TEE types');
      ctx.output.write('  renew       Renew an expiring attestation');
      ctx.output.write('');
      ctx.output.write('Model Provenance:');
      ctx.output.write('  model           Verify model attestation chain');
      ctx.output.write('  provider-attest Verify provider attestation status');
      ctx.output.write('  artifact        Verify artifact hash chain entry');
      ctx.output.write('  chain           Verify full hash chain integrity');
      ctx.output.write('  list            List attestations for a model');
      ctx.output.write('  score           Show model trust score');
      ctx.output.write('  export          Export attestation report');
      process.exit(1);
      break;
  }
}

// ══════════════════════════════════════════════════════════════════
// Options
// ══════════════════════════════════════════════════════════════════

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
    description: 'Output format: text, json, table (or json|yaml for export)',
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
  {
    name: 'model',
    short: '',
    long: '--model',
    description: 'Model ID for listing attestations',
    required: false,
    type: 'string',
  },
  {
    name: 'output',
    short: '',
    long: '--output',
    description: 'Output file path for export',
    required: false,
    type: 'string',
  },
];

// ══════════════════════════════════════════════════════════════════
// Command export
// ══════════════════════════════════════════════════════════════════

export const attestCommand: Command = {
  name: 'attest',
  description: 'TEE attestation & model provenance: submit, verify, model, artifact, chain, list, score, export',
  aliases: ['attestation', 'tee', 'provenance'],
  options: attestOptions,
  action: attestAction,
};
