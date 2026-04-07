"use client";

import { useState, useCallback } from "react";
import { cn } from "@/lib/utils";
import { type ForumCategory, CATEGORY_CONFIG } from "./ForumPost";

// ── Types ──

interface CreatePostModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSubmit?: (post: {
    title: string;
    category: ForumCategory;
    content: string;
    tags: string[];
  }) => void;
}

// ── Component ──

export function CreatePostModal({ isOpen, onClose, onSubmit }: CreatePostModalProps) {
  const [title, setTitle] = useState("");
  const [category, setCategory] = useState<ForumCategory>("general");
  const [content, setContent] = useState("");
  const [tagInput, setTagInput] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  const [showPreview, setShowPreview] = useState(false);

  const categories: ForumCategory[] = [
    "general",
    "support",
    "feature-requests",
    "models",
    "providers",
  ];

  const handleAddTag = useCallback(() => {
    const tag = tagInput.trim().toLowerCase();
    if (tag && !tags.includes(tag) && tags.length < 5) {
      setTags((prev) => [...prev, tag]);
      setTagInput("");
    }
  }, [tagInput, tags]);

  const handleRemoveTag = useCallback((tag: string) => {
    setTags((prev) => prev.filter((t) => t !== tag));
  }, []);

  const handleSubmit = () => {
    if (!title.trim() || !content.trim()) return;
    onSubmit?.({ title: title.trim(), category, content: content.trim(), tags });
    handleClose();
  };

  const handleClose = () => {
    setTitle("");
    setCategory("general");
    setContent("");
    setTags([]);
    setTagInput("");
    setShowPreview(false);
    onClose();
  };

  if (!isOpen) return null;

  const canSubmit = title.trim().length > 0 && content.trim().length > 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" onClick={handleClose} />

      {/* Modal */}
      <div className="relative w-full max-w-2xl mx-4 rounded-2xl bg-surface-0 shadow-xl border border-surface-200 overflow-hidden max-h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-surface-100 shrink-0">
          <h2 className="text-lg font-semibold text-surface-900">Create Post</h2>
          <button
            onClick={handleClose}
            className="rounded-lg p-1.5 text-surface-800/40 hover:bg-surface-100 hover:text-surface-800/70 transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          {/* Title */}
          <div>
            <label className="block text-xs font-medium text-surface-800/50 mb-1">
              Title
            </label>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="What's your post about?"
              maxLength={200}
              className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
            />
            <div className="text-[10px] text-surface-800/30 mt-1 text-right">
              {title.length}/200
            </div>
          </div>

          {/* Category */}
          <div>
            <label className="block text-xs font-medium text-surface-800/50 mb-1">
              Category
            </label>
            <div className="flex flex-wrap gap-1.5">
              {categories.map((cat) => (
                <button
                  key={cat}
                  onClick={() => setCategory(cat)}
                  className={cn(
                    "rounded-full px-3 py-1 text-xs font-medium transition-colors",
                    category === cat
                      ? CATEGORY_CONFIG[cat].color
                      : "bg-surface-100 text-surface-800/50 hover:bg-surface-200",
                  )}
                >
                  {CATEGORY_CONFIG[cat].label}
                </button>
              ))}
            </div>
          </div>

          {/* Content */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="block text-xs font-medium text-surface-800/50">
                Content
              </label>
              <button
                onClick={() => setShowPreview(!showPreview)}
                className={cn(
                  "text-xs font-medium transition-colors",
                  showPreview ? "text-brand-600" : "text-surface-800/40 hover:text-surface-800/60",
                )}
              >
                {showPreview ? "Edit" : "Preview"}
              </button>
            </div>
            {showPreview ? (
              <div className="min-h-[200px] rounded-lg border border-surface-200 bg-surface-50 p-3 text-sm text-surface-800/70 whitespace-pre-wrap">
                {content || <span className="text-surface-800/30">Nothing to preview</span>}
              </div>
            ) : (
              <textarea
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder="Write your post content... (Markdown supported)"
                rows={8}
                className="w-full resize-none rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
              />
            )}
          </div>

          {/* Tags */}
          <div>
            <label className="block text-xs font-medium text-surface-800/50 mb-1">
              Tags (max 5)
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
                placeholder="Add a tag..."
                className="flex-1 rounded-lg border border-surface-200 bg-surface-0 px-3 py-1.5 text-xs placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
              />
              <button
                onClick={handleAddTag}
                disabled={!tagInput.trim() || tags.length >= 5}
                className="rounded-lg border border-surface-200 px-3 py-1.5 text-xs text-surface-800/50 hover:bg-surface-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                Add
              </button>
            </div>
            {tags.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-2">
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 rounded-full bg-surface-100 px-2 py-0.5 text-[10px] text-surface-800/50"
                  >
                    {tag}
                    <button
                      onClick={() => handleRemoveTag(tag)}
                      className="text-surface-800/30 hover:text-red-500 transition-colors"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M18 6 6 18" />
                        <path d="m6 6 12 12" />
                      </svg>
                    </button>
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 px-6 py-4 border-t border-surface-100 shrink-0">
          <button
            onClick={handleClose}
            className="rounded-lg border border-surface-200 px-4 py-2 text-sm font-medium text-surface-800/60 hover:bg-surface-50 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Post
          </button>
        </div>
      </div>
    </div>
  );
}
