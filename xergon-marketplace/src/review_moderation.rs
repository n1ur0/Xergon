use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ModerationStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModerationStatus {
    Pending,
    Approved,
    Rejected,
    Flagged,
    Escalated,
}

impl ModerationStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Flagged => "Flagged",
            Self::Escalated => "Escalated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Pending" => Some(Self::Pending),
            "Approved" => Some(Self::Approved),
            "Rejected" => Some(Self::Rejected),
            "Flagged" => Some(Self::Flagged),
            "Escalated" => Some(Self::Escalated),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ReviewFlag
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewFlag {
    pub flag_id: String,
    pub review_id: String,
    pub reason: String,
    pub reporter_id: String,
    pub created_at: DateTime<Utc>,
    pub resolved: bool,
}

impl ReviewFlag {
    pub fn new(review_id: &str, reason: &str, reporter_id: &str) -> Self {
        Self {
            flag_id: uuid::Uuid::new_v4().to_string(),
            review_id: review_id.to_string(),
            reason: reason.to_string(),
            reporter_id: reporter_id.to_string(),
            created_at: Utc::now(),
            resolved: false,
        }
    }

    pub fn resolve(&mut self) {
        self.resolved = true;
    }
}

// ---------------------------------------------------------------------------
// ModerationAction
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModerationAction {
    pub action_id: String,
    pub review_id: String,
    pub moderator_id: String,
    pub action: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

impl ModerationAction {
    pub fn new(review_id: &str, moderator_id: &str, action: &str, reason: &str) -> Self {
        Self {
            action_id: uuid::Uuid::new_v4().to_string(),
            review_id: review_id.to_string(),
            moderator_id: moderator_id.to_string(),
            action: action.to_string(),
            reason: reason.to_string(),
            timestamp: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// ModerationConditionType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ModerationConditionType {
    KeywordRegex,
    SpamDetector,
    SentimentAnalysis,
}

impl ModerationConditionType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::KeywordRegex => "KeywordRegex",
            Self::SpamDetector => "SpamDetector",
            Self::SentimentAnalysis => "SentimentAnalysis",
        }
    }
}

// ---------------------------------------------------------------------------
// ModerationRuleAction
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ModerationRuleAction {
    AutoApprove,
    AutoReject,
    FlagForReview,
}

impl ModerationRuleAction {
    pub fn as_str(&self) -> &str {
        match self {
            Self::AutoApprove => "AutoApprove",
            Self::AutoReject => "AutoReject",
            Self::FlagForReview => "FlagForReview",
        }
    }
}

// ---------------------------------------------------------------------------
// ModerationRule
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModerationRule {
    pub rule_id: String,
    pub name: String,
    pub condition_type: ModerationConditionType,
    pub pattern: String,
    pub action: ModerationRuleAction,
    pub enabled: bool,
}

impl ModerationRule {
    pub fn new(
        name: &str,
        condition_type: ModerationConditionType,
        pattern: &str,
        action: ModerationRuleAction,
    ) -> Self {
        Self {
            rule_id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            condition_type,
            pattern: pattern.to_string(),
            action,
            enabled: true,
        }
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }
}

