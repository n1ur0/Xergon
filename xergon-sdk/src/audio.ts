/**
 * Audio -- TTS, speech-to-text, and audio translation.
 *
 * Provides client methods for text-to-speech, transcription, and
 * translation via the relay's OpenAI-compatible audio endpoints.
 *
 * @example
 * ```ts
 * import { XergonClient } from '@xergon/sdk';
 *
 * const client = new XergonClient({ baseUrl: 'https://relay.xergon.gg' });
 *
 * // TTS
 * const audio = await client.audio.speech.create({
 *   model: 'tts-1',
 *   input: 'Hello world',
 *   voice: 'alloy',
 * });
 * // audio is a Buffer containing mp3 data
 *
 * // STT
 * const transcript = await client.audio.transcriptions.create({
 *   model: 'whisper-1',
 *   file: '/path/to/audio.mp3',
 * });
 * console.log(transcript.text);
 * ```
 */

import * as fs from 'node:fs';
import { XergonClientCore } from './client';

// ── Types ───────────────────────────────────────────────────────────

export interface SpeechRequest {
  /** TTS model to use (e.g., 'tts-1', 'tts-1-hd'). */
  model: string;
  /** The text to generate audio for. */
  input: string;
  /** The voice to use (e.g., 'alloy', 'echo', 'fable', 'onyx', 'nova', 'shimmer'). */
  voice?: string;
  /** Audio format to return (default: 'mp3'). */
  response_format?: 'mp3' | 'opus' | 'aac' | 'flac' | 'wav';
  /** Speed of the generated audio (0.25 to 4.0, default: 1.0). */
  speed?: number;
}

export interface TranscriptionRequest {
  /** Transcription model to use (e.g., 'whisper-1'). */
  model: string;
  /** Audio file: a file path (string), a Buffer, or a web File object. */
  file: File | Buffer | string;
  /** Language of the input audio (ISO 639-1 format, e.g., 'en'). */
  language?: string;
  /** Output format (default: 'json'). */
  response_format?: 'json' | 'text' | 'srt' | 'vtt';
}

export interface TranscriptionResponse {
  /** Transcribed text. */
  text: string;
}

// ── Client Methods ────────────────────────────────────────────────

/**
 * Create text-to-speech audio. Returns a Buffer containing the audio data.
 * POST /v1/audio/speech
 */
export async function createSpeech(
  client: XergonClientCore,
  request: SpeechRequest,
  options?: { signal?: AbortSignal },
): Promise<Buffer> {
  const url = `${client.getBaseUrl()}/v1/audio/speech`;

  const res = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(client.getPublicKey() ? { 'X-Xergon-Public-Key': client.getPublicKey()! } : {}),
    },
    body: JSON.stringify(request),
    signal: options?.signal,
  });

  if (!res.ok) {
    let errorData: unknown;
    try {
      errorData = await res.json();
    } catch {
      errorData = { message: res.statusText };
    }
    const { XergonError } = await import('./errors');
    throw XergonError.fromResponse(errorData);
  }

  const arrayBuffer = await res.arrayBuffer();
  return Buffer.from(arrayBuffer);
}

/**
 * Transcribe audio to text.
 * POST /v1/audio/transcriptions (multipart/form-data)
 */
export async function createTranscription(
  client: XergonClientCore,
  request: TranscriptionRequest,
  options?: { signal?: AbortSignal },
): Promise<TranscriptionResponse> {
  const url = `${client.getBaseUrl()}/v1/audio/transcriptions`;
  const formData = await buildAudioFormData(request);

  const res = await fetch(url, {
    method: 'POST',
    headers: {
      ...(client.getPublicKey() ? { 'X-Xergon-Public-Key': client.getPublicKey()! } : {}),
    },
    body: formData,
    signal: options?.signal,
  });

  if (!res.ok) {
    let errorData: unknown;
    try {
      errorData = await res.json();
    } catch {
      errorData = { message: res.statusText };
    }
    const { XergonError } = await import('./errors');
    throw XergonError.fromResponse(errorData);
  }

  const contentType = res.headers.get('content-type') ?? '';

  if (request.response_format === 'json' || request.response_format === undefined) {
    return await res.json() as TranscriptionResponse;
  }

  // For text/srt/vtt, the response is plain text
  const text = await res.text();
  return { text };
}

/**
 * Translate audio to English.
 * POST /v1/audio/translations (multipart/form-data)
 */
export async function createTranslation(
  client: XergonClientCore,
  request: TranscriptionRequest,
  options?: { signal?: AbortSignal },
): Promise<TranscriptionResponse> {
  const url = `${client.getBaseUrl()}/v1/audio/translations`;
  const formData = await buildAudioFormData(request);

  const res = await fetch(url, {
    method: 'POST',
    headers: {
      ...(client.getPublicKey() ? { 'X-Xergon-Public-Key': client.getPublicKey()! } : {}),
    },
    body: formData,
    signal: options?.signal,
  });

  if (!res.ok) {
    let errorData: unknown;
    try {
      errorData = await res.json();
    } catch {
      errorData = { message: res.statusText };
    }
    const { XergonError } = await import('./errors');
    throw XergonError.fromResponse(errorData);
  }

  const contentType = res.headers.get('content-type') ?? '';

  if (request.response_format === 'json' || request.response_format === undefined) {
    return await res.json() as TranscriptionResponse;
  }

  const text = await res.text();
  return { text };
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Build a multipart/form-data body for audio transcription/translation.
 */
async function buildAudioFormData(
  request: TranscriptionRequest,
): Promise<FormData> {
  const formData = new FormData();
  formData.append('model', request.model);

  if (request.response_format) {
    formData.append('response_format', request.response_format);
  }
  if (request.language) {
    formData.append('language', request.language);
  }

  // Resolve the file
  const fileSource = request.file;
  if (typeof fileSource === 'string') {
    // File path
    const buffer = fs.readFileSync(fileSource);
    const filename = fileSource.split('/').pop() || 'audio.mp3';
    formData.append('file', new Blob([buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength) as ArrayBuffer]), filename);
  } else if (Buffer.isBuffer(fileSource)) {
    formData.append('file', new Blob([fileSource.buffer.slice(fileSource.byteOffset, fileSource.byteOffset + fileSource.byteLength) as ArrayBuffer]), 'audio.mp3');
  } else {
    // Web File object
    const webFile = fileSource as File;
    formData.append('file', webFile, webFile.name);
  }

  return formData;
}
