/**
 * Tests for the ErgoPay signing module.
 *
 * Covers:
 *   1. Static URI generation
 *   2. Dynamic URI generation
 *   3. URI parsing (static and dynamic)
 *   4. Signing request creation
 *   5. Response validation
 */

import { describe, it, expect } from 'vitest';

import {
  generateErgoPayUri,
  generateErgoPayDynamicUri,
  createErgoPaySigningRequest,
  parseErgoPayUri,
  verifyErgoPayResponse,
} from '../src/wallet/ergopay';
import type { ReducedTransaction, UnsignedTransaction, ErgoPayResponse } from '../src/wallet/ergopay';

// ═══════════════════════════════════════════════════════════════════════
// 1. generateErgoPayUri (Static URI)
// ═══════════════════════════════════════════════════════════════════════

describe('generateErgoPayUri', () => {
  it('should generate a URI starting with ergopay:', () => {
    const reducedTx: ReducedTransaction = {
      id: 'abc123',
      inputs: [],
      dataInputs: [],
      outputs: [],
    };
    const uri = generateErgoPayUri(reducedTx);
    expect(uri).toMatch(/^ergopay:/);
  });

  it('should NOT use ergopay:// (double slash) for static URIs', () => {
    const reducedTx: ReducedTransaction = {
      id: 'abc123',
      inputs: [],
      dataInputs: [],
      outputs: [],
    };
    const uri = generateErgoPayUri(reducedTx);
    expect(uri).not.toMatch(/^ergopay:\/\//);
  });

  it('should produce a valid base64url-encoded payload', () => {
    const reducedTx: ReducedTransaction = {
      id: 'test-tx-id',
      inputs: [],
      dataInputs: [],
      outputs: [],
    };
    const uri = generateErgoPayUri(reducedTx);
    const encoded = uri.slice('ergopay:'.length);

    // Base64url should only contain A-Z, a-z, 0-9, -, _ and no padding
    expect(encoded).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it('should encode a complex reduced transaction', () => {
    const reducedTx: ReducedTransaction = {
      id: 'complex-tx',
      inputs: [
        { boxId: 'box1' },
        {
          boxId: 'box2',
          spendingProof: { proofBytes: 'proof', extension: {} },
        },
      ],
      dataInputs: [{ boxId: 'dbox1' }],
      outputs: [
        {
          value: 1000000,
          ergoTree: 'tree1',
          creationHeight: 800000,
          assets: [{ tokenId: 'token1', amount: 100 }],
          additionalRegisters: {},
          transactionId: 'tx1',
          index: 0,
        },
      ],
      message: 'Please sign this transaction',
    };
    const uri = generateErgoPayUri(reducedTx);
    expect(uri).toMatch(/^ergopay:[A-Za-z0-9_-]+$/);
  });

  it('should produce URIs that can be parsed back', () => {
    const reducedTx: ReducedTransaction = {
      id: 'roundtrip-tx',
      inputs: [],
      dataInputs: [],
      outputs: [],
    };
    const uri = generateErgoPayUri(reducedTx);
    const parsed = parseErgoPayUri(uri);
    expect(parsed.type).toBe('static');
    expect(parsed.data.id).toBe('roundtrip-tx');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. generateErgoPayDynamicUri
// ═══════════════════════════════════════════════════════════════════════

describe('generateErgoPayDynamicUri', () => {
  it('should generate a URI starting with ergopay://', () => {
    const uri = generateErgoPayDynamicUri('https://example.com', 'req-123');
    expect(uri).toMatch(/^ergopay:\/\//);
  });

  it('should include the requestId in the path', () => {
    const uri = generateErgoPayDynamicUri('https://example.com', 'req-abc');
    expect(uri).toContain('/api/ergo-pay/request/req-abc');
  });

  it('should strip the protocol from the base URL', () => {
    const uri = generateErgoPayDynamicUri('https://example.com', 'req-1');
    expect(uri).not.toContain('https://');
    expect(uri).toContain('example.com');
  });

  it('should strip trailing slashes from base URL', () => {
    const uri = generateErgoPayDynamicUri('https://example.com/', 'req-1');
    expect(uri).not.toContain('example.com//');
  });

  it('should append #P2PK_ADDRESS# when address param is true', () => {
    const uri = generateErgoPayDynamicUri('https://example.com', 'req-1', {
      address: true,
    });
    expect(uri).toMatch(/#P2PK_ADDRESS#$/);
  });

  it('should NOT append #P2PK_ADDRESS# when address param is false', () => {
    const uri = generateErgoPayDynamicUri('https://example.com', 'req-1', {
      address: false,
    });
    expect(uri).not.toContain('#P2PK_ADDRESS#');
  });

  it('should NOT append #P2PK_ADDRESS# when no params are provided', () => {
    const uri = generateErgoPayDynamicUri('https://example.com', 'req-1');
    expect(uri).not.toContain('#P2PK_ADDRESS#');
  });

  it('should handle http:// protocol', () => {
    const uri = generateErgoPayDynamicUri('http://localhost:3000', 'req-1');
    expect(uri).toContain('localhost:3000');
    expect(uri).not.toContain('http://');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. parseErgoPayUri
// ═══════════════════════════════════════════════════════════════════════

describe('parseErgoPayUri', () => {
  it('should parse a static URI and return type "static"', () => {
    const reducedTx: ReducedTransaction = {
      id: 'parse-test',
      inputs: [],
      dataInputs: [],
      outputs: [],
    };
    const uri = generateErgoPayUri(reducedTx);
    const parsed = parseErgoPayUri(uri);
    expect(parsed.type).toBe('static');
    expect(parsed.data).toEqual(reducedTx);
  });

  it('should parse a dynamic URI and return type "dynamic"', () => {
    const uri = 'ergopay://example.com/api/ergo-pay/request/req-123';
    const parsed = parseErgoPayUri(uri);
    expect(parsed.type).toBe('dynamic');
    expect(parsed.data.url).toBe('https://example.com/api/ergo-pay/request/req-123');
  });

  it('should parse a dynamic URI with P2PK placeholder', () => {
    const uri = 'ergopay://example.com/api/ergo-pay/request/req-123#P2PK_ADDRESS#';
    const parsed = parseErgoPayUri(uri);
    expect(parsed.type).toBe('dynamic');
    expect(parsed.data.url).toBe('https://example.com/api/ergo-pay/request/req-123#P2PK_ADDRESS#');
  });

  it('should throw for non-ergopay URIs', () => {
    expect(() => parseErgoPayUri('https://example.com')).toThrow(
      'must start with "ergopay:"',
    );
  });

  it('should throw for empty data after ergopay:', () => {
    expect(() => parseErgoPayUri('ergopay:')).toThrow('no data');
  });

  it('should throw for invalid base64url data', () => {
    expect(() => parseErgoPayUri('ergopay:!!!not-valid!!!')).toThrow('failed to decode');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. createErgoPaySigningRequest
// ═══════════════════════════════════════════════════════════════════════

describe('createErgoPaySigningRequest', () => {
  const unsignedTx: UnsignedTransaction = {
    id: 'unsigned-tx-1',
    inputs: [{ boxId: 'input-box-1' }],
    dataInputs: [],
    outputs: [
      {
        value: 1000000,
        ergoTree: 'tree1',
        creationHeight: 800000,
        assets: [],
        additionalRegisters: {},
        transactionId: 'unsigned-tx-1',
        index: 0,
      },
    ],
  };

  it('should create a signing request from an unsigned transaction', () => {
    const request = createErgoPaySigningRequest(unsignedTx);
    expect(request.reducedTx).toEqual(unsignedTx);
  });

  it('should include replyToUrl when provided', () => {
    const request = createErgoPaySigningRequest(
      unsignedTx,
      'https://example.com/callback',
    );
    expect(request.replyToUrl).toBe('https://example.com/callback');
  });

  it('should NOT include replyToUrl when not provided', () => {
    const request = createErgoPaySigningRequest(unsignedTx);
    expect(request.replyToUrl).toBeUndefined();
  });

  it('should preserve all transaction fields', () => {
    const request = createErgoPaySigningRequest(unsignedTx);
    expect(request.reducedTx.id).toBe('unsigned-tx-1');
    expect(request.reducedTx.inputs).toHaveLength(1);
    expect(request.reducedTx.outputs).toHaveLength(1);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 5. verifyErgoPayResponse
// ═══════════════════════════════════════════════════════════════════════

describe('verifyErgoPayResponse', () => {
  const validResponse: ErgoPayResponse = {
    signedTx: {
      id: 'signed-tx-id',
      inputs: [
        {
          boxId: 'box1',
          spendingProof: {
            proofBytes: 'abc123',
            extension: {},
          },
        },
      ],
      dataInputs: [],
      outputs: [
        {
          value: 1000000,
          ergoTree: 'tree1',
          creationHeight: 800000,
          assets: [],
          additionalRegisters: {},
          transactionId: 'signed-tx-id',
          index: 0,
        },
      ],
    },
  };

  it('should return true for a valid response', () => {
    expect(verifyErgoPayResponse(validResponse)).toBe(true);
  });

  it('should return false for null', () => {
    expect(verifyErgoPayResponse(null as any)).toBe(false);
  });

  it('should return false for undefined', () => {
    expect(verifyErgoPayResponse(undefined as any)).toBe(false);
  });

  it('should return false for empty object', () => {
    expect(verifyErgoPayResponse({} as any)).toBe(false);
  });

  it('should return false when signedTx is missing', () => {
    expect(verifyErgoPayResponse({ signedTx: undefined } as any)).toBe(false);
  });

  it('should return false when signedTx.id is missing', () => {
    expect(
      verifyErgoPayResponse({
        signedTx: {
          ...validResponse.signedTx,
          id: '',
        },
      } as any),
    ).toBe(false);
  });

  it('should return false when inputs array is empty', () => {
    expect(
      verifyErgoPayResponse({
        signedTx: {
          ...validResponse.signedTx,
          inputs: [],
        },
      } as any),
    ).toBe(false);
  });

  it('should return false when input has no spendingProof', () => {
    expect(
      verifyErgoPayResponse({
        signedTx: {
          ...validResponse.signedTx,
          inputs: [{ boxId: 'box1' }],
        },
      } as any),
    ).toBe(false);
  });

  it('should return false when outputs array is empty', () => {
    expect(
      verifyErgoPayResponse({
        signedTx: {
          ...validResponse.signedTx,
          outputs: [],
        },
      } as any),
    ).toBe(false);
  });

  it('should return false when output has missing required fields', () => {
    expect(
      verifyErgoPayResponse({
        signedTx: {
          ...validResponse.signedTx,
          outputs: [{ value: 100 }],
        },
      } as any),
    ).toBe(false);
  });

  it('should accept valid response with dataInputs', () => {
    const responseWithDataInputs: ErgoPayResponse = {
      signedTx: {
        ...validResponse.signedTx,
        dataInputs: [{ boxId: 'dbox1' }],
      },
    };
    expect(verifyErgoPayResponse(responseWithDataInputs)).toBe(true);
  });

  it('should return false when dataInputs is not an array', () => {
    expect(
      verifyErgoPayResponse({
        signedTx: {
          ...validResponse.signedTx,
          dataInputs: 'not-an-array',
        },
      } as any),
    ).toBe(false);
  });
});
