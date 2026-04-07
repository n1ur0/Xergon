/**
 * XergonClient -- the main entry point for the Xergon SDK.
 *
 * Provides a fluent API surface covering all relay endpoints:
 * chat completions, models, providers, balance, GPU Bazar,
 * incentive system, bridge, and health probes.
 *
 * @example
 * ```ts
 * import { XergonClient } from '@xergon/sdk';
 *
 * const client = new XergonClient({
 *   baseUrl: 'https://relay.xergon.gg',
 *   publicKey: '0x...',
 *   privateKey: '0x...',
 * });
 *
 * const models = await client.models.list();
 * const completion = await client.chat.completions.create({
 *   model: 'llama-3.3-70b',
 *   messages: [{ role: 'user', content: 'Hello!' }],
 * });
 *
 * for await (const chunk of await client.chat.completions.stream({
 *   model: 'llama-3.3-70b',
 *   messages: [{ role: 'user', content: 'Hello!' }],
 * })) {
 *   process.stdout.write(chunk.choices[0]?.delta?.content ?? '');
 * }
 * ```
 */

import { XergonClientCore } from './client';
import type { XergonClientConfig, LogInterceptor } from './types';
import { XergonWebSocketClient } from './ws';
import type { WSOptions, WSConnectionState, ChatChunk, WSError } from './ws';
import { OpenAPIClient } from './openapi-client';
import { BatchClient } from './batch';
import type { BatchRequestItem, BatchResponseItem, BatchRequest, BatchResponse } from './batch';
import { RequestQueue } from './queue';
import type { QueueOptions, QueueStats } from './queue';
import { BatchChatHelper } from './batch-chat';
import type { MultiModelResult, MultiPromptResult, ConsensusResult } from './batch-chat';
import { RetryClient } from './retry';
import type { RetryConfig, BackoffStrategy, RetryStats } from './retry';
import { CancellationManager, CancellationToken } from './cancellation';
import { ResilientHttpClient } from './resilient-client';
import type { ResilientOptions, RequestOptions, RequestConfig } from './resilient-client';

// Re-export all types
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

// Re-export contract API types
export type {
  RegisterProviderApiParams,
  RegisterProviderResult,
  ProviderBoxStatus,
  OnChainProvider,
  CreateStakingBoxApiParams,
  CreateStakingBoxResult,
  UserStakingBalance,
  StakingBoxInfo,
  OraclePoolStatus,
  SettleableBox,
  BuildSettlementApiParams,
  BuildSettlementResult,
} from './types/contracts';

// Re-export WebSocket types and client
export { XergonWebSocketClient } from './ws';
export type {
  WSOptions,
  WSConnectionState,
  ChatChunk,
  WSError,
} from './ws';

// Re-export OpenAPI types and client
export { OpenAPIClient } from './openapi-client';
export type {
  ChatCompletionRequest as OpenAPIChatCompletionRequest,
  ChatMessage as OpenAPIChatMessage,
  ChatCompletionResponse as OpenAPIChatCompletionResponse,
  ProviderOnboardRequest,
  ProviderInfo,
  ModelSummary,
  ErrorResponse,
  ApiEndpoint,
  OpenAPISpec,
  JSONSchema,
} from './openapi-types';

// Re-export errors
export { XergonError } from './errors';
export type { XergonErrorType, XergonErrorBody } from './errors';

// ── Embeddings ──────────────────────────────────────────────────────

export { createEmbedding } from './embeddings';
export type {
  EmbeddingRequest,
  EmbeddingResponse,
  EmbeddingData,
  EmbeddingUsage,
} from './embeddings';

// ── Audio (TTS / STT / Translation) ────────────────────────────────

export { createSpeech, createTranscription, createTranslation } from './audio';
export type {
  SpeechRequest,
  TranscriptionRequest,
  TranscriptionResponse,
} from './audio';

// ── File Upload ────────────────────────────────────────────────────

export { uploadFile, listFiles, getFile, deleteFile, downloadFile } from './upload';
export type {
  UploadRequest,
  FileObject,
} from './upload';

// ── Fine-Tuning ────────────────────────────────────────────────────

