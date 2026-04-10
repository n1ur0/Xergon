/**
 * GPU Bazar API -- powered by the Xergon SDK.
 *
 * Types are kept in snake_case for marketplace compatibility.
 * SDK calls are wrapped for graceful error handling.
 */

import { sdk } from "./config";

// ── Types ──

export interface GpuListing {
  box_id: string;
  listing_id: string;
  provider_pk: string;
  gpu_type: string;
  gpu_specs_json: string;
  price_per_hour_nanoerg: number;
  region: string;
  value_nanoerg: number;
  available: boolean;
}

export interface GpuSpecs {
  vram_gb?: number;
  cuda_cores?: number;
  memory_type?: string;
  memory_bandwidth?: string;
  tdp_watts?: number;
  driver_version?: string;
  compute_capability?: string;
}

export interface GpuPricing {
  gpu_type: string;
  avg_price_per_hour_erg: number;
  min_price_per_hour_erg: number;
  max_price_per_hour_erg: number;
  listing_count: number;
}

export interface GpuRental {
  rental_box_id: string;
  listing_id: string;
  provider_pk: string;
  renter_pk: string;
  gpu_type: string;
  region: string;
  rental_tx_id: string;
  start_height: number;
  deadline_height: number;
  hours_rented: number;
  total_cost_nanoerg: number;
  active: boolean;
}

export interface RentGpuResponse {
  rental_tx_id: string;
  deadline_height: number;
  rental_box_id: string;
}

export interface GpuReputation {
  average_rating: number;
  total_ratings: number;
}

// ── Helpers ──

const NANOERG_PER_ERG = 1_000_000_000;

export function nanoergToErg(nano: number): number {
  return nano / NANOERG_PER_ERG;
}

export function parseGpuSpecs(json: string): GpuSpecs {
  try {
    return JSON.parse(json);
  } catch {
    return {};
  }
}

// ── Filter types ──

export interface GpuFilters {
  region?: string;
  min_vram?: number;
  max_price?: number;
  gpu_type?: string;
}

// ── API functions ──

export async function fetchGpuListings(
  filters?: GpuFilters,
): Promise<GpuListing[]> {
  const params = new URLSearchParams();
  if (filters?.region) params.set("region", filters.region);
  if (filters?.min_vram != null) params.set("min_vram", String(filters.min_vram));
  if (filters?.max_price != null)
    params.set("max_price_per_hour", String(filters.max_price));
  if (filters?.gpu_type) params.set("gpu_type", filters.gpu_type);

  const qs = params.toString();
  const url = `${sdk.getBaseUrl()}/v1/gpu/listings${qs ? `?${qs}` : ""}`;

  const res = await fetch(url);
  if (!res.ok) throw new Error(`Failed to fetch GPU listings: ${res.statusText}`);
  return res.json();
}

export async function fetchGpuListing(listingId: string): Promise<GpuListing> {
  const res = await fetch(`${sdk.getBaseUrl()}/v1/gpu/listings/${listingId}`);
  if (!res.ok) throw new Error(`Failed to fetch listing: ${res.statusText}`);
  return res.json();
}

export async function rentGpu(
  listingId: string,
  hours: number,
  renterPublicKey: string,
): Promise<RentGpuResponse> {
  const res = await fetch(`${sdk.getBaseUrl()}/v1/gpu/rent`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      listing_id: listingId,
      hours,
      renter_pk: renterPublicKey,
    }),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({ message: res.statusText }));
    throw new Error(data.message || `Failed to rent GPU: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchMyRentals(renterPk: string): Promise<GpuRental[]> {
  const res = await fetch(`${sdk.getBaseUrl()}/v1/gpu/rentals/${renterPk}`);
  if (!res.ok) throw new Error(`Failed to fetch rentals: ${res.statusText}`);
  return res.json();
}

export async function fetchGpuPricing(): Promise<GpuPricing[]> {
  const res = await fetch(`${sdk.getBaseUrl()}/v1/gpu/pricing`);
  if (!res.ok) throw new Error(`Failed to fetch pricing: ${res.statusText}`);
  return res.json();
}

export async function rateGpu(
  providerPk: string,
  listingId: string,
  rating: number,
  comment?: string,
): Promise<{ message: string }> {
  const res = await fetch(`${sdk.getBaseUrl()}/v1/gpu/rate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      provider_pk: providerPk,
      listing_id: listingId,
      rating,
      comment: comment ?? "",
    }),
  });
  if (!res.ok) throw new Error(`Failed to submit rating: ${res.statusText}`);
  return res.json();
}

export async function fetchGpuReputation(
  publicKey: string,
): Promise<GpuReputation> {
  const res = await fetch(`${sdk.getBaseUrl()}/v1/gpu/reputation/${publicKey}`);
  if (!res.ok) throw new Error(`Failed to fetch reputation: ${res.statusText}`);
  return res.json();
}

// ── Mock / Fallback data ──

export const REGIONS = [
  "us-east",
  "us-west",
  "eu-west",
  "eu-central",
  "ap-southeast",
] as const;

