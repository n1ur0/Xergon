/**
 * Tests for offline wallet utilities.
 *
 * Covers:
 *   1. Keypair generation
 *   2. Public key derivation from secret key
 *   3. P2PK address derivation
 *   4. Sign/verify roundtrip
 */

import { describe, it, expect } from 'vitest';

import {
  generateKeypair,
  derivePublicKey,
  deriveAddress,
  signMessage,
  verifySignature,
} from '../src/wallet/offline';

// ═══════════════════════════════════════════════════════════════════════
// 1. generateKeypair
// ═══════════════════════════════════════════════════════════════════════

describe('generateKeypair', () => {
  it('should return an object with secretKey and publicKey', () => {
    const keypair = generateKeypair();
    expect(keypair).toHaveProperty('secretKey');
    expect(keypair).toHaveProperty('publicKey');
  });

  it('should generate a 32-byte secret key (64 hex chars)', () => {
    const keypair = generateKeypair();
    expect(keypair.secretKey).toHaveLength(64);
    expect(keypair.secretKey).toMatch(/^[0-9a-f]{64}$/);
  });

  it('should generate a 33-byte compressed public key (66 hex chars)', () => {
    const keypair = generateKeypair();
    expect(keypair.publicKey).toHaveLength(66);
    expect(keypair.publicKey).toMatch(/^[0-9a-f]{66}$/);
  });

  it('should generate unique keypairs each time', () => {
    const kp1 = generateKeypair();
    const kp2 = generateKeypair();
    expect(kp1.secretKey).not.toBe(kp2.secretKey);
    expect(kp1.publicKey).not.toBe(kp2.publicKey);
  });

  it('should generate a compressed public key (02 or 03 prefix)', () => {
    const keypair = generateKeypair();
    expect(keypair.publicKey.startsWith('02') || keypair.publicKey.startsWith('03')).toBe(
      true,
    );
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. derivePublicKey
// ═══════════════════════════════════════════════════════════════════════

describe('derivePublicKey', () => {
  it('should derive a public key from a secret key', () => {
    const keypair = generateKeypair();
    const derived = derivePublicKey(keypair.secretKey);
    expect(derived).toBe(keypair.publicKey);
  });

  it('should throw for invalid secret key length', () => {
    expect(() => derivePublicKey('abcd')).toThrow('expected 32 bytes');
  });

  it('should throw for empty string', () => {
    expect(() => derivePublicKey('')).toThrow('expected 32 bytes');
  });

  it('should produce consistent results for the same input', () => {
    const sk = '0000000000000000000000000000000000000000000000000000000000000001';
    const pk1 = derivePublicKey(sk);
    const pk2 = derivePublicKey(sk);
    expect(pk1).toBe(pk2);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. deriveAddress
// ═══════════════════════════════════════════════════════════════════════

describe('deriveAddress', () => {
  it('should derive an address from a public key', async () => {
    const keypair = generateKeypair();
    const address = await deriveAddress(keypair.publicKey);
    expect(address).toBeTruthy();
    expect(typeof address).toBe('string');
  });

  it('should produce a Base58-encoded string', async () => {
    const keypair = generateKeypair();
    const address = await deriveAddress(keypair.publicKey);
    // Base58 alphabet does not include 0, O, I, l
    expect(address).toMatch(/^[1-9A-HJ-NP-Za-km-z]+$/);
  });

  it('should throw for invalid public key length', async () => {
    await expect(deriveAddress('02abcdef')).rejects.toThrow('expected 33 bytes');
  });

  it('should produce consistent addresses for the same public key', async () => {
    const keypair = generateKeypair();
    const addr1 = await deriveAddress(keypair.publicKey);
    const addr2 = await deriveAddress(keypair.publicKey);
    expect(addr1).toBe(addr2);
  });

  it('should produce different addresses for different public keys', async () => {
    const kp1 = generateKeypair();
    const kp2 = generateKeypair();
    const addr1 = await deriveAddress(kp1.publicKey);
    const addr2 = await deriveAddress(kp2.publicKey);
    expect(addr1).not.toBe(addr2);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. Sign/Verify Roundtrip
// ═══════════════════════════════════════════════════════════════════════

describe('signMessage / verifySignature', () => {
  it('should produce a 64-byte signature (128 hex chars)', async () => {
    const keypair = generateKeypair();
    const signature = await signMessage('Hello Ergo', keypair.secretKey);
    expect(signature).toHaveLength(128);
    expect(signature).toMatch(/^[0-9a-f]{128}$/);
  });

  it('should verify a valid signature', async () => {
    const keypair = generateKeypair();
    const message = 'Hello Ergo Network!';
    const signature = await signMessage(message, keypair.secretKey);
    const valid = await verifySignature(message, signature, keypair.publicKey);
    expect(valid).toBe(true);
  });

  it('should reject a signature for a different message', async () => {
    const keypair = generateKeypair();
    const signature = await signMessage('Correct message', keypair.secretKey);
    const valid = await verifySignature('Wrong message', signature, keypair.publicKey);
    expect(valid).toBe(false);
  });

  it('should reject a signature with a different public key', async () => {
    const kp1 = generateKeypair();
    const kp2 = generateKeypair();
    const message = 'Shared message';
    const signature = await signMessage(message, kp1.secretKey);
    const valid = await verifySignature(message, signature, kp2.publicKey);
    expect(valid).toBe(false);
  });

  it('should handle empty messages', async () => {
    const keypair = generateKeypair();
    const signature = await signMessage('', keypair.secretKey);
    const valid = await verifySignature('', signature, keypair.publicKey);
    expect(valid).toBe(true);
  });

  it('should handle long messages', async () => {
    const keypair = generateKeypair();
    const message = 'a'.repeat(10000);
    const signature = await signMessage(message, keypair.secretKey);
    const valid = await verifySignature(message, signature, keypair.publicKey);
    expect(valid).toBe(true);
  });

  it('should throw for invalid secret key length', async () => {
    // 16 bytes (32 hex chars) instead of 32 bytes (64 hex chars)
    await expect(signMessage('test', 'ab'.repeat(16))).rejects.toThrow('expected 32 bytes');
  });

  it('should throw for invalid signature length', async () => {
    await expect(verifySignature('test', 'abcdef', '02' + 'ab'.repeat(32))).rejects.toThrow(
      'expected 64 bytes',
    );
  });

  it('should throw for invalid public key length', async () => {
    await expect(verifySignature('test', 'ab'.repeat(64), '02')).rejects.toThrow(
      'expected 33 or 65 bytes',
    );
  });

  it('should produce different signatures for the same message with different keys', async () => {
    const kp1 = generateKeypair();
    const kp2 = generateKeypair();
    const sig1 = await signMessage('same message', kp1.secretKey);
    const sig2 = await signMessage('same message', kp2.secretKey);
    expect(sig1).not.toBe(sig2);
  });
});
