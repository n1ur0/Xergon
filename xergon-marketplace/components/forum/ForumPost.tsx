"use client";

import { cn } from "@/lib/utils";

// ── Types ──

export interface ForumPost {
  id: string;
  title: string;
  author: string;
  authorAvatar?: string;
  date: string;
  content: string;
  category: ForumCategory;
  tags: string[];
  votes: number;
  replyCount: number;
  viewCount: number;
  userVote?: "up" | "down" | null;
}

export type ForumCategory = "general" | "support" | "feature-requests" | "models" | "providers";

export const CATEGORY_CONFIG: Record<ForumCategory, { label: string; color: string }> = {
  general: { label: "General", color: "bg-surface-100 text-surface-800/60" },
  support: { label: "Support", color: "bg-blue-100 text-blue-700" },
  "feature-requests": { label: "Feature Requests", color: "bg-purple-100 text-purple-700" },
  models: { label: "Models", color: "bg-amber-100 text-amber-700" },
  providers: { label: "Providers", color: "bg-emerald-100 text-emerald-700" },
};

// ── Component ──

interface ForumPostCardProps {
  post: ForumPost;
  onVote?: (postId: string, vote: "up" | "down") => void;
  onClick?: (postId: string) => void;
  compact?: boolean;
}

export function ForumPostCard({
  post,
  onVote,
  onClick,
  compact = false,
}: ForumPostCardProps) {
  const catConfig = CATEGORY_CONFIG[post.category];
  const netVotes = post.votes + (post.userVote === "up" ? 0 : post.userVote === "down" ? 0 : 0);

  return (
    <div
      className={cn(
        "group rounded-xl border border-surface-200 bg-surface-0 transition-all hover:shadow-sm hover:border-surface-300",
        onClick && "cursor-pointer",
      )}
      onClick={() => onClick?.(post.id)}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={(e) => {
        if (onClick && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onClick(post.id);
        }
      }}
    >
      <div className={cn("flex", compact ? "p-3" : "p-4")}>
        {/* Vote column */}
        {onVote && (
          <div className="flex flex-col items-center gap-0.5 mr-3 shrink-0">
            <button
              onClick={(e) => {
                e.stopPropagation();
                onVote(post.id, "up");
              }}
              className={cn(
                "rounded p-0.5 transition-colors",
                post.userVote === "up"
                  ? "text-green-600 bg-green-50"
                  : "text-surface-800/20 hover:bg-surface-50 hover:text-surface-800/50",
              )}
              aria-label="Upvote"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M7 10v12" />
                <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2h0a3.13 3.13 0 0 1 3 3.88Z" />
              </svg>
            </button>
            <span
              className={cn(
                "text-xs font-semibold",
                post.userVote === "up"
                  ? "text-green-600"
                  : post.userVote === "down"
                    ? "text-red-600"
                    : "text-surface-800/50",
              )}
            >
              {netVotes}
            </span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                onVote(post.id, "down");
              }}
              className={cn(
                "rounded p-0.5 transition-colors",
                post.userVote === "down"
                  ? "text-red-600 bg-red-50"
                  : "text-surface-800/20 hover:bg-surface-50 hover:text-surface-800/50",
              )}
              aria-label="Downvote"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M17 14V2" />
                <path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22h0a3.13 3.13 0 0 1-3-3.88Z" />
              </svg>
            </button>
          </div>
        )}

        {/* Content */}
        <div className="flex-1 min-w-0">
          {/* Category + meta row */}
          <div className="flex items-center gap-2 mb-1.5 flex-wrap">
            <span
              className={cn(
                "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
                catConfig.color,
              )}
            >
              {catConfig.label}
            </span>
            <span className="text-[10px] text-surface-800/30">
              by {post.author}
            </span>
            <span className="text-[10px] text-surface-800/20">{post.date}</span>
          </div>

          {/* Title */}
          <h3
            className={cn(
              "font-semibold text-surface-900 group-hover:text-brand-600 transition-colors leading-tight",
              compact ? "text-sm" : "text-base",
            )}
          >
            {post.title}
          </h3>

          {/* Content preview */}
          {!compact && (
            <p className="text-sm text-surface-800/50 mt-1.5 line-clamp-2">
              {post.content}
            </p>
          )}

          {/* Tags */}
          {post.tags.length > 0 && (
            <div className="flex flex-wrap gap-1 mt-2">
              {post.tags.map((tag) => (
                <span
                  key={tag}
                  className="inline-flex items-center rounded-full bg-surface-100 px-1.5 py-0.5 text-[10px] text-surface-800/40"
                >
                  {tag}
                </span>
              ))}
            </div>
          )}

          {/* Stats row */}
          <div className="flex items-center gap-4 mt-2 text-[10px] text-surface-800/30">
            <span className="flex items-center gap-1">
              <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
              </svg>
              {post.replyCount} replies
            </span>
            <span className="flex items-center gap-1">
              <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
              {post.viewCount} views
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
