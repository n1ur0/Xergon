/**
 * Interactive REPL for the Xergon CLI.
 *
 * Provides a multi-line chat interface with:
 * - Model fetching on startup with selection display
 * - Streaming and non-streaming output modes
 * - Simple terminal markdown rendering (bold, italic, code blocks)
 * - Multi-line input via backslash continuation
 * - Context window (last 10 messages sent to the API)
 * - Command history (up/down arrows)
 * - Special /commands for configuration
 * - Token counting and latency display
 * - Multi-turn conversation memory with /history
 */

import type { CLIConfig, OutputFormatter } from './mod';
import type { Model } from '../types';
import {
  createConversation,
  addMessage,
  getConversation,
  getActive,
  setActive,
  exportConversation,
  type Conversation,
} from '../conversation';

export interface REPLState {
  client: any;
  config: CLIConfig;
  output: OutputFormatter;
  model: string;
  systemPrompt: string | undefined;
  temperature: number | undefined;
  maxTokens: number | undefined;
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
  history: string[];
  historyIndex: number;
  running: boolean;
  stream: boolean;
  availableModels: Model[];
  conversationId?: string;
  conversationTitle?: string;
}

export interface REPLConfig {
  client: any;
  config: CLIConfig;
  output: OutputFormatter;
  model?: string;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
  stream?: boolean;
  conversationId?: string;
  newConversation?: boolean;
  conversationTitle?: string;
}

/** Maximum number of messages (user+assistant) to send in the context window */
const CONTEXT_WINDOW_SIZE = 10;

/**
 * Start an interactive REPL session.
 */
export async function startRepl(replConfig: REPLConfig): Promise<void> {
  const state: REPLState = {
    client: replConfig.client,
    config: replConfig.config,
    output: replConfig.output,
    model: replConfig.model || replConfig.config.defaultModel || 'llama-3.3-70b',
    systemPrompt: replConfig.systemPrompt,
    temperature: replConfig.temperature,
    maxTokens: replConfig.maxTokens,
    messages: [],
    history: [],
    historyIndex: -1,
    running: true,
    stream: replConfig.stream !== false,
    availableModels: [],
    conversationId: replConfig.conversationId,
    conversationTitle: replConfig.conversationTitle,
  };

  // Handle conversation flags
  if (replConfig.newConversation) {
    // Start a new conversation
    const title = replConfig.conversationTitle || `Chat ${new Date().toLocaleString()}`;
    const conv = createConversation(title, state.model, state.systemPrompt);
    state.conversationId = conv.id;
    state.conversationTitle = conv.title;
  } else if (replConfig.conversationId) {
    // Continue existing conversation
    const conv = getConversation(replConfig.conversationId);
    if (conv) {
      state.conversationId = conv.id;
      state.conversationTitle = conv.title;
      state.systemPrompt = conv.systemPrompt || state.systemPrompt;
      state.model = conv.model || state.model;
      // Load existing messages
      state.messages = conv.messages.map(m => ({
        role: m.role as 'system' | 'user' | 'assistant',
        content: m.content,
      }));
    }
  } else {
    // Auto-create conversation in REPL mode
    const title = replConfig.conversationTitle || `Chat ${new Date().toLocaleString()}`;
    const conv = createConversation(title, state.model, state.systemPrompt);
    state.conversationId = conv.id;
    state.conversationTitle = conv.title;
  }

  // Set up system prompt if provided and not already in messages
  if (state.systemPrompt && !state.messages.some(m => m.role === 'system')) {
    state.messages.push({ role: 'system', content: state.systemPrompt });
  }

  const output = state.output;

  // Fetch available models on startup
  try {
    const models: Model[] = await state.client.models.list();
    state.availableModels = models;

    output.write(output.colorize('\n  Xergon Interactive Chat', 'bold') + '\n');
    output.write(output.colorize('  ─────────────────────────', 'dim') + '\n');

    // Find the selected model
    const selectedModel = models.find(m => m.id === state.model);
    if (selectedModel) {
      output.write(`  Model: ${output.colorize(state.model, 'cyan')}`);
      if (selectedModel.pricing) {
        output.write(output.colorize(`  (${selectedModel.pricing})`, 'dim'));
      }
      output.write('\n');
    } else {
      output.write(`  Model: ${output.colorize(state.model, 'yellow')} (not found in available models)\n`);
    }

    if (state.conversationId) {
      output.write(`  Conversation: ${output.colorize(state.conversationTitle || state.conversationId, 'green')} (${state.conversationId})\n`);
    }
    output.write(`  ${output.colorize(`${models.length}`, 'cyan')} model(s) available\n`);
    output.write('  Type ' + output.colorize('/help', 'yellow') + ' for commands, ' +
      output.colorize('/quit', 'yellow') + ' to exit');
    output.write('\n  Type ' + output.colorize('\\', 'yellow') + ' at end of line for multi-line input');
    output.write('\n\n');
  } catch (err) {
    output.write(output.colorize('\n  Xergon Interactive Chat', 'bold') + '\n');
    output.write(output.colorize('  ─────────────────────────', 'dim') + '\n');
    output.write(`  Model: ${output.colorize(state.model, 'cyan')}\n`);
    if (state.conversationId) {
      output.write(`  Conversation: ${output.colorize(state.conversationTitle || state.conversationId, 'green')} (${state.conversationId})\n`);
    }
    output.write(output.colorize('  ⚠ Could not fetch available models', 'yellow') + '\n');
    output.write('  Type ' + output.colorize('/help', 'yellow') + ' for commands, ' +
      output.colorize('/quit', 'yellow') + ' to exit\n\n');
  }

  // In a real terminal, we'd use readline. For testability and portability,
  // we expose processInput which can be driven programmatically.
  if (typeof process !== 'undefined' && process.stdin.isTTY) {
    await runTerminalREPL(state);
  }
}

