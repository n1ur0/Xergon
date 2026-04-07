// ── Provider Portfolio Types ──

export interface PortfolioStats {
  totalModels: number;
  totalRequests: number;
  uptimePct: number;
  avgRating: number;
  totalRevenue: number; // in nanoERG
  repeatCustomers: number;
  totalUsersServed: number;
}

export interface SkillTag {
  id: string;
  label: string;
  category: "nlp" | "vision" | "code" | "audio" | "multimodal" | "embeddings";
}

export interface PerformanceDataPoint {
  date: string;
  value: number;
}

export interface PortfolioModel {
  id: string;
  name: string;
  description?: string;
  tier: string;
  pricePerInputTokenNanoerg: number;
  pricePerOutputTokenNanoerg: number;
  contextWindow?: number;
  tags?: string[];
  available: boolean;
  requestCount: number;
  avgLatencyMs?: number;
  benchmarks?: Record<string, number>;
}

export interface PortfolioReview {
  id: string;
  author: string;
  rating: number;
  content: string;
  date: string;
  model?: string;
}

export interface ActivityEvent {
  id: string;
  type: "model_added" | "model_updated" | "price_change" | "status_change" | "milestone" | "certification" | "achievement";
  description: string;
  date: string;
  metadata?: Record<string, unknown>;
}

export interface Certification {
  id: string;
  label: string;
  icon: string;
  description: string;
  earnedDate: string;
}

export interface ProviderPortfolio {
  providerId: string;
  displayName?: string;
  bio?: string;
  website?: string;
  socialLinks?: { platform: string; url: string }[];
  stats: PortfolioStats;
  skills: SkillTag[];
  performanceHistory: {
    requests: PerformanceDataPoint[];
    latency: PerformanceDataPoint[];
    availability: PerformanceDataPoint[];
  };
  models: PortfolioModel[];
  reviews: PortfolioReview[];
  activity: ActivityEvent[];
  certifications: Certification[];
  joinedDate: string;
}

export interface PortfolioUpdatePayload {
  displayName?: string;
  bio?: string;
  website?: string;
  socialLinks?: { platform: string; url: string }[];
  skills?: SkillTag[];
}

// ── Model Marketplace v2 Types ──

export type ModelCategory = "nlp" | "vision" | "code" | "audio" | "multimodal" | "embeddings";

export interface ModelCategoryInfo {
  id: ModelCategory;
  label: string;
  description: string;
  icon: string;
  modelCount: number;
}

export interface MarketplaceModel {
  id: string;
  name: string;
  provider: string;
  providerId: string;
  tier: string;
  category: ModelCategory;
  pricePerInputTokenNanoerg: number;
  pricePerOutputTokenNanoerg: number;
  effectivePriceNanoerg?: number;
  providerCount?: number;
  available: boolean;
  description?: string;
  contextWindow?: number;
  speed?: string;
  tags?: string[];
  freeTier?: boolean;
  quantization?: string;
  avgRating?: number;
  reviewCount?: number;
  benchmarkScore?: number;
  benchmarks?: Record<string, number>;
  isFeatured?: boolean;
  isTrending?: boolean;
  createdAt?: string;
}

export type MarketplaceSortField = "relevance" | "price_asc" | "price_desc" | "rating" | "popularity" | "newest" | "benchmark";
export type ViewMode = "grid" | "list";

export interface MarketplaceFilters {
  search?: string;
  category?: ModelCategory;
  task?: string;
  language?: string;
  priceMin?: number;
  priceMax?: number;
  minRating?: number;
  quantization?: string;
  minContextLength?: number;
  sort?: MarketplaceSortField;
  page?: number;
  pageSize?: number;
}

export interface MarketplaceResponse {
  models: MarketplaceModel[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}
