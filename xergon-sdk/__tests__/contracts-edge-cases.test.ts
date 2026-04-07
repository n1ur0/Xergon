/**
 * Edge-case tests for the contracts-api module.
 *
 * Covers scenarios that are hard to trigger in normal operation:
 *   - Empty / malformed responses from the agent
 *   - Network timeout handling
 *   - Invalid or very long hex strings for provider PK / NFT IDs
 *   - Very large nanoERG amounts (u64 max edge cases)
 *   - Concurrent request handling
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { XergonError } from '../src/errors';

// ── Mock Setup ──────────────────────────────────────────────────────────

const mockGet = vi.fn();
const mockPost = vi.fn();

vi.mock('../src/client', () => ({
  XergonClientCore: vi.fn().mockImplementation(() => ({
    get: mockGet,
    post: mockPost,
  })),
}));

vi.mock('../src/ergo-tx', () => ({
  decodeSIntLong: vi.fn(),
  decodeSIntInt: vi.fn(),
}));

import {
  registerProvider,
  queryProviderStatus,
  listOnChainProviders,
  createStakingBox,
  queryUserBalance,
  getUserStakingBoxes,
  getOracleRate,
  getOraclePoolStatus,
  getSettleableBoxes,
  buildSettlementTx,
  createGovernanceProposal,
  voteOnProposal,
  getGovernanceProposals,
} from '../src/contracts-api';

let client: any;

beforeEach(() => {
  vi.clearAllMocks();
  client = { get: mockGet, post: mockPost };
});

// ── Helper ──────────────────────────────────────────────────────────────

function makeXergonError(type: string, message: string, code: number): XergonError {
  return new XergonError({ type: type as any, message, code });
}

// ── Constants ───────────────────────────────────────────────────────────

const MAX_SAFE_BIGINT = 9_223_372_036_854_775_807n;  // 2^63 - 1
const U64_MAX = 18_446_744_073_709_551_615n;           // 2^64 - 1

// ═══════════════════════════════════════════════════════════════════════
// 1. Empty / Malformed Responses
// ═══════════════════════════════════════════════════════════════════════

describe('empty / malformed responses', () => {
  it('registerProvider should propagate when agent returns null body', async () => {
    mockPost.mockResolvedValue(null);
    // The SDK returns whatever the agent returns; null is technically valid JSON
    const result = await registerProvider(client, {
      providerName: 'X', region: 'US', endpoint: 'http://x', models: ['m'],
      ergoAddress: '9eZ24...', providerPkHex: 'a'.repeat(64),
    });
    expect(result).toBeNull();
  });

  it('registerProvider should propagate when agent returns empty object', async () => {
    mockPost.mockResolvedValue({});
    const result = await registerProvider(client, {
      providerName: 'X', region: 'US', endpoint: 'http://x', models: ['m'],
      ergoAddress: '9eZ24...', providerPkHex: 'a'.repeat(64),
    });
    expect(result).toEqual({});
    expect(result.txId).toBeUndefined();
  });

  it('listOnChainProviders should throw when required BigInt fields are missing', async () => {
    mockGet.mockResolvedValue([{ box_id: 'b1' }]);
    // BigInt(undefined) will throw because value_nanoerg is required
    await expect(listOnChainProviders(client)).rejects.toThrow();
  });

  it('listOnChainProviders should handle array with only string fields present', async () => {
    mockGet.mockResolvedValue([{
      box_id: 'b1', provider_nft_id: 'n1', provider_name: 'P',
      endpoint: 'http://p', models: [], region: 'US',
      value_nanoerg: '0', active: true,
    }]);
    const result = await listOnChainProviders(client);
    expect(result).toHaveLength(1);
    expect(result[0].boxId).toBe('b1');
    expect(result[0].valueNanoerg).toBe(0n);
    expect(result[0].models).toEqual([]);
  });

  it('queryProviderStatus should handle response with undefined numeric fields', async () => {
    mockGet.mockResolvedValue({
      box_id: 'b1',
      provider_nft_id: 'nft1',
      provider_name: 'Test',
      endpoint: 'http://t',
      price_per_token: undefined as any,
      min_stake: undefined as any,
      value: '100',
      height: undefined as any,
      confirmations: undefined as any,
    });
    // BigInt(undefined) will throw
    await expect(queryProviderStatus(client, 'nft1')).rejects.toThrow();
  });

  it('createStakingBox should propagate when response amountNanoerg is non-numeric string', async () => {
    mockPost.mockResolvedValue({
      txId: 'tx1',
      stakingBoxId: 'sbox1',
      amountNanoerg: 'not-a-number',
    });
    // SDK just returns the response; the caller decides if the string is valid
    const result = await createStakingBox(client, {
      userPkHex: 'a'.repeat(64), amountNanoerg: 1000n,
    });
    expect(result.amountNanoerg).toBe('not-a-number');
  });

  it('getGovernanceProposals should treat empty string vote fields as 0n', async () => {
    mockGet.mockResolvedValue([{
      proposal_id: 'p1',
      proposal_box_id: 'gbox1',
      title: 'T',
      description: 'D',
      proposal_type: 'x',
      proposer_pk_hex: 'pk',
      creation_height: 1,
      end_height: 100,
      votes_for: '',
      votes_against: '',
      votes_abstain: '',
      active: false,
      executed: false,
    }]);
    // BigInt('') returns 0n in JavaScript
    const result = await getGovernanceProposals(client);
    expect(result[0].votesFor).toBe(0n);
    expect(result[0].votesAgainst).toBe(0n);
    expect(result[0].votesAbstain).toBe(0n);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. Network Timeout Handling
// ═══════════════════════════════════════════════════════════════════════

describe('network timeout handling', () => {
  it('registerProvider should propagate AbortError', async () => {
    const abortError = new DOMException('The operation was aborted', 'AbortError');
    mockPost.mockRejectedValue(abortError);

    await expect(
      registerProvider(client, {
        providerName: 'X', region: 'US', endpoint: 'http://x', models: ['m'],
        ergoAddress: '9eZ24...', providerPkHex: 'a'.repeat(64),
      }),
    ).rejects.toThrow('The operation was aborted');
  });

  it('queryProviderStatus should propagate generic timeout Error', async () => {
    mockGet.mockRejectedValue(new TypeError('fetch failed'));

    await expect(queryProviderStatus(client, 'nft1')).rejects.toThrow('fetch failed');
  });

  it('listOnChainProviders should propagate ECONNREFUSED-style error', async () => {
    mockGet.mockRejectedValue(new Error('connect ECONNREFUSED 127.0.0.1:9090'));

    await expect(listOnChainProviders(client)).rejects.toThrow('ECONNREFUSED');
  });

  it('createStakingBox should propagate DNS resolution failure', async () => {
    mockPost.mockRejectedValue(new Error('getaddrinfo ENOTFOUND relay.xergon.gg'));

    await expect(
      createStakingBox(client, { userPkHex: 'a'.repeat(64), amountNanoerg: 1000n }),
    ).rejects.toThrow('ENOTFOUND');
  });

  it('buildSettlementTx should propagate socket hang-up error', async () => {
    mockPost.mockRejectedValue(new Error('socket hang up'));

    await expect(
      buildSettlementTx(client, {
        stakingBoxIds: ['b1'], feeAmounts: [100n], providerAddress: '9eZ24...', maxFeeNanoerg: 1000n,
      }),
    ).rejects.toThrow('socket hang up');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. Invalid / Malformed ErgoTree Hex & NFT IDs
// ═══════════════════════════════════════════════════════════════════════

describe('invalid hex strings and IDs', () => {
  it('queryProviderStatus should accept short NFT IDs without validation', async () => {
    // The SDK doesn't validate hex length; the agent does
    mockGet.mockResolvedValue({
      box_id: 'b1', provider_nft_id: 'short', provider_name: 'T',
      endpoint: 'http://t', price_per_token: '100', min_stake: '1000',
      value: '5000', height: 100, confirmations: 5,
    });
    const result = await queryProviderStatus(client, 'short');
    expect(result.providerNftId).toBe('short');
  });

  it('queryProviderStatus should accept NFT ID with odd-length hex', async () => {
    mockGet.mockResolvedValue({
      box_id: 'b1', provider_nft_id: 'abc', provider_name: 'T',
      endpoint: 'http://t', price_per_token: '100', min_stake: '1000',
      value: '5000', height: 100, confirmations: 5,
    });
    const result = await queryProviderStatus(client, 'abc');
    expect(result.providerNftId).toBe('abc');
  });

  it('queryUserBalance should pass through arbitrary PK strings', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: '!!!invalid-hex!!!',
      total_balance_nanoerg: '0', staking_box_count: 0, boxes: [],
    });
    const result = await queryUserBalance(client, '!!!invalid-hex!!!');
    expect(result.userPkHex).toBe('!!!invalid-hex!!!');
  });

  it('registerProvider should accept very short provider PK', async () => {
    mockPost.mockResolvedValue({ txId: 't1', providerNftId: 'n1', providerBoxId: 'b1' });
    const result = await registerProvider(client, {
      providerName: 'X', region: 'US', endpoint: 'http://x', models: ['m'],
      ergoAddress: '9eZ24...', providerPkHex: 'ab',
    });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/provider/register',
      expect.objectContaining({ provider_pk_hex: 'ab' }),
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. Very Long Provider PK Hex Strings
// ═══════════════════════════════════════════════════════════════════════

describe('very long provider PK hex strings', () => {
  const longPk = 'a'.repeat(1024); // 512 bytes -- far longer than 32-byte key

  it('registerProvider should pass through very long provider PK to agent', async () => {
    mockPost.mockResolvedValue({ txId: 't1', providerNftId: 'n1', providerBoxId: 'b1' });
    await registerProvider(client, {
      providerName: 'LongKey', region: 'US', endpoint: 'http://x', models: ['m'],
      ergoAddress: '9eZ24...', providerPkHex: longPk,
    });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/provider/register',
      expect.objectContaining({ provider_pk_hex: longPk }),
    );
  });

  it('createStakingBox should pass through very long user PK', async () => {
    mockPost.mockResolvedValue({ txId: 't1', stakingBoxId: 's1', amountNanoerg: '1000' });
    await createStakingBox(client, { userPkHex: longPk, amountNanoerg: 1000n });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/staking/create',
      expect.objectContaining({ user_pk_hex: longPk }),
    );
  });

  it('queryUserBalance should URL-encode very long PK', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: longPk, total_balance_nanoerg: '0', staking_box_count: 0, boxes: [],
    });
    await queryUserBalance(client, longPk);
    // The PK is passed directly; encodeURIComponent is only for special chars
    expect(mockGet).toHaveBeenCalledWith(`/v1/contracts/staking/balance/${longPk}`);
  });

  it('voteOnProposal should pass through very long voter PK', async () => {
    mockPost.mockResolvedValue({ txId: 't1', proposalId: 'p1', voterPkHex: longPk });
    await voteOnProposal(client, {
      proposalId: 'p1', voterPkHex: longPk, vote: 'for', stakeNanoerg: 1000n,
    });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/vote',
      expect.objectContaining({ voter_pk_hex: longPk }),
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 5. Very Large nanoERG Amounts (u64 Edge Cases)
// ═══════════════════════════════════════════════════════════════════════

describe('very large nanoERG amounts (u64 edge cases)', () => {

  it('createStakingBox should serialize MAX_SAFE_BIGINT as string', async () => {
    mockPost.mockResolvedValue({ txId: 't1', stakingBoxId: 's1', amountNanoerg: MAX_SAFE_BIGINT.toString() });
    await createStakingBox(client, { userPkHex: 'a'.repeat(64), amountNanoerg: MAX_SAFE_BIGINT });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/staking/create',
      expect.objectContaining({ amount_nanoerg: MAX_SAFE_BIGINT.toString() }),
    );
  });

  it('createStakingBox should serialize U64_MAX as string', async () => {
    mockPost.mockResolvedValue({ txId: 't1', stakingBoxId: 's1', amountNanoerg: U64_MAX.toString() });
    await createStakingBox(client, { userPkHex: 'a'.repeat(64), amountNanoerg: U64_MAX });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/staking/create',
      expect.objectContaining({ amount_nanoerg: U64_MAX.toString() }),
    );
  });

  it('buildSettlementTx should serialize very large fee amounts', async () => {
    mockPost.mockResolvedValue({
      unsigned_tx: { id: 'utx' },
      total_fees_nanoerg: U64_MAX.toString(),
      net_settlement_nanoerg: U64_MAX.toString(),
      estimated_tx_fee: MAX_SAFE_BIGINT.toString(),
    });
    const result = await buildSettlementTx(client, {
      stakingBoxIds: ['b1'], feeAmounts: [U64_MAX],
      providerAddress: '9eZ24...', maxFeeNanoerg: U64_MAX,
    });
    expect(result.totalFeesNanoerg).toBe(U64_MAX);
    expect(result.netSettlementNanoerg).toBe(U64_MAX);
    expect(result.estimatedTxFee).toBe(MAX_SAFE_BIGINT);
  });

  it('queryUserBalance should parse u64 max from response', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'pk',
      total_balance_nanoerg: U64_MAX.toString(),
      staking_box_count: 1,
      boxes: [{
        box_id: 's1', value_nanoerg: U64_MAX.toString(),
        creation_height: 800000, confirmations: 100,
      }],
    });
    const result = await queryUserBalance(client, 'pk');
    expect(result.totalBalanceNanoerg).toBe(U64_MAX);
    expect(result.boxes[0].valueNanoerg).toBe(U64_MAX);
  });

  it('queryProviderStatus should parse u64 max price_per_token', async () => {
    mockGet.mockResolvedValue({
      box_id: 'b1', provider_nft_id: 'nft1', provider_name: 'T',
      endpoint: 'http://t', price_per_token: U64_MAX.toString(),
      min_stake: U64_MAX.toString(), value: U64_MAX.toString(),
      height: 2_000_000, confirmations: 1_000_000,
    });
    const result = await queryProviderStatus(client, 'nft1');
    expect(result.pricePerToken).toBe(U64_MAX);
    expect(result.minStake).toBe(U64_MAX);
    expect(result.value).toBe(U64_MAX);
    expect(result.height).toBe(2_000_000);
    expect(result.confirmations).toBe(1_000_000);
  });

  it('getSettleableBoxes should parse u64 max values', async () => {
    mockGet.mockResolvedValue([{
      box_id: 's1', value_nanoerg: U64_MAX.toString(),
      user_pk_hex: 'u1', provider_nft_id: 'p1',
      fee_amount_nanoerg: U64_MAX.toString(),
    }]);
    const result = await getSettleableBoxes(client, 1);
    expect(result[0].valueNanoerg).toBe(U64_MAX);
    expect(result[0].feeAmountNanoerg).toBe(U64_MAX);
  });

  it('getGovernanceProposals should parse u64 max vote tallies', async () => {
    mockGet.mockResolvedValue([{
      proposal_id: 'p1', proposal_box_id: 'g1', title: 'T', description: 'D',
      proposal_type: 'x', proposer_pk_hex: 'pk', creation_height: 1, end_height: 100,
      votes_for: U64_MAX.toString(), votes_against: U64_MAX.toString(),
      votes_abstain: U64_MAX.toString(), active: true, executed: false,
    }]);
    const result = await getGovernanceProposals(client);
    expect(result[0].votesFor).toBe(U64_MAX);
    expect(result[0].votesAgainst).toBe(U64_MAX);
    expect(result[0].votesAbstain).toBe(U64_MAX);
  });

  it('voteOnProposal should serialize u64 max stake', async () => {
    mockPost.mockResolvedValue({ txId: 't1', proposalId: 'p1', voterPkHex: 'v' });
    await voteOnProposal(client, {
      proposalId: 'p1', voterPkHex: 'v', vote: 'for', stakeNanoerg: U64_MAX,
    });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/vote',
      expect.objectContaining({ stake_nanoerg: U64_MAX.toString() }),
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 6. Concurrent Request Handling
// ═══════════════════════════════════════════════════════════════════════

describe('concurrent request handling', () => {
  it('multiple registerProvider calls should resolve independently', async () => {
    mockPost.mockImplementation(async (_path: string, params: any) => ({
      txId: `tx_${params.provider_name}`,
      providerNftId: `nft_${params.provider_name}`,
      providerBoxId: `box_${params.provider_name}`,
    }));

    const results = await Promise.all([
      registerProvider(client, {
        providerName: 'A', region: 'US', endpoint: 'http://a', models: ['m'],
        ergoAddress: '9eZ24...', providerPkHex: 'a'.repeat(64),
      }),
      registerProvider(client, {
        providerName: 'B', region: 'EU', endpoint: 'http://b', models: ['m'],
        ergoAddress: '9eZ24...', providerPkHex: 'b'.repeat(64),
      }),
      registerProvider(client, {
        providerName: 'C', region: 'AS', endpoint: 'http://c', models: ['m'],
        ergoAddress: '9eZ24...', providerPkHex: 'c'.repeat(64),
      }),
    ]);

    expect(results).toHaveLength(3);
    expect(results[0].txId).toBe('tx_A');
    expect(results[1].txId).toBe('tx_B');
    expect(results[2].txId).toBe('tx_C');
    expect(mockPost).toHaveBeenCalledTimes(3);
  });

  it('mixed concurrent reads (list + query) should resolve independently', async () => {
    mockGet.mockImplementation(async (path: string) => {
      if (path === '/v1/contracts/providers') {
        return [{ box_id: 'b1', provider_nft_id: 'n1', provider_name: 'P',
          endpoint: 'http://p', models: ['m'], region: 'US',
          value_nanoerg: '1000', active: true }];
      }
      if (path === '/v1/contracts/oracle/rate') {
        return { rate: '350000000', epoch: 42, box_id: 'o1', erg_usd: 0.35 };
      }
      if (path === '/v1/contracts/staking/balance/testpk') {
        return { user_pk_hex: 'testpk', total_balance_nanoerg: '5000',
          staking_box_count: 0, boxes: [] };
      }
      return null;
    });

    const [providers, oracle, balance] = await Promise.all([
      listOnChainProviders(client),
      getOracleRate(client),
      queryUserBalance(client, 'testpk'),
    ]);

    expect(providers).toHaveLength(1);
    expect(oracle.rate).toBe(0.35);
    expect(balance.totalBalanceNanoerg).toBe(5000n);
    expect(mockGet).toHaveBeenCalledTimes(3);
  });

  it('concurrent requests with one failure should not affect others', async () => {
    mockGet.mockImplementation(async (path: string) => {
      if (path.includes('oracle')) {
        throw makeXergonError('service_unavailable', 'Oracle down', 503);
      }
      return [{ box_id: 'b1', provider_nft_id: 'n1', provider_name: 'P',
        endpoint: 'http://p', models: ['m'], region: 'US',
        value_nanoerg: '1000', active: true }];
    });

    const results = await Promise.allSettled([
      listOnChainProviders(client),
      getOracleRate(client),
    ]);

    expect(results[0].status).toBe('fulfilled');
    expect((results[0] as PromiseFulfilledResult<any>).value).toHaveLength(1);
    expect(results[1].status).toBe('rejected');
    expect((results[1] as PromiseRejectedResult).reason).toBeInstanceOf(XergonError);
  });

  it('concurrent createStakingBox calls with delayed responses', async () => {
    // Assign unique IDs before delaying
    let callCount = 0;
    mockPost.mockImplementation(async (_path: string, _body: any) => {
      const id = ++callCount;
      const delay = id * 10;
      await new Promise((r) => setTimeout(r, delay));
      return { txId: `tx${id}`, stakingBoxId: `sbox${id}`, amountNanoerg: '1000' };
    });

    const results = await Promise.all([
      createStakingBox(client, { userPkHex: 'a'.repeat(64), amountNanoerg: 1000n }),
      createStakingBox(client, { userPkHex: 'b'.repeat(64), amountNanoerg: 2000n }),
      createStakingBox(client, { userPkHex: 'c'.repeat(64), amountNanoerg: 3000n }),
    ]);

    expect(results).toHaveLength(3);
    // All three should complete successfully with distinct txIds
    const txIds = results.map((r) => r.txId);
    expect(new Set(txIds).size).toBe(3);
    expect(txIds).toContain('tx1');
    expect(txIds).toContain('tx2');
    expect(txIds).toContain('tx3');
    expect(mockPost).toHaveBeenCalledTimes(3);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 7. Additional Edge Cases
// ═══════════════════════════════════════════════════════════════════════

describe('additional edge cases', () => {
  it('queryUserBalance should handle null boxes array in response', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'pk', total_balance_nanoerg: '0', staking_box_count: 0,
      boxes: null as any,
    });
    // This will crash because null.map is not a function
    await expect(queryUserBalance(client, 'pk')).rejects.toThrow();
  });

  it('listOnChainProviders should handle null response', async () => {
    mockGet.mockResolvedValue(null);
    // null.map will throw
    await expect(listOnChainProviders(client)).rejects.toThrow();
  });

  it('getSettleableBoxes should handle maxBoxes of 0', async () => {
    mockGet.mockResolvedValue([]);
    const result = await getSettleableBoxes(client, 0);
    expect(result).toHaveLength(0);
    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/settlement/settleable', {
      headers: { 'X-Max-Boxes': '0' },
    });
  });

  it('getSettleableBoxes should handle negative maxBoxes', async () => {
    mockGet.mockResolvedValue([]);
    const result = await getSettleableBoxes(client, -1);
    expect(result).toHaveLength(0);
    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/settlement/settleable', {
      headers: { 'X-Max-Boxes': '-1' },
    });
  });

  it('getOraclePoolStatus should handle very large epoch and height values', async () => {
    mockGet.mockResolvedValue({
      epoch: Number.MAX_SAFE_INTEGER,
      erg_usd: 9999.9999,
      rate: U64_MAX.toString(),
      pool_box_id: 'obox',
      last_update_height: Number.MAX_SAFE_INTEGER,
    });
    const result = await getOraclePoolStatus(client);
    expect(result.epoch).toBe(Number.MAX_SAFE_INTEGER);
    expect(result.lastUpdateHeight).toBe(Number.MAX_SAFE_INTEGER);
    expect(result.rate).toBe(U64_MAX);
  });

  it('getOracleRate should handle zero ERG/USD rate', async () => {
    mockGet.mockResolvedValue({
      rate: '0', epoch: 0, box_id: 'obox', erg_usd: 0,
    });
    const result = await getOracleRate(client);
    expect(result.rate).toBe(0);
    expect(result.epoch).toBe(0);
  });

  it('createGovernanceProposal should handle extremely long title and description', async () => {
    mockPost.mockResolvedValue({ txId: 't1', proposalBoxId: 'g1', proposalId: 'p1' });
    const longTitle = 'A'.repeat(10000);
    const longDesc = 'B'.repeat(100000);
    await createGovernanceProposal(client, {
      title: longTitle, description: longDesc, proposalType: 'test',
      proposalData: '{}', proposerPkHex: 'a'.repeat(64), votingDurationBlocks: 7200,
    });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/proposal',
      expect.objectContaining({ title: longTitle, description: longDesc }),
    );
  });

  it('getGovernanceProposals should handle response with active=false and executed=false', async () => {
    mockGet.mockResolvedValue([{
      proposal_id: 'p1', proposal_box_id: 'g1', title: 'T', description: 'D',
      proposal_type: 'x', proposer_pk_hex: 'pk', creation_height: 1, end_height: 100,
      votes_for: '0', votes_against: '0', votes_abstain: '0',
      active: false, executed: false,
    }]);
    const result = await getGovernanceProposals(client);
    expect(result[0].active).toBe(false);
    expect(result[0].executed).toBe(false);
  });

  it('buildSettlementTx should handle empty unsigned_tx object', async () => {
    mockPost.mockResolvedValue({
      unsigned_tx: {},
      total_fees_nanoerg: '0',
      net_settlement_nanoerg: '0',
      estimated_tx_fee: '0',
    });
    const result = await buildSettlementTx(client, {
      stakingBoxIds: ['b1'], feeAmounts: [0n], providerAddress: '9eZ24...', maxFeeNanoerg: 0n,
    });
    expect(result.unsignedTx).toEqual({});
    expect(result.totalFeesNanoerg).toBe(0n);
  });

  it('registerProvider should handle empty models array', async () => {
    mockPost.mockResolvedValue({ txId: 't1', providerNftId: 'n1', providerBoxId: 'b1' });
    await registerProvider(client, {
      providerName: 'NoModels', region: 'US', endpoint: 'http://x', models: [],
      ergoAddress: '9eZ24...', providerPkHex: 'a'.repeat(64),
    });
    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/provider/register',
      expect.objectContaining({ models: [] }),
    );
  });

  it('rate_limit_error (429) should propagate as XergonError', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('rate_limit_error', 'Too many requests', 429),
    );
    try {
      await registerProvider(client, {
        providerName: 'X', region: 'US', endpoint: 'http://x', models: ['m'],
        ergoAddress: '9eZ24...', providerPkHex: 'a'.repeat(64),
      });
      expect.fail('Should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(XergonError);
      expect((err as XergonError).isRateLimited).toBe(true);
      expect((err as XergonError).code).toBe(429);
    }
  });

  it('service_unavailable (503) should propagate with correct flag', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('service_unavailable', 'No providers available', 503),
    );
    try {
      await listOnChainProviders(client);
      expect.fail('Should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(XergonError);
      expect((err as XergonError).isServiceUnavailable).toBe(true);
    }
  });
});
