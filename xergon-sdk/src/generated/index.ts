/**
 * Bridge between hand-written openapi-types.ts and auto-generated types.
 *
 * The types in ../generated/ are produced by the OpenAPI generator from
 * the relay's OpenAPI spec. The hand-written types in openapi-types.ts
 * provide a simplified, stable surface for SDK consumers.
 *
 * This module re-exports the generated types so they are accessible via
 * the barrel export, and provides mapping utilities for converting between
 * the two representations when needed.
 */

// Re-export all auto-generated API clients
export {
  NetworkApi,
  InferenceApi,
  HealthApi,
  BridgeApi,
  GPUBazarApi,
  IncentiveApi,
} from './apis';

// Re-export all auto-generated model types (using 'export type' for isolatedModules)
export type {
  ChatCompletionRequest,
  ChatCompletionResponse,
  ChatCompletionResponseChoicesInner,
  ChatCompletionResponseUsage,
  ChatMessage,
  ErrorResponse,
  ErrorResponseError,
  ModelsResponse,
  ModelsResponseDataInner,
  ProviderEntry,
  BalanceResponse,
  AuthStatus200Response,
  BridgeInvoice,
  BridgeStatus200Response,
  CreateInvoiceRequest,
  ConfirmPaymentRequest,
  RefundInvoiceRequest,
  GpuListing,
  GpuRental,
  RentGpuRequest,
  RateGpuRequest,
  GetGpuPricing200Response,
  GetGpuReputation200Response,
  IncentiveStatus200Response,
  IncentiveModels200ResponseInner,
} from './models';

// Re-export runtime utilities for building typed API clients
export {
  BaseAPI,
  RequiredError,
  JSONApiResponse,
} from './runtime';
export type {
  Configuration,
  RequestOpts,
} from './runtime';
