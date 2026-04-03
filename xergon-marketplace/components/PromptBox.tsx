"use client";

import { usePlaygroundStore } from "@/lib/stores/playground";
import { cn } from "@/lib/utils";

interface PromptBoxProps {
  onSubmit?: () => void;
}

function isMac() {
  if (typeof navigator === "undefined") return false;
  return navigator.platform.includes("Mac") || navigator.userAgent.includes("Mac");
}

export function PromptBox({ onSubmit }: PromptBoxProps) {
  const prompt = usePlaygroundStore((s) => s.prompt);
  const setPrompt = usePlaygroundStore((s) => s.setPrompt);
  const isGenerating = usePlaygroundStore((s) => s.isGenerating);

  const shortcutHint = isMac() ? "⌘+Enter" : "Ctrl+Enter";

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      onSubmit?.();
    }
  };

  return (
    <div className="relative">
      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={`Ask anything... (${shortcutHint} to send)`}
        rows={4}
        disabled={isGenerating}
        className={cn(
          "w-full resize-none rounded-xl border border-surface-200 bg-surface-0 p-3 text-sm md:p-4",
          "placeholder:text-surface-800/30",
          "focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500",
          "disabled:opacity-50 transition-opacity"
        )}
      />
      <div className="absolute bottom-2 right-3 text-xs text-surface-800/30">
        {shortcutHint}
      </div>
    </div>
  );
}
