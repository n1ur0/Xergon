/**
 * Multi-Turn Conversation Memory
 *
 * Provides persistent conversation storage, context window management,
 * search, and import/export for the Xergon SDK.
 *
 * Conversations are stored in ~/.xergon/conversations.json
 */

import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import * as crypto from 'node:crypto';

// ── Types ─────────────────────────────────────────────────────────

export interface Message {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  timestamp: number;
  tokens?: number;
  model?: string;
}

export interface Conversation {
  id: string;
  title: string;
  messages: Message[];
  model: string;
  systemPrompt?: string;
  createdAt: number;
  updatedAt: number;
  metadata?: Record<string, string>;
}

export interface ConversationStore {
  conversations: Record<string, Conversation>;
  activeConversationId?: string;
  maxTokens?: number; // max context window tokens
}

// ── Storage path ──────────────────────────────────────────────────

const CONVERSATIONS_DIR = path.join(os.homedir(), '.xergon');
const CONVERSATIONS_FILE = path.join(CONVERSATIONS_DIR, 'conversations.json');

// ── Helpers ───────────────────────────────────────────────────────

function loadStore(): ConversationStore {
  try {
    const data = fs.readFileSync(CONVERSATIONS_FILE, 'utf-8');
    return JSON.parse(data) as ConversationStore;
  } catch {
    return { conversations: {} };
  }
}

function saveStore(store: ConversationStore): void {
  try {
    fs.mkdirSync(CONVERSATIONS_DIR, { recursive: true });
    fs.writeFileSync(CONVERSATIONS_FILE, JSON.stringify(store, null, 2), 'utf-8');
  } catch (err) {
    throw new Error(`Failed to save conversations: ${err instanceof Error ? err.message : String(err)}`);
  }
}

function generateId(): string {
  return crypto.randomUUID().slice(0, 8);
}

/**
 * Rough token estimate: ~4 chars per token for English text.
 */
function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

// ── API ───────────────────────────────────────────────────────────

/**
 * Create a new conversation.
 */
export function createConversation(
  title: string,
  model: string,
  systemPrompt?: string,
): Conversation {
  const store = loadStore();
  const id = generateId();
  const now = Date.now();

  const conversation: Conversation = {
    id,
    title,
    messages: [],
    model,
    systemPrompt,
    createdAt: now,
    updatedAt: now,
  };

  store.conversations[id] = conversation;
  store.activeConversationId = id;
  saveStore(store);

  return conversation;
}

/**
 * Add a message to a conversation.
 */
export function addMessage(
  conversationId: string,
  message: Omit<Message, 'timestamp'>,
): Message {
  const store = loadStore();
  const conversation = store.conversations[conversationId];

  if (!conversation) {
    throw new Error(`Conversation not found: ${conversationId}`);
  }

  const fullMessage: Message = {
    ...message,
    timestamp: Date.now(),
  };

  conversation.messages.push(fullMessage);
  conversation.updatedAt = Date.now();
  saveStore(store);

  return fullMessage;
}

/**
 * Get a full conversation by ID.
 */
export function getConversation(id: string): Conversation | undefined {
  const store = loadStore();
  return store.conversations[id];
}

/**
 * List all conversations with message counts.
 */
export function listConversations(): Array<{
  id: string;
  title: string;
  model: string;
  messageCount: number;
  createdAt: number;
  updatedAt: number;
  isActive: boolean;
}> {
  const store = loadStore();

  return Object.values(store.conversations).map(conv => ({
    id: conv.id,
    title: conv.title,
    model: conv.model,
    messageCount: conv.messages.length,
    createdAt: conv.createdAt,
    updatedAt: conv.updatedAt,
    isActive: conv.id === store.activeConversationId,
  }));
}

/**
 * Delete a conversation by ID.
 */
export function deleteConversation(id: string): boolean {
  const store = loadStore();

  if (!store.conversations[id]) {
    return false;
  }

  delete store.conversations[id];

  if (store.activeConversationId === id) {
    const remaining = Object.keys(store.conversations);
    store.activeConversationId = remaining.length > 0 ? remaining[0] : undefined;
  }

  saveStore(store);
  return true;
}

/**
 * Set the active conversation.
 */
export function setActive(id: string): void {
  const store = loadStore();

  if (!store.conversations[id]) {
    throw new Error(`Conversation not found: ${id}`);
  }

  store.activeConversationId = id;
  saveStore(store);
}

/**
 * Get the active conversation.
 */
export function getActive(): Conversation | undefined {
  const store = loadStore();

  if (!store.activeConversationId) {
    return undefined;
  }

  return store.conversations[store.activeConversationId];
}

/**
 * Get messages that fit within a token budget for the context window.
 * Drops the oldest messages first. Always keeps the system message.
 */
