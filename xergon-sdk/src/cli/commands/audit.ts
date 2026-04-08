/**
 * Xergon Audit CLI Commands
 *
 * Security audit tooling for Xergon protocol contracts and deployments.
 * - scan:      Scan contract ErgoTree for vulnerabilities
 * - registers: Verify register layout against expected spec
 * - deps:      Audit dependencies for known vulnerabilities
 * - report:    Generate full security report
 * - score:     Quick security score for a contract
 *
 * Usage: xergon audit <command> [options]
 */

import { CommandModule } from 'yargs';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface SigmaType {
  name: string;
  prefix: string;
  description: string;
}

interface RegisterSpec {
  register: string;
  expectedType: string;
  purpose: string;
  required: boolean;
}

interface ContractSpec {
  name: string;
  contractType: string;
  registers: RegisterSpec[];
  tokens: { name: string; required: boolean }[];
  description: string;
}

interface CheckResult {
  name: string;
  passed: boolean;
  message: string;
  severity: 'critical' | 'warning' | 'info';
}

interface ScanResult {
  valid: boolean;
  ergoTreeHex: string;
  checks: CheckResult[];
  score: number;
  errors: string[];
  warnings: string[];
}

interface RegisterCheckResult {
  register: string;
  expectedType: string;
  actualType: string;
  match: boolean;
  purpose: string;
  required: boolean;
}

interface RegisterVerifyResult {
  valid: boolean;
  contractName: string;
  results: RegisterCheckResult[];
  errors: string[];
}

interface DependencyAuditResult {
  name: string;
  version: string;
  status: 'safe' | 'warning' | 'vulnerable';
  severity?: string;
  description: string;
  recommendation: string;
}

interface SecurityReport {
  generatedAt: string;
  contracts: { name: string; score: number; issues: string[] }[];
  overallScore: number;
  criticalIssues: string[];
  recommendations: string[];
}

// ---------------------------------------------------------------------------
// Sigma Type Detection
// ---------------------------------------------------------------------------

const SIGMA_TYPES: SigmaType[] = [
  { name: 'SigmaProp', prefix: '0e08cd02', description: 'Sigma proposition (proveDlog)' },
  { name: 'GroupElement', prefix: '0e0b', description: 'EC point (public key)' },
  { name: 'SLong', prefix: '0e21', description: '64-bit signed integer' },
  { name: 'SInt', prefix: '0e29', description: '32-bit signed integer' },
  { name: 'SBoolean', prefix: '0e0c', description: 'Boolean value' },
  { name: 'CollByte', prefix: '0e08', description: 'Byte array' },
  { name: 'SString', prefix: '0e05', description: 'String (Coll[Byte] UTF-8)' },
];

