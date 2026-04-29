/**
 * Client-safe API exports.
 *
 * This file imports ONLY from @xergon/sdk types/constants and re-exports them.
 * It does NOT import any runtime SDK code that uses Node.js (node:fs).
 *
 * Import this in client components. For server-only SDK client, use server-sdk.ts.
 */

import type {
  ChatMessage,
  ChatCompletionParams,
  ChatCompletionResponse,
  Model,
  Provider,
  LeaderboardEntry,
  BalanceResponse,
} from '@xergon/sdk';
import type { XergonErrorType, XergonErrorBody } from '@xergon/sdk';

export const API_BASE = (process.env.NEXT_PUBLIC_API_BASE || 'http://127.0.0.1:9090') + '/v1';
export type { ChatMessage, ChatCompletionParams, ChatCompletionResponse, Model, Provider, LeaderboardEntry, BalanceResponse };
export type { XergonErrorType, XergonErrorBody };
