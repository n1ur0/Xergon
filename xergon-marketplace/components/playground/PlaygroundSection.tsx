"use client";

import { useCallback, useRef, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import { usePlaygroundStore } from "@/lib/stores/playground";
import { usePlaygroundV2Store } from "@/lib/stores/playground-v2";
import { endpoints, type ModelInfo } from "@/lib/api/client";
import { FALLBACK_MODELS } from "@/lib/constants";
import { API_BASE, getWalletPk } from "@/lib/api/config";
import { ModelSelector } from "@/components/ModelSelector";
import { PromptBox } from "@/components/PromptBox";
import { ResponseArea } from "@/components/ResponseArea";
import { ConversationList } from "@/components/playground/ConversationList";
import { ModelComparison } from "@/components/playground/ModelComparison";
import { RateLimitIndicator } from "@/components/playground/RateLimitIndicator";
import { cn } from "@/lib/utils";

type Tab = "chat" | "compare";

export function PlaygroundSection() {
  const searchParams = useSearchParams();
  const [models, setModels] = useState(FALLBACK_MODELS);
  const [tab, setTab] = useState<Tab>("chat");
  const abortRef = useRef<AbortController | null>(null);

  // ── v1 store (prompt, model, messages, generating) ──
  const selectedModel = usePlaygroundStore((s) => s.selectedModel);
  const prompt = usePlaygroundStore((s) => s.prompt);
  const messages = usePlaygroundStore((s) => s.messages);
  const isGenerating = usePlaygroundStore((s) => s.isGenerating);
  const addMessage = usePlaygroundStore((s) => s.addMessage);
  const updateLastAssistantMessage = usePlaygroundStore((s) => s.updateLastAssistantMessage);
  const updateLastAssistantMessageUsage = usePlaygroundStore((s) => s.updateLastAssistantMessageUsage);
  const setGenerating = usePlaygroundStore((s) => s.setGenerating);
  const setModel = usePlaygroundStore((s) => s.setModel);
  const setPrompt = usePlaygroundStore((s) => s.setPrompt);
  const clearMessages = usePlaygroundStore((s) => s.clearMessages);

  // ── v2 store (conversations) ──
  const activeConvoId = usePlaygroundV2Store((s) => s.activeConversationId);
  const conversations = usePlaygroundV2Store((s) => s.conversations);
  const createConvo = usePlaygroundV2Store((s) => s.createConversation);
  const addConvoMsg = usePlaygroundV2Store((s) => s.addMessage);
  const updateConvoAssistantMsg = usePlaygroundV2Store((s) => s.updateAssistantMessage);
  const setActiveConvo = usePlaygroundV2Store((s) => s.setActiveConversation);

  // Fetch available models on mount
  useEffect(() => {
    endpoints
      .listModels()
      .then((ms: ModelInfo[]) => {
        if (ms.length > 0) setModels(ms);
      })
      .catch(() => {});
  }, []);

  // Pre-select model from ?model= query param
  useEffect(() => {
    const modelParam = searchParams.get("model");
    if (modelParam && !selectedModel) {
      setModel(modelParam);
    }
  }, [searchParams, selectedModel, setModel]);

  // When switching conversations, restore messages into v1 store
  useEffect(() => {
    if (!activeConvoId) {
      clearMessages();
      return;
    }
    const convo = conversations[activeConvoId];
    if (!convo) return;

    // Restore messages into v1 store for rendering
    // We need to carefully sync -- set messages directly via a small trick
    const { usePlaygroundStore: _store } = require("@/lib/stores/playground");
    // Instead, we just update the model to match the conversation
    if (convo.model && convo.model !== selectedModel) {
      setModel(convo.model);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeConvoId]);

  // Restore messages from active conversation into v1 store when switching
  const restoreConvoMessages = useCallback(
    (convoId: string | null) => {
      if (!convoId) {
        clearMessages();
        setPrompt("");
        return;
      }
      const convo = conversations[convoId];
      if (!convo) return;
      // We need to set messages into the v1 store.
      // Since the v1 store doesn't expose a setMessages, we use setState via the store directly.
      const store = usePlaygroundStore.getState();
      // Clear and re-add each message
      usePlaygroundStore.setState({
        messages: convo.messages.map((m) => ({
          id: m.id,
          role: m.role as "user" | "assistant",
          content: m.content,
          model: m.model,
          timestamp: m.timestamp,
          inputTokens: m.tokens?.input,
          outputTokens: m.tokens?.output,
          costNanoerg: m.costNanoerg,
        })),
      });
      if (convo.model) setModel(convo.model);
      setPrompt("");
    },
    [conversations, clearMessages, setModel, setPrompt],
  );

  // Track last restored convo id to avoid re-restoring
  const lastRestoredRef = useRef<string | null>(null);
  useEffect(() => {
    if (activeConvoId !== lastRestoredRef.current) {
      lastRestoredRef.current = activeConvoId;
      restoreConvoMessages(activeConvoId);
    }
  }, [activeConvoId, restoreConvoMessages]);

  // ── Submit handler ──
  const handleSubmit = useCallback(async () => {
    if (!prompt.trim() || !selectedModel || isGenerating) return;

    // Ensure we have an active conversation
    let convoId = activeConvoId;
    if (!convoId) {
      convoId = createConvo(selectedModel);
    }

    const userMsgId = crypto.randomUUID();
    const assistantMsgId = crypto.randomUUID();

    // Add user message to both stores
    addMessage({ role: "user", content: prompt.trim(), model: selectedModel });
    addConvoMsg(convoId, {
      role: "user",
      content: prompt.trim(),
      model: selectedModel,
    });

    // Add empty assistant placeholder
    addMessage({ role: "assistant", content: "", model: selectedModel });
    addConvoMsg(convoId, {
      role: "assistant",
      content: "",
      model: selectedModel,
    });

    setGenerating(true);
    setPrompt("");

    const abort = new AbortController();
    abortRef.current = abort;

    try {
      // Build messages array with conversation history for context
      const convo = usePlaygroundV2Store.getState().conversations[convoId!];
      const historyMessages: { role: string; content: string }[] = [];
      if (convo) {
        for (const m of convo.messages) {
          if (m.id === assistantMsgId) continue; // skip the empty placeholder
          historyMessages.push({ role: m.role, content: m.content });
        }
      }

      const walletPk = getWalletPk();
      const res = await fetch(`${API_BASE}/v1/chat/completions`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "text/event-stream",
          ...(walletPk ? { "x-user-pk": walletPk } : {}),
        },
        body: JSON.stringify({
          model: selectedModel,
          messages: historyMessages,
          stream: true,
        }),
        signal: abort.signal,
      });

      if (!res.ok) {
        // Capture rate limit headers even on error responses
        if (RateLimitIndicator._updateRef) {
          RateLimitIndicator._updateRef(res);
        }
        const errText = `Error: Model returned status ${res.status}. Please try again.`;
        updateLastAssistantMessage(errText);
        updateConvoAssistantMsg(convoId!, assistantMsgId, errText);
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) {
        const errText = "Error: No response stream. Please try again.";
        updateLastAssistantMessage(errText);
        updateConvoAssistantMsg(convoId!, assistantMsgId, errText);
        return;
      }

      // Capture rate limit headers from successful response
      if (RateLimitIndicator._updateRef) {
        RateLimitIndicator._updateRef(res);
      }

      const decoder = new TextDecoder();
      let accumulated = "";
      let buffer = "";
      let usageData: {
        inputTokens?: number;
        outputTokens?: number;
        costNanoerg?: number;
      } = {};

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed === "data: [DONE]") continue;
          if (!trimmed.startsWith("data: ")) continue;

          try {
            const json = JSON.parse(trimmed.slice(6));

            if (json.usage) {
              usageData = {
                inputTokens: json.usage.prompt_tokens,
                outputTokens: json.usage.completion_tokens,
                costNanoerg: json.usage.cost_nanoerg,
              };
            }

            const content = json.choices?.[0]?.delta?.content;
            if (content) {
              accumulated += content;
              updateLastAssistantMessage(accumulated);
              updateConvoAssistantMsg(convoId!, assistantMsgId, accumulated);
            }
          } catch {
            // Skip malformed SSE data
          }
        }
      }

      // Update usage data
      if (usageData.inputTokens !== undefined || usageData.outputTokens !== undefined) {
        updateLastAssistantMessageUsage(usageData);
        updateConvoAssistantMsg(
          convoId!,
          assistantMsgId,
          accumulated || "(No response content received)",
          {
            input: usageData.inputTokens ?? 0,
            output: usageData.outputTokens ?? 0,
          },
          usageData.costNanoerg,
        );
      }

      if (!accumulated) {
        const fallback = "(No response content received)";
        updateLastAssistantMessage(fallback);
        updateConvoAssistantMsg(convoId!, assistantMsgId, fallback);
      }
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") {
        const stopText = "(Generation stopped)";
        updateLastAssistantMessage(stopText);
        if (convoId) updateConvoAssistantMsg(convoId, assistantMsgId, stopText);
      } else {
        const errText = "Error: Failed to get response from the model. Please try again.";
        updateLastAssistantMessage(errText);
        if (convoId) updateConvoAssistantMsg(convoId, assistantMsgId, errText);
      }
    } finally {
      setGenerating(false);
      abortRef.current = null;
    }
  }, [
    prompt,
    selectedModel,
    isGenerating,
    activeConvoId,
    addMessage,
    addConvoMsg,
    createConvo,
    setGenerating,
    setPrompt,
    updateLastAssistantMessage,
    updateLastAssistantMessageUsage,
    updateConvoAssistantMsg,
  ]);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const handleNewChat = useCallback(() => {
    clearMessages();
    setPrompt("");
    setActiveConvo(null);
  }, [clearMessages, setPrompt, setActiveConvo]);

  // ── Keyboard shortcuts ──
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;

      // Ctrl/Cmd+N: new conversation
      if (mod && e.key === "n") {
        e.preventDefault();
        handleNewChat();
      }

      // Ctrl/Cmd+K: focus prompt input
      if (mod && e.key === "k") {
        e.preventDefault();
        const el = (window as unknown as Record<string, HTMLTextAreaElement | undefined>).__xergon_prompt;
        if (el) el.focus();
      }

      // Escape: stop generating
      if (e.key === "Escape" && isGenerating) {
        e.preventDefault();
        handleStop();
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handleNewChat, handleStop, isGenerating]);

  return (
    <section className="mx-auto max-w-7xl px-4 py-16 md:py-24">
      <div className="text-center mb-8">
        <h2 className="text-2xl font-bold text-surface-900 sm:text-3xl">
          Playground
        </h2>
        <p className="mt-2 text-surface-800/60">
          Try models directly in your browser. Compare responses side by side.
        </p>
      </div>

      <div className="rounded-2xl border border-surface-200 bg-surface-0 shadow-sm overflow-hidden">
        {/* Tab bar */}
        <div className="flex items-center border-b border-surface-200">
          <button
            onClick={() => setTab("chat")}
            className={cn(
              "px-5 py-3 text-sm font-medium transition-colors relative",
              tab === "chat"
                ? "text-brand-600"
                : "text-surface-800/40 hover:text-surface-800/70",
            )}
          >
            Chat
            {tab === "chat" && (
              <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-brand-600" />
            )}
          </button>
          <button
            onClick={() => setTab("compare")}
            className={cn(
              "px-5 py-3 text-sm font-medium transition-colors relative",
              tab === "compare"
                ? "text-brand-600"
                : "text-surface-800/40 hover:text-surface-800/70",
            )}
          >
            Compare
            {tab === "compare" && (
              <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-brand-600" />
            )}
          </button>
          <div className="flex-1" />
          <div className="pr-3 hidden sm:flex items-center gap-2">
            <RateLimitIndicator compact />
            <span className="text-xs text-surface-800/30">
              {models.length} models
            </span>
          </div>
        </div>

        {tab === "chat" ? (
          <div className="flex min-h-[500px] md:min-h-[600px]">
            {/* Sidebar */}
            <div className="hidden md:flex flex-col w-64 border-r border-surface-200 bg-surface-50/50">
              <ConversationList onNewChat={handleNewChat} />
            </div>
            {/* Mobile sidebar toggle + main area */}
            <div className="flex-1 flex flex-col">
              {/* Toolbar */}
              <div className="flex items-center justify-between border-b border-surface-200 px-3 py-2 md:px-4">
                <div className="flex items-center gap-2 md:gap-3">
                  {/* Mobile hamburger */}
                  <div className="md:hidden">
                    <ConversationList onNewChat={handleNewChat} />
                  </div>
                  <ModelSelector models={models} />
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={handleNewChat}
                    className="text-xs text-surface-800/40 hover:text-surface-800/70 transition-colors"
                  >
                    Clear
                  </button>
                </div>
              </div>

              {/* Response area */}
              <div className="flex-1 overflow-y-auto px-3 py-3 md:px-4 md:py-4">
                <ResponseArea />
              </div>

              {/* Prompt input */}
              <div className="border-t border-surface-200 p-3 md:p-4">
                <PromptBox onSubmit={handleSubmit} />
                <div className="mt-2 flex justify-end">
                  {isGenerating ? (
                    <button
                      onClick={handleStop}
                      className="min-h-[3rem] w-full rounded-lg bg-red-600 px-6 py-3 text-sm font-medium text-white active:bg-red-800 transition-colors md:min-h-0 md:w-auto md:px-4 md:py-2 md:hover:bg-red-700"
                    >
                      Stop generating
                    </button>
                  ) : (
                    <button
                      onClick={handleSubmit}
                      disabled={!prompt.trim() || !selectedModel || isGenerating}
                      className="min-h-[3rem] w-full rounded-lg bg-brand-600 px-6 py-3 text-sm font-medium text-white active:bg-brand-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors md:min-h-0 md:w-auto md:px-4 md:py-2 md:hover:bg-brand-700"
                    >
                      Send
                    </button>
                  )}
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="p-4 md:p-6">
            <ModelComparison models={models} />
          </div>
        )}
      </div>
    </section>
  );
}
