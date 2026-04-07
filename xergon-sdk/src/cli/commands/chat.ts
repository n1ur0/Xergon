/**
 * CLI command: chat
 *
 * Send chat completions from the command line.
 * Supports one-shot queries, interactive REPL mode,
 * and multi-turn conversation memory.
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import { parsePipeString, pipeOutput } from '../../output-pipe';
import type { OutputFormat } from '../../output-pipe';

/**
 * Read all of stdin as a string. Resolves when stdin closes.
 */
function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    if (process.stdin.isTTY) {
      resolve('');
      return;
    }

    const chunks: string[] = [];
    process.stdin.setEncoding('utf-8');

    process.stdin.on('data', (chunk: string) => {
      chunks.push(chunk);
    });

    process.stdin.on('end', () => {
      resolve(chunks.join(''));
    });

    process.stdin.on('error', (err: Error) => {
      reject(err);
    });

    // If stdin is already ended (e.g. empty pipe), resolve immediately
    if (process.stdin.readableEnded) {
      resolve(chunks.join(''));
    }
  });
}

const chatOptions: CommandOption[] = [
  {
    name: 'model',
    short: '-m',
    long: '--model',
    description: 'Model to use for completion',
    required: false,
    type: 'string',
  },
  {
    name: 'relay',
    short: '',
    long: '--relay',
    description: 'Relay URL (overrides config)',
    required: false,
    type: 'string',
  },
  {
    name: 'apiKey',
    short: '',
    long: '--api-key',
    description: 'API key (overrides config)',
    required: false,
    type: 'string',
  },
  {
    name: 'system',
    short: '-s',
    long: '--system',
    description: 'System prompt',
    required: false,
    type: 'string',
  },
  {
    name: 'temperature',
    short: '-t',
    long: '--temperature',
    description: 'Sampling temperature (0.0 - 2.0)',
    required: false,
    type: 'number',
  },
  {
    name: 'maxTokens',
    short: '-n',
    long: '--max-tokens',
    description: 'Maximum tokens in response',
    required: false,
    type: 'number',
  },
  {
    name: 'stream',
    short: '',
    long: '--stream',
    description: 'Stream the response token by token (default: true for interactive)',
    required: false,
    type: 'boolean',
  },
  {
    name: 'noStream',
    short: '',
    long: '--no-stream',
    description: 'Disable streaming',
    required: false,
    type: 'boolean',
  },
  {
    name: 'interactive',
    short: '-i',
    long: '--interactive',
    description: 'Start interactive REPL mode',
    required: false,
    type: 'boolean',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output response as JSON',
    required: false,
    type: 'boolean',
  },
  {
    name: 'pipe',
    short: '',
    long: '--pipe',
    description: 'Pipe output to destination: file:<path>, clipboard, "command:<cmd>"',
    required: false,
    type: 'string',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Output format: text, json, markdown, csv',
    required: false,
    type: 'string',
  },
  {
    name: 'conversation',
    short: '',
    long: '--conversation',
    description: 'Continue existing conversation by ID',
    required: false,
    type: 'string',
  },
  {
    name: 'new',
    short: '',
    long: '--new',
    description: 'Start a new conversation',
    required: false,
    type: 'boolean',
  },
  {
    name: 'title',
    short: '',
    long: '--title',
    description: 'Set conversation title',
    required: false,
    type: 'string',
  },
];

