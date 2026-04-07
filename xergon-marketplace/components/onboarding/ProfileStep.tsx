"use client";

import { User, Globe, Github, Twitter, ImagePlus, X } from "lucide-react";

const AVAILABLE_TAGS = [
  "NLP",
  "Vision",
  "Code",
  "Audio",
  "Multimodal",
  "Research",
  "Gaming",
  "Finance",
] as const;

export type Tag = (typeof AVAILABLE_TAGS)[number];

interface ProfileStepProps {
  value: {
    displayName: string;
    bio: string;
    avatarUrl: string;
    tags: Tag[];
    website: string;
    twitter: string;
    github: string;
  };
  onChange: (update: Partial<ProfileStepProps["value"]>) => void;
}

export default function ProfileStep({ value, onChange }: ProfileStepProps) {
  const toggleTag = (tag: Tag) => {
    const tags = value.tags.includes(tag)
      ? value.tags.filter((t) => t !== tag)
      : [...value.tags, tag];
    onChange({ tags });
  };

  return (
    <div className="space-y-6">
      <div className="text-center space-y-3">
        <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-violet-500 to-purple-600 shadow-lg shadow-violet-500/20">
          <User className="h-8 w-8 text-white" />
        </div>
        <h2 className="text-xl font-bold text-surface-900 dark:text-surface-0">
          Set Up Your Profile
        </h2>
        <p className="text-sm text-surface-800/60 dark:text-surface-300/60 max-w-md mx-auto">
          Tell the community about yourself. You can always update this later.
        </p>
      </div>

      <div className="mx-auto max-w-lg space-y-5">
        {/* Display Name */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Display Name
          </label>
          <input
            type="text"
            value={value.displayName}
            onChange={(e) => onChange({ displayName: e.target.value })}
            placeholder="Satoshi"
            maxLength={32}
            className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2.5 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
          />
          <p className="mt-1 text-xs text-surface-800/50 dark:text-surface-300/50">
            {value.displayName.length}/32 characters
          </p>
        </div>

        {/* Bio */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Bio
          </label>
          <textarea
            value={value.bio}
            onChange={(e) => onChange({ bio: e.target.value })}
            placeholder="AI researcher and Ergo enthusiast..."
            maxLength={280}
            rows={3}
            className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2.5 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500 resize-none"
          />
          <p className="mt-1 text-xs text-surface-800/50 dark:text-surface-300/50">
            {value.bio.length}/280 characters
          </p>
        </div>

        {/* Avatar URL */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Avatar URL
          </label>
          <div className="flex gap-2">
            <div className="relative flex-1">
              <ImagePlus className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-surface-400" />
              <input
                type="url"
                value={value.avatarUrl}
                onChange={(e) => onChange({ avatarUrl: e.target.value })}
                placeholder="https://example.com/avatar.png"
                className="block w-full rounded-lg border border-surface-300 bg-surface-0 py-2.5 pl-10 pr-3 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
              />
            </div>
            {value.avatarUrl && (
              <button
                type="button"
                onClick={() => onChange({ avatarUrl: "" })}
                className="flex h-10 w-10 items-center justify-center rounded-lg border border-surface-300 text-surface-500 hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800 transition-colors"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
          {value.avatarUrl && (
            <div className="mt-2">
              <img
                src={value.avatarUrl}
                alt="Avatar preview"
                className="h-12 w-12 rounded-full object-cover border-2 border-surface-200 dark:border-surface-700"
                onError={(e) => {
                  (e.target as HTMLImageElement).style.display = "none";
                }}
              />
            </div>
          )}
        </div>

        {/* Interest Tags */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-2">
            Interests
          </label>
          <div className="flex flex-wrap gap-2">
            {AVAILABLE_TAGS.map((tag) => {
              const isSelected = value.tags.includes(tag);
              return (
                <button
                  key={tag}
                  type="button"
                  onClick={() => toggleTag(tag)}
                  className={`rounded-full px-3 py-1.5 text-xs font-medium transition-all ${
                    isSelected
                      ? "bg-emerald-500 text-white shadow-sm"
                      : "bg-surface-100 text-surface-600 hover:bg-surface-200 dark:bg-surface-800 dark:text-surface-400 dark:hover:bg-surface-700"
                  }`}
                >
                  {tag}
                </button>
              );
            })}
          </div>
        </div>

        {/* Social Links */}
        <div className="space-y-3 pt-2">
          <p className="text-sm font-medium text-surface-700 dark:text-surface-300">
            Social Links
          </p>
          <div className="relative">
            <Globe className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-surface-400" />
            <input
              type="url"
              value={value.website}
              onChange={(e) => onChange({ website: e.target.value })}
              placeholder="https://yoursite.com"
              className="block w-full rounded-lg border border-surface-300 bg-surface-0 py-2.5 pl-10 pr-3 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
            />
          </div>
          <div className="relative">
            <Twitter className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-surface-400" />
            <input
              type="text"
              value={value.twitter}
              onChange={(e) => onChange({ twitter: e.target.value })}
              placeholder="@handle"
              className="block w-full rounded-lg border border-surface-300 bg-surface-0 py-2.5 pl-10 pr-3 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
            />
          </div>
          <div className="relative">
            <Github className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-surface-400" />
            <input
              type="text"
              value={value.github}
              onChange={(e) => onChange({ github: e.target.value })}
              placeholder="username"
              className="block w-full rounded-lg border border-surface-300 bg-surface-0 py-2.5 pl-10 pr-3 text-sm placeholder:text-surface-400 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900 dark:placeholder:text-surface-500"
            />
          </div>
        </div>
      </div>
    </div>
  );
}
