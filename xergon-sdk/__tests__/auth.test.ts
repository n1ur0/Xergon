/**
 * Tests for the auth module -- HMAC-SHA256 signing, verification, and payload building.
 *
 * Covers all 3 exported functions:
 *   1. hmacSign
 *   2. hmacVerify
 *   3. buildHmacPayload
 */

import { describe, it, expect } from 'vitest';

import { hmacSign, hmacVerify, buildHmacPayload } from '../src/auth';

// ═══════════════════════════════════════════════════════════════════════
// 1. buildHmacPayload
// ═══════════════════════════════════════════════════════════════════════

describe('buildHmacPayload', () => {
  it('should concatenate body and timestamp', () => {
    const result = buildHmacPayload('{"key":"value"}', 1700000000);
    expect(result).toBe('{"key":"value"}1700000000');
  });

  it('should handle empty body', () => {
    const result = buildHmacPayload('', 1700000000);
    expect(result).toBe('1700000000');
  });

  it('should handle body with special characters', () => {
    const result = buildHmacPayload('hello "world"', 0);
    expect(result).toBe('hello "world"0');
  });

  it('should handle large timestamps', () => {
    const result = buildHmacPayload('body', 9999999999);
    expect(result).toBe('body9999999999');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. hmacSign
// ═══════════════════════════════════════════════════════════════════════

describe('hmacSign', () => {
  // 64 hex chars = 32 bytes = 256-bit key (standard HMAC-SHA256 key length)
  const testKey = 'a'.repeat(64);

  it('should return a hex string of 64 characters (32 bytes)', async () => {
    const signature = await hmacSign('test message', testKey);

    expect(typeof signature).toBe('string');
    expect(signature).toHaveLength(64);
    expect(signature).toMatch(/^[0-9a-f]+$/);
  });

  it('should produce consistent signatures for same input', async () => {
    const message = '{"model":"llama-3.3-70b","messages":[...]}1700000000';
    const sig1 = await hmacSign(message, testKey);
    const sig2 = await hmacSign(message, testKey);

    expect(sig1).toBe(sig2);
  });

  it('should produce different signatures for different messages', async () => {
    const sig1 = await hmacSign('message one', testKey);
    const sig2 = await hmacSign('message two', testKey);

    expect(sig1).not.toBe(sig2);
  });

  it('should produce different signatures for different keys', async () => {
    const otherKey = 'b'.repeat(64);
    const sig1 = await hmacSign('message', testKey);
    const sig2 = await hmacSign('message', otherKey);

    expect(sig1).not.toBe(sig2);
  });

  it('should handle empty message', async () => {
    const signature = await hmacSign('', testKey);

    expect(typeof signature).toBe('string');
    expect(signature).toHaveLength(64);
  });

  it('should handle longer keys (128 hex chars = 64 bytes)', async () => {
    const longKey = 'c'.repeat(128);
    const signature = await hmacSign('test', longKey);

    expect(typeof signature).toBe('string');
    expect(signature).toHaveLength(64);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. hmacVerify
// ═══════════════════════════════════════════════════════════════════════

describe('hmacVerify', () => {
  const testKey = 'a'.repeat(64);

  it('should return true for a valid signature', async () => {
    const message = '{"model":"llama-3.3-70b"}1700000000';
    const signature = await hmacSign(message, testKey);

    const result = await hmacVerify(message, signature, testKey);

    expect(result).toBe(true);
  });

  it('should return false for a tampered message', async () => {
    const message = 'original message';
    const signature = await hmacSign(message, testKey);

    const result = await hmacVerify('tampered message', signature, testKey);

    expect(result).toBe(false);
  });

  it('should return false for a tampered signature', async () => {
    const message = 'test message';
    const signature = await hmacSign(message, testKey);
    const tamperedSignature =
      signature.substring(0, 60) + 'ff';

    const result = await hmacVerify(message, tamperedSignature, testKey);

    expect(result).toBe(false);
  });

  it('should return false for a wrong key', async () => {
    const message = 'test message';
    const signature = await hmacSign(message, testKey);
    const otherKey = 'b'.repeat(64);

    const result = await hmacVerify(message, signature, otherKey);

    expect(result).toBe(false);
  });

  it('should return false for an empty signature', async () => {
    const result = await hmacVerify('message', '', testKey);

    expect(result).toBe(false);
  });

  it('should return false for a signature of wrong length', async () => {
    const message = 'test message';

    const result = await hmacVerify(message, 'abcdef', testKey);

    expect(result).toBe(false);
  });

  it('should verify signatures produced by hmacSign across multiple calls', async () => {
    const messages = ['hello', '{"key":"value"}', '', 'a longer message with spaces 123'];

    for (const msg of messages) {
      const sig = await hmacSign(msg, testKey);
      expect(await hmacVerify(msg, sig, testKey)).toBe(true);
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// Integration: sign + verify round-trip
// ═══════════════════════════════════════════════════════════════════════

describe('HMAC round-trip (buildHmacPayload + sign + verify)', () => {
  it('should produce a verifiable signature for a realistic payload', async () => {
    const body = '{"model":"llama-3.3-70b","messages":[{"role":"user","content":"Hi"}]}';
    const timestamp = 1700000000;

    const payload = buildHmacPayload(body, timestamp);
    const key = '0123456789abcdef'.repeat(8); // 64 hex chars
    const signature = await hmacSign(payload, key);

    expect(typeof signature).toBe('string');
    expect(signature).toHaveLength(64);

    const valid = await hmacVerify(payload, signature, key);
    expect(valid).toBe(true);
  });
});