// ---------------------------------------------------------------------------
// ModeratedReview
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModeratedReview {
    pub review_id: String,
    pub model_id: String,
    pub user_id: String,
    pub rating: u32,
    pub content: String,
    pub status: ModerationStatus,
    pub submitted_at: DateTime<Utc>,
    pub moderated_at: Option<DateTime<Utc>>,
    pub moderator_id: Option<String>,
    pub flags: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ModeratedReview {
    pub fn new(review_id: &str, model_id: &str, user_id: &str, rating: u32, content: &str) -> Self {
        Self {
            review_id: review_id.to_string(),
            model_id: model_id.to_string(),
            user_id: user_id.to_string(),
            rating: rating.min(5),
            content: content.to_string(),
            status: ModerationStatus::Pending,
            submitted_at: Utc::now(),
            moderated_at: None,
            moderator_id: None,
            flags: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ModerationQueue
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ModerationQueue {
    reviews: DashMap<String, ModeratedReview>,
    flags: DashMap<String, ReviewFlag>,
    actions: DashMap<String, ModerationAction>,
    rules: DashMap<String, ModerationRule>,
}

impl ModerationQueue {
    pub fn new() -> Self {
        Self {
            reviews: DashMap::new(),
            flags: DashMap::new(),
            actions: DashMap::new(),
            rules: DashMap::new(),
        }
    }

    pub fn default() -> Self {
        Self::new()
    }

    // ---- Review management ----

    pub fn submit_for_review(&self, review: ModeratedReview) -> String {
        let id = review.review_id.clone();
        let auto_result = self.auto_moderate(&review);

        match auto_result {
            Some(ModerationRuleAction::AutoApprove) => {
                let mut r = review.clone();
                r.status = ModerationStatus::Approved;
                r.moderated_at = Some(Utc::now());
                self.reviews.insert(id.clone(), r);
            }
            Some(ModerationRuleAction::AutoReject) => {
                let mut r = review.clone();
                r.status = ModerationStatus::Rejected;
                r.moderated_at = Some(Utc::now());
                self.reviews.insert(id.clone(), r);
            }
            Some(ModerationRuleAction::FlagForReview) => {
                let mut r = review.clone();
                r.status = ModerationStatus::Flagged;
                self.reviews.insert(id.clone(), r);
            }
            None => {
                self.reviews.insert(id.clone(), review);
            }
        }

        id
    }

    pub fn approve(&self, review_id: &str, moderator_id: &str, reason: &str) -> bool {
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            review.status = ModerationStatus::Approved;
            review.moderated_at = Some(Utc::now());
            review.moderator_id = Some(moderator_id.to_string());

            let action = ModerationAction::new(review_id, moderator_id, "approve", reason);
            self.actions.insert(action.action_id.clone(), action);
            true
        } else {
            false
        }
    }

    pub fn reject(&self, review_id: &str, moderator_id: &str, reason: &str) -> bool {
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            review.status = ModerationStatus::Rejected;
            review.moderated_at = Some(Utc::now());
            review.moderator_id = Some(moderator_id.to_string());

            let action = ModerationAction::new(review_id, moderator_id, "reject", reason);
            self.actions.insert(action.action_id.clone(), action);
            true
        } else {
            false
        }
    }

    pub fn flag(&self, review_id: &str, reason: &str, reporter_id: &str) -> Option<String> {
        if !self.reviews.contains_key(review_id) {
            return None;
        }

        let flag = ReviewFlag::new(review_id, reason, reporter_id);
        let flag_id = flag.flag_id.clone();

        if let Some(mut review) = self.reviews.get_mut(review_id) {
            review.status = ModerationStatus::Flagged;
            review.flags.push(flag_id.clone());
        }

        self.flags.insert(flag_id.clone(), flag);
        Some(flag_id)
    }

    pub fn escalate(&self, review_id: &str, moderator_id: &str, reason: &str) -> bool {
        if let Some(mut review) = self.reviews.get_mut(review_id) {
            review.status = ModerationStatus::Escalated;
            review.moderated_at = Some(Utc::now());
            review.moderator_id = Some(moderator_id.to_string());

            let action = ModerationAction::new(review_id, moderator_id, "escalate", reason);
            self.actions.insert(action.action_id.clone(), action);
            true
        } else {
            false
        }
    }

    // ---- Rules ----

    pub fn add_rule(&self, rule: ModerationRule) -> String {
        let id = rule.rule_id.clone();
        self.rules.insert(id.clone(), rule);
        id
    }

    pub fn remove_rule(&self, rule_id: &str) -> bool {
        self.rules.remove(rule_id).is_some()
    }

    pub fn toggle_rule(&self, rule_id: &str, enabled: bool) -> bool {
        if let Some(mut rule) = self.rules.get_mut(rule_id) {
            if enabled {
                rule.enable();
            } else {
                rule.disable();
            }
            true
        } else {
            false
        }
    }

    pub fn get_rules(&self) -> Vec<ModerationRule> {
        self.rules.iter().map(|e| e.value().clone()).collect()
    }

    // ---- Auto-moderation ----

    pub fn auto_moderate(&self, review: &ModeratedReview) -> Option<ModerationRuleAction> {
        let content_lower = review.content.to_lowercase();

        for entry in self.rules.iter() {
            let rule = entry.value();
            if !rule.enabled {
                continue;
            }

            let matches = match rule.condition_type {
                ModerationConditionType::KeywordRegex => {
                    self.matches_keyword_regex(&content_lower, &rule.pattern)
                }
                ModerationConditionType::SpamDetector => {
                    self.detects_spam(&content_lower)
                }
                ModerationConditionType::SentimentAnalysis => {
                    self.negative_sentiment(&content_lower)
                }
            };

            if matches {
                return Some(rule.action.clone());
            }
        }

        None
    }

    fn matches_keyword_regex(&self, content: &str, pattern: &str) -> bool {
        // Simple keyword matching (comma-separated list of banned words)
        let keywords: Vec<&str> = pattern.split(',').map(|k| k.trim()).collect();
        keywords.iter().any(|kw| !kw.is_empty() && content.contains(kw))
    }

    fn detects_spam(&self, content: &str) -> bool {
        // Heuristic spam detection:
        // - Excessive repetition of characters
        // - Very long content with no spaces
        // - High proportion of uppercase
        let uppercase_count = content.chars().filter(|c| c.is_uppercase()).count();
        let total_chars = content.chars().filter(|c| c.is_alphabetic()).count();

        if total_chars > 0 {
            let upper_ratio = uppercase_count as f64 / total_chars as f64;
            if upper_ratio > 0.8 && total_chars > 20 {
                return true;
            }
        }

        // Check for repeated characters (e.g., "!!!!!!")
        let mut prev_char = '\0';
        let mut repeat_count = 0usize;
        for c in content.chars() {
            if c == prev_char && c.is_alphabetic() {
                repeat_count += 1;
                if repeat_count > 5 {
                    return true;
                }
            } else {
                prev_char = c;
                repeat_count = 0;
            }
        }

        // Check for excessive punctuation
        let punct_count = content.chars().filter(|c| !c.is_alphanumeric() && !c.is_whitespace()).count();
        let alpha_count = content.chars().filter(|c| c.is_alphanumeric()).count();
        if alpha_count > 0 && punct_count as f64 / alpha_count as f64 > 0.5 {
            return true;
        }

        false
    }

    fn negative_sentiment(&self, content: &str) -> bool {
        let negative_words = [
            "terrible", "awful", "worst", "horrible", "garbage", "scam", "fraud",
            "useless", "trash", "disgusting", "abysmal", "pathetic",
        ];
        let lower = content.to_lowercase();
        let matches: usize = negative_words
            .iter()
            .filter(|w| lower.contains(*w))
            .count();
        matches >= 3
    }

    // ---- Queue ----

    pub fn get_queue(&self, status: Option<&ModerationStatus>, limit: usize, offset: usize) -> Vec<ModeratedReview> {
        let filtered: Vec<ModeratedReview> = self
            .reviews
            .iter()
            .filter(|e| {
                if let Some(s) = status {
                    &e.value().status == s
                } else {
                    e.value().status == ModerationStatus::Pending
                        || e.value().status == ModerationStatus::Flagged
                        || e.value().status == ModerationStatus::Escalated
                }
            })
            .map(|e| e.value().clone())
            .collect();

        let end = (offset + limit).min(filtered.len());
        if offset < filtered.len() {
            filtered[offset..end].to_vec()
        } else {
            Vec::new()
        }
    }

    pub fn get_review(&self, review_id: &str) -> Option<ModeratedReview> {
        self.reviews.get(review_id).map(|r| r.clone())
    }

    // ---- Flags ----

    pub fn get_flags(&self, review_id: Option<&str>, resolved: Option<bool>) -> Vec<ReviewFlag> {
        self.flags
            .iter()
            .filter(|e| {
                let f = e.value();
                if let Some(rid) = review_id {
                    if f.review_id != rid {
                        return false;
                    }
                }
                if let Some(res) = resolved {
                    if f.resolved != res {
                        return false;
                    }
                }
                true
            })
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn resolve_flag(&self, flag_id: &str) -> bool {
        if let Some(mut flag) = self.flags.get_mut(flag_id) {
            flag.resolve();
            true
        } else {
            false
        }
    }

    // ---- Actions ----

    pub fn get_actions(&self, review_id: &str) -> Vec<ModerationAction> {
        self.actions
            .iter()
            .filter(|e| e.value().review_id == review_id)
            .map(|e| e.value().clone())
            .collect()
    }

    // ---- Stats ----

    pub fn get_stats(&self) -> serde_json::Value {
        let total = self.reviews.len();
        let mut pending = 0usize;
        let mut approved = 0usize;
        let mut rejected = 0usize;
        let mut flagged = 0usize;
        let mut escalated = 0usize;
        let mut unresolved_flags = 0usize;

        for entry in self.reviews.iter() {
            match entry.value().status {
                ModerationStatus::Pending => pending += 1,
                ModerationStatus::Approved => approved += 1,
                ModerationStatus::Rejected => rejected += 1,
                ModerationStatus::Flagged => flagged += 1,
                ModerationStatus::Escalated => escalated += 1,
            }
        }

        for entry in self.flags.iter() {
            if !entry.value().resolved {
                unresolved_flags += 1;
            }
        }


        serde_json::json!({
            "total_reviews": total,
            "pending": pending,
            "approved": approved,
            "rejected": rejected,
            "flagged": flagged,
            "escalated": escalated,
            "unresolved_flags": unresolved_flags,
            "active_rules": self.rules.iter().filter(|e| e.value().enabled).count(),
            "total_rules": self.rules.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_queue() -> ModerationQueue {
        ModerationQueue::new()
    }

    fn make_review(id: &str, content: &str) -> ModeratedReview {
        ModeratedReview::new(id, "model-1", "user-1", 4, content)
    }

    #[test]
    fn test_submit_for_review() {
        let queue = make_queue();
        let review = make_review("rev-1", "Great model, works well!");
        let id = queue.submit_for_review(review);
        assert_eq!(id, "rev-1");
        let fetched = queue.get_review("rev-1").unwrap();
        assert_eq!(fetched.status, ModerationStatus::Pending);
    }

    #[test]
    fn test_approve_review() {
        let queue = make_queue();
        let review = make_review("rev-1", "Good model");
        queue.submit_for_review(review);
        let ok = queue.approve("rev-1", "mod-1", "Looks good");
        assert!(ok);
        let fetched = queue.get_review("rev-1").unwrap();
        assert_eq!(fetched.status, ModerationStatus::Approved);
        assert_eq!(fetched.moderator_id, Some("mod-1".to_string()));
    }

    #[test]
    fn test_reject_review() {
        let queue = make_queue();
        let review = make_review("rev-1", "Spam content");
        queue.submit_for_review(review);
        let ok = queue.reject("rev-1", "mod-1", "Spam detected");
        assert!(ok);
        let fetched = queue.get_review("rev-1").unwrap();
        assert_eq!(fetched.status, ModerationStatus::Rejected);
    }

    #[test]
    fn test_flag_review() {
        let queue = make_queue();
        let review = make_review("rev-1", "Some review");
        queue.submit_for_review(review);
        let flag_id = queue.flag("rev-1", "Inappropriate content", "user-2");
        assert!(flag_id.is_some());
        let fetched = queue.get_review("rev-1").unwrap();
        assert_eq!(fetched.status, ModerationStatus::Flagged);
        assert_eq!(fetched.flags.len(), 1);
    }

    #[test]
    fn test_escalate_review() {
        let queue = make_queue();
        let review = make_review("rev-1", "Controversial review");
        queue.submit_for_review(review);
        let ok = queue.escalate("rev-1", "mod-1", "Needs senior review");
        assert!(ok);
        let fetched = queue.get_review("rev-1").unwrap();
        assert_eq!(fetched.status, ModerationStatus::Escalated);
    }

    #[test]
    fn test_add_and_remove_rule() {
        let queue = make_queue();
        let rule = ModerationRule::new(
            "Ban bad words",
            ModerationConditionType::KeywordRegex,
            "badword,terrible",
            ModerationRuleAction::AutoReject,
        );
        let id = queue.add_rule(rule);
        let rules = queue.get_rules();
        assert_eq!(rules.len(), 1);

        let removed = queue.remove_rule(&id);
        assert!(removed);
        assert_eq!(queue.get_rules().len(), 0);
    }

    #[test]
    fn test_auto_moderate_keyword() {
        let queue = make_queue();
        let rule = ModerationRule::new(
            "Ban scam",
            ModerationConditionType::KeywordRegex,
            "scam,fraud",
            ModerationRuleAction::AutoReject,
        );
        queue.add_rule(rule);

        let review = make_review("rev-1", "This is a scam!");
        let result = queue.auto_moderate(&review);
        assert_eq!(result, Some(ModerationRuleAction::AutoReject));
    }

    #[test]
    fn test_auto_moderate_no_match() {
        let queue = make_queue();
        let rule = ModerationRule::new(
            "Ban spam",
            ModerationConditionType::KeywordRegex,
            "badword",
            ModerationRuleAction::AutoReject,
        );
        queue.add_rule(rule);

        let review = make_review("rev-1", "This is a perfectly fine review.");
        let result = queue.auto_moderate(&review);
        assert!(result.is_none());
    }

    #[test]
    fn test_spam_detection() {
        let queue = make_queue();
        let rule = ModerationRule::new(
            "Spam detector",
            ModerationConditionType::SpamDetector,
            "",
            ModerationRuleAction::AutoReject,
        );
        queue.add_rule(rule);

        let review = make_review("rev-1", "AAAAAAAHHHHHHHH CHECK THIS OUT!!!!!!!");
        let result = queue.auto_moderate(&review);
        assert_eq!(result, Some(ModerationRuleAction::AutoReject));
    }

    #[test]
    fn test_get_queue() {
        let queue = make_queue();
        queue.submit_for_review(make_review("rev-1", "Review one"));
        queue.submit_for_review(make_review("rev-2", "Review two"));
        queue.approve("rev-1", "mod-1", "OK");

        let pending = queue.get_queue(Some(&ModerationStatus::Pending), 10, 0);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].review_id, "rev-2");
    }

    #[test]
    fn test_get_stats() {
        let queue = make_queue();
        queue.submit_for_review(make_review("rev-1", "Review one"));
        queue.submit_for_review(make_review("rev-2", "Review two"));
        queue.approve("rev-1", "mod-1", "OK");

        let stats = queue.get_stats();
        assert_eq!(stats["total_reviews"], 2);
        assert_eq!(stats["approved"], 1);
        assert_eq!(stats["pending"], 1);
    }

    #[test]
    fn test_resolve_flag() {
        let queue = make_queue();
        queue.submit_for_review(make_review("rev-1", "Some review"));
        let flag_id = queue.flag("rev-1", "Reason", "user-2").unwrap();

        let unresolved = queue.get_flags(None, Some(false));
        assert_eq!(unresolved.len(), 1);

        let ok = queue.resolve_flag(&flag_id);
        assert!(ok);

        let resolved = queue.get_flags(None, Some(true));
        assert_eq!(resolved.len(), 1);
    }
}
