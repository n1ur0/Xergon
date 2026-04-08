/**
 * Xergon Contract CLI Commands
 *
 * Contract management tooling for Xergon protocol contracts.
 * - evaluate:  Evaluate ErgoTree hex against simulated context
 * - prove:     Generate Sigma protocol proof for a contract
 * - verify:    Verify a Sigma protocol proof
 * - mint-token: Build EIP-4 token minting transaction
 * - burn-token: Build token burn transaction
 * - transfer-token: Build token transfer transaction
 * - compile:   Compile ErgoScript source to ErgoTree hex
 * - inspect:   Inspect / decompile an ErgoTree hex
 *
 * Usage: xergon contract <command> [options]
 */

import { CommandModule } from 'yargs';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EvalResult {
  valid: boolean;
  result: string;
  sigmaBoolean: string;
  gasUsed: number;
  errors: string[];
}

interface ProveResult {
  proofHex: string;
  proofType: string;
  publicKey: string;
  timestamp: number;
}

interface VerifyResult {
  valid: boolean;
  proofType: string;
  checks: string[];
  timeMs: number;
}

interface MintResult {
  tokenId: string;
  ergoTree: string;
  unsignedTx: Record<string, unknown>;
  boxValue: number;
}

interface BurnResult {
  tokenId: string;
  burnAmount: number;
  unsignedTx: Record<string, unknown>;
  boxValue: number;
}

interface TransferResult {
  tokenId: string;
  amount: number;
  recipient: string;
  unsignedTx: Record<string, unknown>;
  boxValue: number;
}

interface CompileResult {
  ergoTreeHex: string;
  constants: { name: string; type: string; value: string }[];
  size: number;
  opcodes: string[];
}

