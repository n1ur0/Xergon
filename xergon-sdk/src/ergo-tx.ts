/**
 * Ergo transaction building utilities for Xergon on-chain operations.
 *
 * All transaction building is off-chain only -- the resulting unsigned
 * transaction is intended to be signed via Nautilus wallet or any
 * EIP-12 compatible signer.
 *
 * Uses @fleet-sdk/core for TransactionBuilder, OutputBuilder, and constants.
 */

import {
  TransactionBuilder,
  OutputBuilder,
  RECOMMENDED_MIN_FEE_VALUE,
  SAFE_MIN_BOX_VALUE,
  ErgoAddress,
} from '@fleet-sdk/core';
import { SLong, SInt } from '@fleet-sdk/serializer';

import { CONTRACTS } from './contracts';
import type {
  ProviderRegistrationParams,
  StakingParams,
  SettlementParams,
} from './types/contracts';

// ── SInt Decoders ───────────────────────────────────────────────────────

/**
 * Decode a Sigma SInt Long value from its serialized hex representation.
 *
 * SInt Long encoding: tag byte 0x05 followed by 8 bytes big-endian
 * two's complement signed integer.
 *
 * @param encodedHex - Full hex string including the tag byte (18 hex chars)
 * @returns The decoded signed 64-bit integer as a BigInt
 */
export function decodeSIntLong(encodedHex: string): bigint {
  const hex = encodedHex.replace(/^0x/, '');

  // SInt Long = tag 05 + 8 bytes = 18 hex chars total
  if (hex.length < 18) {
    throw new Error(`Invalid SInt Long hex: expected at least 18 chars, got ${hex.length}`);
  }

  const tag = hex.slice(0, 2);
  if (tag !== '05') {
    throw new Error(`Invalid SInt Long tag: expected 05, got ${tag}`);
  }

  const valueHex = hex.slice(2, 18);
  const buf = Uint8Array.from(Buffer.from(valueHex, 'hex'));

  // Interpret as signed 64-bit big-endian two's complement
  const unsigned = BigInt(`0x${valueHex}`);
  if (unsigned >= (1n << 63n)) {
    return unsigned - (1n << 64n);
  }
  return unsigned;
}

/**
 * Decode a Sigma SInt Int value from its serialized hex representation.
 *
 * SInt Int encoding: tag byte 0x04 followed by 4 bytes big-endian
 * two's complement signed integer.
 *
 * @param encodedHex - Full hex string including the tag byte (10 hex chars)
 * @returns The decoded signed 32-bit integer
 */
export function decodeSIntInt(encodedHex: string): number {
  const hex = encodedHex.replace(/^0x/, '');

  // SInt Int = tag 04 + 4 bytes = 10 hex chars total
  if (hex.length < 10) {
    throw new Error(`Invalid SInt Int hex: expected at least 10 chars, got ${hex.length}`);
  }

  const tag = hex.slice(0, 2);
  if (tag !== '04') {
    throw new Error(`Invalid SInt Int tag: expected 04, got ${tag}`);
  }

  const valueHex = hex.slice(2, 10);
  const buf = Buffer.from(valueHex, 'hex');

  // Interpret as signed 32-bit big-endian
  const unsigned = buf.readUInt32BE(0);
  return unsigned > 0x7fffffff ? unsigned - 0x100000000 : unsigned;
}

// ── ErgoTree Utilities ──────────────────────────────────────────────────

/**
 * Convert an ErgoTree hex string to a P2S (Pay-to-Script) address.
 *
 * P2S addresses use ErgoAddress.fromErgoTree() from fleet-sdk.
 *
 * @param treeHex - The ErgoTree hex string (with or without 0x prefix)
 * @returns The P2S address string (e.g. "2iHk...")
 */
export function ergoTreeToAddress(treeHex: string): string {
  const hex = treeHex.replace(/^0x/, '');
  return ErgoAddress.fromErgoTree(hex).encode();
}

