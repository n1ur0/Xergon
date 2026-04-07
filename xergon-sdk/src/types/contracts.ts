/**
 * TypeScript types for the Xergon on-chain contract system.
 *
 * Covers all 13 compiled contracts, transaction builder parameters,
 * and oracle data structures.
 */

// ── Contract Names ──────────────────────────────────────────────────────

/** Union of all compiled Xergon contract names. */
export type ContractName =
  | 'provider_box'
  | 'provider_registration'
  | 'treasury_box'
  | 'usage_proof'
  | 'user_staking'
  | 'gpu_rental'
  | 'usage_commitment'
  | 'relay_registry'
  | 'gpu_rating'
  | 'gpu_rental_listing'
  | 'payment_bridge'
  | 'provider_slashing'
  | 'governance_proposal';

/** Registry mapping each contract name to its compiled ErgoTree hex. */
export interface CompiledContracts {
  provider_box: string;
  provider_registration: string;
  treasury_box: string;
  usage_proof: string;
  user_staking: string;
  gpu_rental: string;
  usage_commitment: string;
  relay_registry: string;
  gpu_rating: string;
  gpu_rental_listing: string;
  payment_bridge: string;
  provider_slashing: string;
  governance_proposal: string;
}

// ── Oracle ──────────────────────────────────────────────────────────────

/** Result from reading an oracle pool box via node or explorer API. */
export interface OracleResult {
  /** Oracle rate as a signed 64-bit long (e.g. price in nano-units). */
  rate: bigint;
  /** Oracle epoch counter. */
  epoch: number;
  /** Derived ERG/USD price (rate / 1e9). */
  ergUsd: number;
  /** Box ID of the oracle pool box that was read. */
  boxId: string;
  /** Timestamp when the data was fetched. */
  fetchedAt: Date;
}

// ── Transaction Builder Params ──────────────────────────────────────────

/** Parameters for building a provider registration transaction. */
export interface ProviderRegistrationParams {
  /** Sender's Ergo address (P2PK or P2S). */
  senderAddress: string;
  /** Human-readable provider name (will be encoded into R4 as Coll[Byte]). */
  providerName: string;
  /** Provider API endpoint URL (will be encoded into R5 as String). */
  endpoint: string;
  /** Price per token in nanoERG (will be encoded into R6 as SInt Long). */
  pricePerToken: bigint;
  /** Minimum stake required in nanoERG (will be encoded into R7 as SInt Long). */
  minStake: bigint;
  /** Current blockchain height for the transaction. */
  height: number;
  /** Token ID for the Provider NFT to mint (first 32 bytes of a hash, typically from TxId). */
  providerNftId: string;
  /** Optional: input boxes to fund the transaction. */
  inputs?: Array<{
    boxId: string;
    value: bigint;
    tokens?: Array<{ tokenId: string; amount: bigint }>;
  }>;
  /** Optional: change address (defaults to senderAddress). */
  changeAddress?: string;
}

/** Parameters for building a user staking transaction. */
export interface StakingParams {
  /** User's Ergo address. */
  userAddress: string;
  /** Amount to stake in nanoERG. */
  amount: bigint;
  /** Box ID of the provider box the user is staking with. */
  providerBoxId: string;
  /** Current blockchain height. */
  height: number;
  /** Optional: input boxes to fund the transaction. */
  inputs?: Array<{
    boxId: string;
    value: bigint;
    tokens?: Array<{ tokenId: string; amount: bigint }>;
  }>;
  /** Optional: change address (defaults to userAddress). */
  changeAddress?: string;
}

/** Parameters for building a settlement transaction. */
export interface SettlementParams {
  /** Provider's Ergo address (receives settled funds). */
  providerAddress: string;
  /** Staking boxes to spend (accumulated fees). */
  stakingBoxes: Array<{
    boxId: string;
    value: bigint;
    tokens?: Array<{ tokenId: string; amount: bigint }>;
    registers?: Record<string, string>;
  }>;
  /** Provider box to pay (contains the provider contract guard). */
  providerBox: {
    boxId: string;
    value: bigint;
    tokens?: Array<{ tokenId: string; amount: bigint }>;
  };
  /** Current blockchain height. */
  height: number;
  /** Optional: additional input boxes for change. */
  inputs?: Array<{
    boxId: string;
    value: bigint;
    tokens?: Array<{ tokenId: string; amount: bigint }>;
  }>;
}

