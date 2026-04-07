/**
 * Tests for CLI command: org
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  orgCommand,
  orgAction,
  handleCreate,
  handleInfo,
  handleMembers,
  handleInvite,
  handleRemove,
  handleKeys,
  handleKeysList,
  handleKeysCreate,
  handleKeysRevoke,
  handleSettings,
  VALID_ROLES,
  VALID_SCOPES,
  ROLE_PERMISSIONS,
  ROLE_DISPLAY,
  SCOPE_DESCRIPTIONS,
  generateOrgId,
  generateKeyId,
  generateApiKey,
  generateInviteToken,
  generateSlug,
  validateEmail,
  validateRole,
  validateScope,
  maskApiKey,
  formatRoleBadge,
  formatScopeList,
  formatExpiry,
  formatRelativeTime,
  parseScopes,
  isJsonOutput,
  type OrgRole,
  type KeyScope,
} from './org';

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
  };
}

function createMockContext(overrides?: Record<string, any>): any {
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

// ── VALID_ROLES tests ──────────────────────────────────────────────

describe('VALID_ROLES', () => {
  it('includes Admin', () => {
    expect(VALID_ROLES).toContain('Admin');
  });
  it('includes Member', () => {
    expect(VALID_ROLES).toContain('Member');
  });
  it('includes Viewer', () => {
    expect(VALID_ROLES).toContain('Viewer');
  });
  it('has exactly 3 roles', () => {
    expect(VALID_ROLES).toHaveLength(3);
  });
});

// ── VALID_SCOPES tests ─────────────────────────────────────────────

describe('VALID_SCOPES', () => {
  it('includes read', () => {
    expect(VALID_SCOPES).toContain('read');
  });
  it('includes write', () => {
    expect(VALID_SCOPES).toContain('write');
  });
  it('includes admin', () => {
    expect(VALID_SCOPES).toContain('admin');
  });
  it('includes inference', () => {
    expect(VALID_SCOPES).toContain('inference');
  });
  it('includes deploy', () => {
    expect(VALID_SCOPES).toContain('deploy');
  });
  it('has exactly 5 scopes', () => {
    expect(VALID_SCOPES).toHaveLength(5);
  });
});

// ── generateSlug tests ─────────────────────────────────────────────

describe('generateSlug', () => {
  it('converts to lowercase with dashes', () => {
    expect(generateSlug('Acme Corp')).toBe('acme-corp');
  });
  it('removes special characters', () => {
    expect(generateSlug('Hello, World!')).toBe('hello-world');
  });
  it('handles multiple spaces', () => {
    expect(generateSlug('The  Quick   Brown')).toBe('the-quick-brown');
  });
  it('trims to 48 chars', () => {
    const long = 'a'.repeat(100);
    expect(generateSlug(long).length).toBeLessThanOrEqual(48);
  });
  it('removes leading/trailing dashes', () => {
    expect(generateSlug('  Hello  ')).toBe('hello');
  });
});

// ── validateEmail tests ────────────────────────────────────────────

describe('validateEmail', () => {
  it('accepts valid email', () => {
    expect(validateEmail('user@example.com')).toBe(true);
  });
  it('rejects missing @', () => {
    expect(validateEmail('userexample.com')).toBe(false);
  });
  it('rejects missing domain', () => {
    expect(validateEmail('user@')).toBe(false);
  });
  it('rejects empty string', () => {
    expect(validateEmail('')).toBe(false);
  });
  it('rejects spaces', () => {
    expect(validateEmail('user @example.com')).toBe(false);
  });
});

// ── validateRole tests ─────────────────────────────────────────────

describe('validateRole', () => {
  it('accepts Admin', () => {
    expect(validateRole('Admin')).toBe(true);
  });
  it('accepts Member', () => {
    expect(validateRole('Member')).toBe(true);
  });
  it('accepts Viewer', () => {
    expect(validateRole('Viewer')).toBe(true);
  });
  it('rejects invalid role', () => {
    expect(validateRole('SuperAdmin')).toBe(false);
  });
  it('rejects empty string', () => {
    expect(validateRole('')).toBe(false);
  });
  it('is case sensitive', () => {
    expect(validateRole('admin')).toBe(false);
  });
});

// ── validateScope tests ────────────────────────────────────────────

describe('validateScope', () => {
  it('accepts read', () => {
    expect(validateScope('read')).toBe(true);
  });
  it('accepts inference', () => {
    expect(validateScope('inference')).toBe(true);
  });
  it('rejects unknown scope', () => {
    expect(validateScope('superuser')).toBe(false);
  });
});

// ── maskApiKey tests ───────────────────────────────────────────────

describe('maskApiKey', () => {
  it('masks long keys', () => {
    const key = 'xg_live_abcdefghijklmnopqrstuvwxyz1234567890';
    const result = maskApiKey(key);
    expect(result).toContain('••••');
    expect(result.startsWith('xg_live_ab')).toBe(true);
  });
  it('returns dots for short keys', () => {
    expect(maskApiKey('short')).toBe('••••••••••••••••');
  });
  it('returns dots for empty string', () => {
    expect(maskApiKey('')).toBe('••••••••••••••••');
  });
});

// ── formatRoleBadge tests ──────────────────────────────────────────

describe('formatRoleBadge', () => {
  it('formats Admin role', () => {
    expect(formatRoleBadge('Admin')).toBe('[ADMIN]');
  });
  it('formats Member role', () => {
    expect(formatRoleBadge('Member')).toBe('[MEMBER]');
  });
  it('formats Viewer role', () => {
    expect(formatRoleBadge('Viewer')).toBe('[VIEWER]');
  });
});

// ── formatScopeList tests ──────────────────────────────────────────

describe('formatScopeList', () => {
  it('formats single scope', () => {
    expect(formatScopeList(['read'])).toBe('READ');
  });
  it('formats multiple scopes', () => {
    expect(formatScopeList(['read', 'write'])).toBe('READ, WRITE');
  });
  it('formats empty scopes', () => {
    expect(formatScopeList([])).toBe('');
  });
});

// ── formatExpiry tests ─────────────────────────────────────────────

describe('formatExpiry', () => {
  it('returns never for null', () => {
    expect(formatExpiry(null)).toBe('never');
  });
  it('returns expired for past dates', () => {
    expect(formatExpiry(new Date(Date.now() - 1000).toISOString())).toBe('expired');
  });
  it('shows minutes for near future', () => {
    const future = new Date(Date.now() + 5 * 60_000).toISOString();
    expect(formatExpiry(future)).toMatch(/\d+m$/);
  });
  it('shows hours and minutes', () => {
    const future = new Date(Date.now() + 90 * 60_000).toISOString();
    expect(formatExpiry(future)).toMatch(/\d+h \d+m$/);
  });
  it('shows days and hours', () => {
    const future = new Date(Date.now() + 2 * 86400_000 + 5 * 3600_000).toISOString();
    expect(formatExpiry(future)).toMatch(/\d+d \d+h$/);
  });
});

// ── formatRelativeTime tests ───────────────────────────────────────

describe('formatRelativeTime', () => {
  it('shows seconds ago', () => {
    const past = new Date(Date.now() - 30_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+s ago$/);
  });
  it('shows minutes ago', () => {
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+m ago$/);
  });
  it('shows hours ago', () => {
    const past = new Date(Date.now() - 3 * 3600_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+h ago$/);
  });
  it('shows days ago', () => {
    const past = new Date(Date.now() - 2 * 86400_000).toISOString();
    expect(formatRelativeTime(past)).toMatch(/\d+d ago$/);
  });
  it('returns unknown for invalid input', () => {
    expect(formatRelativeTime('bad')).toBe('unknown');
  });
});

// ── parseScopes tests ──────────────────────────────────────────────

describe('parseScopes', () => {
  it('parses single scope', () => {
    expect(parseScopes('read')).toEqual(['read']);
  });
  it('parses comma-separated scopes', () => {
    expect(parseScopes('read,write,inference')).toEqual(['read', 'write', 'inference']);
  });
  it('trims whitespace', () => {
    expect(parseScopes(' read , write ')).toEqual(['read', 'write']);
  });
  it('filters invalid scopes', () => {
    expect(parseScopes('read,invalid,write')).toEqual(['read', 'write']);
  });
  it('returns empty for all invalid', () => {
    expect(parseScopes('bad,evil')).toEqual([]);
  });
  it('returns empty for empty string', () => {
    expect(parseScopes('')).toEqual([]);
  });
});

// ── isJsonOutput tests ─────────────────────────────────────────────

describe('isJsonOutput', () => {
  it('returns true for --json', () => {
    expect(isJsonOutput({ command: 'org', positional: [], options: { json: true } })).toBe(true);
  });
  it('returns true for -j', () => {
    expect(isJsonOutput({ command: 'org', positional: [], options: { j: true } })).toBe(true);
  });
  it('returns false when no json flag', () => {
    expect(isJsonOutput({ command: 'org', positional: [], options: {} })).toBe(false);
  });
});

// ── generateOrgId tests ────────────────────────────────────────────

describe('generateOrgId', () => {
  it('starts with org_', () => {
    expect(generateOrgId()).toMatch(/^org_/);
  });
  it('is unique', () => {
    const ids = new Set(Array.from({ length: 100 }, () => generateOrgId()));
    expect(ids.size).toBe(100);
  });
});

// ── generateKeyId tests ────────────────────────────────────────────

describe('generateKeyId', () => {
  it('starts with xgk_', () => {
    expect(generateKeyId()).toMatch(/^xgk_/);
  });
  it('is unique', () => {
    const ids = new Set(Array.from({ length: 100 }, () => generateKeyId()));
    expect(ids.size).toBe(100);
  });
});

// ── generateApiKey tests ───────────────────────────────────────────

describe('generateApiKey', () => {
  it('starts with xg_live_', () => {
    expect(generateApiKey()).toMatch(/^xg_live_/);
  });
  it('is unique', () => {
    const keys = new Set(Array.from({ length: 100 }, () => generateApiKey()));
    expect(keys.size).toBe(100);
  });
});

// ── generateInviteToken tests ──────────────────────────────────────

describe('generateInviteToken', () => {
  it('is non-empty string', () => {
    expect(generateInviteToken().length).toBeGreaterThan(0);
  });
  it('is unique', () => {
    const tokens = new Set(Array.from({ length: 100 }, () => generateInviteToken()));
    expect(tokens.size).toBe(100);
  });
});

// ── ROLE_PERMISSIONS tests ─────────────────────────────────────────

describe('ROLE_PERMISSIONS', () => {
  it('has permissions for all roles', () => {
    for (const role of VALID_ROLES) {
      expect(ROLE_PERMISSIONS[role]).toBeDefined();
      expect(ROLE_PERMISSIONS[role].length).toBeGreaterThan(0);
    }
  });
  it('Admin has manage_members', () => {
    expect(ROLE_PERMISSIONS.Admin).toContain('manage_members');
  });
  it('Viewer does not have manage_members', () => {
    expect(ROLE_PERMISSIONS.Viewer).not.toContain('manage_members');
  });
});

// ── ROLE_DISPLAY tests ─────────────────────────────────────────────

describe('ROLE_DISPLAY', () => {
  it('has display names for all roles', () => {
    for (const role of VALID_ROLES) {
      expect(ROLE_DISPLAY[role]).toBeTruthy();
      expect(typeof ROLE_DISPLAY[role]).toBe('string');
    }
  });
});

// ── SCOPE_DESCRIPTIONS tests ───────────────────────────────────────

describe('SCOPE_DESCRIPTIONS', () => {
  it('has descriptions for all scopes', () => {
    for (const scope of VALID_SCOPES) {
      expect(SCOPE_DESCRIPTIONS[scope]).toBeTruthy();
      expect(typeof SCOPE_DESCRIPTIONS[scope]).toBe('string');
    }
  });
});

// ── orgCommand definition ──────────────────────────────────────────

describe('orgCommand', () => {
  it('has correct name', () => {
    expect(orgCommand.name).toBe('org');
  });
  it('has description', () => {
    expect(orgCommand.description).toBeTruthy();
  });
  it('has aliases', () => {
    expect(orgCommand.aliases).toContain('organization');
    expect(orgCommand.aliases).toContain('orgs');
  });
  it('has options', () => {
    expect(orgCommand.options.length).toBeGreaterThan(0);
  });
  it('has action function', () => {
    expect(typeof orgCommand.action).toBe('function');
  });
});

// ── Handler tests ──────────────────────────────────────────────────

describe('handleCreate', () => {
  it('requires name argument', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleCreate({ command: 'org', positional: [], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('name required'));
    vi.restoreAllMocks();
  });

  it('rejects name shorter than 2 chars', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleCreate({ command: 'org', positional: ['create', 'A'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('at least 2'));
    vi.restoreAllMocks();
  });

  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleCreate({ command: 'org', positional: ['create', 'Test Org'], options: {} }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith('Organization created');
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('Test Org'));
  });
});

describe('handleInfo', () => {
  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleInfo({ command: 'org', positional: ['info'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('Organization'));
  });
});

describe('handleMembers', () => {
  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleMembers({ command: 'org', positional: ['members'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalled();
  });
});

describe('handleInvite', () => {
  it('requires email argument', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleInvite({ command: 'org', positional: ['invite'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Email required'));
    vi.restoreAllMocks();
  });

  it('validates email format', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleInvite({ command: 'org', positional: ['invite', 'bad-email'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Invalid email'));
    vi.restoreAllMocks();
  });

  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleInvite({ command: 'org', positional: ['invite', 'test@example.com'], options: { role: 'Viewer' } }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith(expect.stringContaining('test@example.com'));
  });
});

describe('handleRemove', () => {
  it('requires email argument', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleRemove({ command: 'org', positional: ['remove'], options: {} }, ctx)).rejects.toThrow('exit');
    vi.restoreAllMocks();
  });

  it('requires --force flag', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleRemove({ command: 'org', positional: ['remove', 'test@example.com'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('remove'));
    vi.restoreAllMocks();
  });

  it('works with --force', async () => {
    const ctx = createMockContext();
    await handleRemove({ command: 'org', positional: ['remove', 'test@example.com'], options: { force: true } }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith(expect.stringContaining('Removed'));
  });
});

describe('handleKeysList', () => {
  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleKeysList({ command: 'org', positional: ['keys'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalled();
  });
});

describe('handleKeysCreate', () => {
  it('works with default scopes', async () => {
    const ctx = createMockContext();
    await handleKeysCreate({ command: 'org', positional: ['keys', 'create'], options: {} }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith('API key created');
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('xg_live_'));
  });

  it('rejects invalid scopes', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleKeysCreate({ command: 'org', positional: ['keys', 'create'], options: { scopes: 'invalid' } }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('No valid scopes'));
    vi.restoreAllMocks();
  });

  it('rejects invalid expiration format', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleKeysCreate({ command: 'org', positional: ['keys', 'create'], options: { expires: 'bad' } }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Invalid expiration'));
    vi.restoreAllMocks();
  });
});

describe('handleKeysRevoke', () => {
  it('requires key ID argument', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleKeysRevoke({ command: 'org', positional: ['keys', 'revoke'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Key ID required'));
    vi.restoreAllMocks();
  });

  it('requires --force flag', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(handleKeysRevoke({ command: 'org', positional: ['keys', 'revoke', 'xgk_123'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('revoke'));
    vi.restoreAllMocks();
  });

  it('works with --force', async () => {
    const ctx = createMockContext();
    await handleKeysRevoke({ command: 'org', positional: ['keys', 'revoke', 'xgk_123'], options: { force: true } }, ctx);
    expect(ctx.output.success).toHaveBeenCalledWith(expect.stringContaining('revoked'));
  });
});

describe('handleSettings', () => {
  it('works without client (offline mode)', async () => {
    const ctx = createMockContext();
    await handleSettings({ command: 'org', positional: ['settings'], options: {} }, ctx);
    expect(ctx.output.write).toHaveBeenCalledWith(expect.stringContaining('Settings'));
  });
});

describe('orgAction dispatch', () => {
  it('shows usage when no subcommand', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(orgAction({ command: 'org', positional: [], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Usage'));
    vi.restoreAllMocks();
  });

  it('rejects unknown subcommand', async () => {
    const ctx = createMockContext();
    vi.spyOn(process, 'exit').mockImplementation(() => { throw new Error('exit'); });
    await expect(orgAction({ command: 'org', positional: ['bogus'], options: {} }, ctx)).rejects.toThrow('exit');
    expect(ctx.output.writeError).toHaveBeenCalledWith(expect.stringContaining('Unknown'));
    vi.restoreAllMocks();
  });
});
