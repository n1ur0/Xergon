/**
 * Contract interaction methods for the Xergon SDK.
 *
 * These methods provide high-level access to on-chain contract operations
 * via the xergon-agent API (proxied through the marketplace at
 * /api/xergon-agent/[...path]).
 *
 * The agent wraps the Ergo node API and handles:
 * - UTXO selection and box creation
 * - Transaction building, signing, and broadcasting
 * - Register value encoding/decoding
 * - Oracle pool queries
 *
 * All methods use the XergonClientCore HTTP layer with HMAC authentication
 * where configured.
 */

import type { XergonClientCore } from './client';
import { XergonError } from './errors';
import { decodeSIntLong, decodeSIntInt } from './ergo-tx';
import type {
  RegisterProviderApiParams,
  RegisterProviderResult,
  ProviderBoxStatus,
  OnChainProvider,
  CreateStakingBoxApiParams,
  CreateStakingBoxResult,
  UserStakingBalance,
  StakingBoxInfo,
  OraclePoolStatus,
  SettleableBox,
  BuildSettlementApiParams,
  BuildSettlementResult,
  CreateGovernanceProposalApiParams,
  CreateGovernanceProposalResult,
  VoteOnProposalApiParams,
  VoteOnProposalResult,
  GovernanceProposal,
} from './types/contracts';

// ── Provider Methods ────────────────────────────────────────────────────

/**
 * Register a new provider on-chain.
 *
 * Calls the agent to build and broadcast a transaction that creates a
 * Provider Box with metadata in registers R4-R7 and mints a Provider NFT.
 *
 * @param client - The SDK core client
 * @param params - Provider registration parameters
 * @returns The transaction ID, NFT token ID, and provider box ID
 * @throws {XergonError} If the agent rejects the request or the tx fails
 *
 * @example
 * ```ts
 * const result = await client.contracts.registerProvider({
 *   providerName: 'MyGPU',
 *   region: 'US',
 *   endpoint: 'https://gpu.example.com',
 *   models: ['llama-3.3-70b', 'mistral-small-24b'],
 *   ergoAddress: '9eZ24...',
 *   providerPkHex: 'abc123...',
 * });
 * console.log(`Registered! NFT: ${result.providerNftId}`);
 * ```
 */
export async function registerProvider(
  client: XergonClientCore,
  params: RegisterProviderApiParams,
): Promise<RegisterProviderResult> {
  return client.post<RegisterProviderResult>('/v1/contracts/provider/register', {
    provider_name: params.providerName,
    region: params.region,
    endpoint: params.endpoint,
    models: params.models,
    ergo_address: params.ergoAddress,
    provider_pk_hex: params.providerPkHex,
  });
}

/**
 * Query the current status of a registered provider by NFT token ID.
 *
 * Fetches the provider box from the UTXO set and decodes the registers
 * to extract provider metadata (name, endpoint, pricing).
 *
 * @param client - The SDK core client
 * @param providerNftId - The Provider NFT token ID (hex, 64 chars)
 * @returns Parsed provider box status with decoded register values
 * @throws {XergonError} If the provider box is not found
 *
 * @example
 * ```ts
 * const status = await client.contracts.queryProviderStatus(nftId);
 * console.log(`Provider: ${status.providerName}`);
 * console.log(`Price: ${status.pricePerToken} nanoERG/token`);
 * ```
 */
export async function queryProviderStatus(
  client: XergonClientCore,
  providerNftId: string,
): Promise<ProviderBoxStatus> {
  const raw = await client.get<{
    box_id: string;
    provider_nft_id: string;
    provider_name: string;
    endpoint: string;
    price_per_token: string;
    min_stake: string;
    value: string;
    height: number;
    confirmations: number;
  }>('/v1/contracts/provider/status', {
    headers: { 'X-Provider-Nft-Id': providerNftId },
  });

  return {
    boxId: raw.box_id,
    providerNftId: raw.provider_nft_id,
    providerName: raw.provider_name,
    endpoint: raw.endpoint,
    pricePerToken: BigInt(raw.price_per_token),
    minStake: BigInt(raw.min_stake),
    value: BigInt(raw.value),
    height: raw.height,
    confirmations: raw.confirmations,
  };
}

/**
 * List all on-chain providers by scanning the UTXO set for provider boxes.
 *
 * The agent scans boxes matching the provider_registration contract
 * ergoTree and returns parsed metadata for each.
 *
 * @param client - The SDK core client
 * @returns Array of on-chain provider entries
 * @throws {XergonError} If the agent query fails
 *
 * @example
 * ```ts
 * const providers = await client.contracts.listOnChainProviders();
 * for (const p of providers) {
 *   console.log(`${p.providerName} (${p.region}): ${p.models.join(', ')}`);
 * }
 * ```
 */
