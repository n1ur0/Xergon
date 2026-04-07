"use client";

import { useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface Endpoint {
  id: string;
  method: "GET" | "POST" | "PUT" | "DELETE" | "PATCH";
  path: string;
  description: string;
  category: string;
  params?: { name: string; type: string; required: boolean; desc: string; default?: string }[];
  headers?: { name: string; desc: string; required: boolean }[];
  requestBody?: string;
  responseExample?: string;
  errors?: { code: number; meaning: string }[];
}

interface SavedRequest {
  id: string;
  name: string;
  endpointId: string;
  body: string;
  createdAt: string;
}

// ---------------------------------------------------------------------------
// Endpoint definitions
// ---------------------------------------------------------------------------

const ENDPOINTS: Endpoint[] = [
  {
    id: "chat-completions",
    method: "POST",
    path: "/v1/chat/completions",
    description: "Send messages to a language model and receive a completion. Supports streaming via SSE.",
    category: "Chat Completions",
    params: [
      { name: "model", type: "string", required: true, desc: "Model ID (e.g. llama-3.1-8b)", default: "llama-3.1-8b" },
      { name: "messages", type: "array", required: true, desc: "Array of message objects with role and content" },
      { name: "temperature", type: "number", required: false, desc: "Sampling temperature (0-2)", default: "0.7" },
      { name: "max_tokens", type: "integer", required: false, desc: "Maximum tokens to generate", default: "512" },
      { name: "stream", type: "boolean", required: false, desc: "Stream response chunks via SSE", default: "false" },
    ],
    headers: [
      { name: "Authorization", desc: "Bearer <api_key>", required: true },
      { name: "Content-Type", desc: "application/json", required: false },
    ],
    requestBody: JSON.stringify(
      {
        model: "llama-3.1-8b",
        messages: [
          { role: "system", content: "You are a helpful assistant." },
          { role: "user", content: "What is Xergon?" },
        ],
        temperature: 0.7,
        max_tokens: 512,
      },
      null,
      2,
    ),
    responseExample: JSON.stringify(
      {
        id: "chatcmpl-abc123",
        object: "chat.completion",
        created: 1714000000,
        model: "llama-3.1-8b",
        choices: [
          {
            index: 0,
            message: { role: "assistant", content: "Xergon is a decentralized AI inference marketplace..." },
            finish_reason: "stop",
          },
        ],
        usage: { prompt_tokens: 24, completion_tokens: 86, total_tokens: 110 },
      },
      null,
      2,
    ),
    errors: [
      { code: 400, meaning: "Invalid request body or parameters" },
      { code: 401, meaning: "Missing or invalid API key" },
      { code: 403, meaning: "Insufficient credits or rate limited" },
      { code: 404, meaning: "Model not found" },
      { code: 503, meaning: "No providers available" },
    ],
  },
  {
    id: "models",
    method: "GET",
    path: "/v1/models",
    description: "List all available models with metadata, pricing, and provider status.",
    category: "Models",
    params: [
      { name: "limit", type: "integer", required: false, desc: "Max results per page", default: "20" },
      { name: "offset", type: "integer", required: false, desc: "Pagination offset", default: "0" },
      { name: "provider", type: "string", required: false, desc: "Filter by provider ID" },
      { name: "features", type: "string", required: false, desc: "Filter by feature (streaming, vision, function_calling)" },
    ],
    headers: [
      { name: "Authorization", desc: "Bearer <api_key>", required: true },
    ],
    responseExample: JSON.stringify(
      {
        object: "list",
        data: [
          {
            id: "llama-3.1-8b",
            object: "model",
            created: 1714000000,
            owned_by: "meta",
            context_window: 131072,
            pricing: { input_per_1k: 0.0001, output_per_1k: 0.0002 },
            features: ["streaming", "function_calling"],
            providers: ["provider-alpha", "provider-beta"],
          },
        ],
      },
      null,
      2,
    ),
    errors: [{ code: 401, meaning: "Missing or invalid API key" }],
  },
  {
    id: "providers",
    method: "GET",
    path: "/v1/providers",
    description: "List all active compute providers with their status and reputation.",
    category: "Providers",
    params: [
      { name: "limit", type: "integer", required: false, desc: "Max results per page", default: "20" },
      { name: "status", type: "string", required: false, desc: "Filter by status (active, idle, offline)" },
    ],
    headers: [
      { name: "Authorization", desc: "Bearer <api_key>", required: true },
    ],
    responseExample: JSON.stringify(
      {
        object: "list",
        data: [
          {
            id: "provider-alpha",
            name: "Alpha GPU Node",
            status: "active",
            models: ["llama-3.1-8b", "mixtral-8x7b"],
            reputation: 0.97,
            region: "eu-west",
          },
        ],
      },
      null,
      2,
    ),
    errors: [],
  },
  {
    id: "health",
    method: "GET",
    path: "/health",
    description: "Health check endpoint. Returns system status and uptime.",
    category: "System",
    responseExample: JSON.stringify(
      { status: "healthy", uptime_seconds: 86400, version: "1.2.0", active_providers: 12 },
      null,
      2,
    ),
    errors: [],
  },
  {
    id: "governance-proposals",
    method: "GET",
    path: "/v1/governance/proposals",
    description: "List all governance proposals for the Xergon network.",
    category: "Governance",
    headers: [
      { name: "Authorization", desc: "Bearer <api_key>", required: true },
    ],
    responseExample: JSON.stringify(
      {
        object: "list",
        data: [
          { id: "prop-001", title: "Reduce provider fee to 2%", status: "active", votes_for: 120, votes_against: 30 },
        ],
      },
      null,
      2,
    ),
    errors: [{ code: 401, meaning: "Missing or invalid API key" }],
  },
  {
    id: "analytics-overview",
    method: "GET",
    path: "/v1/analytics/overview",
    description: "Network overview statistics including requests, tokens, and revenue.",
    category: "Analytics",
    headers: [
      { name: "Authorization", desc: "Bearer <api_key>", required: true },
    ],
    responseExample: JSON.stringify(
      {
        total_requests: 15420,
        total_tokens: 128_400_000,
        total_revenue_nanoerg: 4_560_000_000,
        active_users: 842,
        period: "24h",
      },
      null,
      2,
    ),
    errors: [],
  },
];

const CATEGORIES = [...new Set(ENDPOINTS.map((e) => e.category))];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const METHOD_COLORS: Record<string, string> = {
  GET: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  POST: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
  PUT: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
  DELETE: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  PATCH: "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
};

function generateCurl(endpoint: Endpoint, body: string, authToken: string): string {
  const url = `https://relay.xergon.network${endpoint.path}`;
  const headers: string[] = [`-H "Content-Type: application/json"`];
  if (authToken) {
    headers.push(`-H "Authorization: Bearer ${authToken}"`);
  }
  const headerStr = headers.join(" \\\n  ");
  const bodyStr = endpoint.method !== "GET" && body ? `\n  -d '${body}'` : "";
  return `curl -X ${endpoint.method} \\\n  ${url} \\\n  ${headerStr}${bodyStr}`;
}

function syntaxHighlight(json: string): string {
  return json.replace(
    /("(\\u[\dA-Fa-f]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)/g,
    (match) => {
      let cls = "text-amber-300"; // number
      if (/^"/.test(match)) {
        if (/:$/.test(match)) {
          cls = "text-blue-300"; // key
        } else {
          cls = "text-emerald-300"; // string
        }
      } else if (/true|false/.test(match)) {
        cls = "text-violet-300"; // boolean
      } else if (/null/.test(match)) {
        cls = "text-surface-500"; // null
      }
      return `<span class="${cls}">${match}</span>`;
    },
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ApiExplorer() {
  const [selectedId, setSelectedId] = useState<string>("chat-completions");
  const [requestBody, setRequestBody] = useState<string>("");
  const [authToken, setAuthToken] = useState("");
  const [response, setResponse] = useState<{ data: string; status: number; time: number } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>("");
  const [savedRequests, setSavedRequests] = useState<SavedRequest[]>([]);
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [saveName, setSaveName] = useState("");
  const [responseTab, setResponseTab] = useState<"body" | "headers" | "curl">("body");
  const [showAuthToken, setShowAuthToken] = useState(false);

  const selected = ENDPOINTS.find((e) => e.id === selectedId);

  // Initialize request body when endpoint changes
  const handleSelectEndpoint = useCallback((id: string) => {
    setSelectedId(id);
    setResponse(null);
    setError("");
    const ep = ENDPOINTS.find((e) => e.id === id);
    if (ep?.requestBody) {
      setRequestBody(ep.requestBody);
    } else {
      setRequestBody("");
    }
  }, []);

  // Initialize on mount
  useState(() => {
    const ep = ENDPOINTS.find((e) => e.id === "chat-completions");
    if (ep?.requestBody) setRequestBody(ep.requestBody);
  });

  const handleSend = async () => {
    if (!selected) return;
    setLoading(true);
    setError("");
    setResponse(null);

    const startTime = performance.now();

    try {
      const url = selected.path;
      const fetchOptions: RequestInit = {
        method: selected.method,
        headers: {
          "Content-Type": "application/json",
          ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
        },
      };

      if (selected.method !== "GET" && requestBody) {
        fetchOptions.body = requestBody;
      }

      const res = await fetch(url, fetchOptions);
      const elapsed = Math.round(performance.now() - startTime);
      let data: string;

      try {
        const json = await res.json();
        data = JSON.stringify(json, null, 2);
      } catch {
        data = await res.text();
      }

      setResponse({ data, status: res.status, time: elapsed });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Request failed");
    } finally {
      setLoading(false);
    }
  };

  const handleSaveRequest = () => {
    if (!saveName.trim() || !selected) return;
    const newSaved: SavedRequest = {
      id: `saved-${Date.now()}`,
      name: saveName.trim(),
      endpointId: selected.id,
      body: requestBody,
      createdAt: new Date().toISOString(),
    };
    setSavedRequests((prev) => [...prev, newSaved]);
    setShowSaveDialog(false);
    setSaveName("");
  };

  const handleLoadSaved = (saved: SavedRequest) => {
    handleSelectEndpoint(saved.endpointId);
    setRequestBody(saved.body);
  };

  const handleDeleteSaved = (id: string) => {
    setSavedRequests((prev) => prev.filter((s) => s.id !== id));
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
  };

  const curlCommand = selected ? generateCurl(selected, requestBody, authToken) : "";

  return (
    <div className="flex gap-6 min-h-[600px]">
      {/* Sidebar */}
      <nav className="hidden lg:block w-64 shrink-0">
        <div className="sticky top-8 space-y-4">
          {/* Auth */}
          <div className="rounded-lg border border-surface-200 dark:border-surface-700 p-3">
            <label className="block text-xs font-medium text-surface-800/50 mb-1.5">API Key</label>
            <div className="relative">
              <input
                type={showAuthToken ? "text" : "password"}
                value={authToken}
                onChange={(e) => setAuthToken(e.target.value)}
                placeholder="xrg_sk_..."
                className="w-full px-3 py-1.5 text-sm rounded-md border border-surface-200 dark:border-surface-700 bg-surface-50 dark:bg-surface-800 text-surface-900 dark:text-surface-100 placeholder:text-surface-800/30 pr-8 focus:outline-none focus:ring-2 focus:ring-brand-500/30 font-mono"
              />
              <button
                onClick={() => setShowAuthToken(!showAuthToken)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-surface-800/30 hover:text-surface-800/50"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  {showAuthToken ? (
                    <><path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94" /><path d="M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19" /><line x1="1" y1="1" x2="23" y2="23" /></>
                  ) : (
                    <><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle cx="12" cy="12" r="3" /></>
                  )}
                </svg>
              </button>
            </div>
          </div>

          {/* Saved requests */}
          {savedRequests.length > 0 && (
            <div>
              <h3 className="text-xs font-semibold uppercase tracking-wider text-surface-800/40 mb-2">
                Saved Requests
              </h3>
              <div className="space-y-1">
                {savedRequests.map((saved) => {
                  const ep = ENDPOINTS.find((e) => e.id === saved.endpointId);
                  return (
                    <div
                      key={saved.id}
                      className="flex items-center gap-1 group"
                    >
                      <button
                        onClick={() => handleLoadSaved(saved)}
                        className="flex-1 text-left px-2 py-1.5 text-sm rounded-lg hover:bg-surface-100 dark:hover:bg-surface-800 text-surface-800/70 dark:text-surface-300 truncate"
                        title={saved.name}
                      >
                        {saved.name}
                      </button>
                      <button
                        onClick={() => handleDeleteSaved(saved.id)}
                        className="p-1 rounded text-surface-800/20 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity"
                        title="Delete"
                      >
                        <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <line x1="18" y1="6" x2="6" y2="18" />
                          <line x1="6" y1="6" x2="18" y2="18" />
                        </svg>
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Endpoint categories */}
          {CATEGORIES.map((cat) => (
            <div key={cat}>
              <h3 className="text-xs font-semibold uppercase tracking-wider text-surface-800/40 mb-2">
                {cat}
              </h3>
              <ul className="space-y-1">
                {ENDPOINTS.filter((e) => e.category === cat).map((ep) => (
                  <li key={ep.id}>
                    <button
                      onClick={() => handleSelectEndpoint(ep.id)}
                      className={`block w-full text-left px-2 py-1.5 text-sm rounded-lg transition-colors ${
                        selectedId === ep.id
                          ? "bg-brand-50 text-brand-700 font-medium dark:bg-brand-950/40 dark:text-brand-300"
                          : "text-surface-800/60 hover:text-surface-900 hover:bg-surface-100 dark:hover:bg-surface-800"
                      }`}
                    >
                      <span className={`inline-block px-1.5 py-0 rounded text-[10px] font-mono font-bold mr-1.5 ${METHOD_COLORS[ep.method]}`}>
                        {ep.method}
                      </span>
                      <span className="font-mono text-xs">{ep.path.replace(/^\/v1/, "")}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </nav>

      {/* Main content */}
      <div className="flex-1 min-w-0 space-y-6">
        {selected && (
          <>
            {/* Endpoint header */}
            <div>
              <div className="flex items-center gap-3 mb-2">
                <span className={`inline-block px-2.5 py-1 rounded-md text-xs font-mono font-bold ${METHOD_COLORS[selected.method]}`}>
                  {selected.method}
                </span>
                <code className="text-lg font-mono font-semibold text-surface-900 dark:text-surface-100">
                  {selected.path}
                </code>
              </div>
              <p className="text-surface-800/60">{selected.description}</p>
            </div>

            {/* Request builder */}
            <div className="rounded-xl border border-surface-200 dark:border-surface-700 overflow-hidden">
              {/* Parameters */}
              {selected.params && selected.params.length > 0 && (
                <div className="p-4 border-b border-surface-200 dark:border-surface-700">
                  <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-100 mb-3">Parameters</h3>
                  <div className="space-y-2">
                    {selected.params.map((p) => (
                      <div key={p.name} className="flex items-start gap-3 text-sm">
                        <span className="font-mono text-brand-600 dark:text-brand-400 shrink-0 min-w-[100px]">
                          {p.name}
                          {p.required && <span className="text-red-500 ml-0.5">*</span>}
                        </span>
                        <span className="text-surface-800/40 text-xs mt-0.5">
                          {p.type} — {p.desc}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Headers */}
              {selected.headers && selected.headers.length > 0 && (
                <div className="p-4 border-b border-surface-200 dark:border-surface-700">
                  <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-100 mb-3">Headers</h3>
                  <div className="space-y-1.5 text-sm">
                    {selected.headers.map((h) => (
                      <div key={h.name} className="flex items-center gap-3">
                        <span className="font-mono text-surface-800/70 dark:text-surface-300 min-w-[120px]">{h.name}</span>
                        <span className="text-surface-800/40 text-xs">{h.desc}</span>
                        {h.required && (
                          <span className="text-[10px] font-medium text-red-500 bg-red-500/10 px-1.5 py-0.5 rounded">required</span>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Request body editor */}
              {selected.method !== "GET" && (
                <div className="p-4 border-b border-surface-200 dark:border-surface-700">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-100">Request Body</h3>
                    <button
                      onClick={() => handleCopy(requestBody)}
                      className="text-xs text-surface-800/40 hover:text-surface-800/60 transition-colors"
                    >
                      Copy
                    </button>
                  </div>
                  <textarea
                    value={requestBody}
                    onChange={(e) => setRequestBody(e.target.value)}
                    rows={8}
                    className="w-full px-3 py-2 rounded-lg bg-surface-950 text-surface-200 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-brand-500/30 resize-y"
                    spellCheck={false}
                  />
                </div>
              )}

              {/* Actions */}
              <div className="p-4 flex items-center gap-3">
                <button
                  onClick={handleSend}
                  disabled={loading}
                  className="px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 disabled:opacity-50 transition-colors flex items-center gap-2"
                >
                  {loading ? (
                    <>
                      <svg className="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <circle cx="12" cy="12" r="10" strokeDasharray="60" strokeDashoffset="20" />
                      </svg>
                      Sending...
                    </>
                  ) : (
                    `Send ${selected.method} Request`
                  )}
                </button>

                <button
                  onClick={() => {
                    setShowSaveDialog(true);
                    setSaveName("");
                  }}
                  className="px-3 py-2 rounded-lg border border-surface-200 dark:border-surface-700 text-sm text-surface-800/60 hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors"
                >
                  Save
                </button>

                <button
                  onClick={() => {
                    setRequestBody(selected.requestBody ?? "");
                    setError("");
                    setResponse(null);
                  }}
                  className="px-3 py-2 rounded-lg border border-surface-200 dark:border-surface-700 text-sm text-surface-800/60 hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors"
                >
                  Reset
                </button>
              </div>
            </div>

            {/* Save dialog */}
            {showSaveDialog && (
              <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
                <div className="bg-surface-0 dark:bg-surface-900 rounded-xl border border-surface-200 dark:border-surface-700 p-6 w-96 shadow-xl">
                  <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-100 mb-3">Save Request</h3>
                  <input
                    type="text"
                    value={saveName}
                    onChange={(e) => setSaveName(e.target.value)}
                    placeholder="Request name..."
                    className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 dark:border-surface-700 bg-surface-50 dark:bg-surface-800 text-surface-900 dark:text-surface-100 focus:outline-none focus:ring-2 focus:ring-brand-500/30 mb-3"
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleSaveRequest();
                      if (e.key === "Escape") setShowSaveDialog(false);
                    }}
                  />
                  <div className="flex justify-end gap-2">
                    <button
                      onClick={() => setShowSaveDialog(false)}
                      className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 dark:border-surface-700 text-surface-800/60 hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleSaveRequest}
                      disabled={!saveName.trim()}
                      className="px-3 py-1.5 text-sm rounded-lg bg-brand-600 text-white hover:bg-brand-700 disabled:opacity-50 transition-colors"
                    >
                      Save
                    </button>
                  </div>
                </div>
              </div>
            )}

            {/* Error */}
            {error && (
              <div className="rounded-lg bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-800 p-4">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-sm font-medium text-red-700 dark:text-red-400">Error</span>
                </div>
                <p className="text-sm text-red-600 dark:text-red-300">{error}</p>
              </div>
            )}

            {/* Response */}
            {response && (
              <div className="rounded-xl border border-surface-200 dark:border-surface-700 overflow-hidden">
                {/* Response header */}
                <div className="flex items-center justify-between px-4 py-2.5 bg-surface-50 dark:bg-surface-800 border-b border-surface-200 dark:border-surface-700">
                  <div className="flex items-center gap-3">
                    <span className={`px-2 py-0.5 rounded text-xs font-bold ${
                      response.status >= 200 && response.status < 300
                        ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                        : response.status >= 400
                          ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
                          : "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
                    }`}>
                      {response.status}
                    </span>
                    <span className="text-xs text-surface-800/40">{response.time}ms</span>
                  </div>

                  {/* Tabs */}
                  <div className="flex items-center gap-1">
                    {(["body", "headers", "curl"] as const).map((tab) => (
                      <button
                        key={tab}
                        onClick={() => setResponseTab(tab)}
                        className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors ${
                          responseTab === tab
                            ? "bg-surface-200 dark:bg-surface-700 text-surface-900 dark:text-surface-100"
                            : "text-surface-800/40 hover:text-surface-800/60"
                        }`}
                      >
                        {tab === "body" ? "Body" : tab === "headers" ? "Headers" : "cURL"}
                      </button>
                    ))}
                    <button
                      onClick={() => handleCopy(responseTab === "curl" ? curlCommand : response.data)}
                      className="ml-2 p-1 rounded text-surface-800/30 hover:text-surface-800/60 transition-colors"
                      title="Copy to clipboard"
                    >
                      <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                        <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
                      </svg>
                    </button>
                  </div>
                </div>

                {/* Response body */}
                <div className="bg-surface-950 p-4 overflow-x-auto max-h-96 overflow-y-auto">
                  {responseTab === "body" && (
                    <pre
                      className="text-sm font-mono text-surface-200 whitespace-pre-wrap"
                      dangerouslySetInnerHTML={{ __html: syntaxHighlight(response.data) }}
                    />
                  )}
                  {responseTab === "headers" && (
                    <pre className="text-sm font-mono text-surface-200 whitespace-pre-wrap">
                      {`content-type: application/json\nx-request-id: req-${Math.random().toString(36).slice(2, 10)}\nx-ratelimit-limit: 100\nx-ratelimit-remaining: ${Math.floor(Math.random() * 100)}`}
                    </pre>
                  )}
                  {responseTab === "curl" && (
                    <pre className="text-sm font-mono text-surface-200 whitespace-pre-wrap">
                      {curlCommand}
                    </pre>
                  )}
                </div>
              </div>
            )}

            {/* Response example (when no live response) */}
            {!response && !error && selected.responseExample && (
              <div>
                <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-100 mb-2">Example Response</h3>
                <div className="bg-surface-950 rounded-xl p-4 overflow-x-auto">
                  <pre
                    className="text-sm font-mono text-surface-200 whitespace-pre-wrap"
                    dangerouslySetInnerHTML={{ __html: syntaxHighlight(selected.responseExample) }}
                  />
                </div>
              </div>
            )}

            {/* Error codes */}
            {selected.errors && selected.errors.length > 0 && (
              <div>
                <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-100 mb-2">Error Codes</h3>
                <div className="overflow-x-auto rounded-xl border border-surface-200 dark:border-surface-700">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="bg-surface-50 dark:bg-surface-800 text-left">
                        <th className="px-4 py-2 font-medium text-surface-800/60">Code</th>
                        <th className="px-4 py-2 font-medium text-surface-800/60">Meaning</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-surface-200 dark:divide-surface-700">
                      {selected.errors.map((e) => (
                        <tr key={e.code}>
                          <td className="px-4 py-2.5 font-mono font-bold text-surface-900 dark:text-surface-100">{e.code}</td>
                          <td className="px-4 py-2.5 text-surface-800/60">{e.meaning}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
