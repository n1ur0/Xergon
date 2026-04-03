"use client";

import { useCallback, useRef } from "react";
import { useSearchParams } from "next/navigation";
import { usePlaygroundStore } from "@/lib/stores/playground";
import { endpoints, type ModelInfo } from "@/lib/api/client";
import { FALLBACK_MODELS } from "@/lib/constants";
import { ModelSelector } from "@/components/ModelSelector";
import { PromptBox } from "@/components/PromptBox";
import { ResponseArea } from "@/components/ResponseArea";
import { useEffect, useState } from "react";

function SettingsIcon() {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

// Shared fallback from lib/constants — replaced by API call in production

export function PlaygroundPage() {
  const searchParams = useSearchParams();
  const selectedModel = usePlaygroundStore((s) => s.selectedModel);
  const prompt = usePlaygroundStore((s) => s.prompt);
  const addMessage = usePlaygroundStore((s) => s.addMessage);
  const updateLastAssistantMessage = usePlaygroundStore((s) => s.updateLastAssistantMessage);
  const updateLastAssistantMessageUsage = usePlaygroundStore((s) => s.updateLastAssistantMessageUsage);
  const setGenerating = usePlaygroundStore((s) => s.setGenerating);
  const setModel = usePlaygroundStore((s) => s.setModel);
  const clearMessages = usePlaygroundStore((s) => s.clearMessages);
  const isGenerating = usePlaygroundStore((s) => s.isGenerating);

  const [models, setModels] = useState(FALLBACK_MODELS);
  const abortRef = useRef<AbortController | null>(null);

  // Fetch available models on mount
  useEffect(() => {
    endpoints
      .listModels()
      .then((ms: ModelInfo[]) => {
        if (ms.length > 0) setModels(ms);
      })
      .catch(() => {
        // Keep fallback models on error (e.g. relay not running)
      });
  }, []);

  // Pre-select model from ?model= query param (e.g. from Models page "Try It")
  useEffect(() => {
    const modelParam = searchParams.get("model");
    if (modelParam && !selectedModel) {
      setModel(modelParam);
    }
  }, [searchParams, selectedModel, setModel]);

  const handleSubmit = useCallback(async () => {
    if (!prompt.trim() || !selectedModel || isGenerating) return;

    addMessage({ role: "user", content: prompt.trim(), model: selectedModel });
    setGenerating(true);

    // Add an empty assistant message that we'll stream into
    addMessage({ role: "assistant", content: "", model: selectedModel });

    const abort = new AbortController();
    abortRef.current = abort;

    try {
      const res = await endpoints.inferStream(
        {
          model: selectedModel,
          prompt: prompt.trim(),
        },
        abort.signal,
      );

      if (!res.ok) {
        updateLastAssistantMessage(
          `Error: Model returned status ${res.status}. Please try again.`,
        );
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) {
        updateLastAssistantMessage("Error: No response stream. Please try again.");
        return;
      }

      const decoder = new TextDecoder();
      let accumulated = "";
      let buffer = "";
      let usageData: { inputTokens?: number; outputTokens?: number; creditsCharged?: number } = {};

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        // Keep the last (possibly incomplete) line in the buffer
        buffer = lines.pop() || "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed === "data: [DONE]") continue;
          if (!trimmed.startsWith("data: ")) continue;

          try {
            const json = JSON.parse(trimmed.slice(6));

            // Check for usage data in the chunk (final chunk from relay)
            if (json.usage) {
              usageData = {
                inputTokens: json.usage.prompt_tokens,
                outputTokens: json.usage.completion_tokens,
                creditsCharged: json.usage.credits_charged,
              };
            }

            const content = json.choices?.[0]?.delta?.content;
            if (content) {
              accumulated += content;
              updateLastAssistantMessage(accumulated);
            }
          } catch {
            // Skip malformed SSE data
          }
        }
      }

      // Update usage data on the last assistant message if we got it
      if (usageData.inputTokens !== undefined || usageData.outputTokens !== undefined) {
        updateLastAssistantMessageUsage(usageData);
      }

      // If we got nothing from the stream, show a fallback
      if (!accumulated) {
        updateLastAssistantMessage("(No response content received)");
      }
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") {
        updateLastAssistantMessage("(Generation stopped)");
      } else {
        updateLastAssistantMessage(
          "Error: Failed to get response from the model. Please try again.",
        );
      }
    } finally {
      setGenerating(false);
      abortRef.current = null;
    }
  }, [prompt, selectedModel, isGenerating, addMessage, updateLastAssistantMessage, updateLastAssistantMessageUsage, setGenerating]);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  return (
    <div className="flex min-h-screen flex-col md:h-[calc(100vh-3.5rem)] md:min-h-0">
      {/* Toolbar — compact on mobile, full on desktop */}
      <div className="flex items-center justify-between border-b border-surface-200 px-3 py-2 md:px-4 md:py-2">
        <div className="flex items-center gap-2 md:gap-3">
          <ModelSelector models={models} />
          {/* Mobile: settings gear icon (placeholder — no-op for now) */}
          <button
            className="flex items-center justify-center rounded-lg p-2 text-surface-800/40 hover:bg-surface-100 hover:text-surface-800/70 transition-colors md:hidden"
            aria-label="Settings"
          >
            <SettingsIcon />
          </button>
          {/* Desktop: Clear button + model count */}
          <button
            onClick={clearMessages}
            className="hidden text-xs text-surface-800/40 hover:text-surface-800/70 transition-colors md:block"
          >
            Clear
          </button>
        </div>
        <div className="flex items-center gap-2">
          {/* Mobile: Clear button shown inline */}
          <button
            onClick={clearMessages}
            className="text-xs text-surface-800/40 hover:text-surface-800/70 transition-colors md:hidden"
          >
            Clear
          </button>
          <span className="hidden text-xs text-surface-800/30 md:inline">
            {models.length} models available
          </span>
        </div>
      </div>

      {/* Response area — scrollable with max-height */}
      <div className="flex-1 overflow-y-auto px-3 py-3 md:max-h-none md:px-4 md:py-4" style={{ maxHeight: 'calc(100vh - 14rem)' }}>
        <ResponseArea />
      </div>

      {/* Prompt input — full width, larger buttons on touch devices */}
      <div className="border-t border-surface-200 p-3 md:p-4">
        <PromptBox onSubmit={handleSubmit} />
        <div className="mt-2 flex justify-end">
          {isGenerating ? (
            <button
              onClick={handleStop}
              className="min-h-[3rem] w-full rounded-lg bg-red-600 px-6 py-3 text-sm font-medium text-white active:bg-red-800 transition-colors md:min-h-0 md:w-auto md:px-4 md:py-2 md:hover:bg-red-700"
            >
              Stop
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
  );
}