async function chatAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  // Resolve model: CLI flag > config default > hardcoded default
  const model = String(args.options.model || ctx.config.defaultModel || 'llama-3.3-70b');

  // Resolve relay and API key from CLI flags (override config)
  const relayUrl = args.options.relay ? String(args.options.relay) : ctx.config.baseUrl;
  const apiKey = args.options.apiKey ? String(args.options.apiKey) : ctx.config.apiKey;

  const systemPrompt = args.options.system ? String(args.options.system) : undefined;
  const temperature = args.options.temperature !== undefined ? Number(args.options.temperature) : undefined;
  const maxTokens = args.options.maxTokens !== undefined ? Number(args.options.maxTokens) : undefined;
  const shouldStream = args.options.noStream === true
    ? false
    : (args.options.stream === true || args.options.interactive === true);
  const isInteractive = args.options.interactive === true;
  const outputJson = args.options.json === true;

  // Conversation flags
  const conversationId = args.options.conversation ? String(args.options.conversation) : undefined;
  const isNewConversation = args.options.new === true;
  const conversationTitle = args.options.title ? String(args.options.title) : undefined;

  // Interactive REPL mode
  if (isInteractive) {
    const { startRepl } = await import('../repl');
    await startRepl({
      client: ctx.client,
      config: { ...ctx.config, baseUrl: relayUrl, apiKey },
      output: ctx.output,
      model,
      systemPrompt,
      temperature,
      maxTokens,
      stream: shouldStream,
      conversationId,
      newConversation: isNewConversation,
      conversationTitle,
    });
    return;
  }

  // ── Piped stdin support ─────────────────────────────────────
  let prompt = args.positional.join(' ');

  // Only read stdin if we have no positional args (meaning input comes from pipe).
  // If positional args are provided, stdin is ignored to avoid hanging in non-TTY
  // environments (e.g. test runners, CI).
  if (!prompt && !process.stdin.isTTY) {
    try {
      const stdinContent = await readStdin();
      const stdinTrimmed = stdinContent.trim();
      if (stdinTrimmed) {
        prompt = stdinTrimmed;
      }
    } catch (err) {
      ctx.output.writeError(`Failed to read stdin: ${err instanceof Error ? err.message : String(err)}`);
      process.exit(1);
      return; // unreachable
    }
  }

  if (!prompt) {
    ctx.output.writeError('No prompt provided. Use: xergon chat "your prompt"');
    ctx.output.info('You can also pipe input: echo "prompt" | xergon chat');
    ctx.output.info('Or start interactive mode: xergon chat -i');
    process.exit(1);
  }

  // Build messages, optionally from existing conversation
  let messages: Array<{ role: 'system' | 'user' | 'assistant' | 'tool'; content: string }> = [];

  if (conversationId) {
    // Load existing conversation context
    try {
      const { getMessagesForContext, addMessage } = await import('../../conversation');
      const contextMessages = getMessagesForContext(conversationId);
      messages = contextMessages.map(m => ({ role: m.role, content: m.content }));
      // Add the new user message
      addMessage(conversationId, { role: 'user', content: prompt });
    } catch {
      // If conversation not found, fall through to normal mode
      ctx.output.warn(`Conversation ${conversationId} not found. Starting fresh.`);
    }
  }

  if (systemPrompt && !messages.some(m => m.role === 'system')) {
    messages.push({ role: 'system' as const, content: systemPrompt });
  }
  if (!conversationId) {
    messages.push({ role: 'user' as const, content: prompt });
  }

  const params: Record<string, unknown> = { model, messages };
  if (temperature !== undefined) params.temperature = temperature;
  if (maxTokens !== undefined) params.maxTokens = maxTokens;

  try {
    if (shouldStream) {
      const stream = await ctx.client.chat.completions.stream({
        model,
        messages,
        temperature,
        maxTokens,
      });

      let fullResponse = '';
      for await (const chunk of stream) {
        const delta = chunk.choices?.[0]?.delta?.content;
        if (delta) {
          fullResponse += delta;
          process.stdout.write(delta);
        }
      }
      process.stdout.write('\n');

      // Save assistant response to conversation
      if (conversationId && fullResponse) {
        try {
          const { addMessage: addMsg } = await import('../../conversation');
          addMsg(conversationId, { role: 'assistant', content: fullResponse });
        } catch {
          // Silently ignore
        }
      }
    } else {
      // Non-streaming: show thinking indicator
      const thinkingMsg = ctx.output.colorize('Thinking', 'cyan');
      let dots = 0;
      const spinnerInterval = setInterval(() => {
        dots = (dots + 1) % 4;
        process.stderr.write(`\r${thinkingMsg}${'.'.repeat(dots)}   `);
      }, 300);

      try {
        const response = await ctx.client.chat.completions.create({
          model,
          messages,
          temperature,
          maxTokens,
        });

        clearInterval(spinnerInterval);
        process.stderr.write('\r' + ' '.repeat(20) + '\r');

        // Resolve --pipe and --format flags
        const pipeStr = args.options.pipe ? String(args.options.pipe) : undefined;
        const formatStr = args.options.format ? String(args.options.format) : undefined;
        const outputFormat: OutputFormat = (['text', 'json', 'markdown', 'csv'].includes(formatStr ?? '') ? formatStr : 'text') as OutputFormat;

        // Save assistant response to conversation
        if (conversationId) {
          try {
            const { addMessage: addMsg } = await import('../../conversation');
            const content = response.choices?.[0]?.message?.content || '';
            addMsg(conversationId, { role: 'assistant', content });
          } catch {
            // Silently ignore
          }
        }

        if (pipeStr) {
          const content = response.choices?.[0]?.message?.content || '';
          const pipeConfig = parsePipeString(pipeStr, outputFormat);
          await pipeOutput(content, pipeConfig);
        } else if (outputJson) {
          ctx.output.setFormat('json');
          ctx.output.write(ctx.output.formatOutput(response));
        } else {
          const content = response.choices?.[0]?.message?.content || '(no content)';
          ctx.output.write(content + '\n');

          if (response.usage) {
            ctx.output.info(
              `Tokens: ${response.usage.promptTokens} prompt + ${response.usage.completionTokens} completion = ${response.usage.totalTokens} total`
            );
          }
        }
      } catch (err) {
        clearInterval(spinnerInterval);
        process.stderr.write('\r' + ' '.repeat(20) + '\r');
        throw err;
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Chat completion failed: ${message}`);
    process.exit(1);
  }
}

export const chatCommand: Command = {
  name: 'chat',
  description: 'Send a chat completion request or start interactive REPL',
  aliases: ['ask', 'complete'],
  options: chatOptions,
  action: chatAction,
};
