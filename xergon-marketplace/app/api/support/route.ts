import { NextRequest, NextResponse } from "next/server";

// ── Types ──

interface FAQ {
  id: string;
  question: string;
  answer: string;
  category: string;
  helpful: number;
}

interface Article {
  id: string;
  title: string;
  category: string;
  readTime: string;
  views: number;
  excerpt: string;
  updatedAt: string;
}

interface Ticket {
  id: string;
  subject: string;
  category: string;
  priority: "low" | "medium" | "high" | "urgent";
  status: "open" | "in_progress" | "resolved" | "closed";
  description: string;
  createdAt: string;
  updatedAt: string;
  messages: TicketMessage[];
}

interface TicketMessage {
  id: string;
  sender: "user" | "support";
  content: string;
  createdAt: string;
}

// ── Mock Data ──

const FAQS: FAQ[] = [
  // Getting Started
  { id: "faq-1", question: "What is Xergon Network?", answer: "Xergon Network is a decentralized AI inference marketplace built on Ergo blockchain. It connects AI model providers with consumers, enabling efficient, cost-effective, and censorship-resistant AI inference.", category: "Getting Started", helpful: 42 },
  { id: "faq-2", question: "How do I get started with Xergon?", answer: "Create an account, set up your Ergo wallet, browse available models in the marketplace, and start making inference requests via our API or Playground. You can fund your account with ERG tokens to pay for usage.", category: "Getting Started", helpful: 38 },
  { id: "faq-3", question: "What is the Xergon Playground?", answer: "The Playground is an interactive web interface where you can test AI models before integrating them into your application. It supports chat, completion, and code generation modes with real-time streaming responses.", category: "Getting Started", helpful: 27 },
  // Account
  { id: "faq-4", question: "How do I reset my password?", answer: "Click 'Forgot Password' on the sign-in page and enter your registered email address. You'll receive a password reset link within 5 minutes. If you don't see it, check your spam folder.", category: "Account", helpful: 19 },
  { id: "faq-5", question: "Can I have multiple API keys?", answer: "Yes! Navigate to Settings > API Keys to create and manage multiple keys. Each key can have custom permissions and usage limits. We recommend using separate keys for different environments (dev, staging, prod).", category: "Account", helpful: 31 },
  { id: "faq-6", question: "How do I enable two-factor authentication?", answer: "Go to Settings > Security and click 'Enable 2FA'. Scan the QR code with an authenticator app like Google Authenticator or Authy, then enter the verification code to complete setup.", category: "Account", helpful: 22 },
  // Billing
  { id: "faq-7", question: "How does billing work on Xergon?", answer: "Xergon uses a pay-per-token model. You're charged based on input and output tokens consumed. Prices are set by individual providers in ERG. Your balance is tracked in real-time and you can top up anytime via your Ergo wallet.", category: "Billing", helpful: 35 },
  { id: "faq-8", question: "What payment methods are supported?", answer: "Xergon supports ERG tokens for payments. You can top up your balance directly from your Ergo wallet. We plan to support additional cryptocurrencies and fiat payment options in the future.", category: "Billing", helpful: 14 },
  { id: "faq-9", question: "How do I view my usage history?", answer: "Navigate to the Transactions page to see a complete history of your inference requests, token usage, and payments. You can filter by date range, model, and provider.", category: "Billing", helpful: 18 },
  { id: "faq-10", question: "Are there any free models available?", answer: "Yes, some providers offer free-tier models with limited rate limits. Look for the 'Free' badge on model listings in the marketplace. These are great for testing and development.", category: "Billing", helpful: 26 },
  // Models
  { id: "faq-11", question: "What models are available on Xergon?", answer: "Xergon hosts a variety of open-source AI models including Llama 3.x, Qwen 2.5, Mistral, DeepSeek, Phi-3, Gemma 2, and more. New models are added regularly as providers onboard them.", category: "Models", helpful: 29 },
  { id: "faq-12", question: "How do I choose the right model?", answer: "Consider your use case, budget, and performance requirements. Use the Compare feature to evaluate models side-by-side. Smaller models (7B-14B) are faster and cheaper, while larger models (70B+) offer better quality for complex tasks.", category: "Models", helpful: 33 },
  { id: "faq-13", question: "Can I request a specific model?", answer: "Yes! Visit the community forum and post a model request. If there's enough demand, providers will often onboard requested models. You can also become a provider and host the model yourself.", category: "Models", helpful: 15 },
  // API
  { id: "faq-14", question: "Is the Xergon API compatible with OpenAI?", answer: "Yes, our API follows an OpenAI-compatible format. You can use existing OpenAI SDKs by simply changing the base URL and API key. Check our API reference docs for details on the endpoint format.", category: "API", helpful: 44 },
  { id: "faq-15", question: "What are the API rate limits?", answer: "Rate limits vary by model and provider. Standard accounts get 60 requests/minute. Higher tiers and dedicated access plans offer increased limits. Rate limit headers are included in every API response.", category: "API", helpful: 21 },
  { id: "faq-16", question: "Does the API support streaming?", answer: "Yes! All models on Xergon support server-sent events (SSE) streaming. Set `stream: true` in your request to receive tokens as they're generated. This provides a much better user experience for chat applications.", category: "API", helpful: 37 },
  { id: "faq-17", question: "How do I handle API errors?", answer: "API errors return structured JSON with error codes and messages. Common errors include 401 (auth), 429 (rate limit), 500 (provider error), and 503 (model unavailable). Implement exponential backoff for retry logic.", category: "API", helpful: 20 },
  // Technical
  { id: "faq-18", question: "What is the average response time?", answer: "Average response times range from 200ms to 2s depending on the model size and provider. Smaller models like Phi-3 and Gemma typically respond in under 500ms, while larger models like Llama-70B may take 1-2s.", category: "Technical", helpful: 25 },
  { id: "faq-19", question: "How does Xergon ensure model reliability?", answer: "We use health checks, automatic failover, and a reputation system. If a provider goes offline, requests are automatically routed to the next available provider. The leaderboard tracks provider uptime and performance.", category: "Technical", helpful: 23 },
  { id: "faq-20", question: "Can I run Xergon on-premise?", answer: "Currently Xergon is a cloud-hosted service. However, our open-source relay software can be self-hosted for organizations that require on-premise deployments. Contact us for enterprise options.", category: "Technical", helpful: 12 },
  // Security
  { id: "faq-21", question: "How is my data protected?", answer: "All data is encrypted in transit (TLS 1.3) and at rest. Inference requests are not stored by default. Providers cannot see your prompts after processing. We comply with GDPR and do not sell user data.", category: "Security", helpful: 30 },
  { id: "faq-22", question: "What happens if a provider is malicious?", answer: "Xergon uses staking and reputation systems. Providers must stake ERG as collateral. Malicious behavior results in slashing (loss of stake) and removal from the marketplace. Community reviews also help identify bad actors.", category: "Security", helpful: 28 },
  { id: "faq-23", question: "How do I report a security vulnerability?", answer: "Please report security issues to security@xergon.network. We follow responsible disclosure practices and offer bounties for critical vulnerabilities. Do not publicly disclose issues before they're patched.", category: "Security", helpful: 8 },
];

