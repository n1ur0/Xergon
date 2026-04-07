"use client";

import { useState } from "react";
import Link from "next/link";

interface FeatureSection {
  id: string;
  title: string;
  description: string;
  code: string;
  lang: string;
}

const features: FeatureSection[] = [
  {
    id: "init",
    title: "Initialize Client",
    description: "Create an Xergon client with your API key and base URL.",
    code: `import { XergonClient } from "@xergon/sdk";

const client = new XergonClient({
  apiKey: process.env.XERGON_API_KEY!,
  baseURL: "https://relay.xergon.network",
});

// Optionally configure defaults
const clientWithDefaults = new XergonClient({
  apiKey: process.env.XERGON_API_KEY!,
  defaultModel: "llama-3.1-8b",
  defaultTemperature: 0.7,
});`,
    lang: "typescript",
  },
  {
    id: "chat",
    title: "Chat Completion",
    description: "Send a chat completion request and get the full response.",
    code: `const response = await client.chat.completions.create({
  model: "llama-3.1-8b",
  messages: [
    { role: "system", content: "You are a helpful assistant." },
    { role: "user", content: "Explain decentralized AI." },
  ],
  temperature: 0.7,
  max_tokens: 1024,
});

console.log(response.choices[0].message.content);
console.log(response.usage); // { prompt_tokens: 24, completion_tokens: 156, total_tokens: 180 }`,
    lang: "typescript",
  },
  {
    id: "streaming",
    title: "Stream Responses",
    description: "Stream response tokens in real-time using async iteration.",
    code: `const stream = await client.chat.completions.create({
  model: "llama-3.1-8b",
  messages: [{ role: "user", content: "Tell me a story" }],
  stream: true,
});

for await (const chunk of stream) {
  const delta = chunk.choices[0]?.delta?.content || "";
  process.stdout.write(delta);
}

// Or use the callback-based API
await client.chat.completions.create({
  model: "llama-3.1-8b",
  messages: [{ role: "user", content: "Hello" }],
  stream: true,
  onChunk: (chunk) => {
    console.log("Token:", chunk.choices[0]?.delta?.content);
  },
  onComplete: (full) => {
    console.log("Done:", full.usage);
  },
});`,
    lang: "typescript",
  },
  {
    id: "react-hooks",
    title: "React Hooks",
    description: "Use the useXergon hook for seamless integration with React components.",
    code: `"use client";
import { useXergon } from "@xergon/sdk/react";

function ChatComponent() {
  const { messages, isLoading, error, send } = useXergon({
    model: "llama-3.1-8b",
    systemPrompt: "You are a helpful assistant.",
  });

  return (
    <div>
      {messages.map((msg, i) => (
        <div key={i} className={msg.role}>
          {msg.content}
        </div>
      ))}
      {isLoading && <span>Thinking...</span>}
      {error && <span className="text-red-500">{error.message}</span>}
      <input
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            send(e.currentTarget.value);
            e.currentTarget.value = "";
          }
        }}
        placeholder="Type a message..."
      />
    </div>
  );
}`,
    lang: "typescript",
  },
  {
    id: "embed",
    title: "Embed Chat Widget",
    description: "Add a ready-to-use chat widget to any page with a single component.",
    code: `"use client";
import { XergonChatWidget } from "@xergon/sdk/react";

export default function Page() {
  return (
    <XergonChatWidget
      model="llama-3.1-8b"
      apiKey={process.env.NEXT_PUBLIC_XERGON_API_KEY}
      title="Xergon Assistant"
      placeholder="Ask me anything..."
      theme="auto" // "light" | "dark" | "auto"
      position="bottom-right" // "bottom-right" | "bottom-left"
    />
  );
}`,
    lang: "typescript",
  },
  {
    id: "auth",
    title: "Authentication",
    description: "Pass your API key via constructor, environment variable, or custom header.",
    code: `// Method 1: Constructor
const client = new XergonClient({
  apiKey: "xrg_sk_...",
});

// Method 2: Environment variable (auto-detected)
// Set XERGON_API_KEY=xrg_sk_... in your .env
const client = new XergonClient();

// Method 3: Custom auth header
const client = new XergonClient({
  authHeader: (key) => \`Bearer \${key}\`,
});

// Method 4: Dynamic key provider (for multi-tenant apps)
const client = new XergonClient({
  getApiKey: () => getCurrentUserApiKey(),
});`,
    lang: "typescript",
  },
  {
    id: "error-handling",
    title: "Error Handling",
    description: "Handle API errors with typed error classes and retry information.",
    code: `import { XergonError, XergonRateLimitError, XergonProviderError } from "@xergon/sdk";

try {
  const response = await client.chat.completions.create({
    model: "llama-3.1-8b",
    messages: [{ role: "user", content: "Hello" }],
  });
} catch (error) {
  if (error instanceof XergonRateLimitError) {
    console.log("Rate limited. Retry after:", error.retryAfter, "ms");
    console.log("Remaining requests:", error.remaining);
  } else if (error instanceof XergonProviderError) {
    console.log("Provider error:", error.providerId);
    console.log("Suggested model:", error.fallbackModel);
  } else if (error instanceof XergonError) {
    console.log("Xergon error:", error.code, error.message);
    console.log("Request ID:", error.requestId); // For support
  } else {
    throw error; // Network error, etc.
  }
}`,
    lang: "typescript",
  },
  {
    id: "retry",
    title: "Retry & Backoff",
    description: "Automatic retry with exponential backoff for transient failures.",
    code: `const client = new XergonClient({
  apiKey: process.env.XERGON_API_KEY!,
  retry: {
    maxRetries: 3,           // Max retry attempts
    baseDelay: 1000,         // Initial delay in ms
    maxDelay: 10000,         // Max delay cap
    backoffMultiplier: 2,    // Exponential multiplier
    retryOn: [503, 429],     // Status codes to retry
  },
});

// Or configure per-request
const response = await client.chat.completions.create({
  model: "llama-3.1-8b",
  messages: [{ role: "user", content: "Hello" }],
}, {
  retry: { maxRetries: 5, baseDelay: 500 },
});`,
    lang: "typescript",
  },
  {
    id: "cancellation",
    title: "Cancellation",
    description: "Cancel in-flight requests using AbortController.",
    code: `const controller = new AbortController();

// Set a 5-second timeout
setTimeout(() => controller.abort(), 5000);

try {
  const response = await client.chat.completions.create({
    model: "llama-3.1-8b",
    messages: [{ role: "user", content: "Long response" }],
    stream: true,
  }, { signal: controller.signal });

  for await (const chunk of response) {
    console.log(chunk.choices[0]?.delta?.content);
  }
} catch (error) {
  if (error instanceof DOMException && error.name === "AbortError") {
    console.log("Request was cancelled");
  }
}`,
    lang: "typescript",
  },
  {
    id: "batch",
    title: "Batch Requests",
    description: "Send multiple requests efficiently in a single batch.",
    code: `const results = await client.chat.completions.batch([
  {
    model: "llama-3.1-8b",
    messages: [{ role: "user", content: "Summarize: Article A" }],
  },
  {
    model: "llama-3.1-8b",
    messages: [{ role: "user", content: "Summarize: Article B" }],
  },
  {
    model: "mixtral-8x7b",
    messages: [{ role: "user", content: "Translate to French: Hello" }],
  },
]);

results.forEach((result, i) => {
  console.log(\`Request \${i}:\`, result.choices[0].message.content);
});

// All results include timing info
console.log(results[0].timing); // { queue_ms: 12, inference_ms: 340, total_ms: 352 }`,
    lang: "typescript",
  },
  {
    id: "websocket",
    title: "WebSocket Client",
    description: "Use the WebSocket client for low-latency bidirectional communication.",
    code: `import { XergonWebSocket } from "@xergon/sdk";

const ws = new XergonWebSocket({
  apiKey: process.env.XERGON_API_KEY!,
  url: "wss://relay.xergon.network/v1/ws",
});

ws.on("connected", () => {
  console.log("Connected to relay");
});

ws.on("message", (data) => {
  console.log("Received:", data);
});

ws.on("error", (error) => {
  console.error("WebSocket error:", error);
});

// Send a chat request
await ws.send({
  type: "chat",
  model: "llama-3.1-8b",
  messages: [{ role: "user", content: "Hello" }],
});

// Clean up
ws.disconnect();`,
    lang: "typescript",
  },
];

