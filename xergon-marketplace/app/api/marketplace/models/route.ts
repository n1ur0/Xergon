import { NextResponse } from "next/server";

import { RELAY_BASE } from "@/lib/api/server-sdk";

// ---------------------------------------------------------------------------
// Mock marketplace data
// ---------------------------------------------------------------------------

function mockMarketplaceModels() {
  const models = [
    { id: "llama-3.3-70b", name: "Llama 3.3 70B", provider: "Meta", providerId: "provider-1", tier: "pro", category: "nlp" as const, pricePerInputTokenNanoerg: 200, pricePerOutputTokenNanoerg: 300, effectivePriceNanoerg: 200, providerCount: 3, available: true, description: "General-purpose large language model with strong reasoning capabilities.", contextWindow: 8192, speed: "balanced", tags: ["Smart", "Code"], freeTier: false, quantization: "Q4_K_M", avgRating: 4.7, reviewCount: 128, benchmarkScore: 82, benchmarks: { MMLU: 82, HumanEval: 72, GSM8K: 88 }, isFeatured: true, isTrending: true, createdAt: "2025-10-01" },
    { id: "qwen2.5-72b", name: "Qwen 2.5 72B", provider: "Alibaba", providerId: "provider-2", tier: "pro", category: "nlp" as const, pricePerInputTokenNanoerg: 180, pricePerOutputTokenNanoerg: 250, effectivePriceNanoerg: 180, providerCount: 2, available: true, description: "High-capability multilingual model with strong reasoning.", contextWindow: 32768, speed: "balanced", tags: ["Smart", "Creative"], freeTier: false, quantization: "Q5_K_M", avgRating: 4.6, reviewCount: 95, benchmarkScore: 80, benchmarks: { MMLU: 80, HumanEval: 68, GSM8K: 85 }, isFeatured: true, isTrending: false, createdAt: "2025-09-15" },
    { id: "mistral-small-24b", name: "Mistral Small 24B", provider: "Mistral AI", providerId: "provider-3", tier: "free", category: "nlp" as const, pricePerInputTokenNanoerg: 0, pricePerOutputTokenNanoerg: 0, effectivePriceNanoerg: 0, providerCount: 4, available: true, description: "Efficient and fast model, free to use on Xergon.", contextWindow: 32768, speed: "fast", tags: ["Fast", "Creative", "Free"], freeTier: true, quantization: "Q4_0", avgRating: 4.3, reviewCount: 210, benchmarkScore: 72, benchmarks: { MMLU: 72, HumanEval: 60, GSM8K: 78 }, isFeatured: true, isTrending: true, createdAt: "2025-08-20" },
    { id: "deepseek-coder-33b", name: "DeepSeek Coder 33B", provider: "DeepSeek", providerId: "provider-4", tier: "pro", category: "code" as const, pricePerInputTokenNanoerg: 150, pricePerOutputTokenNanoerg: 200, effectivePriceNanoerg: 150, providerCount: 2, available: true, description: "Specialized code generation model with excellent benchmark performance.", contextWindow: 16384, speed: "balanced", tags: ["Code", "Smart"], freeTier: false, quantization: "Q5_K_S", avgRating: 4.8, reviewCount: 76, benchmarkScore: 85, benchmarks: { MMLU: 68, HumanEval: 85, GSM8K: 70 }, isFeatured: false, isTrending: true, createdAt: "2025-11-01" },
    { id: "llama-3.1-8b", name: "Llama 3.1 8B", provider: "Meta", providerId: "provider-1", tier: "free", category: "nlp" as const, pricePerInputTokenNanoerg: 0, pricePerOutputTokenNanoerg: 0, effectivePriceNanoerg: 0, providerCount: 5, available: true, description: "Lightweight and fast model for quick tasks.", contextWindow: 8192, speed: "fast", tags: ["Fast", "Free"], freeTier: true, quantization: "Q4_0", avgRating: 4.1, reviewCount: 320, benchmarkScore: 62, benchmarks: { MMLU: 62, HumanEval: 48, GSM8K: 70 }, isFeatured: false, isTrending: false, createdAt: "2025-07-10" },
    { id: "stable-diffusion-xl", name: "Stable Diffusion XL", provider: "Stability AI", providerId: "provider-5", tier: "pro", category: "vision" as const, pricePerInputTokenNanoerg: 500, pricePerOutputTokenNanoerg: 500, effectivePriceNanoerg: 500, providerCount: 1, available: true, description: "High-quality image generation model.", contextWindow: null, speed: "slow", tags: ["Creative"], freeTier: false, quantization: "FP16", avgRating: 4.5, reviewCount: 89, benchmarkScore: 78, benchmarks: { "CLIP Score": 78, FID: 25 }, isFeatured: true, isTrending: false, createdAt: "2025-09-01" },
    { id: "whisper-large-v3", name: "Whisper Large V3", provider: "OpenAI", providerId: "provider-6", tier: "pro", category: "audio" as const, pricePerInputTokenNanoerg: 300, pricePerOutputTokenNanoerg: 100, effectivePriceNanoerg: 200, providerCount: 2, available: true, description: "State-of-the-art speech recognition and transcription.", contextWindow: null, speed: "balanced", tags: ["Smart"], freeTier: false, quantization: "FP32", avgRating: 4.4, reviewCount: 45, benchmarkScore: 90, benchmarks: { WER: 8, CER: 3 }, isFeatured: false, isTrending: false, createdAt: "2025-10-15" },
    { id: "nomic-embed-v1.5", name: "Nomic Embed v1.5", provider: "Nomic AI", providerId: "provider-7", tier: "free", category: "embeddings" as const, pricePerInputTokenNanoerg: 0, pricePerOutputTokenNanoerg: 0, effectivePriceNanoerg: 0, providerCount: 3, available: true, description: "High-quality text embeddings with large context window.", contextWindow: 8192, speed: "fast", tags: ["Fast", "Free"], freeTier: true, quantization: "Q8_0", avgRating: 4.2, reviewCount: 67, benchmarkScore: 75, benchmarks: { "MTEB Score": 75 }, isFeatured: false, isTrending: true, createdAt: "2025-11-10" },
    { id: "phi-3-medium", name: "Phi-3 Medium", provider: "Microsoft", providerId: "provider-8", tier: "free", category: "nlp" as const, pricePerInputTokenNanoerg: 0, pricePerOutputTokenNanoerg: 0, effectivePriceNanoerg: 0, providerCount: 2, available: true, description: "Efficient reasoning model optimized for edge deployment.", contextWindow: 8192, speed: "fast", tags: ["Smart", "Fast", "Free"], freeTier: true, quantization: "Q4_K_M", avgRating: 4.0, reviewCount: 54, benchmarkScore: 68, benchmarks: { MMLU: 68, HumanEval: 55, GSM8K: 75 }, isFeatured: false, isTrending: false, createdAt: "2025-10-20" },
    { id: "codestral-22b", name: "Codestral 22B", provider: "Mistral AI", providerId: "provider-3", tier: "pro", category: "code" as const, pricePerInputTokenNanoerg: 120, pricePerOutputTokenNanoerg: 180, effectivePriceNanoerg: 120, providerCount: 2, available: true, description: "Mistral's dedicated code model with 256K context.", contextWindow: 262144, speed: "balanced", tags: ["Code", "Smart"], freeTier: false, quantization: "Q5_K_M", avgRating: 4.7, reviewCount: 42, benchmarkScore: 83, benchmarks: { MMLU: 70, HumanEval: 83, GSM8K: 65 }, isFeatured: true, isTrending: true, createdAt: "2025-11-05" },
    { id: "llava-v1.6-34b", name: "LLaVA v1.6 34B", provider: "Microsoft", providerId: "provider-8", tier: "pro", category: "multimodal" as const, pricePerInputTokenNanoerg: 350, pricePerOutputTokenNanoerg: 400, effectivePriceNanoerg: 350, providerCount: 1, available: true, description: "Vision-language model for image understanding and reasoning.", contextWindow: 4096, speed: "slow", tags: ["Smart", "Creative"], freeTier: false, quantization: "Q4_K_M", avgRating: 4.3, reviewCount: 31, benchmarkScore: 76, benchmarks: { MMBench: 76, "MathVista": 65 }, isFeatured: false, isTrending: false, createdAt: "2025-10-25" },
    { id: "gemma-2-27b", name: "Gemma 2 27B", provider: "Google", providerId: "provider-9", tier: "free", category: "nlp" as const, pricePerInputTokenNanoerg: 0, pricePerOutputTokenNanoerg: 0, effectivePriceNanoerg: 0, providerCount: 3, available: true, description: "Google's open model with strong performance across tasks.", contextWindow: 8192, speed: "balanced", tags: ["Smart", "Free"], freeTier: true, quantization: "Q4_K_M", avgRating: 4.2, reviewCount: 88, benchmarkScore: 70, benchmarks: { MMLU: 70, HumanEval: 58, GSM8K: 72 }, isFeatured: false, isTrending: false, createdAt: "2025-09-20" },
  ];
  return models;
}