export async function listOnChainProviders(
  client: XergonClientCore,
): Promise<OnChainProvider[]> {
  const raw = await client.get<Array<{
    box_id: string;
    provider_nft_id: string;
    provider_name: string;
    endpoint: string;
    models: string[];
    region: string;
    value_nanoerg: string;
    active: boolean;
  }>>('/v1/contracts/providers');

  return raw.map((p) => ({
    boxId: p.box_id,
    providerNftId: p.provider_nft_id,
    providerName: p.provider_name,
    endpoint: p.endpoint,
    models: p.models,
    region: p.region,
    valueNanoerg: BigInt(p.value_nanoerg),
    active: p.active,
  }));
}

// ── Staking Methods ─────────────────────────────────────────────────────

/**
 * Create a new User Staking Box on-chain.
 *
 * Calls the agent to build and broadcast a transaction that creates a box
 * guarded by the user_staking contract. The user's ERG is locked in this
 * box and used to pay for inference requests.
 *
 * @param client - The SDK core client
 * @param params - Staking creation parameters
 * @returns The transaction ID, staking box ID, and amount staked
 * @throws {XergonError} If insufficient funds or agent rejects the request
 *
 * @example
 * ```ts
 * const result = await client.contracts.createStakingBox({
 *   userPkHex: 'abc123...',
 *   amountNanoerg: 5_000_000_000n, // 5 ERG
 * });
 * console.log(`Staked in box: ${result.stakingBoxId}`);
 * ```
 */
export async function createStakingBox(
  client: XergonClientCore,
  params: CreateStakingBoxApiParams,
): Promise<CreateStakingBoxResult> {
  return client.post<CreateStakingBoxResult>('/v1/contracts/staking/create', {
    user_pk_hex: params.userPkHex,
    amount_nanoerg: params.amountNanoerg.toString(),
  });
}

/**
 * Query a user's ERG balance across all their staking boxes.
 *
 * Finds all boxes matching the user_staking contract ergoTree where
 * the guard matches the given public key, and sums the values.
 *
 * @param client - The SDK core client
 * @param userPkHex - The user's public key hex (64 chars)
 * @returns The user's total staking balance and individual box details
 * @throws {XergonError} If the query fails
 *
 * @example
 * ```ts
 * const balance = await client.contracts.queryUserBalance(userPk);
 * console.log(`Balance: ${Number(balance.totalBalanceNanoerg) / 1e9} ERG`);
 * ```
 */
export async function queryUserBalance(
  client: XergonClientCore,
  userPkHex: string,
): Promise<UserStakingBalance> {
  const raw = await client.get<{
    user_pk_hex: string;
    total_balance_nanoerg: string;
    staking_box_count: number;
    boxes: Array<{
      box_id: string;
      value_nanoerg: string;
      creation_height: number;
      confirmations: number;
    }>;
  }>(`/v1/contracts/staking/balance/${encodeURIComponent(userPkHex)}`);

  return {
    userPkHex: raw.user_pk_hex,
    totalBalanceNanoerg: BigInt(raw.total_balance_nanoerg),
    stakingBoxCount: raw.staking_box_count,
    boxes: raw.boxes.map((b) => ({
      boxId: b.box_id,
      valueNanoerg: BigInt(b.value_nanoerg),
      creationHeight: b.creation_height,
      confirmations: b.confirmations,
    })),
  };
}

/**
 * Get all staking boxes for a given user.
 *
 * Returns detailed information about each staking box including
 * value, creation height, and confirmation count.
 *
 * @param client - The SDK core client
 * @param userPkHex - The user's public key hex (64 chars)
 * @returns Array of staking box information
 * @throws {XergonError} If the query fails
 *
 * @example
 * ```ts
 * const boxes = await client.contracts.getUserStakingBoxes(userPk);
 * for (const box of boxes) {
 *   console.log(`Box ${box.boxId}: ${Number(box.valueNanoerg) / 1e9} ERG`);
 * }
 * ```
 */
export async function getUserStakingBoxes(
  client: XergonClientCore,
  userPkHex: string,
): Promise<StakingBoxInfo[]> {
  const raw = await client.get<{
    user_pk_hex: string;
    total_balance_nanoerg: string;
    staking_box_count: number;
    boxes: Array<{
      box_id: string;
      value_nanoerg: string;
      creation_height: number;
      confirmations: number;
    }>;
  }>(`/v1/contracts/staking/boxes/${encodeURIComponent(userPkHex)}`);

  return raw.boxes.map((b) => ({
    boxId: b.box_id,
    valueNanoerg: BigInt(b.value_nanoerg),
    creationHeight: b.creation_height,
    confirmations: b.confirmations,
  }));
}

// ── Oracle Methods ──────────────────────────────────────────────────────