export {
  createFineTuneJob,
  listFineTuneJobs,
  getFineTuneJob,
  cancelFineTuneJob,
  exportFineTuneJob,
} from './fine-tune';
export type {
  FineTuneCreateRequest,
  FineTuneJob,
  FineTuneExportResult,
} from './fine-tune';

// ── Deploy ─────────────────────────────────────────────────────────

export {
  deploy,
  listDeployments,
  getDeployment,
  stopDeployment,
  getDeploymentLogs,
} from './deploy';
export type {
  DeployConfig,
  Deployment,
  DeploymentLog,
} from './deploy';

// ── Plugin System ──────────────────────────────────────────────────

export { PluginManager, getBuiltinPlugins } from './plugins/plugin-manager';
export {
  loggingPlugin,
  retryPlugin,
  cachePlugin,
  rateLimitDisplayPlugin,
} from './plugins/plugin-manager';
export type {
  Plugin,
  PluginHooks,
  PluginManifest,
  PluginState,
  HookName,
} from './plugins/plugin-manager';

// ── Config Profiles ────────────────────────────────────────────────

export {
  getProfile,
  listProfiles,
  setProfile,
  useProfile,
  getCurrentProfile,
  deleteProfile,
  applyProfileOverrides,
} from './config/profiles';
export type { ProfileConfig, ProfilesData } from './config/profiles';

// ── Bench ──────────────────────────────────────────────────────────

export { runBench } from './bench';
export type { BenchConfig, BenchResult } from './bench';

// ── Workspace ──────────────────────────────────────────────────────

export {
  createWorkspace,
  switchWorkspace,
  listWorkspaces,
  deleteWorkspace,
  setWorkspaceVar,
  getWorkspaceVar,
  getCurrentWorkspace,
  getWorkspace,
} from './workspace';
export type { Workspace, WorkspaceConfig } from './workspace';

// ── Batch Requests ───────────────────────────────────────────────────

export { BatchClient } from './batch';
export type {
  BatchRequestItem,
  BatchResponseItem,
  BatchRequest,
  BatchResponse,
} from './batch';

// ── Request Queue ────────────────────────────────────────────────────

export { RequestQueue } from './queue';
export type { QueueOptions, QueueStats } from './queue';

// ── Batch Chat Helper ────────────────────────────────────────────────

export { BatchChatHelper } from './batch-chat';
export type { MultiModelResult, MultiPromptResult, ConsensusResult } from './batch-chat';

// ── Multi-Provider Failover ──────────────────────────────────────────

export { FailoverProviderManager } from './providers/failover';
export type {
  ProviderEndpoint,
  FailoverOptions,
  EndpointHealth,
  EndpointStatus,
} from './providers/failover';
export { AllEndpointsFailedError } from './providers/failover';

// ── Cost Optimization ────────────────────────────────────────────────

export { TokenCounter, CostEstimator, BudgetGuard } from './cost-optimizer';
export type {
  PricingInfo,
  CostEstimatorOptions,
  BudgetOptions,
  BudgetSummary,
  BudgetUsageEntry,
} from './cost-optimizer';

// Re-export retry utilities
export { retryWithBackoff, calculateBackoffDelay, isNetworkError } from './retry';
export { RetryClient } from './retry';
export type { RetryOptions, RetryConfig, BackoffStrategy, RetryStats } from './retry';

// ── Cancellation ─────────────────────────────────────────────────────

export { CancellationToken, CancellationManager } from './cancellation';

// ── Resilient HTTP Client ────────────────────────────────────────────

export { ResilientHttpClient } from './resilient-client';
export type { ResilientOptions, RequestOptions, RequestConfig } from './resilient-client';

// Re-export auth helpers
export { hmacSign, hmacVerify, buildHmacPayload } from './auth';

// ── React Hooks ─────────────────────────────────────────────────────

export { useChat } from './hooks/use-chat';
export type { UseChatOptions, ChatMessage as HookChatMessage, TokenUsage } from './hooks/use-chat';

export { useModels } from './hooks/use-models';
export type { UseModelsOptions } from './hooks/use-models';

export { useProvider } from './hooks/use-provider';
export type { UseProviderOptions, ProviderStatus } from './hooks/use-provider';
export type { ProviderInfo as ProviderHealthInfo } from './hooks/use-provider';

