/**
 * OpenAPI-compatible type definitions for the Xergon Relay API.
 *
 * These types mirror the relay's OpenAPI spec and extend the existing
 * types in types.ts with additional request/response shapes.
 */

// ── Chat Completions ──────────────────────────────────────────────────

/** Request body for chat completions (OpenAI-compatible). */
export interface ChatCompletionRequest {
  model: string;
  messages: ChatMessage[];
  stream?: boolean;
  temperature?: number;
  max_tokens?: number;
  top_p?: number;
  stop?: string | string[];
}

/** A single message in a chat conversation. */
export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

/** Non-streaming chat completion response. */
export interface ChatCompletionResponse {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: Array<{
    index: number;
    message: ChatMessage;
    finish_reason: string;
  }>;
  usage: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

// ── Providers ─────────────────────────────────────────────────────────

/** Request body for provider onboarding. */
export interface ProviderOnboardRequest {
  endpoint: string;
  region: string;
  auth_token?: string;
}

/** Information about a registered provider. */
export interface ProviderInfo {
  provider_pk: string;
  endpoint: string;
  region: string;
  models: string[];
  pown_score: number;
  is_active: boolean;
  healthy: boolean;
  latency_ms?: number;
}

// ── Models ────────────────────────────────────────────────────────────

/** Summary of an available model. */
export interface ModelSummary {
  model_id: string;
  available_providers: number;
  cheapest_price_nanoerg_per_million_tokens: number;
  max_context_length: number;
}

// ── Errors ────────────────────────────────────────────────────────────

/** Standard error response from the relay. */
export interface ErrorResponse {
  error: {
    code: string;
    message: string;
  };
}

// ── OpenAPI Spec Types ────────────────────────────────────────────────

/** Describes an OpenAPI path operation. */
export interface ApiEndpoint {
  method: string;
  path: string;
  operationId?: string;
  summary?: string;
  description?: string;
  tags?: string[];
  requestBody?: {
    required?: boolean;
    content?: Record<string, { schema?: unknown }>;
  };
  responses?: Record<string, {
    description?: string;
    content?: Record<string, { schema?: unknown }>;
  }>;
}

/** A minimal representation of an OpenAPI 3.x document. */
export interface OpenAPISpec {
  openapi: string;
  info: {
    title: string;
    version: string;
    description?: string;
  };
  paths: Record<string, Record<string, ApiEndpoint>>;
  components?: {
    schemas?: Record<string, unknown>;
    securitySchemes?: Record<string, unknown>;
  };
  tags?: Array<{ name: string; description?: string }>;
}

/** JSON Schema subset used for type descriptions. */
export interface JSONSchema {
  type?: string;
  properties?: Record<string, JSONSchema>;
  items?: JSONSchema;
  required?: string[];
  description?: string;
  example?: unknown;
  enum?: unknown[];
  $ref?: string;
  oneOf?: JSONSchema[];
  anyOf?: JSONSchema[];
  allOf?: JSONSchema[];
  additionalProperties?: boolean | JSONSchema;
}