/**
 * Get the current ERG/USD rate from the oracle pool.
 *
 * Queries the agent (which queries the Ergo node or explorer) for the
 * latest oracle pool box and decodes the SInt-encoded rate from R4.
 *
 * @param client - The SDK core client
 * @returns The current ERG/USD rate and oracle metadata
 * @throws {XergonError} If the oracle pool is unavailable or the box has no rate
 *
 * @example
 * ```ts
 * const rate = await client.contracts.getOracleRate();
 * console.log(`ERG/USD: $${rate.ergUsd.toFixed(4)}`);
 * ```
 */
export async function getOracleRate(
  client: XergonClientCore,
): Promise<{ rate: number; epoch: number; fetchedAt: Date }> {
  const raw = await client.get<{
    rate: string;
    epoch: number;
    box_id: string;
    erg_usd: number;
  }>('/v1/contracts/oracle/rate');

  return {
    rate: raw.erg_usd,
    epoch: raw.epoch,
    fetchedAt: new Date(),
  };
}

/**
 * Get detailed oracle pool status including epoch, box ID, and update height.
 *
 * @param client - The SDK core client
 * @returns Full oracle pool status
 * @throws {XergonError} If the oracle pool is unavailable
 *
 * @example
 * ```ts
 * const status = await client.contracts.getOraclePoolStatus();
 * console.log(`Epoch ${status.epoch}, Rate: $${status.ergUsd.toFixed(4)}`);
 * ```
 */
export async function getOraclePoolStatus(
  client: XergonClientCore,
): Promise<OraclePoolStatus> {
  const raw = await client.get<{
    epoch: number;
    erg_usd: number;
    rate: string;
    pool_box_id: string;
    last_update_height: number;
  }>('/v1/contracts/oracle/status');

  return {
    epoch: raw.epoch,
    ergUsd: raw.erg_usd,
    rate: BigInt(raw.rate),
    poolBoxId: raw.pool_box_id,
    lastUpdateHeight: raw.last_update_height,
  };
}

// ── Settlement Methods ──────────────────────────────────────────────────

/**
 * Get staking boxes that are ready for settlement.
 *
 * Returns staking boxes where accumulated inference fees exceed the
 * minimum settlement threshold. Providers call this to find boxes
 * they can settle to receive their earned ERG.
 *
 * @param client - The SDK core client
 * @param maxBoxes - Maximum number of boxes to return (default: 50)
 * @returns Array of settleable staking boxes
 * @throws {XergonError} If the query fails
 *
 * @example
 * ```ts
 * const boxes = await client.contracts.getSettleableBoxes(20);
 * const totalFees = boxes.reduce((sum, b) => sum + b.feeAmountNanoerg, 0n);
 * console.log(`${boxes.length} boxes with ${Number(totalFees) / 1e9} ERG in fees`);
 * ```
 */
export async function getSettleableBoxes(
  client: XergonClientCore,
  maxBoxes: number = 50,
): Promise<SettleableBox[]> {
  const raw = await client.get<Array<{
    box_id: string;
    value_nanoerg: string;
    user_pk_hex: string;
    provider_nft_id: string;
    fee_amount_nanoerg: string;
  }>>(`/v1/contracts/settlement/settleable`, {
    headers: { 'X-Max-Boxes': String(maxBoxes) },
  });

  return raw.map((b) => ({
    boxId: b.box_id,
    valueNanoerg: BigInt(b.value_nanoerg),
    userPkHex: b.user_pk_hex,
    providerNftId: b.provider_nft_id,
    feeAmountNanoerg: BigInt(b.fee_amount_nanoerg),
  }));
}

/**
 * Build a settlement transaction via the agent.
 *
 * The agent constructs an unsigned transaction that spends the specified
 * staking boxes, deducts fees, and sends the settled ERG to the provider.
 * The provider then signs and broadcasts via their wallet.
 *
 * @param client - The SDK core client
 * @param params - Settlement parameters
 * @returns The unsigned transaction and settlement summary
 * @throws {XergonError} If any box is invalid, already settled, or the tx build fails
 *
 * @example
 * ```ts
 * const result = await client.contracts.buildSettlementTx({
 *   stakingBoxIds: ['box1', 'box2'],
 *   feeAmounts: [500_000n, 300_000n],
 *   providerAddress: '9eZ24...',
 *   maxFeeNanoerg: 1_100_000n,
 * });
 * // Sign result.unsignedTx with Nautilus, then broadcast
 * ```
 */
