use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Days, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ReviewTag
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReviewTag {
    Performance,
    Accuracy,
    Latency,
    Reliability,
    Documentation,
    Value,
}

impl ReviewTag {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "performance" => Some(Self::Performance),
            "accuracy" => Some(Self::Accuracy),
            "latency" => Some(Self::Latency),
            "reliability" => Some(Self::Reliability),
            "documentation" => Some(Self::Documentation),
            "value" => Some(Self::Value),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ReviewStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
    Flagged,
}

// ---------------------------------------------------------------------------
// ReviewVote
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ReviewVote {
    Helpful,
    NotHelpful,
}

// ---------------------------------------------------------------------------
// FlagReason
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum FlagReason {
    Spam,
    Inappropriate,
    OffTopic,
    FakeReview,
    Other(String),
}

// ---------------------------------------------------------------------------
// ReviewResponse
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewResponse {
    pub responder_id: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Review
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Review {
    pub id: String,
    pub model_id: String,
    pub reviewer_id: String,
    pub rating: u8,
    pub text: String,
    pub tags: Vec<ReviewTag>,
    pub status: ReviewStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub helpful_votes: u64,
    pub not_helpful_votes: u64,
    pub response: Option<ReviewResponse>,
    pub flag_reason: Option<FlagReason>,
    pub flagged_by: Option<String>,
    pub is_spam: bool,
    /// Normalised lowercase text used for duplicate detection.
    pub normalized_text: String,
}

// ---------------------------------------------------------------------------
// ReviewSort
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ReviewSort {
    Newest,
    Oldest,
    HighestRated,
    LowestRated,
    MostHelpful,
    LeastHelpful,
}

// ---------------------------------------------------------------------------
// SpamCheckResult
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SpamCheckResult {
    Clean,
    SpamShort,
    SpamDuplicate,
    SpamFrequency,
}

// ---------------------------------------------------------------------------
// ModelReviewStats  (per-model aggregation)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelReviewStats {
    pub model_id: String,
    pub total_reviews: u64,
    pub average_rating: f64,
    pub bayesian_average: f64,
    pub rating_distribution: HashMap<u8, u64>,
    pub reviews_by_tag: HashMap<String, u64>,
    pub helpful_ratio: f64,
}

// ---------------------------------------------------------------------------
// ReviewerStats
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewerStats {
    pub reviewer_id: String,
    pub total_reviews: u64,
    pub average_rating: f64,
    pub total_helpful_votes: u64,
    pub models_reviewed: u64,
}

// ---------------------------------------------------------------------------
// ReviewTrendBucket
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewTrendBucket {
    pub date: String,
    pub count: u64,
    pub average_rating: f64,
}

// ---------------------------------------------------------------------------
// SubmitReviewRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubmitReviewRequest {
    pub model_id: String,
    pub reviewer_id: String,
    pub rating: u8,
    pub text: String,
    pub tags: Vec<ReviewTag>,
}

// ---------------------------------------------------------------------------
// ModelReviewSystem
// ---------------------------------------------------------------------------

/// Global Bayesian prior: assumed mean rating across all models.
const BAYESIAN_PRIOR_MEAN: f64 = 3.5;
/// Global Bayesian prior: minimum number of reviews to trust the raw average.
const BAYESIAN_PRIOR_C: u64 = 5;

/// Minimum review text length (characters) to not be considered spam-short.
const MIN_REVIEW_TEXT_LEN: usize = 10;
/// Maximum number of reviews a single user can post per model.
const MAX_REVIEWS_PER_USER_MODEL: u64 = 3;
/// Minimum hours between reviews by the same user on the same model.
const MIN_REVIEW_INTERVAL_HOURS: i64 = 24;

pub struct ModelReviewSystem {
    /// review_id -> Review
    reviews: DashMap<String, Review>,
    /// model_id -> set of review_ids
    model_reviews: DashMap<String, Vec<String>>,
    /// reviewer_id -> set of review_ids
    reviewer_reviews: DashMap<String, Vec<String>>,
    /// reviewer_id + model_id -> count (for frequency limiting)
    review_counts: DashMap<String, u64>,
    /// reviewer_id + model_id -> last review DateTime (raw epoch seconds for simple comparison)
    last_review_time: DashMap<String, i64>,
    /// moderation queue: review_ids pending moderation
    moderation_queue: DashMap<String, bool>,
    /// (reviewer_id, model_id) -> set of normalized texts (for duplicate detection)
    reviewer_model_texts: DashMap<String, Vec<String>>,
    /// Global counters
    total_reviews: AtomicU64,
    total_approved: AtomicU64,
    total_rejected: AtomicU64,
    total_flagged: AtomicU64,
    /// Global rating sum + count for Bayesian prior computation
    global_rating_sum: AtomicU64,
    global_rating_count: AtomicU64,
}

impl ModelReviewSystem {
    /// Create a new, empty review system.
    pub fn new() -> Self {
        Self {
            reviews: DashMap::new(),
            model_reviews: DashMap::new(),
            reviewer_reviews: DashMap::new(),
            review_counts: DashMap::new(),
            last_review_time: DashMap::new(),
            moderation_queue: DashMap::new(),
            reviewer_model_texts: DashMap::new(),
            total_reviews: AtomicU64::new(0),
            total_approved: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
            total_flagged: AtomicU64::new(0),
            global_rating_sum: AtomicU64::new(0),
            global_rating_count: AtomicU64::new(0),
        }
    }

