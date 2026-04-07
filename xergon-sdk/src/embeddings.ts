/**
 * Embeddings -- OpenAI-compatible text embedding generation.
 *
 * Provides a client method for creating text embeddings via the relay's
 * /v1/embeddings endpoint.
 *
 * @example
 * ```ts
 * import { XergonClient } from '@xergon/sdk';
 *
 * const client = new XergonClient({ baseUrl: 'https://relay.xergon.gg' });
 *
 * const response = await client.embeddings.create({
 *   model: 'text-embedding-3-small',
 *   input: 'Hello world',
 * });
 *
 * console.log(response.data[0].embedding); // number[]
 * ```
 */

import { XergonClientCore } from './client';

// ── Types ───────────────────────────────────────────────────────────

export interface EmbeddingRequest {
  /** Model to use for embedding generation. */
  model: string;
  /** Input text or array of texts to embed. */
  input: string | string[];
  /** Format of the embedding data: 'float' (default) or 'base64'. */
  encoding_format?: 'float' | 'base64';
  /** Number of dimensions for the output embedding (model-dependent). */
  dimensions?: number;
}

export interface EmbeddingData {
  object: 'embedding';
  /** The embedding vector (when encoding_format is 'float'). */
  embedding: number[];
  /** The index of this embedding in the input array. */
  index: number;
}

export interface EmbeddingUsage {
  prompt_tokens: number;
  total_tokens: number;
}

export interface EmbeddingResponse {
  object: 'list';
  data: EmbeddingData[];
  model: string;
  usage: EmbeddingUsage;
}

// ── Client Method ──────────────────────────────────────────────────

/**
 * Create embeddings for one or more text inputs via /v1/embeddings.
 */
export async function createEmbedding(
  client: XergonClientCore,
  request: EmbeddingRequest,
  options?: { signal?: AbortSignal },
): Promise<EmbeddingResponse> {
  return client.post<EmbeddingResponse>(
    '/v1/embeddings',
    request,
    { signal: options?.signal },
  );
}
