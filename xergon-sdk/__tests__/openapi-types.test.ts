/**
 * Tests for OpenAPI type definitions and OpenAPIClient.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { OpenAPIClient } from '../src/openapi-client';
import type {
  ChatCompletionRequest,
  ChatMessage,
  ChatCompletionResponse,
  ProviderOnboardRequest,
  ProviderInfo,
  ModelSummary,
  ErrorResponse,
  OpenAPISpec,
  ApiEndpoint,
  JSONSchema,
} from '../src/openapi-types';

// ── Type Shape Tests ──────────────────────────────────────────────────

describe('OpenAPI Type Definitions', () => {
  it('ChatCompletionRequest has correct shape', () => {
    const req: ChatCompletionRequest = {
      model: 'llama-3.3-70b',
      messages: [
        { role: 'system', content: 'You are helpful.' },
        { role: 'user', content: 'Hello' },
      ],
      stream: true,
      temperature: 0.7,
      max_tokens: 1024,
      top_p: 0.9,
      stop: ['\n'],
    };

    expect(req.model).toBe('llama-3.3-70b');
    expect(req.messages).toHaveLength(2);
    expect(req.stream).toBe(true);
    expect(req.temperature).toBe(0.7);
    expect(req.max_tokens).toBe(1024);
    expect(req.top_p).toBe(0.9);
    expect(req.stop).toEqual(['\n']);
  });

  it('ChatCompletionRequest accepts optional fields only', () => {
    const req: ChatCompletionRequest = {
      model: 'llama-3.3-70b',
      messages: [{ role: 'user', content: 'Hi' }],
    };
    expect(req.model).toBe('llama-3.3-70b');
    expect(req.stream).toBeUndefined();
    expect(req.temperature).toBeUndefined();
  });

  it('ChatMessage has correct roles', () => {
    const system: ChatMessage = { role: 'system', content: 'Sys' };
    const user: ChatMessage = { role: 'user', content: 'User' };
    const assistant: ChatMessage = { role: 'assistant', content: 'Asst' };

    expect(system.role).toBe('system');
    expect(user.role).toBe('user');
    expect(assistant.role).toBe('assistant');
  });

  it('ChatCompletionResponse has correct shape', () => {
    const res: ChatCompletionResponse = {
      id: 'chatcmpl-abc',
      object: 'chat.completion',
      created: 1700000000,
      model: 'llama-3.3-70b',
      choices: [{
        index: 0,
        message: { role: 'assistant', content: 'Hello!' },
        finish_reason: 'stop',
      }],
      usage: {
        prompt_tokens: 10,
        completion_tokens: 5,
        total_tokens: 15,
      },
    };

    expect(res.id).toBe('chatcmpl-abc');
    expect(res.object).toBe('chat.completion');
    expect(res.choices).toHaveLength(1);
    expect(res.choices[0].finish_reason).toBe('stop');
    expect(res.usage.total_tokens).toBe(15);
  });

  it('ProviderOnboardRequest has correct shape', () => {
    const req: ProviderOnboardRequest = {
      endpoint: 'https://provider.example.com',
      region: 'us-east',
      auth_token: 'secret-token',
    };

    expect(req.endpoint).toBe('https://provider.example.com');
    expect(req.region).toBe('us-east');
    expect(req.auth_token).toBe('secret-token');
  });

  it('ProviderOnboardRequest works without optional auth_token', () => {
    const req: ProviderOnboardRequest = {
      endpoint: 'https://provider.example.com',
      region: 'eu-west',
    };
    expect(req.auth_token).toBeUndefined();
  });

  it('ProviderInfo has correct shape', () => {
    const info: ProviderInfo = {
      provider_pk: '0xabc123',
      endpoint: 'https://provider.example.com',
      region: 'us-east',
      models: ['llama-3.3-70b', 'mistral-7b'],
      pown_score: 95.5,
      is_active: true,
      healthy: true,
      latency_ms: 120,
    };

    expect(info.provider_pk).toBe('0xabc123');
    expect(info.models).toHaveLength(2);
    expect(info.pown_score).toBe(95.5);
    expect(info.healthy).toBe(true);
    expect(info.latency_ms).toBe(120);
  });

  it('ProviderInfo works without optional latency_ms', () => {
    const info: ProviderInfo = {
      provider_pk: '0xabc',
      endpoint: 'https://example.com',
      region: 'us',
      models: ['model1'],
      pown_score: 80,
      is_active: true,
      healthy: false,
    };
    expect(info.latency_ms).toBeUndefined();
  });

  it('ModelSummary has correct shape', () => {
    const summary: ModelSummary = {
      model_id: 'llama-3.3-70b',
      available_providers: 5,
      cheapest_price_nanoerg_per_million_tokens: 100000,
      max_context_length: 8192,
    };

    expect(summary.model_id).toBe('llama-3.3-70b');
    expect(summary.available_providers).toBe(5);
    expect(summary.cheapest_price_nanoerg_per_million_tokens).toBe(100000);
    expect(summary.max_context_length).toBe(8192);
  });

  it('ErrorResponse has correct shape', () => {
    const err: ErrorResponse = {
      error: {
        code: 'invalid_request',
        message: 'Model not found',
      },
    };

    expect(err.error.code).toBe('invalid_request');
    expect(err.error.message).toBe('Model not found');
  });

  it('ChatCompletionRequest accepts stop as string', () => {
    const req: ChatCompletionRequest = {
      model: 'test',
      messages: [{ role: 'user', content: 'hi' }],
      stop: '\n\n',
    };
    expect(typeof req.stop).toBe('string');
  });

  it('ChatCompletionRequest accepts stop as string array', () => {
    const req: ChatCompletionRequest = {
      model: 'test',
      messages: [{ role: 'user', content: 'hi' }],
      stop: ['\n', 'END'],
    };
    expect(Array.isArray(req.stop)).toBe(true);
  });
});

// ── OpenAPIClient Tests ──────────────────────────────────────────────

const mockSpec: OpenAPISpec = {
  openapi: '3.0.0',
  info: {
    title: 'Xergon Relay API',
    version: '1.0.0',
    description: 'Decentralized AI inference relay',
  },
  paths: {
    '/v1/chat/completions': {
      post: {
        operationId: 'createChatCompletion',
        summary: 'Create a chat completion',
        tags: ['Chat'],
        requestBody: {
          required: true,
          content: {
            'application/json': {
              schema: { $ref: '#/components/schemas/ChatCompletionRequest' },
            },
          },
        },
        responses: {
          '200': {
            description: 'Successful response',
            content: {
              'application/json': {
                schema: { $ref: '#/components/schemas/ChatCompletionResponse' },
              },
            },
          },
        },
      },
    },
    '/v1/models': {
      get: {
        operationId: 'listModels',
        summary: 'List available models',
        tags: ['Models'],
        responses: {
          '200': {
            description: 'List of models',
          },
        },
      },
    },
    '/v1/providers': {
      get: {
        operationId: 'listProviders',
        summary: 'List active providers',
        tags: ['Providers'],
      },
    },
  },
  components: {
    schemas: {
      ChatCompletionRequest: {
        type: 'object',
        properties: {
          model: { type: 'string' },
          messages: {
            type: 'array',
            items: { $ref: '#/components/schemas/ChatMessage' },
          },
        },
        required: ['model', 'messages'],
      },
      ChatMessage: {
        type: 'object',
        properties: {
          role: { type: 'string', enum: ['system', 'user', 'assistant'] },
          content: { type: 'string' },
        },
        required: ['role', 'content'],
      },
      ChatCompletionResponse: {
        type: 'object',
        properties: {
          id: { type: 'string' },
          model: { type: 'string' },
          choices: { type: 'array' },
        },
      },
    },
  },
  tags: [
    { name: 'Chat', description: 'Chat completion endpoints' },
    { name: 'Models', description: 'Model listing endpoints' },
    { name: 'Providers', description: 'Provider management' },
  ],
};

describe('OpenAPIClient', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let client: OpenAPIClient;

  beforeEach(() => {
    fetchMock = vi.fn();
    globalThis.fetch = fetchMock;
    client = new OpenAPIClient('https://relay.xergon.gg');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('getSpec', () => {
    it('fetches and caches the OpenAPI spec', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      const spec = await client.getSpec();
      expect(spec.openapi).toBe('3.0.0');
      expect(spec.info.title).toBe('Xergon Relay API');
      expect(fetchMock).toHaveBeenCalledTimes(1);

      // Second call should use cache
      const spec2 = await client.getSpec();
      expect(spec2).toBe(spec);
      expect(fetchMock).toHaveBeenCalledTimes(1); // Still 1
    });

    it('throws on fetch failure', async () => {
      fetchMock.mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
      });

      await expect(client.getSpec()).rejects.toThrow(
        'Failed to fetch OpenAPI spec: 500 Internal Server Error',
      );
    });

    it('requests from correct URL', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      expect(fetchMock).toHaveBeenCalledWith(
        'https://relay.xergon.gg/v1/openapi.json',
        { headers: { Accept: 'application/json' } },
      );
    });

    it('clears cache and refetches', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      client.clearCache();
      await client.getSpec();

      expect(fetchMock).toHaveBeenCalledTimes(2);
    });
  });

  describe('getEndpoints', () => {
    it('returns flat list of endpoints', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      const endpoints = client.getEndpoints();

      expect(endpoints).toHaveLength(3);

      const chat = endpoints.find((e) => e.path === '/v1/chat/completions');
      expect(chat).toBeDefined();
      expect(chat!.method).toBe('POST');
      expect(chat!.operationId).toBe('createChatCompletion');
      expect(chat!.tags).toEqual(['Chat']);

      const models = endpoints.find((e) => e.path === '/v1/models');
      expect(models).toBeDefined();
      expect(models!.method).toBe('GET');

      const providers = endpoints.find((e) => e.path === '/v1/providers');
      expect(providers).toBeDefined();
      expect(providers!.method).toBe('GET');
    });

    it('throws if spec not loaded', () => {
      expect(() => client.getEndpoints()).toThrow('Spec not loaded');
    });
  });

  describe('getSchema', () => {
    it('returns a schema by name', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      const schema = client.getSchema('ChatCompletionRequest');

      expect(schema).toBeDefined();
      expect(schema!.type).toBe('object');
      expect(schema!.required).toEqual(['model', 'messages']);
    });

    it('returns undefined for unknown schema', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      expect(client.getSchema('NonExistent')).toBeUndefined();
    });

    it('returns undefined if no components schemas', async () => {
      const specNoSchemas: OpenAPISpec = {
        openapi: '3.0.0',
        info: { title: 'Test', version: '1.0.0' },
        paths: {},
      };

      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(specNoSchemas),
      });

      await client.getSpec();
      expect(client.getSchema('anything')).toBeUndefined();
    });
  });

  describe('getEndpointById', () => {
    it('finds an endpoint by operationId', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      const ep = client.getEndpointById('listModels');

      expect(ep).toBeDefined();
      expect(ep!.path).toBe('/v1/models');
      expect(ep!.method).toBe('GET');
    });

    it('returns undefined for unknown operationId', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();
      expect(client.getEndpointById('nonexistent')).toBeUndefined();
    });
  });

  describe('getEndpointsByTag', () => {
    it('filters endpoints by tag', async () => {
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await client.getSpec();

      const chatEndpoints = client.getEndpointsByTag('Chat');
      expect(chatEndpoints).toHaveLength(1);
      expect(chatEndpoints[0].path).toBe('/v1/chat/completions');

      const modelEndpoints = client.getEndpointsByTag('Models');
      expect(modelEndpoints).toHaveLength(1);

      const nonexistent = client.getEndpointsByTag('Nonexistent');
      expect(nonexistent).toHaveLength(0);
    });
  });

  describe('base URL handling', () => {
    it('strips trailing slashes from base URL', async () => {
      const clientNoSlash = new OpenAPIClient('https://relay.xergon.gg/');
      fetchMock.mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpec),
      });

      await clientNoSlash.getSpec();
      expect(fetchMock).toHaveBeenCalledWith(
        'https://relay.xergon.gg/v1/openapi.json',
        expect.any(Object),
      );
    });
  });
});