// ── Helper: Encode string to Sigma Coll[Byte] hex ───────────────────────

/**
 * Encode a UTF-8 string as a Sigma Coll[Byte] (collection of bytes).
 * Format: tag 0x0d + compact int length + raw bytes
 *
 * @param str - The string to encode
 * @returns Hex string of the encoded Coll[Byte]
 */
function encodeCollByte(str: string): string {
  const bytes = Buffer.from(str, 'utf-8');
  const length = bytes.length;

  // Sigma compact integer encoding for positive values
  let lenHex = '';
  if (length < 0x40) {
    lenHex = (length << 2).toString(16).padStart(2, '0');
  } else if (length < 0x2000) {
    const val = (length << 2) | 0x01;
    lenHex = val.toString(16).padStart(4, '0');
  } else {
    const val = (length << 2) | 0x02;
    lenHex = val.toString(16).padStart(6, '0');
  }

  // Tag 0x0d for Coll[SByte]
  return '0d' + lenHex + bytes.toString('hex');
}

/**
 * Encode a string as a Sigma String value.
 * Format: tag 0x0e + compact int length + UTF-8 bytes
 *
 * @param str - The string to encode
 * @returns Hex string of the encoded String
 */
function encodeSigmaString(str: string): string {
  const bytes = Buffer.from(str, 'utf-8');
  const length = bytes.length;

  let lenHex = '';
  if (length < 0x40) {
    lenHex = (length << 2).toString(16).padStart(2, '0');
  } else if (length < 0x2000) {
    const val = (length << 2) | 0x01;
    lenHex = val.toString(16).padStart(4, '0');
  } else {
    const val = (length << 2) | 0x02;
    lenHex = val.toString(16).padStart(6, '0');
  }

  // Tag 0x0e for String
  return '0e' + lenHex + bytes.toString('hex');
}

/**
 * Encode a BigInt as a Sigma SInt Long hex.
 * Format: tag 0x05 + 8 bytes big-endian two's complement
 *
 * @param value - The BigInt to encode
 * @returns Hex string (18 chars)
 */
function encodeSIntLong(value: bigint): string {
  // Handle negative values with two's complement
  let buf: Buffer;
  if (value < 0n) {
    const twosComp = (1n << 64n) + value;
    buf = Buffer.alloc(8);
    buf.writeBigUInt64BE(twosComp, 0);
  } else {
    buf = Buffer.alloc(8);
    buf.writeBigUInt64BE(value, 0);
  }
  return '05' + buf.toString('hex');
}

// ── Transaction Builders ────────────────────────────────────────────────

/**
 * Build an unsigned provider registration transaction.
 *
 * Creates a new box guarded by the provider_registration contract with
 * metadata in registers R4-R7 and mints a Provider NFT (singleton token).
 *
 * Registers:
 * - R4: Provider name (Coll[Byte])
 * - R5: Endpoint URL (String)
 * - R6: Price per token (SInt Long)
 * - R7: Minimum stake (SInt Long)
 *
 * @param params - Provider registration parameters
 * @returns The unsigned transaction ready for Nautilus signing
 *
 * @example
 * ```ts
 * const unsignedTx = buildProviderRegistrationTx({
 *   senderAddress: '9eZ24...',
 *   providerName: 'MyGPU',
 *   endpoint: 'https://gpu.example.com',
 *   pricePerToken: 1000000n,
 *   minStake: 1000000000n,
 *   height: 800000,
 *   providerNftId: 'abcd...ef01',
 *   inputs: [/* funding boxes *\/],
 * });
 * ```
 */