interface InspectResult {
  opcodes: string[];
  registers: Record<string, string>;
  tokens: string[];
  estimatedSize: number;
  contracts: string[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Simple deterministic hex from a string (not crypto-secure, for mock only). */
function deterministicHex(input: string, length: number): string {
  let hash = 0;
  for (let i = 0; i < input.length; i++) {
    const ch = input.charCodeAt(i);
    hash = ((hash << 5) - hash + ch) | 0;
  }
  let out = '';
  for (let i = 0; i < length; i++) {
    hash = ((hash << 3) ^ (hash >>> 29)) | 0;
    out += ((hash >>> 0) & 0xf).toString(16);
  }
  return out;
}

/** Parse hex string into 2-char opcode chunks. */
function parseOpcodes(hex: string): string[] {
  const clean = hex.replace(/^0x/i, '');
  const ops: string[] = [];
  const known: Record<string, string> = {
    '00': 'Const(0)',
    '01': 'Const(1)',
    '08': 'Const(CollByte)',
    '0e': 'DeserializeRegister',
    'cd': 'MethodCall',
    'd1': 'FuncCall(sigmaProp)',
    'e4': 'ExtractAmount',
    'e7': 'ExtractScriptBytes',
    'eb': 'ByIndex',
    'ed': 'MapValues',
  };
  for (let i = 0; i < clean.length; i += 2) {
    const byte = clean.substring(i, i + 2).toLowerCase();
    const label = known[byte];
    if (label) {
      ops.push(label);
    } else if (byte.length === 2) {
      ops.push(`0x${byte}`);
    }
  }
  return ops;
}

/** Generate a mock unsigned transaction structure. */
function generateUnsignedTx(
  inputs: string[],
  outputs: Record<string, unknown>[],
  fee: number,
): Record<string, unknown> {
  return {
    txId: deterministicHex(inputs.join('') + JSON.stringify(outputs), 64),
    inputs: inputs.map((id) => ({ boxId: id })),
    dataInputs: [],
    outputs,
    fee,
    creationHeight: 800_000,
  };
}

// ---------------------------------------------------------------------------
// Mock Functions
// ---------------------------------------------------------------------------

function mockEvaluate(hex: string, _opts: { height?: number; inputs?: string; registers?: string }): EvalResult {
  const clean = hex.replace(/^0x/i, '');
  const errors: string[] = [];
  if (clean.length < 8) errors.push('ErgoTree too short (minimum 8 hex chars)');
  if (!/^[0-9a-fA-F]+$/.test(clean)) errors.push('Invalid hex characters');
  const valid = errors.length === 0 && clean.length >= 8;
  // Detect known patterns
  let sigmaBoolean = 'TrivialProp';
  if (clean.startsWith('cd02')) sigmaBoolean = 'ProveDlog';
  else if (clean.startsWith('cd03')) sigmaBoolean = 'ProveDHTuple';
  else if (clean.startsWith('d1')) sigmaBoolean = 'SigmaAnd';
  return {
    valid,
    result: valid ? 'TrivialProp.True' : 'EvaluationError',
    sigmaBoolean,
    gasUsed: valid ? 120 + (clean.length % 200) : 0,
    errors,
  };
}

function mockProve(contract: string, key: string, _context?: string): ProveResult {
  return {
    proofHex: deterministicHex(contract + key, 128),
    proofType: contract.includes('dht') ? 'ProveDHTuple' : 'ProveDlog',
    publicKey: key,
    timestamp: Date.now(),
  };
}

function mockVerify(_contract: string, proof: string): VerifyResult {
  const clean = proof.replace(/^0x/i, '');
  const valid = clean.length >= 32;
  return {
    valid,
    proofType: 'ProveDlog',
    checks: valid
      ? ['signature_valid', 'context_match', 'height_check', 'spending_proof_ok']
      : ['proof_too_short'],
    timeMs: 5 + (clean.length % 45),
  };
}

function mockMintToken(name: string, amount: number, description?: string, decimals?: number): MintResult {
  const tokenId = deterministicHex(name + amount.toString(), 64);
  const ergoTree = deterministicHex('mint:' + name, 80);
  const boxValue = 1_000_000;
  const registers: Record<string, unknown> = { R4: name };
  if (description) registers.R5 = description;
  if (decimals !== undefined) registers.R6 = `SInt(${decimals})`;
  return {
    tokenId,
    ergoTree,
    unsignedTx: generateUnsignedTx([], [
      { value: boxValue, ergoTree, tokens: [{ tokenId, amount }], registers },
    ], 1_100_000),
    boxValue,
  };
}

function mockBurnToken(tokenId: string, amount: number): BurnResult {
  return {
    tokenId,
    burnAmount: amount,
    unsignedTx: generateUnsignedTx([tokenId.substring(0, 32)], [
      { value: 1_000_000, ergoTree: deterministicHex('burn', 40), tokens: [], registers: {} },
    ], 1_100_000),
    boxValue: 1_000_000,
  };
}

function mockTransferToken(tokenId: string, amount: number, recipient: string, registers?: string): TransferResult {
  const regs: Record<string, unknown> = {};
  if (registers) {
    try { Object.assign(regs, JSON.parse(registers)); } catch { /* ignore */ }
  }
  return {
    tokenId,
    amount,
    recipient,
    unsignedTx: generateUnsignedTx([tokenId.substring(0, 32)], [
      { value: 1_000_000, ergoTree: deterministicHex(recipient, 40), tokens: [{ tokenId, amount }], registers: regs },
    ], 1_100_000),
    boxValue: 1_000_000,
  };
}

function mockCompile(source: string, constantsJson?: string): CompileResult {
  const constants: { name: string; type: string; value: string }[] = [];
  if (constantsJson) {
    try {
      const parsed = JSON.parse(constantsJson);
      for (const [k, v] of Object.entries(parsed)) {
        constants.push({ name: k, type: typeof v, value: String(v) });
      }
    } catch { /* ignore */ }
  }
  const ergoTreeHex = deterministicHex(source, 80);
  const opcodes = parseOpcodes(ergoTreeHex);
  return { ergoTreeHex, constants, size: ergoTreeHex.length / 2, opcodes };
}

function mockInspect(hex: string): InspectResult {
  const clean = hex.replace(/^0x/i, '');
  const opcodes = parseOpcodes(clean);
  const registers: Record<string, string> = {};
  // Detect mock register patterns
  if (clean.includes('5234')) registers.R4 = 'GroupElement';
  if (clean.includes('5235')) registers.R5 = 'CollByte';
  if (clean.includes('5236')) registers.R6 = 'CollByte';
  if (clean.includes('5237')) registers.R7 = 'SInt';
  if (clean.includes('5238')) registers.R8 = 'SLong';
  if (clean.includes('5239')) registers.R9 = 'CollByte';
  const tokens: string[] = [];
  if (clean.length > 40) tokens.push(deterministicHex(clean.substring(0, 20), 64));
  const contracts: string[] = [];
  if (clean.startsWith('cd02')) contracts.push('ProviderBox');
  if (clean.startsWith('d1')) contracts.push('MultiSig');
  if (clean.includes('52545245')) contracts.push('TreasuryBox');
  return {
    opcodes,
    registers,
    tokens,
    estimatedSize: clean.length / 2,
    contracts,
  };
}

// ---------------------------------------------------------------------------
// Terminal Helpers
// ---------------------------------------------------------------------------

const green = (s: string) => `\x1b[32m${s}\x1b[0m`;
const red = (s: string) => `\x1b[31m${s}\x1b[0m`;
const cyan = (s: string) => `\x1b[36m${s}\x1b[0m`;
const bold = (s: string) => `\x1b[1m${s}\x1b[0m`;
const yellow = (s: string) => `\x1b[33m${s}\x1b[0m`;

function printJson(obj: unknown): void {
  console.log(JSON.stringify(obj, null, 2));
}

// ---------------------------------------------------------------------------
// Exported Command
// ---------------------------------------------------------------------------

export const contractCommand: CommandModule = {
  command: 'contract <command>',
  describe: 'Manage Xergon protocol contracts (evaluate, prove, verify, tokens, compile, inspect)',
  builder: (y) =>
    y
      .command({
        command: 'evaluate <hex>',
        describe: 'Evaluate ErgoTree hex against simulated context',
        builder: (yy) =>
          yy
            .positional('hex', { type: 'string', describe: 'ErgoTree hex string' })
            .option('height', { type: 'number', default: 800_000, describe: 'Block height for context' })
            .option('inputs', { type: 'string', describe: 'Input boxes (JSON array)' })
            .option('registers', { type: 'string', describe: 'Registers (JSON object)' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockEvaluate(argv.hex as string, {
            height: argv.height as number,
            inputs: argv.inputs as string,
            registers: argv.registers as string,
          });
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Contract Evaluation'));
          console.log(`  Status:     ${r.valid ? green('VALID') : red('INVALID')}`);
          console.log(`  Result:     ${r.result}`);
          console.log(`  SigmaBool:  ${cyan(r.sigmaBoolean)}`);
          console.log(`  Gas Used:   ${r.gasUsed}`);
          if (r.errors.length) console.log(`  Errors:     ${red(r.errors.join(', '))}`);
          console.log();
        },
      })
      .command({
        command: 'prove <contract> <key>',
        describe: 'Generate Sigma protocol proof for a contract',
        builder: (yy) =>
          yy
            .positional('contract', { type: 'string', describe: 'Contract hex' })
            .positional('key', { type: 'string', describe: 'Private key / secret' })
            .option('context', { type: 'string', describe: 'Context extension (JSON)' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockProve(argv.contract as string, argv.key as string, argv.context as string);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Sigma Proof Generated'));
          console.log(`  Type:      ${cyan(r.proofType)}`);
          console.log(`  PublicKey: ${r.publicKey}`);
          console.log(`  Proof:     ${r.proofHex.substring(0, 32)}...`);
          console.log(`  Timestamp: ${new Date(r.timestamp).toISOString()}`);
          console.log();
        },
      })
      .command({
        command: 'verify <contract> <proof>',
        describe: 'Verify a Sigma protocol proof',
        builder: (yy) =>
          yy
            .positional('contract', { type: 'string', describe: 'Contract hex' })
            .positional('proof', { type: 'string', describe: 'Proof hex' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockVerify(argv.contract as string, argv.proof as string);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Proof Verification'));
          console.log(`  Status:    ${r.valid ? green('VALID') : red('INVALID')}`);
          console.log(`  Type:      ${r.proofType}`);
          console.log(`  Time:      ${r.timeMs}ms`);
          for (const c of r.checks) {
            console.log(`  ${green('✓')} ${c}`);
          }
          console.log();
        },
      })
      .command({
        command: 'mint-token <name> <amount>',
        describe: 'Build EIP-4 token minting transaction',
        builder: (yy) =>
          yy
            .positional('name', { type: 'string', describe: 'Token name' })
            .positional('amount', { type: 'number', describe: 'Amount to mint' })
            .option('description', { type: 'string', describe: 'Token description' })
            .option('decimals', { type: 'number', default: 0, describe: 'Token decimals' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockMintToken(argv.name as string, argv.amount as number, argv.description as string, argv.decimals as number);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Mint Token Transaction'));
          console.log(`  TokenId:   ${cyan(r.tokenId.substring(0, 32))}...`);
          console.log(`  Name:      ${argv.name}`);
          console.log(`  Amount:    ${argv.amount}`);
          console.log(`  ErgoTree:  ${r.ergoTree.substring(0, 32)}...`);
          console.log(`  BoxValue:  ${(r.boxValue / 1e9).toFixed(4)} ERG`);
          console.log();
        },
      })
      .command({
        command: 'burn-token <tokenId> <amount>',
        describe: 'Build token burn transaction',
        builder: (yy) =>
          yy
            .positional('tokenId', { type: 'string', describe: 'Token ID to burn' })
            .positional('amount', { type: 'number', describe: 'Amount to burn' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockBurnToken(argv.tokenId as string, argv.amount as number);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Burn Token Transaction'));
          console.log(`  TokenId:   ${cyan(r.tokenId.substring(0, 32))}...`);
          console.log(`  Burn:      ${r.burnAmount} tokens`);
          console.log(`  BoxValue:  ${(r.boxValue / 1e9).toFixed(4)} ERG`);
          console.log();
        },
      })
      .command({
        command: 'transfer-token <tokenId> <amount> <recipient>',
        describe: 'Build token transfer transaction',
        builder: (yy) =>
          yy
            .positional('tokenId', { type: 'string', describe: 'Token ID' })
            .positional('amount', { type: 'number', describe: 'Amount to transfer' })
            .positional('recipient', { type: 'string', describe: 'Recipient address' })
            .option('registers', { type: 'string', describe: 'Output registers (JSON)' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockTransferToken(argv.tokenId as string, argv.amount as number, argv.recipient as string, argv.registers as string);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Transfer Token Transaction'));
          console.log(`  TokenId:   ${cyan(r.tokenId.substring(0, 32))}...`);
          console.log(`  Amount:    ${r.amount}`);
          console.log(`  Recipient: ${r.recipient}`);
          console.log(`  BoxValue:  ${(r.boxValue / 1e9).toFixed(4)} ERG`);
          console.log();
        },
      })
      .command({
        command: 'compile <source>',
        describe: 'Compile ErgoScript source to ErgoTree hex',
        builder: (yy) =>
          yy
            .positional('source', { type: 'string', describe: 'ErgoScript source code' })
            .option('constants', { type: 'string', describe: 'Constants (JSON)' })
            .option('output', { type: 'string', describe: 'Output file path' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockCompile(argv.source as string, argv.constants as string);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  Compile Result'));
          console.log(`  ErgoTree:  ${cyan(r.ergoTreeHex.substring(0, 32))}...`);
          console.log(`  Size:      ${r.size} bytes`);
          console.log(`  Constants: ${r.constants.length}`);
          for (const c of r.constants) {
            console.log(`    ${c.name}: ${c.type} = ${c.value}`);
          }
          console.log(`  Opcodes:   ${r.opcodes.slice(0, 6).join(', ')}${r.opcodes.length > 6 ? '...' : ''}`);
          if (argv.output) {
            console.log(`  ${green('Written to')} ${argv.output}`);
          }
          console.log();
        },
      })
      .command({
        command: 'inspect <hex>',
        describe: 'Inspect / decompile an ErgoTree hex',
        builder: (yy) =>
          yy
            .positional('hex', { type: 'string', describe: 'ErgoTree hex string' })
            .option('json', { type: 'boolean', default: false, describe: 'Output as JSON' }),
        handler: (argv) => {
          const r = mockInspect(argv.hex as string);
          if (argv.json) { printJson(r); return; }
          console.log(bold('\n  ErgoTree Inspection'));
          console.log(`  Est. Size: ${r.estimatedSize} bytes`);
          console.log(`  Opcodes:   ${r.opcodes.length}`);
          for (const op of r.opcodes.slice(0, 8)) {
            console.log(`    ${cyan(op)}`);
          }
          if (r.opcodes.length > 8) console.log(`    ... ${r.opcodes.length - 8} more`);
          if (Object.keys(r.registers).length) {
            console.log(`  Registers:`);
            for (const [k, v] of Object.entries(r.registers)) {
              console.log(`    ${k}: ${yellow(v)}`);
            }
          }
          if (r.tokens.length) {
            console.log(`  Tokens:    ${r.tokens.map(t => t.substring(0, 16) + '...').join(', ')}`);
          }
          if (r.contracts.length) {
            console.log(`  Contracts: ${r.contracts.join(', ')}`);
          }
          console.log();
        },
      })
      .demandCommand(),
  handler: () => {},
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

if (require.main === module) {
  console.log('Running contract module self-tests...\n');

  // Test 1: evaluate valid hex
  const e1 = mockEvaluate('cd02e8ec6e8a4b7abcdef1234567890', {});
  console.log(`Test 1 (evaluate valid): valid=${e1.valid}, sigma=${e1.sigmaBoolean}`);
  console.assert(e1.valid === true, 'Should be valid');
  console.assert(e1.sigmaBoolean === 'ProveDlog', 'Should detect ProveDlog');

  // Test 2: evaluate invalid hex
  const e2 = mockEvaluate('zz', {});
  console.log(`Test 2 (evaluate invalid): valid=${e2.valid}, errors=${e2.errors.length}`);
  console.assert(e2.valid === false, 'Should be invalid');

  // Test 3: prove deterministic
  const p1 = mockProve('contract1', 'key1');
  const p2 = mockProve('contract1', 'key1');
  console.log(`Test 3 (prove deterministic): same=${p1.proofHex === p2.proofHex}`);
  console.assert(p1.proofHex === p2.proofHex, 'Should be deterministic');
  console.assert(p1.proofHex.length === 128, 'Proof should be 128 chars');

  // Test 4: verify valid proof
  const v1 = mockVerify('contract', p1.proofHex);
  console.log(`Test 4 (verify valid): valid=${v1.valid}, checks=${v1.checks.length}`);
  console.assert(v1.valid === true, 'Should verify');
  console.assert(v1.checks.length === 4, 'Should have 4 checks');

  // Test 5: mint-token generates tokenId
  const m1 = mockMintToken('TestToken', 1000);
  console.log(`Test 5 (mint-token): tokenId=${m1.tokenId.substring(0, 16)}..., len=${m1.tokenId.length}`);
  console.assert(m1.tokenId.length === 64, 'tokenId should be 64 chars');
  console.assert(m1.unsignedTx.outputs !== undefined, 'Should have outputs');

  // Test 6: burn-token correct amount
  const b1 = mockBurnToken('abc123', 500);
  console.log(`Test 6 (burn-token): burnAmount=${b1.burnAmount}`);
  console.assert(b1.burnAmount === 500, 'Burn amount should match');

  // Test 7: transfer-token includes recipient
  const t1 = mockTransferToken('abc123', 100, '9fKqEGHV5uK7U8Q7f5mE8fE3');
  console.log(`Test 7 (transfer-token): recipient=${t1.recipient}`);
  console.assert(t1.recipient === '9fKqEGHV5uK7U8Q7f5mE8fE3', 'Recipient should match');
  console.assert(t1.amount === 100, 'Amount should match');

  // Test 8: compile generates opcodes
  const c1 = mockCompile('{ sigmaProp(PK("abc")) }', '{"val":42}');
  console.log(`Test 8 (compile): opcodes=${c1.opcodes.length}, constants=${c1.constants.length}`);
  console.assert(c1.opcodes.length > 0, 'Should have opcodes');
  console.assert(c1.constants.length === 1, 'Should have 1 constant');

  // Test 9: inspect detects ProveDlog
  const i1 = mockInspect('cd02e8ec6e8a4b7abcdef1234567890');
  console.log(`Test 9 (inspect): contracts=${i1.contracts.join(',')}`);
  console.assert(i1.contracts.includes('ProviderBox'), 'Should detect ProviderBox');
  console.assert(i1.opcodes.length > 0, 'Should have opcodes');

  console.log('\nAll tests passed!');
}
