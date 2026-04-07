/**
 * Tests for CLI command: config
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  validateValue,
  validateFullConfig,
  resolveKey,
  maskValue,
  getEffectiveValue,
  getEnvOverride,
  loadConfigFile,
  loadLocalConfig,
  configAction,
  configCommand,
  DEFAULT_CONFIG,
  CONFIG_SCHEMA,
  KEY_MAP,
  type ConfigField,
  type ValidationResult,
} from './config';

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
      color: true,
      timeout: 30000,
    },
    output: createMockOutput(),
    ...overrides,
  };
}

// ── validateValue tests ────────────────────────────────────────────

describe('validateValue', () => {
  it('accepts valid string value', () => {
    const result = validateValue('baseUrl', 'https://example.com');
    expect(result.valid).toBe(true);
    expect(result.parsed).toBe('https://example.com');
  });
  it('accepts valid boolean true', () => {
    expect(validateValue('color', 'true').valid).toBe(true);
    expect(validateValue('color', 'true').parsed).toBe(true);
  });
  it('accepts valid boolean false', () => {
    expect(validateValue('color', 'false').valid).toBe(true);
    expect(validateValue('color', 'false').parsed).toBe(false);
  });
  it('accepts boolean yes/no', () => {
    expect(validateValue('color', 'yes').parsed).toBe(true);
    expect(validateValue('color', 'no').parsed).toBe(false);
  });
  it('accepts boolean 1/0', () => {
    expect(validateValue('color', '1').parsed).toBe(true);
    expect(validateValue('color', '0').parsed).toBe(false);
  });
  it('rejects invalid boolean', () => {
    const result = validateValue('color', 'maybe');
    expect(result.valid).toBe(false);
    expect(result.error).toBeDefined();
  });
  it('accepts valid number', () => {
    const result = validateValue('timeout', '60000');
    expect(result.valid).toBe(true);
    expect(result.parsed).toBe(60000);
  });
  it('rejects non-number for number field', () => {
    const result = validateValue('timeout', 'abc');
    expect(result.valid).toBe(false);
    expect(result.error).toContain('number');
  });
  it('accepts allowed value for outputFormat', () => {
    expect(validateValue('outputFormat', 'json').valid).toBe(true);
    expect(validateValue('outputFormat', 'table').valid).toBe(true);
  });
  it('rejects unallowed value for outputFormat', () => {
    const result = validateValue('outputFormat', 'xml');
    expect(result.valid).toBe(false);
    expect(result.error).toContain('json');
  });
  it('accepts unknown key', () => {
    const result = validateValue('customKey', 'anyValue');
    expect(result.valid).toBe(true);
  });
});

// ── resolveKey tests ───────────────────────────────────────────────

describe('resolveKey', () => {
  it('resolves base-url to baseUrl', () => {
    expect(resolveKey('base-url')).toBe('baseUrl');
  });
  it('resolves base_url to baseUrl', () => {
    expect(resolveKey('base_url')).toBe('baseUrl');
  });
  it('resolves api-key to apiKey', () => {
    expect(resolveKey('api-key')).toBe('apiKey');
  });
  it('resolves model to defaultModel', () => {
    expect(resolveKey('model')).toBe('defaultModel');
  });
  it('returns original for unknown key', () => {
    expect(resolveKey('unknownKey')).toBe('unknownKey');
  });
  it('is case insensitive', () => {
    expect(resolveKey('BASE-URL')).toBe('baseUrl');
  });
});

// ── maskValue tests ────────────────────────────────────────────────

describe('maskValue', () => {
  it('masks sensitive values longer than 8 chars', () => {
    const result = maskValue('apiKey', 'my-secret-api-key-value');
    expect(result).toBe('my-secre...');
  });
  it('does not mask short values', () => {
    const result = maskValue('apiKey', 'short');
    expect(result).toBe('short');
  });
  it('does not mask non-sensitive keys', () => {
    const result = maskValue('baseUrl', 'https://very-long-url.example.com');
    expect(result).toBe('https://very-long-url.example.com');
  });
  it('converts non-string to string', () => {
    const result = maskValue('timeout', 30000);
    expect(result).toBe('30000');
  });
});

// ── validateFullConfig tests ───────────────────────────────────────

describe('validateFullConfig', () => {
  it('returns valid result for default config', () => {
    const result = validateFullConfig();
    expect(typeof result.valid).toBe('boolean');
    expect(result.configPath).toContain('.xergon');
    expect(result.localConfigPath).toContain('.xergon');
  });
  it('includes errors array', () => {
    const result = validateFullConfig();
    expect(Array.isArray(result.errors)).toBe(true);
  });
  it('includes warnings array', () => {
    const result = validateFullConfig();
    expect(Array.isArray(result.warnings)).toBe(true);
  });
  it('has configSource field', () => {
    const result = validateFullConfig();
    expect(['global', 'local', 'env', 'default']).toContain(result.configSource);
  });
});

// ── KEY_MAP tests ──────────────────────────────────────────────────

describe('KEY_MAP', () => {
  it('maps all known aliases', () => {
    expect(KEY_MAP['url']).toBe('baseUrl');
    expect(KEY_MAP['key']).toBe('apiKey');
    expect(KEY_MAP['format']).toBe('outputFormat');
    expect(KEY_MAP['agent-url']).toBe('agentUrl');
  });
});

// ── DEFAULT_CONFIG tests ───────────────────────────────────────────

describe('DEFAULT_CONFIG', () => {
  it('has baseUrl', () => {
    expect(DEFAULT_CONFIG.baseUrl).toContain('xergon');
  });
  it('has defaultModel', () => {
    expect(DEFAULT_CONFIG.defaultModel).toBeTruthy();
  });
  it('has timeout as number', () => {
    expect(typeof DEFAULT_CONFIG.timeout).toBe('number');
    expect(DEFAULT_CONFIG.timeout).toBeGreaterThan(0);
  });
});

// ── CONFIG_SCHEMA tests ────────────────────────────────────────────

describe('CONFIG_SCHEMA', () => {
  it('has all required fields', () => {
    expect(CONFIG_SCHEMA.length).toBeGreaterThanOrEqual(6);
    const keys = CONFIG_SCHEMA.map(f => f.key);
    expect(keys).toContain('baseUrl');
    expect(keys).toContain('apiKey');
    expect(keys).toContain('defaultModel');
    expect(keys).toContain('outputFormat');
    expect(keys).toContain('timeout');
    expect(keys).toContain('color');
  });
  it('has sections', () => {
    const sections = [...new Set(CONFIG_SCHEMA.map(f => f.section))];
    expect(sections).toContain('relay');
    expect(sections).toContain('auth');
    expect(sections).toContain('cli');
  });
  it('apiKey is marked sensitive', () => {
    const apiKeyField = CONFIG_SCHEMA.find(f => f.key === 'apiKey');
    expect(apiKeyField?.sensitive).toBe(true);
  });
  it('has envOverride for baseUrl', () => {
    const field = CONFIG_SCHEMA.find(f => f.key === 'baseUrl');
    expect(field?.envOverride).toBe('XERGON_BASE_URL');
  });
});

// ── configCommand definition ───────────────────────────────────────

describe('configCommand', () => {
  it('has correct name', () => {
    expect(configCommand.name).toBe('config');
  });
  it('has description', () => {
    expect(configCommand.description).toContain('config');
  });
  it('has aliases', () => {
    expect(configCommand.aliases).toContain('settings');
    expect(configCommand.aliases).toContain('cfg');
  });
  it('has options', () => {
    expect(configCommand.options.length).toBeGreaterThan(0);
    expect(configCommand.options.some(o => o.name === 'json')).toBe(true);
    expect(configCommand.options.some(o => o.name === 'set')).toBe(true);
    expect(configCommand.options.some(o => o.name === 'get')).toBe(true);
    expect(configCommand.options.some(o => o.name === 'reset')).toBe(true);
  });
  it('has action function', () => {
    expect(typeof configCommand.action).toBe('function');
  });
});

// ── configAction integration tests ─────────────────────────────────

describe('configAction', () => {
  const mockOutput = createMockOutput();
  const mockCtx: any = {
    client: null,
    config: {
      baseUrl: 'https://relay.xergon.gg',
      apiKey: '',
      defaultModel: 'llama-3.3-70b',
      outputFormat: 'text',
      color: true,
      timeout: 30000,
    } as any,
    output: mockOutput as any,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows current config by default', async () => {
    await configAction({ command: 'config', positional: [], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles list subcommand', async () => {
    await configAction({ command: 'config', positional: ['list'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles path subcommand', async () => {
    await configAction({ command: 'config', positional: ['path'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles validate subcommand', async () => {
    await configAction({ command: 'config', positional: ['validate'], options: {} }, mockCtx);
    expect(mockOutput.write).toHaveBeenCalled();
  });
  it('handles json output for validate', async () => {
    await configAction({ command: 'config', positional: ['validate'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
  it('get requires key argument', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await configAction({ command: 'config', positional: ['get'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('set requires key and value', async () => {
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    await configAction({ command: 'config', positional: ['set'], options: {} }, mockCtx);
    expect(mockOutput.writeError).toHaveBeenCalled();
    exitSpy.mockRestore();
  });
  it('handles json output for list', async () => {
    await configAction({ command: 'config', positional: ['list'], options: { json: true } }, mockCtx);
    const written = mockOutput.write.mock.calls[0][0];
    expect(() => JSON.parse(written)).not.toThrow();
  });
});

// ── getEffectiveValue tests ────────────────────────────────────────

describe('getEffectiveValue', () => {
  it('returns default value for missing config', () => {
    const { value, source } = getEffectiveValue('baseUrl');
    expect(value).toBeTruthy();
    expect(source).toBe('default');
  });
  it('has source field', () => {
    const result = getEffectiveValue('baseUrl');
    expect(['config', 'local', 'env', 'default']).toContain(result.source);
  });
});

// ── getEnvOverride tests ───────────────────────────────────────────

describe('getEnvOverride', () => {
  it('returns undefined when env var not set', () => {
    // This key doesn't have an envOverride in schema
    const result = getEnvOverride('color');
    expect(result).toBeUndefined();
  });
});
