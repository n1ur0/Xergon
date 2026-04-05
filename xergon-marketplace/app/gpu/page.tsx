"use client";

import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import {
  fetchGpuListings,
  fetchGpuPricing,
  fetchMyRentals,
  fetchGpuReputation,
  rentGpu,
  nanoergToErg,
  parseGpuSpecs,
  FALLBACK_LISTINGS,
  FALLBACK_PRICING,
  type GpuListing,
  type GpuPricing,
  type GpuRental,
  type GpuFilters,
  type GpuReputation,
} from "@/lib/api/gpu";
import { useAuthStore } from "@/lib/stores/auth";
import { GpuCard } from "@/components/gpu/GpuCard";
import { GpuFiltersBar } from "@/components/gpu/GpuFilters";
import { RentModal } from "@/components/gpu/RentModal";
import { MyRentals } from "@/components/gpu/MyRentals";
import { PricingOverview } from "@/components/gpu/PricingOverview";
import { SkeletonCardGrid } from "@/components/ui/SkeletonCard";
import { SkeletonPricingTable, SkeletonRentalItem } from "@/components/ui/SkeletonElements";
import { ApiErrorDisplay } from "@/components/ui/ErrorBoundary";
import { EmptyState } from "@/components/ui/EmptyState";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { toast } from "@/components/ui/Toast";
import { Cpu } from "lucide-react";

