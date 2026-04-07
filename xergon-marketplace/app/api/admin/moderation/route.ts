import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ContentType = "review" | "forum_post" | "model_listing";
export type ContentStatus = "pending" | "approved" | "dismissed" | "deleted";

export interface FlaggedContent {
  id: string;
  type: ContentType;
  title: string;
  author: string;
  authorPk: string;
  flagCount: number;
  reason: string;
  status: ContentStatus;
  flaggedAt: string;
  resolvedAt?: string;
  resolvedBy?: string;
  contentPreview: string;
  autoFlagged: boolean;
  autoFlagReason?: string;
  suspiciousPatterns: string[];
}

// ---------------------------------------------------------------------------
// Auto-flagging helpers
// ---------------------------------------------------------------------------

const PROFANITY_LIST = [
  "fuck", "shit", "ass", "damn", "hell", "bitch", "crap",
  "dick", "piss", "bastard", "loser", "idiot", "moron",
];

const SPAM_PATTERNS = [
  /(?:https?:\/\/)?[^\s]+\.(com|net|org|xyz|link|top|click)/gi,
  /(?:buy|cheap|free|discount|sale|offer|deal)\s+\w+/gi,
  /(?:subscribe|sign.?up|register|click.?here|check.?out)/gi,
  /\b\w{10,}\s+\b\w{10,}\s+\b\w{10,}/, // long repeated words
];

const SUSPICIOUS_PATTERNS = [
  "all caps content",
  "excessive punctuation",
  "repeated content",
  "external links",
  "spam keywords",
  "profanity detected",
  "no rental history",
  "new account activity",
];

function detectProfanity(text: string): boolean {
  const lower = text.toLowerCase();
  return PROFANITY_LIST.some((word) => lower.includes(word));
}

function detectSpam(text: string): boolean {
  return SPAM_PATTERNS.some((pattern) => pattern.test(text));
}

function isAllCaps(text: string): boolean {
  const letters = text.replace(/[^a-zA-Z]/g, "");
  return letters.length > 10 && (letters.toUpperCase() === letters);
}

function hasExcessivePunctuation(text: string): boolean {
  return /[!?.]{4,}/.test(text);
}

function autoFlagContent(content: string, authorInfo?: { rentalCount?: number; accountAge?: number }): {
  shouldFlag: boolean;
  patterns: string[];
  reason?: string;
} {
  const patterns: string[] = [];

  if (detectProfanity(content)) {
    patterns.push("profanity detected");
  }
  if (detectSpam(content)) {
    patterns.push("spam keywords");
  }
  if (isAllCaps(content)) {
    patterns.push("all caps content");
  }
  if (hasExcessivePunctuation(content)) {
    patterns.push("excessive punctuation");
  }
  if (/https?:\/\//i.test(content)) {
    patterns.push("external links");
  }

  if (authorInfo?.rentalCount === 0) {
    patterns.push("no rental history");
  }
  if (authorInfo?.accountAge && authorInfo.accountAge < 7 * 86_400_000) {
    patterns.push("new account activity");
  }

  const shouldFlag = patterns.length >= 1;
  const reason = patterns.length > 0 ? patterns.join(", ") : undefined;

  return { shouldFlag, patterns, reason };
}

// ---------------------------------------------------------------------------
// In-memory store
// ---------------------------------------------------------------------------

const flaggedContent: FlaggedContent[] = [
  {
    id: "flag-1",
    type: "review",
    title: "Inappropriate language in review",
    author: "0xabc...123",
    authorPk: "abc123",
    flagCount: 3,
    reason: "Contains offensive language",
    status: "pending",
    flaggedAt: new Date(Date.now() - 300_000).toISOString(),
    contentPreview: "This provider is terrible and uses bad words in responses...",
    autoFlagged: true,
    autoFlagReason: "profanity detected",
    suspiciousPatterns: ["profanity detected"],
  },
  {
    id: "flag-2",
    type: "forum_post",
    title: "Spam post in General Discussion",
    author: "0xdef...456",
    authorPk: "def456",
    flagCount: 5,
    reason: "Spam / self-promotion",
    status: "pending",
    flaggedAt: new Date(Date.now() - 3_600_000).toISOString(),
    contentPreview: "Check out my amazing service at external-link.com, buy now for cheap!",
    autoFlagged: true,
    autoFlagReason: "spam keywords, external links",
    suspiciousPatterns: ["spam keywords", "external links"],
  },
  {
    id: "flag-3",
    type: "model_listing",
    title: "Misleading model description",
    author: "0x789...abc",
    authorPk: "789abc",
    flagCount: 2,
    reason: "Model claims capabilities it does not have",
    status: "pending",
    flaggedAt: new Date(Date.now() - 86_400_000).toISOString(),
    contentPreview: "This model achieves 99% accuracy on ALL BENCHMARKS!!! BUY NOW!!!",
    autoFlagged: true,
    autoFlagReason: "excessive punctuation, all caps content",
    suspiciousPatterns: ["excessive punctuation", "all caps content"],
  },
  {
    id: "flag-4",
    type: "review",
    title: "Fake review detected",
    author: "0x111...222",
    authorPk: "111222",
    flagCount: 7,
    reason: "Reviewer has no rental history with this provider",
    status: "pending",
    flaggedAt: new Date(Date.now() - 172_800_000).toISOString(),
    contentPreview: "Best provider ever! 10/10 would rent again!",
    autoFlagged: true,
    autoFlagReason: "no rental history, new account activity",
    suspiciousPatterns: ["no rental history", "new account activity"],
  },
  {
    id: "flag-5",
    type: "forum_post",
    title: "Off-topic discussion",
    author: "0x333...444",
    authorPk: "333444",
    flagCount: 1,
    reason: "Completely unrelated to Xergon marketplace",
    status: "approved",
    flaggedAt: new Date(Date.now() - 259_200_000).toISOString(),
    resolvedAt: new Date(Date.now() - 200_000_000).toISOString(),
    contentPreview: "What do you think about the weather today?",
    autoFlagged: false,
    suspiciousPatterns: [],
  },
  {
    id: "flag-6",
    type: "review",
    title: "Suspicious 5-star review burst",
    author: "0x555...666",
    authorPk: "555666",
    flagCount: 4,
    reason: "Multiple 5-star reviews from new accounts within minutes",
    status: "pending",
    flaggedAt: new Date(Date.now() - 600_000).toISOString(),
    contentPreview: "Absolutely amazing provider, best I've ever used! Highly recommend to everyone!",
    autoFlagged: true,
    autoFlagReason: "no rental history, new account activity",
    suspiciousPatterns: ["no rental history", "new account activity"],
  },
];