    // ===================================================================
    // Spam Detection
    // ===================================================================

    /// Run all spam heuristics on a review submission. Returns the first
    /// failure or `SpamCheckResult::Clean`.
    pub fn check_spam(&self, req: &SubmitReviewRequest) -> SpamCheckResult {
        // 1. Short spam
        if req.text.trim().len() < MIN_REVIEW_TEXT_LEN {
            return SpamCheckResult::SpamShort;
        }

        // 2. Duplicate text from the same reviewer on the same model
        let key = format!("{}:{}", req.reviewer_id, req.model_id);
        let normalized = req.text.trim().to_lowercase();
        if let Some(texts) = self.reviewer_model_texts.get(&key) {
            if texts.iter().any(|t| t == &normalized) {
                return SpamCheckResult::SpamDuplicate;
            }
        }

        // 3. Frequency: too many reviews on the same model
        let count = self
            .review_counts
            .get(&key)
            .map(|v| *v)
            .unwrap_or(0);
        if count >= MAX_REVIEWS_PER_USER_MODEL {
            return SpamCheckResult::SpamFrequency;
        }

        // 4. Frequency: minimum time between reviews
        if let Some(last) = self.last_review_time.get(&key) {
            let now = Utc::now().timestamp();
            let elapsed_hours = (now - *last) / 3600;
            if elapsed_hours < MIN_REVIEW_INTERVAL_HOURS {
                return SpamCheckResult::SpamFrequency;
            }
        }

        SpamCheckResult::Clean
    }

    // ===================================================================
    // Review Submission
    // ===================================================================

    /// Submit a new review. Runs spam checks first; returns an error string
    /// on failure, or the review ID on success.
    ///
    /// New reviews start in `ReviewStatus::Pending`.
    pub fn submit_review(&self, req: SubmitReviewRequest) -> Result<String, String> {
        // Validate rating
        if req.rating < 1 || req.rating > 5 {
            return Err("Rating must be between 1 and 5".to_string());
        }

        // Validate tags
        if req.tags.is_empty() {
            return Err("At least one tag is required".to_string());
        }

        // Spam checks
        let spam = self.check_spam(&req);
        if spam != SpamCheckResult::Clean {
            return Err(format!("Review rejected as spam: {:?}", spam));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let normalized = req.text.trim().to_lowercase();
        let now = Utc::now();

        let review = Review {
            id: id.clone(),
            model_id: req.model_id.clone(),
            reviewer_id: req.reviewer_id.clone(),
            rating: req.rating,
            text: req.text.trim().to_string(),
            tags: req.tags.clone(),
            status: ReviewStatus::Pending,
            created_at: now,
            updated_at: now,
            helpful_votes: 0,
            not_helpful_votes: 0,
            response: None,
            flag_reason: None,
            flagged_by: None,
            is_spam: false,
            normalized_text: normalized.clone(),
        };

        // Store the review
        self.reviews.insert(id.clone(), review);

        // Index by model
        self.model_reviews
            .entry(req.model_id.clone())
            .or_insert_with(Vec::new)
            .push(id.clone());

        // Index by reviewer
        self.reviewer_reviews
            .entry(req.reviewer_id.clone())
            .or_insert_with(Vec::new)
            .push(id.clone());

        // Update frequency tracking
        let freq_key = format!("{}:{}", req.reviewer_id, req.model_id);
        *self.review_counts.entry(freq_key.clone()).or_insert(0) += 1;
        self.last_review_time.insert(freq_key.clone(), now.timestamp());

        // Store normalized text for duplicate detection
        self.reviewer_model_texts
            .entry(freq_key)
            .or_insert_with(Vec::new)
            .push(normalized);

        // Add to moderation queue
        self.moderation_queue.insert(id.clone(), true);

        // Update global counters
        self.total_reviews.fetch_add(1, Ordering::Relaxed);

        Ok(id)
    }

    // ===================================================================
    // Review Moderation
    // ===================================================================

    /// Approve a pending review. Returns `true` if the review was found
    /// and was in a pending state.
    pub fn approve_review(&self, review_id: &str) -> bool {
        let mut updated = false;
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            if review.status == ReviewStatus::Pending {
                review.status = ReviewStatus::Approved;
                review.updated_at = Utc::now();
                self.total_approved.fetch_add(1, Ordering::Relaxed);
                self.global_rating_sum
                    .fetch_add(review.rating as u64, Ordering::Relaxed);
                self.global_rating_count.fetch_add(1, Ordering::Relaxed);
                updated = true;
            }
        }
        if updated {
            self.moderation_queue.remove(review_id);
        }
        updated
    }

