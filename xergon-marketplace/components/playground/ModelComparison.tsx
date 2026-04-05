"use client";

import { useState, useRef, useCallback, useEffect } from "react";
import { MarkdownRenderer } from "@/components/ResponseArea";
import { TokenCounter } from "@/components/playground/TokenCounter";
import { cn } from "@/lib/utils";
import { API_BASE, getWalletPk } from "@/lib/api/config";

interface ModelComparisonProps {
  models: { id: string; name: string }[];
}

interface StreamState {
  content: string;
  isGenerating: boolean;
  done: boolean;
  error: string | null;
  tokens?: { promptTokens?: number; completionTokens?: number };
  costNanoerg?: number;
}

const STORAGE_KEY = "xergon-comparison-votes";

type VoteValue = "up" | "down";

function loadVotes(): Record<string, VoteValue> {
  if (typeof window === "undefined") return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function saveVotes(votes: Record<string, VoteValue>) {
  if (typeof window === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(votes));
}

export function ModelComparison({ models }: ModelComparisonProps) {
  const [modelA, setModelA] = useState("");
  const [modelB, setModelB] = useState("");
  const [prompt, setPrompt] = useState("");
  const [responses, setResponses] = useState<Record<string, StreamState>>({});
  const [layout, setLayout] = useState<"side" | "stack">("side");
  const [votes, setVotes] = useState<Record<string, VoteValue>>(loadVotes);
  const abortRefs = useRef<Record<string, AbortController | null>>({});

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      Object.values(abortRefs.current).forEach((c) => c?.abort());
    };
  }, []);

  // Auto-detect mobile and switch layout
  useEffect(() => {
    const mq = window.matchMedia("(max-width: 768px)");
    if (mq.matches) setLayout("stack");
    const handler = (e: MediaQueryListEvent) => setLayout(e.matches ? "stack" : "side");
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  const canSend = prompt.trim() && modelA && modelB && modelA !== modelB;

  const handleVote = useCallback((key: string, vote: VoteValue) => {
    setVotes((prev) => {
      const next = { ...prev, [key]: prev[key] === vote ? undefined as unknown as VoteValue : vote };
      // Clean up undefined values
      Object.keys(next).forEach((k) => {
        if (next[k] === undefined) delete next[k];
      });
      saveVotes(next as Record<string, VoteValue>);
      return next as Record<string, VoteValue>;
    });
  }, []);

  const handleSend = useCallback(async () => {
    if (!canSend) return;

    const pairs: [string, string][] = [[modelA, "a"], [modelB, "b"]];

    setResponses({
      a: { content: "", isGenerating: true, done: false, error: null },
      b: { content: "", isGenerating: true, done: false, error: null },
    });

    for (const [model, key] of pairs) {
      const abort = new AbortController();
      abortRefs.current[key] = abort;

      (async () => {
        const walletPk = getWalletPk();
        try {
          const res = await fetch(`${API_BASE}/chat/completions`, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              Accept: "text/event-stream",
              ...(walletPk ? { "X-Wallet-PK": walletPk } : {}),
            },
            body: JSON.stringify({
              model,
              messages: [{ role: "user", content: prompt.trim() }],
              stream: true,
            }),
            signal: abort.signal,
          });

          if (!res.ok) {
            setResponses((prev) => ({
              ...prev,
              [key]: {
                content: "",
                isGenerating: false,
                done: true,
                error: `Error ${res.status}`,
              },
            }));
            return;
          }

          const reader = res.body?.getReader();
          if (!reader) {
            setResponses((prev) => ({
              ...prev,
              [key]: {
                content: "",
                isGenerating: false,
                done: true,
                error: "No stream body",
              },
            }));
            return;
          }

          const decoder = new TextDecoder();
          let accumulated = "";
          let buffer = "";
          let usage: { promptTokens?: number; completionTokens?: number } = {};
          let cost: number | undefined;

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
                  usage = {
                    promptTokens: json.usage.prompt_tokens,
                    completionTokens: json.usage.completion_tokens,
                  };
                  cost = json.usage.cost_nanoerg;
                }
                const delta = json.choices?.[0]?.delta?.content;
                if (delta) {
                  accumulated += delta;
                  setResponses((prev) => ({
                    ...prev,
                    [key]: { ...prev[key], content: accumulated },
                  }));
                }
              } catch {
                // skip
              }
            }
          }

          setResponses((prev) => ({
            ...prev,
            [key]: {
              content: accumulated || "(No response)",
              isGenerating: false,
              done: true,
              error: null,
              tokens: usage,
              costNanoerg: cost,
            },
          }));
        } catch (err) {
          if (err instanceof DOMException && err.name === "AbortError") {
            setResponses((prev) => ({
              ...prev,
              [key]: {
                ...prev[key],
                isGenerating: false,
                done: true,
                error: null,
              },
            }));
          } else {
            setResponses((prev) => ({
              ...prev,
              [key]: {
                ...prev[key],
                isGenerating: false,
                done: true,
                error: "Request failed",
              },
            }));
          }
        } finally {
          abortRefs.current[key] = null;
        }
      })();
    }
  }, [canSend, prompt, modelA, modelB]);

  const handleStopAll = useCallback(() => {
    Object.values(abortRefs.current).forEach((c) => c?.abort());
  }, []);

  const isAnyGenerating = responses.a?.isGenerating || responses.b?.isGenerating;

  return (
    <div className="flex flex-col gap-4">
      {/* Controls */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:gap-3">
        <div className="flex-1">
          <label className="block text-xs font-medium text-surface-800/50 mb-1">
            Prompt
          </label>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="Enter a prompt to compare..."
            rows={2}
            className="w-full resize-none rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
          />
        </div>
        <div className="flex gap-2">
          <div className="w-36">
            <label className="block text-xs font-medium text-surface-800/50 mb-1">
              Model A
            </label>
            <ModelPicker models={models} value={modelA} onChange={setModelA} />
          </div>
          <div className="w-36">
            <label className="block text-xs font-medium text-surface-800/50 mb-1">
              Model B
            </label>
            <ModelPicker models={models} value={modelB} onChange={setModelB} />
          </div>
        </div>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2">
        {isAnyGenerating ? (
          <button
            onClick={handleStopAll}
            className="rounded-lg bg-red-600 px-4 py-2 text-xs font-medium text-white hover:bg-red-700 transition-colors"
          >
            Stop both
          </button>
        ) : (
          <button
            onClick={handleSend}
            disabled={!canSend}
            className="rounded-lg bg-brand-600 px-4 py-2 text-xs font-medium text-white hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Compare
          </button>
        )}
        <button
          onClick={() => setLayout(layout === "side" ? "stack" : "side")}
          className="rounded-lg border border-surface-200 px-3 py-2 text-xs text-surface-800/50 hover:bg-surface-50 transition-colors"
          title="Toggle layout"
        >
          {layout === "side" ? "Stack" : "Side by side"}
        </button>
      </div>

      {/* Responses */}
      {(responses.a || responses.b) && (
        <div
          className={cn(
            "grid gap-4",
            layout === "side" ? "grid-cols-1 md:grid-cols-2" : "grid-cols-1",
          )}
        >
          <ResponsePanel
            label={modelA || "Model A"}
            state={responses.a}
            voteKey={`a-${prompt.slice(0, 50)}`}
            vote={votes[`a-${prompt.slice(0, 50)}`]}
            onVote={handleVote}
          />
          <ResponsePanel
            label={modelB || "Model B"}
            state={responses.b}
            voteKey={`b-${prompt.slice(0, 50)}`}
            vote={votes[`b-${prompt.slice(0, 50)}`]}
            onVote={handleVote}
          />
        </div>
      )}
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────

function ModelPicker({
  models,
  value,
  onChange,
}: {
  models: { id: string; name: string }[];
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full rounded-lg border border-surface-200 bg-surface-0 px-2 py-1.5 text-xs focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
    >
      <option value="" disabled>
        Select...
      </option>
      {models.map((m) => (
        <option key={m.id} value={m.id}>
          {m.name}
        </option>
      ))}
    </select>
  );
}

function ResponsePanel({
  label,
  state,
  voteKey,
  vote,
  onVote,
}: {
  label: string;
  state?: StreamState;
  voteKey: string;
  vote?: VoteValue;
  onVote: (key: string, vote: VoteValue) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current && state?.content) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [state?.content]);

  if (!state) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
        <div className="text-xs font-medium text-surface-800/40 mb-2">{label}</div>
        <div className="text-xs text-surface-800/20">Waiting...</div>
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-surface-100">
        <span className="text-xs font-medium text-surface-800/60">{label}</span>
        <div className="flex items-center gap-1">
          {state.done && state.content && (
            <>
              <button
                onClick={() => onVote(voteKey, "up")}
                className={cn(
                  "rounded p-1 transition-colors",
                  vote === "up"
                    ? "text-green-600 bg-green-50"
                    : "text-surface-800/30 hover:bg-surface-50 hover:text-surface-800/60",
                )}
                title="Good response"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M7 10v12" />
                  <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2h0a3.13 3.13 0 0 1 3 3.88Z" />
                </svg>
              </button>
              <button
                onClick={() => onVote(voteKey, "down")}
                className={cn(
                  "rounded p-1 transition-colors",
                  vote === "down"
                    ? "text-red-600 bg-red-50"
                    : "text-surface-800/30 hover:bg-surface-50 hover:text-surface-800/60",
                )}
                title="Bad response"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M17 14V2" />
                  <path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22h0a3.13 3.13 0 0 1-3-3.88Z" />
                </svg>
              </button>
            </>
          )}
          <TokenCounter
            promptTokens={state.tokens?.promptTokens}
            completionTokens={state.tokens?.completionTokens}
            costNanoerg={state.costNanoerg}
          />
        </div>
      </div>

      {/* Content */}
      <div
        ref={containerRef}
        className="flex-1 overflow-y-auto p-4 min-h-[200px] max-h-[400px]"
      >
        {state.error ? (
          <div className="text-xs text-red-500">{state.error}</div>
        ) : state.isGenerating && !state.content ? (
          <div className="flex items-center gap-1.5 text-surface-800/40">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:0ms]" />
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:150ms]" />
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:300ms]" />
            <span className="ml-1 text-xs">Thinking...</span>
          </div>
        ) : (
          <MarkdownRenderer content={state.content || ""} />
        )}
      </div>
    </div>
  );
}
