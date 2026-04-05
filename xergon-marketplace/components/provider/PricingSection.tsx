"use client";

import { useState, useEffect, useCallback } from "react";
import {
  fetchPricingData,
  updateModelPrice,
  type PricingData,
} from "@/lib/api/provider";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function nanoergToErg(nanoerg: number): string {
  return (nanoerg / 1e9).toFixed(6);
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function PricingSkeleton() {
  return (
    <div className="animate-pulse space-y-3">
      <div className="h-4 bg-surface-200 rounded w-48" />
      <div className="h-10 bg-surface-100 rounded" />
      <div className="space-y-2">
        {[0, 1, 2].map((i) => (
          <div key={i} className="h-12 bg-surface-100 rounded" />
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// PricingSection component
// ---------------------------------------------------------------------------

export function PricingSection() {
  const [pricing, setPricing] = useState<PricingData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Editing state
  const [editingModel, setEditingModel] = useState<string | null>(null);
  const [editValue, setEditValue] = useState<string>("");
  const [saving, setSaving] = useState(false);

  // Add model state
  const [addingModel, setAddingModel] = useState(false);
  const [newModelName, setNewModelName] = useState("");
  const [newModelPrice, setNewModelPrice] = useState("");

  const load = useCallback(async () => {
    try {
      const data = await fetchPricingData();
      setPricing(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load pricing");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleStartEdit = (model: string, currentPrice: number) => {
    setEditingModel(model);
    setEditValue(String(currentPrice));
    setAddingModel(false);
  };

  const handleCancelEdit = () => {
    setEditingModel(null);
    setEditValue("");
  };

  const handleSaveEdit = async () => {
    if (!editingModel) return;
    const price = parseInt(editValue, 10);
    if (isNaN(price) || price < 0) {
      toast.error("Invalid price", {
        description: "Price must be a non-negative integer (nanoERG).",
      });
      return;
    }

    setSaving(true);
    try {
      const updated = await updateModelPrice(editingModel, price);
      setPricing(updated);
      setEditingModel(null);
      setEditValue("");
      toast.success("Price updated", {
        description: `${editingModel}: ${price.toLocaleString()} nanoERG/1M tokens`,
      });
    } catch (err) {
      toast.error("Failed to update price", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
    } finally {
      setSaving(false);
    }
  };

  const handleAddModel = async () => {
    const model = newModelName.trim();
    const price = parseInt(newModelPrice, 10);
    if (!model) {
      toast.error("Model name is required");
      return;
    }
    if (isNaN(price) || price < 0) {
      toast.error("Invalid price", {
        description: "Price must be a non-negative integer (nanoERG).",
      });
      return;
    }

    setSaving(true);
    try {
      const updated = await updateModelPrice(model, price);
      setPricing(updated);
      setAddingModel(false);
      setNewModelName("");
      setNewModelPrice("");
      toast.success("Model added", {
        description: `${model}: ${price.toLocaleString()} nanoERG/1M tokens`,
      });
    } catch (err) {
      toast.error("Failed to add model", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
    } finally {
      setSaving(false);
    }
  };

  const modelEntries = pricing
    ? Object.entries(pricing.models).sort(([a], [b]) => a.localeCompare(b))
    : [];

  return (
    <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="font-semibold flex items-center gap-2">
          <span className="text-lg">&#x1F4B3;</span> Model Pricing
        </h2>
        <div className="flex items-center gap-2">
          {!addingModel && (
            <button
              onClick={() => {
                setAddingModel(true);
                setEditingModel(null);
              }}
              className="px-3 py-1.5 rounded-lg text-xs font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors"
            >
              + Add Model
            </button>
          )}
          <button
            onClick={load}
            className="px-3 py-1.5 rounded-lg text-xs font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
          >
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="text-sm text-danger-600 bg-danger-50 border border-danger-200 rounded-lg p-3 mb-4">
          {error}
        </div>
      )}

      {loading ? (
        <PricingSkeleton />
      ) : pricing ? (
        <div>
          {/* Default price info */}
          <div className="flex items-center justify-between p-3 rounded-lg bg-surface-50 border border-surface-100 text-sm mb-4">
            <div>
              <span className="text-surface-800/50 text-xs block mb-0.5">
                Default Price
              </span>
              <span className="font-medium">
                {pricing.default_price_per_1m_tokens.toLocaleString()} nanoERG
                <span className="text-surface-800/40 mx-1">/</span>
                1M tokens
              </span>
            </div>
            <span className="text-surface-800/50 font-mono text-xs">
              = {nanoergToErg(pricing.default_price_per_1m_tokens)} ERG
            </span>
          </div>

          {/* Model pricing table */}
          {modelEntries.length === 0 && !addingModel ? (
            <p className="text-sm text-surface-800/40 py-4">
              No per-model pricing overrides set. All models use the default
              price. Click &quot;+ Add Model&quot; to set model-specific pricing.
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-surface-100 text-left">
                    <th className="pb-2 text-xs font-medium uppercase tracking-wide text-surface-800/50">
                      Model
                    </th>
                    <th className="pb-2 text-xs font-medium uppercase tracking-wide text-surface-800/50 text-right">
                      Price (nanoERG/1M)
                    </th>
                    <th className="pb-2 text-xs font-medium uppercase tracking-wide text-surface-800/50 text-right">
                      Price (ERG)
                    </th>
                    <th className="pb-2 text-xs font-medium uppercase tracking-wide text-surface-800/50 text-right">
                      Actions
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-surface-50">
                  {modelEntries.map(([model, price]) => (
                    <tr key={model}>
                      <td className="py-2.5 font-medium">{model}</td>
                      <td className="py-2.5 text-right">
                        {editingModel === model ? (
                          <input
                            type="number"
                            value={editValue}
                            onChange={(e) => setEditValue(e.target.value)}
                            className="w-32 px-2 py-1 text-right text-sm border border-surface-300 rounded-md bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-brand-500 font-mono"
                            min={0}
                            autoFocus
                            onKeyDown={(e) => {
                              if (e.key === "Enter") handleSaveEdit();
                              if (e.key === "Escape") handleCancelEdit();
                            }}
                          />
                        ) : (
                          <span className="font-mono">
                            {price.toLocaleString()}
                          </span>
                        )}
                      </td>
                      <td className="py-2.5 text-right text-surface-800/60 font-mono">
                        {nanoergToErg(price)}
                      </td>
                      <td className="py-2.5 text-right">
                        {editingModel === model ? (
                          <div className="flex items-center justify-end gap-1">
                            <button
                              onClick={handleSaveEdit}
                              disabled={saving}
                              className="px-2.5 py-1 rounded text-xs font-medium bg-accent-500 text-white hover:bg-accent-600 transition-colors disabled:opacity-50"
                            >
                              {saving ? "Saving..." : "Save"}
                            </button>
                            <button
                              onClick={handleCancelEdit}
                              disabled={saving}
                              className="px-2.5 py-1 rounded text-xs font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
                            >
                              Cancel
                            </button>
                          </div>
                        ) : (
                          <button
                            onClick={() => handleStartEdit(model, price)}
                            className="px-2.5 py-1 rounded text-xs font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
                          >
                            Edit
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}

                  {/* Add model row */}
                  {addingModel && (
                    <tr className="bg-brand-50/30">
                      <td className="py-2.5">
                        <input
                          type="text"
                          value={newModelName}
                          onChange={(e) => setNewModelName(e.target.value)}
                          placeholder="e.g. llama-3.1-8b"
                          className="w-full px-2 py-1 text-sm border border-surface-300 rounded-md bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-brand-500"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === "Escape") {
                              setAddingModel(false);
                              setNewModelName("");
                              setNewModelPrice("");
                            }
                          }}
                        />
                      </td>
                      <td className="py-2.5 text-right">
                        <input
                          type="number"
                          value={newModelPrice}
                          onChange={(e) => setNewModelPrice(e.target.value)}
                          placeholder={String(
                            pricing.default_price_per_1m_tokens
                          )}
                          className="w-32 px-2 py-1 text-right text-sm border border-surface-300 rounded-md bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-brand-500 font-mono"
                          min={0}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleAddModel();
                          }}
                        />
                      </td>
                      <td className="py-2.5 text-right text-surface-800/40 font-mono text-xs">
                        {newModelPrice && !isNaN(parseInt(newModelPrice, 10))
                          ? nanoergToErg(parseInt(newModelPrice, 10))
                          : "--"}
                      </td>
                      <td className="py-2.5 text-right">
                        <div className="flex items-center justify-end gap-1">
                          <button
                            onClick={handleAddModel}
                            disabled={saving}
                            className="px-2.5 py-1 rounded text-xs font-medium bg-accent-500 text-white hover:bg-accent-600 transition-colors disabled:opacity-50"
                          >
                            {saving ? "Adding..." : "Add"}
                          </button>
                          <button
                            onClick={() => {
                              setAddingModel(false);
                              setNewModelName("");
                              setNewModelPrice("");
                            }}
                            disabled={saving}
                            className="px-2.5 py-1 rounded text-xs font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
                          >
                            Cancel
                          </button>
                        </div>
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          )}

          {modelEntries.length > 0 && (
            <p className="text-xs text-surface-800/40 mt-3">
              {modelEntries.length} model{modelEntries.length !== 1 ? "s" : ""}{" "}
              with custom pricing. Changes are persisted to the agent config file.
            </p>
          )}
        </div>
      ) : null}
    </section>
  );
}