    /// Reject a pending review. Returns `true` if the review was found
    /// and was in a pending or flagged state.
    pub fn reject_review(&self, review_id: &str) -> bool {
        let mut updated = false;
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            if review.status == ReviewStatus::Pending || review.status == ReviewStatus::Flagged {
                review.status = ReviewStatus::Rejected;
                review.updated_at = Utc::now();
                self.total_rejected.fetch_add(1, Ordering::Relaxed);
                updated = true;
            }
        }
        if updated {
            self.moderation_queue.remove(review_id);
        }
        updated
    }

    /// Flag a review for moderation. Returns `true` if the review was
    /// successfully flagged (must be in Approved or Pending state).
    pub fn flag_review(
        &self,
        review_id: &str,
        flagged_by: &str,
        reason: FlagReason,
    ) -> bool {
        let mut updated = false;
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            if review.status == ReviewStatus::Approved || review.status == ReviewStatus::Pending {
                let was_approved = review.status == ReviewStatus::Approved;
                review.status = ReviewStatus::Flagged;
                review.flag_reason = Some(reason);
                review.flagged_by = Some(flagged_by.to_string());
                review.updated_at = Utc::now();
                self.total_flagged.fetch_add(1, Ordering::Relaxed);
                // If was approved, undo the global rating contribution
                if was_approved {
                    self.global_rating_sum
                        .fetch_sub(review.rating as u64, Ordering::Relaxed);
                    self.global_rating_count.fetch_sub(1, Ordering::Relaxed);
                    self.total_approved.fetch_sub(1, Ordering::Relaxed);
                }
                updated = true;
            }
        }
        if updated {
            self.moderation_queue.insert(review_id.to_string(), true);
        }
        updated
    }

    /// Get the current moderation queue (review IDs in Pending or Flagged state).
    pub fn get_moderation_queue(&self) -> Vec<String> {
        self.moderation_queue
            .iter()
            .map(|kv| kv.key().clone())
            .collect()
    }

    // ===================================================================
    // Rating Aggregation
    // ===================================================================

    /// Get a specific review by ID.
    pub fn get_review(&self, review_id: &str) -> Option<Review> {
        self.reviews.get(review_id).map(|r| r.clone())
    }

    /// Get all reviews for a model (all statuses).
    pub fn get_model_reviews(&self, model_id: &str) -> Vec<Review> {
        let ids = self
            .model_reviews
            .get(model_id)
            .map(|v| v.clone())
            .unwrap_or_default();
        ids.iter()
            .filter_map(|id| self.reviews.get(id).map(|r| r.clone()))
            .collect()
    }

    /// Get only approved reviews for a model.
    fn get_approved_reviews(&self, model_id: &str) -> Vec<Review> {
        self.get_model_reviews(model_id)
            .into_iter()
            .filter(|r| r.status == ReviewStatus::Approved)
            .collect()
    }

    /// Compute simple average rating from approved reviews.
    pub fn average_rating(&self, model_id: &str) -> f64 {
        let approved = self.get_approved_reviews(model_id);
        if approved.is_empty() {
            return 0.0;
        }
        let sum: f64 = approved.iter().map(|r| r.rating as f64).sum();
        sum / approved.len() as f64
    }

    /// Compute the rating distribution (1..=5 -> count) from approved reviews.
    pub fn rating_distribution(&self, model_id: &str) -> HashMap<u8, u64> {
        let mut dist = HashMap::new();
        for i in 1u8..=5 {
            dist.insert(i, 0);
        }
        for r in self.get_approved_reviews(model_id) {
            *dist.entry(r.rating).or_insert(0) += 1;
        }
        dist
    }

    /// Compute a Bayesian average to give a fair score even when there are
    /// very few reviews.
    ///
    /// Formula: `(C * m + sum) / (C + n)`
    /// where `C` = prior count, `m` = prior mean, `n` = actual count,
    /// `sum` = actual sum of ratings.
    pub fn bayesian_average(&self, model_id: &str) -> f64 {
        let approved = self.get_approved_reviews(model_id);
        if approved.is_empty() {
            return 0.0;
        }
        let n = approved.len() as u64;
        let sum: u64 = approved.iter().map(|r| r.rating as u64).sum();
        let c = BAYESIAN_PRIOR_C;
        let m = BAYESIAN_PRIOR_MEAN;
        (c as f64 * m + sum as f64) / (c as f64 + n as f64)
    }

    /// Compute full per-model review statistics.
    pub fn model_review_stats(&self, model_id: &str) -> ModelReviewStats {
        let approved = self.get_approved_reviews(model_id);
        let all = self.get_model_reviews(model_id);

        let total_reviews = all.len() as u64;

        let average_rating = if approved.is_empty() {
            0.0
        } else {
            let sum: f64 = approved.iter().map(|r| r.rating as f64).sum();
            sum / approved.len() as f64
        };

        let bayesian_average = if approved.is_empty() {
            0.0
        } else {
            let n = approved.len() as u64;
            let sum: u64 = approved.iter().map(|r| r.rating as u64).sum();
            let c = BAYESIAN_PRIOR_C;
            let m = BAYESIAN_PRIOR_MEAN;
            (c as f64 * m + sum as f64) / (c as f64 + n as f64)
        };

        let mut rating_distribution = HashMap::new();
        for i in 1u8..=5 {
            rating_distribution.insert(i, 0);
        }
        for r in &approved {
            *rating_distribution.entry(r.rating).or_insert(0) += 1;
        }

        let mut reviews_by_tag = HashMap::new();
        for r in &approved {
            for tag in &r.tags {
                let key = format!("{:?}", tag);
                *reviews_by_tag.entry(key).or_insert(0) += 1;
            }
        }

        let total_votes: u64 = approved.iter().map(|r| r.helpful_votes + r.not_helpful_votes).sum();
        let helpful_votes: u64 = approved.iter().map(|r| r.helpful_votes).sum();
        let helpful_ratio = if total_votes == 0 {
            0.0
        } else {
            helpful_votes as f64 / total_votes as f64
        };

        ModelReviewStats {
            model_id: model_id.to_string(),
            total_reviews,
            average_rating,
            bayesian_average,
            rating_distribution,
            reviews_by_tag,
            helpful_ratio,
        }
    }

    // ===================================================================
    // Review Sorting
    // ===================================================================

    /// Get approved reviews for a model, sorted according to the given criteria.
    pub fn get_sorted_reviews(
        &self,
        model_id: &str,
        sort: ReviewSort,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Vec<Review> {
        let mut reviews = self.get_approved_reviews(model_id);
        let offset = offset.unwrap_or(0);

        match sort {
            ReviewSort::Newest => {
                reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
            ReviewSort::Oldest => {
                reviews.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            }
            ReviewSort::HighestRated => {
                reviews.sort_by(|a, b| b.rating.cmp(&a.rating));
            }
            ReviewSort::LowestRated => {
                reviews.sort_by(|a, b| a.rating.cmp(&b.rating));
            }
            ReviewSort::MostHelpful => {
                reviews.sort_by(|a, b| {
                    b.helpful_votes
                        .cmp(&a.helpful_votes)
                        .then_with(|| b.not_helpful_votes.cmp(&a.not_helpful_votes))
                });
            }
            ReviewSort::LeastHelpful => {
                reviews.sort_by(|a, b| {
                    a.helpful_votes
                        .cmp(&b.helpful_votes)
                        .then_with(|| a.not_helpful_votes.cmp(&b.not_helpful_votes))
                });
            }
        }

        reviews
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .collect()
    }

    /// Get approved reviews for a model filtered by a specific tag.
    pub fn get_reviews_by_tag(&self, model_id: &str, tag: ReviewTag) -> Vec<Review> {
        self.get_approved_reviews(model_id)
            .into_iter()
            .filter(|r| r.tags.contains(&tag))
            .collect()
    }

    // ===================================================================
    // Review Responses
    // ===================================================================

    /// Add a provider response to a review. The review must be approved.
    /// Returns `true` on success.
    pub fn respond_to_review(&self, review_id: &str, responder_id: &str, text: &str) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            if review.status == ReviewStatus::Approved {
                review.response = Some(ReviewResponse {
                    responder_id: responder_id.to_string(),
                    text: text.trim().to_string(),
                    created_at: Utc::now(),
                });
                review.updated_at = Utc::now();
                return true;
            }
        }
        false
    }

    /// Update an existing provider response. Returns `true` on success.
    pub fn update_response(&self, review_id: &str, text: &str) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            if let Some(ref mut resp) = review.response {
                resp.text = text.trim().to_string();
                review.updated_at = Utc::now();
                return true;
            }
        }
        false
    }

    // ===================================================================
    // Review Voting
    // ===================================================================

    /// Cast a helpful/not-helpful vote on a review. The review must be approved.
    /// Each voter can only vote once per review. Returns `true` on success.
    pub fn vote_review(
        &self,
        review_id: &str,
        voter_id: &str,
        vote: ReviewVote,
    ) -> bool {
        // Track votes per (review_id, voter_id) to prevent duplicates.
        // We reuse the reviewer_model_texts map pattern with a "votes:" prefix key.
        let vote_key = format!("votes:{}:{}", review_id, voter_id);
        if self.reviewer_model_texts.contains_key(&vote_key) {
            return false; // Already voted
        }
        self.reviewer_model_texts
            .insert(vote_key.clone(), vec!["voted".to_string()]);

        if let Some(mut review) = self.reviews.get_mut(review_id) {
            if review.status != ReviewStatus::Approved {
                // Undo the vote tracking since review wasn't approved
                self.reviewer_model_texts.remove(&vote_key);
                return false;
            }
            match vote {
                ReviewVote::Helpful => review.helpful_votes += 1,
                ReviewVote::NotHelpful => review.not_helpful_votes += 1,
            }
            return true;
        }
        // Review not found, clean up
        self.reviewer_model_texts.remove(&vote_key);
        false
    }

    // ===================================================================
    // Review Analytics
    // ===================================================================

    /// Compute stats for a specific reviewer.
    pub fn reviewer_stats(&self, reviewer_id: &str) -> ReviewerStats {
        let ids = self
            .reviewer_reviews
            .get(reviewer_id)
            .map(|v| v.clone())
            .unwrap_or_default();

        let reviews: Vec<Review> = ids
            .iter()
            .filter_map(|id| self.reviews.get(id).map(|r| r.clone()))
            .filter(|r| r.status == ReviewStatus::Approved)
            .collect();

        let total_reviews = reviews.len() as u64;
        let average_rating = if total_reviews == 0 {
            0.0
        } else {
            reviews.iter().map(|r| r.rating as f64).sum::<f64>() / total_reviews as f64
        };
        let total_helpful_votes: u64 = reviews.iter().map(|r| r.helpful_votes).sum();
        let models_reviewed = reviews
            .iter()
            .map(|r| r.model_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .len() as u64;

        ReviewerStats {
            reviewer_id: reviewer_id.to_string(),
            total_reviews,
            average_rating,
            total_helpful_votes,
            models_reviewed,
        }
    }

    /// Get daily review trend data for a model over the last `days` days.
    pub fn review_trends(&self, model_id: &str, days: u32) -> Vec<ReviewTrendBucket> {
        let mut buckets: Vec<ReviewTrendBucket> = Vec::new();
        let today = Utc::now().date_naive();

        for i in (0..days).rev() {
            let date = today - Days::new(i as u64);
            let date_str = date.format("%Y-%m-%d").to_string();

            let reviews: Vec<Review> = self
                .reviews
                .iter()
                .filter(|kv| {
                    let r = kv.value();
                    r.model_id == model_id
                        && r.status == ReviewStatus::Approved
                        && r.created_at.date_naive() == date
                })
                .map(|kv| kv.value().clone())
                .collect();

            let count = reviews.len() as u64;
            let average_rating = if count == 0 {
                0.0
            } else {
                reviews.iter().map(|r| r.rating as f64).sum::<f64>() / count as f64
            };

            buckets.push(ReviewTrendBucket {
                date: date_str,
                count,
                average_rating,
            });
        }

        buckets
    }

    /// Get the total number of reviews across all models and statuses.
    pub fn total_review_count(&self) -> u64 {
        self.total_reviews.load(Ordering::Relaxed)
    }

    /// Get the total number of approved reviews.
    pub fn total_approved_count(&self) -> u64 {
        self.total_approved.load(Ordering::Relaxed)
    }

    /// Get the total number of rejected reviews.
    pub fn total_rejected_count(&self) -> u64 {
        self.total_rejected.load(Ordering::Relaxed)
    }

    /// Get the total number of flagged reviews.
    pub fn total_flagged_count(&self) -> u64 {
        self.total_flagged.load(Ordering::Relaxed)
    }

    /// Get the moderation queue size.
    pub fn moderation_queue_size(&self) -> usize {
        self.moderation_queue.len()
    }

    /// Delete a review by ID. Returns `true` if the review existed and was removed.
    pub fn delete_review(&self, review_id: &str) -> bool {
        if let Some((_, review)) = self.reviews.remove(review_id) {
            // Remove from model index
            if let Some(mut ids) = self.model_reviews.get_mut(&review.model_id) {
                ids.retain(|id| id != review_id);
            }
            // Remove from reviewer index
            if let Some(mut ids) = self.reviewer_reviews.get_mut(&review.reviewer_id) {
                ids.retain(|id| id != review_id);
            }
            // Remove from moderation queue
            self.moderation_queue.remove(review_id);
            // Remove vote tracking entries for this review
            let vote_prefix = format!("votes:{}:", review_id);
            self.reviewer_model_texts
                .retain(|k, _| !k.starts_with(&vote_prefix));

            self.total_reviews.fetch_sub(1, Ordering::Relaxed);
            return true;
        }
        false
    }
}

