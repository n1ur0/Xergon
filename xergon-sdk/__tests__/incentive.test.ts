/**
 * Tests for the incentive module -- rare model bonuses / rarity scoring API.
 *
 * Covers all 3 exported functions:
 *   1. getIncentiveStatus
 *   2. getIncentiveModels
 *   3. getIncentiveModelDetail
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

import {
  getIncentiveStatus,
  getIncentiveModels,
  getIncentiveModelDetail,
} from '../src/incentive';

let client: any;

beforeEach(() => {
  vi.clearAllMocks();
  client = { get: mockGet, post: mockPost };
});

// ── Helper ──────────────────────────────────────────────────────────────

function makeXergonError(type: string, message: string, code: number): XergonError {
  return new XergonError({ type: type as any, message, code });
}

// ═══════════════════════════════════════════════════════════════════════
// 1. getIncentiveStatus
// ═══════════════════════════════════════════════════════════════════════

describe('getIncentiveStatus', () => {
  const successResponse = {
    active: true,
    totalBonusErg: '150.5',
    rareModelsCount: 7,
  };

  it('should GET /v1/incentive/status', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getIncentiveStatus(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/incentive/status');
    expect(result).toEqual(successResponse);
  });

  it('should return active status, bonus amount, and model count', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getIncentiveStatus(client);

    expect(result.active).toBe(true);
    expect(result.totalBonusErg).toBe('150.5');
    expect(result.rareModelsCount).toBe(7);
  });

  it('should handle inactive status', async () => {
    mockGet.mockResolvedValue({
      active: false,
      totalBonusErg: '0',
      rareModelsCount: 0,
    });

    const result = await getIncentiveStatus(client);

    expect(result.active).toBe(false);
    expect(result.rareModelsCount).toBe(0);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Incentive service unavailable', 500),
    );

    await expect(getIncentiveStatus(client)).rejects.toThrow(
      'Incentive service unavailable',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. getIncentiveModels
// ═══════════════════════════════════════════════════════════════════════

describe('getIncentiveModels', () => {
  const successResponse = [
    {
      model: 'llama-3.3-70b',
      rarityScore: 95,
      bonusMultiplier: 2.5,
      providersCount: 3,
    },
    {
      model: 'deepseek-coder-33b',
      rarityScore: 82,
      bonusMultiplier: 1.8,
      providersCount: 5,
    },
    {
      model: 'mistral-small-24b',
      rarityScore: 60,
      bonusMultiplier: 1.2,
      providersCount: 12,
    },
  ];

  it('should GET /v1/incentive/models', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getIncentiveModels(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/incentive/models');
    expect(result).toEqual(successResponse);
  });

  it('should return array of rare models with correct fields', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getIncentiveModels(client);

    expect(result).toHaveLength(3);
    expect(result[0].model).toBe('llama-3.3-70b');
    expect(result[0].rarityScore).toBe(95);
    expect(result[0].bonusMultiplier).toBe(2.5);
    expect(result[0].providersCount).toBe(3);
    expect(result[1].rarityScore).toBe(82);
    expect(result[2].model).toBe('mistral-small-24b');
  });

  it('should handle empty models list', async () => {
    mockGet.mockResolvedValue([]);

    const result = await getIncentiveModels(client);

    expect(result).toEqual([]);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('service_unavailable', 'Service temporarily unavailable', 503),
    );

    await expect(getIncentiveModels(client)).rejects.toThrow(
      'Service temporarily unavailable',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. getIncentiveModelDetail
// ═══════════════════════════════════════════════════════════════════════

describe('getIncentiveModelDetail', () => {
  const successResponse = {
    model: 'llama-3.3-70b',
    rarityScore: 95,
    bonusMultiplier: 2.5,
    providersCount: 3,
    recentRequests: 1420,
    bonusErgAccumulated: '45.75',
  };

  it('should GET /v1/incentive/models/:model', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getIncentiveModelDetail(client, 'llama-3.3-70b');

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/incentive/models/llama-3.3-70b');
    expect(result).toEqual(successResponse);
  });

  it('should return detailed rarity fields', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getIncentiveModelDetail(client, 'llama-3.3-70b');

    expect(result.model).toBe('llama-3.3-70b');
    expect(result.rarityScore).toBe(95);
    expect(result.bonusMultiplier).toBe(2.5);
    expect(result.recentRequests).toBe(1420);
    expect(result.bonusErgAccumulated).toBe('45.75');
  });

  it('should handle optional fields being absent', async () => {
    mockGet.mockResolvedValue({
      model: 'some-model',
      rarityScore: 50,
      bonusMultiplier: 1.0,
      providersCount: 10,
    });

    const result = await getIncentiveModelDetail(client, 'some-model');

    expect(result.recentRequests).toBeUndefined();
    expect(result.bonusErgAccumulated).toBeUndefined();
  });

  it('should URL-encode model names with special characters', async () => {
    mockGet.mockResolvedValue(successResponse);

    await getIncentiveModelDetail(client, 'model/special name');

    expect(mockGet).toHaveBeenCalledWith('/v1/incentive/models/model%2Fspecial%20name');
  });

  it('should propagate not_found errors for unknown models', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('not_found', 'Model not found in incentive program', 404),
    );

    await expect(
      getIncentiveModelDetail(client, 'nonexistent-model'),
    ).rejects.toThrow('Model not found in incentive program');
  });

  it('should propagate internal server errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Database query failed', 500),
    );

    await expect(getIncentiveModelDetail(client, 'llama-3.3-70b')).rejects.toThrow(
      'Database query failed',
    );
  });
});