// ── Chat Widget ─────────────────────────────────────────────────────

export { ChatWidget } from './widget/chat-widget';
export type { ChatWidgetProps } from './widget/chat-widget';

export { ChatMessageComponent } from './widget/chat-message';
export type { ChatMessageProps } from './widget/chat-message';

export { ChatInput } from './widget/chat-input';
export type { ChatInputProps } from './widget/chat-input';

export { ModelSelector } from './widget/model-selector';
export type { ModelSelectorProps } from './widget/model-selector';

// ── Widget Loader (script-tag embedding) ────────────────────────────

export { initWidget, destroyWidget, updateWidgetConfig } from './widget/loader';

// ── Prompt Templates ──────────────────────────────────────────────

export {
  listTemplates,
  getTemplate,
  renderTemplate,
  renderTemplateRaw,
  addTemplate,
  removeTemplate,
} from './prompt-templates';
export type {
  PromptTemplate,
  RenderedPrompt,
} from './prompt-templates';

// ── Template Marketplace ──────────────────────────────────────────

export {
  searchTemplates as searchMarketplaceTemplates,
  getTemplate as getMarketplaceTemplate,
  downloadTemplate as downloadMarketplaceTemplate,
  publishTemplate,
  updatePublishedTemplate,
  unpublishTemplate,
  forkTemplate,
  rateTemplate,
  getTemplateReviews,
  getMyTemplates,
  getPopularTemplates,
  getTrendingTemplates,
  getVerifiedTemplates,
  getCategories as getTemplateCategories,
  clearMarketplaceCache,
} from './template-marketplace';
export type {
  SharedTemplate,
  TemplateReview,
  TemplateSearchOptions,
  TemplateCategory,
  PublishOptions,
} from './template-marketplace';

// ── Output Piping ─────────────────────────────────────────────────

export {
  pipeOutput,
  formatOutput,
  copyToClipboard,
  appendToFile,
  pipeToCommand,
  parsePipeString,
} from './output-pipe';
export type {
  OutputFormat,
  PipeDestination,
  PipeConfig,
} from './output-pipe';

// ── Model Aliases ─────────────────────────────────────────────────

export {
  resolveAlias,
  resolveModelName,
  listAliases,
  addAlias,
  removeAlias,
  getAlias,
} from './model-alias';
export type { ModelAlias } from './model-alias';

// ── ErgoPay (EIP-20) ──────────────────────────────────────────────

export type {
  ReducedTransaction,
  ErgoPaySigningRequest,
  ErgoPayResponse,
} from './wallet/ergopay';

export {
  generateErgoPayUri,
  generateErgoPayDynamicUri,
  createErgoPaySigningRequest,
  parseErgoPayUri,
  verifyErgoPayResponse,
} from './wallet/ergopay';

// ── Offline Wallet Utilities ──────────────────────────────────────

export {
  deriveAddress,
  derivePublicKey,
  generateKeypair,
  signMessage as signMessageOffline,
  verifySignature,
} from './wallet/offline';

// ── Contracts (v2) ────────────────────────────────────────────────────

export { CONTRACTS, getContract } from './contracts';
export type { ContractName, CompiledContracts } from './types/contracts';

// ── Ergo Transaction Building (v2) ───────────────────────────────────

export {
  buildProviderRegistrationTx,
  buildStakingTx,
  buildSettlementTx,
  decodeSIntLong,
  decodeSIntInt,
  ergoTreeToAddress,
} from './ergo-tx';

// ── Oracle Client (v2) ───────────────────────────────────────────────

export { getOracleRate } from './oracle-client';
export type {
  OracleResult,
  ProviderRegistrationParams,
  StakingParams,
  SettlementParams,
} from './types/contracts';

// ── Conversation Memory ───────────────────────────────────────

export {
  createConversation,
  addMessage,
  getConversation,
  listConversations,
  deleteConversation,
  setActive,
  getActive,
  getMessagesForContext,
  searchConversations,
  exportConversation,
  importConversation,
} from './conversation';
export type {
  Message,
  Conversation,
  ConversationStore,
} from './conversation';

