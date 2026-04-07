"use client";

import Link from "next/link";
import { useState } from "react";

type CodeTab = "curl" | "typescript" | "python";

function CodeBlock({
  code,
  lang,
}: {
  code: string;
  lang: string;
}) {
  return (
    <div className="relative">
      <div className="absolute top-2 right-2 text-[10px] font-mono text-surface-800/30 uppercase tracking-wider">
        {lang}
      </div>
      <pre className="bg-surface-950 text-surface-900 dark:text-surface-200 rounded-xl p-4 overflow-x-auto text-sm font-mono leading-relaxed">
        <code>{code}</code>
      </pre>
    </div>
  );
}

export default function GettingStartedPage() {
  const [activeTab, setActiveTab] = useState<CodeTab>("curl");

  const curlExample = `curl -X POST https://relay.xergon.network/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -d '{
    "model": "llama-3.1-8b",
    "messages": [
      { "role": "user", "content": "Hello, Xergon!" }
    ],
    "stream": false
  }'`;

  const tsExample = `import { XergonClient } from "@xergon/sdk";

const client = new XergonClient({
  apiKey: process.env.XERGON_API_KEY!,
  baseURL: "https://relay.xergon.network",
});

const response = await client.chat.completions.create({
  model: "llama-3.1-8b",
  messages: [
    { role: "user", content: "Hello, Xergon!" }
  ],
});

console.log(response.choices[0].message.content);`;

  const pythonExample = `import requests

response = requests.post(
    "https://relay.xergon.network/v1/chat/completions",
    headers={
        "Authorization": "Bearer YOUR_API_KEY",
        "Content-Type": "application/json",
    },
    json={
        "model": "llama-3.1-8b",
        "messages": [
            {"role": "user", "content": "Hello, Xergon!"}
        ],
        "stream": False,
    },
)

print(response.json()["choices"][0]["message"]["content"])`;

  const codeMap: Record<CodeTab, { code: string; lang: string }> = {
    curl: { code: curlExample, lang: "bash" },
    typescript: { code: tsExample, lang: "typescript" },
    python: { code: pythonExample, lang: "python" },
  };

  const steps = [
    {
      num: 1,
      title: "Create Account & Connect Wallet",
      description:
        "Sign up on the Xergon Marketplace and connect your Ergo wallet (Nautilus, Rosen, or compatible). This creates your on-chain identity and unlocks API access.",
    },
    {
      num: 2,
      title: "Get Your API Key",
      description:
        "Navigate to Settings and generate an API key. Your key is tied to your Ergo address and used for authenticating requests to the relay network.",
    },
    {
      num: 3,
      title: "Make Your First Request",
      description:
        "Use the API key to send your first chat completion request. Choose your preferred language below.",
    },
    {
      num: 4,
      title: "Explore Models & Pricing",
      description:
        "Browse the model catalog to find the right model for your use case. Compare pricing, context windows, and supported features.",
    },
  ];

  return (
    <div className="space-y-12">
      {/* Hero */}
      <section>
        <div className="inline-flex items-center gap-2 rounded-full bg-brand-50 dark:bg-brand-950/40 px-3 py-1 text-xs font-medium text-brand-700 dark:text-brand-300 mb-4">
          <span className="h-1.5 w-1.5 rounded-full bg-brand-500" />
          Documentation
        </div>
        <h1 className="text-3xl sm:text-4xl font-bold text-surface-900 mb-4">
          Get Started with Xergon
        </h1>
        <p className="text-lg text-surface-800/60 max-w-2xl">
          Build AI-powered applications with decentralized GPU inference. Pay
          with ERG, run open-source models, and integrate in minutes.
        </p>
      </section>

      {/* Prerequisites */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-4">
          Prerequisites
        </h2>
        <div className="grid sm:grid-cols-3 gap-4">
          {[
            {
              icon: "{ }",
              label: "Node.js 20+",
              desc: "Required for the SDK and local development",
            },
            {
              icon: "npm",
              label: "npm / yarn / pnpm",
              desc: "Package manager for installing the SDK",
            },
            {
              icon: "ERG",
              label: "Ergo Wallet",
              desc: "Nautilus, Rosen, or any compatible Ergo wallet",
            },
          ].map((item) => (
            <div
              key={item.label}
              className="rounded-xl border border-surface-200 p-4 bg-surface-0"
            >
              <div className="text-sm font-mono text-brand-600 mb-1">
                {item.icon}
              </div>
              <div className="font-medium text-surface-900 text-sm">
                {item.label}
              </div>
              <div className="text-xs text-surface-800/50 mt-1">
                {item.desc}
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* Quick start steps */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-6">
          Quick Start
        </h2>
        <div className="space-y-6">
          {steps.map((step) => (
            <div key={step.num} className="flex gap-4">
              <div className="flex-shrink-0">
                <div className="h-8 w-8 rounded-full bg-brand-600 text-white flex items-center justify-center text-sm font-bold">
                  {step.num}
                </div>
              </div>
              <div className="flex-1">
                <h3 className="font-semibold text-surface-900 mb-1">
                  {step.title}
                </h3>
                <p className="text-surface-800/60 text-sm">{step.description}</p>
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* Code examples */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-4">
          First Request
        </h2>
        <div className="mb-3">
          <div className="inline-flex rounded-lg border border-surface-200 overflow-hidden">
            {(["curl", "typescript", "python"] as CodeTab[]).map((tab) => (
              <button
                key={tab}
                onClick={() => setActiveTab(tab)}
                className={`px-4 py-2 text-sm font-medium transition-colors ${
                  activeTab === tab
                    ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                    : "bg-surface-0 text-surface-800/60 hover:text-surface-900"
                }`}
              >
                {tab === "typescript" ? "TypeScript" : tab === "python" ? "Python" : "cURL"}
              </button>
            ))}
          </div>
        </div>
        <CodeBlock code={codeMap[activeTab].code} lang={codeMap[activeTab].lang} />
      </section>

      {/* Install SDK */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-4">
          Install the SDK
        </h2>
        <CodeBlock
          lang="bash"
          code={`# npm
npm install @xergon/sdk

# yarn
yarn add @xergon/sdk

# pnpm
pnpm add @xergon/sdk`}
        />
      </section>

      {/* Next steps links */}
      <section>
        <h2 className="text-xl font-semibold text-surface-900 mb-4">
          Next Steps
        </h2>
        <div className="grid sm:grid-cols-2 gap-4">
          {[
            {
              href: "/docs/api-reference",
              title: "API Reference",
              desc: "Complete documentation for all endpoints, parameters, and responses.",
            },
            {
              href: "/docs/sdk",
              title: "SDK Documentation",
              desc: "Learn about authentication, streaming, React hooks, and more.",
            },
            {
              href: "/docs/models",
              title: "Model Catalog",
              desc: "Browse available models, compare features and pricing.",
            },
            {
              href: "/docs/concepts",
              title: "Key Concepts",
              desc: "Understand how Xergon works: providers, payments, governance, and more.",
            },
          ].map((link) => (
            <Link
              key={link.href}
              href={link.href}
              className="block rounded-xl border border-surface-200 p-4 bg-surface-0 hover:border-brand-300 hover:shadow-sm transition-all group"
            >
              <h3 className="font-medium text-surface-900 group-hover:text-brand-700 transition-colors">
                {link.title}
                <span className="ml-1 text-surface-800/30 group-hover:text-brand-400 transition-colors">
                  &rarr;
                </span>
              </h3>
              <p className="text-sm text-surface-800/50 mt-1">{link.desc}</p>
            </Link>
          ))}
        </div>
      </section>
    </div>
  );
}