const ARTICLES: Article[] = [
  { id: "art-1", title: "Getting Started Guide: Your First Inference Request", category: "Getting Started", readTime: "5 min", views: 2840, excerpt: "Learn how to make your first API call to Xergon, from API key creation to receiving your first response.", updatedAt: "2026-03-28" },
  { id: "art-2", title: "Understanding the Ergo Blockchain Integration", category: "Getting Started", readTime: "8 min", views: 1920, excerpt: "Deep dive into how Xergon leverages Ergo blockchain for payments, staking, and decentralized governance.", updatedAt: "2026-03-15" },
  { id: "art-3", title: "API Authentication and Key Management", category: "API", readTime: "6 min", views: 3410, excerpt: "Complete guide to API key creation, rotation, permissions, and best practices for secure key management.", updatedAt: "2026-04-01" },
  { id: "art-4", title: "Streaming Responses with Server-Sent Events", category: "API", readTime: "7 min", views: 2150, excerpt: "How to implement streaming in your applications using SSE, with examples in Python, JavaScript, and Go.", updatedAt: "2026-03-22" },
  { id: "art-5", title: "Model Selection Guide: Finding the Right Fit", category: "Models", readTime: "10 min", views: 4230, excerpt: "Comprehensive comparison of available models including benchmarks, use cases, and cost analysis.", updatedAt: "2026-04-02" },
  { id: "art-6", title: "Billing and Cost Optimization Strategies", category: "Billing", readTime: "6 min", views: 1780, excerpt: "Tips for reducing costs including prompt optimization, model selection, and batch processing.", updatedAt: "2026-03-10" },
  { id: "art-7", title: "Error Handling and Retry Strategies", category: "Technical", readTime: "5 min", views: 1560, excerpt: "Best practices for handling API errors, implementing retry logic, and building resilient applications.", updatedAt: "2026-02-28" },
  { id: "art-8", title: "Becoming a Provider on Xergon", category: "Provider", readTime: "12 min", views: 890, excerpt: "Step-by-step guide to setting up your inference node, staking requirements, and earning on Xergon.", updatedAt: "2026-03-05" },
  { id: "art-9", title: "Security Best Practices for API Users", category: "Security", readTime: "8 min", views: 2340, excerpt: "Essential security practices including key rotation, input validation, and monitoring for unusual activity.", updatedAt: "2026-03-18" },
  { id: "art-10", title: "Integrating Xergon with LangChain", category: "Technical", readTime: "9 min", views: 3120, excerpt: "How to use Xergon models with LangChain for building RAG pipelines, agents, and complex AI workflows.", updatedAt: "2026-04-03" },
  { id: "art-11", title: "Rate Limiting and Quota Management", category: "API", readTime: "4 min", views: 1430, excerpt: "Understanding rate limits, monitoring your usage, and requesting higher limits for production workloads.", updatedAt: "2026-02-20" },
  { id: "art-12", title: "Understanding Provider Reputation Scores", category: "Getting Started", readTime: "6 min", views: 980, excerpt: "How the reputation system works, what factors affect scores, and how to evaluate providers.", updatedAt: "2026-03-25" },
];