// ── Flow / Pipeline Builder ────────────────────────────────────

export {
  createFlow,
  runFlow,
  runFlowParallel,
  listBuiltInFlows,
  getBuiltInFlow,
} from './flow';
export type {
  FlowStep,
  Flow,
  FlowResult,
  FlowExecutor,
} from './flow';

// ── Eval Benchmark Runner ────────────────────────────────────────

export {
  runBenchmark,
  listBenchmarks,
  compareBenchmarks,
  exportResults as exportEvalResults,
  saveToHistory as saveEvalToHistory,
  loadHistory as loadEvalHistory,
} from './eval';
export type {
  EvalBenchmark,
  EvalResult,
  RunBenchmarkOptions,
  CompareResult,
} from './eval';

// ── Canary Deployment ─────────────────────────────────────────────

export {
  startCanary,
  checkCanary,
  promoteCanary,
  rollbackCanary,
  listCanaries,
  recordCanaryRequest,
  loadCanaryHistory,
  saveCanaryToHistory,
} from './canary';
export type {
  CanaryConfig,
  CanaryMetrics,
  CanaryDeployment,
  CanaryCheckResult,
  CanaryHistoryEntry,
} from './canary';

// ── Data Export / Portability ─────────────────────────────────────

export {
  exportData,
  importData,
  validateExport,
  listExportScopes,
  getExportSize,
} from './export';
export {
  ExportFormat,
} from './export';
export type {
  ExportConfig,
  ExportManifest,
  ExportResult,
  ScopeInfo,
} from './export';

// ── Plugin Marketplace ──────────────────────────────────────────

export { PluginMarketplace } from './plugins/plugin-marketplace';
export {
  searchPlugins,
  getPlugin,
  installPlugin,
  uninstallPlugin,
  updatePlugin,
  publishPlugin,
  listInstalledPlugins,
  getPluginReviews,
  ratePlugin,
  getCategories,
  getPopularPlugins,
  getFeaturedPlugins,
} from './plugins/plugin-marketplace';
export type {
  MarketplacePluginManifest,
  MarketplacePlugin,
  PluginReview,
  PluginSortField,
  SearchOptions,
} from './plugins/plugin-marketplace';

// ── Team Collaboration ────────────────────────────────────────────

export { TeamClient } from './team';
export {
  createTeam,
  getTeam,
  listTeams,
  updateTeam,
  deleteTeam,
  inviteMember,
  acceptInvite,
  removeMember,
  updateRole,
  getTeamActivity,
  getTeamUsage,
  transferOwnership,
} from './team';
export type {
  Team,
  TeamMember,
  TeamSettings,
  TeamInvite,
  TeamActivity,
  TeamUsage,
  TeamRole,
  NotificationLevel,
  CreateTeamParams,
  UpdateTeamParams,
} from './team';

// ── Webhook Management ────────────────────────────────────────────

export { WebhookClient, SUPPORTED_WEBHOOK_EVENTS } from './webhook';
export {
  createWebhook,
  listWebhooks,
  getWebhook,
  updateWebhook,
  deleteWebhook,
  testWebhook,
  getDeliveries,
  replayDelivery,
  getSupportedEvents,
} from './webhook';
export type {
  Webhook,
  WebhookEvent,
  WebhookDelivery,
  CreateWebhookParams,
  UpdateWebhookParams,
} from './webhook';

// ── Enhanced Logging ───────────────────────────────────────────

export {
  setLevel,
  getLevel,
  debug,
  info,
  warn,
  error,
  getHistory,
  exportLogs,
  clearHistory,
} from './log';
export {
  LogLevel,
} from './log';
export type {
  LogEntry,
  LogConfig,
} from './log';

// ── Model Registry ───────────────────────────────────────────

export {
  listModels as registryListModels,
  getModel,
  searchModels,
  getModelVersions,
  compareModels,
  getRecommended,
  getPopularModels,
  subscribeModel,
  getDeprecationNotice,
  getModelLineage,
  notifyModelChange,
  setPopularityScore,
  registerVersion,
  registerLineage,
  clearRegistryCache,
} from './model-registry';
export type {
  ModelInfo,
  ModelVersion,
  ModelFilter,
  SortOption,
  PaginationOptions,
  ModelComparison,
  ModelRecommendation,
  LineageNode,
} from './model-registry';

