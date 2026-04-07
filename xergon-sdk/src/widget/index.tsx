/**
 * Chat widget components for embedding Xergon chat in any web app.
 *
 * @example
 * ```tsx
 * import { ChatWidget } from '@xergon/sdk/widget';
 *
 * function App() {
 *   return (
 *     <ChatWidget
 *       apiKey="..."
 *       theme="dark"
 *       defaultModel="llama-3.3-70b"
 *     />
 *   );
 * }
 * ```
 */

export { ChatWidget } from './chat-widget';
export type { ChatWidgetProps } from './chat-widget';

export { ChatMessageComponent } from './chat-message';
export type { ChatMessageProps } from './chat-message';

export { ChatInput } from './chat-input';
export type { ChatInputProps } from './chat-input';

export { ModelSelector } from './model-selector';
export type { ModelSelectorProps } from './model-selector';