const CATEGORIES = [
  { id: "nlp" as const, label: "NLP", description: "Natural language processing models for text understanding and generation", icon: "\uD83D\uDCDD" },
  { id: "vision" as const, label: "Vision", description: "Image generation, recognition, and understanding models", icon: "\uD83D\uDCF8" },
  { id: "code" as const, label: "Code", description: "Code generation, completion, and review models", icon: "\uD83D\uDCBB" },
  { id: "audio" as const, label: "Audio", description: "Speech recognition, synthesis, and audio processing", icon: "\uD83C\uDFB5" },
  { id: "multimodal" as const, label: "Multimodal", description: "Models that handle text, images, and other modalities", icon: "\uD83C\uDFA8" },
  { id: "embeddings" as const, label: "Embeddings", description: "Text embedding models for search, retrieval, and similarity", icon: "\uD83D\uDD0D" },
];

// ---------------------------------------------------------------------------
// In-memory favorites store (demo only -- would be DB-backed in production)
// ---------------------------------------------------------------------------

const FAVORITES_KEY = "xergon-favorites";

// ---------------------------------------------------------------------------
// GET /api/marketplace/models
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const subpath = searchParams.get("subpath"); // "featured", "categories", "favorites"

  // ---- Featured / Trending ----
  if (subpath === "featured") {
    const allModels = mockMarketplaceModels();
    const featured = allModels.filter(m => m.isFeatured);
    const trending = allModels.filter(m => m.isTrending);
    return NextResponse.json({ featured, trending });
  }

  // ---- Categories ----
  if (subpath === "categories") {
    const allModels = mockMarketplaceModels();
    const categories = CATEGORIES.map(cat => ({
      ...cat,
      modelCount: allModels.filter(m => m.category === cat.id).length,
    }));
    return NextResponse.json({ categories });
  }

  // ---- User favorites ----
  if (subpath === "favorites") {
    // In a real app, this would be user-specific from DB
    return NextResponse.json({ favorites: [] });
  }

  // ---- Browse models with filters ----
  const search = searchParams.get("search")?.toLowerCase();
  const category = searchParams.get("category");
  const task = searchParams.get("task")?.toLowerCase();
  const language = searchParams.get("language")?.toLowerCase();
  const priceMin = searchParams.get("priceMin") ? Number(searchParams.get("priceMin")) : undefined;
  const priceMax = searchParams.get("priceMax") ? Number(searchParams.get("priceMax")) : undefined;
  const minRating = searchParams.get("minRating") ? Number(searchParams.get("minRating")) : undefined;
  const quantization = searchParams.get("quantization");
  const minContext = searchParams.get("minContext") ? Number(searchParams.get("minContext")) : undefined;
  const sort = searchParams.get("sort") ?? "relevance";
  const page = Math.max(1, Number(searchParams.get("page") ?? 1));
  const pageSize = Math.min(50, Math.max(1, Number(searchParams.get("pageSize") ?? 24)));

  let models = mockMarketplaceModels();

  // Apply filters
  if (search) {
    models = models.filter(m =>
      m.name.toLowerCase().includes(search) ||
      m.description?.toLowerCase().includes(search) ||
      m.provider.toLowerCase().includes(search) ||
      m.tags?.some(t => t.toLowerCase().includes(search)),
    );
  }
  if (category) {
    models = models.filter(m => m.category === category);
  }
  if (task) {
    models = models.filter(m =>
      m.tags?.some(t => t.toLowerCase().includes(task)) ||
      m.description?.toLowerCase().includes(task),
    );
  }
  if (language) {
    models = models.filter(m =>
      m.description?.toLowerCase().includes(language) ||
      m.tags?.some(t => t.toLowerCase().includes(language)),
    );
  }
  if (priceMin !== undefined) {
    models = models.filter(m => m.effectivePriceNanoerg ?? m.pricePerInputTokenNanoerg >= priceMin);
  }
  if (priceMax !== undefined) {
    models = models.filter(m => m.effectivePriceNanoerg ?? m.pricePerInputTokenNanoerg <= priceMax);
  }
  if (minRating !== undefined) {
    models = models.filter(m => (m.avgRating ?? 0) >= minRating);
  }
  if (quantization) {
    models = models.filter(m => m.quantization === quantization);
  }
  if (minContext !== undefined) {
    models = models.filter(m => (m.contextWindow ?? 0) >= minContext);
  }

  // Sort
  models.sort((a, b) => {
    switch (sort) {
      case "price_asc":
        return (a.effectivePriceNanoerg ?? a.pricePerInputTokenNanoerg) - (b.effectivePriceNanoerg ?? b.pricePerInputTokenNanoerg);
      case "price_desc":
        return (b.effectivePriceNanoerg ?? b.pricePerInputTokenNanoerg) - (a.effectivePriceNanoerg ?? a.pricePerInputTokenNanoerg);
      case "rating":
        return (b.avgRating ?? 0) - (a.avgRating ?? 0);
      case "popularity":
        return (b.reviewCount ?? 0) - (a.reviewCount ?? 0);
      case "newest":
        return (b.createdAt ?? "").localeCompare(a.createdAt ?? "");
      case "benchmark":
        return (b.benchmarkScore ?? 0) - (a.benchmarkScore ?? 0);
      case "relevance":
      default:
        // Featured first, then by rating
        if (a.isFeatured && !b.isFeatured) return -1;
        if (!a.isFeatured && b.isFeatured) return 1;
        return (b.avgRating ?? 0) - (a.avgRating ?? 0);
    }
  });

  const total = models.length;
  const totalPages = Math.ceil(total / pageSize);
  const pagedModels = models.slice((page - 1) * pageSize, page * pageSize);

  return NextResponse.json({
    models: pagedModels,
    total,
    page,
    pageSize,
    totalPages,
  });
}

// ---------------------------------------------------------------------------
// POST /api/marketplace/models/[id]/favorite
// ---------------------------------------------------------------------------

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { modelId, action } = body;

    if (!modelId || !["add", "remove"].includes(action)) {
      return NextResponse.json(
        { error: "Invalid request. Provide modelId and action ('add' or 'remove')" },
        { status: 400 },
      );
    }

    // In production, this would persist to DB associated with authenticated user
    return NextResponse.json({
      modelId,
      favorited: action === "add",
      message: action === "add" ? "Model added to favorites" : "Model removed from favorites",
    });
  } catch {
    return NextResponse.json({ error: "Invalid request body" }, { status: 400 });
  }
}
