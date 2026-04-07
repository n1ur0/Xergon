/**
 * `xergon rent` CLI -- Storage rent inspection, consolidation, and box migration.
 *
 * Ergo storage rent: boxes older than 4 years (1,051,200 blocks) can be spent
 * by miners. Min box value = 360 nanoERG/byte. Tokens cannot pay rent -- only ERG.
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';

// ─── Constants ───────────────────────────────────────────────────────
const RENT_THRESHOLD_BLOCKS = 1_051_200;
const NANOERG_PER_BYTE = 360;
const NANOERG_PER_ERG = 1_000_000_000n;
const BLOCKS_PER_DAY = 720;
const RENT_ACTIVATION_HEIGHT = 1_051_200; // July 20, 2023

// ─── Types ───────────────────────────────────────────────────────────

type RiskLevel = 'emergency' | 'critical' | 'warning' | 'caution' | 'safe';

interface BoxRentInfo {
  boxId: string;
  boxType: string;
  address: string;
  valueNanoerg: bigint;
  creationHeight: number;
  currentHeight: number;
  ageBlocks: number;
  byteSize: number;
  minBoxValue: bigint;
  daysUntilDeadline: number;
  riskLevel: RiskLevel;
  tokens: { tokenId: string; amount: bigint }[];
}

interface ConsolidationCandidate {
  boxId: string;
  address: string;
  valueNanoerg: bigint;
  ageBlocks: number;
  byteSize: number;
  riskLevel: RiskLevel;
}

interface ConsolidationPlan {
  address: string;
  boxesToConsolidate: string[];
  totalInputValue: bigint;
  estimatedFee: bigint;
  estimatedOutputValue: bigint;
  rentSavedBoxes: number;
  priority: RiskLevel;
}

interface RentEstimateResult {
  boxId: string;
  byteSize: number;
  currentValue: bigint;
  minValue: bigint;
  deficit: bigint;
  ageBlocks: number;
  daysUntilRent: number;
  annualRentCost: bigint;
  riskLevel: RiskLevel;
  recommendations: string[];
}

interface RentBudgetSummary {
  totalBoxes: number;
  safeBoxes: number;
  atRiskBoxes: number;
  emergencyBoxes: number;
  totalValueErg: string;
  totalDeficitErg: string;
  ergNeededForProtection: string;
  estimatedAnnualRentErg: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────

function nanoergToErg(n: bigint | number): string {
  const v = typeof n === 'bigint' ? n : BigInt(n);
  const whole = v / NANOERG_PER_ERG;
  const frac = v % NANOERG_PER_ERG;
  return `${whole}.${frac.toString().padStart(9, '0').replace(/0+$/, '')}`;
}

function computeRiskLevel(daysUntil: number): RiskLevel {
  if (daysUntil <= 30) return 'emergency';
  if (daysUntil <= 90) return 'critical';
  if (daysUntil <= 365) return 'warning';
  if (daysUntil <= 1095) return 'caution';
  return 'safe';
}

function riskColor(level: RiskLevel): string {
  const colors: Record<RiskLevel, string> = {
    emergency: '\x1b[31m\x1b[1m',  // red bold
    critical: '\x1b[33m\x1b[1m',   // yellow bold
    warning: '\x1b[38;5;208m',      // orange
    caution: '\x1b[36m',            // cyan
    safe: '\x1b[32m',               // green
  };
  return colors[level] || '';
}

function resetColor(): string {
  return '\x1b[0m';
}

function riskBadge(level: RiskLevel): string {
  const labels: Record<RiskLevel, string> = {
    emergency: 'EMERGENCY',
    critical: 'CRITICAL ',
    warning: 'WARNING  ',
    caution: 'CAUTION  ',
    safe: 'SAFE     ',
  };
  return `${riskColor(level)}${labels[level]}${resetColor()}`;
}

function computeMinBoxValue(byteSize: number): bigint {
  return BigInt(NANOERG_PER_BYTE * byteSize);
}

function computeDaysUntilRent(ageBlocks: number): number {
  const remaining = RENT_THRESHOLD_BLOCKS - ageBlocks;
  return Math.max(0, remaining) / BLOCKS_PER_DAY;
}

function computeAnnualRent(byteSize: number): bigint {
  // Approximate: each 4-year cycle costs ~boxSize * 360 nanoERG
  const perCycle = computeMinBoxValue(byteSize);
  return perCycle / 4n; // annualized
}

// ─── Mock data for demo (in production, queries node API) ───────────

function getMockBoxes(): BoxRentInfo[] {
  const height = RENT_ACTIVATION_HEIGHT + 1_000_000; // ~3.8 years after activation
  return [
    { boxId: 'a1b2c3d4e5f6...old1', boxType: 'Provider', address: '9hPU9YXhJ5oJ3k1', valueNanoerg: BigInt(50_000_000), creationHeight: RENT_ACTIVATION_HEIGHT, currentHeight: height, ageBlocks: 1_000_000, byteSize: 320, daysUntilDeadline: computeDaysUntilRent(1_000_000), riskLevel: computeRiskLevel(computeDaysUntilRent(1_000_000)), tokens: [] },
    { boxId: 'f6e5d4c3b2a1...old2', boxType: 'Treasury', address: '9hPU9YXhJ5oJ3k2', valueNanoerg: BigInt(500_000_000), creationHeight: RENT_ACTIVATION_HEIGHT + 100_000, currentHeight: height, ageBlocks: 900_000, byteSize: 450, daysUntilDeadline: computeDaysUntilRent(900_000), riskLevel: computeRiskLevel(computeDaysUntilRent(900_000)), tokens: [{ tokenId: 'token-abc', amount: BigInt(1000) }] },
    { boxId: '112233445566...new1', boxType: 'Provider', address: '9hPU9YXhJ5oJ3k3', valueNanoerg: BigInt(1_000_000_000), creationHeight: height - 100_000, currentHeight: height, ageBlocks: 100_000, byteSize: 280, daysUntilDeadline: computeDaysUntilRent(100_000), riskLevel: computeRiskLevel(computeDaysUntilRent(100_000)), tokens: [] },
    { boxId: '778899aabbcc...new2', boxType: 'Settlement', address: '9hPU9YXhJ5oJ3k1', valueNanoerg: BigInt(200_000_000), creationHeight: height - 50_000, currentHeight: height, ageBlocks: 50_000, byteSize: 200, daysUntilDeadline: computeDaysUntilRent(50_000), riskLevel: computeRiskLevel(computeDaysUntilRent(50_000)), tokens: [] },
    { boxId: 'ddeeff001122...dust1', boxType: 'Dust', address: '9hPU9YXhJ5oJ3k1', valueNanoerg: BigInt(72_000), creationHeight: RENT_ACTIVATION_HEIGHT, currentHeight: height, ageBlocks: 1_000_000, byteSize: 200, daysUntilDeadline: computeDaysUntilRent(1_000_000), riskLevel: computeRiskLevel(computeDaysUntilRent(1_000_000)), tokens: [] },
    { boxId: '334455667788...dust2', boxType: 'Dust', address: '9hPU9YXhJ5oJ3k1', valueNanoerg: BigInt(36_000), creationHeight: RENT_ACTIVATION_HEIGHT + 200_000, currentHeight: height, ageBlocks: 800_000, byteSize: 100, daysUntilDeadline: computeDaysUntilRent(800_000), riskLevel: computeRiskLevel(computeDaysUntilRent(800_000)), tokens: [] },
  ];
}

// ─── Formatters ──────────────────────────────────────────────────────

function formatStatusTable(boxes: BoxRentInfo[], json: boolean): string {
  if (json) return JSON.stringify(boxes, (_, v) => typeof v === 'bigint' ? v.toString() : v, 2);

  const lines: string[] = [];
  lines.push('');
  lines.push('  Storage Rent Status');
  lines.push('  ──────────────────────────────────────────────────────────────────────────────────────────────────');
  lines.push(`  ${'Box ID'.padEnd(22)} ${'Type'.padEnd(10)} ${'Risk'.padEnd(11)} ${'Age'.padEnd(12)} ${'Deadline'.padEnd(12)} ${'Value (ERG)'.padEnd(14)} ${'Min Value'.padEnd(14)} ${'Tokens'}`);
  lines.push('  ──────────────────────────────────────────────────────────────────────────────────────────────────');

  // Sort by risk (most urgent first)
  const riskOrder: Record<RiskLevel, number> = { emergency: 0, critical: 1, warning: 2, caution: 3, safe: 4 };
  const sorted = [...boxes].sort((a, b) => riskOrder[a.riskLevel] - riskOrder[b.riskLevel]);

  for (const box of sorted) {
    const badge = riskBadge(box.riskLevel);
    const ageYears = (box.ageBlocks / BLOCKS_PER_DAY / 365).toFixed(1);
    const deadline = box.daysUntilDeadline < 365
      ? `${Math.floor(box.daysUntilDeadline)} days`
      : `${(box.daysUntilDeadline / 365).toFixed(1)} years`;
    const tokenCount = box.tokens.length > 0 ? `${box.tokens.length} token(s)` : '-';

    lines.push(
      `  ${box.boxId.padEnd(22)} ${box.boxType.padEnd(10)} ${badge} ${(`${ageYears} yrs`).padEnd(12)} ${deadline.padEnd(12)} ${nanoergToErg(box.valueNanoerg).padEnd(14)} ${nanoergToErg(box.minBoxValue).padEnd(14)} ${tokenCount}`
    );
  }

  const emergency = boxes.filter(b => b.riskLevel === 'emergency').length;
  const critical = boxes.filter(b => b.riskLevel === 'critical').length;
  const safe = boxes.filter(b => b.riskLevel === 'safe').length;

  lines.push('  ──────────────────────────────────────────────────────────────────────────────────────────────────');
  lines.push(`  Total: ${boxes.length} boxes | ${riskBadge('emergency')}: ${emergency} | ${riskBadge('critical')}: ${critical} | ${riskBadge('safe')}: ${safe}`);
  if (emergency > 0) {
    lines.push(`  ${riskColor('emergency')}WARNING: ${emergency} box(es) within 30 days of rent deadline! Immediate action required.${resetColor()}`);
  }
  lines.push('');
  return lines.join('\n');
}

function formatEstimate(est: RentEstimateResult, json: boolean): string {
  if (json) return JSON.stringify(est, (_, v) => typeof v === 'bigint' ? v.toString() : v, 2);

  const lines: string[] = [];
  lines.push('');
  lines.push(`  Rent Estimate for ${est.boxId}`);
  lines.push('  ─────────────────────────────────────────');
  lines.push(`  Box size:         ${est.byteSize} bytes`);
  lines.push(`  Current value:    ${nanoergToErg(est.currentValue)} ERG`);
  lines.push(`  Min box value:    ${nanoergToErg(est.minValue)} ERG (${NANOERG_PER_BYTE} nanoERG/byte)`);
  lines.push(`  Value deficit:    ${est.deficit < 0n ? riskColor('emergency') + nanoergToErg(-est.deficit) + ' ERG (UNDERFUNDED)' + resetColor() : nanoergToErg(est.deficit) + ' ERG (OK)'}`);
  lines.push(`  Box age:          ${(est.ageBlocks / BLOCKS_PER_DAY / 365).toFixed(1)} years (${est.ageBlocks.toLocaleString()} blocks)`);
  lines.push(`  Days until rent:  ${est.daysUntilDeadline < 30 ? riskColor('emergency') : ''}${Math.floor(est.daysUntilDeadline)} days${est.daysUntilDeadline < 30 ? resetColor() : ''}`);
  lines.push(`  Annual rent cost: ${nanoergToErg(est.annualRentCost)} ERG`);
  lines.push(`  Risk level:       ${riskBadge(est.riskLevel)}`);
  if (est.recommendations.length > 0) {
    lines.push('  Recommendations:');
    for (const rec of est.recommendations) {
      lines.push(`    - ${rec}`);
    }
  }
  lines.push('');
  return lines.join('\n');
}

function formatConsolidationPlan(plan: ConsolidationPlan, json: boolean): string {
  if (json) return JSON.stringify(plan, (_, v) => typeof v === 'bigint' ? v.toString() : v, 2);

  const lines: string[] = [];
  lines.push('');
  lines.push(`  Consolidation Plan`);
  lines.push('  ─────────────────────────────────────────');
  lines.push(`  Address:          ${plan.address}`);
  lines.push(`  Boxes to merge:   ${plan.boxesToConsolidate.length}`);
  lines.push(`  Total input:      ${nanoergToErg(plan.totalInputValue)} ERG`);
  lines.push(`  Estimated fee:    ${nanoergToErg(plan.estimatedFee)} ERG`);
  lines.push(`  Estimated output: ${nanoergToErg(plan.estimatedOutputValue)} ERG`);
  lines.push(`  Rent saved:       ${plan.rentSavedBoxes} fewer boxes to protect`);
  lines.push(`  Priority:         ${riskBadge(plan.priority)}`);
  lines.push('  Boxes:');
  for (const boxId of plan.boxesToConsolidate) {
    lines.push(`    - ${boxId}`);
  }
  lines.push('');
  return lines.join('\n');
}

function formatBudget(summary: RentBudgetSummary, json: boolean): string {
  if (json) return JSON.stringify(summary, null, 2);

  const lines: string[] = [];
  lines.push('');
  lines.push('  Rent Protection Budget Summary');
  lines.push('  ─────────────────────────────────────────');
  lines.push(`  Total boxes:              ${summary.totalBoxes}`);
  lines.push(`  Safe boxes:               ${summary.safeBoxes}`);
  lines.push(`  At-risk boxes:            ${summary.atRiskBoxes}`);
  lines.push(`  Emergency boxes:          ${summary.emergencyBoxes}`);
  lines.push(`  Total box value:          ${summary.totalValueErg} ERG`);
  lines.push(`  Total deficit:            ${summary.totalDeficitErg} ERG`);
  lines.push(`  ERG needed for protection: ${summary.ergNeededForProtection}`);
  lines.push(`  Estimated annual rent:    ${summary.estimatedAnnualRentErg}`);
  if (summary.emergencyBoxes > 0) {
    lines.push(`  ${riskColor('emergency')}ALERT: ${summary.emergencyBoxes} boxes need immediate funding!${resetColor()}`);
  }
  lines.push('');
  return lines.join('\n');
}

// ─── Subcommand logic ────────────────────────────────────────────────

function runStatus(json: boolean): string {
  const boxes = getMockBoxes();
  return formatStatusTable(boxes, json);
}

function runEstimate(boxId: string, json: boolean): string {
  const boxes = getMockBoxes();
  const box = boxes.find(b => b.boxId.startsWith(boxId));
  if (!box) return `  Error: Box ${boxId} not found in tracked boxes`;

  const deficit = box.valueNanoerg - box.minBoxValue;
  const annualRent = computeAnnualRent(box.byteSize);
  const recommendations: string[] = [];

  if (deficit < 0n) recommendations.push(`Add at least ${nanoergToErg(-deficit)} ERG to meet minimum box value`);
  if (box.daysUntilDeadline < 90) recommendations.push('Consider migrating this box to reset its creation height');
  if (box.daysUntilDeadline < 30) recommendations.push('URGENT: Box is within 30 days of rent deadline. Migrate immediately.');
  if (box.tokens.length > 0 && box.valueNanoerg < box.minBoxValue * 2n) {
    recommendations.push('Box holds tokens but has minimal ERG. Add more ERG to protect tokens from rent collection.');
  }
  if (recommendations.length === 0) {
    recommendations.push('Box is adequately funded and not approaching rent deadline.');
  }

  const est: RentEstimateResult = {
    boxId: box.boxId,
    byteSize: box.byteSize,
    currentValue: box.valueNanoerg,
    minValue: box.minBoxValue,
    deficit,
    ageBlocks: box.ageBlocks,
    daysUntilRent: box.daysUntilDeadline,
    annualRentCost: annualRent,
    riskLevel: box.riskLevel,
    recommendations,
  };
  return formatEstimate(est, json);
}

function runScan(address: string, json: boolean): string {
  const boxes = getMockBoxes().filter(b => b.address === address || !address);
  const atRisk = boxes.filter(b => b.riskLevel === 'emergency' || b.riskLevel === 'critical');
  const dust = boxes.filter(b => b.valueNanoerg < b.minBoxValue && b.tokens.length === 0);

  if (json) {
    return JSON.stringify({ address: address || 'all', totalBoxes: boxes.length, atRiskBoxes: atRisk.length, dustBoxes: dust.length, boxes }, (_, v) => typeof v === 'bigint' ? v.toString() : v, 2);
  }

  const lines: string[] = [];
  lines.push('');
  lines.push(`  Rent Scan${address ? ` for ${address}` : ''}`);
  lines.push('  ─────────────────────────────────────────');
  lines.push(`  Boxes found:       ${boxes.length}`);
  lines.push(`  At-risk boxes:     ${atRisk.length}`);
  lines.push(`  Dust boxes:        ${dust.length}`);
  if (atRisk.length > 0) {
    lines.push(`  ${riskColor('emergency')}At-risk boxes:`);
    for (const box of atRisk) {
      lines.push(`    ${box.boxId} -- ${riskBadge(box.riskLevel)} -- ${Math.floor(box.daysUntilDeadline)} days until deadline`);
    }
    lines.push(resetColor());
  }
  if (dust.length > 0) {
    lines.push(`  ${riskColor('warning')}Dust boxes (eligible for consolidation):`);
    for (const box of dust) {
      lines.push(`    ${box.boxId} -- ${nanoergToErg(box.valueNanoerg)} ERG (${box.byteSize} bytes)`);
    }
    lines.push(resetColor());
  }
  lines.push('');
  return lines.join('\n');
}

function runConsolidate(address: string, json: boolean): string {
  const boxes = getMockBoxes().filter(b => b.address === address || !address);
  const dust = boxes.filter(b => b.valueNanoerg < b.minBoxValue * 5n && b.tokens.length === 0);

  if (dust.length < 2) {
    return json ? JSON.stringify({ error: 'Need at least 2 dust boxes to consolidate', dustCount: dust.length }) : `  Not enough dust boxes to consolidate (${dust.length} found, need 2+)`;
  }

  const totalValue = dust.reduce((sum, b) => sum + b.valueNanoerg, 0n);
  const fee = 1_000_000n; // ~0.001 ERG
  const outputValue = totalValue - fee;
  const riskLevels = dust.map(b => b.riskLevel);
  const worstRisk = riskOrder(riskLevels);

  const plan: ConsolidationPlan = {
    address: address || 'consolidation-target',
    boxesToConsolidate: dust.map(b => b.boxId),
    totalInputValue: totalValue,
    estimatedFee: fee,
    estimatedOutputValue: outputValue,
    rentSavedBoxes: dust.length - 1,
    priority: worstRisk,
  };
  return formatConsolidationPlan(plan, json);
}

function riskOrder(levels: RiskLevel[]): RiskLevel {
  const order: Record<RiskLevel, number> = { emergency: 0, critical: 1, warning: 2, caution: 3, safe: 4 };
  return levels.sort((a, b) => order[a] - order[b])[0];
}

function runMigrate(boxId: string, json: boolean): string {
  const boxes = getMockBoxes();
  const box = boxes.find(b => b.boxId.startsWith(boxId));
  if (!box) return `  Error: Box ${boxId} not found`;

  if (json) {
    return JSON.stringify({
      action: 'migrate',
      boxId: box.boxId,
      currentValue: box.valueNanoerg.toString(),
      creationHeight: box.creationHeight,
      daysUntilRent: box.daysUntilDeadline,
      tokens: box.tokens,
      reason: `Box is ${(box.ageBlocks / BLOCKS_PER_DAY / 365).toFixed(1)} years old. Migration creates a new box with the same data and tokens but a fresh creation height.`,
      estimatedFee: '0.001 ERG',
    }, null, 2);
  }

  const lines: string[] = [];
  lines.push('');
  lines.push(`  Migration Plan for ${box.boxId}`);
  lines.push('  ─────────────────────────────────────────');
  lines.push(`  Current age:      ${(box.ageBlocks / BLOCKS_PER_DAY / 365).toFixed(1)} years`);
  lines.push(`  Days until rent:  ${Math.floor(box.daysUntilDeadline)}`);
  lines.push(`  Current value:    ${nanoergToErg(box.valueNanoerg)} ERG`);
  lines.push(`  Tokens:           ${box.tokens.length > 0 ? box.tokens.map(t => `${t.amount.toString()} of ${t.tokenId}`).join(', ') : 'None'}`);
  lines.push(`  Estimated fee:    0.001 ERG`);
  lines.push('');
  lines.push(`  Action: Spend this box and create a new box with the same registers, tokens, and ERG value.`);
  lines.push(`  Result: New creation height = current block height. Rent clock resets to 0.`);
  if (box.daysUntilDeadline < 90) {
    lines.push(`  ${riskColor('emergency')}URGENT: This box should be migrated immediately!${resetColor()}`);
  }
  lines.push('');
  return lines.join('\n');
}

function runBudget(json: boolean): string {
  const boxes = getMockBoxes();
  const safeBoxes = boxes.filter(b => b.riskLevel === 'safe').length;
  const atRiskBoxes = boxes.filter(b => b.riskLevel === 'critical' || b.riskLevel === 'warning').length;
  const emergencyBoxes = boxes.filter(b => b.riskLevel === 'emergency').length;
  const totalValue = boxes.reduce((sum, b) => sum + b.valueNanoerg, 0n);
  const totalDeficit = boxes.reduce((sum, b) => {
    const deficit = b.minBoxValue - b.valueNanoerg;
    return deficit > 0n ? sum + deficit : sum;
  }, 0n);
  const annualRent = boxes.reduce((sum, b) => sum + computeAnnualRent(b.byteSize), 0n);

  const summary: RentBudgetSummary = {
    totalBoxes: boxes.length,
    safeBoxes,
    atRiskBoxes,
    emergencyBoxes,
    totalValueErg: nanoergToErg(totalValue),
    totalDeficitErg: nanoergToErg(totalDeficit),
    ergNeededForProtection: nanoergToErg(totalDeficit),
    estimatedAnnualRentErg: nanoergToErg(annualRent),
  };
  return formatBudget(summary, json);
}

// ─── Command definition ──────────────────────────────────────────────

export const rentCommand: Command = {
  name: 'rent',
  description: 'Storage rent inspection, consolidation, and box migration',
  aliases: ['storage-rent', 'box-age'],
  options: [
    { name: 'json', short: 'j', long: '--json', description: 'Output as JSON', required: false, type: 'boolean' },
    { name: 'box', short: 'b', long: '--box', description: 'Box ID for estimate/migrate', required: false, type: 'string' },
    { name: 'address', short: 'a', long: '--address', description: 'Address for scan/consolidate', required: false, type: 'string' },
  ],
  action: async (args: ParsedArgs, _ctx: CLIContext) => {
    const json = !!args.options.json;
    const sub = args.positional[0];

    if (!sub || sub === 'status') {
      console.log(runStatus(json));
    } else if (sub === 'estimate') {
      const boxId = (args.options.box as string) || args.positional[1];
      if (!boxId) {
        console.error('  Error: --box <id> required for estimate');
        return;
      }
      console.log(runEstimate(boxId, json));
    } else if (sub === 'scan') {
      const address = (args.options.address as string) || args.positional[1];
      console.log(runScan(address, json));
    } else if (sub === 'consolidate') {
      const address = (args.options.address as string) || args.positional[1];
      console.log(runConsolidate(address, json));
    } else if (sub === 'migrate') {
      const boxId = (args.options.box as string) || args.positional[1];
      if (!boxId) {
        console.error('  Error: --box <id> required for migrate');
        return;
      }
      console.log(runMigrate(boxId, json));
    } else if (sub === 'budget') {
      console.log(runBudget(json));
    } else {
      console.log('  Usage: xergon rent [status|estimate|scan|consolidate|migrate|budget]');
      console.log('');
      console.log('  Subcommands:');
      console.log('    status       Show all tracked boxes with rent age and risk level');
      console.log('    estimate     Estimate storage rent cost for a specific box');
      console.log('    scan         Scan address for boxes approaching rent deadline');
      console.log('    consolidate  Plan UTXO consolidation for dust boxes');
      console.log('    migrate      Plan box migration to reset creation height');
      console.log('    budget       Show rent protection budget summary');
      console.log('');
      console.log('  Options:');
      console.log('    --json       Output in JSON format');
      console.log('    --box <id>   Specify box ID');
      console.log('    --address    Specify Ergo address');
    }
  },
};

// ─── Exports for testing ─────────────────────────────────────────────
export {
  computeRiskLevel, riskColor, resetColor, riskBadge, nanoergToErg,
  computeMinBoxValue, computeDaysUntilRent, computeAnnualRent,
  RENT_THRESHOLD_BLOCKS, NANOERG_PER_BYTE, NANOERG_PER_ERG, BLOCKS_PER_DAY,
  formatStatusTable, formatEstimate, formatConsolidationPlan, formatBudget,
};
export type { BoxRentInfo, ConsolidationCandidate, ConsolidationPlan, RentEstimateResult, RentBudgetSummary, RiskLevel };
