"use client";

import { useState, useMemo } from "react";
import { cn } from "@/lib/utils";
import {
  ForumPostCard,
  type ForumPost,
  type ForumCategory,
  CATEGORY_CONFIG,
} from "./ForumPost";

// ── Types ──

interface ForumListProps {
  posts: ForumPost[];
  onPostClick?: (postId: string) => void;
  onVote?: (postId: string, vote: "up" | "down") => void;
  onCreatePost?: () => void;
  className?: string;
}

type SortOption = "newest" | "most-voted" | "most-replied";

// ── Mock Data ──

const MOCK_POSTS: ForumPost[] = [
  {
    id: "1",
    title: "Best practices for running Llama 3.3 70B on consumer GPUs?",
    author: "0x3f8a...b2c1",
    date: "2 hours ago",
    content: "I'm looking to run Llama 3.3 70B on my RTX 4090 with GGUF quantization. What quantization level do you recommend for a good balance of quality and speed?",
    category: "models",
    tags: ["llama", "gpu", "quantization"],
    votes: 15,
    replyCount: 8,
    viewCount: 234,
  },
  {
    id: "2",
    title: "Feature Request: Streaming response progress indicators",
    author: "0x7d2e...f4a9",
    date: "5 hours ago",
    content: "It would be great to have a token-by-token streaming progress indicator in the playground. This would help users understand how the model is processing their request.",
    category: "feature-requests",
    tags: ["playground", "streaming", "ux"],
    votes: 22,
    replyCount: 12,
    viewCount: 456,
  },
  {
    id: "3",
    title: "Provider setup guide for new operators",
    author: "0xa1b2...c3d4",
    date: "1 day ago",
    content: "I've put together a comprehensive guide for setting up a new Xergon provider node. Covers hardware requirements, software installation, and configuration best practices.",
    category: "providers",
    tags: ["guide", "setup", "tutorial"],
    votes: 34,
    replyCount: 15,
    viewCount: 678,
  },
  {
    id: "4",
    title: "How to troubleshoot connection timeout errors?",
    author: "0xe5f6...g7h8",
    date: "1 day ago",
    content: "I'm getting frequent timeout errors when connecting to providers. Is there a way to increase the timeout or should I look for providers with lower latency?",
    category: "support",
    tags: ["timeout", "troubleshooting"],
    votes: 8,
    replyCount: 5,
    viewCount: 123,
  },
  {
    id: "5",
    title: "Welcome to the Xergon Community Forum!",
    author: "xergon-team",
    date: "3 days ago",
    content: "Welcome to the official Xergon community forum! This is a space for users, providers, and developers to discuss all things related to the Xergon decentralized AI marketplace.",
    category: "general",
    tags: ["welcome", "community"],
    votes: 45,
    replyCount: 20,
    viewCount: 1200,
  },
  {
    id: "6",
    title: "Comparing Qwen 3.5 vs Mistral Small for code generation",
    author: "0x9i0j...k1l2",
    date: "2 days ago",
    content: "I ran some benchmarks comparing Qwen 3.5 4B and Mistral Small 24B for code generation tasks. Here are my findings on accuracy, speed, and cost efficiency.",
    category: "models",
    tags: ["qwen", "mistral", "benchmark", "code"],
    votes: 28,
    replyCount: 18,
    viewCount: 890,
  },
];

// ── Component ──

export function ForumList({
  posts: initialPosts,
  onPostClick,
  onVote,
  onCreatePost,
  className,
}: ForumListProps) {
  const posts = initialPosts.length > 0 ? initialPosts : MOCK_POSTS;
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<ForumCategory | "all">("all");
  const [sortBy, setSortBy] = useState<SortOption>("newest");
  const [currentPage, setCurrentPage] = useState(1);
  const postsPerPage = 10;

  const filteredPosts = useMemo(() => {
    let result = [...posts];

    // Search
    if (search.trim()) {
      const q = search.toLowerCase();
      result = result.filter(
        (p) =>
          p.title.toLowerCase().includes(q) ||
          p.content.toLowerCase().includes(q) ||
          p.tags.some((t) => t.toLowerCase().includes(q)),
      );
    }

    // Category filter
    if (categoryFilter !== "all") {
      result = result.filter((p) => p.category === categoryFilter);
    }

    // Sort
    switch (sortBy) {
      case "newest":
        result.sort((a, b) => {
          // Simple sort by id (lower id = older in mock data)
          return parseInt(b.id) - parseInt(a.id);
        });
        break;
      case "most-voted":
        result.sort((a, b) => b.votes - a.votes);
        break;
      case "most-replied":
        result.sort((a, b) => b.replyCount - a.replyCount);
        break;
    }

    return result;
  }, [posts, search, categoryFilter, sortBy]);

  const totalPages = Math.ceil(filteredPosts.length / postsPerPage);
  const paginatedPosts = filteredPosts.slice(
    (currentPage - 1) * postsPerPage,
    currentPage * postsPerPage,
  );

  const categories: (ForumCategory | "all")[] = [
    "all",
    "general",
    "support",
    "feature-requests",
    "models",
    "providers",
  ];

  return (
    <div className={cn("space-y-4", className)}>
      {/* Search + Controls */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="flex-1">
          <input
            type="text"
            placeholder="Search posts..."
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setCurrentPage(1);
            }}
            className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
          />
        </div>
        {onCreatePost && (
          <button
            onClick={onCreatePost}
            className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 transition-colors shrink-0"
          >
            + New Post
          </button>
        )}
      </div>

      {/* Category filters */}
      <div className="flex flex-wrap gap-1.5">
        {categories.map((cat) => {
          const label = cat === "all" ? "All" : CATEGORY_CONFIG[cat].label;
          return (
            <button
              key={cat}
              onClick={() => {
                setCategoryFilter(cat);
                setCurrentPage(1);
              }}
              className={cn(
                "rounded-full px-3 py-1 text-xs font-medium transition-colors",
                categoryFilter === cat
                  ? cat === "all"
                    ? "bg-surface-900 text-white"
                    : CATEGORY_CONFIG[cat as ForumCategory].color
                  : "bg-surface-100 text-surface-800/50 hover:bg-surface-200",
              )}
            >
              {label}
            </button>
          );
        })}
      </div>

      {/* Sort */}
      <div className="flex items-center gap-3 text-xs text-surface-800/50">
        <span>Sort by:</span>
        {(
          [
            ["newest", "Newest"],
            ["most-voted", "Most Voted"],
            ["most-replied", "Most Replied"],
          ] as const
        ).map(([opt, label]) => (
          <button
            key={opt}
            onClick={() => setSortBy(opt)}
            className={cn(
              "rounded px-2 py-1 transition-colors",
              sortBy === opt
                ? "bg-brand-50 text-brand-700 font-medium"
                : "hover:bg-surface-100",
            )}
          >
            {label}
          </button>
        ))}
        <span className="ml-auto">{filteredPosts.length} post{filteredPosts.length !== 1 ? "s" : ""}</span>
      </div>

      {/* Post list */}
      <div className="space-y-3">
        {paginatedPosts.map((post) => (
          <ForumPostCard
            key={post.id}
            post={post}
            onClick={onPostClick}
            onVote={onVote}
          />
        ))}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-1 pt-4">
          {Array.from({ length: totalPages }).map((_, i) => (
            <button
              key={i}
              onClick={() => setCurrentPage(i + 1)}
              className={cn(
                "h-8 w-8 rounded-lg text-xs font-medium transition-colors",
                currentPage === i + 1
                  ? "bg-brand-600 text-white"
                  : "text-surface-800/50 hover:bg-surface-100",
              )}
            >
              {i + 1}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
