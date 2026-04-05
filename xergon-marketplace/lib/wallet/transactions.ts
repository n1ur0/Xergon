/**
 * Transaction builder utilities for the Xergon protocol.
 *
 * These are pure functions that construct UnsignedTransaction objects
 * for Xergon on-chain actions. They follow the headless dApp pattern:
 * they build unsigned txs but never sign or submit -- the caller
 * is responsible for passing the result to sign_tx() / submit_tx().
 *
 * Contract ergoTree hex values are placeholder constants that will be
 * replaced with compiled output from the contracts/ directory once
 * Fleet SDK integration is complete.
 */

import type {
  ErgoAsset,
  ErgoBox,
  ErgoBoxCandidate,
  ErgoDataInput,
  ErgoTransactionInput,
  UnsignedTransaction,
} from '@/types/ergo-connector'

// ── Constants ────────────────────────────────────────────────────────────

/** Minimum ERG value for a box (dust threshold) in nanoERG. */
export const MIN_BOX_VALUE = 1_000_000n

/** Default fee in nanoERG for a standard transaction. */
export const DEFAULT_FEE = 1_100_000n

// ── Contract ErgoTree placeholders ───────────────────────────────────────
// These will be replaced with real compiled hex from the contracts.
// Format: serialized SigmaProp ergoTree (hex, no 0x prefix).

/**
 * Placeholder ergoTree for the user staking contract.
 * Guard: only the staking owner (PK) can spend the box.
 */
export const USER_STAKING_TREE =
  'PLACEHOLDER_USER_STAKING_TREE'

/**
 * Placeholder ergoTree for the provider box contract.
 * Holds provider metadata and NFT token.
 */
export const PROVIDER_BOX_TREE =
  'PLACEHOLDER_PROVIDER_BOX_TREE'

/**
 * Placeholder ergoTree for the provider registration contract.
 * Used when minting a new Provider NFT.
 */
export const PROVIDER_REGISTRATION_TREE =
  'PLACEHOLDER_PROVIDER_REGISTRATION_TREE'

/**
 * Placeholder ergoTree for the payment/output contract.
 * Used to receive inference payments.
 */
export const TREASURY_BOX_TREE =
  'PLACEHOLDER_TREASURY_BOX_TREE'

/**
 * Placeholder ergoTree for the usage proof contract.
 */
export const USAGE_PROOF_TREE =
  'PLACEHOLDER_USAGE_PROOF_TREE'

/**
 * Placeholder ergoTree for the GPU rental contract.
 */
export const GPU_RENTAL_TREE =
  'PLACEHOLDER_GPU_RENTAL_TREE'

/**
 * Placeholder ergoTree for the usage commitment contract.
 */
export const USAGE_COMMITMENT_TREE =
  'PLACEHOLDER_USAGE_COMMITMENT_TREE'

/**
 * Placeholder ergoTree for the relay registry contract.
 */
export const RELAY_REGISTRY_TREE =
  'PLACEHOLDER_RELAY_REGISTRY_TREE'

/**
 * Placeholder ergoTree for the GPU rating contract.
 */
export const GPU_RATING_TREE =
  'PLACEHOLDER_GPU_RATING_TREE'

/**
 * Placeholder ergoTree for the GPU rental listing contract.
 */
export const GPU_RENTAL_LISTING_TREE =
  'PLACEHOLDER_GPU_RENTAL_LISTING_TREE'

/**
 * Placeholder ergoTree for the payment bridge contract.
 */
export const PAYMENT_BRIDGE_TREE =
  'PLACEHOLDER_PAYMENT_BRIDGE_TREE'

// ── Helpers ──────────────────────────────────────────────────────────────

/**
 * Create an ErgoTransactionInput from an existing UTXO box.
 */
function boxToInput(box: ErgoBox): ErgoTransactionInput {
  return { boxId: box.boxId }
}

/**
 * Create an ErgoDataInput from an existing box.
 */
function boxToDataInput(box: ErgoBox): ErgoDataInput {
  return { boxId: box.boxId }
}

/**
 * Build an output box candidate with the given parameters.
 */
function buildOutput(params: {
  value: bigint
  ergoTree: string
  creationHeight: number
  assets?: ErgoAsset[]
  additionalRegisters?: Record<string, string>
}): ErgoBoxCandidate {
  return {
    value: Number(params.value),
    ergoTree: params.ergoTree,
    creationHeight: params.creationHeight,
    assets: params.assets ?? [],
    additionalRegisters: params.additionalRegisters ?? {},
    transactionId: '',  // filled by the signing process
    index: 0,           // filled by the signing process
  }
}

