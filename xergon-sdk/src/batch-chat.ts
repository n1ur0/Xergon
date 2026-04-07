/**
 * Batch chat helpers -- multi-model, multi-prompt, and consensus requests.
 *
 * Provides BatchChatHelper for common batch chat patterns:
 * - Send the same prompt to multiple models
 * - Send different prompts to the same model
 * - Send a conversation to multiple providers for consensus/verification
 */

import type { ChatMessage } from './types';
import type { ChatCompletionResponse } from './types';

// ── Types ────────────────────────────────────────────────────────────

export interface MultiModelResult {
  results: Array<{
    modelId: string;
    response: ChatCompletionResponse;
    duration_ms: number;
    providerPk?: string;
  }>;
  total_duration_ms: number;
}

export interface MultiPromptResult {
  results: Array<{
    prompt: string;
    response: ChatCompletionResponse;
    duration_ms: number;
  }>;
  total_duration_ms: number;
}

export interface ConsensusResult {
  responses: Array<{
    providerPk: string;
    response: string;
    duration_ms: number;
  }>;
  /** Majority response if >50% of providers agree. */
  consensus: string | null;
  /** Agreement level: 0-1, fraction of providers that returned the consensus response. */
  agreement: number;
}

/** Internal: a resolved chat completion result with metadata. */
interface ResolvedCompletion {
  response: ChatCompletionResponse;
  duration_ms: number;
  providerPk?: string;
}

// ── BatchChatHelper ──────────────────────────────────────────────────

/**
 * Helper for common batch chat patterns.
 *
 * Accepts a callback that performs a single chat completion, making it
 * agnostic to the specific client implementation.
 */
export class BatchChatHelper {
  /**
   * @param client - The XergonClient instance (used for its base URL and auth).
   * @param chatFn - Optional custom chat completion function. If not provided,
   *   uses the client's built-in chat.completions.create.
   */
  constructor(
    private client: any,
    private chatFn?: (
      params: any,
    ) => Promise<ChatCompletionResponse>,
  ) {}

  /**
   * Send the same prompt to multiple models in parallel.
   */
  async multiModel(
    prompt: string,
    modelIds: string[],
    options?: Partial<{ messages: ChatMessage[]; temperature: number; maxTokens: number }>,
  ): Promise<MultiModelResult> {
    const startTime = Date.now();

    const messages = options?.messages ?? [{ role: 'user' as const, content: prompt }];

    const results = await Promise.all(
      modelIds.map(async (modelId) => {
        const reqStart = Date.now();
        const response = await this.doChatCompletion({
          model: modelId,
          messages,
          stream: false,
          temperature: options?.temperature,
          maxTokens: options?.maxTokens,
        });
        return {
          modelId,
          response,
          duration_ms: Date.now() - reqStart,
          providerPk: response.model ? undefined : undefined,
        };
      }),
    );

    return {
      results,
      total_duration_ms: Date.now() - startTime,
    };
  }

  /**
   * Send different prompts to the same model in parallel.
   */
  async multiPrompt(
    prompts: string[],
    modelId: string,
    options?: Partial<{ temperature: number; maxTokens: number }>,
  ): Promise<MultiPromptResult> {
    const startTime = Date.now();

    const results = await Promise.all(
      prompts.map(async (prompt) => {
        const reqStart = Date.now();
        const response = await this.doChatCompletion({
          model: modelId,
          messages: [{ role: 'user' as const, content: prompt }],
          stream: false,
          temperature: options?.temperature,
          maxTokens: options?.maxTokens,
        });
        return {
          prompt,
          response,
          duration_ms: Date.now() - reqStart,
        };
      }),
    );

    return {
      results,
      total_duration_ms: Date.now() - startTime,
    };
  }

  /**
   * Send a conversation to multiple providers for consensus/verification.
   *
   * Uses the client's provider failover mechanism by specifying different
   * providers. If a providerPk is available in the response, it's extracted.
   *
   * Returns the majority response (consensus) if >50% of providers agree.
   */
  async consensus(
    messages: ChatMessage[],
    modelId: string,
    numProviders: number = 3,
  ): Promise<ConsensusResult> {
    const startTime = Date.now();

    const providerResults: Array<{
      providerPk: string;
      response: string;
      duration_ms: number;
    }> = [];

    // Execute multiple requests to get responses from different providers
    const completions = await Promise.all(
      Array.from({ length: numProviders }, async (_, i) => {
        const reqStart = Date.now();
        const response = await this.doChatCompletion({
          model: modelId,
          messages,
          stream: false,
        });
        return {
          response,
          duration_ms: Date.now() - reqStart,
          index: i,
        };
      }),
    );

    for (let i = 0; i < completions.length; i++) {
      const comp = completions[i];
      const text = extractResponseText(comp.response);
      // Use response ID as a proxy for provider identification
      const providerPk = comp.response.id ?? `provider-${i}`;
      providerResults.push({
        providerPk,
        response: text,
        duration_ms: comp.duration_ms,
      });
    }

    // Calculate consensus
    const responseCounts = new Map<string, number>();
    for (const r of providerResults) {
      const normalized = r.response.trim().toLowerCase();
      responseCounts.set(normalized, (responseCounts.get(normalized) ?? 0) + 1);
    }

    let consensus: string | null = null;
    let maxCount = 0;
    for (const [response, count] of responseCounts) {
      if (count > maxCount) {
        maxCount = count;
        consensus = response;
      }
    }

    const agreement = maxCount / providerResults.length;
    if (agreement <= 0.5) {
      consensus = null;
    }

    // Find original (non-normalized) response matching consensus
    const originalResponse = consensus != null
      ? providerResults.find(r => r.response.trim().toLowerCase() === consensus)
      : null;

    return {
      responses: providerResults,
      consensus: originalResponse?.response ?? consensus,
      agreement,
    };
  }

  // ── Private ─────────────────────────────────────────────────────────

  private async doChatCompletion(
    params: any,
  ): Promise<ChatCompletionResponse> {
    if (this.chatFn) {
      return this.chatFn(params);
    }
    // Fallback: use client's chat.completions.create
    return this.client.chat.completions.create(params);
  }
}

// ── Helpers ──────────────────────────────────────────────────────────

function extractResponseText(response: ChatCompletionResponse): string {
  if (!response.choices || response.choices.length === 0) {
    return '';
  }
  return response.choices[0].message.content ?? '';
}