export default function SdkPage() {
  const [openSections, setOpenSections] = useState<Set<string>>(
    new Set(["init", "chat", "streaming", "react-hooks", "embed"])
  );

  const toggleSection = (id: string) => {
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  return (
    <div className="space-y-10">
      {/* Header */}
      <section>
        <h1 className="text-3xl font-bold text-surface-900 mb-2">
          SDK Documentation
        </h1>
        <p className="text-lg text-surface-800/60">
          The @xergon/sdk provides a typed client for the Xergon Relay API with
          support for streaming, React hooks, and more.
        </p>
      </section>

      {/* Installation */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-4">
          Installation
        </h2>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {[
            {
              label: "npm",
              code: "npm install @xergon/sdk",
            },
            {
              label: "yarn",
              code: "yarn add @xergon/sdk",
            },
            {
              label: "pnpm",
              code: "pnpm add @xergon/sdk",
            },
            {
              label: "CDN",
              code: '<script src="https://cdn.xergon.network/sdk.js"></script>',
            },
          ].map((item) => (
            <div key={item.label} className="rounded-xl border border-surface-200 overflow-hidden">
              <div className="px-3 py-1.5 bg-surface-50 text-xs font-medium text-surface-800/50 border-b border-surface-200">
                {item.label}
              </div>
              <pre className="px-3 py-2 text-xs font-mono bg-surface-950 text-surface-200 overflow-x-auto">
                {item.code}
              </pre>
            </div>
          ))}
        </div>
      </section>

      {/* Feature sections */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-6">
          Features
        </h2>
        <div className="space-y-3">
          {features.map((feature) => (
            <div
              key={feature.id}
              className="rounded-xl border border-surface-200 overflow-hidden bg-surface-0"
            >
              <button
                onClick={() => toggleSection(feature.id)}
                className="w-full flex items-center justify-between px-5 py-4 text-left hover:bg-surface-50 transition-colors"
              >
                <div>
                  <h3 className="font-medium text-surface-900">{feature.title}</h3>
                  <p className="text-sm text-surface-800/50 mt-0.5">
                    {feature.description}
                  </p>
                </div>
                <svg
                  className={`w-5 h-5 text-surface-800/30 transition-transform shrink-0 ml-4 ${
                    openSections.has(feature.id) ? "rotate-180" : ""
                  }`}
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </button>
              {openSections.has(feature.id) && (
                <div className="border-t border-surface-200">
                  <pre className="bg-surface-950 text-surface-200 p-4 overflow-x-auto text-sm font-mono leading-relaxed">
                    {feature.code}
                  </pre>
                </div>
              )}
            </div>
          ))}
        </div>
      </section>

      {/* Links */}
      <section className="flex flex-wrap gap-3">
        <Link
          href="/docs/api-reference"
          className="px-4 py-2 rounded-lg border border-surface-200 text-sm font-medium text-surface-800 hover:border-brand-300 hover:text-brand-700 transition-colors"
        >
          API Reference
        </Link>
        <Link
          href="/docs/models"
          className="px-4 py-2 rounded-lg border border-surface-200 text-sm font-medium text-surface-800 hover:border-brand-300 hover:text-brand-700 transition-colors"
        >
          Model Catalog
        </Link>
        <Link
          href="/docs/getting-started"
          className="px-4 py-2 rounded-lg border border-surface-200 text-sm font-medium text-surface-800 hover:border-brand-300 hover:text-brand-700 transition-colors"
        >
          Getting Started
        </Link>
      </section>
    </div>
  );
}