/**
 * Generate a pseudo-random token ID for NFT minting.
 * In production this should come from deterministic tx hashing
 * or be supplied by the caller.
 */
export function generateTokenId(): string {
  const bytes = new Uint8Array(32)
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    crypto.getRandomValues(bytes)
  } else {
    for (let i = 0; i < 32; i++) {
      bytes[i] = Math.floor(Math.random() * 256)
    }
  }
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

/**
 * Assemble an UnsignedTransaction from the given parts.
 */
function assembleTx(params: {
  inputs: ErgoTransactionInput[]
  dataInputs: ErgoDataInput[]
  outputs: ErgoBoxCandidate[]
}): UnsignedTransaction {
  return {
    id: '',
    inputs: params.inputs,
    dataInputs: params.dataInputs,
    outputs: params.outputs,
  }
}

// ── UTXO selection ───────────────────────────────────────────────────────

/**
 * Select UTXOs that cover at least `targetAmount` nanoERG.
 * Uses a simple greedy algorithm (biggest-first) for the MVP.
 *
 * @param utxos - Available UTXOs from the wallet
 * @param targetAmount - Minimum nanoERG needed (including fee + outputs)
 * @returns Array of selected boxes, or null if insufficient funds
 */
export function selectUtxos(
  utxos: ErgoBox[],
  targetAmount: bigint
): ErgoBox[] | null {
  // Sort by value descending (greedy selection)
  const sorted = [...utxos].sort((a, b) => b.value - a.value)

  let accumulated = 0n
  const selected: ErgoBox[] = []

  for (const box of sorted) {
    selected.push(box)
    accumulated += BigInt(box.value)
    if (accumulated >= targetAmount) {
      return selected
    }
  }

  return null // insufficient funds
}

// ── Transaction Builders ─────────────────────────────────────────────────

export interface CreateStakingBoxParams {
  /** UTXOs available from the user's wallet */
  utxos: ErgoBox[]
  /** Current blockchain height */
  height: number
  /** Amount to stake in nanoERG */
  stakeAmount: bigint
  /** The user's change address (P2PK address) */
  changeAddress: string
  /**
   * The ergoTree for the staking box output.
   * Defaults to USER_STAKING_TREE.
   */
  stakingTree?: string
  /** Transaction fee in nanoERG. Defaults to DEFAULT_FEE. */
  fee?: bigint
}

export interface CreateStakingBoxResult {
  tx: UnsignedTransaction
  /** Total nanoERG being staked (output box value) */
  stakedAmount: bigint
  /** Change returned to user in nanoERG */
  changeAmount: bigint
}

/**
 * Build a transaction that deposits ERG into a user staking box.
 *
 * The staking box uses a contract that only allows the owner to spend.
 * Any leftover ERG after the staking amount + fee is returned as change.
 *
 * @returns The unsigned transaction and amounts, or throws on error
 * @throws If insufficient UTXOs to cover stake + fee + change dust
 */
export function createStakingBox(
  params: CreateStakingBoxParams
): CreateStakingBoxResult {
  const fee = params.fee ?? DEFAULT_FEE
  const stakingTree = params.stakingTree ?? USER_STAKING_TREE

  // Total needed: stake + fee + potential change box dust
  const totalNeeded = params.stakeAmount + fee + MIN_BOX_VALUE
  const selected = selectUtxos(params.utxos, totalNeeded)
  if (!selected) {
    throw new Error(
      `Insufficient ERG to create staking box. Need at least ${totalNeeded} nanoERG.`
    )
  }

  const totalInput = selected.reduce((sum, box) => sum + BigInt(box.value), 0n)
  const changeAmount = totalInput - params.stakeAmount - fee

  const inputs = selected.map(boxToInput)

  const outputs: ErgoBoxCandidate[] = [
    // Staking box output
    buildOutput({
      value: params.stakeAmount,
      ergoTree: stakingTree,
      creationHeight: params.height,
    }),
  ]

  // Add change box only if there's enough to cover dust
  if (changeAmount >= MIN_BOX_VALUE) {
    // P2PK ergoTree from change address
    const changeTree = addressToErgoTree(params.changeAddress)
    outputs.push(
      buildOutput({
        value: changeAmount,
        ergoTree: changeTree,
        creationHeight: params.height,
      })
    )
  }

  const tx = assembleTx({ inputs, dataInputs: [], outputs })

  return {
    tx,
    stakedAmount: params.stakeAmount,
    changeAmount: changeAmount >= MIN_BOX_VALUE ? changeAmount : 0n,
  }
}