impl Default for ModelReviewSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(model_id: &str, reviewer_id: &str, rating: u8, text: &str) -> SubmitReviewRequest {
        SubmitReviewRequest {
            model_id: model_id.to_string(),
            reviewer_id: reviewer_id.to_string(),
            rating,
            text: text.to_string(),
            tags: vec![ReviewTag::Performance],
        }
    }

    fn make_system() -> ModelReviewSystem {
        ModelReviewSystem::new()
    }

    fn submit_approved(
        sys: &ModelReviewSystem,
        model_id: &str,
        reviewer_id: &str,
        rating: u8,
        text: &str,
    ) -> String {
        let id = sys
            .submit_review(make_request(model_id, reviewer_id, rating, text))
            .unwrap();
        sys.approve_review(&id);
        id
    }

    // -- Review Submission Tests -----------------------------------------

    #[test]
    fn test_submit_review_success() {
        let sys = make_system();
        let req = make_request("model-1", "user-1", 5, "Excellent model performance");
        let id = sys.submit_review(req).unwrap();
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.rating, 5);
        assert_eq!(review.model_id, "model-1");
        assert_eq!(review.reviewer_id, "user-1");
        assert_eq!(review.status, ReviewStatus::Pending);
        assert_eq!(review.text, "Excellent model performance");
    }

    #[test]
    fn test_submit_review_invalid_rating_zero() {
        let sys = make_system();
        let req = make_request("model-1", "user-1", 0, "This is a valid length review text");
        let result = sys.submit_review(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Rating"));
    }

    #[test]
    fn test_submit_review_invalid_rating_six() {
        let sys = make_system();
        let req = make_request("model-1", "user-1", 6, "This is a valid length review text");
        let result = sys.submit_review(req);
        assert!(result.is_err());
    }

    #[test]
    fn test_submit_review_empty_tags() {
        let sys = make_system();
        let req = SubmitReviewRequest {
            model_id: "model-1".to_string(),
            reviewer_id: "user-1".to_string(),
            rating: 4,
            text: "This is a valid length review text".to_string(),
            tags: vec![],
        };
        let result = sys.submit_review(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("tag"));
    }

    // -- Spam Detection Tests --------------------------------------------

    #[test]
    fn test_spam_short_text() {
        let sys = make_system();
        let req = make_request("model-1", "user-1", 3, "too short");
        let result = sys.check_spam(&req);
        assert_eq!(result, SpamCheckResult::SpamShort);
    }

    #[test]
    fn test_spam_duplicate_text() {
        let sys = make_system();
        let req1 = make_request("model-1", "user-1", 4, "This is a great model indeed");
        let _ = sys.submit_review(req1).unwrap();

        let req2 = make_request("model-1", "user-1", 4, "This is a great model indeed");
        let result = sys.check_spam(&req2);
        assert_eq!(result, SpamCheckResult::SpamDuplicate);
    }

    #[test]
    fn test_spam_frequency_count() {
        let sys = make_system();
        for i in 0..MAX_REVIEWS_PER_USER_MODEL {
            let req = make_request(
                "model-1",
                "user-1",
                4,
                &format!("Review number {} about this model", i),
            );
            sys.submit_review(req).unwrap();
            // Bypass the time check by clearing the last_review_time
            sys.last_review_time.remove("user-1:model-1");
        }
        let req = make_request("model-1", "user-1", 4, "Yet another review for frequency test");
        let result = sys.check_spam(&req);
        assert_eq!(result, SpamCheckResult::SpamFrequency);
    }

    #[test]
    fn test_spam_clean() {
        let sys = make_system();
        let req = make_request("model-1", "user-1", 5, "This model works perfectly for my use case");
        assert_eq!(sys.check_spam(&req), SpamCheckResult::Clean);
    }

    // -- Moderation Tests ------------------------------------------------

    #[test]
    fn test_approve_review() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 5, "Great model for production"))
            .unwrap();
        assert_eq!(sys.moderation_queue_size(), 1);
        assert!(sys.approve_review(&id));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.status, ReviewStatus::Approved);
        assert_eq!(sys.moderation_queue_size(), 0);
    }

    #[test]
    fn test_approve_already_approved_is_noop() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 5, "Great model for production"))
            .unwrap();
        sys.approve_review(&id);
        assert!(!sys.approve_review(&id)); // second call is a no-op
    }

    #[test]
    fn test_reject_review() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 1, "Terrible model"))
            .unwrap();
        assert!(sys.reject_review(&id));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.status, ReviewStatus::Rejected);
        assert_eq!(sys.moderation_queue_size(), 0);
    }

    #[test]
    fn test_flag_review() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 5, "Great model for production"))
            .unwrap();
        sys.approve_review(&id);
        assert!(sys.flag_review(&id, "mod-1", FlagReason::Spam));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.status, ReviewStatus::Flagged);
        assert_eq!(review.flagged_by.as_deref(), Some("mod-1"));
        assert_eq!(sys.moderation_queue_size(), 1);
    }

    #[test]
    fn test_flag_pending_review() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 5, "Great model for production"))
            .unwrap();
        assert!(sys.flag_review(&id, "mod-1", FlagReason::OffTopic));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.status, ReviewStatus::Flagged);
    }

    #[test]
    fn test_reject_flagged_review() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 5, "Great model for production"))
            .unwrap();
        sys.flag_review(&id, "mod-1", FlagReason::FakeReview);
        assert!(sys.reject_review(&id));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.status, ReviewStatus::Rejected);
    }

    #[test]
    fn test_moderation_queue() {
        let sys = make_system();
        let id1 = sys
            .submit_review(make_request("m1", "u1", 5, "Review one about the model"))
            .unwrap();
        let id2 = sys
            .submit_review(make_request("m2", "u2", 4, "Review two about the model"))
            .unwrap();
        let queue = sys.get_moderation_queue();
        assert_eq!(queue.len(), 2);
        assert!(queue.contains(&id1));
        assert!(queue.contains(&id2));

        sys.approve_review(&id1);
        assert_eq!(sys.get_moderation_queue().len(), 1);
    }

    // -- Rating Aggregation Tests ----------------------------------------

    #[test]
    fn test_average_rating() {
        let sys = make_system();
        submit_approved(&sys, "model-1", "user-1", 4, "Pretty good model for everyday use");
        submit_approved(&sys, "model-1", "user-2", 5, "Excellent model for my workflow");
        submit_approved(&sys, "model-1", "user-3", 3, "Decent but needs some work");
        let avg = sys.average_rating("model-1");
        assert!((avg - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_average_rating_no_reviews() {
        let sys = make_system();
        assert_eq!(sys.average_rating("nonexistent"), 0.0);
    }

    #[test]
    fn test_rating_distribution() {
        let sys = make_system();
        submit_approved(&sys, "model-1", "user-1", 5, "Five stars for this amazing model");
        submit_approved(&sys, "model-1", "user-2", 5, "Another five star rating here");
        submit_approved(&sys, "model-1", "user-3", 3, "Three stars for this model");
        submit_approved(&sys, "model-1", "user-4", 1, "One star only bad experience");
        let dist = sys.rating_distribution("model-1");
        assert_eq!(*dist.get(&1).unwrap(), 1);
        assert_eq!(*dist.get(&2).unwrap(), 0);
        assert_eq!(*dist.get(&3).unwrap(), 1);
        assert_eq!(*dist.get(&4).unwrap(), 0);
        assert_eq!(*dist.get(&5).unwrap(), 2);
    }

    #[test]
    fn test_bayesian_average_few_reviews() {
        let sys = make_system();
        // Only 1 review of 5 stars — Bayesian average should pull toward prior (3.5)
        submit_approved(&sys, "model-1", "user-1", 5, "Only review five stars");
        let ba = sys.bayesian_average("model-1");
        let raw = 5.0;
        assert!(ba < raw);
        assert!(ba > BAYESIAN_PRIOR_MEAN);
    }

    #[test]
    fn test_bayesian_average_many_reviews() {
        let sys = make_system();
        // 10 reviews all 5 stars — should be close to 5
        for i in 0..10 {
            submit_approved(
                &sys,
                "model-1",
                &format!("user-{}", i),
                5,
                &format!("Five star review number {}", i),
            );
        }
        // With 10 reviews of 5 stars: (5 * 3.5 + 50) / (5 + 10) = 4.5
        let ba = sys.bayesian_average("model-1");
        assert!(ba > 4.0);
        assert!(ba < 5.0);
    }

    #[test]
    fn test_model_review_stats() {
        let sys = make_system();
        submit_approved(
            &sys,
            "model-1",
            "user-1",
            5,
            "Great performance",
        );
        submit_approved(
            &sys,
            "model-1",
            "user-2",
            3,
            "Average accuracy",
        );
        let stats = sys.model_review_stats("model-1");
        assert_eq!(stats.total_reviews, 2);
        assert!((stats.average_rating - 4.0).abs() < f64::EPSILON);
        assert!(stats.bayesian_average > 0.0);
        assert!(!stats.rating_distribution.is_empty());
    }

    // -- Sorting Tests ---------------------------------------------------

    #[test]
    fn test_sort_newest() {
        let sys = make_system();
        let _id1 = submit_approved(&sys, "model-1", "user-1", 5, "First review of the model");
        let _id2 = submit_approved(&sys, "model-1", "user-2", 4, "Second review of the model");
        let reviews = sys.get_sorted_reviews("model-1", ReviewSort::Newest, None, None);
        assert_eq!(reviews.len(), 2);
        assert!(reviews[0].created_at >= reviews[1].created_at);
    }

    #[test]
    fn test_sort_highest_rated() {
        let sys = make_system();
        submit_approved(&sys, "model-1", "user-1", 3, "Mediocre model experience");
        submit_approved(&sys, "model-1", "user-2", 5, "Excellent model performance");
        submit_approved(&sys, "model-1", "user-3", 1, "Terrible model output");
        let reviews = sys.get_sorted_reviews("model-1", ReviewSort::HighestRated, None, None);
        assert_eq!(reviews[0].rating, 5);
        assert_eq!(reviews[1].rating, 3);
        assert_eq!(reviews[2].rating, 1);
    }

    #[test]
    fn test_sort_most_helpful() {
        let sys = make_system();
        let id1 = submit_approved(&sys, "model-1", "user-1", 5, "Very helpful review here");
        let id2 = submit_approved(&sys, "model-1", "user-2", 4, "Another helpful review here");
        sys.vote_review(&id1, "voter-1", ReviewVote::Helpful);
        sys.vote_review(&id1, "voter-2", ReviewVote::Helpful);
        sys.vote_review(&id1, "voter-3", ReviewVote::Helpful);
        sys.vote_review(&id2, "voter-4", ReviewVote::Helpful);
        let reviews = sys.get_sorted_reviews("model-1", ReviewSort::MostHelpful, None, None);
        assert_eq!(reviews[0].id, id1);
        assert_eq!(reviews[0].helpful_votes, 3);
        assert_eq!(reviews[1].helpful_votes, 1);
    }

    #[test]
    fn test_sort_with_pagination() {
        let sys = make_system();
        for i in 0..5 {
            submit_approved(
                &sys,
                "model-1",
                &format!("user-{}", i),
                5,
                &format!("Review number {} for pagination test", i),
            );
        }
        let page = sys.get_sorted_reviews(
            "model-1",
            ReviewSort::Newest,
            Some(2),
            Some(2),
        );
        assert_eq!(page.len(), 2);
    }

    #[test]
    fn test_filter_by_tag() {
        let sys = make_system();
        let req1 = SubmitReviewRequest {
            model_id: "model-1".to_string(),
            reviewer_id: "user-1".to_string(),
            rating: 5,
            text: "Great performance review text here".to_string(),
            tags: vec![ReviewTag::Performance],
        };
        let id1 = sys.submit_review(req1).unwrap();
        sys.approve_review(&id1);

        let req2 = SubmitReviewRequest {
            model_id: "model-1".to_string(),
            reviewer_id: "user-2".to_string(),
            rating: 3,
            text: "Accuracy is not so great in this model".to_string(),
            tags: vec![ReviewTag::Accuracy],
        };
        let id2 = sys.submit_review(req2).unwrap();
        sys.approve_review(&id2);

        let perf_reviews = sys.get_reviews_by_tag("model-1", ReviewTag::Performance);
        assert_eq!(perf_reviews.len(), 1);
        assert_eq!(perf_reviews[0].id, id1);

        let acc_reviews = sys.get_reviews_by_tag("model-1", ReviewTag::Accuracy);
        assert_eq!(acc_reviews.len(), 1);
        assert_eq!(acc_reviews[0].id, id2);
    }

    // -- Response Tests --------------------------------------------------

    #[test]
    fn test_respond_to_review() {
        let sys = make_system();
        let id = submit_approved(&sys, "model-1", "user-1", 4, "Good model with some issues");
        assert!(sys.respond_to_review(&id, "provider-1", "Thank you for the feedback!"));
        let review = sys.get_review(&id).unwrap();
        assert!(review.response.is_some());
        let resp = review.response.unwrap();
        assert_eq!(resp.responder_id, "provider-1");
        assert_eq!(resp.text, "Thank you for the feedback!");
    }

    #[test]
    fn test_respond_to_non_approved_review() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 4, "Good model with some issues"))
            .unwrap();
        assert!(!sys.respond_to_review(&id, "provider-1", "Can't respond to pending"));
    }

    #[test]
    fn test_update_response() {
        let sys = make_system();
        let id = submit_approved(&sys, "model-1", "user-1", 4, "Good model with some issues");
        sys.respond_to_review(&id, "provider-1", "Initial response");
        assert!(sys.update_response(&id, "Updated response text"));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.response.unwrap().text, "Updated response text");
    }

    // -- Voting Tests ----------------------------------------------------

    #[test]
    fn test_vote_helpful() {
        let sys = make_system();
        let id = submit_approved(&sys, "model-1", "user-1", 5, "Very detailed and helpful review");
        assert!(sys.vote_review(&id, "voter-1", ReviewVote::Helpful));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.helpful_votes, 1);
        assert_eq!(review.not_helpful_votes, 0);
    }

    #[test]
    fn test_vote_not_helpful() {
        let sys = make_system();
        let id = submit_approved(&sys, "model-1", "user-1", 1, "Not helpful review at all");
        assert!(sys.vote_review(&id, "voter-1", ReviewVote::NotHelpful));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.not_helpful_votes, 1);
    }

    #[test]
    fn test_vote_duplicate_prevented() {
        let sys = make_system();
        let id = submit_approved(&sys, "model-1", "user-1", 5, "Great model review text here");
        assert!(sys.vote_review(&id, "voter-1", ReviewVote::Helpful));
        assert!(!sys.vote_review(&id, "voter-1", ReviewVote::Helpful));
        let review = sys.get_review(&id).unwrap();
        assert_eq!(review.helpful_votes, 1);
    }

    #[test]
    fn test_vote_on_pending_review_fails() {
        let sys = make_system();
        let id = sys
            .submit_review(make_request("model-1", "user-1", 5, "Pending review text here"))
            .unwrap();
        assert!(!sys.vote_review(&id, "voter-1", ReviewVote::Helpful));
    }

    // -- Analytics Tests -------------------------------------------------

    #[test]
    fn test_reviewer_stats() {
        let sys = make_system();
        submit_approved(&sys, "model-1", "user-1", 5, "Five stars for this model");
        submit_approved(&sys, "model-2", "user-1", 3, "Three stars for this other model");
        // Also submit a rejected review that should not count
        let id = sys
            .submit_review(make_request("model-3", "user-1", 1, "One star rejected review"))
            .unwrap();
        sys.reject_review(&id);

        let stats = sys.reviewer_stats("user-1");
        assert_eq!(stats.total_reviews, 2); // only approved
        assert!((stats.average_rating - 4.0).abs() < f64::EPSILON);
        assert_eq!(stats.models_reviewed, 2);
    }

    #[test]
    fn test_review_trends() {
        let sys = make_system();
        submit_approved(&sys, "model-1", "user-1", 5, "Five star review for trend test");
        let trends = sys.review_trends("model-1", 7);
        assert_eq!(trends.len(), 7);
        let today_count: u64 = trends.iter().filter(|b| b.count > 0).map(|b| b.count).sum();
        assert_eq!(today_count, 1);
    }

    #[test]
    fn test_global_counters() {
        let sys = make_system();
        assert_eq!(sys.total_review_count(), 0);
        let id1 = sys
            .submit_review(make_request("m1", "u1", 5, "Approved review text here"))
            .unwrap();
        sys.approve_review(&id1);
        let id2 = sys
            .submit_review(make_request("m1", "u2", 1, "Rejected review text here"))
            .unwrap();
        sys.reject_review(&id2);

        assert_eq!(sys.total_review_count(), 2);
        assert_eq!(sys.total_approved_count(), 1);
        assert_eq!(sys.total_rejected_count(), 1);
    }

    // -- Delete Tests ----------------------------------------------------

    #[test]
    fn test_delete_review() {
        let sys = make_system();
        let id = submit_approved(&sys, "model-1", "user-1", 5, "Review to be deleted");
        assert!(sys.get_review(&id).is_some());
        assert!(sys.delete_review(&id));
        assert!(sys.get_review(&id).is_none());
        assert_eq!(sys.total_review_count(), 0);
    }

    #[test]
    fn test_delete_nonexistent_review() {
        let sys = make_system();
        assert!(!sys.delete_review("nonexistent"));
    }

    // -- Default Tests ---------------------------------------------------

    #[test]
    fn test_default() {
        let sys = ModelReviewSystem::default();
        assert_eq!(sys.total_review_count(), 0);
    }
}