// ── Debug Diagnostics ─────────────────────────────────────────

export {
  runDiagnostics,
  runDiagnostic,
  generateDebugDump,
  troubleshoot,
  checkConnectionToEndpoint,
  checkModelAvailabilityAtUrl,
  verifyWalletConnection,
  checkDiskSpaceAvailable,
  measureNetworkLatency,
  getSystemInfo,
  exportDiagnostics,
} from './debug';
export type {
  DiagnosticResult,
  DebugDump,
  DiagnosticCategory,
} from './debug';

// ── Documentation Generator ──────────────────────────────────

export {
  generateCLIDocs,
  generateAPIDocs,
  generateConfigDocs,
  generatePluginDocs,
  generateQuickStart,
  generateCheatsheet,
  serveDocs,
} from './docs-generator';
export type {
  DocConfig,
} from './docs-generator';

// ── API Gateway ─────────────────────────────────────────────────

export { Gateway } from './gateway';
export type {
  GatewayConfig,
  GatewayRoute,
  RateLimitConfig,
  GatewayAuthConfig,
  LoadBalancerConfig,
  RetryPolicy,
  CacheConfig,
  GatewayLoggingConfig,
  CorsConfig,
  GatewayMetrics,
  GatewayHealth,
  GatewayMiddlewareType,
  GatewayLogLevel,
  GatewayLogEntry,
} from './gateway';

// Import API modules
import { createEmbedding } from './embeddings';
import { createChatCompletion, streamChatCompletion } from './chat';
import { listModels } from './models';
import { listProviders, getLeaderboard } from './providers';
import { getBalance } from './balance';
import { createSpeech, createTranscription, createTranslation } from './audio';
import { uploadFile, listFiles, getFile, deleteFile, downloadFile } from './upload';
import {
  listGpuListings,
  getGpuListing,
  rentGpu,
  getMyRentals,
  getGpuPricing,
  rateGpu,
  getGpuReputation,
} from './gpu';
import {
  getIncentiveStatus,
  getIncentiveModels,
  getIncentiveModelDetail,
} from './incentive';
import {
  getBridgeStatus,
  getBridgeInvoices,
  getBridgeInvoice,
  createBridgeInvoice,
  confirmBridgePayment,
  refundBridgeInvoice,
} from './bridge';
import { healthCheck, readyCheck } from './health';
import {
  registerProvider as apiRegisterProvider,
  queryProviderStatus as apiQueryProviderStatus,
  listOnChainProviders as apiListOnChainProviders,
  createStakingBox as apiCreateStakingBox,
  queryUserBalance as apiQueryUserBalance,
  getUserStakingBoxes as apiGetUserStakingBoxes,
  getOracleRate as apiGetOracleRate,
  getOraclePoolStatus as apiGetOraclePoolStatus,
  getSettleableBoxes as apiGetSettleableBoxes,
  buildSettlementTx as apiBuildSettlementTx,
} from './contracts-api';

export class XergonClient {
  private core: XergonClientCore;
  private _ws: XergonWebSocketClient | null = null;
  private _openapi: OpenAPIClient;
  private _batch: BatchClient | null = null;
  private _queue: RequestQueue | null = null;
  private _batchChat: BatchChatHelper | null = null;
  private _retry: RetryClient | null = null;
  private _cancellation: CancellationManager | null = null;
  private _resilientClient: ResilientHttpClient | null = null;

  constructor(config: XergonClientConfig = {}) {
    this.core = new XergonClientCore(config);
    this._openapi = new OpenAPIClient(this.core.getBaseUrl());
  }

  // ── Auth ─────────────────────────────────────────────────────────────

  /**
   * Set full keypair for HMAC authentication.
   */
  authenticate(publicKey: string, privateKey: string): void {
    this.core.authenticate(publicKey, privateKey);
  }

  /**
   * Set only the public key (for Nautilus / wallet-managed signing).
   */
  setPublicKey(pk: string): void {
    this.core.setPublicKey(pk);
  }

