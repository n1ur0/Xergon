/**
 * Comprehensive end-to-end tests for the contracts-api module.
 *
 * Tests all 10 contract API methods against mocked HTTP layer:
 *   1. registerProvider
 *   2. queryProviderStatus
 *   3. listOnChainProviders
 *   4. createStakingBox
 *   5. queryUserBalance
 *   6. getUserStakingBoxes
 *   7. buildSettlementTx
 *   8. getSettleableBoxes
 *   9. getOraclePoolStatus
 *  10. createGovernanceProposal
 *  11. voteOnProposal
 *  12. getGovernanceProposals
 *
 * Each method has success and error/edge cases.
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
  client = {
    get: mockGet,
    post: mockPost,
  };
});

// ── Helper ──────────────────────────────────────────────────────────────

/** Create an XergonError matching what the client throws on non-2xx. */
function makeXergonError(type: string, message: string, code: number): XergonError {
  return new XergonError({ type: type as any, message, code });
}

// ═══════════════════════════════════════════════════════════════════════
// 1. registerProvider
// ═══════════════════════════════════════════════════════════════════════

describe('registerProvider', () => {
  const successResponse = {
    txId: 'tx_abc123',
    providerNftId: 'nft_456def',
    providerBoxId: 'box_789ghi',
  };

  const validParams = {
    providerName: 'MyGPU',
    region: 'US',
    endpoint: 'https://gpu.example.com',
    models: ['llama-3.3-70b', 'mistral-small-24b'],
    ergoAddress: '9eZ24K1s...',
    providerPkHex: 'abcd1234ef567890abcd1234ef567890abcd1234ef567890abcd1234ef567890',
  };

  it('should POST to /v1/contracts/provider/register with snake_case params', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await registerProvider(client, validParams);

    expect(mockPost).toHaveBeenCalledTimes(1);
    expect(mockPost).toHaveBeenCalledWith('/v1/contracts/provider/register', {
      provider_name: 'MyGPU',
      region: 'US',
      endpoint: 'https://gpu.example.com',
      models: ['llama-3.3-70b', 'mistral-small-24b'],
      ergo_address: '9eZ24K1s...',
      provider_pk_hex: 'abcd1234ef567890abcd1234ef567890abcd1234ef567890abcd1234ef567890',
    });
    expect(result).toEqual(successResponse);
  });

  it('should return txId, providerNftId, and providerBoxId on success', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await registerProvider(client, validParams);

    expect(result.txId).toBe('tx_abc123');
    expect(result.providerNftId).toBe('nft_456def');
    expect(result.providerBoxId).toBe('box_789ghi');
  });

  it('should handle single model in models array', async () => {
    mockPost.mockResolvedValue(successResponse);

    await registerProvider(client, { ...validParams, models: ['llama-3.3-70b'] });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/provider/register',
      expect.objectContaining({ models: ['llama-3.3-70b'] }),
    );
  });

  it('should propagate errors from the agent', async () => {
    mockPost.mockRejectedValue(makeXergonError('invalid_request', 'Missing provider_name', 400));

    await expect(registerProvider(client, validParams)).rejects.toThrow('Missing provider_name');
  });

  it('should propagate unauthorized errors', async () => {
    mockPost.mockRejectedValue(makeXergonError('unauthorized', 'Invalid HMAC signature', 401));

    await expect(registerProvider(client, validParams)).rejects.toThrow('Invalid HMAC signature');
  });

  it('should propagate internal server errors', async () => {
    mockPost.mockRejectedValue(makeXergonError('internal_error', 'Node connection failed', 500));

    await expect(registerProvider(client, validParams)).rejects.toThrow('Node connection failed');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. queryProviderStatus
// ═══════════════════════════════════════════════════════════════════════

describe('queryProviderStatus', () => {
  const successResponse = {
    box_id: 'box1',
    provider_nft_id: 'nft1',
    provider_name: 'MyGPU',
    endpoint: 'https://gpu.example.com',
    price_per_token: '1000000',
    min_stake: '5000000000',
    value: '1000000000',
    height: 800000,
    confirmations: 42,
  };

  it('should GET from /v1/contracts/provider/status with NFT ID header', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await queryProviderStatus(client, 'nft1');

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/provider/status', {
      headers: { 'X-Provider-Nft-Id': 'nft1' },
    });
  });

  it('should convert snake_case to camelCase and parse BigInt fields', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await queryProviderStatus(client, 'nft1');

    expect(result.boxId).toBe('box1');
    expect(result.providerNftId).toBe('nft1');
    expect(result.providerName).toBe('MyGPU');
    expect(result.endpoint).toBe('https://gpu.example.com');
    expect(result.pricePerToken).toBe(1000000n);
    expect(result.minStake).toBe(5000000000n);
    expect(result.value).toBe(1000000000n);
    expect(result.height).toBe(800000);
    expect(result.confirmations).toBe(42);
  });

  it('should handle zero confirmations (freshly created box)', async () => {
    mockGet.mockResolvedValue({
      ...successResponse,
      confirmations: 0,
      height: 800001,
    });

    const result = await queryProviderStatus(client, 'nft1');
    expect(result.confirmations).toBe(0);
  });

  it('should throw not_found error when provider box does not exist', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('not_found', 'Provider box not found for NFT ID: nonexistent_nft', 404),
    );

    await expect(queryProviderStatus(client, 'nonexistent_nft')).rejects.toThrow(
      'Provider box not found',
    );
  });

  it('should propagate network errors', async () => {
    mockGet.mockRejectedValue(new Error('Network timeout'));

    await expect(queryProviderStatus(client, 'nft1')).rejects.toThrow('Network timeout');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. listOnChainProviders
// ═══════════════════════════════════════════════════════════════════════

describe('listOnChainProviders', () => {
  it('should GET from /v1/contracts/providers and map all fields', async () => {
    mockGet.mockResolvedValue([
      {
        box_id: 'box1',
        provider_nft_id: 'nft1',
        provider_name: 'ProviderA',
        endpoint: 'https://a.example.com',
        models: ['llama-3.3-70b'],
        region: 'US',
        value_nanoerg: '2000000000',
        active: true,
      },
      {
        box_id: 'box2',
        provider_nft_id: 'nft2',
        provider_name: 'ProviderB',
        endpoint: 'https://b.example.com',
        models: ['mistral-small-24b'],
        region: 'EU',
        value_nanoerg: '3000000000',
        active: false,
      },
    ]);

    const result = await listOnChainProviders(client);

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/providers');
    expect(result).toHaveLength(2);
    expect(result[0].providerName).toBe('ProviderA');
    expect(result[0].valueNanoerg).toBe(2000000000n);
    expect(result[0].active).toBe(true);
    expect(result[0].models).toEqual(['llama-3.3-70b']);
    expect(result[1].region).toBe('EU');
    expect(result[1].active).toBe(false);
    expect(result[1].valueNanoerg).toBe(3000000000n);
  });

  it('should return empty array when no providers exist', async () => {
    mockGet.mockResolvedValue([]);

    const result = await listOnChainProviders(client);
    expect(result).toHaveLength(0);
  });

  it('should handle providers with multiple models', async () => {
    mockGet.mockResolvedValue([
      {
        box_id: 'box1',
        provider_nft_id: 'nft1',
        provider_name: 'MultiModel',
        endpoint: 'https://multi.example.com',
        models: ['llama-3.3-70b', 'mistral-small-24b', 'qwen-2.5-72b'],
        region: 'US',
        value_nanoerg: '5000000000',
        active: true,
      },
    ]);

    const result = await listOnChainProviders(client);
    expect(result[0].models).toHaveLength(3);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(makeXergonError('internal_error', 'Scanning failed', 500));

    await expect(listOnChainProviders(client)).rejects.toThrow('Scanning failed');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. createStakingBox
// ═══════════════════════════════════════════════════════════════════════

describe('createStakingBox', () => {
  const successResponse = {
    txId: 'stx_001',
    stakingBoxId: 'sbox_001',
    amountNanoerg: '5000000000',
  };

  it('should POST to /v1/contracts/staking/create with bigint as string', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await createStakingBox(client, {
      userPkHex: 'userpk1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
      amountNanoerg: 5000000000n,
    });

    expect(mockPost).toHaveBeenCalledWith('/v1/contracts/staking/create', {
      user_pk_hex: 'userpk1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
      amount_nanoerg: '5000000000',
    });
    expect(result.txId).toBe('stx_001');
    expect(result.stakingBoxId).toBe('sbox_001');
  });

  it('should handle large staking amounts', async () => {
    mockPost.mockResolvedValue({
      ...successResponse,
      amountNanoerg: '100000000000000', // 100,000 ERG
    });

    const result = await createStakingBox(client, {
      userPkHex: 'userpk',
      amountNanoerg: 100000000000000n,
    });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/staking/create',
      expect.objectContaining({ amount_nanoerg: '100000000000000' }),
    );
  });

  it('should propagate insufficient funds error', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Insufficient ERG balance for staking', 400),
    );

    await expect(
      createStakingBox(client, { userPkHex: 'userpk', amountNanoerg: 5000000000n }),
    ).rejects.toThrow('Insufficient ERG balance');
  });

  it('should propagate unauthorized error', async () => {
    mockPost.mockRejectedValue(makeXergonError('unauthorized', 'Invalid public key', 401));

    await expect(
      createStakingBox(client, { userPkHex: 'badpk', amountNanoerg: 5000000000n }),
    ).rejects.toThrow('Invalid public key');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 5. queryUserBalance
// ═══════════════════════════════════════════════════════════════════════

describe('queryUserBalance', () => {
  it('should GET from /v1/contracts/staking/balance/{pk} and parse all fields', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'userpk',
      total_balance_nanoerg: '10000000000',
      staking_box_count: 2,
      boxes: [
        {
          box_id: 'sbox1',
          value_nanoerg: '6000000000',
          creation_height: 799000,
          confirmations: 1000,
        },
        {
          box_id: 'sbox2',
          value_nanoerg: '4000000000',
          creation_height: 800000,
          confirmations: 500,
        },
      ],
    });

    const result = await queryUserBalance(client, 'userpk');

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/staking/balance/userpk');
    expect(result.userPkHex).toBe('userpk');
    expect(result.totalBalanceNanoerg).toBe(10000000000n);
    expect(result.stakingBoxCount).toBe(2);
    expect(result.boxes).toHaveLength(2);
    expect(result.boxes[0].boxId).toBe('sbox1');
    expect(result.boxes[0].valueNanoerg).toBe(6000000000n);
    expect(result.boxes[0].creationHeight).toBe(799000);
    expect(result.boxes[0].confirmations).toBe(1000);
    expect(result.boxes[1].valueNanoerg).toBe(4000000000n);
  });

  it('should return zero balance with empty boxes array', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'nopk',
      total_balance_nanoerg: '0',
      staking_box_count: 0,
      boxes: [],
    });

    const result = await queryUserBalance(client, 'nopk');
    expect(result.totalBalanceNanoerg).toBe(0n);
    expect(result.stakingBoxCount).toBe(0);
    expect(result.boxes).toHaveLength(0);
  });

  it('should URL-encode special characters in user PK', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'a/b+c',
      total_balance_nanoerg: '0',
      staking_box_count: 0,
      boxes: [],
    });

    await queryUserBalance(client, 'a/b+c');

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/staking/balance/a%2Fb%2Bc');
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Failed to query staking boxes', 500),
    );

    await expect(queryUserBalance(client, 'userpk')).rejects.toThrow(
      'Failed to query staking boxes',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 6. getUserStakingBoxes
// ═══════════════════════════════════════════════════════════════════════

describe('getUserStakingBoxes', () => {
  it('should GET from /v1/contracts/staking/boxes/{pk} and map boxes', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'userpk',
      total_balance_nanoerg: '5000000000',
      staking_box_count: 1,
      boxes: [
        {
          box_id: 'sbox1',
          value_nanoerg: '5000000000',
          creation_height: 800000,
          confirmations: 100,
        },
      ],
    });

    const result = await getUserStakingBoxes(client, 'userpk');

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/staking/boxes/userpk');
    expect(result).toHaveLength(1);
    expect(result[0].boxId).toBe('sbox1');
    expect(result[0].valueNanoerg).toBe(5000000000n);
    expect(result[0].creationHeight).toBe(800000);
    expect(result[0].confirmations).toBe(100);
  });

  it('should return empty array when user has no staking boxes', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'nopk',
      total_balance_nanoerg: '0',
      staking_box_count: 0,
      boxes: [],
    });

    const result = await getUserStakingBoxes(client, 'nopk');
    expect(result).toHaveLength(0);
  });

  it('should handle multiple staking boxes', async () => {
    mockGet.mockResolvedValue({
      user_pk_hex: 'userpk',
      total_balance_nanoerg: '15000000000',
      staking_box_count: 3,
      boxes: [
        { box_id: 'sb1', value_nanoerg: '5000000000', creation_height: 799000, confirmations: 1100 },
        { box_id: 'sb2', value_nanoerg: '5000000000', creation_height: 800000, confirmations: 600 },
        { box_id: 'sb3', value_nanoerg: '5000000000', creation_height: 801000, confirmations: 100 },
      ],
    });

    const result = await getUserStakingBoxes(client, 'userpk');
    expect(result).toHaveLength(3);
    expect(result[2].confirmations).toBe(100);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(makeXergonError('not_found', 'User has no staking boxes', 404));

    await expect(getUserStakingBoxes(client, 'nopk')).rejects.toThrow('User has no staking boxes');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 7. buildSettlementTx
// ═══════════════════════════════════════════════════════════════════════

describe('buildSettlementTx', () => {
  const mockUnsignedTx = { id: 'unsigned-tx-123', inputs: [], outputs: [] };
  const successResponse = {
    unsigned_tx: mockUnsignedTx,
    total_fees_nanoerg: '1000000',
    net_settlement_nanoerg: '900000',
    estimated_tx_fee: '100000',
  };

  it('should POST to /v1/contracts/settlement/build with snake_case params', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await buildSettlementTx(client, {
      stakingBoxIds: ['sbox1', 'sbox2'],
      feeAmounts: [500000n, 500000n],
      providerAddress: '9eZ24K1s...',
      maxFeeNanoerg: 1100000n,
    });

    expect(mockPost).toHaveBeenCalledWith('/v1/contracts/settlement/build', {
      staking_box_ids: ['sbox1', 'sbox2'],
      fee_amounts: ['500000', '500000'],
      provider_address: '9eZ24K1s...',
      max_fee_nanoerg: '1100000',
    });
  });

  it('should convert all bigint fields to strings and parse response BigInts', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await buildSettlementTx(client, {
      stakingBoxIds: ['sbox1'],
      feeAmounts: [1000000n],
      providerAddress: '9eZ24...',
      maxFeeNanoerg: 200000n,
    });

    expect(result.unsignedTx).toEqual(mockUnsignedTx);
    expect(result.totalFeesNanoerg).toBe(1000000n);
    expect(result.netSettlementNanoerg).toBe(900000n);
    expect(result.estimatedTxFee).toBe(100000n);
  });

  it('should handle validation error for empty box IDs', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'staking_box_ids must not be empty', 400),
    );

    await expect(
      buildSettlementTx(client, {
        stakingBoxIds: [],
        feeAmounts: [],
        providerAddress: '9eZ24...',
        maxFeeNanoerg: 100000n,
      }),
    ).rejects.toThrow('staking_box_ids must not be empty');
  });

  it('should handle validation error for mismatched box IDs and fee amounts', async () => {
    mockPost.mockRejectedValue(
      makeXergonError(
        'invalid_request',
        'staking_box_ids length (2) must match fee_amounts length (3)',
        400,
      ),
    );

    await expect(
      buildSettlementTx(client, {
        stakingBoxIds: ['sbox1', 'sbox2'],
        feeAmounts: [100n, 200n, 300n],
        providerAddress: '9eZ24...',
        maxFeeNanoerg: 100000n,
      }),
    ).rejects.toThrow('length (2) must match fee_amounts length (3)');
  });

  it('should propagate box already settled error', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Box sbox1 has already been settled', 400),
    );

    await expect(
      buildSettlementTx(client, {
        stakingBoxIds: ['sbox1'],
        feeAmounts: [100n],
        providerAddress: '9eZ24...',
        maxFeeNanoerg: 100000n,
      }),
    ).rejects.toThrow('already been settled');
  });

  it('should propagate service unavailable errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('service_unavailable', 'Node is syncing, try again later', 503),
    );

    await expect(
      buildSettlementTx(client, {
        stakingBoxIds: ['sbox1'],
        feeAmounts: [100n],
        providerAddress: '9eZ24...',
        maxFeeNanoerg: 100000n,
      }),
    ).rejects.toThrow('Node is syncing');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 8. getSettleableBoxes
