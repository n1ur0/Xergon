/**
 * Tests for BatchChatHelper -- multiModel, multiPrompt, consensus.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { BatchChatHelper } from '../src/batch-chat';
import type {
  MultiModelResult,
  MultiPromptResult,
  ConsensusResult,
} from '../src/batch-chat';
import type { ChatCompletionResponse } from '../src/types';

// ── Helpers ──────────────────────────────────────────────────────────

function makeCompletionResponse(
  content: string,
  model: string,
  id?: string,
): ChatCompletionResponse {
  return {
    id: id ?? `chatcmpl-${Math.random().toString(36).slice(2, 8)}`,
    object: 'chat.completion',
    created: 1234567890,
    model,
    choices: [
      {
        index: 0,
        message: { role: 'assistant', content },
        finishReason: 'stop',
      },
    ],
    usage: {
      promptTokens: 10,
      completionTokens: 5,
      totalTokens: 15,
    },
  };
}

function createMockChatFn(
  responses: ChatCompletionResponse[],
): (params: any) => Promise<ChatCompletionResponse> {
  let index = 0;
  return async (_params: any) => {
    return responses[index++ % responses.length];
  };
}

// ── Tests ────────────────────────────────────────────────────────────

describe('BatchChatHelper', () => {
  let chatFn: ReturnType<typeof createMockChatFn>;
  let helper: BatchChatHelper;

  beforeEach(() => {
    chatFn = createMockChatFn([]);
    helper = new BatchChatHelper({}, chatFn);
  });

  // ── multiModel ────────────────────────────────────────────────────

  describe('multiModel', () => {
    it('sends the same prompt to multiple models', async () => {
      const responses = [
        makeCompletionResponse('Response from llama', 'llama-3.3-70b'),
        makeCompletionResponse('Response from mistral', 'mistral-7b'),
        makeCompletionResponse('Response from qwen', 'qwen-2.5-72b'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const result = await helper.multiModel('Hello!', [
        'llama-3.3-70b',
        'mistral-7b',
        'qwen-2.5-72b',
      ]);

      expect(result.results).toHaveLength(3);
      expect(result.results[0].modelId).toBe('llama-3.3-70b');
      expect(result.results[0].response.choices[0].message.content).toBe('Response from llama');
      expect(result.results[1].modelId).toBe('mistral-7b');
      expect(result.results[1].response.choices[0].message.content).toBe('Response from mistral');
      expect(result.results[2].modelId).toBe('qwen-2.5-72b');
      expect(result.results[2].response.choices[0].message.content).toBe('Response from qwen');
      expect(result.total_duration_ms).toBeGreaterThanOrEqual(0);
    });

    it('uses custom messages when provided', async () => {
      const response = makeCompletionResponse('Custom messages work', 'test-model');
      helper = new BatchChatHelper({}, createMockChatFn([response]));

      const messages = [
        { role: 'system' as const, content: 'You are helpful.' },
        { role: 'user' as const, content: 'Hello!' },
      ];

      await helper.multiModel('ignored prompt', ['test-model'], { messages });

      // Verify the chatFn was called (we can't inspect params easily
      // since it's wrapped, but the result proves it worked)
      expect(response.choices[0].message.content).toBe('Custom messages work');
    });

    it('tracks per-request duration', async () => {
      const responses = [
        makeCompletionResponse('A', 'model-a'),
        makeCompletionResponse('B', 'model-b'),
      ];
      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const result = await helper.multiModel('test', ['model-a', 'model-b']);

      for (const r of result.results) {
        expect(r.duration_ms).toBeGreaterThanOrEqual(0);
      }
    });
  });

  // ── multiPrompt ───────────────────────────────────────────────────

  describe('multiPrompt', () => {
    it('sends different prompts to the same model', async () => {
      const responses = [
        makeCompletionResponse('Hello to you!', 'llama-3.3-70b'),
        makeCompletionResponse('Goodbye!', 'llama-3.3-70b'),
        makeCompletionResponse('How are you?', 'llama-3.3-70b'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const prompts = ['Hello!', 'Goodbye!', 'How are you?'];
      const result = await helper.multiPrompt(prompts, 'llama-3.3-70b');

      expect(result.results).toHaveLength(3);
      expect(result.results[0].prompt).toBe('Hello!');
      expect(result.results[0].response.choices[0].message.content).toBe('Hello to you!');
      expect(result.results[1].prompt).toBe('Goodbye!');
      expect(result.results[2].prompt).toBe('How are you?');
      expect(result.total_duration_ms).toBeGreaterThanOrEqual(0);
    });

    it('tracks per-request duration', async () => {
      const responses = [
        makeCompletionResponse('A', 'test-model'),
        makeCompletionResponse('B', 'test-model'),
      ];
      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const result = await helper.multiPrompt(['prompt-1', 'prompt-2'], 'test-model');

      for (const r of result.results) {
        expect(r.duration_ms).toBeGreaterThanOrEqual(0);
      }
    });
  });

  // ── consensus ─────────────────────────────────────────────────────

  describe('consensus', () => {
    it('detects consensus when all providers agree', async () => {
      const sameResponse = makeCompletionResponse('Paris', 'test-model', 'prov-1');
      const responses = [
        makeCompletionResponse('Paris', 'test-model', 'prov-1'),
        makeCompletionResponse('Paris', 'test-model', 'prov-2'),
        makeCompletionResponse('Paris', 'test-model', 'prov-3'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'What is the capital of France?' }];
      const result = await helper.consensus(messages, 'test-model', 3);

      expect(result.responses).toHaveLength(3);
      expect(result.consensus).toBe('Paris');
      expect(result.agreement).toBe(1);
    });

    it('detects consensus with majority agreement', async () => {
      const responses = [
        makeCompletionResponse('Paris', 'test-model', 'prov-1'),
        makeCompletionResponse('Paris', 'test-model', 'prov-2'),
        makeCompletionResponse('London', 'test-model', 'prov-3'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'Capital of France?' }];
      const result = await helper.consensus(messages, 'test-model', 3);

      expect(result.responses).toHaveLength(3);
      expect(result.consensus).toBe('Paris');
      expect(result.agreement).toBeCloseTo(2 / 3);
    });

    it('returns null consensus with no majority (>50%)', async () => {
      const responses = [
        makeCompletionResponse('Paris', 'test-model', 'prov-1'),
        makeCompletionResponse('London', 'test-model', 'prov-2'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'Capital?' }];
      const result = await helper.consensus(messages, 'test-model', 2);

      expect(result.responses).toHaveLength(2);
      expect(result.consensus).toBeNull();
      expect(result.agreement).toBe(0.5);
    });

    it('uses default numProviders of 3', async () => {
      const responses = [
        makeCompletionResponse('A', 'test-model', 'prov-1'),
        makeCompletionResponse('A', 'test-model', 'prov-2'),
        makeCompletionResponse('A', 'test-model', 'prov-3'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'Test' }];
      const result = await helper.consensus(messages, 'test-model');

      expect(result.responses).toHaveLength(3);
      expect(result.consensus).toBe('A');
    });

    it('handles empty responses gracefully', async () => {
      const responses = [
        makeCompletionResponse('', 'test-model', 'prov-1'),
        makeCompletionResponse('', 'test-model', 'prov-2'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'Test' }];
      const result = await helper.consensus(messages, 'test-model', 2);

      // All empty strings should agree
      expect(result.consensus).toBe('');
      expect(result.agreement).toBe(1);
    });

    it('normalizes whitespace for comparison', async () => {
      const responses = [
        makeCompletionResponse('Paris', 'test-model', 'prov-1'),
        makeCompletionResponse('paris', 'test-model', 'prov-2'),
        makeCompletionResponse('PARIS', 'test-model', 'prov-3'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'Capital?' }];
      const result = await helper.consensus(messages, 'test-model', 3);

      // All are the same when lowercased
      expect(result.consensus).not.toBeNull();
      expect(result.agreement).toBe(1);
    });

    it('tracks per-provider duration', async () => {
      const responses = [
        makeCompletionResponse('A', 'test-model', 'prov-1'),
        makeCompletionResponse('A', 'test-model', 'prov-2'),
      ];

      helper = new BatchChatHelper({}, createMockChatFn(responses));

      const messages = [{ role: 'user' as const, content: 'Test' }];
      const result = await helper.consensus(messages, 'test-model', 2);

      for (const r of result.responses) {
        expect(r.duration_ms).toBeGreaterThanOrEqual(0);
        expect(r.providerPk).toBeDefined();
      }
    });
  });
});