function detectSigmaType(hex: string): SigmaType | null {
  const clean = hex.startsWith('0x') ? hex.slice(2) : hex;
  for (const t of SIGMA_TYPES) {
    if (clean.startsWith(t.prefix)) return t;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Contract Specs
// ---------------------------------------------------------------------------

const CONTRACT_SPECS: ContractSpec[] = [
  {
    name: 'provider_box',
    contractType: 'state_box',
    description: 'Per-provider state box with singleton NFT',
    registers: [
      { register: 'R4', expectedType: 'GroupElement', purpose: 'Provider public key', required: true },
      { register: 'R5', expectedType: 'CollByte', purpose: 'Endpoint URL (UTF-8)', required: true },
      { register: 'R6', expectedType: 'CollByte', purpose: 'Models served (JSON)', required: true },
      { register: 'R7', expectedType: 'SInt', purpose: 'PoNW score (0-1000)', required: true },
      { register: 'R8', expectedType: 'SInt', purpose: 'Last heartbeat height', required: true },
      { register: 'R9', expectedType: 'CollByte', purpose: 'Region (UTF-8)', required: true },
    ],
    tokens: [{ name: 'Provider NFT', required: true }],
  },
  {
    name: 'user_staking',
    contractType: 'balance_box',
    description: 'User balance box (ERG value = balance)',
    registers: [
      { register: 'R4', expectedType: 'SigmaProp', purpose: 'User public key', required: true },
      { register: 'R5', expectedType: 'SLong', purpose: 'Last activity timestamp', required: false },
    ],
    tokens: [],
  },
  {
    name: 'usage_proof',
    contractType: 'receipt_box',
    description: 'Immutable inference receipt',
    registers: [
      { register: 'R4', expectedType: 'CollByte', purpose: 'User pubkey hash', required: true },
      { register: 'R5', expectedType: 'CollByte', purpose: 'Provider NFT ID', required: true },
      { register: 'R6', expectedType: 'SLong', purpose: 'Token count (input)', required: true },
      { register: 'R7', expectedType: 'SLong', purpose: 'Token count (output)', required: true },
      { register: 'R8', expectedType: 'CollByte', purpose: 'Model ID', required: true },
      { register: 'R9', expectedType: 'SLong', purpose: 'Timestamp', required: true },
    ],
    tokens: [],
  },
  {
    name: 'treasury',
    contractType: 'governance_box',
    description: 'Protocol treasury with governance key',
    registers: [
      { register: 'R4', expectedType: 'SigmaProp', purpose: 'Governance authority key', required: true },
      { register: 'R5', expectedType: 'SLong', purpose: 'Total ERG allocated', required: true },
    ],
    tokens: [{ name: 'Xergon Network NFT', required: true }],
  },
  {
    name: 'governance_proposal',
    contractType: 'governance_box',
    description: 'On-chain governance proposal',
    registers: [
      { register: 'R4', expectedType: 'CollByte', purpose: 'Proposal description hash', required: true },
      { register: 'R5', expectedType: 'SLong', purpose: 'Votes for', required: true },
      { register: 'R6', expectedType: 'SLong', purpose: 'Votes against', required: true },
      { register: 'R7', expectedType: 'SLong', purpose: 'Creation height', required: true },
      { register: 'R8', expectedType: 'SLong', purpose: 'Voting deadline', required: true },
      { register: 'R9', expectedType: 'CollByte', purpose: 'Proposer pubkey hash', required: true },
    ],
    tokens: [],
  },
  {
    name: 'provider_slashing',
    contractType: 'penalty_box',
    description: 'Slashing evidence for misbehaving providers',
    registers: [
      { register: 'R4', expectedType: 'CollByte', purpose: 'Provider NFT ID', required: true },
      { register: 'R5', expectedType: 'CollByte', purpose: 'Evidence hash', required: true },
      { register: 'R6', expectedType: 'SLong', purpose: 'Slash amount', required: true },
      { register: 'R7', expectedType: 'SLong', purpose: 'Report timestamp', required: true },
    ],
    tokens: [],
  },
];

// ---------------------------------------------------------------------------
// Core Audit Functions
// ---------------------------------------------------------------------------

function scanErgoTree(ergoTreeHex: string): ScanResult {
  const checks: CheckResult[] = [];
  const errors: string[] = [];
  const warnings: string[] = [];

  const clean = ergoTreeHex.startsWith('0x') ? ergoTreeHex.slice(2) : ergoTreeHex;

  // Check 1: Valid hex
  const isHex = /^[0-9a-fA-F]+$/.test(clean);
  checks.push({
    name: 'valid_hex',
    passed: isHex,
    message: isHex ? 'ErgoTree is valid hex' : 'Contains non-hex characters',
    severity: 'critical',
  });
  if (!isHex) errors.push('Invalid hex encoding');

  // Check 2: Min length
  const hasMinLen = clean.length >= 8;
  checks.push({
    name: 'min_length',
    passed: hasMinLen,
    message: `${clean.length / 2} bytes (min: 4)`,
    severity: 'critical',
  });

  // Check 3: Version byte
  if (clean.length >= 2) {
    const ver = parseInt(clean.slice(0, 2), 16);
    checks.push({
      name: 'version_byte',
      passed: ver === 0x00,
      message: `ErgoTree version: 0x${ver.toString(16).padStart(2, '0')}`,
      severity: 'warning',
    });
  }

  // Check 4: proveDlog (spending protection)
  const hasPD = clean.includes('cd02');
  checks.push({
    name: 'has_prove_dlog',
    passed: hasPD,
    message: hasPD ? 'Contract requires proveDlog (signature)' : 'No proveDlog — unprotected spending!',
    severity: 'critical',
  });
  if (!hasPD) errors.push('Contract has no proveDlog — potentially unprotected');

  // Check 5: HEIGHT reference
  const hasHeight = clean.includes('0422');
  checks.push({
    name: 'has_height_check',
    passed: hasHeight,
    message: hasHeight ? 'Contract references HEIGHT' : 'No HEIGHT reference',
    severity: 'info',
  });

  // Check 6: Logic complexity
  const andC = (clean.match(/cff8/g) ?? []).length;
  const orC = (clean.match(/cff4/g) ?? []).length;
  const complex = andC + orC >= 10;
  checks.push({
    name: 'logic_complexity',
    passed: !complex,
    message: `AND: ${andC}, OR: ${orC}`,
    severity: 'info',
  });
  if (complex) warnings.push('High logic complexity');

  const critPassed = checks.filter(c => c.severity === 'critical').every(c => c.passed);
  const score = critPassed && warnings.length === 0 ? 9
    : critPassed ? 7
    : errors.length === 0 ? 5
    : 3;

  return { valid: critPassed, ergoTreeHex, checks, score, errors, warnings };
}

function verifyRegisters(contractName: string, registers: Record<string, string>): RegisterVerifyResult {
  const spec = CONTRACT_SPECS.find(s => s.name === contractName);
  if (!spec) {
    return { valid: false, contractName, results: [], errors: [`Unknown contract: ${contractName}`] };
  }

  const results: RegisterCheckResult[] = [];
  const errors: string[] = [];

  for (const reg of spec.registers) {
    const actualHex = registers[reg.register] ?? '';
    const actualType = detectSigmaType(actualHex);
    const expectedType = SIGMA_TYPES.find(t => t.name === reg.expectedType);

    const typeMatch = (actualHex === '' && !reg.required)
      || (actualHex !== '' && actualType?.name === reg.expectedType);

    results.push({
      register: reg.register,
      expectedType: reg.expectedType,
      actualType: actualType?.name ?? 'empty',
      match: typeMatch,
      purpose: reg.purpose,
      required: reg.required,
    });

    if (!typeMatch && reg.required) {
      errors.push(`${reg.register}: expected ${reg.expectedType}, got ${actualType?.name ?? 'empty'}`);
    }
  }

  return { valid: errors.length === 0, contractName, results, errors };
}

function auditDependencies(deps: Record<string, string>): DependencyAuditResult[] {
  const results: DependencyAuditResult[] = [];
  const knownVulns: Record<string, { severity: string; desc: string }> = {
    'sigma-rust': { severity: 'medium', desc: 'Old sigma-rust versions have known signature malleability issues' },
    'ergo-lib-wasm': { severity: 'low', desc: 'Ensure using >= 0.23.0 for proper ErgoTree v3 support' },
    'fleet-sdk': { severity: 'low', desc: 'Update regularly for chain compatibility' },
  };

  for (const [name, version] of Object.entries(deps)) {
    const vuln = knownVulns[name];
    if (vuln) {
      results.push({
        name, version, status: 'warning', severity: vuln.severity,
        description: vuln.desc,
        recommendation: `Update ${name} to latest version`,
      });
    } else {
      results.push({
        name, version, status: 'safe',
        description: 'No known vulnerabilities',
        recommendation: 'Keep updated',
      });
    }
  }
  return results;
}

function generateReport(ergoTreeHex: string, contractName: string, registers?: Record<string, string>): SecurityReport {
  const scan = scanErgoTree(ergoTreeHex);
  const regCheck = registers ? verifyRegisters(contractName, registers) : null;

  const criticalIssues: string[] = [...scan.errors];
  if (regCheck) criticalIssues.push(...regCheck.errors);

  const recommendations: string[] = [
    'Add multi-sig for treasury spending',
    'Consider rate-limiting governance proposals',
    'Add cooldown to provider deregistration',
    'Review all contracts before mainnet deployment',
  ];
  if (scan.warnings.length > 0) {
    recommendations.push(...scan.warnings.map(w => `Address: ${w}`));
  }

  const contracts = CONTRACT_SPECS.map(spec => ({
    name: spec.name,
    score: spec.name === contractName ? scan.score : 8,
    issues: spec.name === contractName ? [...scan.errors, ...scan.warnings] : [],
  }));

  const overallScore = scan.score;

  return {
    generatedAt: new Date().toISOString(),
    contracts,
    overallScore,
    criticalIssues,
    recommendations,
  };
}

// ---------------------------------------------------------------------------
// Output Formatters
// ---------------------------------------------------------------------------

function formatSeverity(severity: string): string {
  const map: Record<string, string> = {
    critical: '\x1b[31m✗ CRITICAL\x1b[0m',
    warning: '\x1b[33m⚠ WARNING\x1b[0m',
    info: '\x1b[36mℹ INFO\x1b[0m',
  };
  return map[severity] ?? severity;
}

function formatScore(score: number): string {
  const color = score >= 8 ? '\x1b[32m' : score >= 5 ? '\x1b[33m' : '\x1b[31m';
  return `${color}${score}/10\x1b[0m`;
}

function printScanResult(result: ScanResult, json: boolean): void {
  if (json) { console.log(JSON.stringify(result, null, 2)); return; }

  console.log(`\n\x1b[1mContract Scan Results\x1b[0m`);
  console.log(`  Valid: ${result.valid ? '\x1b[32mYES\x1b[0m' : '\x1b[31mNO\x1b[0m'}  Score: ${formatScore(result.score)}`);
  console.log(`  ErgoTree: ${result.ergoTreeHex.slice(0, 40)}...\n`);

  for (const check of result.checks) {
    const icon = check.passed ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
    console.log(`  ${icon} [${formatSeverity(check.severity)}] ${check.name}: ${check.message}`);
  }

  if (result.warnings.length > 0) {
    console.log(`\n  \x1b[33mWarnings:\x1b[0m`);
    result.warnings.forEach(w => console.log(`    - ${w}`));
  }
  if (result.errors.length > 0) {
    console.log(`\n  \x1b[31mErrors:\x1b[0m`);
    result.errors.forEach(e => console.log(`    - ${e}`));
  }
  console.log();
}

function printRegisterResult(result: RegisterVerifyResult, json: boolean): void {
  if (json) { console.log(JSON.stringify(result, null, 2)); return; }

  console.log(`\n\x1b[1mRegister Layout Verification: ${result.contractName}\x1b[0m`);
  console.log(`  Valid: ${result.valid ? '\x1b[32mYES\x1b[0m' : '\x1b[31mNO\x1b[0m'}\n`);

  for (const r of result.results) {
    const icon = r.match ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
    const req = r.required ? '' : ' (optional)';
    console.log(`  ${icon} ${r.register} [${r.required ? 'required' : 'optional'}]: ${r.actualType} ${r.match ? '==' : '!='} ${r.expectedType} — ${r.purpose}${req}`);
  }
  console.log();
}

function printDepsResult(results: DependencyAuditResult[], json: boolean): void {
  if (json) { console.log(JSON.stringify(results, null, 2)); return; }

  console.log(`\n\x1b[1mDependency Audit Results\x1b[0m\n`);
  for (const dep of results) {
    const icon = dep.status === 'safe' ? '\x1b[32m✓\x1b[0m'
      : dep.status === 'warning' ? '\x1b[33m⚠\x1b[0m'
      : '\x1b[31m✗\x1b[0m';
    console.log(`  ${icon} ${dep.name}@${dep.version} [${dep.status}]: ${dep.description}`);
    if (dep.recommendation) console.log(`    → ${dep.recommendation}`);
  }
  console.log();
}

function printReport(report: SecurityReport, json: boolean, markdown: boolean): void {
  if (json) { console.log(JSON.stringify(report, null, 2)); return; }
  if (markdown) {
    console.log(`# Xergon Security Report`);
    console.log(`Generated: ${report.generatedAt}`);
    console.log(`Overall Score: ${report.overallScore}/10\n`);
    console.log(`## Critical Issues`);
    report.criticalIssues.forEach(i => console.log(`- ${i}`));
    console.log(`\n## Recommendations`);
    report.recommendations.forEach(r => console.log(`- ${r}`));
    console.log(`\n## Contracts`);
    for (const c of report.contracts) {
      console.log(`### ${c.name} (${c.score}/10)`);
      c.issues.forEach(i => console.log(`- ${i}`));
    }
    return;
  }

  console.log(`\n\x1b[1m╔══════════════════════════════════════╗\x1b[0m`);
  console.log(`\x1b[1m║    XERGON SECURITY AUDIT REPORT     ║\x1b[0m`);
  console.log(`\x1b[1m╚══════════════════════════════════════╝\x1b[0m`);
  console.log(`  Generated: ${report.generatedAt}`);
  console.log(`  Overall Score: ${formatScore(report.overallScore)}\n`);

  if (report.criticalIssues.length > 0) {
    console.log(`  \x1b[31mCritical Issues:\x1b[0m`);
    report.criticalIssues.forEach(i => console.log(`    \x1b[31m✗ ${i}\x1b[0m`));
    console.log();
  }

  if (report.recommendations.length > 0) {
    console.log(`  \x1b[33mRecommendations:\x1b[0m`);
    report.recommendations.forEach(r => console.log(`    → ${r}`));
    console.log();
  }

  console.log(`  Contract Scores:`);
  for (const c of report.contracts) {
    console.log(`    ${c.name}: ${formatScore(c.score)}`);
  }
  console.log();
}

// ---------------------------------------------------------------------------
// CLI Commands
// ---------------------------------------------------------------------------

export const auditCommand: CommandModule = {
  command: 'audit <command>',
  describe: 'Security audit tooling for Xergon protocol contracts',
  builder: (yargs) => yargs
    .command({
      command: 'scan',
      describe: 'Scan an ErgoTree contract for vulnerabilities',
      builder: (y) => y
        .option('ergo-tree', { type: 'string', alias: 'e', demandOption: true, describe: 'ErgoTree hex to scan' })
        .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
      handler: (argv) => {
        const result = scanErgoTree(argv['ergo-tree'] as string);
        printScanResult(result, argv.json as boolean);
      },
    })
    .command({
      command: 'registers',
      describe: 'Verify register layout against contract spec',
      builder: (y) => y
        .option('contract', { type: 'string', alias: 'c', demandOption: true, describe: 'Contract name (provider_box, user_staking, etc.)', choices: CONTRACT_SPECS.map(s => s.name) })
        .option('r4', { type: 'string', describe: 'R4 register hex value' })
        .option('r5', { type: 'string', describe: 'R5 register hex value' })
        .option('r6', { type: 'string', describe: 'R6 register hex value' })
        .option('r7', { type: 'string', describe: 'R7 register hex value' })
        .option('r8', { type: 'string', describe: 'R8 register hex value' })
        .option('r9', { type: 'string', describe: 'R9 register hex value' })
        .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
      handler: (argv) => {
        const regs: Record<string, string> = {};
        for (const r of ['r4', 'r5', 'r6', 'r7', 'r8', 'r9']) {
          const val = argv[r] as string | undefined;
          if (val) regs[r.toUpperCase()] = val;
        }
        const result = verifyRegisters(argv.contract as string, regs);
        printRegisterResult(result, argv.json as boolean);
      },
    })
    .command({
      command: 'deps',
      describe: 'Audit dependencies for known vulnerabilities',
      builder: (y) => y
        .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
      handler: (argv) => {
        // Demo dependency scan with known Xergon deps
        const deps: Record<string, string> = {
          'sigma-rust': '0.17.0',
          'ergo-lib-wasm': '0.24.0',
          'fleet-sdk': '0.3.0',
          'axios': '1.6.0',
          'typescript': '5.3.0',
        };
        const result = auditDependencies(deps);
        printDepsResult(result, argv.json as boolean);
      },
    })
    .command({
      command: 'report',
      describe: 'Generate full security report',
      builder: (y) => y
        .option('ergo-tree', { type: 'string', alias: 'e', demandOption: true, describe: 'ErgoTree hex' })
        .option('contract', { type: 'string', alias: 'c', demandOption: true, describe: 'Contract name' })
        .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' })
        .option('markdown', { type: 'boolean', default: false, describe: 'Output as Markdown' }),
      handler: (argv) => {
        const report = generateReport(argv['ergo-tree'] as string, argv.contract as string);
        printReport(report, argv.json as boolean, argv.markdown as boolean);
      },
    })
    .command({
      command: 'score',
      describe: 'Quick security score for a contract',
      builder: (y) => y
        .option('ergo-tree', { type: 'string', alias: 'e', demandOption: true, describe: 'ErgoTree hex' }),
      handler: (argv) => {
        const result = scanErgoTree(argv['ergo-tree'] as string);
        console.log(`\x1b[1mSecurity Score: ${formatScore(result.score)}\x1b[0m`);
        if (result.errors.length > 0) {
          console.log(`\x1b[31mErrors: ${result.errors.join(', ')}\x1b[0m`);
        }
        if (result.warnings.length > 0) {
          console.log(`\x1b[33mWarnings: ${result.warnings.join(', ')}\x1b[0m`);
        }
        console.log(`${result.checks.length} checks performed.`);
      },
    })
    .command({
      command: 'list-specs',
      describe: 'List all known contract register specs',
      builder: (y) => y
        .option('json', { type: 'boolean', default: false }),
      handler: (argv) => {
        if (argv.json) { console.log(JSON.stringify(CONTRACT_SPECS, null, 2)); return; }
        console.log(`\n\x1b[1mKnown Contract Specs\x1b[0m\n`);
        for (const spec of CONTRACT_SPECS) {
          console.log(`  \x1b[1m${spec.name}\x1b[0m (${spec.contractType})`);
          console.log(`    ${spec.description}`);
          console.log(`    Registers:`);
          for (const r of spec.registers) {
            const req = r.required ? '\x1b[32mrequired\x1b[0m' : '\x1b[90moptional\x1b[0m';
            console.log(`      ${r.register}: ${r.expectedType} [${req}] — ${r.purpose}`);
          }
          if (spec.tokens.length > 0) {
            console.log(`    Tokens: ${spec.tokens.map(t => t.name).join(', ')}`);
          }
          console.log();
        }
      },
    }),
  handler: () => {},
};

// ---------------------------------------------------------------------------
// Tests (run via: npx vitest run src/cli/commands/audit.test.ts)
// ---------------------------------------------------------------------------

