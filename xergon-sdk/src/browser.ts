/**
 * Xergon SDK - Browser Entry Point
 * 
 * This is a browser-compatible version of the SDK that excludes
 * Node.js-only modules (debug, gateway, docs-generator, etc.)
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

export { listTemplates, getTemplate, renderTemplate, renderTemplateRaw, addTemplate, removeTemplate } from './prompt-templates';
export type { PromptTemplate, RenderedPrompt } from './prompt-templates';

export { pipeOutput, formatOutput, copyToClipboard, appendToFile, pipeToCommand, parsePipeString } from './output-pipe';
export type { OutputFormat, PipeDestination, PipeConfig } from './output-pipe';

export { resolveAlias, resolveModelName, listAliases, addAlias, removeAlias, getAlias } from './model-alias';
export type { ModelAlias } from './model-alias';

export { deriveAddress, derivePublicKey, generateKeypair, signMessage as signMessageOffline, verifySignature } from './wallet/offline';

export { CONTRACTS, getContract } from './contracts';
export type { ContractName, CompiledContracts } from './types/contracts';

export { buildProviderRegistrationTx, buildStakingTx, buildSettlementTx, decodeSIntLong, decodeSIntInt, ergoTreeToAddress } from './ergo-tx';

export { getOracleRate } from './oracle-client';
export type { OracleResult, ProviderRegistrationParams, StakingParams, SettlementParams } from './types/contracts';

export { createConversation, addMessage, getConversation, listConversations, deleteConversation, setActive, getActive, getMessagesForContext, searchConversations, exportConversation, importConversation } from './conversation';
export type { Message, Conversation, ConversationStore } from './conversation';

export { createFlow, runFlow, runFlowParallel, listBuiltInFlows, getBuiltInFlow } from './flow';
export type { FlowStep, Flow, FlowResult, FlowExecutor } from './flow';

export { runBenchmark, listBenchmarks, compareBenchmarks, exportResults as exportEvalResults, saveToHistory as saveEvalToHistory, loadHistory as loadEvalHistory } from './eval';
export type { EvalBenchmark, EvalResult, RunBenchmarkOptions, CompareResult } from './eval';

export { startCanary, checkCanary, promoteCanary, rollbackCanary, listCanaries, recordCanaryRequest, loadCanaryHistory, saveCanaryToHistory } from './canary';
export type { CanaryConfig, CanaryMetrics, CanaryDeployment, CanaryCheckResult, CanaryHistoryEntry } from './canary';

export { exportData, importData, validateExport, listExportScopes, getExportSize } from './export';
export { ExportFormat } from './export';
export type { ExportConfig, ExportManifest, ExportResult, ScopeInfo } from './export';

export { TeamClient } from './team';
export { createTeam, getTeam, listTeams, updateTeam, deleteTeam, inviteMember, acceptInvite, removeMember, updateRole, getTeamActivity, getTeamUsage, transferOwnership } from './team';
export type { Team, TeamMember, TeamSettings, TeamInvite, TeamActivity, TeamUsage, TeamRole, NotificationLevel, CreateTeamParams, UpdateTeamParams } from './team';

export { WebhookClient, SUPPORTED_WEBHOOK_EVENTS } from './webhook';
export { createWebhook, listWebhooks, getWebhook, updateWebhook, deleteWebhook, testWebhook, getDeliveries, replayDelivery, getSupportedEvents } from './webhook';
export type { Webhook, WebhookEvent, WebhookDelivery, CreateWebhookParams, UpdateWebhookParams } from './webhook';

export { setLevel, getLevel, debug, info, warn, error, getHistory, exportLogs, clearHistory } from './log';
export { LogLevel } from './log';
export type { LogEntry, LogConfig } from './log';

export { listModels as registryListModels, getModel, searchModels, getModelVersions, compareModels, getRecommended, getPopularModels, subscribeModel, getDeprecationNotice, getModelLineage, notifyModelChange, setPopularityScore, registerVersion, registerLineage, clearRegistryCache } from './model-registry';
export type { ModelInfo, ModelVersion, ModelFilter, SortOption, PaginationOptions, ModelComparison, ModelRecommendation, LineageNode } from './model-registry';

export { PluginMarketplace } from './plugins/plugin-marketplace';
export { searchPlugins, getPlugin, installPlugin, uninstallPlugin, updatePlugin, publishPlugin, listInstalledPlugins, getPluginReviews, ratePlugin, getCategories, getPopularPlugins, getFeaturedPlugins } from './plugins/plugin-marketplace';
export type { MarketplacePluginManifest, MarketplacePlugin, PluginReview, PluginSortField, SearchOptions } from './plugins/plugin-marketplace';