  /**
   * Clear all credentials.
   */
  clearAuth(): void {
    this.core.clearAuth();
  }

  getPublicKey(): string | null {
    return this.core.getPublicKey();
  }

  getBaseUrl(): string {
    return this.core.getBaseUrl();
  }

  /**
   * Add a log interceptor for request/response events.
   */
  addInterceptor(fn: LogInterceptor): void {
    this.core.addInterceptor(fn);
  }

  /**
   * Remove a log interceptor.
   */
  removeInterceptor(fn: LogInterceptor): void {
    this.core.removeInterceptor(fn);
  }

  /**
   * Verify authentication with the relay.
   */
  async authStatus(): Promise<import('./types').AuthStatus> {
    return this.core.get<import('./types').AuthStatus>('/v1/auth/status');
  }

  // ── Chat (OpenAI-compatible) ─────────────────────────────────────────

  readonly chat = {
    completions: {
      /**
       * Create a chat completion (non-streaming).
       */
      create: (params: import('./types').ChatCompletionParams, options?: { signal?: AbortSignal }) =>
        createChatCompletion(this.core, params, options),

      /**
       * Stream a chat completion via SSE.
       * Returns an AsyncIterable of ChatCompletionChunk.
       */
      stream: (params: import('./types').ChatCompletionParams, options?: { signal?: AbortSignal; sseRetry?: import('./sse-retry').SSERetryOptions | false }) =>
        streamChatCompletion(this.core, params, options),
    },
  };

  // ── Embeddings ────────────────────────────────────────────────────

  readonly embeddings = {
    /**
     * Create embeddings for one or more text inputs.
     */
    create: (request: import('./embeddings').EmbeddingRequest, options?: { signal?: AbortSignal }) =>
      createEmbedding(this.core, request, options),
  };

  // ── Audio (TTS / STT / Translation) ──────────────────────────────

  readonly audio = {
    speech: {
      /**
       * Create text-to-speech audio. Returns a Buffer of audio data.
       */
      create: (request: import('./audio').SpeechRequest, options?: { signal?: AbortSignal }) =>
        createSpeech(this.core, request, options),
    },
    transcriptions: {
      /**
       * Transcribe audio to text.
       */
      create: (request: import('./audio').TranscriptionRequest, options?: { signal?: AbortSignal }) =>
        createTranscription(this.core, request, options),
    },
    translations: {
      /**
       * Translate audio to English text.
       */
      create: (request: import('./audio').TranscriptionRequest, options?: { signal?: AbortSignal }) =>
        createTranslation(this.core, request, options),
    },
  };

  // ── Files ────────────────────────────────────────────────────────

  readonly files = {
    /**
     * Upload a file to the relay.
     */
    upload: (request: import('./upload').UploadRequest, options?: { signal?: AbortSignal }) =>
      uploadFile(this.core, request, options),

    /**
     * List all uploaded files.
     */
    list: (options?: { signal?: AbortSignal }) =>
      listFiles(this.core, options),

    /**
     * Get metadata for a specific uploaded file.
     */
    get: (fileId: string, options?: { signal?: AbortSignal }) =>
      getFile(this.core, fileId, options),

    /**
     * Delete an uploaded file.
     */
    delete: (fileId: string, options?: { signal?: AbortSignal }) =>
      deleteFile(this.core, fileId, options),

    /**
     * Download the contents of an uploaded file.
     */
    download: (fileId: string, options?: { signal?: AbortSignal }) =>
      downloadFile(this.core, fileId, options),
  };

  // ── Models ───────────────────────────────────────────────────────────

  readonly models = {
    /**
     * List all available models.
     */
    list: () => listModels(this.core),
  };

  // ── Providers ────────────────────────────────────────────────────────

  readonly providers = {
    /**
     * List all active providers.
     */
    list: () => listProviders(this.core),
  };

  /**
   * Get provider leaderboard ranked by PoNW score.
   */
  leaderboard = (params?: { limit?: number; offset?: number }) =>
    getLeaderboard(this.core, params);

  // ── Balance ──────────────────────────────────────────────────────────

  readonly balance = {
    /**
     * Get user's ERG balance from their on-chain Staking Box.
     */
    get: (userPk: string) => getBalance(this.core, userPk),
  };

