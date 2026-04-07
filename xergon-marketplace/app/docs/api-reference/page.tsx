"use client";

import { useState } from "react";
import { ApiExplorer } from "@/components/docs/ApiExplorer";

/* ─── Endpoint definitions ─── */
interface Endpoint {
  id: string;
  method: "GET" | "POST" | "PUT" | "DELETE";
  path: string;
  description: string;
  category: string;
  params?: { name: string; type: string; required: boolean; desc: string }[];
  requestBody?: string;
  responseExample?: string;
  errors?: { code: number; meaning: string }[];
}

const ENDPOINTS: Endpoint[] = [
  {
    id: "chat-completions",
    method: "POST",
    path: "/v1/chat/completions",
    description:
      "Send messages to a language model and receive a completion. Supports streaming via SSE.",
    category: "Chat Completions",
    params: [
      { name: "model", type: "string", required: true, desc: "Model ID (e.g. llama-3.1-8b)" },
      { name: "messages", type: "array", required: true, desc: "Array of message objects with role and content" },
      { name: "temperature", type: "number", required: false, desc: "Sampling temperature (0-2, default 1)" },
      { name: "max_tokens", type: "integer", required: false, desc: "Maximum tokens to generate (default 1024)" },
      { name: "stream", type: "boolean", required: false, desc: "Stream response chunks via SSE (default false)" },
      { name: "top_p", type: "number", required: false, desc: "Nucleus sampling threshold (0-1)" },
      { name: "stop", type: "string[]", required: false, desc: "Stop sequences to end generation" },
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
        stream: false,
      },
      null,
      2
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
            message: {
              role: "assistant",
              content:
                "Xergon is a decentralized AI inference marketplace built on the Ergo blockchain...",
            },
            finish_reason: "stop",
          },
        ],
        usage: { prompt_tokens: 24, completion_tokens: 86, total_tokens: 110 },
      },
      null,
      2
    ),
    errors: [
      { code: 400, meaning: "Invalid request body or parameters" },
      { code: 401, meaning: "Missing or invalid API key" },
      { code: 403, meaning: "Insufficient credits or rate limited" },
      { code: 404, meaning: "Model not found" },
      { code: 503, meaning: "No providers available for the requested model" },
    ],
  },
  {
    id: "models",
    method: "GET",
    path: "/v1/models",
    description: "List all available models with metadata, pricing, and provider status.",
    category: "Models",
    params: [
      { name: "limit", type: "integer", required: false, desc: "Max results per page (default 20, max 100)" },
      { name: "offset", type: "integer", required: false, desc: "Pagination offset (default 0)" },
      { name: "provider", type: "string", required: false, desc: "Filter by provider ID" },
      { name: "features", type: "string", required: false, desc: "Filter by feature (streaming, vision, function_calling)" },
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
      2
    ),
    errors: [
      { code: 401, meaning: "Missing or invalid API key" },
    ],
  },
  {
    id: "providers",
    method: "GET",
    path: "/v1/providers",
    description: "List all active compute providers with their status, supported models, and reputation scores.",
    category: "Provider",
    params: [
      { name: "limit", type: "integer", required: false, desc: "Max results per page" },
      { name: "offset", type: "integer", required: false, desc: "Pagination offset" },
      { name: "status", type: "string", required: false, desc: "Filter by status (active, idle, offline)" },
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
            gpu_type: "NVIDIA A100 80GB",
          },
        ],
      },
      null,
      2
    ),
    errors: [],
  },
  {
    id: "health",
    method: "GET",
    path: "/health",
    description: "Health check endpoint. Returns system status, uptime, and active provider count.",
    category: "Health",
    responseExample: JSON.stringify(
      {
        status: "healthy",
        uptime_seconds: 86400,
        version: "1.2.0",
        active_providers: 12,
        total_requests_today: 15420,
      },
      null,
      2
    ),
    errors: [],
  },
];

const GOVERNANCE_ENDPOINTS = [
  { method: "GET", path: "/v1/governance/proposals", desc: "List all governance proposals" },
  { method: "POST", path: "/v1/governance/proposals", desc: "Create a new proposal" },
  { method: "POST", path: "/v1/governance/proposals/:id/vote", desc: "Cast a vote on a proposal" },
  { method: "GET", path: "/v1/governance/proposals/:id", desc: "Get proposal details and vote counts" },
];

const ANALYTICS_ENDPOINTS = [
  { method: "GET", path: "/v1/analytics/overview", desc: "Network overview statistics" },
  { method: "GET", path: "/v1/analytics/models", desc: "Per-model usage and revenue analytics" },
  { method: "GET", path: "/v1/analytics/providers", desc: "Provider performance analytics" },
  { method: "GET", path: "/v1/analytics/regions", desc: "Regional distribution of requests" },
];