export const GPU_TYPES = [
  "RTX 4090",
  "RTX 4080",
  "RTX 3090",
  "A100 80GB",
  "A6000",
  "RTX 6000 Ada",
  "H100 80GB",
] as const;

export const FALLBACK_LISTINGS: GpuListing[] = [
  {
    box_id: "box-mock-001",
    listing_id: "list-mock-001",
    provider_pk: "9eYRs...mock1",
    gpu_type: "RTX 4090",
    gpu_specs_json: JSON.stringify({
      vram_gb: 24,
      cuda_cores: 16384,
      memory_type: "GDDR6X",
      memory_bandwidth: "1.0 TB/s",
      tdp_watts: 450,
    }),
    price_per_hour_nanoerg: 50_000_000, // 0.05 ERG
    region: "us-east",
    value_nanoerg: 500_000_000,
    available: true,
  },
  {
    box_id: "box-mock-002",
    listing_id: "list-mock-002",
    provider_pk: "3kFpA...mock2",
    gpu_type: "A100 80GB",
    gpu_specs_json: JSON.stringify({
      vram_gb: 80,
      cuda_cores: 6912,
      memory_type: "HBM2e",
      memory_bandwidth: "2.0 TB/s",
      tdp_watts: 300,
    }),
    price_per_hour_nanoerg: 120_000_000, // 0.12 ERG
    region: "us-east",
    value_nanoerg: 1_200_000_000,
    available: true,
  },
  {
    box_id: "box-mock-003",
    listing_id: "list-mock-003",
    provider_pk: "7xQmB...mock3",
    gpu_type: "RTX 3090",
    gpu_specs_json: JSON.stringify({
      vram_gb: 24,
      cuda_cores: 10496,
      memory_type: "GDDR6X",
      memory_bandwidth: "936 GB/s",
      tdp_watts: 350,
    }),
    price_per_hour_nanoerg: 30_000_000, // 0.03 ERG
    region: "eu-west",
    value_nanoerg: 300_000_000,
    available: true,
  },
  {
    box_id: "box-mock-004",
    listing_id: "list-mock-004",
    provider_pk: "2jRtC...mock4",
    gpu_type: "H100 80GB",
    gpu_specs_json: JSON.stringify({
      vram_gb: 80,
      cuda_cores: 16896,
      memory_type: "HBM3",
      memory_bandwidth: "3.35 TB/s",
      tdp_watts: 700,
    }),
    price_per_hour_nanoerg: 250_000_000, // 0.25 ERG
    region: "us-west",
    value_nanoerg: 2_500_000_000,
    available: true,
  },
  {
    box_id: "box-mock-005",
    listing_id: "list-mock-005",
    provider_pk: "8wLpD...mock5",
    gpu_type: "RTX 4080",
    gpu_specs_json: JSON.stringify({
      vram_gb: 16,
      cuda_cores: 9728,
      memory_type: "GDDR6X",
      memory_bandwidth: "717 GB/s",
      tdp_watts: 320,
    }),
    price_per_hour_nanoerg: 35_000_000, // 0.035 ERG
    region: "eu-central",
    value_nanoerg: 350_000_000,
    available: false,
  },
  {
    box_id: "box-mock-006",
    listing_id: "list-mock-006",
    provider_pk: "5nKfE...mock6",
    gpu_type: "A6000",
    gpu_specs_json: JSON.stringify({
      vram_gb: 48,
      cuda_cores: 10752,
      memory_type: "GDDR6",
      memory_bandwidth: "768 GB/s",
      tdp_watts: 300,
    }),
    price_per_hour_nanoerg: 80_000_000, // 0.08 ERG
    region: "ap-southeast",
    value_nanoerg: 800_000_000,
    available: true,
  },
];

export const FALLBACK_PRICING: GpuPricing[] = [
  { gpu_type: "RTX 4090", avg_price_per_hour_erg: 0.05, min_price_per_hour_erg: 0.03, max_price_per_hour_erg: 0.08, listing_count: 12 },
  { gpu_type: "RTX 4080", avg_price_per_hour_erg: 0.035, min_price_per_hour_erg: 0.025, max_price_per_hour_erg: 0.05, listing_count: 8 },
  { gpu_type: "RTX 3090", avg_price_per_hour_erg: 0.03, min_price_per_hour_erg: 0.02, max_price_per_hour_erg: 0.04, listing_count: 15 },
  { gpu_type: "A100 80GB", avg_price_per_hour_erg: 0.12, min_price_per_hour_erg: 0.08, max_price_per_hour_erg: 0.18, listing_count: 6 },
  { gpu_type: "H100 80GB", avg_price_per_hour_erg: 0.25, min_price_per_hour_erg: 0.2, max_price_per_hour_erg: 0.35, listing_count: 3 },
  { gpu_type: "A6000", avg_price_per_hour_erg: 0.08, min_price_per_hour_erg: 0.06, max_price_per_hour_erg: 0.12, listing_count: 5 },
  { gpu_type: "RTX 6000 Ada", avg_price_per_hour_erg: 0.15, min_price_per_hour_erg: 0.1, max_price_per_hour_erg: 0.22, listing_count: 4 },
];
