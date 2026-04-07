/**
 * Tests for CLI commands.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ArgumentParser, type Command, OutputFormatter, type CLIContext, type CLIConfig, type ParsedArgs } from '../../src/cli/mod';

// Mock XergonClient
function createMockClient(overrides: Record<string, any> = {}) {
  return {
    models: { list: vi.fn().mockResolvedValue([
      { id: 'llama-3.3-70b', object: 'model', ownedBy: 'meta', pricing: '0.001 ERG/1K tokens' },
      { id: 'mistral-7b', object: 'model', ownedBy: 'mistral-ai' },
    ])},
    providers: { list: vi.fn().mockResolvedValue([
      { publicKey: '0xabcdef1234567890', endpoint: 'https://node1.xergon.gg', models: ['llama-3.3-70b'], region: 'us-east', pownScore: 95 },
      { publicKey: '0x1234567890abcdef', endpoint: 'https://node2.xergon.gg', models: ['mistral-7b'], region: 'eu-west', pownScore: 87 },
    ])},
    leaderboard: vi.fn().mockResolvedValue([
      { publicKey: '0xabcdef1234567890', region: 'us-east', pownScore: 95, models: ['llama-3.3-70b'], totalRequests: 1000, totalTokens: 50000, online: true },
    ]),
    balance: { get: vi.fn().mockResolvedValue({
      publicKey: '0xabcdef1234567890abcdef1234567890',
      balanceNanoerg: '1000000000',
      balanceErg: '1.0',
      stakingBoxId: 'box123',
    })},
    chat: {
      completions: {
        create: vi.fn().mockResolvedValue({
          id: 'chat-1',
          object: 'chat.completion',
          created: Date.now(),
          model: 'llama-3.3-70b',
          choices: [{ index: 0, message: { role: 'assistant', content: 'Hello from Xergon!' }, finishReason: 'stop' }],
          usage: { promptTokens: 10, completionTokens: 5, totalTokens: 15 },
        }),
        stream: vi.fn().mockResolvedValue((async function*() {
          yield { id: 'chunk-1', object: 'chat.completion.chunk', created: Date.now(), model: 'llama-3.3-70b', choices: [{ index: 0, delta: { content: 'Hello' }, finishReason: null }] };
          yield { id: 'chunk-2', object: 'chat.completion.chunk', created: Date.now(), model: 'llama-3.3-70b', choices: [{ index: 0, delta: { content: ' from Xergon!' }, finishReason: null }] };
          yield { id: 'chunk-3', object: 'chat.completion.chunk', created: Date.now(), model: 'llama-3.3-70b', choices: [{ index: 0, delta: {}, finishReason: 'stop' }] };
        })()),
      },
    },
    ...overrides,
  };
}

function createMockContext(client: any): CLIContext {
  const config: CLIConfig = {
    baseUrl: 'https://relay.xergon.gg',
    apiKey: '0xabcdef1234567890abcdef1234567890',
    defaultModel: 'llama-3.3-70b',
    outputFormat: 'text',
    color: false,
    timeout: 30000,
  };
  return {
    client,
    config,
    output: new OutputFormatter('text', false),
  };
}

describe('Chat Command', () => {
  let chatCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/chat');
    chatCommand = mod.chatCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and options', () => {
    expect(chatCommand.name).toBe('chat');
    expect(chatCommand.aliases).toContain('ask');
    expect(chatCommand.aliases).toContain('complete');
    expect(chatCommand.options.map(o => o.name)).toContain('model');
    expect(chatCommand.options.map(o => o.name)).toContain('stream');
    expect(chatCommand.options.map(o => o.name)).toContain('interactive');
  });

  it('sends a chat completion and outputs text', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chatCommand.action(
      { command: 'chat', positional: ['Hello!'], options: {} },
      ctx
    );
    expect(mockClient.chat.completions.create).toHaveBeenCalledTimes(1);
    const callArgs = mockClient.chat.completions.create.mock.calls[0][0];
    expect(callArgs.messages).toContainEqual({ role: 'user', content: 'Hello!' });
    expect(callArgs.model).toBe('llama-3.3-70b');
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Hello from Xergon!'));
    writeSpy.mockRestore();
  });

  it('uses specified model', async () => {
    vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chatCommand.action(
      { command: 'chat', positional: ['Hello!'], options: { model: 'mistral-7b' } },
      ctx
    );
    const callArgs = mockClient.chat.completions.create.mock.calls[0][0];
    expect(callArgs.model).toBe('mistral-7b');
    vi.restoreAllMocks();
  });

  it('streams output when --stream flag is set', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chatCommand.action(
      { command: 'chat', positional: ['Hello!'], options: { stream: true } },
      ctx
    );
    expect(mockClient.chat.completions.stream).toHaveBeenCalledTimes(1);
    writeSpy.mockRestore();
  });

  it('sets system prompt with -s flag', async () => {
    vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chatCommand.action(
      { command: 'chat', positional: ['Hello!'], options: { system: 'You are helpful.' } },
      ctx
    );
    const callArgs = mockClient.chat.completions.create.mock.calls[0][0];
    expect(callArgs.messages).toContainEqual({ role: 'system', content: 'You are helpful.' });
    vi.restoreAllMocks();
  });

  it('sets temperature with -t flag', async () => {
    vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chatCommand.action(
      { command: 'chat', positional: ['Hello!'], options: { temperature: 0.7 } },
      ctx
    );
    const callArgs = mockClient.chat.completions.create.mock.calls[0][0];
    expect(callArgs.temperature).toBe(0.7);
    vi.restoreAllMocks();
  });

  it('outputs JSON with --json flag', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await chatCommand.action(
      { command: 'chat', positional: ['Hello!'], options: { json: true } },
      ctx
    );
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"id"'));
    vi.restoreAllMocks();
  });
});

describe('Models Command', () => {
  let modelsCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/models');
    modelsCommand = mod.modelsCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(modelsCommand.name).toBe('models');
    expect(modelsCommand.aliases).toContain('model');
    expect(modelsCommand.aliases).toContain('list-models');
  });

  it('lists models in table format', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await modelsCommand.action({ command: 'models', positional: [], options: {} }, ctx);
    expect(mockClient.models.list).toHaveBeenCalledTimes(1);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('llama-3.3-70b'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('mistral-7b'));
    writeSpy.mockRestore();
  });

  it('filters models by search term', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await modelsCommand.action({ command: 'models', positional: [], options: { search: 'llama' } }, ctx);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('llama-3.3-70b'));
    expect(writeSpy).not.toHaveBeenCalledWith(expect.stringContaining('mistral-7b'));
    writeSpy.mockRestore();
  });

  it('outputs JSON with --json flag', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await modelsCommand.action({ command: 'models', positional: [], options: { json: true } }, ctx);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"id"'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('"ownedBy"'));
    writeSpy.mockRestore();
  });
});

describe('Provider Command', () => {
  let providerCommand: Command;
  let mockClient: any;
  let ctx: CLIContext;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/provider');
    providerCommand = mod.providerCommand;
    mockClient = createMockClient();
    ctx = createMockContext(mockClient);
  });

  it('registers with correct name and aliases', () => {
    expect(providerCommand.name).toBe('provider');
    expect(providerCommand.aliases).toContain('providers');
  });

  it('lists providers', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await providerCommand.action({ command: 'provider', positional: [], options: {} }, ctx);
    expect(mockClient.providers.list).toHaveBeenCalledTimes(1);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('us-east'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('eu-west'));
    writeSpy.mockRestore();
  });

  it('filters providers by region', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await providerCommand.action({ command: 'provider', positional: [], options: { region: 'us' } }, ctx);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('us-east'));
    expect(writeSpy).not.toHaveBeenCalledWith(expect.stringContaining('eu-west'));
    writeSpy.mockRestore();
  });

  it('shows health scores with --health flag', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    await providerCommand.action({ command: 'provider', positional: [], options: { health: true } }, ctx);
    expect(mockClient.leaderboard).toHaveBeenCalledTimes(1);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Provider Health'));
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('95'));
    writeSpy.mockRestore();
  });
});

describe('Version Command', () => {
  let versionCommand: Command;

  beforeEach(async () => {
    vi.restoreAllMocks();
    const mod = await import('../../src/cli/commands/version');
    versionCommand = mod.versionCommand;
  });

  it('registers with correct name', () => {
    expect(versionCommand.name).toBe('version');
    expect(versionCommand.aliases).toContain('ver');
  });

  it('outputs version string', async () => {
    const writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const ctx = createMockContext({});
    await versionCommand.action({ command: 'version', positional: [], options: {} }, ctx);
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('0.1.0'));
    writeSpy.mockRestore();
  });
});

describe('Help Text Generation', () => {
  it('generates program help with all registered commands', () => {
    const parser = new ArgumentParser('xergon', '1.0.0');
    parser.registerCommand({
      name: 'test',
      description: 'Test command',
      aliases: ['t'],
      options: [],
      action: async () => {},
    });
    parser.addGlobalOption({
      name: 'json',
      short: '-j',
      long: '--json',
      description: 'JSON output',
      required: false,
      type: 'boolean',
    });

    const help = parser.generateHelp();
    expect(help).toContain('xergon');
    expect(help).toContain('1.0.0');
    expect(help).toContain('test');
    expect(help).toContain('Test command');
    expect(help).toContain('Aliases: t');
    expect(help).toContain('--json');
    expect(help).toContain('USAGE:');
  });

  it('generates command-specific help with options', () => {
    const parser = new ArgumentParser('xergon', '1.0.0');
    parser.registerCommand({
      name: 'deploy',
      description: 'Deploy something',
      aliases: [],
      options: [
        { name: 'force', short: '-f', long: '--force', description: 'Force deploy', required: false, type: 'boolean' },
        { name: 'env', short: '-e', long: '--env', description: 'Environment', required: true, type: 'string', default: 'dev' },
      ],
      action: async () => {},
    });

    const help = parser.generateHelp('deploy');
    expect(help).toContain('COMMAND: deploy');
    expect(help).toContain('Deploy something');
    expect(help).toContain('--force');
    expect(help).toContain('--env');
    expect(help).toContain('(required)');
    expect(help).toContain('(default: dev)');
  });
});