export default function GpuBazarPage() {
  const user = useAuthStore((s) => s.user);

  // ── State ──
  const [listings, setListings] = useState<GpuListing[]>([]);
  const [pricing, setPricing] = useState<GpuPricing[]>([]);
  const [rentals, setRentals] = useState<GpuRental[]>([]);
  const [reputations, setReputations] = useState<Record<string, GpuReputation>>({});
  const [filters, setFilters] = useState<GpuFilters>({});
  const [isLoading, setIsLoading] = useState(true);
  const [isPricingLoading, setIsPricingLoading] = useState(true);
  const [isLoadingRentals, setIsLoadingRentals] = useState(false);
  const [fetchError, setFetchError] = useState<Error | null>(null);
  const [rentModalListing, setRentModalListing] = useState<GpuListing | null>(null);
  const [showMyRentals, setShowMyRentals] = useState(false);

  // Track whether we're using fallback data
  const [isUsingFallback, setIsUsingFallback] = useState(false);

  // ── Fetch listings ──
  const loadListings = useCallback((currentFilters: GpuFilters) => {
    setIsLoading(true);
    setFetchError(null);
    fetchGpuListings(currentFilters)
      .then((data) => {
        if (data.length > 0) {
          setListings(data);
          setIsUsingFallback(false);
        } else {
          setListings(FALLBACK_LISTINGS);
          setIsUsingFallback(true);
        }
      })
      .catch((err: Error) => {
        setFetchError(err);
        setListings(FALLBACK_LISTINGS);
        setIsUsingFallback(true);
      })
      .finally(() => setIsLoading(false));
  }, []);

  useEffect(() => {
    loadListings(filters);
  }, [filters, loadListings]);

  // ── Fetch pricing ──
  useEffect(() => {
    setIsPricingLoading(true);
    fetchGpuPricing()
      .then((data) => {
        setPricing(data.length > 0 ? data : FALLBACK_PRICING);
      })
      .catch(() => {
        setPricing(FALLBACK_PRICING);
      })
      .finally(() => setIsPricingLoading(false));
  }, []);

  // ── Fetch reputations for listed providers ──
  const reputationsRef = useRef(reputations);
  reputationsRef.current = reputations;

  useEffect(() => {
    const providerPks = [...new Set(listings.map((l) => l.provider_pk))];
    providerPks.forEach((pk) => {
      if (reputationsRef.current[pk]) return;
      fetchGpuReputation(pk)
        .then((rep) => {
          if (rep.total_ratings > 0) {
            setReputations((prev) => ({ ...prev, [pk]: rep }));
          }
        })
        .catch(() => {
          // No reputation data — that's fine
        });
    });
  }, [listings]);

  // ── Fetch my rentals ──
  const loadRentals = useCallback(() => {
    if (!user?.ergoAddress) return;
    setIsLoadingRentals(true);
    fetchMyRentals(user.ergoAddress)
      .then((data) => setRentals(data))
      .catch(() => setRentals([]))
      .finally(() => setIsLoadingRentals(false));
  }, [user?.ergoAddress]);

  useEffect(() => {
    if (showMyRentals && user?.ergoAddress) {
      loadRentals();
    }
  }, [showMyRentals, loadRentals, user?.ergoAddress]);

  // ── Client-side filtering (VRAM / price) ──
  const filteredListings = useMemo(() => {
    return listings.filter((listing) => {
      if (filters.region && listing.region !== filters.region) return false;
      if (filters.gpu_type && !listing.gpu_type.toLowerCase().includes(filters.gpu_type.toLowerCase())) return false;

      const specs = parseGpuSpecs(listing.gpu_specs_json);
      if (filters.min_vram != null && (specs.vram_gb ?? 0) < filters.min_vram) return false;

      const price = nanoergToErg(listing.price_per_hour_nanoerg);
      if (filters.max_price != null && price > filters.max_price) return false;

      return true;
    });
  }, [listings, filters]);

  // Determine if there are active filters
  const hasActiveFilters = !!(filters.region || filters.gpu_type || filters.min_vram || filters.max_price);

  // ── Rent handler with optimistic updates ──
  const pendingRentalsRef = useRef<Set<string>>(new Set());

  async function handleRent(listingId: string, hours: number) {
    if (!user?.ergoAddress) {
      toast.error("Wallet Required", {
        description: "Link your Ergo wallet in Settings to rent GPUs.",
      });
      return;
    }

    // Find the listing for the toast message
    const listing = listings.find((l) => l.listing_id === listingId);

    // Show pending toast immediately
    toast.rentalPending();

    // Optimistic update: mark listing as unavailable
    setListings((prev) =>
      prev.map((l) =>
        l.listing_id === listingId ? { ...l, available: false } : l,
      ),
    );

    try {
      await rentGpu(listingId, hours, user.ergoAddress);
      toast.rentalSuccess(listing?.gpu_type);
      // Refresh listings to reflect real availability
      fetchGpuListings(filters)
        .then((data) => setListings(data.length > 0 ? data : FALLBACK_LISTINGS))
        .catch(() => {});
      // Refresh rentals if viewing them
      if (showMyRentals) {
        loadRentals();
      }
    } catch (err) {
      // Revert optimistic update
      setListings((prev) =>
        prev.map((l) =>
          l.listing_id === listingId ? { ...l, available: true } : l,
        ),
      );

      const errorMsg = err instanceof Error ? err.message : "Failed to rent GPU";

      // Check for insufficient balance
      if (errorMsg.toLowerCase().includes("balance") || errorMsg.toLowerCase().includes("insufficient")) {
        toast.insufficientBalance();
      } else {
        toast.rentalFailed(errorMsg);
      }
      throw err; // Re-throw so modal can show error too
    }
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <div className="flex items-center gap-2 mb-2">
          <Cpu className="w-6 h-6 text-brand-600" />
          <h1 className="text-2xl font-bold">GPU Bazar</h1>
        </div>
        <p className="text-surface-800/60">
          Rent GPU time directly on Xergon Network. Pay with ERG, compute on-chain.
        </p>
      </div>

      {/* Tab switcher: Listings / My Rentals */}
      <div className="flex gap-1 mb-6 border-b border-surface-200">
        <button
          onClick={() => setShowMyRentals(false)}
          className={`px-4 py-2.5 text-sm font-medium border-b-2 transition-colors ${
            !showMyRentals
              ? "border-brand-600 text-brand-600"
              : "border-transparent text-surface-800/50 hover:text-surface-800/70"
          }`}
        >
          Browse GPUs
        </button>
        <button
          onClick={() => setShowMyRentals(true)}
          className={`px-4 py-2.5 text-sm font-medium border-b-2 transition-colors ${
            showMyRentals
              ? "border-brand-600 text-brand-600"
              : "border-transparent text-surface-800/50 hover:text-surface-800/70"
          }`}
        >
          My Rentals
          {rentals.length > 0 && (
            <span className="ml-1.5 rounded-full bg-brand-100 text-brand-700 px-1.5 py-0.5 text-xs">
              {rentals.filter((r) => r.active).length}
            </span>
          )}
        </button>
      </div>

      {/* My Rentals view */}
      {showMyRentals ? (
        <div className="space-y-6">
          {!user?.ergoAddress ? (
            <EmptyState
              type="no-rentals"
              description="Link your Ergo wallet in Settings to view your rental history."
              action={{
                label: "Open Settings",
                onClick: () => {
                  // Navigate to settings — using window.location for simplicity
                  window.location.href = "/settings";
                },
              }}
            />
          ) : isLoadingRentals ? (
            <div className="space-y-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <SkeletonRentalItem key={i} />
              ))}
            </div>
          ) : (
            <MyRentals rentals={rentals} />
          )}
        </div>
      ) : (
        <ErrorBoundary context="GPU Listings">
          <>
            {/* Fetch error */}
            {fetchError && !isLoading && (
              <ApiErrorDisplay
                error={fetchError}
                onRetry={() => loadListings(filters)}
                className="mb-6"
              />
            )}

            {/* Filters */}
            <div className="mb-6">
              <GpuFiltersBar
                filters={filters}
                onFiltersChange={setFilters}
                totalResults={filteredListings.length}
              />
            </div>

            {/* Listings grid */}
            {isLoading ? (
              <SkeletonCardGrid count={6} />
            ) : filteredListings.length === 0 && hasActiveFilters ? (
              <EmptyState
                type="no-search-results"
                action={{
                  label: "Clear All Filters",
                  onClick: () => setFilters({}),
                }}
              />
            ) : filteredListings.length === 0 && !isUsingFallback ? (
              <EmptyState type="no-gpus" />
            ) : (
              <div className="animate-fade-in">
                <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                  {filteredListings.map((listing) => (
                    <GpuCard
                      key={listing.listing_id}
                      listing={listing}
                      reputation={reputations[listing.provider_pk]}
                      onRent={setRentModalListing}
                    />
                  ))}
                </div>
              </div>
            )}

            {/* Pricing Overview */}
            <div className="mt-10">
              <h2 className="text-lg font-bold text-surface-900 mb-4">Market Overview</h2>
              {isPricingLoading ? (
                <SkeletonPricingTable />
              ) : (
                <PricingOverview pricing={pricing} />
              )}
            </div>

            {/* Footer */}
            <div className="mt-8 text-sm text-surface-800/50 text-center">
              All prices in ERG. Rentals are settled on the Ergo blockchain.
              {isUsingFallback && (
                <span className="block mt-1 text-surface-800/30">
                  Showing demo data — connect to a live relay for real listings.
                </span>
              )}
            </div>
          </>
        </ErrorBoundary>
      )}

      {/* Rent Modal */}
      <RentModal
        listing={rentModalListing}
        isOpen={!!rentModalListing}
        onClose={() => setRentModalListing(null)}
        onConfirm={handleRent}
      />
    </div>
  );
}
