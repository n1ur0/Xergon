/**
 * Tests for the GPU Bazar module -- listings, rent, pricing, ratings, reputation.
 *
 * Covers all 7 exported functions:
 *   1. listGpuListings
 *   2. getGpuListing
 *   3. rentGpu
 *   4. getMyRentals
 *   5. getGpuPricing
 *   6. rateGpu
 *   7. getGpuReputation
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
  listGpuListings,
  getGpuListing,
  rentGpu,
  getMyRentals,
  getGpuPricing,
  rateGpu,
  getGpuReputation,
} from '../src/gpu';

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
// 1. listGpuListings
// ═══════════════════════════════════════════════════════════════════════

describe('listGpuListings', () => {
  const successResponse = [
    {
      listingId: 'gpu_001',
      providerPk: '0xabc123',
      gpuType: 'A100',
      vramGb: 80,
      pricePerHourNanoerg: '50000000',
      region: 'US',
      available: true,
      bandwidthMbps: 1000,
    },
    {
      listingId: 'gpu_002',
      providerPk: '0xdef456',
      gpuType: 'H100',
      vramGb: 80,
      pricePerHourNanoerg: '100000000',
      region: 'EU',
      available: false,
    },
  ];

  it('should GET /v1/gpu/listings without filters', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await listGpuListings(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings');
    expect(result).toEqual(successResponse);
  });

  it('should apply gpu_type filter', async () => {
    mockGet.mockResolvedValue(successResponse);

    await listGpuListings(client, { gpuType: 'A100' });

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings?gpu_type=A100');
  });

  it('should apply min_vram filter', async () => {
    mockGet.mockResolvedValue(successResponse);

    await listGpuListings(client, { minVram: 40 });

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings?min_vram=40');
  });

  it('should apply max_price filter', async () => {
    mockGet.mockResolvedValue(successResponse);

    await listGpuListings(client, { maxPrice: 200 });

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings?max_price=200');
  });

  it('should apply region filter', async () => {
    mockGet.mockResolvedValue(successResponse);

    await listGpuListings(client, { region: 'US' });

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings?region=US');
  });

  it('should combine multiple filters', async () => {
    mockGet.mockResolvedValue(successResponse);

    await listGpuListings(client, {
      gpuType: 'H100',
      minVram: 80,
      maxPrice: 150,
      region: 'EU',
    });

    const calledUrl = mockGet.mock.calls[0][0];
    expect(calledUrl).toContain('gpu_type=H100');
    expect(calledUrl).toContain('min_vram=80');
    expect(calledUrl).toContain('max_price=150');
    expect(calledUrl).toContain('region=EU');
  });

  it('should skip null/undefined filters', async () => {
    mockGet.mockResolvedValue(successResponse);

    await listGpuListings(client, { gpuType: 'A100', minVram: undefined, maxPrice: null as any });

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings?gpu_type=A100');
  });

  it('should return array of listings with correct types', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await listGpuListings(client);

    expect(result).toHaveLength(2);
    expect(result[0].listingId).toBe('gpu_001');
    expect(result[0].available).toBe(true);
    expect(result[1].available).toBe(false);
  });

  it('should handle empty listings', async () => {
    mockGet.mockResolvedValue([]);

    const result = await listGpuListings(client);

    expect(result).toEqual([]);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'GPU service down', 500),
    );

    await expect(listGpuListings(client)).rejects.toThrow('GPU service down');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. getGpuListing
// ═══════════════════════════════════════════════════════════════════════

describe('getGpuListing', () => {
  const successResponse = {
    listingId: 'gpu_001',
    providerPk: '0xabc123',
    gpuType: 'A100',
    vramGb: 80,
    pricePerHourNanoerg: '50000000',
    region: 'US',
    available: true,
    bandwidthMbps: 1000,
  };

  it('should GET /v1/gpu/listings/:id', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getGpuListing(client, 'gpu_001');

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings/gpu_001');
    expect(result).toEqual(successResponse);
  });

  it('should URL-encode the listing ID', async () => {
    mockGet.mockResolvedValue(successResponse);

    await getGpuListing(client, 'id with spaces');

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/listings/id%20with%20spaces');
  });

  it('should return listing fields correctly', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getGpuListing(client, 'gpu_001');

    expect(result.listingId).toBe('gpu_001');
    expect(result.gpuType).toBe('A100');
    expect(result.vramGb).toBe(80);
    expect(result.available).toBe(true);
  });

  it('should handle optional fields being absent', async () => {
    mockGet.mockResolvedValue({
      listingId: 'gpu_003',
      providerPk: '0xabc',
      gpuType: 'RTX4090',
      pricePerHourNanoerg: '30000000',
      region: 'US',
      available: true,
    });

    const result = await getGpuListing(client, 'gpu_003');

    expect(result.vramGb).toBeUndefined();
    expect(result.bandwidthMbps).toBeUndefined();
  });

  it('should propagate not_found errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('not_found', 'Listing not found', 404),
    );

    await expect(getGpuListing(client, 'nonexistent')).rejects.toThrow(
      'Listing not found',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. rentGpu
// ═══════════════════════════════════════════════════════════════════════

describe('rentGpu', () => {
  const successResponse = {
    rentalId: 'rental_001',
    listingId: 'gpu_001',
    providerPk: '0xabc123',
    renterPk: '0xrenter',
    hours: 4,
    costNanoerg: '200000000',
    startedAt: 1700000000,
    expiresAt: 1700014400,
    status: 'active' as const,
  };

  it('should POST to /v1/gpu/rent with snake_case body', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await rentGpu(client, 'gpu_001', 4);

    expect(mockPost).toHaveBeenCalledTimes(1);
    expect(mockPost).toHaveBeenCalledWith('/v1/gpu/rent', {
      listing_id: 'gpu_001',
      hours: 4,
    });
    expect(result).toEqual(successResponse);
  });

  it('should return rental with correct fields', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await rentGpu(client, 'gpu_001', 4);

    expect(result.rentalId).toBe('rental_001');
    expect(result.hours).toBe(4);
    expect(result.costNanoerg).toBe('200000000');
    expect(result.status).toBe('active');
  });

  it('should propagate invalid_request errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Listing not available', 400),
    );

    await expect(rentGpu(client, 'gpu_002', 1)).rejects.toThrow(
      'Listing not available',
    );
  });

  it('should propagate unauthorized errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('unauthorized', 'Authentication required', 401),
    );

    await expect(rentGpu(client, 'gpu_001', 2)).rejects.toThrow(
      'Authentication required',
    );
  });

  it('should propagate internal server errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('internal_error', 'Rental creation failed', 500),
    );

    await expect(rentGpu(client, 'gpu_001', 1)).rejects.toThrow(
      'Rental creation failed',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. getMyRentals
// ═══════════════════════════════════════════════════════════════════════

describe('getMyRentals', () => {
  const successResponse = [
    {
      rentalId: 'rental_001',
      listingId: 'gpu_001',
      providerPk: '0xabc123',
      renterPk: '0xrenter',
      hours: 4,
      costNanoerg: '200000000',
      startedAt: 1700000000,
      expiresAt: 1700014400,
      status: 'active' as const,
    },
  ];

  it('should GET /v1/gpu/rentals/:renterPk', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getMyRentals(client, '0xrenter');

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/rentals/0xrenter');
    expect(result).toEqual(successResponse);
  });

  it('should URL-encode the public key', async () => {
    mockGet.mockResolvedValue(successResponse);

    await getMyRentals(client, '0x/key with spaces');

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/rentals/0x%2Fkey%20with%20spaces');
  });

  it('should handle empty rentals list', async () => {
    mockGet.mockResolvedValue([]);

    const result = await getMyRentals(client, '0xnew');

    expect(result).toEqual([]);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Failed to fetch rentals', 500),
    );

    await expect(getMyRentals(client, '0xrenter')).rejects.toThrow(
      'Failed to fetch rentals',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 5. getGpuPricing
// ═══════════════════════════════════════════════════════════════════════

describe('getGpuPricing', () => {
  const rawResponse = {
    avg_price_per_hour: '75000000',
    models: {
      A100: '50000000',
      H100: '100000000',
      RTX4090: '30000000',
    },
  };

  it('should GET /v1/gpu/pricing and transform response', async () => {
    mockGet.mockResolvedValue(rawResponse);

    const result = await getGpuPricing(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/pricing');
  });

  it('should convert raw models object into GpuPricingEntry array', async () => {
    mockGet.mockResolvedValue(rawResponse);

    const result = await getGpuPricing(client);

    expect(result).toHaveLength(3);
    expect(result).toEqual([
      { gpuType: 'A100', avgPricePerHourNanoerg: '50000000' },
      { gpuType: 'H100', avgPricePerHourNanoerg: '100000000' },
      { gpuType: 'RTX4090', avgPricePerHourNanoerg: '30000000' },
    ]);
  });

  it('should handle single model', async () => {
    mockGet.mockResolvedValue({
      avg_price_per_hour: '50000000',
      models: { A100: '50000000' },
    });

    const result = await getGpuPricing(client);

    expect(result).toHaveLength(1);
    expect(result[0].gpuType).toBe('A100');
    expect(result[0].avgPricePerHourNanoerg).toBe('50000000');
  });

  it('should handle empty models', async () => {
    mockGet.mockResolvedValue({
      avg_price_per_hour: '0',
      models: {},
    });

    const result = await getGpuPricing(client);

    expect(result).toEqual([]);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Pricing service unavailable', 500),
    );

    await expect(getGpuPricing(client)).rejects.toThrow(
      'Pricing service unavailable',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 6. rateGpu
// ═══════════════════════════════════════════════════════════════════════

describe('rateGpu', () => {
  it('should POST to /v1/gpu/rate with snake_case body', async () => {
    mockPost.mockResolvedValue(undefined);

    await rateGpu(client, {
      targetPk: '0xprovider',
      rentalId: 'rental_001',
      score: 5,
      comment: 'Great GPU!',
    });

    expect(mockPost).toHaveBeenCalledTimes(1);
    expect(mockPost).toHaveBeenCalledWith('/v1/gpu/rate', {
      target_pk: '0xprovider',
      rental_id: 'rental_001',
      score: 5,
      comment: 'Great GPU!',
    });
  });

  it('should default comment to empty string when omitted', async () => {
    mockPost.mockResolvedValue(undefined);

    await rateGpu(client, {
      targetPk: '0xprovider',
      rentalId: 'rental_001',
      score: 4,
    });

    expect(mockPost).toHaveBeenCalledWith('/v1/gpu/rate', {
      target_pk: '0xprovider',
      rental_id: 'rental_001',
      score: 4,
      comment: '',
    });
  });

  it('should return void on success', async () => {
    mockPost.mockResolvedValue(undefined);

    const result = await rateGpu(client, {
      targetPk: '0xprovider',
      rentalId: 'rental_001',
      score: 5,
    });

    expect(result).toBeUndefined();
  });

  it('should propagate invalid_request errors for invalid scores', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Score must be 1-5', 400),
    );

    await expect(
      rateGpu(client, {
        targetPk: '0xprovider',
        rentalId: 'rental_001',
        score: 10,
      }),
    ).rejects.toThrow('Score must be 1-5');
  });

  it('should propagate not_found errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('not_found', 'Rental not found', 404),
    );

    await expect(
      rateGpu(client, {
        targetPk: '0xprovider',
        rentalId: 'nonexistent',
        score: 5,
      }),
    ).rejects.toThrow('Rental not found');
  });

  it('should propagate unauthorized errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('unauthorized', 'Authentication required', 401),
    );

    await expect(
      rateGpu(client, {
        targetPk: '0xprovider',
        rentalId: 'rental_001',
        score: 5,
      }),
    ).rejects.toThrow('Authentication required');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 7. getGpuReputation
// ═══════════════════════════════════════════════════════════════════════

describe('getGpuReputation', () => {
  const successResponse = {
    publicKey: '0xprovider',
    score: 4.7,
    totalRatings: 42,
    average: 4.7,
  };

  it('should GET /v1/gpu/reputation/:publicKey', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getGpuReputation(client, '0xprovider');

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/reputation/0xprovider');
    expect(result).toEqual(successResponse);
  });

  it('should return reputation fields correctly', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getGpuReputation(client, '0xprovider');

    expect(result.publicKey).toBe('0xprovider');
    expect(result.score).toBe(4.7);
    expect(result.totalRatings).toBe(42);
    expect(result.average).toBe(4.7);
  });

  it('should URL-encode the public key', async () => {
    mockGet.mockResolvedValue(successResponse);

    await getGpuReputation(client, '0x/key with spaces');

    expect(mockGet).toHaveBeenCalledWith('/v1/gpu/reputation/0x%2Fkey%20with%20spaces');
  });

  it('should propagate not_found errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('not_found', 'Reputation not found', 404),
    );

    await expect(getGpuReputation(client, '0xunknown')).rejects.toThrow(
      'Reputation not found',
    );
  });

  it('should propagate internal server errors', async () => {
    mockGet.mockRejectedValue(
      makeXergonError('internal_error', 'Reputation service error', 500),
    );

    await expect(getGpuReputation(client, '0xprovider')).rejects.toThrow(
      'Reputation service error',
    );
  });
});
