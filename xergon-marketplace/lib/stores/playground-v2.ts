/**
 * Playground v2 store -- conversation history with persistence.
 *
 * Uses zustand persist (localStorage "xergon-playground") to keep
 * conversation state across sessions.  Conversations are stored as a
 * Record<string, Conversation> for easy serialization (Maps don't
 * serialize well with zustand persist).
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";

// ── Types ──────────────────────────────────────────────────────────────

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  tokens?: { input: number; output: number };
  timestamp: number;
  model?: string;
  costNanoerg?: number;
}

export interface Conversation {
  id: string;
  title: string;
  model: string;
  messages: ChatMessage[];
  createdAt: number;
  updatedAt: number;
  totalTokens: number;
}

interface PlaygroundV2State {
  conversations: Record<string, Conversation>;
  activeConversationId: string | null;
  isGenerating: boolean;

  // Actions
  createConversation: (model: string) => string;
  addMessage: (
    conversationId: string,
    msg: Omit<ChatMessage, "id" | "timestamp">,
  ) => void;
  updateAssistantMessage: (
    conversationId: string,
    messageId: string,
    content: string,
    tokens?: { input: number; output: number },
    costNanoerg?: number,
  ) => void;
  deleteConversation: (id: string) => void;
  setActiveConversation: (id: string | null) => void;
  clearHistory: () => void;
  setGenerating: (val: boolean) => void;
}

// ── Helpers ────────────────────────────────────────────────────────────

const MAX_CONVERSATIONS = 50;

function autoTitle(content: string): string {
  const trimmed = content.trim();
  if (trimmed.length <= 40) return trimmed;
  return trimmed.slice(0, 40).trim() + "…";
}

/** Evict oldest conversation when exceeding MAX_CONVERSATIONS */
function evictOldest(
  convos: Record<string, Conversation>,
): Record<string, Conversation> {
  const keys = Object.keys(convos);
  if (keys.length <= MAX_CONVERSATIONS) return convos;

  // Sort by createdAt ascending, remove the oldest
  const sorted = keys.sort(
    (a, b) => convos[a].createdAt - convos[b].createdAt,
  );
  const toRemove = sorted.slice(0, keys.length - MAX_CONVERSATIONS);
  const next = { ...convos };
  for (const k of toRemove) {
    delete next[k];
  }
  return next;
}

// ── Store ──────────────────────────────────────────────────────────────

export const usePlaygroundV2Store = create<PlaygroundV2State>()(
  persist(
    (set, get) => ({
      conversations: {},
      activeConversationId: null,
      isGenerating: false,

      createConversation: (model: string) => {
        const id = crypto.randomUUID();
        const now = Date.now();
        const convo: Conversation = {
          id,
          title: "New Chat",
          model,
          messages: [],
          createdAt: now,
          updatedAt: now,
          totalTokens: 0,
        };
        set((state) => ({
          conversations: evictOldest({ ...state.conversations, [id]: convo }),
          activeConversationId: id,
        }));
        return id;
      },

      addMessage: (conversationId, msg) => {
        const message: ChatMessage = {
          ...msg,
          id: crypto.randomUUID(),
          timestamp: Date.now(),
        };
        set((state) => {
          const convo = state.conversations[conversationId];
          if (!convo) return state;

          const isFirstUserMsg =
            msg.role === "user" && convo.messages.length === 0;

          const updated: Conversation = {
            ...convo,
            messages: [...convo.messages, message],
            title: isFirstUserMsg ? autoTitle(msg.content) : convo.title,
            updatedAt: Date.now(),
          };
          return {
            conversations: evictOldest({
              ...state.conversations,
              [conversationId]: updated,
            }),
          };
        });
      },

      updateAssistantMessage: (
        conversationId,
        messageId,
        content,
        tokens,
        costNanoerg,
      ) => {
        set((state) => {
          const convo = state.conversations[conversationId];
          if (!convo) return state;

          const messages = convo.messages.map((m) =>
            m.id === messageId
              ? {
                  ...m,
                  content,
                  tokens: tokens ?? m.tokens,
                  costNanoerg: costNanoerg ?? m.costNanoerg,
                }
              : m,
          );

          // Recalculate total tokens
          const totalTokens = messages.reduce(
            (sum, m) => sum + (m.tokens?.input ?? 0) + (m.tokens?.output ?? 0),
            0,
          );

          return {
            conversations: {
              ...state.conversations,
              [conversationId]: {
                ...convo,
                messages,
                totalTokens,
                updatedAt: Date.now(),
              },
            },
          };
        });
      },

      deleteConversation: (id) => {
        set((state) => {
          const { [id]: _, ...rest } = state.conversations;
          return {
            conversations: rest,
            activeConversationId:
              state.activeConversationId === id
                ? null
                : state.activeConversationId,
          };
        });
      },

      setActiveConversation: (id) => {
        set({ activeConversationId: id });
      },

      clearHistory: () => {
        set({ conversations: {}, activeConversationId: null });
      },

      setGenerating: (val) => {
        set({ isGenerating: val });
      },
    }),
    {
      name: "xergon-playground",
      // Only persist conversation data, not transient UI state
      partialize: (state) => ({
        conversations: state.conversations,
        activeConversationId: state.activeConversationId,
      }),
    },
  ),
);
