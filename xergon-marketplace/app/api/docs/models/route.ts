import { NextRequest, NextResponse } from "next/server";

/* ─── Mock model data ─── */

interface ModelDoc {
  slug: string;
  name: string;
  description: string;
  version: string;
  license: string;
  provider: string;
  parameterCount: string;
  contextWindow: number;
  maxOutputTokens: number;
  quantization: string[];
  capabilities: {
    chat: boolean;
    completion: boolean;
    embedding: boolean;
    vision: boolean;
    functionCalling: boolean;
    jsonMode: boolean;
    streaming: boolean;
  };
  pricing: {
    provider: string;
    inputPer1M: number;
    outputPer1M: number;
  }[];
  benchmarks: {
    label: string;
    latencyP50: string;
    latencyP95: string;
    throughput: string;
    qualityScore: number;
  }[];
  codeExamples: {
    curl: string;
    typescript: string;
    python: string;
    rust: string;
  };
  tips: string[];
  relatedModels: { slug: string; name: string }[];
  versionHistory: { version: string; date: string; notes: string }[];
  providers: { name: string; region: string; gpuType: string; status: string }[];
}

const MODELS: ModelDoc[] = [
  {
    slug: "llama-3.1-8b",
    name: "Llama 3.1 8B",
    description:
      "Meta's efficient 8B parameter model. Great for general-purpose tasks, summarization, and code assistance. Offers excellent latency and cost-effectiveness for production workloads.",
    version: "3.1",
    license: "Llama 3.1 Community License",
    provider: "Meta",
    parameterCount: "8B",
    contextWindow: 131072,
    maxOutputTokens: 4096,
    quantization: ["FP16", "Q8_0", "Q5_K_M", "Q4_K_M", "IQ4_XS"],
    capabilities: {
      chat: true,
      completion: true,
      embedding: false,
      vision: false,
      functionCalling: true,
      jsonMode: true,
      streaming: true,
    },
    pricing: [
      { provider: "Xergon (default)", inputPer1M: 0.10, outputPer1M: 0.20 },
      { provider: "Provider Alpha", inputPer1M: 0.08, outputPer1M: 0.16 },
      { provider: "Provider Beta", inputPer1M: 0.12, outputPer1M: 0.22 },
    ],
    benchmarks: [
      {
        label: "Chat",
        latencyP50: "120ms",
        latencyP95: "350ms",
        throughput: "145 tok/s",
        qualityScore: 72,
      },
      {
        label: "Code",
        latencyP50: "150ms",
        latencyP95: "400ms",
        throughput: "120 tok/s",
        qualityScore: 65,
      },
    ],
    codeExamples: {
      curl: `curl -X POST https://relay.xergon.network/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -d '{
    "model": "llama-3.1-8b",
    "messages": [
      { "role": "system", "content": "You are a helpful assistant." },
      { "role": "user", "content": "Explain quantum computing in simple terms." }
    ],
    "temperature": 0.7,
    "max_tokens": 512
  }'`,
      typescript: `import { XergonClient } from "@xergon/sdk";

const client = new XergonClient({
  apiKey: process.env.XERGON_API_KEY!,
  baseURL: "https://relay.xergon.network",
});

const response = await client.chat.completions.create({
  model: "llama-3.1-8b",
  messages: [
    { role: "system", content: "You are a helpful assistant." },
    { role: "user", content: "Explain quantum computing in simple terms." },
  ],
  temperature: 0.7,
  max_tokens: 512,
});

console.log(response.choices[0].message.content);`,
      python: `import requests

response = requests.post(
    "https://relay.xergon.network/v1/chat/completions",
    headers={
        "Authorization": "Bearer YOUR_API_KEY",
        "Content-Type": "application/json",
    },
    json={
        "model": "llama-3.1-8b",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Explain quantum computing in simple terms."},
        ],
        "temperature": 0.7,
        "max_tokens": 512,
    },
)

print(response.json()["choices"][0]["message"]["content"])`,
      rust: `use reqwest::Client;
use serde_json::json;

let client = Client::new();
let res = client
    .post("https://relay.xergon.network/v1/chat/completions")
    .header("Authorization", "Bearer YOUR_API_KEY")
    .json(&json!({
        "model": "llama-3.1-8b",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Explain quantum computing in simple terms."}
        ],
        "temperature": 0.7,
        "max_tokens": 512
    }))
    .send()
    .await?;

let body: serde_json::Value = res.json().await?;
println!("{}", body["choices"][0]["message"]["content"]);`,
    },
    tips: [
      "Use temperature 0.7 for balanced creativity and coherence.",
      "For factual tasks, lower temperature to 0.1-0.3.",
      "Enable function_calling for structured tool use.",
      "JSON mode works best with explicit formatting instructions.",
    ],
    relatedModels: [
      { slug: "llama-3.1-70b", name: "Llama 3.1 70B" },
      { slug: "mistral-7b", name: "Mistral 7B v0.3" },
      { slug: "phi-4", name: "Phi-4" },
    ],
    versionHistory: [
      { version: "3.1.0", date: "2024-07-23", notes: "Initial release on Xergon" },
      { version: "3.1.1", date: "2024-08-15", notes: "Improved function calling accuracy" },
    ],
    providers: [
      { name: "Provider Alpha", region: "eu-west", gpuType: "NVIDIA A100 40GB", status: "active" },
      { name: "Provider Beta", region: "us-east", gpuType: "NVIDIA RTX 4090", status: "active" },
      { name: "Provider Gamma", region: "ap-south", gpuType: "NVIDIA A100 80GB", status: "idle" },
    ],
  },
  {
    slug: "llama-3.1-70b",
    name: "Llama 3.1 70B",
    description:
      "High-quality 70B parameter model with strong reasoning capabilities. Ideal for complex tasks requiring deep understanding, multi-step reasoning, and nuanced generation.",
    version: "3.1",
    license: "Llama 3.1 Community License",
    provider: "Meta",
    parameterCount: "70B",
    contextWindow: 131072,
    maxOutputTokens: 8192,
    quantization: ["FP16", "Q8_0", "Q5_K_M", "Q4_K_M"],
    capabilities: {
      chat: true,
      completion: true,
      embedding: false,
      vision: true,
      functionCalling: true,
      jsonMode: true,
      streaming: true,
    },
    pricing: [
      { provider: "Xergon (default)", inputPer1M: 0.40, outputPer1M: 0.80 },
      { provider: "Provider Alpha", inputPer1M: 0.35, outputPer1M: 0.70 },
    ],
    benchmarks: [
      {
        label: "Chat",
        latencyP50: "450ms",
        latencyP95: "900ms",
        throughput: "45 tok/s",
        qualityScore: 86,
      },
      {
        label: "Code",
        latencyP50: "500ms",
        latencyP95: "1000ms",
        throughput: "40 tok/s",
        qualityScore: 78,
      },
    ],
    codeExamples: {
      curl: `curl -X POST https://relay.xergon.network/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -d '{
    "model": "llama-3.1-70b",
    "messages": [
      { "role": "user", "content": "Write a Python function to find prime numbers." }
    ],
    "max_tokens": 1024
  }'`,
      typescript: `const response = await client.chat.completions.create({
  model: "llama-3.1-70b",
  messages: [
    { role: "user", content: "Write a Python function to find prime numbers." },
  ],
  max_tokens: 1024,
});`,
      python: `response = requests.post(
    "https://relay.xergon.network/v1/chat/completions",
    headers={"Authorization": "Bearer YOUR_API_KEY"},
    json={
        "model": "llama-3.1-70b",
        "messages": [{"role": "user", "content": "Write a Python function to find prime numbers."}],
        "max_tokens": 1024,
    },
)`,
      rust: `let res = client
    .post("https://relay.xergon.network/v1/chat/completions")
    .header("Authorization", "Bearer YOUR_API_KEY")
    .json(&json!({
        "model": "llama-3.1-70b",
        "messages": [{"role": "user", "content": "Write a Python function to find prime numbers."}],
        "max_tokens": 1024
    }))
    .send()
    .await?;`,
    },
    tips: [
      "Best for complex reasoning tasks that need deeper understanding.",
      "Vision capability works with base64 images or URLs.",
      "Use max_tokens=8192 for longer outputs.",
    ],
    relatedModels: [
      { slug: "llama-3.1-8b", name: "Llama 3.1 8B" },
      { slug: "llama-3.1-405b", name: "Llama 3.1 405B" },
      { slug: "qwen-2.5-72b", name: "Qwen 2.5 72B" },
    ],
    versionHistory: [
      { version: "3.1.0", date: "2024-07-23", notes: "Initial release on Xergon" },
    ],
    providers: [
      { name: "Provider Alpha", region: "eu-west", gpuType: "NVIDIA A100 80GB x2", status: "active" },
      { name: "Provider Delta", region: "us-west", gpuType: "NVIDIA H100 80GB", status: "active" },
    ],
  },
  {
    slug: "mixtral-8x7b",
    name: "Mixtral 8x7B",
    description:
      "Mistral's mixture-of-experts model. Efficient inference with excellent quality-to-cost ratio. Only 2 of 8 experts active per token for fast performance.",
    version: "1.0",
    license: "Apache 2.0",
    provider: "Mistral AI",
    parameterCount: "8x7B (47B total)",
    contextWindow: 32768,
    maxOutputTokens: 4096,
    quantization: ["FP16", "Q8_0", "Q5_K_M", "Q4_K_M"],
    capabilities: {
      chat: true,
      completion: true,
      embedding: false,
      vision: false,
      functionCalling: true,
      jsonMode: true,
      streaming: true,
    },
    pricing: [
      { provider: "Xergon (default)", inputPer1M: 0.30, outputPer1M: 0.60 },
      { provider: "Provider Alpha", inputPer1M: 0.25, outputPer1M: 0.50 },
    ],
    benchmarks: [
      { label: "Chat", latencyP50: "200ms", latencyP95: "500ms", throughput: "90 tok/s", qualityScore: 80 },
      { label: "Code", latencyP50: "220ms", latencyP95: "550ms", throughput: "85 tok/s", qualityScore: 74 },
    ],
    codeExamples: {
      curl: `curl -X POST https://relay.xergon.network/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -d '{
    "model": "mixtral-8x7b",
    "messages": [{ "role": "user", "content": "Hello!" }],
    "stream": true
  }'`,
      typescript: `const stream = await client.chat.completions.create({
  model: "mixtral-8x7b",
  messages: [{ role: "user", content: "Hello!" }],
  stream: true,
});

for await (const chunk of stream) {
  process.stdout.write(chunk.choices[0]?.delta?.content || "");
}`,
      python: `# Streaming example
response = requests.post(
    "https://relay.xergon.network/v1/chat/completions",
    headers={"Authorization": "Bearer YOUR_API_KEY"},
    json={
        "model": "mixtral-8x7b",
        "messages": [{"role": "user", "content": "Hello!"}],
        "stream": True,
    },
    stream=True,
)

for line in response.iter_lines():
    if line:
        print(line.decode("utf-8"))`,
      rust: `let res = client
    .post("https://relay.xergon.network/v1/chat/completions")
    .header("Authorization", "Bearer YOUR_API_KEY")
    .json(&json!({
        "model": "mixtral-8x7b",
        "messages": [{"role": "user", "content": "Hello!"}]
    }))
    .send()
    .await?;`,
    },
    tips: [
      "MoE architecture means only ~13B params active per token - very efficient.",
      "Great for multilingual tasks.",
      "Streaming is highly efficient due to MoE routing.",
    ],
    relatedModels: [
      { slug: "mistral-7b", name: "Mistral 7B v0.3" },
      { slug: "llama-3.1-70b", name: "Llama 3.1 70B" },
    ],
    versionHistory: [
      { version: "1.0.0", date: "2024-06-10", notes: "Initial release on Xergon" },
      { version: "1.0.1", date: "2024-07-01", notes: "Added function calling support" },
    ],
    providers: [
      { name: "Provider Alpha", region: "eu-west", gpuType: "NVIDIA A100 80GB", status: "active" },
      { name: "Provider Beta", region: "us-east", gpuType: "NVIDIA A100 40GB x2", status: "active" },
    ],
  },
  {
    slug: "deepseek-coder-v2",
    name: "DeepSeek Coder V2",
    description:
      "Specialized code generation model with excellent performance on programming benchmarks. Supports 338 languages with a 128K context window.",
    version: "2.0",
    license: "DeepSeek License",
    provider: "DeepSeek",
    parameterCount: "236B (MoE, 21B active)",
    contextWindow: 131072,
    maxOutputTokens: 8192,
    quantization: ["FP16", "Q8_0", "Q5_K_M"],
    capabilities: {
      chat: true,
      completion: true,
      embedding: false,
      vision: false,
      functionCalling: true,
      jsonMode: true,
      streaming: true,
    },
    pricing: [
      { provider: "Xergon (default)", inputPer1M: 0.30, outputPer1M: 0.60 },
    ],
    benchmarks: [
      { label: "Code", latencyP50: "280ms", latencyP95: "650ms", throughput: "65 tok/s", qualityScore: 88 },
      { label: "Chat", latencyP50: "250ms", latencyP95: "600ms", throughput: "70 tok/s", qualityScore: 76 },
    ],
    codeExamples: {
      curl: `curl -X POST https://relay.xergon.network/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -d '{
    "model": "deepseek-coder-v2",
    "messages": [
      { "role": "system", "content": "You are an expert programmer." },
      { "role": "user", "content": "Implement a binary search tree in Rust." }
    ],
    "max_tokens": 2048
  }'`,
      typescript: `const response = await client.chat.completions.create({
  model: "deepseek-coder-v2",
  messages: [
    { role: "system", content: "You are an expert programmer." },
    { role: "user", content: "Implement a binary search tree in Rust." },
  ],
  max_tokens: 2048,
});`,
      python: `response = requests.post(
    "https://relay.xergon.network/v1/chat/completions",
    headers={"Authorization": "Bearer YOUR_API_KEY"},
    json={
        "model": "deepseek-coder-v2",
        "messages": [
            {"role": "system", "content": "You are an expert programmer."},
            {"role": "user", "content": "Implement a binary search tree in Rust."},
        ],
        "max_tokens": 2048,
    },
)`,
      rust: `let res = client
    .post("https://relay.xergon.network/v1/chat/completions")
    .header("Authorization", "Bearer YOUR_API_KEY")
    .json(&json!({
        "model": "deepseek-coder-v2",
        "messages": [
            {"role": "system", "content": "You are an expert programmer."},
            {"role": "user", "content": "Implement a binary search tree in Rust."}
        ],
        "max_tokens": 2048
    }))
    .send()
    .await?;`,
    },
    tips: [
      "Optimized for code generation - best used for programming tasks.",
      "Supports 338 programming languages.",
      "Large 128K context is ideal for codebase analysis.",
    ],
    relatedModels: [
      { slug: "llama-3.1-70b", name: "Llama 3.1 70B" },
      { slug: "qwen-2.5-72b", name: "Qwen 2.5 72B" },
    ],
    versionHistory: [
      { version: "2.0.0", date: "2024-08-01", notes: "Initial release on Xergon" },
    ],
    providers: [
      { name: "Provider Alpha", region: "eu-west", gpuType: "NVIDIA H100 80GB", status: "active" },
    ],
  },
  {
    slug: "qwen-2.5-72b",
    name: "Qwen 2.5 72B",
    description:
      "Alibaba's powerful 72B model with strong multilingual and coding capabilities. Supports 29+ languages natively with excellent instruction following.",
    version: "2.5",
    license: "Apache 2.0",
    provider: "Alibaba",
    parameterCount: "72B",
    contextWindow: 131072,
    maxOutputTokens: 8192,
    quantization: ["FP16", "Q8_0", "Q5_K_M", "Q4_K_M"],
    capabilities: {
      chat: true,
      completion: true,
      embedding: false,
      vision: true,
      functionCalling: true,
      jsonMode: true,
      streaming: true,
    },
    pricing: [
      { provider: "Xergon (default)", inputPer1M: 0.40, outputPer1M: 0.80 },
      { provider: "Provider Beta", inputPer1M: 0.35, outputPer1M: 0.70 },
    ],
    benchmarks: [
      { label: "Chat", latencyP50: "420ms", latencyP95: "850ms", throughput: "50 tok/s", qualityScore: 84 },
      { label: "Code", latencyP50: "460ms", latencyP95: "920ms", throughput: "42 tok/s", qualityScore: 82 },
    ],
    codeExamples: {
      curl: `curl -X POST https://relay.xergon.network/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -d '{
    "model": "qwen-2.5-72b",
    "messages": [{ "role": "user", "content": "Translate to Japanese: Hello, how are you?" }]
  }'`,
      typescript: `const response = await client.chat.completions.create({
  model: "qwen-2.5-72b",
  messages: [
    { role: "user", content: "Translate to Japanese: Hello, how are you?" },
  ],
});`,
      python: `response = requests.post(
    "https://relay.xergon.network/v1/chat/completions",
    headers={"Authorization": "Bearer YOUR_API_KEY"},
    json={
        "model": "qwen-2.5-72b",
        "messages": [{"role": "user", "content": "Translate to Japanese: Hello, how are you?"}],
    },
)`,
      rust: `let res = client
    .post("https://relay.xergon.network/v1/chat/completions")
    .header("Authorization", "Bearer YOUR_API_KEY")
    .json(&json!({
        "model": "qwen-2.5-72b",
        "messages": [{"role": "user", "content": "Translate to Japanese: Hello, how are you?"}]
    }))
    .send()
    .await?;`,
    },
    tips: [
      "Excellent multilingual support - 29+ languages.",
      "Strong at following complex instructions.",
      "Good balance of quality and speed for a 72B model.",
    ],
    relatedModels: [
      { slug: "llama-3.1-70b", name: "Llama 3.1 70B" },
      { slug: "deepseek-coder-v2", name: "DeepSeek Coder V2" },
    ],
    versionHistory: [
      { version: "2.5.0", date: "2024-09-01", notes: "Initial release on Xergon" },
    ],
    providers: [
      { name: "Provider Beta", region: "us-east", gpuType: "NVIDIA A100 80GB x2", status: "active" },
      { name: "Provider Gamma", region: "ap-south", gpuType: "NVIDIA A100 80GB", status: "active" },
    ],
  },
];

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const slug = searchParams.get("slug");

  if (slug) {
    const model = MODELS.find((m) => m.slug === slug);
    if (!model) {
      return NextResponse.json({ error: "Model not found" }, { status: 404 });
    }
    return NextResponse.json(model);
  }

  const summaries = MODELS.map((m) => ({
    slug: m.slug,
    name: m.name,
    description: m.description,
    provider: m.provider,
    parameterCount: m.parameterCount,
    contextWindow: m.contextWindow,
    status: "active" as const,
  }));

  return NextResponse.json({ data: summaries, total: summaries.length });
}
