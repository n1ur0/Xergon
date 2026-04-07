/**
 * Tests for the bridge module -- cross-chain payment bridge API.
 *
 * Covers all 6 exported functions:
 *   1. getBridgeStatus
 *   2. getBridgeInvoices
 *   3. getBridgeInvoice
 *   4. createBridgeInvoice
 *   5. confirmBridgePayment
 *   6. refundBridgeInvoice
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
  getBridgeStatus,
  getBridgeInvoices,
  getBridgeInvoice,
  createBridgeInvoice,
  confirmBridgePayment,
  refundBridgeInvoice,
} from '../src/bridge';

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
// 1. getBridgeStatus
// ═══════════════════════════════════════════════════════════════════════

describe('getBridgeStatus', () => {
  const successResponse = {
    status: 'operational',
    supportedChains: ['btc', 'eth', 'ada'],
  };

  it('should GET /v1/bridge/status', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getBridgeStatus(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/bridge/status');
    expect(result).toEqual(successResponse);
  });

  it('should return status and supportedChains', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getBridgeStatus(client);

    expect(result.status).toBe('operational');
    expect(result.supportedChains).toEqual(['btc', 'eth', 'ada']);
  });

  it('should handle empty supportedChains array', async () => {
    mockGet.mockResolvedValue({ status: 'operational', supportedChains: [] });

    const result = await getBridgeStatus(client);

    expect(result.supportedChains).toEqual([]);
  });

  it('should propagate errors', async () => {
    mockGet.mockRejectedValue(makeXergonError('internal_error', 'Bridge service down', 500));

    await expect(getBridgeStatus(client)).rejects.toThrow('Bridge service down');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. getBridgeInvoices
// ═══════════════════════════════════════════════════════════════════════

describe('getBridgeInvoices', () => {
  const successResponse = [
    {
      invoiceId: 'inv_001',
      amountNanoerg: '1000000000',
      chain: 'btc' as const,
      status: 'pending' as const,
      createdAt: 1700000000,
      refundTimeout: 1700086400,
    },
    {
      invoiceId: 'inv_002',
      amountNanoerg: '500000000',
      chain: 'eth' as const,
      status: 'confirmed' as const,
      createdAt: 1699900000,
      refundTimeout: 1699986400,
    },
  ];

  it('should GET /v1/bridge/invoices', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getBridgeInvoices(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/bridge/invoices');
    expect(result).toEqual(successResponse);
  });

  it('should return an array of invoices', async () => {
    mockGet.mockResolvedValue(successResponse);

    const result = await getBridgeInvoices(client);

    expect(result).toHaveLength(2);
    expect(result[0].invoiceId).toBe('inv_001');
    expect(result[0].chain).toBe('btc');
    expect(result[1].status).toBe('confirmed');
  });

  it('should handle empty invoices list', async () => {
    mockGet.mockResolvedValue([]);

    const result = await getBridgeInvoices(client);

    expect(result).toEqual([]);
  });

  it('should propagate unauthorized errors', async () => {
    mockGet.mockRejectedValue(makeXergonError('unauthorized', 'Invalid HMAC', 401));

    await expect(getBridgeInvoices(client)).rejects.toThrow('Invalid HMAC');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. getBridgeInvoice
// ═══════════════════════════════════════════════════════════════════════

describe('getBridgeInvoice', () => {
  const invoiceResponse = {
    invoiceId: 'inv_001',
    amountNanoerg: '1000000000',
    chain: 'eth' as const,
    status: 'pending' as const,
    createdAt: 1700000000,
    refundTimeout: 1700086400,
  };

  it('should GET /v1/bridge/invoice/:id', async () => {
    mockGet.mockResolvedValue(invoiceResponse);

    const result = await getBridgeInvoice(client, 'inv_001');

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/v1/bridge/invoice/inv_001');
    expect(result).toEqual(invoiceResponse);
  });

  it('should URL-encode the invoice ID', async () => {
    mockGet.mockResolvedValue(invoiceResponse);

    await getBridgeInvoice(client, 'inv/with slashes');

    expect(mockGet).toHaveBeenCalledWith('/v1/bridge/invoice/inv%2Fwith%20slashes');
  });

  it('should return invoice fields correctly', async () => {
    mockGet.mockResolvedValue(invoiceResponse);

    const result = await getBridgeInvoice(client, 'inv_001');

    expect(result.invoiceId).toBe('inv_001');
    expect(result.amountNanoerg).toBe('1000000000');
    expect(result.chain).toBe('eth');
    expect(result.status).toBe('pending');
    expect(result.createdAt).toBe(1700000000);
  });

  it('should propagate not_found errors', async () => {
    mockGet.mockRejectedValue(makeXergonError('not_found', 'Invoice not found', 404));

    await expect(getBridgeInvoice(client, 'nonexistent')).rejects.toThrow('Invoice not found');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. createBridgeInvoice
// ═══════════════════════════════════════════════════════════════════════

describe('createBridgeInvoice', () => {
  const successResponse = {
    invoiceId: 'inv_new',
    amountNanoerg: '2000000000',
    chain: 'btc' as const,
    status: 'pending' as const,
    createdAt: 1700000000,
    refundTimeout: 1700086400,
  };

  it('should POST to /v1/bridge/create-invoice with snake_case body', async () => {
    mockPost.mockResolvedValue(successResponse);

    const result = await createBridgeInvoice(client, '2000000000', 'btc');

    expect(mockPost).toHaveBeenCalledTimes(1);
    expect(mockPost).toHaveBeenCalledWith('/v1/bridge/create-invoice', {
      amount_nanoerg: '2000000000',
      chain: 'btc',
    });
    expect(result).toEqual(successResponse);
  });

  it('should work with eth chain', async () => {
    mockPost.mockResolvedValue({ ...successResponse, chain: 'eth' });

    const result = await createBridgeInvoice(client, '500000000', 'eth');

    expect(mockPost).toHaveBeenCalledWith('/v1/bridge/create-invoice', {
      amount_nanoerg: '500000000',
      chain: 'eth',
    });
    expect(result.chain).toBe('eth');
  });

  it('should work with ada chain', async () => {
    mockPost.mockResolvedValue({ ...successResponse, chain: 'ada' });

    await createBridgeInvoice(client, '300000000', 'ada');

    expect(mockPost).toHaveBeenCalledWith('/v1/bridge/create-invoice', {
      amount_nanoerg: '300000000',
      chain: 'ada',
    });
  });

  it('should propagate invalid_request errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Invalid amount_nanoerg', 400),
    );

    await expect(createBridgeInvoice(client, '-1', 'btc')).rejects.toThrow(
      'Invalid amount_nanoerg',
    );
  });

  it('should propagate internal server errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('internal_error', 'Bridge node unavailable', 500),
    );

    await expect(createBridgeInvoice(client, '100', 'eth')).rejects.toThrow(
      'Bridge node unavailable',
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 5. confirmBridgePayment
// ═══════════════════════════════════════════════════════════════════════

describe('confirmBridgePayment', () => {
  it('should POST to /v1/bridge/confirm with snake_case body', async () => {
    mockPost.mockResolvedValue(undefined);

    await confirmBridgePayment(client, 'inv_001', 'tx_hash_abc');

    expect(mockPost).toHaveBeenCalledTimes(1);
    expect(mockPost).toHaveBeenCalledWith('/v1/bridge/confirm', {
      invoice_id: 'inv_001',
      tx_hash: 'tx_hash_abc',
    });
  });

  it('should return void on success', async () => {
    mockPost.mockResolvedValue(undefined);

    const result = await confirmBridgePayment(client, 'inv_001', 'tx_hash_abc');

    expect(result).toBeUndefined();
  });

  it('should propagate not_found errors for invalid invoice', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('not_found', 'Invoice not found', 404),
    );

    await expect(
      confirmBridgePayment(client, 'bad_inv', 'tx_hash'),
    ).rejects.toThrow('Invoice not found');
  });

  it('should propagate unauthorized errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('unauthorized', 'Invalid HMAC signature', 401),
    );

    await expect(
      confirmBridgePayment(client, 'inv_001', 'tx_hash'),
    ).rejects.toThrow('Invalid HMAC signature');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 6. refundBridgeInvoice
// ═══════════════════════════════════════════════════════════════════════

describe('refundBridgeInvoice', () => {
  it('should POST to /v1/bridge/refund with snake_case body', async () => {
    mockPost.mockResolvedValue(undefined);

    await refundBridgeInvoice(client, 'inv_001');

    expect(mockPost).toHaveBeenCalledTimes(1);
    expect(mockPost).toHaveBeenCalledWith('/v1/bridge/refund', {
      invoice_id: 'inv_001',
    });
  });

  it('should return void on success', async () => {
    mockPost.mockResolvedValue(undefined);

    const result = await refundBridgeInvoice(client, 'inv_001');

    expect(result).toBeUndefined();
  });

  it('should propagate errors when invoice cannot be refunded', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('invalid_request', 'Invoice already confirmed', 400),
    );

    await expect(refundBridgeInvoice(client, 'inv_confirmed')).rejects.toThrow(
      'Invoice already confirmed',
    );
  });

  it('should propagate not_found errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('not_found', 'Invoice not found', 404),
    );

    await expect(refundBridgeInvoice(client, 'nonexistent')).rejects.toThrow(
      'Invoice not found',
    );
  });

  it('should propagate internal server errors', async () => {
    mockPost.mockRejectedValue(
      makeXergonError('internal_error', 'Refund processing failed', 500),
    );

    await expect(refundBridgeInvoice(client, 'inv_001')).rejects.toThrow(
      'Refund processing failed',
    );
  });
});