// ── Contract API Types (Agent-mediated) ─────────────────────────────────

/**
 * Parameters for registering a provider via the agent API.
 *
 * The agent will build and broadcast the registration transaction
 * using the node's /wallet/payment endpoint.
 */
export interface RegisterProviderApiParams {
  /** Human-readable provider name. */
  providerName: string;
  /** Provider region (e.g. "US", "EU", "Asia"). */
  region: string;
  /** Provider API endpoint URL. */
  endpoint: string;
  /** Supported model identifiers. */
  models: string[];
  /** Provider's Ergo P2PK address. */
  ergoAddress: string;
  /** Provider's public key hex (32 bytes, 64 hex chars). */
  providerPkHex: string;
}

/** Result from a successful provider registration. */
export interface RegisterProviderResult {
  /** Transaction ID of the registration tx. */
  txId: string;
  /** The NFT token ID minted for the provider. */
  providerNftId: string;
  /** The box ID of the created provider box. */
  providerBoxId: string;
}

/**
 * Parsed on-chain provider box data returned by queryProviderStatus.
 */
export interface ProviderBoxStatus {
  /** The box ID. */
  boxId: string;
  /** The provider NFT token ID. */
  providerNftId: string;
  /** Provider name decoded from R4. */
  providerName: string;
  /** Endpoint URL decoded from R5. */
  endpoint: string;
  /** Price per token in nanoERG decoded from R6. */
  pricePerToken: bigint;
  /** Minimum stake in nanoERG decoded from R7. */
  minStake: bigint;
  /** Box value in nanoERG. */
  value: bigint;
  /** Current blockchain height when queried. */
  height: number;
  /** Number of confirmations. */
  confirmations: number;
}

/** An on-chain provider entry returned by listOnChainProviders. */
export interface OnChainProvider {
  /** The provider box ID. */
  boxId: string;
  /** The provider NFT token ID. */
  providerNftId: string;
  /** Provider name decoded from R4. */
  providerName: string;
  /** Endpoint URL decoded from R5. */
  endpoint: string;
  /** Supported models (decoded or stored). */
  models: string[];
  /** Region stored in R8 or metadata. */
  region: string;
  /** Box value in nanoERG. */
  valueNanoerg: bigint;
  /** Whether the provider is currently active (responding to pings). */
  active: boolean;
}

/**
 * Parameters for creating a staking box via the agent API.
 */
export interface CreateStakingBoxApiParams {
  /** User's public key hex (32 bytes, 64 hex chars). */
  userPkHex: string;
  /** Amount to stake in nanoERG. */
  amountNanoerg: bigint;
}

/** Result from creating a staking box. */
export interface CreateStakingBoxResult {
  /** Transaction ID of the staking tx. */
  txId: string;
  /** The box ID of the created staking box. */
  stakingBoxId: string;
  /** Amount staked in nanoERG. */
  amountNanoerg: bigint;
}

/** Parsed user balance from a staking box query. */
export interface UserStakingBalance {
  /** User's public key hex. */
  userPkHex: string;
  /** Total ERG balance across all staking boxes in nanoERG. */
  totalBalanceNanoerg: bigint;
  /** Number of staking boxes found. */
  stakingBoxCount: number;
  /** Individual staking box details. */
  boxes: StakingBoxInfo[];
}

/** Information about a single staking box. */
export interface StakingBoxInfo {
  /** Box ID. */
  boxId: string;
  /** Value in nanoERG. */
  valueNanoerg: bigint;
  /** Blockchain height when the box was created. */
  creationHeight: number;
  /** Number of confirmations. */
  confirmations: number;
}

/** Oracle pool status from getOraclePoolStatus. */
export interface OraclePoolStatus {
  /** Current oracle epoch number. */
  epoch: number;
  /** ERG/USD rate. */
  ergUsd: number;
  /** Raw oracle rate (SInt Long). */
  rate: bigint;
  /** Box ID of the oracle pool box. */
  poolBoxId: string;
  /** Block height of the last oracle update. */
  lastUpdateHeight: number;
}