// ═══════════════════════════════════════════════════════════════════════

describe('getSettleableBoxes', () => {
  it('should GET from /v1/contracts/settlement/settleable with X-Max-Boxes header', async () => {
    mockGet.mockResolvedValue([
      {
        box_id: 'sbox1',
        value_nanoerg: '1000000000',
        user_pk_hex: 'user1',
        provider_nft_id: 'pnft1',
        fee_amount_nanoerg: '500000',
      },
    ]);

    const result = await getSettleableBoxes(client, 20);

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/settlement/settleable', {
      headers: { 'X-Max-Boxes': '20' },
    });
    expect(result).toHaveLength(1);
    expect(result[0].boxId).toBe('sbox1');
    expect(result[0].valueNanoerg).toBe(1000000000n);
    expect(result[0].userPkHex).toBe('user1');
    expect(result[0].providerNftId).toBe('pnft1');
    expect(result[0].feeAmountNanoerg).toBe(500000n);
  });

  it('should use default max of 50 when not specified', async () => {
    mockGet.mockResolvedValue([]);
    await getSettleableBoxes(client);
    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/settlement/settleable', {
      headers: { 'X-Max-Boxes': '50' },
    });
  });

  it('should return empty array when no settleable boxes exist', async () => {
    mockGet.mockResolvedValue([]);
    const result = await getSettleableBoxes(client, 10);
    expect(result).toHaveLength(0);
  });

  it('should handle multiple settleable boxes with different providers', async () => {
    mockGet.mockResolvedValue([
      {
        box_id: 'sbox1',
        value_nanoerg: '1000000000',
        user_pk_hex: 'user1',
        provider_nft_id: 'pnft1',
        fee_amount_nanoerg: '500000',
      },
      {
        box_id: 'sbox2',
        value_nanoerg: '2000000000',
        user_pk_hex: 'user2',
        provider_nft_id: 'pnft2',
        fee_amount_nanoerg: '1200000',
      },
      {
        box_id: 'sbox3',
        value_nanoerg: '3000000000',
        user_pk_hex: 'user1',
        provider_nft_id: 'pnft1',
        fee_amount_nanoerg: '800000',
      },
    ]);

    const result = await getSettleableBoxes(client, 3);
    expect(result).toHaveLength(3);
    expect(result[1].feeAmountNanoerg).toBe(1200000n);
    expect(result[2].providerNftId).toBe('pnft1');
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Failed to scan settleable boxes', 500),
    );

    await expect(getSettleableBoxes(client)).rejects.toThrow('Failed to scan settleable boxes');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 9. getOracleRate & getOraclePoolStatus
// ═══════════════════════════════════════════════════════════════════════

describe('getOracleRate', () => {
  it('should GET from /v1/contracts/oracle/rate and return simplified result', async () => {
    mockGet.mockResolvedValue({
      rate: '350000000',
      epoch: 421,
      box_id: 'oraclebox1',
      erg_usd: 0.35,
    });

    const result = await getOracleRate(client);

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/oracle/rate');
    expect(result.rate).toBe(0.35);
    expect(result.epoch).toBe(421);
    expect(result.fetchedAt).toBeInstanceOf(Date);
  });

  it('should handle oracle service unavailable', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('service_unavailable', 'Oracle pool not available', 503),
    );

    await expect(getOracleRate(client)).rejects.toThrow('Oracle pool not available');
  });
});

