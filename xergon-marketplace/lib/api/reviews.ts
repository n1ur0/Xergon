/**
 * Reviews API — fetch helpers for model reviews and ratings.
 *
 * All requests go through the marketplace API proxy routes.
 */

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

export interface ReviewStats {
  average: number;
  totalCount: number;
  distribution: Record<number, number>; // { 5: 12, 4: 8, 3: 3, 2: 1, 1: 0 }
}

export interface SubmitReviewPayload {
  modelId: string;
  rating: number;
  title?: string;
  text: string;
  tags?: string[];
}

export interface UpdateReviewPayload {
  rating?: number;
  title?: string;
  text?: string;
  tags?: string[];
}

// ---------------------------------------------------------------------------
// API base
// ---------------------------------------------------------------------------

const API = "/api/reviews";

async function handleResponse<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.message ?? `API error ${res.status}`);
  }
  return res.json();
}

// ---------------------------------------------------------------------------
// Fetch helpers
// ---------------------------------------------------------------------------

/**
 * Fetch reviews for a model.
 */
export async function fetchReviews(
  modelId: string,
  sort: "newest" | "highest" | "lowest" = "newest",
  page: number = 1,
  pageSize: number = 10
): Promise<Review[]> {
  const params = new URLSearchParams({
    modelId,
    sort,
    page: String(page),
    pageSize: String(pageSize),
  });
  const res = await fetch(`${API}?${params}`);
  return handleResponse<Review[]>(res);
}

/**
 * Fetch review statistics for a model.
 */
export async function fetchReviewStats(modelId: string): Promise<ReviewStats> {
  const res = await fetch(`${API}/stats?modelId=${modelId}`);
  return handleResponse<ReviewStats>(res);
}

/**
 * Submit a new review.
 */
export async function submitReview(payload: SubmitReviewPayload): Promise<Review> {
  const res = await fetch(API, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  return handleResponse<Review>(res);
}

/**
 * Update an existing review.
 */
export async function updateReview(
  reviewId: string,
  payload: UpdateReviewPayload
): Promise<Review> {
  const res = await fetch(`${API}/${reviewId}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  return handleResponse<Review>(res);
}

/**
 * Delete a review.
 */
export async function deleteReview(reviewId: string): Promise<void> {
  const res = await fetch(`${API}/${reviewId}`, { method: "DELETE" });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.message ?? `API error ${res.status}`);
  }
}