function MethodBadge({ method }: { method: string }) {
  const colorMap: Record<string, string> = {
    GET: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    POST: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
    PUT: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
    DELETE: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  };
  return (
    <span
      className={`inline-block px-2 py-0.5 rounded text-xs font-mono font-bold ${colorMap[method] || "bg-surface-200 text-surface-800"}`}
    >
      {method}
    </span>
  );
}

/* ─── Sidebar category items ─── */
const SIDEBAR_CATEGORIES = [
  {
    label: "Chat Completions",
    endpoints: ENDPOINTS.filter((e) => e.category === "Chat Completions"),
  },
  { label: "Models", endpoints: ENDPOINTS.filter((e) => e.category === "Models") },
  { label: "Provider", endpoints: ENDPOINTS.filter((e) => e.category === "Provider") },
  { label: "Health", endpoints: ENDPOINTS.filter((e) => e.category === "Health") },
  {
    label: "Governance",
    endpoints: GOVERNANCE_ENDPOINTS.map((e) => ({
      id: `gov-${e.path}`,
      method: e.method as "GET" | "POST",
      path: e.path,
      description: e.desc,
      category: "Governance",
    })),
  },
  {
    label: "Analytics",
    endpoints: ANALYTICS_ENDPOINTS.map((e) => ({
      id: `analytics-${e.path}`,
      method: e.method as "GET",
      path: e.path,
      description: e.desc,
      category: "Analytics",
    })),
  },
];