/** A staking box that is ready for settlement. */
export interface SettleableBox {
  /** Box ID. */
  boxId: string;
  /** Value in nanoERG. */
  valueNanoerg: bigint;
  /** Owning user's public key hex. */
  userPkHex: string;
  /** Provider NFT token ID the box is linked to. */
  providerNftId: string;
  /** Fee accumulated in this box in nanoERG. */
  feeAmountNanoerg: bigint;
}

/**
 * Parameters for building a settlement transaction via the agent API.
 */
export interface BuildSettlementApiParams {
  /** Array of staking box IDs to settle. */
  stakingBoxIds: string[];
  /** Fee amounts to deduct from each staking box (indexed to match stakingBoxIds). */
  feeAmounts: bigint[];
  /** Provider's Ergo address to receive settled funds. */
  providerAddress: string;
  /** Maximum fee the provider is willing to pay for the settlement tx itself. */
  maxFeeNanoerg: bigint;
}

/** Result from building a settlement transaction. */
export interface BuildSettlementResult {
  /** The unsigned transaction (EIP-12 format). */
  unsignedTx: object;
  /** Total fees collected from all staking boxes in nanoERG. */
  totalFeesNanoerg: bigint;
  /** Total ERG to be sent to the provider in nanoERG (fees - tx fee). */
  netSettlementNanoerg: bigint;
  /** Estimated transaction fee in nanoERG. */
  estimatedTxFee: bigint;
}

// ── Governance Types (Agent-mediated) ──────────────────────────────────

/** Parameters for creating a governance proposal via the agent API. */
export interface CreateGovernanceProposalApiParams {
  /** Proposal title. */
  title: string;
  /** Proposal description (may include markdown). */
  description: string;
  /** Type of proposal (e.g. "parameter_change", "contract_upgrade", "fund_release"). */
  proposalType: string;
  /** JSON-encoded proposal data specific to the type. */
  proposalData: string;
  /** Proposer's public key hex. */
  proposerPkHex: string;
  /** Duration in blocks the proposal should remain active. */
  votingDurationBlocks: number;
}

/** Result from creating a governance proposal. */
export interface CreateGovernanceProposalResult {
  /** Transaction ID of the proposal creation tx. */
  txId: string;
  /** The proposal box ID. */
  proposalBoxId: string;
  /** Unique proposal ID assigned by the contract. */
  proposalId: string;
}

/** Parameters for voting on a governance proposal via the agent API. */
export interface VoteOnProposalApiParams {
  /** The proposal ID to vote on. */
  proposalId: string;
  /** Voter's public key hex. */
  voterPkHex: string;
  /** Vote value: "for", "against", or "abstain". */
  vote: 'for' | 'against' | 'abstain';
  /** Amount of staked ERG backing the vote in nanoERG. */
  stakeNanoerg: bigint;
}

/** Result from voting on a proposal. */
export interface VoteOnProposalResult {
  /** Transaction ID of the vote tx. */
  txId: string;
  /** The proposal ID that was voted on. */
  proposalId: string;
  /** The voter's public key hex. */
  voterPkHex: string;
}

/** A governance proposal entry returned by getGovernanceProposals. */
export interface GovernanceProposal {
  /** Unique proposal ID. */
  proposalId: string;
  /** The proposal box ID. */
  proposalBoxId: string;
  /** Proposal title. */
  title: string;
  /** Proposal description. */
  description: string;
  /** Proposal type. */
  proposalType: string;
  /** Proposer's public key hex. */
  proposerPkHex: string;
  /** Block height when the proposal was created. */
  creationHeight: number;
  /** Block height when voting ends. */
  endHeight: number;
  /** Total votes "for" in nanoERG. */
  votesFor: bigint;
  /** Total votes "against" in nanoERG. */
  votesAgainst: bigint;
  /** Total abstention votes in nanoERG. */
  votesAbstain: bigint;
  /** Whether the proposal is still active. */
  active: boolean;
  /** Whether the proposal has been executed. */
  executed: boolean;
}