export interface PayForInferenceParams {
  /** The user's existing staking box to spend */
  stakingBox: ErgoBox
  /** Additional UTXOs if the staking box doesn't cover fee + payment */
  extraUtxos?: ErgoBox[]
  /** Current blockchain height */
  height: number
  /** Payment amount in nanoERG to send to the provider */
  paymentAmount: bigint
  /** Provider's address or box tree to receive payment */
  providerPayeeTree: string
  /**
   * The ergoTree for the new staking box (remainder).
   * Defaults to USER_STAKING_TREE.
   */
  stakingTree?: string
  /** Transaction fee in nanoERG. Defaults to DEFAULT_FEE. */
  fee?: bigint
}

export interface PayForInferenceResult {
  tx: UnsignedTransaction
  /** Remaining staking amount in the new box */
  remainingStake: bigint
  /** Payment sent to provider */
  paymentAmount: bigint
}

/**
 * Build a transaction that spends the user's staking box to pay for inference.
 *
 * Flow:
 * 1. Spend user's staking box (input)
 * 2. Create payment box for the provider (output)
 * 3. Create new staking box with (original value - payment - fee)
 *
 * @returns The unsigned transaction and amounts
 * @throws If staking box doesn't have enough to cover payment + fee + min stake
 */
export function payForInference(
  params: PayForInferenceParams
): PayForInferenceResult {
  const fee = params.fee ?? DEFAULT_FEE
  const stakingTree = params.stakingTree ?? USER_STAKING_TREE

  const stakingValue = BigInt(params.stakingBox.value)
  const totalNeeded = params.paymentAmount + fee + MIN_BOX_VALUE

  // Check if staking box alone covers it, or use extra UTXOs
  let allInputs: ErgoBox[]
  let totalInput: bigint

  if (stakingValue >= totalNeeded) {
    allInputs = [params.stakingBox]
    totalInput = stakingValue
  } else if (params.extraUtxos && params.extraUtxos.length > 0) {
    const extraNeeded = totalNeeded - stakingValue
    const selected = selectUtxos(params.extraUtxos, extraNeeded)
    if (!selected) {
      throw new Error(
        'Insufficient ERG to pay for inference. Staking box + extra UTXOs do not cover payment + fee.'
      )
    }
    allInputs = [params.stakingBox, ...selected]
    totalInput = stakingValue + selected.reduce(
      (sum, box) => sum + BigInt(box.value),
      0n
    )
  } else {
    throw new Error(
      'Insufficient ERG in staking box to cover payment + fee.'
    )
  }

  const remainingStake = totalInput - params.paymentAmount - fee

  if (remainingStake < MIN_BOX_VALUE) {
    throw new Error(
      'Payment + fee would consume the entire staking box. Leave at least the minimum box value.'
    )
  }

  const inputs = allInputs.map(boxToInput)

  const outputs: ErgoBoxCandidate[] = [
    // Provider payment box
    buildOutput({
      value: params.paymentAmount,
      ergoTree: params.providerPayeeTree,
      creationHeight: params.height,
    }),
    // Remaining stake box
    buildOutput({
      value: remainingStake,
      ergoTree: stakingTree,
      creationHeight: params.height,
    }),
  ]

  const tx = assembleTx({ inputs, dataInputs: [], outputs })

  return {
    tx,
    remainingStake,
    paymentAmount: params.paymentAmount,
  }
}

export interface RegisterProviderParams {
  /** UTXOs available from the provider's wallet */
  utxos: ErgoBox[]
  /** Current blockchain height */
  height: number
  /** Provider's P2PK change address */
  changeAddress: string
  /** Provider metadata to store in registers */
  metadata: {
    /** Provider name (will be encoded to Sigma string) */
    name: string
    /** Provider endpoint URL */
    endpoint: string
    /** Supported model identifiers */
    models: string[]
  }
  /** Token ID for the Provider NFT. Use generateTokenId() for new providers. */
  nftTokenId: string
  /** Transaction fee in nanoERG. Defaults to DEFAULT_FEE. */
  fee?: bigint
}

export interface RegisterProviderResult {
  tx: UnsignedTransaction
  /** The token ID of the minted Provider NFT */
  nftTokenId: string
}

