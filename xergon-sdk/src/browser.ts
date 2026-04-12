/**
 * Xergon SDK - Browser Entry Point
 * 
 * This is a browser-compatible version of the SDK that excludes
 * Node.js-only modules (fs, crypto, path, child_process, etc.)
 * 
 * For Node.js environments, use the main index.ts export.
 */

// Re-export browser-safe types
export type {
  ChatRole,
  ChatMessage,
  ChatCompletionParams,
  ChatCompletionResponse,
  ChatCompletionChunk,
  ChatCompletionUsage,
  ChatCompletionChoice,
  ChatCompletionDelta,
  ChatCompletionChunkChoice,
  Model,
  ModelsResponse,
  Provider,
  LeaderboardEntry,
  BalanceResponse,
  GpuListing,
  GpuRental,
  GpuPricingEntry,
  GpuFilters,
  RateGpuParams,
  GpuReputation,
  IncentiveStatus,
  RareModel,
  RareModelDetail,
  BridgeChain,
  BridgeInvoiceStatus,
  BridgeInvoice,
  BridgeStatus,
  HealthResponse,
  AuthStatus,
  XergonClientConfig,
  LogInterceptor,
} from './types';

// Re-export error types
export { XergonError } from './errors';
export type { XergonErrorType, XergonErrorBody } from './errors';

// Import and re-export the core client (browser-safe)
export { XergonClient } from './client-browser';

// Re-export browser-safe modules
export { useChat } from './hooks/use-chat';
export type { UseChatOptions, ChatMessage as HookChatMessage, TokenUsage } from './hooks/use-chat';

export { useModels } from './hooks/use-models';
export type { UseModelsOptions } from './hooks/use-models';

export { useProvider } from './hooks/use-provider';
export type { UseProviderOptions, ProviderStatus } from './hooks/use-provider';
export type { ProviderInfo as ProviderHealthInfo } from './hooks/use-provider';

export { ChatWidget } from './widget/chat-widget';
export type { ChatWidgetProps } from './widget/chat-widget';

export { ChatMessageComponent } from './widget/chat-message';
export type { ChatMessageProps } from './widget/chat-message';

export { ChatInput } from './widget/chat-input';
export type { ChatInputProps } from './widget/chat-input';

export { ModelSelector } from './widget/model-selector';
export type { ModelSelectorProps } from './widget/model-selector';

export { initWidget, destroyWidget, updateWidgetConfig } from './widget/loader';

export { listTemplates, getTemplate, renderTemplate, renderTemplateRaw, addTemplate, removeTemplate } from './prompt-templates-browser';
export type { PromptTemplate, RenderedPrompt } from './prompt-templates';

export { pipeOutput, formatOutput, copyToClipboard, downloadFile, parsePipeString } from './output-pipe-browser';
export type { OutputFormat, PipeDestination, PipeConfig } from './output-pipe-browser';

// Browser-safe alias system (in-memory only, no file persistence)
const _browserAliases: Map<string, string> = new Map([
  ['code', 'deepseek-coder/DeepSeek-Coder-V2-Instruct'],
  ['fast', 'meta-llama/Meta-Llama-3.1-8B-Instruct'],
]);
export function resolveAlias(alias: string): string { return _browserAliases.get(alias) || alias; }
export function resolveModelName(name: string): string { return name; }
export function listAliases(): string[] { return Array.from(_browserAliases.keys()); }
export function addAlias(alias: string, model: string): void { _browserAliases.set(alias, model); }
export function removeAlias(alias: string): void { _browserAliases.delete(alias); }
export function getAlias(alias: string): string | undefined { return _browserAliases.get(alias); }
export type ModelAlias = { alias: string; model: string };

export { deriveAddress, derivePublicKey, generateKeypair, signMessage as signMessageOffline, verifySignature } from './wallet/offline';

export { CONTRACTS, getContract } from './contracts';
export type { ContractName, CompiledContracts } from './types/contracts';

export { buildProviderRegistrationTx, buildStakingTx, buildSettlementTx, decodeSIntLong, decodeSIntInt, ergoTreeToAddress } from './ergo-tx';

export { getOracleRate } from './oracle-client';
export type { OracleResult, ProviderRegistrationParams, StakingParams, SettlementParams } from './types/contracts';

// NOTE: The following modules require Node.js and are NOT available in browser:
// - conversation (uses fs, crypto)
// - flow (uses fs)
// - eval (uses fs)
// - canary (uses fs)
// - export (uses fs)
// - team (uses fs)
// - webhook (uses fs)
// - log (uses fs)
// - model-registry (uses fs)
// - plugins/plugin-marketplace (uses fs)
// These are available in the Node.js SDK (index.ts) but not here.