describe('getOraclePoolStatus', () => {
  it('should GET from /v1/contracts/oracle/status with full details', async () => {
    mockGet.mockResolvedValue({
      epoch: 421,
      erg_usd: 0.35,
      rate: '350000000',
      pool_box_id: 'oraclebox1',
      last_update_height: 800500,
    });

    const result = await getOraclePoolStatus(client);

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/oracle/status');
    expect(result.epoch).toBe(421);
    expect(result.ergUsd).toBe(0.35);
    expect(result.rate).toBe(350000000n);
    expect(result.poolBoxId).toBe('oraclebox1');
    expect(result.lastUpdateHeight).toBe(800500);
  });

  it('should handle large epoch numbers', async () => {
    mockGet.mockResolvedValue({
      epoch: 999999,
      erg_usd: 1.5,
      rate: '1500000000',
      pool_box_id: 'obox',
      last_update_height: 2000000,
    });

    const result = await getOraclePoolStatus(client);
    expect(result.epoch).toBe(999999);
    expect(result.rate).toBe(1500000000n);
  });

  it('should propagate not_found error when oracle pool has no rate', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('not_found', 'Oracle pool box has no rate in R4', 404),
    );

    await expect(getOraclePoolStatus(client)).rejects.toThrow('no rate in R4');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 10. createGovernanceProposal
// ═══════════════════════════════════════════════════════════════════════

describe('createGovernanceProposal', () => {
  const successResponse = {
    txId: 'gtx_001',
    proposalBoxId: 'gbox_001',
    proposalId: 'prop_001',
  };

  const validParams = {
    title: 'Reduce min stake to 1 ERG',
    description: 'Lower the minimum staking requirement from 5 ERG to 1 ERG to encourage participation.',
    proposalType: 'parameter_change',
    proposalData: JSON.stringify({ param: 'min_stake', value: '1000000000' }),
    proposerPkHex: 'proposer1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
    votingDurationBlocks: 7200,
  };

  it('should POST to /v1/contracts/governance/proposal with snake_case params', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await createGovernanceProposal(client, validParams);

    expect(mockPost).toHaveBeenCalledWith('/v1/contracts/governance/proposal', {
      title: 'Reduce min stake to 1 ERG',
      description: 'Lower the minimum staking requirement from 5 ERG to 1 ERG to encourage participation.',
      proposal_type: 'parameter_change',
      proposal_data: JSON.stringify({ param: 'min_stake', value: '1000000000' }),
      proposer_pk_hex: 'proposer1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
      voting_duration_blocks: 7200,
    });
    expect(result.txId).toBe('gtx_001');
    expect(result.proposalBoxId).toBe('gbox_001');
    expect(result.proposalId).toBe('prop_001');
  });

  it('should handle different proposal types', async () => {
    mockPost.mockResolvedValue(successResponse);

    await createGovernanceProposal(client, {
      ...validParams,
      proposalType: 'contract_upgrade',
      proposalData: JSON.stringify({ contract: 'user_staking', version: 2 }),
    });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/proposal',
      expect.objectContaining({
        proposal_type: 'contract_upgrade',
        proposal_data: JSON.stringify({ contract: 'user_staking', version: 2 }),
      }),
    );
  });

  it('should handle fund_release proposal type', async () => {
    mockPost.mockResolvedValue(successResponse);

    await createGovernanceProposal(client, {
      ...validParams,
      proposalType: 'fund_release',
      proposalData: JSON.stringify({ amount: '1000000000000', recipient: '9eZ24...' }),
    });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/proposal',
      expect.objectContaining({ proposal_type: 'fund_release' }),
    );
  });

  it('should propagate validation error for missing fields', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'title is required', 400),
    );

    await expect(createGovernanceProposal(client, validParams)).rejects.toThrow('title is required');
  });

  it('should propagate unauthorized error', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('unauthorized', 'Proposer not authorized', 401),
    );

    await expect(createGovernanceProposal(client, validParams)).rejects.toThrow(
      'Proposer not authorized',
    );
  });

  it('should propagate internal error from agent', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('internal_error', 'Failed to broadcast proposal tx', 500),
    );

    await expect(createGovernanceProposal(client, validParams)).rejects.toThrow(
      'Failed to broadcast proposal tx',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 11. voteOnProposal
// ═══════════════════════════════════════════════════════════════════════

describe('voteOnProposal', () => {
  const successResponse = {
    txId: 'vtx_001',
    proposalId: 'prop_001',
    voterPkHex: 'voter1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
  };

  const validParams = {
    proposalId: 'prop_001',
    voterPkHex: 'voter1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
    vote: 'for' as const,
    stakeNanoerg: 5000000000n,
  };

  it('should POST to /v1/contracts/governance/vote with snake_case params', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await voteOnProposal(client, validParams);

    expect(mockPost).toHaveBeenCalledWith('/v1/contracts/governance/vote', {
      proposal_id: 'prop_001',
      voter_pk_hex: 'voter1234abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd5678ef9012abcd',
      vote: 'for',
      stake_nanoerg: '5000000000',
    });
    expect(result.txId).toBe('vtx_001');
    expect(result.proposalId).toBe('prop_001');
    expect(result.voterPkHex).toBe(validParams.voterPkHex);
  });

  it('should handle "against" vote', async () => {
    mockPost.mockResolvedValue(successResponse);

    await voteOnProposal(client, { ...validParams, vote: 'against' });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/vote',
      expect.objectContaining({ vote: 'against' }),
    );
  });

  it('should handle "abstain" vote', async () => {
    mockPost.mockResolvedValue(successResponse);

    await voteOnProposal(client, { ...validParams, vote: 'abstain' });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/vote',
      expect.objectContaining({ vote: 'abstain' }),
    );
  });

  it('should convert bigint stake to string', async () => {
    mockPost.mockResolvedValue(successResponse);

    await voteOnProposal(client, { ...validParams, stakeNanoerg: 100000000000n });

    expect(mockPost).toHaveBeenCalledWith(
      '/v1/contracts/governance/vote',
      expect.objectContaining({ stake_nanoerg: '100000000000' }),
    );
  });

  it('should propagate error for inactive proposal', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Proposal prop_001 is no longer active', 400),
    );

    await expect(voteOnProposal(client, validParams)).rejects.toThrow('no longer active');
  });

  it('should propagate error for duplicate vote', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Voter has already voted on proposal prop_001', 409),
    );

    await expect(voteOnProposal(client, validParams)).rejects.toThrow('already voted');
  });

  it('should propagate error for insufficient stake', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Insufficient staked ERG for vote weight', 400),
    );

    await expect(voteOnProposal(client, validParams)).rejects.toThrow('Insufficient staked ERG');
  });

  it('should propagate unauthorized error', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('unauthorized', 'Invalid voter public key', 401),
    );

    await expect(voteOnProposal(client, validParams)).rejects.toThrow('Invalid voter public key');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 12. getGovernanceProposals