const MOCK_TICKETS: Ticket[] = [
  {
    id: "TKT-1001",
    subject: "API returns 503 for llama-3.1-70b",
    category: "Technical",
    priority: "high",
    status: "in_progress",
    description: "Getting 503 errors when trying to use llama-3.1-70b model. Other models work fine. Started happening 2 hours ago.",
    createdAt: "2026-04-05T10:30:00Z",
    updatedAt: "2026-04-05T14:15:00Z",
    messages: [
      { id: "msg-1", sender: "user", content: "Getting 503 errors when trying to use llama-3.1-70b model. Other models work fine. Started happening 2 hours ago.", createdAt: "2026-04-05T10:30:00Z" },
      { id: "msg-2", sender: "support", content: "Thank you for reporting this. We've identified the issue — one of the primary providers for this model experienced hardware failure. We're routing traffic to backup providers. ETA for full resolution is 1 hour.", createdAt: "2026-04-05T12:00:00Z" },
      { id: "msg-3", sender: "user", content: "Thanks for the update. It seems to be working intermittently now.", createdAt: "2026-04-05T13:45:00Z" },
      { id: "msg-4", sender: "support", content: "Yes, backup providers are handling the load. We're still working on restoring the primary. Your requests should succeed now, though latency may be slightly higher.", createdAt: "2026-04-05T14:15:00Z" },
    ],
  },
  {
    id: "TKT-1002",
    subject: "Billing discrepancy on March statement",
    category: "Billing",
    priority: "medium",
    status: "open",
    description: "I was charged 12.5 ERG on March 28 but I only used approximately 8 ERG worth of tokens based on my calculations.",
    createdAt: "2026-04-04T09:00:00Z",
    updatedAt: "2026-04-04T09:00:00Z",
    messages: [
      { id: "msg-5", sender: "user", content: "I was charged 12.5 ERG on March 28 but I only used approximately 8 ERG worth of tokens based on my calculations.", createdAt: "2026-04-04T09:00:00Z" },
    ],
  },
  {
    id: "TKT-1003",
    subject: "How to enable vision for supported models?",
    category: "Models",
    priority: "low",
    status: "resolved",
    description: "I saw that some models support vision but I'm not sure how to send image inputs through the API.",
    createdAt: "2026-04-02T15:20:00Z",
    updatedAt: "2026-04-02T17:30:00Z",
    messages: [
      { id: "msg-6", sender: "user", content: "I saw that some models support vision but I'm not sure how to send image inputs through the API.", createdAt: "2026-04-02T15:20:00Z" },
      { id: "msg-7", sender: "support", content: "Great question! For vision-capable models, you can pass image URLs in the `messages` array using the OpenAI-compatible format:\n\n```json\n{\n  \"role\": \"user\",\n  \"content\": [\n    {\"type\": \"text\", \"text\": \"Describe this image\"},\n    {\"type\": \"image_url\", \"image_url\": {\"url\": \"https://...\"}}\n  ]\n}\n```\n\nCheck the model's feature list for vision support.", createdAt: "2026-04-02T16:00:00Z" },
      { id: "msg-8", sender: "user", content: "That worked perfectly, thank you!", createdAt: "2026-04-02T17:30:00Z" },
    ],
  },
  {
    id: "TKT-1004",
    subject: "Request for enterprise SLA",
    category: "Account",
    priority: "low",
    status: "closed",
    description: "We're interested in an enterprise plan with guaranteed uptime SLA and dedicated support.",
    createdAt: "2026-03-28T11:00:00Z",
    updatedAt: "2026-04-01T10:00:00Z",
    messages: [
      { id: "msg-9", sender: "user", content: "We're interested in an enterprise plan with guaranteed uptime SLA and dedicated support.", createdAt: "2026-03-28T11:00:00Z" },
      { id: "msg-10", sender: "support", content: "Thank you for your interest! I've forwarded your request to our enterprise team. They'll reach out within 24 hours with available plans and pricing. In the meantime, you can check our pricing page for standard tiers.", createdAt: "2026-03-28T14:00:00Z" },
      { id: "msg-11", sender: "support", content: "Following up — our enterprise team has sent details to your registered email. Let us know if you have any questions!", createdAt: "2026-04-01T10:00:00Z" },
    ],
  },
];