// ---------------------------------------------------------------------------
// GET /api/admin/moderation — list flagged content
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const type = searchParams.get("type");
  const status = searchParams.get("status");
  const sort = searchParams.get("sort") ?? "newest";

  let list = [...flaggedContent];

  // Filters
  if (type && type !== "all") {
    list = list.filter((f) => f.type === type);
  }
  if (status && status !== "all") {
    list = list.filter((f) => f.status === status);
  }

  // Sort
  switch (sort) {
    case "newest":
      list.sort((a, b) => new Date(b.flaggedAt).getTime() - new Date(a.flaggedAt).getTime());
      break;
    case "most_flags":
      list.sort((a, b) => b.flagCount - a.flagCount);
      break;
    case "unresolved":
      list.sort((a, b) => {
        if (a.status === "pending" && b.status !== "pending") return -1;
        if (a.status !== "pending" && b.status === "pending") return 1;
        return new Date(b.flaggedAt).getTime() - new Date(a.flaggedAt).getTime();
      });
      break;
  }

  // Stats
  const stats = {
    total: flaggedContent.length,
    pending: flaggedContent.filter((f) => f.status === "pending").length,
    approved: flaggedContent.filter((f) => f.status === "approved").length,
    dismissed: flaggedContent.filter((f) => f.status === "dismissed").length,
    deleted: flaggedContent.filter((f) => f.status === "deleted").length,
    autoFlagged: flaggedContent.filter((f) => f.autoFlagged).length,
    averageFlagCount:
      flaggedContent.length > 0
        ? Math.round(flaggedContent.reduce((s, f) => s + f.flagCount, 0) / flaggedContent.length * 10) / 10
        : 0,
  };

  return NextResponse.json({
    items: list,
    stats,
    total: list.length,
  });
}

// ---------------------------------------------------------------------------
// POST /api/admin/moderation — create a flag or bulk action
// ---------------------------------------------------------------------------

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { action, ids } = body as { action?: string; ids?: string[] };

    if (action === "bulk_approve" && Array.isArray(ids)) {
      for (const id of ids) {
        const item = flaggedContent.find((f) => f.id === id);
        if (item) {
          item.status = "approved";
          item.resolvedAt = new Date().toISOString();
          item.resolvedBy = "admin";
        }
      }
      return NextResponse.json({ success: true, action: "bulk_approve", count: ids.length });
    }

    if (action === "bulk_dismiss" && Array.isArray(ids)) {
      for (const id of ids) {
        const item = flaggedContent.find((f) => f.id === id);
        if (item) {
          item.status = "dismissed";
          item.resolvedAt = new Date().toISOString();
          item.resolvedBy = "admin";
        }
      }
      return NextResponse.json({ success: true, action: "bulk_dismiss", count: ids.length });
    }

    if (action === "bulk_delete" && Array.isArray(ids)) {
      for (const id of ids) {
        const item = flaggedContent.find((f) => f.id === id);
        if (item) {
          item.status = "deleted";
          item.resolvedAt = new Date().toISOString();
          item.resolvedBy = "admin";
        }
      }
      return NextResponse.json({ success: true, action: "bulk_delete", count: ids.length });
    }

    // Auto-flag content
    const { content, type, title, author, authorPk } = body as {
      content?: string;
      type?: ContentType;
      title?: string;
      author?: string;
      authorPk?: string;
    };

    if (content && type) {
      const { shouldFlag, patterns, reason } = autoFlagContent(content);

      if (shouldFlag) {
        const newFlag: FlaggedContent = {
          id: `flag-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
          type,
          title: title ?? "Auto-flagged content",
          author: author ?? "unknown",
          authorPk: authorPk ?? "unknown",
          flagCount: 0,
          reason: reason ?? "Auto-detected suspicious patterns",
          status: "pending",
          flaggedAt: new Date().toISOString(),
          contentPreview: content.slice(0, 200),
          autoFlagged: true,
          autoFlagReason: reason,
          suspiciousPatterns: patterns,
        };
        flaggedContent.unshift(newFlag);
        return NextResponse.json({ flagged: true, id: newFlag.id, patterns });
      }

      return NextResponse.json({ flagged: false });
    }

    return NextResponse.json({ error: "Invalid action" }, { status: 400 });
  } catch {
    return NextResponse.json({ error: "Invalid request body" }, { status: 400 });
  }
}