// ═══════════════════════════════════════════════════════════════════════

describe('getGovernanceProposals', () => {
  const activeProposal = {
    proposal_id: 'prop_001',
    proposal_box_id: 'gbox_001',
    title: 'Reduce min stake to 1 ERG',
    description: 'Lower the minimum staking requirement.',
    proposal_type: 'parameter_change',
    proposer_pk_hex: 'proposerpk',
    creation_height: 800000,
    end_height: 807200,
    votes_for: '50000000000',
    votes_against: '10000000000',
    votes_abstain: '5000000000',
    active: true,
    executed: false,
  };

  const expiredProposal = {
    proposal_id: 'prop_002',
    proposal_box_id: 'gbox_002',
    title: 'Increase oracle update frequency',
    description: 'Update oracle every 4 blocks instead of 6.',
    proposal_type: 'parameter_change',
    proposer_pk_hex: 'proposer2pk',
    creation_height: 700000,
    end_height: 707200,
    votes_for: '80000000000',
    votes_against: '5000000000',
    votes_abstain: '2000000000',
    active: false,
    executed: true,
  };

  it('should GET from /v1/contracts/governance/proposals and map all fields', async () => {
    mockGet.mockResolvedValue([activeProposal]);

    const result = await getGovernanceProposals(client);

    expect(mockGet).toHaveBeenCalledWith('/v1/contracts/governance/proposals');
    expect(result).toHaveLength(1);
    expect(result[0].proposalId).toBe('prop_001');
    expect(result[0].proposalBoxId).toBe('gbox_001');
    expect(result[0].title).toBe('Reduce min stake to 1 ERG');
    expect(result[0].description).toBe('Lower the minimum staking requirement.');
    expect(result[0].proposalType).toBe('parameter_change');
    expect(result[0].proposerPkHex).toBe('proposerpk');
    expect(result[0].creationHeight).toBe(800000);
    expect(result[0].endHeight).toBe(807200);
    expect(result[0].votesFor).toBe(50000000000n);
    expect(result[0].votesAgainst).toBe(10000000000n);
    expect(result[0].votesAbstain).toBe(5000000000n);
    expect(result[0].active).toBe(true);
    expect(result[0].executed).toBe(false);
  });

  it('should handle multiple proposals with different statuses', async () => {
    mockGet.mockResolvedValue([activeProposal, expiredProposal]);

    const result = await getGovernanceProposals(client);

    expect(result).toHaveLength(2);
    expect(result[0].active).toBe(true);
    expect(result[0].executed).toBe(false);
    expect(result[1].active).toBe(false);
    expect(result[1].executed).toBe(true);
    expect(result[1].votesFor).toBe(80000000000n);
  });

  it('should return empty array when no proposals exist', async () => {
    mockGet.mockResolvedValue([]);

    const result = await getGovernanceProposals(client);
    expect(result).toHaveLength(0);
  });

  it('should handle proposals with zero votes', async () => {
    mockGet.mockResolvedValue([
      {
        ...activeProposal,
        votes_for: '0',
        votes_against: '0',
        votes_abstain: '0',
      },
    ]);

    const result = await getGovernanceProposals(client);
    expect(result[0].votesFor).toBe(0n);
    expect(result[0].votesAgainst).toBe(0n);
    expect(result[0].votesAbstain).toBe(0n);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Failed to scan governance proposals', 500),
    );

    await expect(getGovernanceProposals(client)).rejects.toThrow('Failed to scan governance proposals');
  });
});