export default function ApiReferencePage() {
  const [selectedId, setSelectedId] = useState<string>("chat-completions");
  const [tryItResponse, setTryItResponse] = useState<string>("");
  const [tryItLoading, setTryItLoading] = useState(false);
  const [tryItError, setTryItError] = useState<string>("");

  const selected = ENDPOINTS.find((e) => e.id === selectedId);

  const handleTryIt = async () => {
    setTryItLoading(true);
    setTryItError("");
    setTryItResponse("");
    try {
      const res = await fetch(selected!.path, {
        method: selected!.method,
        headers: {
          "Content-Type": "application/json",
        },
      });
      const data = await res.json();
      setTryItResponse(JSON.stringify(data, null, 2));
    } catch (err) {
      setTryItError(err instanceof Error ? err.message : "Request failed");
    } finally {
      setTryItLoading(false);
    }
  };

  return (
    <div className="space-y-8">
      <section>
        <h1 className="text-3xl font-bold text-surface-900 mb-2">API Reference</h1>
        <p className="text-surface-800/60">
          Complete reference for all Xergon Relay API endpoints. Base URL:
          <code className="ml-1 px-1.5 py-0.5 rounded bg-surface-100 text-sm font-mono">
            https://relay.xergon.network
          </code>
        </p>
      </section>

      <div className="flex gap-8">
        {/* Left sidebar - endpoints list */}
        <nav className="hidden xl:block w-56 shrink-0">
          <div className="sticky top-8 space-y-4">
            {SIDEBAR_CATEGORIES.map((cat) => (
              <div key={cat.label}>
                <h3 className="text-xs font-semibold uppercase tracking-wider text-surface-800/40 mb-2">
                  {cat.label}
                </h3>
                <ul className="space-y-1">
                  {cat.endpoints.map((ep) => (
                    <li key={ep.id}>
                      <button
                        onClick={() => setSelectedId(ep.id)}
                        className={`block w-full text-left px-2 py-1.5 text-sm rounded-lg transition-colors ${
                          selectedId === ep.id
                            ? "bg-brand-50 text-brand-700 font-medium dark:bg-brand-950/40 dark:text-brand-300"
                            : "text-surface-800/60 hover:text-surface-900 hover:bg-surface-100"
                        }`}
                      >
                        <MethodBadge method={ep.method} />
                        <span className="ml-1.5 font-mono text-xs">{ep.path.replace(/^\/v1/, "")}</span>
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
          {selected ? (
            <>
              {/* Endpoint header */}
              <div>
                <div className="flex items-center gap-3 mb-2">
                  <MethodBadge method={selected.method} />
                  <code className="text-lg font-mono font-semibold text-surface-900">
                    {selected.path}
                  </code>
                </div>
                <p className="text-surface-800/60">{selected.description}</p>
              </div>

              {/* Parameters */}
              {selected.params && selected.params.length > 0 && (
                <div>
                  <h2 className="text-lg font-semibold text-surface-900 mb-3">
                    Parameters
                  </h2>
                  <div className="overflow-x-auto rounded-xl border border-surface-200">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="bg-surface-50 text-left">
                          <th className="px-4 py-2 font-medium text-surface-800/60">Name</th>
                          <th className="px-4 py-2 font-medium text-surface-800/60">Type</th>
                          <th className="px-4 py-2 font-medium text-surface-800/60">Required</th>
                          <th className="px-4 py-2 font-medium text-surface-800/60">Description</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-surface-200">
                        {selected.params.map((p) => (
                          <tr key={p.name}>
                            <td className="px-4 py-2.5 font-mono text-brand-600">{p.name}</td>
                            <td className="px-4 py-2.5 text-surface-800/60">{p.type}</td>
                            <td className="px-4 py-2.5">
                              {p.required ? (
                                <span className="text-xs font-medium text-danger-500 bg-danger-500/10 px-1.5 py-0.5 rounded">
                                  required
                                </span>
                              ) : (
                                <span className="text-xs text-surface-800/40">optional</span>
                              )}
                            </td>
                            <td className="px-4 py-2.5 text-surface-800/60">{p.desc}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}

              {/* Request body */}
              {selected.requestBody && (
                <div>
                  <h2 className="text-lg font-semibold text-surface-900 mb-3">
                    Request Body
                  </h2>
                  <pre className="bg-surface-950 text-surface-200 rounded-xl p-4 overflow-x-auto text-sm font-mono">
                    {selected.requestBody}
                  </pre>
                </div>
              )}

              {/* Response */}
              {selected.responseExample && (
                <div>
                  <h2 className="text-lg font-semibold text-surface-900 mb-3">
                    Response
                  </h2>
                  <pre className="bg-surface-950 text-surface-200 rounded-xl p-4 overflow-x-auto text-sm font-mono">
                    {selected.responseExample}
                  </pre>
                </div>
              )}

              {/* Error codes */}
              {selected.errors && selected.errors.length > 0 && (
                <div>
                  <h2 className="text-lg font-semibold text-surface-900 mb-3">
                    Error Codes
                  </h2>
                  <div className="overflow-x-auto rounded-xl border border-surface-200">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="bg-surface-50 text-left">
                          <th className="px-4 py-2 font-medium text-surface-800/60">Code</th>
                          <th className="px-4 py-2 font-medium text-surface-800/60">Meaning</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-surface-200">
                        {selected.errors.map((e) => (
                          <tr key={e.code}>
                            <td className="px-4 py-2.5 font-mono font-bold text-surface-900">
                              {e.code}
                            </td>
                            <td className="px-4 py-2.5 text-surface-800/60">{e.meaning}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}

              {/* Try it section */}
              <div className="rounded-xl border border-surface-200 p-6 bg-surface-0">
                <h2 className="text-lg font-semibold text-surface-900 mb-4">
                  Try It
                </h2>
                <p className="text-sm text-surface-800/50 mb-4">
                  {selected.method === "POST"
                    ? "Send a test request. Requires a valid API key for POST endpoints."
                    : "This is a read-only endpoint and can be called directly."}
                </p>
                <button
                  onClick={handleTryIt}
                  disabled={tryItLoading}
                  className="px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 disabled:opacity-50 transition-colors"
                >
                  {tryItLoading ? "Sending..." : `Send ${selected.method} Request`}
                </button>

                {tryItError && (
                  <div className="mt-4 p-3 rounded-lg bg-danger-500/10 text-danger-600 text-sm">
                    {tryItError}
                  </div>
                )}

                {tryItResponse && (
                  <div className="mt-4">
                    <div className="text-xs font-medium text-surface-800/40 uppercase tracking-wider mb-2">
                      Response
                    </div>
                    <pre className="bg-surface-950 text-surface-200 rounded-xl p-4 overflow-x-auto text-sm font-mono max-h-80 overflow-y-auto">
                      {tryItResponse}
                    </pre>
                  </div>
                )}
              </div>
            </>
          ) : (
            /* Non-detailed endpoints (governance, analytics) */
            (() => {
              const ep = [
                ...GOVERNANCE_ENDPOINTS.map((e) => ({ ...e, category: "Governance" })),
                ...ANALYTICS_ENDPOINTS.map((e) => ({ ...e, category: "Analytics" })),
              ].find((e) => `gov-${e.path}` === selectedId || `analytics-${e.path}` === selectedId);
              if (!ep) return null;
              return (
                <>
                  <div>
                    <div className="flex items-center gap-3 mb-2">
                      <MethodBadge method={ep.method} />
                      <code className="text-lg font-mono font-semibold text-surface-900">
                        {ep.path}
                      </code>
                    </div>
                    <p className="text-surface-800/60">{ep.desc}</p>
                  </div>
                  <div className="rounded-xl border border-surface-200 p-6 bg-surface-50 text-sm text-surface-800/50">
                    Detailed parameters and examples for this endpoint are coming soon.
                    Check the main API docs at{" "}
                    <code className="px-1.5 py-0.5 rounded bg-surface-100 font-mono text-xs">
                      /api/docs
                    </code>{" "}
                    for OpenAPI spec.
                  </div>
                </>
              );
            })()
          )}
        </div>
      </div>
    </div>
  );
}
