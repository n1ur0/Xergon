"use client";

import { usePlaygroundStore } from "@/lib/stores/playground";
import { cn } from "@/lib/utils";

interface ModelSelectorProps {
  models: { id: string; name: string }[];
}

export function ModelSelector({ models }: ModelSelectorProps) {
  const selectedModel = usePlaygroundStore((s) => s.selectedModel);
  const setModel = usePlaygroundStore((s) => s.setModel);

  return (
    <select
      value={selectedModel}
      onChange={(e) => setModel(e.target.value)}
      className={cn(
        "rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm",
        "focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500",
        "transition-shadow"
      )}
    >
      <option value="" disabled>
        Select a model...
      </option>
      {models.map((m) => (
        <option key={m.id} value={m.id}>
          {m.name}
        </option>
      ))}
    </select>
  );
}
