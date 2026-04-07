"use client";

import { useState } from "react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import {
  type ForumPost,
  type ForumCategory,
  CATEGORY_CONFIG,
  ForumPostCard,
} from "@/components/forum/ForumPost";

// ── Types ──

interface PostPageClientProps {
  postId: string;
}

interface Reply {
  id: string;
  author: string;
  content: string;
  date: string;
  votes: number;
  userVote?: "up" | "down" | null;
}

// ── Mock Data ──

const MOCK_POST: ForumPost = {
  id: "1",
  title: "Best practices for running Llama 3.3 70B on consumer GPUs?",
  author: "0x3f8a...b2c1",
  date: "2 hours ago",
  content: `I'm looking to run Llama 3.3 70B on my RTX 4090 with GGUF quantization. What quantization level do you recommend for a good balance of quality and speed?

## What I've tried so far

- **Q4_K_M**: Runs at about 8 tok/s, quality seems decent
- **Q5_K_S**: Runs at about 6 tok/s, quality is noticeably better
- **Q8_0**: Runs at about 3 tok/s, quality is great but too slow

## My setup

- GPU: NVIDIA RTX 4090 (24GB VRAM)
- RAM: 64GB DDR5
- CPU: AMD Ryzen 9 7950X
- Using llama.cpp with CUDA backend

Any recommendations would be appreciated!`,
  category: "models",
  tags: ["llama", "gpu", "quantization"],
  votes: 15,
  replyCount: 8,
  viewCount: 234,
};

const MOCK_REPLIES: Reply[] = [
  {
    id: "r1",
    author: "0x7d2e...f4a9",
    content: `I'd recommend **Q5_K_M** as the sweet spot. It gives you about 5-6 tok/s with quality that's nearly indistinguishable from Q8_0 for most tasks.

Key tips:
1. Enable flash attention in llama.cpp
2. Use \`-ngl 99\` to offload all layers to GPU
3. Set threads to your physical core count`,
    date: "1 hour ago",
    votes: 8,
  },
  {
    id: "r2",
    author: "0xa1b2...c3d4",
    content: `Have you considered using **Q4_K_M** with speculative decoding? It can give you a significant speedup (2-3x) while maintaining quality. The draft model runs at Q2_K so it's very fast.

Also, make sure you're using the latest llama.cpp from main - there have been significant CUDA optimizations recently.`,
    date: "45 min ago",
    votes: 5,
  },
  {
    id: "r3",
    author: "0xe5f6...g7h8",
    content: `I'm running a similar setup and found that **Q5_K_S** with context size limited to 4096 tokens gives the best balance for my use case (code generation). 

For longer conversations, I fall back to Q4_K_M. The quality difference is minimal for code tasks but the speed improvement is significant.`,
    date: "30 min ago",
    votes: 3,
  },
];

// ── Component ──

