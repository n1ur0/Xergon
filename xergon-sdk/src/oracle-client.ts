/**
 * Oracle pool reader for querying ERG/USD price feeds on-chain.
 *
 * Supports two backends:
 * 1. Ergo node API (preferred, lower latency)
 * 2. Ergo Explorer API (fallback, no local node required)
 *
 * Decodes SInt-encoded rate and epoch values from the oracle pool box registers.
 */

import type { OracleResult } from './types/contracts';
import { decodeSIntLong, decodeSIntInt } from './ergo-tx';

/** Default Ergo node URL for local queries. */
const DEFAULT_NODE_URL = 'http://127.0.0.1:9053';

/** Ergo Explorer API base URL (public fallback). */
const EXPLORER_API = 'https://api.ergoplatform.com/api/v1';

/**
 * Query an oracle pool box and extract the rate/epoch data.
 *
 * Tries the Ergo node API first. If that fails (e.g. no local node),
 * falls back to the public Explorer API.
 *
 * @param poolNftId - The NFT token ID that identifies the oracle pool
 * @param nodeUrl - Optional Ergo node URL (defaults to localhost:9053)
 * @returns Oracle result with rate, epoch, derived ERG/USD price, and metadata
 *
 * @example
 * ```ts
 * import { getOracleRate } from '@xergon/sdk';
 *
 * const oracle = await getOracleRate(
 *   'TOKEN_ID_HERE',
 *   'http://127.0.0.1:9053'
 * );
 * console.log(`ERG/USD: ${oracle.ergUsd.toFixed(4)}`);
 * console.log(`Epoch: ${oracle.epoch}`);
 * ```
 */
export async function getOracleRate(
  poolNftId: string,
  nodeUrl: string = DEFAULT_NODE_URL,
): Promise<OracleResult> {
  // Try node API first
  try {
    return await fetchFromNode(poolNftId, nodeUrl);
  } catch {
    // Fall through to Explorer
  }

  // Fallback to Explorer API
  return fetchFromExplorer(poolNftId);
}

/**
 * Query the Ergo node API for an oracle pool box.
 *
 * @param poolNftId - The NFT token ID
 * @param nodeUrl - Ergo node base URL
 */
async function fetchFromNode(
  poolNftId: string,
  nodeUrl: string,
): Promise<OracleResult> {
  const url = `${nodeUrl}/utxo/withPool/byTokenId/${poolNftId}`;

  const response = await fetch(url, {
    headers: { 'Accept': 'application/json' },
    signal: AbortSignal.timeout(10_000),
  });

  if (!response.ok) {
    throw new Error(`Node API returned ${response.status}: ${response.statusText}`);
  }

  const data = await response.json();

  if (!Array.isArray(data) || data.length === 0) {
    throw new Error('No boxes found for oracle pool NFT');
  }

  // The oracle pool box should contain the rate in R4 and epoch in R5
  const box = data[0];
  return parseOracleBox(box);
}

/**
 * Query the Ergo Explorer API for an oracle pool box.
 *
 * @param poolNftId - The NFT token ID
 */
async function fetchFromExplorer(poolNftId: string): Promise<OracleResult> {
  const url = `${EXPLORER_API}/boxes/unspent/byTokenId/${poolNftId}`;

  const response = await fetch(url, {
    headers: { 'Accept': 'application/json' },
    signal: AbortSignal.timeout(15_000),
  });

  if (!response.ok) {
    throw new Error(`Explorer API returned ${response.status}: ${response.statusText}`);
  }

  const data = await response.json();

  // Explorer returns { items: Box[] }
  const items = data?.items ?? data;
  if (!Array.isArray(items) || items.length === 0) {
    throw new Error('No boxes found for oracle pool NFT via Explorer');
  }

  const box = items[0];
  return parseOracleBox(box);
}

/**
 * Parse an oracle pool box (from node or explorer) to extract rate and epoch.
 *
 * The box should have:
 * - R4: rate as SInt Long (tag 0x05 + 8 bytes)
 * - R5: epoch as SInt Int (tag 0x04 + 4 bytes)
 *
 * @param box - The box JSON object from the API
 * @returns Parsed OracleResult
 */
function parseOracleBox(box: Record<string, any>): OracleResult {
  const boxId = box.boxId ?? box.id ?? 'unknown';

  // Extract register values
  // Node API format: additionalRegisters.R4 / registers.R4
  // Explorer API format: additionalRegisters.R4
  const registers = box.additionalRegisters ?? box.registers ?? {};

  const r4Raw = registers.R4 ?? registers['R4'] ?? '';
  const r5Raw = registers.R5 ?? registers['R5'] ?? '';

  if (!r4Raw) {
    throw new Error(`Oracle box ${boxId} has no R4 register (rate)`);
  }

  // Decode values
  const rate = decodeSIntLong(r4Raw);
  const epoch = r5Raw ? decodeSIntInt(r5Raw) : 0;

  // Derive ERG/USD: rate is typically in nano-units (1e-9 precision)
  const ergUsd = Number(rate) / 1_000_000_000;

  return {
    rate,
    epoch,
    ergUsd,
    boxId,
    fetchedAt: new Date(),
  };
}
