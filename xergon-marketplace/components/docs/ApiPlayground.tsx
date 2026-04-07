"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { cn } from "@/lib/utils";

/* ─── Types ─── */
interface HistoryEntry {
  id: string;
  timestamp: string;
  endpoint: string;
  method: string;
  status: number;
  duration: number;
  requestBody: string;
  responseBody: string;
}

interface Preset {
  id: string;
  name: string;
  endpoint: string;
  method: string;
  body: string;
}

/* ─── Endpoints config ─── */
const ENDPOINTS = [
  { path: "/v1/chat/completions", method: "POST", label: "Chat Completions" },
  { path: "/v1/completions", method: "POST", label: "Completions" },
  { path: "/v1/embeddings", method: "POST", label: "Embeddings" },
  { path: "/v1/models", method: "GET", label: "List Models" },
];

const DEFAULT_BODIES: Record<string, string> = {
  "/v1/chat/completions": JSON.stringify(
    {
      model: "llama-3.1-8b",
      messages: [
        { role: "system", content: "You are a helpful assistant." },
        { role: "user", content: "What is Xergon?" },
      ],
      temperature: 0.7,
      max_tokens: 512,
      stream: false,
    },
    null,
    2
  ),
  "/v1/completions": JSON.stringify(
    {
      model: "llama-3.1-8b",
      prompt: "The future of decentralized AI is",
      max_tokens: 128,
      temperature: 0.7,
    },
    null,
    2
  ),
  "/v1/embeddings": JSON.stringify(
    {
      model: "llama-3.1-8b",
      input: "Hello world",
    },
    null,
    2
  ),
  "/v1/models": "",
};

const DEFAULT_PRESETS: Preset[] = [
  {
    id: "basic-chat",
    name: "Basic Chat",
    endpoint: "/v1/chat/completions",
    method: "POST",
    body: JSON.stringify(
      { model: "llama-3.1-8b", messages: [{ role: "user", content: "Hello!" }], max_tokens: 256 },
      null,
      2
    ),
  },
  {
    id: "streaming",
    name: "Streaming Chat",
    endpoint: "/v1/chat/completions",
    method: "POST",
    body: JSON.stringify(
      { model: "llama-3.1-8b", messages: [{ role: "user", content: "Tell me a story." }], stream: true },
      null,
      2
    ),
  },
  {
    id: "json-mode",
    name: "JSON Mode",
    endpoint: "/v1/chat/completions",
    method: "POST",
    body: JSON.stringify(
      {
        model: "llama-3.1-8b",
        messages: [{ role: "user", content: "List 3 colors as JSON array." }],
        response_format: { type: "json_object" },
        max_tokens: 128,
      },
      null,
      2
    ),
  },
  {
    id: "list-models",
    name: "List Models",
    endpoint: "/v1/models",
    method: "GET",
    body: "",
  },
];

const MODEL_OPTIONS = [
  "llama-3.1-8b",
  "llama-3.1-70b",
  "llama-3.1-405b",
  "llama-3.3-70b",
  "mixtral-8x7b",
  "mistral-7b",
  "qwen-2.5-72b",
  "deepseek-coder-v2",
  "phi-4",
  "command-r-plus",
];

/* ─── JSON syntax highlighter (simple) ─── */
function highlightJson(json: string): string {
  try {
    const obj = JSON.parse(json);
    const formatted = JSON.stringify(obj, null, 2);
    return formatted
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"([^"]+)":/g, '<span class="text-purple-400">"$1"</span>:')
      .replace(/: "([^"]*)"/g, ': <span class="text-emerald-400">"$1"</span>')
      .replace(/: (\d+\.?\d*)/g, ': <span class="text-amber-400">$1</span>')
      .replace(/: (true|false|null)/g, ': <span class="text-sky-400">$1</span>');
  } catch {
    return json
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }
}

