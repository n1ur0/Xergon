"use client";

import { useState, useEffect, useCallback } from "react";
import { StarRating } from "./StarRating";
import { ReviewCard, type Review } from "./ReviewCard";
import { WriteReviewModal } from "./WriteReviewModal";
import { fetchReviews, fetchReviewStats, type ReviewStats } from "@/lib/api/reviews";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type SortOption = "newest" | "highest" | "lowest";

interface ReviewListProps {
  modelId: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDistributionBar(count: number, total: number): string {
  if (total === 0) return "0%";
  return `${Math.round((count / total) * 100)}%`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ReviewList({ modelId }: ReviewListProps) {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [stats, setStats] = useState<ReviewStats | null>(null);
  const [sort, setSort] = useState<SortOption>("newest");
  const [page, setPage] = useState(1);
  const [isLoading, setIsLoading] = useState(true);
  const [showWriteModal, setShowWriteModal] = useState(false);

  const PAGE_SIZE = 10;

  const loadData = useCallback(async () => {
    try {
      setIsLoading(true);
      const [reviewData, statsData] = await Promise.all([
        fetchReviews(modelId, sort, page, PAGE_SIZE),
        fetchReviewStats(modelId),
      ]);
      setReviews(reviewData);
      setStats(statsData);
    } catch {
      // Silently handle — empty state will show
    } finally {
      setIsLoading(false);
    }
  }, [modelId, sort, page]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const sortedReviews = [...reviews].sort((a, b) => {
    if (sort === "newest") return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
    if (sort === "highest") return b.rating - a.rating;
    return a.rating - b.rating;
  });

  const totalPages = stats ? Math.ceil(stats.totalCount / PAGE_SIZE) : 1;

  return (
    <div className="space-y-6">
      {/* Average rating summary */}
      {stats && (
        <div className="rounded-xl border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 p-5">
          <div className="flex flex-col sm:flex-row gap-6">
            {/* Left: average */}
            <div className="flex flex-col items-center justify-center min-w-[120px]">
              <span className="text-4xl font-bold text-surface-900 dark:text-surface-0">
                {stats.average.toFixed(1)}
              </span>
              <StarRating value={Math.round(stats.average)} readonly size="md" className="mt-1" />
              <span className="text-xs text-surface-800/40 mt-1">
                {stats.totalCount} review{stats.totalCount !== 1 ? "s" : ""}
              </span>
            </div>

            {/* Right: distribution */}
            <div className="flex-1 space-y-1.5">
              {[5, 4, 3, 2, 1].map((star) => {
                const count = stats.distribution[star] ?? 0;
                const pct = formatDistributionBar(count, stats.totalCount);
                return (
                  <div key={star} className="flex items-center gap-2">
                    <span className="text-xs text-surface-800/50 w-6 text-right">{star}</span>
                    <svg className="w-3.5 h-3.5 text-amber-400" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                    </svg>
                    <div className="flex-1 h-2 rounded-full bg-surface-100 dark:bg-surface-800 overflow-hidden">
                      <div
                        className="h-full rounded-full bg-amber-400 transition-all duration-300"
                        style={{ width: pct }}
                      />
                    </div>
                    <span className="text-xs text-surface-800/30 w-8">{count}</span>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}

      {/* Controls bar */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-sm text-surface-800/50">Sort by:</span>
          {(["newest", "highest", "lowest"] as SortOption[]).map((option) => (
            <button
              key={option}
              onClick={() => { setSort(option); setPage(1); }}
              className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors capitalize ${
                sort === option
                  ? "bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900"
                  : "bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700"
              }`}
            >
              {option}
            </button>
          ))}
        </div>

        <button
          onClick={() => setShowWriteModal(true)}
          className="rounded-lg bg-brand-600 text-white px-4 py-1.5 text-xs font-medium hover:bg-brand-500 transition-colors"
        >
          Write a Review
        </button>
      </div>

      {/* Review list */}
      {isLoading ? (
        <div className="space-y-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="rounded-xl border border-surface-200 dark:border-surface-700 p-4">
              <div className="skeleton-shimmer h-4 w-1/3 mb-3 rounded" />
              <div className="skeleton-shimmer h-3 w-full mb-2 rounded" />
              <div className="skeleton-shimmer h-3 w-2/3 rounded" />
            </div>
          ))}
        </div>
      ) : sortedReviews.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-sm text-surface-800/40">No reviews yet. Be the first to review!</p>
        </div>
      ) : (
        <div className="space-y-4">
          {sortedReviews.map((review) => (
            <ReviewCard key={review.id} review={review} />
          ))}
        </div>
      )}

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-4">
          <button
            onClick={() => setPage((p) => Math.max(1, p - 1))}
            disabled={page <= 1}
            className="rounded-lg px-3 py-1.5 text-xs font-medium bg-surface-100 dark:bg-surface-800 text-surface-800/60 dark:text-surface-200/60 disabled:opacity-40 transition-colors"
          >
            Previous
          </button>
          <span className="text-xs text-surface-800/40">
            Page {page} of {totalPages}
          </span>
          <button
            onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
            disabled={page >= totalPages}
            className="rounded-lg px-3 py-1.5 text-xs font-medium bg-surface-100 dark:bg-surface-800 text-surface-800/60 dark:text-surface-200/60 disabled:opacity-40 transition-colors"
          >
            Next
          </button>
        </div>
      )}

      {/* Write review modal */}
      {showWriteModal && (
        <WriteReviewModal
          modelId={modelId}
          onClose={() => setShowWriteModal(false)}
          onSubmit={loadData}
        />
      )}
    </div>
  );
}