  // ── GPU Bazar ────────────────────────────────────────────────────────

  readonly gpu = {
    /**
     * Browse GPU listings with optional filters.
     */
    listings: (filters?: import('./types').GpuFilters) =>
      listGpuListings(this.core, filters),

    /**
     * Get details for a specific GPU listing.
     */
    getListing: (id: string) => getGpuListing(this.core, id),

    /**
     * Rent a GPU for a given number of hours.
     */
    rent: (listingId: string, hours: number) =>
      rentGpu(this.core, listingId, hours),

    /**
     * Get a user's active rentals.
     */
    myRentals: (renterPk: string) => getMyRentals(this.core, renterPk),

    /**
     * Get GPU pricing information.
     */
    pricing: () => getGpuPricing(this.core),

    /**
     * Rate a GPU provider or renter.
     */
    rate: (params: import('./types').RateGpuParams) =>
      rateGpu(this.core, params),

    /**
     * Get reputation score for a public key.
     */
    reputation: (publicKey: string) =>
      getGpuReputation(this.core, publicKey),
  };

  // ── Incentive ────────────────────────────────────────────────────────

  readonly incentive = {
    /**
     * Get incentive system status.
     */
    status: () => getIncentiveStatus(this.core),

    /**
     * Get all rare models with bonus information.
     */
    models: () => getIncentiveModels(this.core),

    /**
     * Get detailed rarity information for a specific model.
     */
    modelDetail: (model: string) => getIncentiveModelDetail(this.core, model),
  };

  // ── Bridge ───────────────────────────────────────────────────────────

  readonly bridge = {
    /**
     * Get bridge operational status.
     */
    status: () => getBridgeStatus(this.core),

    /**
     * List all invoices for the authenticated user.
     */
    invoices: () => getBridgeInvoices(this.core),

    /**
     * Get details for a specific invoice.
     */
    getInvoice: (id: string) => getBridgeInvoice(this.core, id),

    /**
     * Create a new payment invoice.
     */
    createInvoice: (amountNanoerg: string, chain: 'btc' | 'eth' | 'ada') =>
      createBridgeInvoice(this.core, amountNanoerg, chain),

    /**
     * Confirm a payment for an invoice.
     */
    confirm: (invoiceId: string, txHash: string) =>
      confirmBridgePayment(this.core, invoiceId, txHash),

    /**
     * Request a refund for an invoice.
     */
    refund: (invoiceId: string) =>
      refundBridgeInvoice(this.core, invoiceId),
  };

  // ── Contracts (Agent-mediated) ──────────────────────────────────────

  readonly contracts = {
    /**
     * Register a new provider on-chain.
     * Builds and broadcasts a tx to create a Provider Box with NFT + metadata.
     */
    registerProvider: (params: import('./types/contracts').RegisterProviderApiParams) =>
      apiRegisterProvider(this.core, params),

    /**
     * Query provider status by NFT token ID.
     * Returns decoded register values (name, endpoint, pricing).
     */
    queryProviderStatus: (providerNftId: string) =>
      apiQueryProviderStatus(this.core, providerNftId),

    /**
     * List all on-chain providers by scanning the UTXO set.
     */
    listOnChainProviders: () => apiListOnChainProviders(this.core),

    /**
     * Create a new User Staking Box.
     * Locks ERG in a staking contract for inference payments.
     */
    createStakingBox: (params: import('./types/contracts').CreateStakingBoxApiParams) =>
      apiCreateStakingBox(this.core, params),

    /**
     * Query a user's total ERG balance across all staking boxes.
     */
    queryUserBalance: (userPkHex: string) =>
      apiQueryUserBalance(this.core, userPkHex),

    /**
     * Get all staking boxes for a given user.
     */
    getUserStakingBoxes: (userPkHex: string) =>
      apiGetUserStakingBoxes(this.core, userPkHex),

    /**
     * Get the current ERG/USD rate from the oracle pool.
     */
    getOracleRate: () => apiGetOracleRate(this.core),

    /**
     * Get detailed oracle pool status (epoch, box ID, update height).
     */
    getOraclePoolStatus: () => apiGetOraclePoolStatus(this.core),

    /**
     * Get staking boxes ready for settlement.
     */
    getSettleableBoxes: (maxBoxes?: number) =>
      apiGetSettleableBoxes(this.core, maxBoxes),

    /**
     * Build a settlement transaction for a provider to claim fees.
     */
    buildSettlementTx: (params: import('./types/contracts').BuildSettlementApiParams) =>
      apiBuildSettlementTx(this.core, params),
  };

