/**
 * CLI command: embed
 *
 * Generate text embeddings from the command line.
 * Supports single text, file input, and JSON output.
 *
 * Usage:
 *   xergon embed "Hello world"
 *   xergon embed --input document.txt --output embeddings.json
 *   xergon embed "Hello" --model text-embedding-3-small --dimensions 512
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import type { EmbeddingResponse } from '../../embeddings';
import * as fs from 'node:fs';

const embedOptions: CommandOption[] = [
  {
    name: 'model',
    short: '-m',
    long: '--model',
    description: 'Embedding model to use (default: text-embedding-3-small)',
    required: false,
    type: 'string',
  },
  {
    name: 'format',
    short: '',
    long: '--format',
    description: 'Encoding format: float or base64 (default: float)',
    required: false,
    type: 'string',
  },
  {
    name: 'dimensions',
    short: '-d',
    long: '--dimensions',
    description: 'Number of embedding dimensions',
    required: false,
    type: 'number',
  },
  {
    name: 'input',
    short: '',
    long: '--input',
    description: 'Read input text from a file',
    required: false,
    type: 'string',
  },
  {
    name: 'output',
    short: '-o',
    long: '--output',
    description: 'Write full JSON output to a file',
    required: false,
    type: 'string',
  },
  {
    name: 'json',
    short: '',
    long: '--json',
    description: 'Output full response as JSON to stdout',
    required: false,
    type: 'boolean',
  },
];

async function embedAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = String(args.options.model || 'text-embedding-3-small');
  const encodingFormat = args.options.format
    ? String(args.options.format) as 'float' | 'base64'
    : undefined;
  const dimensions = args.options.dimensions !== undefined
    ? Number(args.options.dimensions)
    : undefined;
  const inputFile = args.options.input ? String(args.options.input) : undefined;
  const outputFile = args.options.output ? String(args.options.output) : undefined;
  const outputJson = args.options.json === true;

  // Resolve input text
  let inputText: string;

  if (inputFile) {
    try {
      inputText = fs.readFileSync(inputFile, 'utf-8').trim();
    } catch (err) {
      ctx.output.writeError(`Failed to read input file: ${err instanceof Error ? err.message : String(err)}`);
      process.exit(1);
      return;
    }
  } else {
    inputText = args.positional.join(' ').trim();
  }

  if (!inputText) {
    ctx.output.writeError('No input text provided. Use: xergon embed "your text" or --input <file>');
    process.exit(1);
    return;
  }

  // Show progress
  const thinkingMsg = ctx.output.colorize('Generating embeddings', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const response: EmbeddingResponse = await ctx.client.embeddings.create({
      model,
      input: inputText,
      encoding_format: encodingFormat,
      dimensions,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    // Write full output to file if requested
    if (outputFile) {
      try {
        fs.writeFileSync(outputFile, JSON.stringify(response, null, 2) + '\n');
        ctx.output.success(`Full response written to ${outputFile}`);
      } catch (err) {
        ctx.output.writeError(`Failed to write output file: ${err instanceof Error ? err.message : String(err)}`);
        process.exit(1);
        return;
      }
    }

    // Display output
    if (outputJson) {
      ctx.output.setFormat('json');
      ctx.output.write(ctx.output.formatOutput(response));
    } else {
      // Display summary in terminal
      const embedding = response.data[0];
      const dims = embedding.embedding.length;
      const preview = embedding.embedding.slice(0, 10);

      ctx.output.write(ctx.output.colorize('Embedding Generated', 'bold'));
      ctx.output.write('');

      ctx.output.write(`  Model:     ${response.model}`);
      ctx.output.write(`  Dimensions: ${dims}`);
      ctx.output.write(`  Tokens:    ${response.usage.prompt_tokens} prompt / ${response.usage.total_tokens} total`);

      if (response.data.length > 1) {
        ctx.output.write(`  Inputs:    ${response.data.length} embeddings returned`);
      }

      ctx.output.write('');
      ctx.output.write(ctx.output.colorize('  Vector preview (first 10 dimensions):', 'dim'));
      ctx.output.write(`  [${preview.map(v => v.toFixed(6)).join(', ')}, ...]`);

      if (outputFile) {
        ctx.output.info(`Full vector written to ${outputFile}`);
      } else {
        ctx.output.info('Use --output <file> to write the full embedding to a file');
        ctx.output.info('Use --json to output the complete response as JSON');
      }
    }
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Embedding request failed: ${message}`);
    process.exit(1);
  }
}

export const embedCommand: Command = {
  name: 'embed',
  description: 'Generate text embeddings for a given input',
  aliases: ['embedding', 'embeddings'],
  options: embedOptions,
  action: embedAction,
};
