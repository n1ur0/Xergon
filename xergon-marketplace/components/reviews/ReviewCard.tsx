"use client";

import { useState } from "react";
import { StarRating } from "./StarRating";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Review {
  id: string;
  modelId: string;
  authorId: string;
  authorName: string;
  authorAvatar?: string;
  isVerified: boolean;
  rating: number;
  title?: string;
  text: string;
  tags?: string[];
  helpfulCount: number;
  notHelpfulCount: number;
  userVote?: "helpful" | "notHelpful";
  createdAt: string;
  updatedAt?: string;
}

interface ReviewCardProps {
  review: Review;
  onVote?: (reviewId: string, vote: "helpful" | "notHelpful") => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function getInitials(name: string): string {
  return name
    .split(" ")
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ReviewCard({ review, onVote }: ReviewCardProps) {
  const [optimisticVote, setOptimisticVote] = useState<"helpful" | "notHelpful" | null>(
    review.userVote ?? null
  );

  const handleVote = (vote: "helpful" | "notHelpful") => {
    if (optimisticVote === vote) {
      setOptimisticVote(null);
      onVote?.(review.id, "helpful" as never); // toggle off
      return;
    }
    setOptimisticVote(vote);
    onVote?.(review.id, vote);
  };

  const helpfulActive = optimisticVote === "helpful";
  const notHelpfulActive = optimisticVote === "notHelpful";

  const helpfulCount = review.helpfulCount + (helpfulActive ? 1 : 0);
  const notHelpfulCount = review.notHelpfulCount + (notHelpfulActive ? 1 : 0);

  return (
    <article
      className="rounded-xl border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 p-4 transition-all hover:shadow-sm"
      aria-label={`Review by ${review.authorName}`}
    >
      {/* Header: author + rating */}
      <div className="flex items-start gap-3 mb-3">
        {/* Avatar */}
        <div
          className="w-9 h-9 rounded-full bg-brand-100 dark:bg-brand-900/40 flex items-center justify-center
                     text-xs font-semibold text-brand-700 dark:text-brand-300 shrink-0"
          aria-hidden="true"
        >
          {review.authorAvatar ? (
            <img
              src={review.authorAvatar}
              alt={review.authorName}
              className="w-9 h-9 rounded-full object-cover"
            />
          ) : (
            getInitials(review.authorName)
          )}
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-semibold text-surface-900 dark:text-surface-0 truncate">
              {review.authorName}
            </span>
            {review.isVerified && (
              <span className="inline-flex items-center gap-0.5 text-emerald-600 dark:text-emerald-400" title="Verified reviewer">
                <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
                  <path fillRule="evenodd" d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.643.304 1.254.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                </svg>
              </span>
            )}
          </div>
          <div className="flex items-center gap-2 mt-0.5">
            <StarRating value={review.rating} readonly size="sm" />
            <span className="text-xs text-surface-800/40">{formatDate(review.createdAt)}</span>
          </div>
        </div>
      </div>

      {/* Title */}
      {review.title && (
        <h4 className="text-sm font-semibold text-surface-900 dark:text-surface-0 mb-1">
          {review.title}
        </h4>
      )}

      {/* Text */}
      <p className="text-sm text-surface-800/70 dark:text-surface-200/70 leading-relaxed mb-3">
        {review.text}
      </p>

      {/* Tags */}
      {review.tags && review.tags.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mb-3">
          {review.tags.map((tag) => (
            <span
              key={tag}
              className="inline-flex items-center rounded-full bg-surface-100 dark:bg-surface-800 px-2 py-0.5 text-[10px] font-medium text-surface-800/50 dark:text-surface-200/50"
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* Helpful / Not helpful */}
      <div className="flex items-center gap-4 pt-2 border-t border-surface-100 dark:border-surface-800">
        <button
          onClick={() => handleVote("helpful")}
          className={`inline-flex items-center gap-1 text-xs font-medium transition-colors ${
            helpfulActive
              ? "text-brand-600 dark:text-brand-400"
              : "text-surface-800/40 hover:text-surface-800/70 dark:hover:text-surface-200/70"
          }`}
          aria-label="Helpful"
          aria-pressed={helpfulActive}
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
            <path d="M1 8.25a1.25 1.25 0 112.5 0v7.5a1.25 1.25 0 11-2.5 0v-7.5zM6 7V4.5A2.5 2.5 0 018.5 2h1.716a2.5 2.5 0 011.97 1.007l2.507 3.245A2.5 2.5 0 0115.508 7.5H18a2 2 0 012 2v5a2 2 0 01-2 2H8.5A2.5 2.5 0 016 14V7z" />
          </svg>
          {helpfulCount}
        </button>

        <button
          onClick={() => handleVote("notHelpful")}
          className={`inline-flex items-center gap-1 text-xs font-medium transition-colors ${
            notHelpfulActive
              ? "text-red-500 dark:text-red-400"
              : "text-surface-800/40 hover:text-surface-800/70 dark:hover:text-surface-200/70"
          }`}
          aria-label="Not helpful"
          aria-pressed={notHelpfulActive}
        >
          <svg className="w-3.5 h-3.5 rotate-180" viewBox="0 0 20 20" fill="currentColor">
            <path d="M1 8.25a1.25 1.25 0 112.5 0v7.5a1.25 1.25 0 11-2.5 0v-7.5zM6 7V4.5A2.5 2.5 0 018.5 2h1.716a2.5 2.5 0 011.97 1.007l2.507 3.245A2.5 2.5 0 0115.508 7.5H18a2 2 0 012 2v5a2 2 0 01-2 2H8.5A2.5 2.5 0 016 14V7z" />
          </svg>
          {notHelpfulCount}
        </button>
      </div>
    </article>
  );
}