export function PostPageClient({ postId }: PostPageClientProps) {
  const [replies, setReplies] = useState(MOCK_REPLIES);
  const [replyContent, setReplyContent] = useState("");
  const [postVotes, setPostVotes] = useState(MOCK_POST.votes);
  const [postUserVote, setPostUserVote] = useState<"up" | "down" | null>(null);

  const post: ForumPost = {
    ...MOCK_POST,
    id: postId,
    votes: postVotes,
    userVote: postUserVote,
  };

  const handlePostVote = (vote: "up" | "down") => {
    if (postUserVote === vote) {
      setPostUserVote(null);
      setPostVotes((v) => v + (vote === "up" ? -1 : 1));
    } else {
      const prev = postUserVote;
      setPostUserVote(vote);
      if (prev === "up") setPostVotes((v) => v - 2);
      else if (prev === "down") setPostVotes((v) => v + 2);
      else setPostVotes((v) => v + (vote === "up" ? 1 : -1));
    }
  };

  const handleReplyVote = (replyId: string, vote: "up" | "down") => {
    setReplies((prev) =>
      prev.map((r) => {
        if (r.id !== replyId) return r;
        if (r.userVote === vote) {
          return { ...r, userVote: null, votes: r.votes + (vote === "up" ? -1 : 1) };
        }
        const prev = r.userVote;
        let newVotes = r.votes + (vote === "up" ? 1 : -1);
        if (prev === "up") newVotes -= 2;
        else if (prev === "down") newVotes += 2;
        return { ...r, userVote: vote, votes: newVotes };
      }),
    );
  };

  const handleSubmitReply = () => {
    if (!replyContent.trim()) return;
    const newReply: Reply = {
      id: `r-new-${Date.now()}`,
      author: "you",
      content: replyContent.trim(),
      date: "Just now",
      votes: 0,
      userVote: null,
    };
    setReplies((prev) => [...prev, newReply]);
    setReplyContent("");
  };

  const catConfig = CATEGORY_CONFIG[post.category];

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      {/* Back link */}
      <Link
        href="/community"
        className="inline-flex items-center gap-1.5 text-sm text-surface-800/50 hover:text-surface-800/80 mb-6 transition-colors"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="m12 19-7-7 7-7" />
          <path d="M19 12H5" />
        </svg>
        Back to Forum
      </Link>

      {/* Post */}
      <div className="mb-8">
        <div className="flex gap-3">
          {/* Vote column */}
          <div className="flex flex-col items-center gap-0.5 shrink-0">
            <button
              onClick={() => handlePostVote("up")}
              className={cn(
                "rounded p-1 transition-colors",
                postUserVote === "up"
                  ? "text-green-600 bg-green-50"
                  : "text-surface-800/20 hover:bg-surface-50 hover:text-surface-800/50",
              )}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M7 10v12" />
                <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2h0a3.13 3.13 0 0 1 3 3.88Z" />
              </svg>
            </button>
            <span
              className={cn(
                "text-sm font-bold",
                postUserVote === "up"
                  ? "text-green-600"
                  : postUserVote === "down"
                    ? "text-red-600"
                    : "text-surface-800/60",
              )}
            >
              {postVotes}
            </span>
            <button
              onClick={() => handlePostVote("down")}
              className={cn(
                "rounded p-1 transition-colors",
                postUserVote === "down"
                  ? "text-red-600 bg-red-50"
                  : "text-surface-800/20 hover:bg-surface-50 hover:text-surface-800/50",
              )}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M17 14V2" />
                <path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22h0a3.13 3.13 0 0 1-3-3.88Z" />
              </svg>
            </button>
          </div>

          {/* Post content */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-2 flex-wrap">
              <span className={cn("inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium", catConfig.color)}>
                {catConfig.label}
              </span>
              <span className="text-xs text-surface-800/30">by {post.author}</span>
              <span className="text-xs text-surface-800/20">{post.date}</span>
              <span className="text-xs text-surface-800/20">{post.viewCount} views</span>
            </div>

            <h1 className="text-xl font-bold text-surface-900 mb-3">{post.title}</h1>

            {/* Tags */}
            {post.tags.length > 0 && (
              <div className="flex flex-wrap gap-1 mb-4">
                {post.tags.map((tag) => (
                  <span key={tag} className="inline-flex items-center rounded-full bg-surface-100 px-2 py-0.5 text-[10px] text-surface-800/40">
                    {tag}
                  </span>
                ))}
              </div>
            )}

            {/* Content */}
            <div className="prose prose-sm max-w-none text-surface-800/70 whitespace-pre-wrap">
              {post.content}
            </div>
          </div>
        </div>
      </div>

      {/* Reply count */}
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-surface-900">
          {replies.length} Replies
        </h2>
      </div>

      {/* Replies */}
      <div className="space-y-4 mb-8">
        {replies.map((reply) => (
          <div key={reply.id} className="flex gap-3">
            {/* Reply vote column */}
            <div className="flex flex-col items-center gap-0.5 shrink-0">
              <button
                onClick={() => handleReplyVote(reply.id, "up")}
                className={cn(
                  "rounded p-0.5 transition-colors",
                  reply.userVote === "up"
                    ? "text-green-600 bg-green-50"
                    : "text-surface-800/20 hover:bg-surface-50 hover:text-surface-800/50",
                )}
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M7 10v12" />
                  <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2h0a3.13 3.13 0 0 1 3 3.88Z" />
                </svg>
              </button>
              <span
                className={cn(
                  "text-xs font-semibold",
                  reply.userVote === "up"
                    ? "text-green-600"
                    : reply.userVote === "down"
                      ? "text-red-600"
                      : "text-surface-800/50",
                )}
              >
                {reply.votes}
              </span>
              <button
                onClick={() => handleReplyVote(reply.id, "down")}
                className={cn(
                  "rounded p-0.5 transition-colors",
                  reply.userVote === "down"
                    ? "text-red-600 bg-red-50"
                    : "text-surface-800/20 hover:bg-surface-50 hover:text-surface-800/50",
                )}
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M17 14V2" />
                  <path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22h0a3.13 3.13 0 0 1-3-3.88Z" />
                </svg>
              </button>
            </div>

            {/* Reply content */}
            <div className="flex-1 rounded-lg border border-surface-200 bg-surface-0 p-4">
              <div className="flex items-center gap-2 mb-2">
                <span className="text-xs font-medium text-surface-800/60">{reply.author}</span>
                <span className="text-xs text-surface-800/20">{reply.date}</span>
              </div>
              <div className="text-sm text-surface-800/70 whitespace-pre-wrap">{reply.content}</div>
            </div>
          </div>
        ))}
      </div>

      {/* Reply form */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
        <h3 className="text-sm font-medium text-surface-900 mb-2">Write a Reply</h3>
        <textarea
          value={replyContent}
          onChange={(e) => setReplyContent(e.target.value)}
          placeholder="Share your thoughts..."
          rows={4}
          className="w-full resize-none rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
        />
        <div className="flex justify-end mt-2">
          <button
            onClick={handleSubmitReply}
            disabled={!replyContent.trim()}
            className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Reply
          </button>
        </div>
      </div>
    </div>
  );
}