// Ergo register constants
const R4 = 'R4'
const R5 = 'R5'
const R6 = 'R6'

/**
 * Encode a UTF-8 string as a Coll[Byte] hex for an Ergo register.
 * This is a simplified encoding: length-prefixed UTF-8 bytes.
 */
export function encodeStringForRegister(str: string): string {
  const encoder = new TextEncoder()
  const bytes = encoder.encode(str)
  // Ergo Coll[Byte] encoding: each byte as two hex chars
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

/**
 * Build a transaction that registers a new GPU provider on-chain.
 *
 * Flow:
 * 1. Mint a unique Provider NFT (singleton token)
 * 2. Create a Provider Box holding the NFT + metadata in registers
 * 3. Return change to provider's wallet
 *
 * The Provider NFT serves as an on-chain identity for the provider.
 * Metadata registers:
 *   R4 = provider name (Coll[Byte])
 *   R5 = endpoint URL (Coll[Byte])
 *   R6 = supported models (Coll[Byte], comma-separated for MVP)
 *
 * @returns The unsigned transaction and the NFT token ID
 * @throws If insufficient UTXOs to cover box value + fee
 */
export function registerProvider(
  params: RegisterProviderParams
): RegisterProviderResult {
  const fee = params.fee ?? DEFAULT_FEE

  // Provider box needs at least the minimum value + NFT
  const providerBoxValue = MIN_BOX_VALUE + fee + MIN_BOX_VALUE
  const selected = selectUtxos(params.utxos, providerBoxValue)
  if (!selected) {
    throw new Error(
      `Insufficient ERG to register as provider. Need at least ${providerBoxValue} nanoERG.`
    )
  }

  const totalInput = selected.reduce((sum, box) => sum + BigInt(box.value), 0n)
  const changeAmount = totalInput - MIN_BOX_VALUE - fee

  const inputs = selected.map(boxToInput)

  // Build the Provider NFT asset
  const nftAsset: ErgoAsset = {
    tokenId: params.nftTokenId,
    amount: 1,
  }

  // Encode metadata into registers
  const additionalRegisters: Record<string, string> = {
    [R4]: encodeStringForRegister(params.metadata.name),
    [R5]: encodeStringForRegister(params.metadata.endpoint),
    [R6]: encodeStringForRegister(params.metadata.models.join(',')),
  }

  const outputs: ErgoBoxCandidate[] = [
    // Provider box with NFT + metadata
    buildOutput({
      value: MIN_BOX_VALUE,
      ergoTree: PROVIDER_BOX_TREE,
      creationHeight: params.height,
      assets: [nftAsset],
      additionalRegisters,
    }),
  ]

  // Change box
  if (changeAmount >= MIN_BOX_VALUE) {
    const changeTree = addressToErgoTree(params.changeAddress)
    outputs.push(
      buildOutput({
        value: changeAmount,
        ergoTree: changeTree,
        creationHeight: params.height,
      })
    )
  }

  const tx = assembleTx({ inputs, dataInputs: [], outputs })

  return {
    tx,
    nftTokenId: params.nftTokenId,
  }
}

// ── Address conversion ───────────────────────────────────────────────────

/**
 * Convert an Ergo P2PK address to its ergoTree hex string.
 *
 * This is a simplified implementation for the MVP. It handles the
 * standard P2PK address format (prefix '9' for mainnet, '2' for testnet).
 *
 * For production use, Fleet SDK's address parsing should be used instead.
 *
 * @param address - Base58-encoded Ergo address
 * @returns Hex-encoded ergoTree (no 0x prefix)
 */
export function addressToErgoTree(address: string): string {
  // Standard P2PK ergoTree:
  // 0x1001 = SigmaProp(ProveDlog) prefix
  // followed by the 32-byte public key
  //
  // For MVP, we return a placeholder. In production this would:
  // 1. Base58-decode the address
  // 2. Extract the content hash
  // 3. Construct the P2PK ergoTree

  if (!address || address.length < 10) {
    throw new Error(`Invalid Ergo address: ${address}`)
  }

  // P2PK ergoTree template: 100104{pubkey}
  // For now we return the standard P2PK template as a placeholder
  // The wallet's sign_tx will handle the actual verification
  return '100104' + '0'.repeat(64)
}

/**
 * Check if a contract tree is still a placeholder (not yet compiled).
 */
export function isPlaceholderTree(tree: string): boolean {
  return tree.startsWith('PLACEHOLDER_')
}
