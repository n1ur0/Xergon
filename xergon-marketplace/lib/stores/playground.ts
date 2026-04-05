import { create } from "zustand";

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  model?: string;
  timestamp: number;
  inputTokens?: number;
  outputTokens?: number;
  costNanoerg?: number;
}

interface PlaygroundState {
  selectedModel: string;
  prompt: string;
  messages: Message[];
  isGenerating: boolean;
  setModel: (model: string) => void;
  setPrompt: (prompt: string) => void;
  addMessage: (message: Omit<Message, "id" | "timestamp">) => void;
  updateLastAssistantMessage: (content: string) => void;
  updateLastAssistantMessageUsage: (usage: { inputTokens?: number; outputTokens?: number; costNanoerg?: number }) => void;
  clearMessages: () => void;
  setGenerating: (generating: boolean) => void;
}

export const usePlaygroundStore = create<PlaygroundState>((set) => ({
  selectedModel: "",
  prompt: "",
  messages: [],
  isGenerating: false,

  setModel: (selectedModel) => set({ selectedModel }),
  setPrompt: (prompt) => set({ prompt }),

  addMessage: (message) =>
    set((state) => ({
      messages: [
        ...state.messages,
        {
          ...message,
          id: crypto.randomUUID(),
          timestamp: Date.now(),
        },
      ],
    })),

  updateLastAssistantMessage: (content: string) =>
    set((state) => {
      const msgs = [...state.messages];
      // Find last assistant message and update its content
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i].role === "assistant") {
          msgs[i] = { ...msgs[i], content };
          break;
        }
      }
      return { messages: msgs };
    }),

  updateLastAssistantMessageUsage: (usage) =>
    set((state) => {
      const msgs = [...state.messages];
      for (let i = msgs.length - 1; i >= 0; i--) {
        if (msgs[i].role === "assistant") {
          msgs[i] = { ...msgs[i], ...usage };
          break;
        }
      }
      return { messages: msgs };
    }),

  clearMessages: () => set({ messages: [] }),
  setGenerating: (isGenerating) => set({ isGenerating }),
}));
