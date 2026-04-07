/**
 * Tests for the health module -- liveness and readiness probes.
 *
 * Covers all 2 exported functions:
 *   1. healthCheck (liveness -- /health)
 *   2. readyCheck (readiness -- /ready)
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Mock Setup ──────────────────────────────────────────────────────────

const mockGet = vi.fn();
const mockPost = vi.fn();

vi.mock('../src/client', () => ({
  XergonClientCore: vi.fn().mockImplementation(() => ({
    get: mockGet,
    post: mockPost,
  })),
}));

import { healthCheck, readyCheck } from '../src/health';

let client: any;

beforeEach(() => {
  vi.clearAllMocks();
  client = { get: mockGet, post: mockPost };
});

// ═══════════════════════════════════════════════════════════════════════
// 1. healthCheck
// ═══════════════════════════════════════════════════════════════════════

describe('healthCheck', () => {
  it('should GET /health with skipAuth: true', async () => {
    mockGet.mockResolvedValue('OK');

    await healthCheck(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/health', { skipAuth: true });
  });

  it('should return true when response is "OK"', async () => {
    mockGet.mockResolvedValue('OK');

    const result = await healthCheck(client);

    expect(result).toBe(true);
  });

  it('should return true when response is "ok" (case-insensitive)', async () => {
    mockGet.mockResolvedValue('ok');

    const result = await healthCheck(client);

    expect(result).toBe(true);
  });

  it('should return true when response has whitespace', async () => {
    mockGet.mockResolvedValue('  OK  ');

    const result = await healthCheck(client);

    expect(result).toBe(true);
  });

  it('should return false when response is not "OK"', async () => {
    mockGet.mockResolvedValue('DEGRADED');

    const result = await healthCheck(client);

    expect(result).toBe(false);
  });

  it('should return false when the request throws an error', async () => {
    mockGet.mockRejectedValue(new Error('Network error'));

    const result = await healthCheck(client);

    expect(result).toBe(false);
  });

  it('should return false when the server returns 500', async () => {
    mockGet.mockRejectedValue(new Error('Internal Server Error'));

    const result = await healthCheck(client);

    expect(result).toBe(false);
  });

  it('should return false when the server is unreachable', async () => {
    mockGet.mockRejectedValue(new TypeError('fetch failed'));

    const result = await healthCheck(client);

    expect(result).toBe(false);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. readyCheck
// ═══════════════════════════════════════════════════════════════════════

describe('readyCheck', () => {
  it('should GET /ready with skipAuth: true', async () => {
    mockGet.mockResolvedValue('OK');

    await readyCheck(client);

    expect(mockGet).toHaveBeenCalledTimes(1);
    expect(mockGet).toHaveBeenCalledWith('/ready', { skipAuth: true });
  });

  it('should return true when response is "OK"', async () => {
    mockGet.mockResolvedValue('OK');

    const result = await readyCheck(client);

    expect(result).toBe(true);
  });

  it('should return true when response is "ok" (case-insensitive)', async () => {
    mockGet.mockResolvedValue('ok');

    const result = await readyCheck(client);

    expect(result).toBe(true);
  });

  it('should return true when response has whitespace', async () => {
    mockGet.mockResolvedValue('\tOK\n');

    const result = await readyCheck(client);

    expect(result).toBe(true);
  });

  it('should return false when response is not "OK"', async () => {
    mockGet.mockResolvedValue('NOT_READY');

    const result = await readyCheck(client);

    expect(result).toBe(false);
  });

  it('should return false when the request throws an error', async () => {
    mockGet.mockRejectedValue(new Error('Network error'));

    const result = await readyCheck(client);

    expect(result).toBe(false);
  });

  it('should return false when the server returns 503', async () => {
    mockGet.mockRejectedValue(new Error('Service Unavailable'));

    const result = await readyCheck(client);

    expect(result).toBe(false);
  });

  it('should return false when the server is unreachable', async () => {
    mockGet.mockRejectedValue(new TypeError('fetch failed'));

    const result = await readyCheck(client);

    expect(result).toBe(false);
  });
});