export function buildProviderRegistrationTx(
  params: ProviderRegistrationParams,
): object {
  const {
    senderAddress,
    providerName,
    endpoint,
    pricePerToken,
    minStake,
    height,
    providerNftId,
    inputs = [],
    changeAddress,
  } = params;

  const change = changeAddress ?? senderAddress;
  const contractTree = CONTRACTS.provider_registration;

  // Build Sigma constant hex values for registers
  const r4Value = encodeCollByte(providerName);     // Coll[Byte] for name
  const r5Value = encodeSigmaString(endpoint);      // String for endpoint
  const r6Value = encodeSIntLong(pricePerToken);    // SInt Long for price
  const r7Value = encodeSIntLong(minStake);         // SInt Long for min stake

  // Build the provider registration output box
  const providerOutput = new OutputBuilder(SAFE_MIN_BOX_VALUE, contractTree)
    .mintToken({ amount: 1n, name: 'Xergon Provider NFT' })
    .setAdditionalRegisters({
      R4: r4Value,
      R5: r5Value,
      R6: r6Value,
      R7: r7Value,
    });

  const txBuilder = new TransactionBuilder(height);

  // Add funding inputs
  if (inputs.length > 0) {
    txBuilder.from(inputs as any, { ensureInclusion: true });
  }

  // Add outputs and configure change/fee
  txBuilder
    .to(providerOutput)
    .sendChangeTo(change)
    .payMinFee();

  const tx = txBuilder.build();
  return tx.toEIP12Object();
}

/**
 * Build an unsigned user staking transaction.
 *
 * Creates a new box guarded by the user_staking contract. The box
 * holds the staked ERG and references the provider box.
 *
 * @param params - Staking parameters
 * @returns The unsigned transaction ready for Nautilus signing
 *
 * @example
 * ```ts
 * const unsignedTx = buildStakingTx({
 *   userAddress: '9eZ24...',
 *   amount: 5000000000n,
 *   providerBoxId: 'boxIdHere...',
 *   height: 800000,
 *   inputs: [/* funding boxes *\/],
 * });
 * ```
 */
export function buildStakingTx(params: StakingParams): object {
  const {
    userAddress,
    amount,
    height,
    inputs = [],
    changeAddress,
  } = params;

  const change = changeAddress ?? userAddress;
  const contractTree = CONTRACTS.user_staking;

  // Build the staking output box
  const stakingOutput = new OutputBuilder(amount, contractTree);

  const txBuilder = new TransactionBuilder(height);

  if (inputs.length > 0) {
    txBuilder.from(inputs as any, { ensureInclusion: true });
  }

  txBuilder
    .to(stakingOutput)
    .sendChangeTo(change)
    .payMinFee();

  const tx = txBuilder.build();
  return tx.toEIP12Object();
}

/**
 * Build an unsigned settlement transaction.
 *
 * Spends accumulated staking boxes and pays the settled ERG to the
 * provider address. The provider box is also included as an input
 * to satisfy the contract guard.
 *
 * @param params - Settlement parameters
 * @returns The unsigned transaction ready for Nautilus signing
 *
 * @example
 * ```ts
 * const unsignedTx = buildSettlementTx({
 *   providerAddress: '9eZ24...',
 *   stakingBoxes: [/* staking boxes to settle *\/],
 *   providerBox: { boxId: '...', value: 1000000n },
 *   height: 800000,
 * });
 * ```
 */
export function buildSettlementTx(params: SettlementParams): object {
  const {
    providerAddress,
    stakingBoxes,
    providerBox,
    height,
    inputs = [],
  } = params;

  // Collect all inputs
  const allInputs = [
    ...stakingBoxes,
    providerBox,
    ...inputs,
  ];

  // Build settlement output to provider
  const settlementOutput = new OutputBuilder(
    SAFE_MIN_BOX_VALUE,
    providerAddress,
  );

  // Return provider box with its value
  const providerOutput = new OutputBuilder(
    providerBox.value,
    CONTRACTS.provider_box,
  );

  const txBuilder = new TransactionBuilder(height);

  txBuilder.from(allInputs as any, { ensureInclusion: true });

  txBuilder
    .to(settlementOutput)
    .to(providerOutput)
    .sendChangeTo(providerAddress)
    .payMinFee();

  const tx = txBuilder.build();
  return tx.toEIP12Object();
}