export function getMessagesForContext(
  conversationId: string,
  maxTokens: number = 4096,
): Message[] {
  const store = loadStore();
  const conversation = store.conversations[conversationId];

  if (!conversation) {
    throw new Error(`Conversation not found: ${conversationId}`);
  }

  const effectiveMax = store.maxTokens ?? maxTokens;
  const systemMessages = conversation.messages.filter(m => m.role === 'system');
  const otherMessages = conversation.messages.filter(m => m.role !== 'system');

  // Calculate system message tokens
  let systemTokens = 0;
  for (const msg of systemMessages) {
    systemTokens += estimateTokens(msg.content);
  }

  const remaining = effectiveMax - systemTokens;
  if (remaining <= 0) {
    return systemMessages;
  }

  // Walk from newest to oldest, accumulating until we hit the budget
  const selected: Message[] = [];
  let usedTokens = 0;

  for (let i = otherMessages.length - 1; i >= 0; i--) {
    const msg = otherMessages[i];
    const tokens = msg.tokens ?? estimateTokens(msg.content);

    if (usedTokens + tokens > remaining) {
      break;
    }

    usedTokens += tokens;
    selected.unshift(msg);
  }

  return [...systemMessages, ...selected];
}

/**
 * Search across all conversations for a text query.
 */
export function searchConversations(query: string): Array<{
  conversationId: string;
  conversationTitle: string;
  messageId: number;
  role: string;
  content: string;
}> {
  const store = loadStore();
  const lowerQuery = query.toLowerCase();
  const results: Array<{
    conversationId: string;
    conversationTitle: string;
    messageId: number;
    role: string;
    content: string;
  }> = [];

  for (const conv of Object.values(store.conversations)) {
    for (let i = 0; i < conv.messages.length; i++) {
      const msg = conv.messages[i];
      if (msg.content.toLowerCase().includes(lowerQuery)) {
        results.push({
          conversationId: conv.id,
          conversationTitle: conv.title,
          messageId: i,
          role: msg.role,
          content: msg.content,
        });
      }
    }
  }

  return results;
}

/**
 * Export a conversation as a markdown string.
 */
export function exportConversation(id: string): string {
  const store = loadStore();
  const conversation = store.conversations[id];

  if (!conversation) {
    throw new Error(`Conversation not found: ${id}`);
  }

  const lines: string[] = [];
  lines.push(`# ${conversation.title}`);
  lines.push('');
  lines.push(`- **Model:** ${conversation.model}`);
  lines.push(`- **Created:** ${new Date(conversation.createdAt).toISOString()}`);
  lines.push(`- **Updated:** ${new Date(conversation.updatedAt).toISOString()}`);
  lines.push(`- **Messages:** ${conversation.messages.length}`);
  if (conversation.systemPrompt) {
    lines.push(`- **System Prompt:** ${conversation.systemPrompt}`);
  }
  lines.push('');
  lines.push('---');
  lines.push('');

  for (const msg of conversation.messages) {
    const time = new Date(msg.timestamp).toISOString();
    const role = msg.role.charAt(0).toUpperCase() + msg.role.slice(1);
    lines.push(`### ${role} (${time})`);
    lines.push('');
    lines.push(msg.content);
    lines.push('');
    lines.push('---');
    lines.push('');
  }

  return lines.join('\n');
}

/**
 * Import a conversation from a markdown string.
 * Creates a new conversation and returns it.
 */
export function importConversation(markdown: string): Conversation {
  const titleMatch = markdown.match(/^# (.+)$/m);
  const title = titleMatch ? titleMatch[1].trim() : 'Imported Conversation';

  const modelMatch = markdown.match(/\*\*Model:\*\*\s*(.+)/);
  const model = modelMatch ? modelMatch[1].trim() : 'llama-3.3-70b';

  const systemMatch = markdown.match(/\*\*System Prompt:\*\*\s*(.+)/);
  const systemPrompt = systemMatch ? systemMatch[1].trim() : undefined;

  const conversation = createConversation(title, model, systemPrompt);

  // Parse message blocks
  const messageRegex = /^### (System|User|Assistant|Tool) \((\d{4}-\d{2}-\d{2}T[\d:.]+Z)\)\s*\n\n([\s\S]*?)(?=\n---\n\n|$)/gm;
  let match: RegExpExecArray | null;

  while ((match = messageRegex.exec(markdown)) !== null) {
    const role = match[1].toLowerCase() as Message['role'];
    const timestamp = new Date(match[2]).getTime();
    const content = match[3].trim();

    if (content) {
      const msg: Omit<Message, 'timestamp'> = { role, content };
      addMessage(conversation.id, msg);
    }
  }

  return getConversation(conversation.id)!;
}
