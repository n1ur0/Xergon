/**
 * CLI command: audio
 *
 * Text-to-speech, speech-to-text, and audio translation.
 *
 * Usage:
 *   xergon audio speak "Hello world" --voice alloy --output speech.mp3
 *   xergon audio transcribe recording.mp3 --language en
 *   xergon audio translate recording.mp3
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import * as fs from 'node:fs';

// ── Options for the top-level audio command ──────────────────────

const audioOptions: CommandOption[] = [];

async function audioAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon audio <speak|transcribe|translate> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'speak':
      await handleSpeak(args, ctx);
      break;
    case 'transcribe':
      await handleTranscribe(args, ctx);
      break;
    case 'translate':
      await handleTranslate(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown audio subcommand: ${sub}`);
      ctx.output.write('Usage: xergon audio <speak|transcribe|translate> [options]');
      process.exit(1);
  }
}

// ── speak ──────────────────────────────────────────────────────────

async function handleSpeak(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const text = args.positional.slice(1).join(' ').trim();
  const model = String(args.options.model || 'tts-1');
  const voice = args.options.voice ? String(args.options.voice) : 'alloy';
  const format = args.options.format ? String(args.options.format) : 'mp3';
  const speed = args.options.speed !== undefined ? Number(args.options.speed) : undefined;
  const outputFile = args.options.output ? String(args.options.output) : undefined;

  if (!text) {
    ctx.output.writeError('No input text provided. Use: xergon audio speak "your text"');
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Generating speech', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { createSpeech } = await import('../../audio');
    const audioBuffer = await createSpeech(ctx.client._core || ctx.client.core, {
      model,
      input: text,
      voice,
      response_format: format as 'mp3' | 'opus' | 'aac' | 'flac' | 'wav',
      speed,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    const ext = format || 'mp3';
    const outputPath = outputFile || `speech.${ext}`;

    fs.writeFileSync(outputPath, audioBuffer);
    ctx.output.success(`Audio saved to ${outputPath} (${audioBuffer.length} bytes)`);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`TTS request failed: ${message}`);
    process.exit(1);
  }
}

// ── transcribe ─────────────────────────────────────────────────────

async function handleTranscribe(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const file = args.positional[1];
  const model = String(args.options.model || 'whisper-1');
  const language = args.options.language ? String(args.options.language) : undefined;
  const format = args.options.format ? String(args.options.format) : undefined;

  if (!file) {
    ctx.output.writeError('No audio file specified. Use: xergon audio transcribe <file>');
    process.exit(1);
    return;
  }

  if (!fs.existsSync(file)) {
    ctx.output.writeError(`File not found: ${file}`);
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Transcribing audio', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { createTranscription } = await import('../../audio');
    const result = await createTranscription(ctx.client._core || ctx.client.core, {
      model,
      file,
      language,
      response_format: format as 'json' | 'text' | 'srt' | 'vtt' | undefined,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    ctx.output.write(result.text);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Transcription failed: ${message}`);
    process.exit(1);
  }
}

// ── translate ──────────────────────────────────────────────────────

async function handleTranslate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const file = args.positional[1];
  const model = String(args.options.model || 'whisper-1');
  const format = args.options.format ? String(args.options.format) : undefined;

  if (!file) {
    ctx.output.writeError('No audio file specified. Use: xergon audio translate <file>');
    process.exit(1);
    return;
  }

  if (!fs.existsSync(file)) {
    ctx.output.writeError(`File not found: ${file}`);
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Translating audio to English', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { createTranslation } = await import('../../audio');
    const result = await createTranslation(ctx.client._core || ctx.client.core, {
      model,
      file,
      response_format: format as 'json' | 'text' | 'srt' | 'vtt' | undefined,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    ctx.output.write(result.text);
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Translation failed: ${message}`);
    process.exit(1);
  }
}

export const audioCommand: Command = {
  name: 'audio',
  description: 'Text-to-speech, transcription, and audio translation',
  aliases: ['tts', 'stt'],
  options: [
    {
      name: 'model',
      short: '-m',
      long: '--model',
      description: 'Model to use (default: tts-1 for speak, whisper-1 for transcribe/translate)',
      required: false,
      type: 'string',
    },
    {
      name: 'voice',
      short: '',
      long: '--voice',
      description: 'TTS voice (alloy, echo, fable, onyx, nova, shimmer)',
      required: false,
      type: 'string',
    },
    {
      name: 'format',
      short: '',
      long: '--format',
      description: 'Audio/output format (mp3, opus, aac, flac, wav, json, text, srt, vtt)',
      required: false,
      type: 'string',
    },
    {
      name: 'speed',
      short: '',
      long: '--speed',
      description: 'TTS speed (0.25 to 4.0, default: 1.0)',
      required: false,
      type: 'number',
    },
    {
      name: 'output',
      short: '-o',
      long: '--output',
      description: 'Output file path for TTS audio',
      required: false,
      type: 'string',
    },
    {
      name: 'language',
      short: '',
      long: '--language',
      description: 'Audio language for transcription (ISO 639-1, e.g., en, es, fr)',
      required: false,
      type: 'string',
    },
  ],
  action: audioAction,
};