/* ─── Main component ─── */
export default function ApiPlayground({ defaultModel }: { defaultModel?: string }) {
  const [selectedEndpoint, setSelectedEndpoint] = useState(ENDPOINTS[0].path);
  const [headers, setHeaders] = useState(
    JSON.stringify(
      [
        { key: "Content-Type", value: "application/json", enabled: true },
        { key: "Authorization", value: "Bearer YOUR_API_KEY", enabled: true },
      ],
      null,
      2
    )
  );
  const [body, setBody] = useState(DEFAULT_BODIES["/v1/chat/completions"]);
  const [response, setResponse] = useState("");
  const [responseHeaders, setResponseHeaders] = useState("");
  const [responseStatus, setResponseStatus] = useState<number | null>(null);
  const [responseTime, setResponseTime] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [showHistory, setShowHistory] = useState(false);
  const [showPresets, setShowPresets] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [activeTab, setActiveTab] = useState<"body" | "headers">("body");
  const [modelDropdownOpen, setModelDropdownOpen] = useState(false);
  const [modelSearch, setModelSearch] = useState("");
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Set default model if provided
  useEffect(() => {
    if (defaultModel) {
      try {
        const parsed = JSON.parse(body);
        parsed.model = defaultModel;
        setBody(JSON.stringify(parsed, null, 2));
      } catch {
        // ignore
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [defaultModel]);

  // Close dropdown on click outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setModelDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const endpointConfig = ENDPOINTS.find((e) => e.path === selectedEndpoint);

  const handleEndpointChange = (path: string) => {
    setSelectedEndpoint(path);
    setBody(DEFAULT_BODIES[path] || "");
    setResponse("");
    setResponseStatus(null);
    setResponseTime(null);
    setError("");
  };

  const handleModelSelect = (model: string) => {
    try {
      const parsed = JSON.parse(body);
      parsed.model = model;
      setBody(JSON.stringify(parsed, null, 2));
    } catch {
      // ignore
    }
    setModelDropdownOpen(false);
    setModelSearch("");
  };

  const handleSend = async () => {
    setLoading(true);
    setError("");
    setResponse("");
    setResponseStatus(null);
    setResponseTime(null);

    const start = performance.now();

    try {
      const payload: Record<string, unknown> = { endpoint: selectedEndpoint };
      if (body && endpointConfig?.method === "POST") {
        try {
          payload.body = JSON.parse(body);
        } catch {
          setError("Invalid JSON in request body");
          setLoading(false);
          return;
        }
      }

      const res = await fetch("/api/docs/playground", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      const elapsed = Math.round(performance.now() - start);
      const data = await res.json();

      const responseStr = JSON.stringify(data, null, 2);
      setResponse(responseStr);
      setResponseStatus(res.status);
      setResponseTime(elapsed);
      setResponseHeaders(
        JSON.stringify(
          { "content-type": "application/json", "x-response-time": `${elapsed}ms` },
          null,
          2
        )
      );

      // Add to history
      const entry: HistoryEntry = {
        id: Date.now().toString(),
        timestamp: new Date().toISOString(),
        endpoint: selectedEndpoint,
        method: endpointConfig?.method || "GET",
        status: res.status,
        duration: elapsed,
        requestBody: body || "(no body)",
        responseBody: responseStr,
      };
      setHistory((prev) => [entry, ...prev].slice(0, 50));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Request failed");
    } finally {
      setLoading(false);
    }
  };

  const handleReplay = (entry: HistoryEntry) => {
    setSelectedEndpoint(entry.endpoint);
    setBody(entry.requestBody);
    setResponse(entry.responseBody);
    setResponseStatus(entry.status);
    setResponseTime(entry.duration);
    setShowHistory(false);
  };

  const handleLoadPreset = (preset: Preset) => {
    setSelectedEndpoint(preset.endpoint);
    setBody(preset.body);
    setResponse("");
    setResponseStatus(null);
    setResponseTime(null);
    setError("");
    setShowPresets(false);
  };

  const handleSavePreset = () => {
    const name = prompt("Preset name:");
    if (!name) return;
    const newPreset: Preset = {
      id: `custom-${Date.now()}`,
      name,
      endpoint: selectedEndpoint,
      method: endpointConfig?.method || "GET",
      body,
    };
    DEFAULT_PRESETS.push(newPreset);
    setShowPresets(true);
  };

  const handleCurlExport = () => {
    const curlHeaders = `  -H "Content-Type: application/json" \\\n  -H "Authorization: Bearer YOUR_API_KEY"`;
    const curlBody = body ? `\n  -d '${body}'` : "";
    const curl = `curl -X ${endpointConfig?.method || "GET"} https://relay.xergon.network${selectedEndpoint} \\\n${curlHeaders}${curlBody}`;
    navigator.clipboard.writeText(curl);
    alert("cURL command copied to clipboard!");
  };

  const filteredModels = MODEL_OPTIONS.filter((m) =>
    m.toLowerCase().includes(modelSearch.toLowerCase())
  );

  const methodColor: Record<string, string> = {
    GET: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    POST: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <h1 className="text-3xl font-bold text-surface-900 mb-2">API Playground</h1>
          <p className="text-surface-800/60">
            Send test requests to the Xergon Relay API. Responses are mocked for demo purposes.
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => setShowPresets(!showPresets)}
            className="px-3 py-2 rounded-lg border border-surface-200 text-xs font-medium text-surface-800 hover:border-brand-300 transition-colors"
          >
            Presets
          </button>
          <button
            onClick={() => setShowHistory(!showHistory)}
            className="px-3 py-2 rounded-lg border border-surface-200 text-xs font-medium text-surface-800 hover:border-brand-300 transition-colors"
          >
            History ({history.length})
          </button>
          <button
            onClick={handleSavePreset}
            className="px-3 py-2 rounded-lg border border-surface-200 text-xs font-medium text-surface-800 hover:border-brand-300 transition-colors"
          >
            Save Preset
          </button>
        </div>
      </div>

      <div className="flex gap-6">
        {/* ─── Main area ─── */}
        <div className="flex-1 min-w-0 space-y-4">
          {/* Endpoint selector */}
          <div className="flex gap-3 items-center">
            <div className="flex rounded-lg border border-surface-200 overflow-hidden">
              {ENDPOINTS.map((ep) => (
                <button
                  key={ep.path}
                  onClick={() => handleEndpointChange(ep.path)}
                  className={cn(
                    "px-3 py-2 text-xs font-medium transition-colors whitespace-nowrap",
                    selectedEndpoint === ep.path
                      ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                      : "bg-surface-0 text-surface-800/60 hover:text-surface-900"
                  )}
                >
                  <span className={cn("inline-block px-1.5 py-0.5 rounded text-[10px] font-mono font-bold mr-1.5", methodColor[ep.method])}>
                    {ep.method}
                  </span>
                  {ep.label}
                </button>
              ))}
            </div>
            <div className="flex-1 font-mono text-sm text-surface-800/40 bg-surface-50 px-3 py-2 rounded-lg">
              https://relay.xergon.network{selectedEndpoint}
            </div>
          </div>

          {/* Tabs: Body / Headers */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <div className="inline-flex rounded-lg border border-surface-200 overflow-hidden">
                <button
                  onClick={() => setActiveTab("body")}
                  className={cn(
                    "px-3 py-1.5 text-xs font-medium transition-colors",
                    activeTab === "body"
                      ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                      : "bg-surface-0 text-surface-800/60"
                  )}
                >
                  Body
                </button>
                <button
                  onClick={() => setActiveTab("headers")}
                  className={cn(
                    "px-3 py-1.5 text-xs font-medium transition-colors",
                    activeTab === "headers"
                      ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                      : "bg-surface-0 text-surface-800/60"
                  )}
                >
                  Headers
                </button>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={handleCurlExport}
                  className="px-2 py-1 text-[10px] font-mono rounded bg-surface-100 text-surface-800/50 hover:bg-surface-200 transition-colors"
                >
                  cURL
                </button>
              </div>
            </div>

            {/* Model autocomplete (shown for body tab on POST endpoints) */}
            {activeTab === "body" && endpointConfig?.method === "POST" && (
              <div className="relative mb-2" ref={dropdownRef}>
                <button
                  onClick={() => setModelDropdownOpen(!modelDropdownOpen)}
                  className="flex items-center gap-2 px-3 py-1.5 rounded-lg border border-surface-200 text-xs bg-surface-0 hover:border-brand-300 transition-colors"
                >
                  <span className="text-surface-800/40">Model:</span>
                  <span className="font-mono text-surface-900 font-medium">
                    {(() => {
                      try {
                        return JSON.parse(body).model || "Select model";
                      } catch {
                        return "Select model";
                      }
                    })()}
                  </span>
                  <svg className="w-3 h-3 text-surface-800/40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M6 9l6 6 6-6" />
                  </svg>
                </button>
                {modelDropdownOpen && (
                  <div className="absolute z-20 top-full left-0 mt-1 w-64 rounded-xl border border-surface-200 bg-surface-0 shadow-lg overflow-hidden">
                    <div className="p-2 border-b border-surface-200">
                      <input
                        type="text"
                        placeholder="Search models..."
                        value={modelSearch}
                        onChange={(e) => setModelSearch(e.target.value)}
                        className="w-full px-2 py-1.5 text-xs rounded-lg border border-surface-200 bg-surface-50 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
                        autoFocus
                      />
                    </div>
                    <ul className="max-h-48 overflow-y-auto py-1">
                      {filteredModels.map((m) => (
                        <li key={m}>
                          <button
                            onClick={() => handleModelSelect(m)}
                            className="w-full text-left px-3 py-1.5 text-xs font-mono hover:bg-surface-100 transition-colors text-surface-800"
                          >
                            {m}
                          </button>
                        </li>
                      ))}
                      {filteredModels.length === 0 && (
                        <li className="px-3 py-2 text-xs text-surface-800/40">No models found</li>
                      )}
                    </ul>
                  </div>
                )}
              </div>
            )}

            {/* Editor */}
            <div className="relative">
              <textarea
                ref={bodyRef}
                value={activeTab === "body" ? body : headers}
                onChange={(e) => {
                  if (activeTab === "body") setBody(e.target.value);
                  else setHeaders(e.target.value);
                }}
                className="w-full h-64 bg-surface-950 text-surface-200 rounded-xl p-4 text-sm font-mono resize-y focus:outline-none focus:ring-2 focus:ring-brand-500/30"
                spellCheck={false}
              />
              {/* JSON validation indicator */}
              {activeTab === "body" && body && (
                <div className="absolute bottom-2 right-2">
                  <span
                    className={cn(
                      "text-[10px] font-mono px-1.5 py-0.5 rounded",
                      (() => {
                        try {
                          JSON.parse(body);
                          return "bg-emerald-900/50 text-emerald-400";
                        } catch {
                          return "bg-red-900/50 text-red-400";
                        }
                      })()
                    )}
                  >
                    {(() => {
                      try {
                        JSON.parse(body);
                        return "Valid JSON";
                      } catch {
                        return "Invalid JSON";
                      }
                    })()}
                  </span>
                </div>
              )}
            </div>
          </div>

          {/* Send button */}
          <div className="flex items-center gap-3">
            <button
              onClick={handleSend}
              disabled={loading}
              className={cn(
                "px-6 py-2.5 rounded-lg text-sm font-medium transition-colors",
                loading
                  ? "bg-surface-200 text-surface-800/40 cursor-not-allowed"
                  : "bg-brand-600 text-white hover:bg-brand-700"
              )}
            >
              {loading ? (
                <span className="flex items-center gap-2">
                  <span className="h-4 w-4 rounded-full border-2 border-brand-300 border-t-transparent animate-spin" />
                  Sending...
                </span>
              ) : (
                `Send ${endpointConfig?.method || "GET"} Request`
              )}
            </button>
          </div>

          {/* Error */}
          {error && (
            <div className="p-4 rounded-xl bg-danger-500/10 border border-danger-500/20 text-danger-600 text-sm">
              {error}
            </div>
          )}

          {/* Response */}
          {response && (
            <div>
              {/* Response meta */}
              <div className="flex items-center gap-3 mb-3">
                <h3 className="text-sm font-semibold text-surface-900">Response</h3>
                {responseStatus && (
                  <span
                    className={cn(
                      "px-2 py-0.5 rounded-full text-[11px] font-mono font-bold",
                      responseStatus >= 200 && responseStatus < 300
                        ? "bg-emerald-100 text-emerald-700"
                        : "bg-red-100 text-red-700"
                    )}
                  >
                    {responseStatus}
                  </span>
                )}
                {responseTime !== null && (
                  <span className="px-2 py-0.5 rounded-full text-[11px] font-mono bg-surface-100 text-surface-800/60">
                    {responseTime}ms
                  </span>
                )}
                <button
                  onClick={() => {
                    navigator.clipboard.writeText(response);
                  }}
                  className="ml-auto px-2 py-1 text-[10px] font-mono rounded bg-surface-100 text-surface-800/50 hover:bg-surface-200 transition-colors"
                >
                  Copy
                </button>
              </div>

              {/* Response body */}
              <pre
                className="bg-surface-950 text-surface-200 rounded-xl p-4 overflow-x-auto text-sm font-mono max-h-96 overflow-y-auto"
                dangerouslySetInnerHTML={{ __html: highlightJson(response) }}
              />

              {/* Token usage (if present) */}
              {(() => {
                try {
                  const parsed = JSON.parse(response);
                  if (parsed.usage) {
                    return (
                      <div className="mt-3 flex gap-4">
                        {[
                          { label: "Prompt Tokens", value: parsed.usage.prompt_tokens },
                          { label: "Completion Tokens", value: parsed.usage.completion_tokens },
                          { label: "Total Tokens", value: parsed.usage.total_tokens },
                        ].map((t) => (
                          <div key={t.label} className="px-3 py-2 rounded-lg bg-surface-50 border border-surface-200">
                            <div className="text-[10px] text-surface-800/40 uppercase tracking-wider">{t.label}</div>
                            <div className="text-sm font-mono font-semibold text-surface-900">{t.value}</div>
                          </div>
                        ))}
                      </div>
                    );
                  }
                } catch {
                  // ignore
                }
                return null;
              })()}

              {/* Response headers */}
              {responseHeaders && (
                <details className="mt-3">
                  <summary className="text-xs text-surface-800/40 cursor-pointer hover:text-surface-800/60">
                    Response Headers
                  </summary>
                  <pre className="mt-2 bg-surface-950 text-surface-200 rounded-xl p-4 overflow-x-auto text-xs font-mono">
                    {responseHeaders}
                  </pre>
                </details>
              )}
            </div>
          )}
        </div>

        {/* ─── History / Presets sidebar ─── */}
        {(showHistory || showPresets) && (
          <div className="w-72 shrink-0">
            <div className="sticky top-8">
              {showPresets && (
                <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
                  <div className="px-4 py-3 border-b border-surface-200 bg-surface-50">
                    <h3 className="text-sm font-semibold text-surface-900">Presets</h3>
                  </div>
                  <ul className="divide-y divide-surface-200 max-h-96 overflow-y-auto">
                    {DEFAULT_PRESETS.map((preset) => (
                      <li key={preset.id}>
                        <button
                          onClick={() => handleLoadPreset(preset)}
                          className="w-full text-left px-4 py-3 hover:bg-surface-50 transition-colors"
                        >
                          <div className="text-sm font-medium text-surface-900">{preset.name}</div>
                          <div className="flex items-center gap-1.5 mt-1">
                            <span className={cn("px-1.5 py-0.5 rounded text-[10px] font-mono font-bold", methodColor[preset.method])}>
                              {preset.method}
                            </span>
                            <span className="text-[11px] font-mono text-surface-800/40">{preset.endpoint}</span>
                          </div>
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {showHistory && (
                <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
                  <div className="px-4 py-3 border-b border-surface-200 bg-surface-50">
                    <h3 className="text-sm font-semibold text-surface-900">Request History</h3>
                  </div>
                  {history.length === 0 ? (
                    <div className="px-4 py-8 text-center text-xs text-surface-800/40">
                      No requests yet
                    </div>
                  ) : (
                    <ul className="divide-y divide-surface-200 max-h-96 overflow-y-auto">
                      {history.map((entry) => (
                        <li key={entry.id}>
                          <button
                            onClick={() => handleReplay(entry)}
                            className="w-full text-left px-4 py-3 hover:bg-surface-50 transition-colors"
                          >
                            <div className="flex items-center gap-2 mb-1">
                              <span className={cn("px-1.5 py-0.5 rounded text-[10px] font-mono font-bold", methodColor[entry.method])}>
                                {entry.method}
                              </span>
                              <span
                                className={cn(
                                  "px-1.5 py-0.5 rounded text-[10px] font-mono font-bold",
                                  entry.status >= 200 && entry.status < 300
                                    ? "bg-emerald-100 text-emerald-700"
                                    : "bg-red-100 text-red-700"
                                )}
                              >
                                {entry.status}
                              </span>
                              <span className="text-[10px] text-surface-800/40 ml-auto">{entry.duration}ms</span>
                            </div>
                            <div className="text-[11px] font-mono text-surface-800/40">{entry.endpoint}</div>
                            <div className="text-[10px] text-surface-800/30 mt-0.5">
                              {new Date(entry.timestamp).toLocaleTimeString()}
                            </div>
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
