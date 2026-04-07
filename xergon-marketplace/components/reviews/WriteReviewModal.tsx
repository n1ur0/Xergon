"use client";

import { useState } from "react";
import { StarRating } from "./StarRating";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WriteReviewModalProps {
  modelId: string;
  existingReview?: {
    id: string;
    rating: number;
    title?: string;
    text: string;
    tags?: string[];
  };
  onClose: () => void;
  onSubmit: () => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function WriteReviewModal({
  modelId,
  existingReview,
  onClose,
  onSubmit,
}: WriteReviewModalProps) {
  const [rating, setRating] = useState(existingReview?.rating ?? 0);
  const [title, setTitle] = useState(existingReview?.title ?? "");
  const [text, setText] = useState(existingReview?.text ?? "");
  const [tags, setTags] = useState<string[]>(existingReview?.tags ?? []);
  const [tagInput, setTagInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isEditing = !!existingReview;

  const handleAddTag = () => {
    const trimmed = tagInput.trim().toLowerCase();
    if (trimmed && !tags.includes(trimmed) && tags.length < 5) {
      setTags((prev) => [...prev, trimmed]);
      setTagInput("");
    }
  };

  const handleRemoveTag = (tag: string) => {
    setTags((prev) => prev.filter((t) => t !== tag));
  };

  const handleSubmit = async () => {
    if (rating === 0) {
      setError("Please select a rating");
      return;
    }
    if (text.trim().length < 10) {
      setError("Review must be at least 10 characters");
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      const { submitReview, updateReview } = await import("@/lib/api/reviews");

      if (isEditing && existingReview) {
        await updateReview(existingReview.id, { rating, title, text, tags });
      } else {
        await submitReview({ modelId, rating, title, text, tags });
      }

      onSubmit();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to submit review");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={isEditing ? "Edit review" : "Write a review"}
    >
      <div
        className="w-full max-w-md mx-4 rounded-2xl border border-surface-200 dark:border-surface-700
                   bg-surface-0 dark:bg-surface-900 shadow-xl p-6"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <h3 className="text-lg font-semibold text-surface-900 dark:text-surface-0">
            {isEditing ? "Edit Review" : "Write a Review"}
          </h3>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-surface-800/40 hover:text-surface-800/70 dark:hover:text-surface-200/70 transition-colors"
            aria-label="Close"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        {/* Error */}
        {error && (
          <div className="mb-4 rounded-lg border border-red-200 dark:border-red-800/40 bg-red-50 dark:bg-red-950/20 px-3 py-2 text-sm text-red-600 dark:text-red-400">
            {error}
          </div>
        )}

        {/* Rating */}
        <div className="mb-4">
          <label className="block text-sm font-medium text-surface-900 dark:text-surface-0 mb-2">
            Rating
          </label>
          <StarRating value={rating} onChange={setRating} size="lg" />
          {rating === 0 && (
            <p className="text-xs text-surface-800/30 mt-1">Click a star to rate</p>
          )}
        </div>

        {/* Title */}
        <div className="mb-4">
          <label
            htmlFor="review-title"
            className="block text-sm font-medium text-surface-900 dark:text-surface-0 mb-1.5"
          >
            Title <span className="text-surface-800/30">(optional)</span>
          </label>
          <input
            id="review-title"
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            maxLength={100}
            placeholder="Summarize your experience"
            className="w-full rounded-lg border border-surface-200 dark:border-surface-600 bg-surface-50 dark:bg-surface-800
                       px-3 py-2 text-sm text-surface-900 dark:text-surface-0 placeholder:text-surface-800/30
                       focus:outline-none focus:ring-2 focus:ring-brand-500 transition-colors"
          />
        </div>

        {/* Text */}
        <div className="mb-4">
          <label
            htmlFor="review-text"
            className="block text-sm font-medium text-surface-900 dark:text-surface-0 mb-1.5"
          >
            Review
          </label>
          <textarea
            id="review-text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={4}
            maxLength={2000}
            placeholder="What did you like or dislike? How was the model's performance?"
            className="w-full rounded-lg border border-surface-200 dark:border-surface-600 bg-surface-50 dark:bg-surface-800
                       px-3 py-2 text-sm text-surface-900 dark:text-surface-0 placeholder:text-surface-800/30
                       resize-none focus:outline-none focus:ring-2 focus:ring-brand-500 transition-colors"
          />
          <p className="text-xs text-surface-800/30 mt-1 text-right">
            {text.length}/2000
          </p>
        </div>

        {/* Tags */}
        <div className="mb-5">
          <label className="block text-sm font-medium text-surface-900 dark:text-surface-0 mb-1.5">
            Tags <span className="text-surface-800/30">(optional, max 5)</span>
          </label>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={tagInput}
              onChange={(e) => setTagInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleAddTag();
                }
              }}
              placeholder="e.g. fast, accurate"
              className="flex-1 rounded-lg border border-surface-200 dark:border-surface-600 bg-surface-50 dark:bg-surface-800
                         px-3 py-2 text-sm text-surface-900 dark:text-surface-0 placeholder:text-surface-800/30
                         focus:outline-none focus:ring-2 focus:ring-brand-500 transition-colors"
            />
            <button
              onClick={handleAddTag}
              className="rounded-lg bg-surface-100 dark:bg-surface-800 px-3 py-2 text-xs font-medium text-surface-800/60 dark:text-surface-200/60 hover:bg-surface-200 dark:hover:bg-surface-700 transition-colors"
            >
              Add
            </button>
          </div>
          {tags.length > 0 && (
            <div className="flex flex-wrap gap-1.5 mt-2">
              {tags.map((tag) => (
                <span
                  key={tag}
                  className="inline-flex items-center gap-1 rounded-full bg-brand-50 dark:bg-brand-950/30 px-2.5 py-0.5 text-xs font-medium text-brand-700 dark:text-brand-300"
                >
                  {tag}
                  <button
                    onClick={() => handleRemoveTag(tag)}
                    className="text-brand-400 hover:text-brand-600 dark:hover:text-brand-200 transition-colors"
                    aria-label={`Remove tag ${tag}`}
                  >
                    <svg className="w-3 h-3" viewBox="0 0 20 20" fill="currentColor">
                      <path fillRule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clipRule="evenodd" />
                    </svg>
                  </button>
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Actions */}
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            className="flex-1 rounded-lg border border-surface-200 dark:border-surface-600 px-4 py-2 text-sm font-medium
                       text-surface-800/60 dark:text-surface-200/60 hover:bg-surface-50 dark:hover:bg-surface-800 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={isSubmitting}
            className="flex-1 rounded-lg bg-brand-600 text-white px-4 py-2 text-sm font-medium
                       hover:bg-brand-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isSubmitting ? "Submitting..." : isEditing ? "Update" : "Submit"}
          </button>
        </div>
      </div>
    </div>
  );
}
