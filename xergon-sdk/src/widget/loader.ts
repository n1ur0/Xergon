/**
 * Widget loader -- for embedding the chat widget without React/TypeScript.
 *
 * @example
 * ```html
 * <script src="https://cdn.xergon.gg/widget.js"></script>
 * <script>
 *   XergonChat.init({ apiKey: '...', theme: 'dark' });
 * </script>
 * ```
 */

import type { ChatWidgetProps } from './chat-widget';

/**
 * Mount a ChatWidget into the DOM using Shadow DOM for style isolation.
 */
export function initWidget(config: ChatWidgetProps): void {
  // Validate DOM availability
  if (typeof document === 'undefined') {
    console.error('[xergon-sdk] initWidget() must be called in a browser environment.');
    return;
  }

  // Dynamically import React and ReactDOM -- this loader is meant for
  // non-bundled environments where the user loads the SDK via script tag.
  // In practice, the widget bundle (built separately) will bundle React.
  const container = document.createElement('div');
  container.id = 'xergon-chat-widget-root';
  document.body.appendChild(container);

  // For the script-loader use case, we create a global mount function
  // that the pre-built widget bundle will call.
  // This function is called by the UMD bundle after React is loaded.
  const global = globalThis as Record<string, unknown>;
  (global as Record<string, unknown>).__xergonWidgetConfig = config;
  (global as Record<string, unknown>).__xergonWidgetRoot = container;
}

/**
 * Unmount and remove the chat widget from the DOM.
 */
export function destroyWidget(): void {
  const root = document.getElementById('xergon-chat-widget-root');
  if (root) {
    root.remove();
  }
  const global = globalThis as Record<string, unknown>;
  delete (global as Record<string, unknown>).__xergonWidgetConfig;
  delete (global as Record<string, unknown>).__xergonWidgetRoot;
}

/**
 * Update the widget configuration at runtime.
 */
export function updateWidgetConfig(config: Partial<ChatWidgetProps>): void {
  const global = globalThis as Record<string, unknown>;
  const existing = (global as Record<string, unknown>).__xergonWidgetConfig as ChatWidgetProps | undefined;
  if (existing) {
    (global as Record<string, unknown>).__xergonWidgetConfig = { ...existing, ...config };
  }
}

// Expose on global scope for script-tag usage
if (typeof globalThis !== 'undefined') {
  const g = globalThis as Record<string, unknown>;
  g.XergonChat = {
    init: initWidget,
    destroy: destroyWidget,
    update: updateWidgetConfig,
  };
}
