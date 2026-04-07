/**
 * Tests for the CLI REPL module.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { processInput, type REPLState } from '../../src/cli/repl';
import { OutputFormatter, type CLIConfig } from '../../src/cli/mod';

function createMockClient(overrides: Record<string, any> = {}) {
  return {
    chat: {
      completions: {
        stream: vi.fn().mockResolvedValue((async function*() {
          yield { id: 'c1', object: 'chat.completion.chunk', created: 1234, model: 'test', choices: [{ index: 0, delta: { content: 'Hi' }, finishReason: null }] };
          yield { id: 'c2', object: 'chat.completion.chunk', created: 1234, model: 'test', choices: [{ index: 0, delta: {}, finishReason: 'stop' }] };
        })()),
      },
    },
    ...overrides,
  };
}

function createREPLState(overrides: Partial<REPLState> = {}): REPLState {
  const config: CLIConfig = {
    baseUrl: 'https://relay.xergon.gg',
    apiKey: '',
    defaultModel: 'test-model',
    outputFormat: 'text',
    color: false,
    timeout: 30000,
  };

  return {
    client: createMockClient(),
    config,
    output: new OutputFormatter('text', false),
    model: 'test-model',
    systemPrompt: undefined,
    temperature: undefined,
    maxTokens: undefined,
    messages: [],
    history: [],
    historyIndex: -1,
    running: true,
    stream: true,
    availableModels: [],
    ...overrides,
  };
}

describe('REPL', () => {
  let state: REPLState;
  let writeSpy: any;

  beforeEach(() => {
    vi.restoreAllMocks();
    state = createREPLState();
    writeSpy = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
  });

  afterEach(() => {
    writeSpy.mockRestore();
  });

  describe('processInput', () => {
    it('returns true for empty input', async () => {
      const result = await processInput(state, '');
      expect(result).toBe(true);
    });

    it('returns true for whitespace-only input', async () => {
      const result = await processInput(state, '   ');
      expect(result).toBe(true);
    });

    it('adds non-empty input to history', async () => {
      await processInput(state, 'hello');
      expect(state.history).toContain('hello');
      expect(state.historyIndex).toBe(1);
    });

    it('sends a chat message and gets a response', async () => {
      const result = await processInput(state, 'Hello!');
      expect(result).toBe(true);
      expect(state.messages).toHaveLength(2);
      expect(state.messages[0].role).toBe('user');
      expect(state.messages[0].content).toBe('Hello!');
      expect(state.messages[1].role).toBe('assistant');
      expect(state.messages[1].content).toBe('Hi');
      expect(state.client.chat.completions.stream).toHaveBeenCalledTimes(1);
    });

    it('removes failed user message on error', async () => {
      const errorClient = createMockClient({
        chat: {
          completions: {
            stream: vi.fn().mockRejectedValue(new Error('Network error')),
          },
        },
      });
      const errorState = createREPLState({ client: errorClient });
      const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
      const result = await processInput(errorState, 'Hello!');
      expect(result).toBe(true);
      expect(errorState.messages).toHaveLength(0);
      errSpy.mockRestore();
    });
  });

  describe('/quit command', () => {
    it('returns false to exit REPL', async () => {
      const result = await processInput(state, '/quit');
      expect(result).toBe(false);
    });

    it('/exit also exits', async () => {
      const result = await processInput(state, '/exit');
      expect(result).toBe(false);
    });

    it('/q also exits', async () => {
      const result = await processInput(state, '/q');
      expect(result).toBe(false);
    });
  });

  describe('/model command', () => {
    it('switches the model', async () => {
      await processInput(state, '/model llama-3.3-70b');
      expect(state.model).toBe('llama-3.3-70b');
    });

    it('shows error when no model name given', async () => {
      const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
      await processInput(state, '/model');
      expect(state.model).toBe('test-model'); // unchanged
      errSpy.mockRestore();
    });
  });

  describe('/system command', () => {
    it('sets system prompt', async () => {
      await processInput(state, '/system You are a helpful assistant.');
      expect(state.systemPrompt).toBe('You are a helpful assistant.');
      expect(state.messages[0]).toEqual({ role: 'system', content: 'You are a helpful assistant.' });
    });

    it('replaces existing system prompt', async () => {
      state.messages = [{ role: 'system', content: 'Old prompt' }];
      await processInput(state, '/system New prompt');
      expect(state.systemPrompt).toBe('New prompt');
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0].content).toBe('New prompt');
    });
  });

  describe('/temperature command', () => {
    it('sets temperature', async () => {
      await processInput(state, '/temperature 0.7');
      expect(state.temperature).toBe(0.7);
    });

    it('rejects invalid temperature', async () => {
      const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
      await processInput(state, '/temperature 5');
      expect(state.temperature).toBeUndefined();
      errSpy.mockRestore();
    });

    it('rejects non-numeric temperature', async () => {
      const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
      await processInput(state, '/temperature abc');
      expect(state.temperature).toBeUndefined();
      errSpy.mockRestore();
    });
  });

  describe('/max-tokens command', () => {
    it('sets max tokens', async () => {
      await processInput(state, '/max-tokens 100');
      expect(state.maxTokens).toBe(100);
    });

    it('rejects invalid max tokens', async () => {
      const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
      await processInput(state, '/max-tokens -5');
      expect(state.maxTokens).toBeUndefined();
      errSpy.mockRestore();
    });
  });

  describe('/clear command', () => {
    it('clears conversation history but preserves system message', async () => {
      state.messages = [
        { role: 'system', content: 'System' },
        { role: 'user', content: 'Hello' },
        { role: 'assistant', content: 'Hi' },
      ];
      await processInput(state, '/clear');
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0].role).toBe('system');
    });

    it('clears all messages when no system prompt', async () => {
      state.messages = [
        { role: 'user', content: 'Hello' },
        { role: 'assistant', content: 'Hi' },
      ];
      await processInput(state, '/clear');
      expect(state.messages).toHaveLength(0);
    });
  });

  describe('/history command', () => {
    it('shows message count', async () => {
      state.messages = [
        { role: 'user', content: 'Hello' },
        { role: 'assistant', content: 'Hi there' },
      ];
      await processInput(state, '/history');
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('2 message(s)'));
    });

    it('shows empty state', async () => {
      await processInput(state, '/history');
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('No conversation history'));
    });
  });

  describe('/help command', () => {
    it('shows available commands', async () => {
      await processInput(state, '/help');
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('/model'));
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('/system'));
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('/clear'));
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('/quit'));
    });
  });

  describe('/config command', () => {
    it('shows current REPL settings', async () => {
      await processInput(state, '/config');
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Current REPL Settings'));
      expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('test-model'));
    });
  });

  describe('unknown commands', () => {
    it('shows error for unknown /commands', async () => {
      const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
      await processInput(state, '/foobar');
      expect(errSpy).toHaveBeenCalledWith(expect.stringContaining('Unknown command'));
      errSpy.mockRestore();
    });
  });
});