const STATS = {
  avgResponseTime: "2.4 hours",
  resolvedPercent: 94,
  openTickets: 23,
  totalResolved: 847,
  satisfactionScore: 4.7,
};

// ── Route Handlers ──

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const section = searchParams.get("section");
  const query = searchParams.get("q")?.toLowerCase() ?? "";

  switch (section) {
    case "faq": {
      let faqs = FAQS;
      if (query) {
        faqs = faqs.filter(
          (f) =>
            f.question.toLowerCase().includes(query) ||
            f.answer.toLowerCase().includes(query) ||
            f.category.toLowerCase().includes(query),
        );
      }
      return NextResponse.json({ faqs, categories: ["Getting Started", "Account", "Billing", "Models", "API", "Technical", "Security"] });
    }

    case "articles": {
      let articles = ARTICLES;
      if (query) {
        articles = articles.filter(
          (a) =>
            a.title.toLowerCase().includes(query) ||
            a.excerpt.toLowerCase().includes(query) ||
            a.category.toLowerCase().includes(query),
        );
      }
      return NextResponse.json({ articles, categories: [...new Set(ARTICLES.map((a) => a.category))] });
    }

    case "tickets":
      return NextResponse.json({ tickets: MOCK_TICKETS });

    case "stats":
      return NextResponse.json({ stats: STATS });

    default:
      return NextResponse.json(
        { error: "Invalid section. Use: faq, articles, tickets, or stats" },
        { status: 400 },
      );
  }
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { subject, category, priority, description } = body;

    if (!subject || !category || !priority || !description) {
      return NextResponse.json(
        { error: "Missing required fields: subject, category, priority, description" },
        { status: 400 },
      );
    }

    const validPriorities = ["low", "medium", "high", "urgent"];
    if (!validPriorities.includes(priority)) {
      return NextResponse.json(
        { error: `Invalid priority. Must be one of: ${validPriorities.join(", ")}` },
        { status: 400 },
      );
    }

    const newTicket: Ticket = {
      id: `TKT-${1000 + MOCK_TICKETS.length + 1}`,
      subject,
      category,
      priority: priority as Ticket["priority"],
      status: "open",
      description,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      messages: [
        {
          id: `msg-${Date.now()}`,
          sender: "user",
          content: description,
          createdAt: new Date().toISOString(),
        },
      ],
    };

    return NextResponse.json({ ticket: newTicket, message: "Ticket submitted successfully" }, { status: 201 });
  } catch {
    return NextResponse.json({ error: "Invalid request body" }, { status: 400 });
  }
}
