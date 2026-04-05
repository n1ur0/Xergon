/**
 * GPU Bazar -- listings, rent, pricing, ratings, reputation.
 */

import type {
  GpuListing,
  GpuRental,
  GpuPricingEntry,
  GpuFilters,
  RateGpuParams,
  GpuReputation,
} from './types';
import { XergonClientCore } from './client';

/**
 * Browse GPU listings with optional filters.
 */
export async function listGpuListings(
  client: XergonClientCore,
  filters?: GpuFilters,
): Promise<GpuListing[]> {
  const params = new URLSearchParams();
  if (filters?.gpuType) params.set('gpu_type', filters.gpuType);
  if (filters?.minVram != null) params.set('min_vram', String(filters.minVram));
  if (filters?.maxPrice != null) params.set('max_price', String(filters.maxPrice));
  if (filters?.region) params.set('region', filters.region);
  const qs = params.toString();
  return client.get<GpuListing[]>(
    `/v1/gpu/listings${qs ? `?${qs}` : ''}`,
  );
}

/**
 * Get details for a specific GPU listing.
 */
export async function getGpuListing(
  client: XergonClientCore,
  listingId: string,
): Promise<GpuListing> {
  return client.get<GpuListing>(`/v1/gpu/listings/${encodeURIComponent(listingId)}`);
}

/**
 * Rent a GPU for a given number of hours.
 */
export async function rentGpu(
  client: XergonClientCore,
  listingId: string,
  hours: number,
): Promise<GpuRental> {
  return client.post<GpuRental>('/v1/gpu/rent', {
    listing_id: listingId,
    hours,
  });
}

/**
 * Get a user's active rentals.
 */
export async function getMyRentals(
  client: XergonClientCore,
  renterPk: string,
): Promise<GpuRental[]> {
  return client.get<GpuRental[]>(
    `/v1/gpu/rentals/${encodeURIComponent(renterPk)}`,
  );
}

/**
 * Get GPU pricing information.
 */
export async function getGpuPricing(
  client: XergonClientCore,
): Promise<GpuPricingEntry[]> {
  const raw = await client.get<{
    avg_price_per_hour: string;
    models: Record<string, string>;
  }>('/v1/gpu/pricing');

  // Convert the raw pricing response into a typed array
  return Object.entries(raw.models).map(([gpuType, price]) => ({
    gpuType,
    avgPricePerHourNanoerg: price,
  }));
}

/**
 * Rate a GPU provider or renter.
 */
export async function rateGpu(
  client: XergonClientCore,
  params: RateGpuParams,
): Promise<void> {
  await client.post('/v1/gpu/rate', {
    target_pk: params.targetPk,
    rental_id: params.rentalId,
    score: params.score,
    comment: params.comment ?? '',
  });
}

/**
 * Get reputation score for a public key.
 */
export async function getGpuReputation(
  client: XergonClientCore,
  publicKey: string,
): Promise<GpuReputation> {
  return client.get<GpuReputation>(
    `/v1/gpu/reputation/${encodeURIComponent(publicKey)}`,
  );
}