  // ── Health ───────────────────────────────────────────────────────────

  readonly health = {
    /**
     * Liveness probe -- is the relay process running?
     */
    check: () => healthCheck(this.core),
  };

  readonly ready = {
    /**
     * Readiness probe -- can the relay serve requests?
     */
    check: () => readyCheck(this.core),
  };

  // ── WebSocket ───────────────────────────────────────────────────────

  /**
   * Access the WebSocket client instance (null if not yet connected).
   */
  get ws(): XergonWebSocketClient | null {
    return this._ws;
  }

  /**
   * Create and connect a WebSocket client to the relay's /v1/chat/ws endpoint.
   * Shares the same base URL. Auth token can be passed via options.
   */
  connectWebSocket(options?: WSOptions): Promise<XergonWebSocketClient> {
    if (this._ws) {
      return this._ws.connect().then(() => this._ws!);
    }

    const baseUrl = this.core.getBaseUrl();
    const wsUrl = baseUrl.replace(/^http/, 'ws') + '/v1/chat/ws';

    this._ws = new XergonWebSocketClient(wsUrl, options);
    return this._ws.connect().then(() => this._ws!);
  }

  /**
   * Disconnect and clean up the WebSocket client.
   */
  disconnectWebSocket(): void {
    if (this._ws) {
      this._ws.disconnect();
      this._ws.removeAllListeners();
      this._ws = null;
    }
  }

  // ── OpenAPI ─────────────────────────────────────────────────────────

  /**
   * Access the OpenAPI client for spec introspection.
   */
  get openapi(): OpenAPIClient {
    return this._openapi;
  }

  // ── Batch Requests ─────────────────────────────────────────────────

  /**
   * Access the BatchClient for executing batch HTTP requests.
   * Lazily created on first access.
   */
  get batch(): BatchClient {
    if (!this._batch) {
      this._batch = new BatchClient(this.core.getBaseUrl());
    }
    return this._batch;
  }

  // ── Request Queue ──────────────────────────────────────────────────

  /**
   * Access the RequestQueue for request deduplication and debouncing.
   * Lazily created on first access.
   */
  get queue(): RequestQueue {
    if (!this._queue) {
      this._queue = new RequestQueue();
    }
    return this._queue;
  }

  // ── Batch Chat Helper ──────────────────────────────────────────────

  /**
   * Access the BatchChatHelper for multi-model, multi-prompt, and
   * consensus chat patterns. Lazily created on first access.
   */
  get batchChat(): BatchChatHelper {
    if (!this._batchChat) {
      this._batchChat = new BatchChatHelper(this);
    }
    return this._batchChat;
  }

  // ── Retry ──────────────────────────────────────────────────────────

  /**
   * Access the RetryClient for programmatic retry with configurable
   * backoff strategies. Lazily created on first access.
   */
  get retryClient(): RetryClient {
    if (!this._retry) {
      this._retry = new RetryClient();
    }
    return this._retry;
  }

  // ── Cancellation ───────────────────────────────────────────────────

  /**
   * Access the CancellationManager for creating and managing
   * cancellation tokens. Lazily created on first access.
   */
  get cancellation(): CancellationManager {
    if (!this._cancellation) {
      this._cancellation = new CancellationManager();
    }
    return this._cancellation;
  }

  // ── Resilient HTTP Client ──────────────────────────────────────────

  /**
   * Access a ResilientHttpClient that combines retry, cancellation,
   * and timeout for HTTP requests. Lazily created on first access.
   */
  get resilient(): ResilientHttpClient {
    if (!this._resilientClient) {
      this._resilientClient = new ResilientHttpClient(this.core.getBaseUrl());
    }
    return this._resilientClient;
  }
}
