/**
 * React hooks for the Xergon SDK.
 *
 * @example
 * ```tsx
 * import { useChat, useModels, useProvider } from '@xergon/sdk/hooks';
 *
 * function MyComponent() {
 *   const { messages, send, isLoading, stop } = useChat({
 *     model: 'llama-3.3-70b',
 *     apiKey: '...',
 *   });
 *   const { models } = useModels();
 *   const { status, latency } = useProvider();
 *
 *   // ...
 * }
 * ```
 */

export { useChat } from './use-chat';
export type { UseChatOptions, ChatMessage, TokenUsage } from './use-chat';

export { useModels } from './use-models';
export type { UseModelsOptions } from './use-models';

export { useProvider } from './use-provider';
export type { UseProviderOptions, ProviderStatus, ProviderInfo } from './use-provider';
