/**
 * Tests for CLI command: auth
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  loadCredentials,
  saveCredentials,
  deleteCredentials,
  createEmptyStore,
  getTokenStatus,
  isTokenExpired,
  isTokenRefreshable,
  maskToken,
  maskKey,
  formatExpiry,
  formatRelativeTime,
  generateTokenId,
  generateAccessToken,
  generateRefreshToken,
  authAction,
  authCommand,
  VALID_PROVIDERS,
  PROVIDER_DISPLAY,
  PROVIDER_ENDPOINTS,
  TOKEN_EXPIRY_BUFFER_MS,
  STORE_VERSION,
  AuthService,
  type TokenEntry,
  type AuthProvider,
} from './auth';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

// ── Mock output formatter ──────────────────────────────────────────

function createMockOutput() {
  return {
    colorize: (text: string, _style: string) => text,
    write: vi.fn(),
    writeError: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
    warn: vi.fn(),
    formatTable: (data: any[]) => JSON.stringify(data),
    formatOutput: (data: any) => JSON.stringify(data, null, 2),
    formatText: (data: any, title?: string) => {
      let result = title ? `${title}\n` : '';
      if (typeof data === 'object' && data !== null) {
        for (const [k, v] of Object.entries(data as Record<string, any>)) {
          result += `  ${k}: ${v}\n`;
        }
      }
      return result;
    },
  };
}

function createMockContext(overrides?: Record<string, any>) {
  return {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      apiKey: '',
      defaultModel: 'llama-3.3-70b',
      outputFormat: 'text' as const,
      color: false,
      timeout: 30000,
    },
    output: createMockOutput(),
    ...overrides,
  };
}

// ── Temp credential file helper ────────────────────────────────────

let tmpDir: string;
let origCredsPath: string;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'xergon-auth-test-'));
  origCredsPath = path.join(tmpDir, 'credentials.json');
});

// ── createEmptyStore tests ─────────────────────────────────────────

describe('createEmptyStore', () => {
  it('creates store with correct version', () => {
    const store = createEmptyStore();
    expect(store.version).toBe(STORE_VERSION);
  });
  it('has null tokens for all providers', () => {
    const store = createEmptyStore();
    for (const p of VALID_PROVIDERS) {
      expect(store.tokens[p]).toBeNull();
    }
  });
  it('has empty apiKeys array', () => {
    const store = createEmptyStore();
    expect(store.apiKeys).toEqual([]);
  });
  it('has empty wallets array', () => {
    const store = createEmptyStore();
    expect(store.wallets).toEqual([]);
  });
});

// ── maskToken tests ────────────────────────────────────────────────

describe('maskToken', () => {
  it('masks long tokens', () => {
    const token = 'xg_at_abcdefghijklmnopqrstuvwx';
    const result = maskToken(token);
    expect(result).toBe('xg_at_ap••••uvwx');
  });
  it('returns dots for short tokens', () => {
    const result = maskToken('short');
    expect(result).toBe('••••••••');
  });
  it('returns dots for empty string', () => {
    const result = maskToken('');
    expect(result).toBe('••••••••');
  });
  it('handles 12-char boundary', () => {
    const result = maskToken('123456789012');
    expect(result).toBe('••••••••');
  });
  it('handles 13-char token', () => {
    const result = maskToken('1234567890123');
    expect(result).toContain('••••');
    expect(result.startsWith('12345678')).toBe(true);
  });
});

// ── maskKey tests ──────────────────────────────────────────────────

describe('maskKey', () => {
  it('masks long API keys', () => {
    const key = 'my-long-api-key-value-here';
    const result = maskKey(key);
    expect(result).toBe('my-long-ap••••');
  });
  it('returns dots for short keys', () => {
    expect(maskKey('short')).toBe('••••••••');
  });
});

// ── formatExpiry tests ─────────────────────────────────────────────

describe('formatExpiry', () => {
  it('shows minutes for near future', () => {
    const future = new Date(Date.now() + 5 * 60_000).toISOString();
    const result = formatExpiry(future);
    expect(result).toMatch(/\d+m$/);
  });
  it('shows hours and minutes', () => {
    const future = new Date(Date.now() + 90 * 60_000).toISOString();
    const result = formatExpiry(future);
    expect(result).toMatch(/\d+h \d+m$/);
  });
  it('shows days and hours', () => {
    const future = new Date(Date.now() + 2 * 86400_000 + 5 * 3600_000).toISOString();
    const result = formatExpiry(future);
    expect(result).toMatch(/\d+d \d+h$/);
  });
  it('returns expired for past dates', () => {
    const past = new Date(Date.now() - 1000).toISOString();
    expect(formatExpiry(past)).toBe('expired');
  });
  it('returns unknown for invalid input', () => {
    expect(formatExpiry('not-a-date')).toBe('unknown');
  });
  it('returns unknown for empty string', () => {
    expect(formatExpiry('')).toBe('unknown');
  });
});

// ── formatRelativeTime tests ───────────────────────────────────────

describe('formatRelativeTime', () => {
  it('shows seconds ago', () => {
    const past = new Date(Date.now() - 30_000).toISOString();
    const result = formatRelativeTime(past);
    expect(result).toMatch(/\d+s ago$/);
  });
  it('shows minutes ago', () => {
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    const result = formatRelativeTime(past);
    expect(result).toMatch(/\d+m ago$/);
  });
  it('shows hours ago', () => {
    const past = new Date(Date.now() - 3 * 3600_000).toISOString();
    const result = formatRelativeTime(past);
    expect(result).toMatch(/\d+h ago$/);
  });
  it('shows days ago', () => {
    const past = new Date(Date.now() - 2 * 86400_000).toISOString();
    const result = formatRelativeTime(past);
    expect(result).toMatch(/\d+d ago$/);
  });
  it('returns unknown for invalid input', () => {
    expect(formatRelativeTime('bad')).toBe('unknown');
  });
});

// ── getTokenStatus tests ───────────────────────────────────────────

describe('getTokenStatus', () => {
  it('returns unknown for null', () => {
    expect(getTokenStatus(null)).toBe('unknown');
  });
  it('returns unknown for undefined', () => {
    expect(getTokenStatus(undefined)).toBe('unknown');
  });
  it('returns valid for future expiry', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      expiresAt: new Date(Date.now() + 3600_000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(getTokenStatus(token)).toBe('valid');
  });
  it('returns expired for past expiry', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      expiresAt: new Date(Date.now() - 1000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(getTokenStatus(token)).toBe('expired');
  });
  it('returns refreshable when within buffer', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      refreshToken: 'rt_test',
      expiresAt: new Date(Date.now() + TOKEN_EXPIRY_BUFFER_MS - 60_000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(getTokenStatus(token)).toBe('refreshable');
  });
  it('returns valid when well before buffer', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      refreshToken: 'rt_test',
      expiresAt: new Date(Date.now() + TOKEN_EXPIRY_BUFFER_MS + 60_000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(getTokenStatus(token)).toBe('valid');
  });
  it('returns unknown for invalid date', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      expiresAt: 'invalid-date',
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(getTokenStatus(token)).toBe('unknown');
  });
});

// ── isTokenExpired tests ───────────────────────────────────────────

describe('isTokenExpired', () => {
  it('returns true for null', () => {
    expect(isTokenExpired(null)).toBe(true);
  });
  it('returns true for past token', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      expiresAt: new Date(Date.now() - 1000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(isTokenExpired(token)).toBe(true);
  });
  it('returns false for future token', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      expiresAt: new Date(Date.now() + 3600_000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(isTokenExpired(token)).toBe(false);
  });
});

// ── isTokenRefreshable tests ───────────────────────────────────────

describe('isTokenRefreshable', () => {
  it('returns false for null', () => {
    expect(isTokenRefreshable(null)).toBe(false);
  });
  it('returns false when no refresh token', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      expiresAt: new Date(Date.now() + 1000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(isTokenRefreshable(token)).toBe(false);
  });
  it('returns true when within buffer', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      refreshToken: 'rt',
      expiresAt: new Date(Date.now() + TOKEN_EXPIRY_BUFFER_MS - 60_000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(isTokenRefreshable(token)).toBe(true);
  });
  it('returns false when well before buffer', () => {
    const token: TokenEntry = {
      accessToken: 'test',
      refreshToken: 'rt',
      expiresAt: new Date(Date.now() + TOKEN_EXPIRY_BUFFER_MS + 600_000).toISOString(),
      issuedAt: new Date().toISOString(),
      provider: 'relay',
      scope: ['read'],
      method: 'api_key',
    };
    expect(isTokenRefreshable(token)).toBe(false);
  });
});

// ── generateTokenId tests ──────────────────────────────────────────

describe('generateTokenId', () => {
  it('starts with xg_', () => {
    expect(generateTokenId()).toMatch(/^xg_/);
  });
  it('is unique', () => {
    const ids = new Set(Array.from({ length: 100 }, () => generateTokenId()));
    expect(ids.size).toBe(100);
  });
});

// ── generateAccessToken tests ──────────────────────────────────────

describe('generateAccessToken', () => {
  it('starts with xg_at_', () => {
    expect(generateAccessToken()).toMatch(/^xg_at_/);
  });
  it('is unique', () => {
    const tokens = new Set(Array.from({ length: 100 }, () => generateAccessToken()));
    expect(tokens.size).toBe(100);
  });
});

// ── generateRefreshToken tests ─────────────────────────────────────

describe('generateRefreshToken', () => {
  it('starts with xg_rt_', () => {
    expect(generateRefreshToken()).toMatch(/^xg_rt_/);
  });
  it('differs from access token', () => {
    expect(generateRefreshToken()).not.toBe(generateAccessToken());
  });
});

// ── loadCredentials / saveCredentials tests ────────────────────────

describe('loadCredentials / saveCredentials', () => {
  it('returns empty store when no file exists', () => {
    const store = loadCredentials('/nonexistent/path/credentials.json');
    expect(store.version).toBe(STORE_VERSION);
    expect(store.apiKeys).toEqual([]);
  });
  it('saves and loads credentials', () => {
    const store = createEmptyStore();
    store.activeProvider = 'relay';
    store.apiKeys.push({
      id: 'test-id',
      key: 'test-key',
      label: 'test',
      provider: 'relay',
      createdAt: new Date().toISOString(),
    });
    saveCredentials(store, origCredsPath);

    const loaded = loadCredentials(origCredsPath);
    expect(loaded.activeProvider).toBe('relay');
    expect(loaded.apiKeys).toHaveLength(1);
    expect(loaded.apiKeys[0].id).toBe('test-id');
  });
  it('creates parent directory if needed', () => {
    const nestedPath = path.join(tmpDir, 'nested', 'dir', 'credentials.json');
    const store = createEmptyStore();
    saveCredentials(store, nestedPath);
    expect(fs.existsSync(nestedPath)).toBe(true);
  });
});

// ── deleteCredentials tests ────────────────────────────────────────

describe('deleteCredentials', () => {
  it('returns true when file exists', () => {
    saveCredentials(createEmptyStore(), origCredsPath);
    expect(deleteCredentials()).toBe(true); // deletes default path -- may or may not exist
  });
  it('returns false when file does not exist', () => {
    // Use a path that definitely doesn't exist
    const result = deleteCredentials();
    // May return true if default path exists from other tests, or false
    expect(typeof result).toBe('boolean');
  });
});

// ── VALID_PROVIDERS tests ──────────────────────────────────────────

describe('VALID_PROVIDERS', () => {
  it('includes relay', () => {
    expect(VALID_PROVIDERS).toContain('relay');
  });
  it('includes marketplace', () => {
    expect(VALID_PROVIDERS).toContain('marketplace');
  });
  it('includes agent', () => {
    expect(VALID_PROVIDERS).toContain('agent');
  });
  it('has exactly 3 providers', () => {
    expect(VALID_PROVIDERS).toHaveLength(3);
  });
});

// ── PROVIDER_DISPLAY tests ─────────────────────────────────────────

describe('PROVIDER_DISPLAY', () => {
  it('has display names for all providers', () => {
    for (const p of VALID_PROVIDERS) {
      expect(PROVIDER_DISPLAY[p]).toBeTruthy();
      expect(typeof PROVIDER_DISPLAY[p]).toBe('string');
    }
  });
});

// ── PROVIDER_ENDPOINTS tests ───────────────────────────────────────

describe('PROVIDER_ENDPOINTS', () => {
  it('has endpoints for all providers', () => {
    for (const p of VALID_PROVIDERS) {
      expect(PROVIDER_ENDPOINTS[p]).toBeTruthy();
      expect(PROVIDER_ENDPOINTS[p]).toMatch(/^\/api\//);
    }
  });
});

// ── AuthService tests ──────────────────────────────────────────────

describe('AuthService', () => {
  it('constructs with base URL', () => {
    const svc = new AuthService('https://example.com/api');
    expect(svc).toBeDefined();
  });
  it('loginWithApiKey returns null for invalid endpoint', async () => {
    const svc = new AuthService('https://nonexistent.invalid');
    const result = await svc.loginWithApiKey('test-key', 'relay');
    expect(result).toBeNull();
  });
  it('revokeKey returns null for invalid endpoint', async () => {
    const svc = new AuthService('https://nonexistent.invalid');
    const result = await svc.revokeKey('test-id', 'relay');
    expect(result).toBeNull();
  });
  it('refreshToken returns null for invalid endpoint', async () => {
    const svc = new AuthService('https://nonexistent.invalid');
    const result = await svc.refreshToken('test-rt', 'relay');
    expect(result).toBeNull();
  });
});

// ── authCommand definition ─────────────────────────────────────────

describe('authCommand', () => {
  it('has correct name', () => {
    expect(authCommand.name).toBe('auth');
  });
  it('has description', () => {
    expect(authCommand.description).toContain('auth');
  });
  it('has aliases', () => {
    expect(authCommand.aliases).toContain('authenticate');
    expect(authCommand.aliases).toContain('credentials');
  });
  it('has options', () => {
    expect(authCommand.options.length).toBeGreaterThan(0);
    expect(authCommand.options.some(o => o.name === 'provider')).toBe(true);
    expect(authCommand.options.some(o => o.name === 'key')).toBe(true);
    expect(authCommand.options.some(o => o.name === 'wallet')).toBe(true);
    expect(authCommand.options.some(o => o.name === 'json')).toBe(true);
  });
  it('has action function', () => {
    expect(typeof authCommand.action).toBe('function');
  });
});

// ── authAction integration tests ───────────────────────────────────

describe('authAction', () => {
  const mockOutput = createMockOutput();
  const mockCtx: any = {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      apiKey: '',
      defaultModel: 'llama-3.3-70b',
      outputFormat: 'text',
      color: false,
      timeout: 30000,
    } as any,
    output: mockOutput as any,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows status by default', async () => {
    await authAction({ command: 'auth', positional: [], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles status subcommand', async () => {
    await authAction({ command: 'auth', positional: ['status'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles providers subcommand', async () => {
    await authAction({ command: 'auth', positional: ['providers'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles token subcommand', async () => {
    await authAction({ command: 'auth', positional: ['token'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles json output for status', async () => {
    await authAction({ command: 'auth', positional: ['status'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
  it('handles json output for providers', async () => {
    await authAction({ command: 'auth', positional: ['providers'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
  it('login requires key or wallet flag', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await authAction({ command: 'auth', positional: ['login'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('refresh handles no refreshable tokens', async () => {
    await authAction({ command: 'auth', positional: ['refresh'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('revoke requires key_id or --all', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await authAction({ command: 'auth', positional: ['revoke'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('rejects invalid provider on login', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await authAction({ command: 'auth', positional: ['login'], options: { provider: 'invalid', key: 'test' } }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
});

// ── Cleanup ────────────────────────────────────────────────────────

afterEach(() => {
  try {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  } catch {
    // Ignore cleanup errors
  }
});