export async function buildSettlementTx(
  client: XergonClientCore,
  params: BuildSettlementApiParams,
): Promise<BuildSettlementResult> {
  const raw = await client.post<{
    unsigned_tx: object;
    total_fees_nanoerg: string;
    net_settlement_nanoerg: string;
    estimated_tx_fee: string;
  }>('/v1/contracts/settlement/build', {
    staking_box_ids: params.stakingBoxIds,
    fee_amounts: params.feeAmounts.map((f) => f.toString()),
    provider_address: params.providerAddress,
    max_fee_nanoerg: params.maxFeeNanoerg.toString(),
  });

  return {
    unsignedTx: raw.unsigned_tx,
    totalFeesNanoerg: BigInt(raw.total_fees_nanoerg),
    netSettlementNanoerg: BigInt(raw.net_settlement_nanoerg),
    estimatedTxFee: BigInt(raw.estimated_tx_fee),
  };
}

// ── Governance Methods ─────────────────────────────────────────────────

/**
 * Create a new governance proposal on-chain.
 *
 * Calls the agent to build and broadcast a transaction that creates a
 * Governance Proposal box with the proposal metadata in registers.
 *
 * @param client - The SDK core client
 * @param params - Proposal creation parameters
 * @returns The transaction ID, proposal box ID, and assigned proposal ID
 * @throws {XergonError} If the agent rejects the request or the tx fails
 *
 * @example
 * ```ts
 * const result = await client.contracts.createGovernanceProposal({
 *   title: 'Reduce min stake',
 *   description: 'Lower the minimum staking requirement from 5 ERG to 1 ERG.',
 *   proposalType: 'parameter_change',
 *   proposalData: JSON.stringify({ param: 'min_stake', value: '1000000000' }),
 *   proposerPkHex: 'abc123...',
 *   votingDurationBlocks: 7200,
 * });
 * ```
 */
export async function createGovernanceProposal(
  client: XergonClientCore,
  params: CreateGovernanceProposalApiParams,
): Promise<CreateGovernanceProposalResult> {
  return client.post<CreateGovernanceProposalResult>(
    '/v1/contracts/governance/proposal',
    {
      title: params.title,
      description: params.description,
      proposal_type: params.proposalType,
      proposal_data: params.proposalData,
      proposer_pk_hex: params.proposerPkHex,
      voting_duration_blocks: params.votingDurationBlocks,
    },
  );
}

/**
 * Vote on an active governance proposal.
 *
 * Calls the agent to build and broadcast a vote transaction. The voter's
 * staked ERG weight determines their voting power.
 *
 * @param client - The SDK core client
 * @param params - Vote parameters
 * @returns The transaction ID, proposal ID, and voter PK
 * @throws {XergonError} If the proposal is not active or the vote is invalid
 *
 * @example
 * ```ts
 * const result = await client.contracts.voteOnProposal({
 *   proposalId: 'prop42',
 *   voterPkHex: 'def456...',
 *   vote: 'for',
 *   stakeNanoerg: 5_000_000_000n,
 * });
 * ```
 */
export async function voteOnProposal(
  client: XergonClientCore,
  params: VoteOnProposalApiParams,
): Promise<VoteOnProposalResult> {
  return client.post<VoteOnProposalResult>(
    '/v1/contracts/governance/vote',
    {
      proposal_id: params.proposalId,
      voter_pk_hex: params.voterPkHex,
      vote: params.vote,
      stake_nanoerg: params.stakeNanoerg.toString(),
    },
  );
}

/**
 * List all governance proposals (active and expired).
 *
 * Queries the agent for all governance proposal boxes and returns
 * parsed metadata including vote tallies and status.
 *
 * @param client - The SDK core client
 * @returns Array of governance proposals
 * @throws {XergonError} If the query fails
 *
 * @example
 * ```ts
 * const proposals = await client.contracts.getGovernanceProposals();
 * for (const p of proposals) {
 *   console.log(`[${p.active ? 'ACTIVE' : 'CLOSED'}] ${p.title}`);
 *   console.log(`  For: ${Number(p.votesFor) / 1e9} ERG`);
 * }
 * ```
 */
export async function getGovernanceProposals(
  client: XergonClientCore,
): Promise<GovernanceProposal[]> {
  const raw = await client.get<Array<{
    proposal_id: string;
    proposal_box_id: string;
    title: string;
    description: string;
    proposal_type: string;
    proposer_pk_hex: string;
    creation_height: number;
    end_height: number;
    votes_for: string;
    votes_against: string;
    votes_abstain: string;
    active: boolean;
    executed: boolean;
  }>>('/v1/contracts/governance/proposals');

  return raw.map((p) => ({
    proposalId: p.proposal_id,
    proposalBoxId: p.proposal_box_id,
    title: p.title,
    description: p.description,
    proposalType: p.proposal_type,
    proposerPkHex: p.proposer_pk_hex,
    creationHeight: p.creation_height,
    endHeight: p.end_height,
    votesFor: BigInt(p.votes_for),
    votesAgainst: BigInt(p.votes_against),
    votesAbstain: BigInt(p.votes_abstain),
    active: p.active,
    executed: p.executed,
  }));
}