/**
 * Get the messages to send to the API, respecting the context window.
 * Always includes system message + last N user/assistant messages.
 */
function getContextMessages(state: REPLState): Array<{ role: 'system' | 'user' | 'assistant'; content: string }> {
  const systemMsgs = state.messages.filter(m => m.role === 'system');
  const conversationMsgs = state.messages.filter(m => m.role !== 'system');

  // Keep only the last CONTEXT_WINDOW_SIZE messages
  const recentMsgs = conversationMsgs.slice(-CONTEXT_WINDOW_SIZE);

  return [...systemMsgs, ...recentMsgs];
}

/**
 * Process a single input line in the REPL.
 * Returns true if the REPL should continue, false if it should exit.
 */
export async function processInput(state: REPLState, input: string): Promise<boolean> {
  const trimmed = input.trim();

  if (!trimmed) return true;

  // Add to history
  state.history.push(trimmed);
  state.historyIndex = state.history.length;

  // Handle special commands
  if (trimmed.startsWith('/')) {
    return await handleCommand(state, trimmed);
  }

  // Regular chat message
  state.messages.push({ role: 'user', content: trimmed });

  // Save to conversation store
  if (state.conversationId) {
    try {
      addMessage(state.conversationId, { role: 'user', content: trimmed });
    } catch {
      // Silently ignore
    }
  }

  const output = state.output;
  output.write(output.colorize('\nYou: ', 'green') + trimmed + '\n');

  try {
    const startTime = Date.now();
    const contextMessages = getContextMessages(state);

    if (state.stream) {
      // ── Streaming mode ──
      const stream = await state.client.chat.completions.stream({
        model: state.model,
        messages: contextMessages,
        temperature: state.temperature,
        maxTokens: state.maxTokens,
      });

      output.write(output.colorize('Assistant: ', 'blue'));

      let fullResponse = '';
      for await (const chunk of stream) {
        const delta = chunk.choices?.[0]?.delta?.content;
        if (delta) {
          fullResponse += delta;
          process.stdout.write(delta);
        }
      }

      const elapsed = Date.now() - startTime;
      process.stdout.write('\n');

      state.messages.push({ role: 'assistant', content: fullResponse });

      // Save assistant response to conversation
      if (state.conversationId) {
        try {
          addMessage(state.conversationId, { role: 'assistant', content: fullResponse });
        } catch {
          // Silently ignore
        }
      }

      output.write(output.colorize(
        `  [${elapsed}ms, ${fullResponse.split(/\s+/).filter(Boolean).length} words, streaming]\n`,
        'dim'
      ));
    } else {
      // ── Non-streaming mode with spinner ──
      output.write(output.colorize('Thinking', 'blue') + ' ');
      let dots = 0;
      const spinnerInterval = setInterval(() => {
        dots = (dots + 1) % 4;
        process.stdout.write('\r' + output.colorize('Thinking', 'blue') + '.'.repeat(dots).padEnd(3) + '   ');
      }, 300);

      try {
        const response = await state.client.chat.completions.create({
          model: state.model,
          messages: contextMessages,
          temperature: state.temperature,
          maxTokens: state.maxTokens,
        });

        clearInterval(spinnerInterval);
        // Clear the spinner line
        process.stdout.write('\r' + ' '.repeat(20) + '\r');

        const elapsed = Date.now() - startTime;
        const content = response.choices?.[0]?.message?.content || '(no content)';

        output.write(output.colorize('Assistant: ', 'blue') + renderMarkdown(content, output) + '\n');

        state.messages.push({ role: 'assistant', content });

        // Save assistant response to conversation
        if (state.conversationId) {
          try {
            addMessage(state.conversationId, { role: 'assistant', content });
          } catch {
            // Silently ignore
          }
        }

        output.write(output.colorize(
          `  [${elapsed}ms, ${content.split(/\s+/).filter(Boolean).length} words]\n`,
          'dim'
        ));

        if (response.usage) {
          output.write(output.colorize(
            `  Tokens: ${response.usage.promptTokens} prompt + ${response.usage.completionTokens} completion = ${response.usage.totalTokens} total\n`,
            'dim'
          ));
        }
      } catch (err) {
        clearInterval(spinnerInterval);
        process.stdout.write('\r' + ' '.repeat(20) + '\r');
        throw err;
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    output.writeError(`Failed: ${message}`);
    // Remove the failed user message
    state.messages.pop();
  }

  return true;
}

/**
 * Render simple markdown to terminal-friendly output.
 * Supports: **bold**, *italic*, `code`, ```code blocks```, > blockquotes.
 */
function renderMarkdown(text: string, output: OutputFormatter): string {
  let result = text;

  // Code blocks: ```...```
  result = result.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, _lang, code) => {
    const lines = code.trimEnd().split('\n');
    const indented = lines.map((l: string) => '    ' + l).join('\n');
    return output.colorize(indented, 'dim');
  });

  // Inline code: `...`
  result = result.replace(/`([^`]+)`/g, (_match, code: string) => {
    return output.colorize(` ${code} `, 'dim');
  });

  // Bold: **...**
  result = result.replace(/\*\*([^*]+)\*\*/g, (_match, content: string) => {
    return output.colorize(content, 'bold');
  });

  // Italic: *...* (but not within **...**)
  result = result.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, (_match, content: string) => {
    return output.colorize(content, 'dim');
  });

  // Blockquotes: > ...
  result = result.replace(/^>\s?(.*)$/gm, (_match, content: string) => {
    return output.colorize(`  │ ${content}`, 'dim');
  });

  // Headers: ## ..., ### ...
  result = result.replace(/^###\s+(.*)$/gm, (_match, content: string) => {
    return '\n' + output.colorize(content, 'bold') + '\n';
  });
  result = result.replace(/^##\s+(.*)$/gm, (_match, content: string) => {
    return '\n' + output.colorize(content, 'bold') + '\n' + output.colorize('─'.repeat(content.length), 'dim') + '\n';
  });
  result = result.replace(/^#\s+(.*)$/gm, (_match, content: string) => {
    return '\n' + output.colorize(content, 'bold') + '\n' + output.colorize('═'.repeat(content.length), 'dim') + '\n';
  });

  // Unordered lists: - ...
  result = result.replace(/^[-*]\s+(.*)$/gm, (_match, content: string) => {
    return `  • ${content}`;
  });

  // Ordered lists: 1. ...
  result = result.replace(/^\d+\.\s+(.*)$/gm, (_match, content: string) => {
    return `  ${content}`;
  });

  return result;
}

/**
 * Handle a /command in the REPL.
 */
async function handleCommand(state: REPLState, input: string): Promise<boolean> {
  const parts = input.split(/\s+/);
  const cmd = parts[0].toLowerCase();
  const args = parts.slice(1);
  const output = state.output;

  switch (cmd) {
    case '/quit':
    case '/exit':
    case '/q':
      output.write(output.colorize('Goodbye!\n', 'dim'));
      return false;

    case '/help':
    case '/h':
    case '/?':
      output.write(output.colorize('\nREPL Commands:\n', 'bold'));
      output.write('  /model <name>        Switch model\n');
      output.write('  /models              List available models\n');
      output.write('  /system <prompt>     Set system prompt\n');
      output.write('  /temperature <value> Set temperature\n');
      output.write('  /max-tokens <n>      Set max tokens\n');
      output.write('  /stream [on|off]     Toggle streaming mode\n');
      output.write('  /clear               Clear conversation history\n');
      output.write('  /history             Show conversation history\n');
      output.write('  /conversations       List saved conversations\n');
      output.write('  /switch <id>         Switch to a different conversation\n');
      output.write('  /export <file>       Export conversation to file\n');
      output.write('  /config              Show current settings\n');
      output.write('  /help                Show this help\n');
      output.write('  /quit                Exit REPL\n\n');
      output.write(output.colorize('Input:', 'dim') + ' Type ' +
        output.colorize('\\', 'yellow') + ' at end of line for multi-line input\n\n');
      return true;

    case '/model':
      if (!args[0]) {
        output.writeError('Usage: /model <model-name>');
        output.info(`Current model: ${state.model}`);
        return true;
      }
      state.model = args[0];
      output.success(`Model set to: ${state.model}`);
      return true;

    case '/models':
      if (state.availableModels.length === 0) {
        try {
          const models: Model[] = await state.client.models.list();
          state.availableModels = models;
        } catch (err) {
          output.writeError(`Failed to fetch models: ${err instanceof Error ? err.message : String(err)}`);
          return true;
        }
      }
      output.write(output.colorize(`\nAvailable Models (${state.availableModels.length}):\n`, 'bold'));
      output.write(output.colorize('─'.repeat(50) + '\n', 'dim'));
      for (const m of state.availableModels) {
        const isSelected = m.id === state.model;
        const marker = isSelected ? output.colorize(' ◄', 'green') : '';
        const pricing = m.pricing ? output.colorize(`  ${m.pricing}`, 'dim') : '';
        output.write(`  ${m.id}${marker}${pricing}\n`);
      }
      output.write(output.colorize('─'.repeat(50) + '\n', 'dim'));
      output.write(`  Use ${output.colorize('/model <name>', 'yellow')} to switch\n\n`);
      return true;

    case '/system':
      if (!args.length) {
        output.writeError('Usage: /system <prompt>');
        if (state.systemPrompt) {
          output.info(`Current system prompt: ${state.systemPrompt.substring(0, 80)}...`);
        } else {
          output.info('No system prompt set.');
        }
        return true;
      }
      state.systemPrompt = args.join(' ');
      // Update messages: replace or add system message at position 0
      const sysIdx = state.messages.findIndex(m => m.role === 'system');
      if (sysIdx !== -1) {
        state.messages[sysIdx] = { role: 'system', content: state.systemPrompt };
      } else {
        state.messages.unshift({ role: 'system', content: state.systemPrompt });
      }
      output.success('System prompt updated.');
      return true;

    case '/temperature':
      if (!args[0]) {
        output.writeError('Usage: /temperature <value>');
        output.info(`Current temperature: ${state.temperature ?? 'default'}`);
        return true;
      }
      const temp = Number(args[0]);
      if (isNaN(temp) || temp < 0 || temp > 2) {
        output.writeError('Temperature must be a number between 0 and 2.');
        return true;
      }
      state.temperature = temp;
      output.success(`Temperature set to: ${temp}`);
      return true;

    case '/max-tokens':
      if (!args[0]) {
        output.writeError('Usage: /max-tokens <n>');
        output.info(`Current max tokens: ${state.maxTokens ?? 'default'}`);
        return true;
      }
      const maxTok = Number(args[0]);
      if (isNaN(maxTok) || maxTok < 1) {
        output.writeError('Max tokens must be a positive number.');
        return true;
      }
      state.maxTokens = maxTok;
      output.success(`Max tokens set to: ${maxTok}`);
      return true;

    case '/stream':
      if (args[0] === 'off' || args[0] === 'false') {
        state.stream = false;
        output.success('Streaming disabled. Responses will show with a loading indicator.');
      } else if (args[0] === 'on' || args[0] === 'true') {
        state.stream = true;
        output.success('Streaming enabled.');
      } else {
        state.stream = !state.stream;
        output.success(`Streaming ${state.stream ? 'enabled' : 'disabled'}.`);
      }
      return true;

    case '/clear':
      const sysMsg = state.messages.find(m => m.role === 'system');
      state.messages = sysMsg ? [sysMsg] : [];
      output.success('Conversation history cleared.');
      return true;

    case '/history':
      if (state.messages.length === 0) {
        output.info('No conversation history.');
        return true;
      }
      output.write(output.colorize('\nConversation History:\n', 'bold'));
      output.write(output.colorize('─'.repeat(40) + '\n', 'dim'));
      for (const msg of state.messages) {
        const role = output.colorize(
          msg.role.charAt(0).toUpperCase() + msg.role.slice(1),
          msg.role === 'user' ? 'green' : msg.role === 'assistant' ? 'blue' : 'cyan'
        );
        const content = msg.content.length > 100
          ? msg.content.substring(0, 100) + '...'
          : msg.content;
        output.write(`  ${role}: ${content}\n`);
      }
      output.write(output.colorize('─'.repeat(40) + '\n', 'dim'));
      output.write(`  ${state.messages.length} message(s)`);
      output.write(output.colorize(`  (context window: last ${CONTEXT_WINDOW_SIZE} sent to API)\n`, 'dim'));
      output.write('\n');
      return true;

    case '/conversations': {
      try {
        const { listConversations } = await import('../conversation');
        const convs = listConversations();
        if (convs.length === 0) {
          output.info('No saved conversations.');
          return true;
        }
        output.write(output.colorize('\nSaved Conversations:\n', 'bold'));
        output.write(output.colorize('─'.repeat(60) + '\n', 'dim'));
        for (const conv of convs) {
          const isActive = conv.isActive ? output.colorize(' ◄ active', 'green') : '';
          const date = new Date(conv.updatedAt).toLocaleDateString();
          output.write(`  ${output.colorize(conv.id, 'cyan')}  ${conv.title}  (${conv.messageCount} msgs, ${date})${isActive}\n`);
        }
        output.write(output.colorize('─'.repeat(60) + '\n', 'dim'));
        output.write(`  Use ${output.colorize('/switch <id>', 'yellow')} to switch\n\n`);
      } catch (err) {
        output.writeError(`Failed to list conversations: ${err instanceof Error ? err.message : String(err)}`);
      }
      return true;
    }

    case '/switch': {
      if (!args[0]) {
        output.writeError('Usage: /switch <conversation-id>');
        if (state.conversationId) {
          output.info(`Current: ${state.conversationId} (${state.conversationTitle})`);
        }
        return true;
      }
      try {
        const conv = getConversation(args[0]);
        if (!conv) {
          output.writeError(`Conversation not found: ${args[0]}`);
          return true;
        }
        state.conversationId = conv.id;
        state.conversationTitle = conv.title;
        state.messages = conv.messages.map(m => ({
          role: m.role as 'system' | 'user' | 'assistant',
          content: m.content,
        }));
        setActive(conv.id);
        output.success(`Switched to conversation: ${conv.title} (${conv.id})`);
        output.info(`  ${conv.messages.length} message(s) loaded`);
      } catch (err) {
        output.writeError(`Failed to switch: ${err instanceof Error ? err.message : String(err)}`);
      }
      return true;
    }

    case '/export': {
      if (!args[0]) {
        output.writeError('Usage: /export <filename>');
        return true;
      }
      const filename = args[0];
      try {
        const { writeFileSync } = await import('node:fs');

        if (state.conversationId && filename.endsWith('.md')) {
          // Export as markdown using conversation module
          const markdown = exportConversation(state.conversationId);
          writeFileSync(filename, markdown);
        } else {
          const exportData = {
            model: state.model,
            exportedAt: new Date().toISOString(),
            messages: state.messages,
          };
          writeFileSync(filename, JSON.stringify(exportData, null, 2));
        }
        output.success(`Conversation exported to: ${filename}`);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        output.writeError(`Export failed: ${msg}`);
      }
      return true;
    }

    case '/config':
      const configData = {
        model: state.model,
        streaming: state.stream,
        systemPrompt: state.systemPrompt ? `${state.systemPrompt.substring(0, 60)}...` : '(none)',
        temperature: state.temperature ?? 'default',
        maxTokens: state.maxTokens ?? 'default',
        messages: state.messages.length,
        contextWindow: CONTEXT_WINDOW_SIZE,
        conversation: state.conversationId ? `${state.conversationTitle} (${state.conversationId})` : '(none)',
      };
      output.write(output.formatText(configData, 'Current REPL Settings'));
      return true;

    default:
      output.writeError(`Unknown command: ${cmd}`);
      output.info('Type /help for available commands.');
      return true;
  }
}

/**
 * Run the REPL with terminal readline interface.
 * Supports multi-line input via backslash continuation.
 */
async function runTerminalREPL(state: REPLState): Promise<void> {
  // Dynamic import to avoid requiring readline in non-terminal contexts
  const { createInterface } = await import('node:readline');
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: state.output.colorize('> ', 'green'),
  });

  rl.prompt();

  for await (const line of rl) {
    // Handle multi-line input via backslash continuation
    if (line.endsWith('\\')) {
      let fullLine = line.slice(0, -1) + '\n';
      const contPrompt = state.output.colorize('  ... ', 'dim');

      const contLines: string[] = [];
      let current = line;

      while (current.endsWith('\\')) {
        fullLine = current.slice(0, -1) + '\n';
        process.stdout.write(contPrompt);
        const contLine = await new Promise<string>((resolve) => {
          rl.once('line', resolve);
          process.stdout.write(contPrompt);
        });
        fullLine += contLine + '\n';
        current = contLine;
      }
      fullLine = fullLine.trimEnd();

      const shouldContinue = await processInput(state, fullLine);
      if (!shouldContinue) {
        rl.close();
        break;
      }
      rl.prompt();
    } else {
      const shouldContinue = await processInput(state, line);
      if (!shouldContinue) {
        rl.close();
        break;
      }
      rl.prompt();
    }
  }
}
