"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

interface FAQ {
  id: string;
  question: string;
  answer: string;
  category: string;
  helpful: number;
}

interface Article {
  id: string;
  title: string;
  category: string;
  readTime: string;
  views: number;
  excerpt: string;
  updatedAt: string;
}

interface Ticket {
  id: string;
  subject: string;
  category: string;
  priority: "low" | "medium" | "high" | "urgent";
  status: "open" | "in_progress" | "resolved" | "closed";
  description: string;
  createdAt: string;
  updatedAt: string;
  messages: TicketMessage[];
}

interface TicketMessage {
  id: string;
  sender: "user" | "support";
  content: string;
  createdAt: string;
}

interface SupportStats {
  avgResponseTime: string;
  resolvedPercent: number;
  openTickets: number;
  totalResolved: number;
  satisfactionScore: number;
}

type TabId = "faq" | "knowledge-base" | "submit-ticket" | "my-tickets";

// ── Helpers ──

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

function formatRelativeTime(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  const diffMs = now - then;
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return "Just now";
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return `${diffDays}d ago`;
  return formatDate(iso);
}

const PRIORITY_STYLES: Record<string, string> = {
  low: "bg-surface-100 text-surface-800/60",
  medium: "bg-amber-50 text-amber-700",
  high: "bg-orange-50 text-orange-700",
  urgent: "bg-red-50 text-red-700",
};

const STATUS_STYLES: Record<string, string> = {
  open: "bg-blue-50 text-blue-700",
  in_progress: "bg-amber-50 text-amber-700",
  resolved: "bg-emerald-50 text-emerald-700",
  closed: "bg-surface-100 text-surface-800/50",
};

const STATUS_LABELS: Record<string, string> = {
  open: "Open",
  in_progress: "In Progress",
  resolved: "Resolved",
  closed: "Closed",
};

const CATEGORY_ICONS: Record<string, string> = {
  "Getting Started": "🚀",
  Account: "👤",
  Billing: "💳",
  Models: "🤖",
  API: "🔌",
  Technical: "⚙️",
  Security: "🔒",
  Provider: "🏢",
};

// ── Component ──

export default function SupportCenter() {
  const [activeTab, setActiveTab] = useState<TabId>("faq");
  const [search, setSearch] = useState("");
  const [isLoading, setIsLoading] = useState(true);

  // Data states
  const [faqs, setFaqs] = useState<FAQ[]>([]);
  const [faqCategories, setFaqCategories] = useState<string[]>([]);
  const [articles, setArticles] = useState<Article[]>([]);
  const [articleCategories, setArticleCategories] = useState<string[]>([]);
  const [tickets, setTickets] = useState<Ticket[]>([]);
  const [stats, setStats] = useState<SupportStats | null>(null);

  // UI states
  const [expandedFaq, setExpandedFaq] = useState<string | null>(null);
  const [selectedFaqCategory, setSelectedFaqCategory] = useState<string>("All");
  const [helpfulVotes, setHelpfulVotes] = useState<Record<string, boolean>>({});

  // Ticket form
  const [formSubject, setFormSubject] = useState("");
  const [formCategory, setFormCategory] = useState("Technical");
  const [formPriority, setFormPriority] = useState<"low" | "medium" | "high" | "urgent">("medium");
  const [formDescription, setFormDescription] = useState("");
  const [formSubmitting, setFormSubmitting] = useState(false);
  const [formSuccess, setFormSuccess] = useState(false);

  // Ticket detail
  const [selectedTicket, setSelectedTicket] = useState<Ticket | null>(null);
  const [replyText, setReplyText] = useState("");

  // Fetch initial data
  useEffect(() => {
    Promise.all([
      fetch("/api/support?section=faq").then((r) => r.json()),
      fetch("/api/support?section=articles").then((r) => r.json()),
      fetch("/api/support?section=tickets").then((r) => r.json()),
      fetch("/api/support?section=stats").then((r) => r.json()),
    ])
      .then(([faqData, articleData, ticketData, statsData]) => {
        setFaqs(faqData.faqs ?? []);
        setFaqCategories(faqData.categories ?? []);
        setArticles(articleData.articles ?? []);
        setArticleCategories(articleData.categories ?? []);
        setTickets(ticketData.tickets ?? []);
        setStats(statsData.stats ?? null);
      })
      .catch(() => {})
      .finally(() => setIsLoading(false));
  }, []);

  // Filtered data
  const filteredFaqs = useMemo(() => {
    let result = faqs;
    if (selectedFaqCategory !== "All") {
      result = result.filter((f) => f.category === selectedFaqCategory);
    }
    if (search.trim()) {
      const q = search.toLowerCase();
      result = result.filter(
        (f) =>
          f.question.toLowerCase().includes(q) ||
          f.answer.toLowerCase().includes(q),
      );
    }
    return result;
  }, [faqs, selectedFaqCategory, search]);

  const filteredArticles = useMemo(() => {
    if (!search.trim()) return articles;
    const q = search.toLowerCase();
    return articles.filter(
      (a) =>
        a.title.toLowerCase().includes(q) ||
        a.excerpt.toLowerCase().includes(q) ||
        a.category.toLowerCase().includes(q),
    );
  }, [articles, search]);

  // Grouped FAQs
  const groupedFaqs = useMemo(() => {
    const groups: Record<string, FAQ[]> = {};
    for (const faq of filteredFaqs) {
      if (!groups[faq.category]) groups[faq.category] = [];
      groups[faq.category].push(faq);
    }
    return groups;
  }, [filteredFaqs]);

  // Handlers
  const toggleHelpful = useCallback((faqId: string) => {
    setHelpfulVotes((prev) => ({ ...prev, [faqId]: !prev[faqId] }));
  }, []);

  const handleSubmitTicket = useCallback(async () => {
    if (!formSubject.trim() || !formDescription.trim()) return;
    setFormSubmitting(true);
    try {
      const res = await fetch("/api/support", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          subject: formSubject,
          category: formCategory,
          priority: formPriority,
          description: formDescription,
        }),
      });
      const data = await res.json();
      if (data.ticket) {
        setTickets((prev) => [data.ticket, ...prev]);
        setFormSubject("");
        setFormDescription("");
        setFormSuccess(true);
        setTimeout(() => setFormSuccess(false), 3000);
        setActiveTab("my-tickets");
      }
    } catch {}
    setFormSubmitting(false);
  }, [formSubject, formCategory, formPriority, formDescription]);

  const handleAddReply = useCallback(() => {
    if (!replyText.trim() || !selectedTicket) return;
    const newMsg: TicketMessage = {
      id: `msg-${Date.now()}`,
      sender: "user",
      content: replyText,
      createdAt: new Date().toISOString(),
    };
    setSelectedTicket((prev) =>
      prev
        ? {
            ...prev,
            messages: [...prev.messages, newMsg],
            updatedAt: newMsg.createdAt,
          }
        : null,
    );
    setReplyText("");
  }, [replyText, selectedTicket]);

  // ── Tabs config ──
  const tabs: { id: TabId; label: string; icon: string }[] = [
    { id: "faq", label: "FAQ", icon: "❓" },
    { id: "knowledge-base", label: "Knowledge Base", icon: "📚" },
    { id: "submit-ticket", label: "Submit Ticket", icon: "✏️" },
    { id: "my-tickets", label: "My Tickets", icon: "🎫" },
  ];

  // ── Loading ──
  if (isLoading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="h-8 w-48 rounded-lg bg-surface-100 animate-pulse mb-2" />
        <div className="h-4 w-72 rounded bg-surface-50 animate-pulse mb-8" />
        <div className="flex gap-2 mb-6">
          {[1, 2, 3, 4].map((i) => (
            <div key={i} className="h-10 w-32 rounded-lg bg-surface-100 animate-pulse" />
          ))}
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
          <div className="lg:col-span-3 space-y-4">
            {[1, 2, 3, 4, 5].map((i) => (
              <div key={i} className="h-16 rounded-xl bg-surface-50 animate-pulse" />
            ))}
          </div>
          <div className="h-64 rounded-xl bg-surface-50 animate-pulse" />
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-surface-900 mb-1">
          Support Center
        </h1>
        <p className="text-surface-800/60">
          Find answers, browse articles, or get help from our team.
        </p>
      </div>

      {/* Search */}
      <div className="relative mb-6">
        <span className="absolute left-3.5 top-1/2 -translate-y-1/2 text-surface-800/30">
          🔍
        </span>
        <input
          type="text"
          placeholder="Search FAQs, articles, and troubleshooting guides..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full rounded-xl border border-surface-200 bg-surface-0 pl-10 pr-4 py-3 text-sm text-surface-900 placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20 transition-colors"
        />
        {search && (
          <button
            onClick={() => setSearch("")}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-surface-800/30 hover:text-surface-800/60"
          >
            ✕
          </button>
        )}
      </div>

      {/* Tab navigation */}
      <div className="flex flex-wrap gap-1 mb-6 border-b border-surface-200 pb-px">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              "inline-flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium rounded-t-lg border-b-2 transition-colors -mb-px",
              activeTab === tab.id
                ? "border-brand-500 text-brand-600 bg-brand-50/30"
                : "border-transparent text-surface-800/50 hover:text-surface-900 hover:bg-surface-50",
            )}
          >
            <span className="text-base">{tab.icon}</span>
            {tab.label}
            {tab.id === "my-tickets" && tickets.length > 0 && (
              <span className="ml-1 inline-flex items-center justify-center w-5 h-5 rounded-full bg-brand-100 text-brand-700 text-xs font-semibold">
                {tickets.length}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Content grid */}
      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
        {/* Main content */}
        <div className="lg:col-span-3">
          {/* FAQ Tab */}
          {activeTab === "faq" && (
            <div>
              {/* Category filter */}
              <div className="flex flex-wrap gap-2 mb-6">
                <button
                  onClick={() => setSelectedFaqCategory("All")}
                  className={cn(
                    "px-3 py-1.5 rounded-lg text-xs font-medium transition-colors",
                    selectedFaqCategory === "All"
                      ? "bg-brand-500 text-white"
                      : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
                  )}
                >
                  All
                </button>
                {faqCategories.map((cat) => (
                  <button
                    key={cat}
                    onClick={() => setSelectedFaqCategory(cat)}
                    className={cn(
                      "inline-flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors",
                      selectedFaqCategory === cat
                        ? "bg-brand-500 text-white"
                        : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
                    )}
                  >
                    <span>{CATEGORY_ICONS[cat] ?? "📄"}</span>
                    {cat}
                  </button>
                ))}
              </div>

              {Object.keys(groupedFaqs).length === 0 ? (
                <div className="rounded-xl border border-surface-200 bg-surface-0 p-12 text-center">
                  <p className="text-3xl mb-3">🔍</p>
                  <p className="text-surface-800/50 font-medium">No results found</p>
                  <p className="text-sm text-surface-800/40 mt-1">
                    Try a different search term or category.
                  </p>
                </div>
              ) : (
                Object.entries(groupedFaqs).map(([category, items]) => (
                  <div key={category} className="mb-8">
                    <h2 className="text-lg font-semibold text-surface-900 mb-3 flex items-center gap-2">
                      <span>{CATEGORY_ICONS[category] ?? "📄"}</span>
                      {category}
                      <span className="text-xs font-normal text-surface-800/40">
                        ({items.length})
                      </span>
                    </h2>
                    <div className="space-y-2">
                      {items.map((faq) => (
                        <div
                          key={faq.id}
                          className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden"
                        >
                          <button
                            onClick={() =>
                              setExpandedFaq(
                                expandedFaq === faq.id ? null : faq.id,
                              )
                            }
                            className="w-full flex items-center justify-between px-5 py-4 text-left hover:bg-surface-50 transition-colors"
                          >
                            <span className="font-medium text-sm text-surface-900 pr-4">
                              {faq.question}
                            </span>
                            <span
                              className={cn(
                                "text-surface-800/30 transition-transform flex-shrink-0",
                                expandedFaq === faq.id && "rotate-180",
                              )}
                            >
                              ▾
                            </span>
                          </button>
                          {expandedFaq === faq.id && (
                            <div className="px-5 pb-4 border-t border-surface-100">
                              <p className="text-sm text-surface-800/70 mt-3 whitespace-pre-line leading-relaxed">
                                {faq.answer}
                              </p>
                              <div className="flex items-center gap-3 mt-4 pt-3 border-t border-surface-100">
                                <span className="text-xs text-surface-800/40">
                                  Was this helpful?
                                </span>
                                <button
                                  onClick={() => toggleHelpful(faq.id)}
                                  className={cn(
                                    "inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors",
                                    helpfulVotes[faq.id]
                                      ? "bg-brand-50 text-brand-600"
                                      : "bg-surface-50 text-surface-800/50 hover:bg-surface-100",
                                  )}
                                >
                                  👍{" "}
                                  {faq.helpful + (helpfulVotes[faq.id] ? 1 : 0)}
                                </button>
                                <button
                                  onClick={() => toggleHelpful(faq.id)}
                                  className={cn(
                                    "inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors",
                                    helpfulVotes[faq.id]
                                      ? "bg-red-50 text-red-500"
                                      : "bg-surface-50 text-surface-800/50 hover:bg-surface-100",
                                  )}
                                >
                                  👎
                                </button>
                              </div>
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  </div>
                ))
              )}
            </div>
          )}

          {/* Knowledge Base Tab */}
          {activeTab === "knowledge-base" && (
            <div>
              <h2 className="text-lg font-semibold text-surface-900 mb-4">
                Knowledge Base
              </h2>
              {filteredArticles.length === 0 ? (
                <div className="rounded-xl border border-surface-200 bg-surface-0 p-12 text-center">
                  <p className="text-3xl mb-3">📚</p>
                  <p className="text-surface-800/50 font-medium">
                    No articles found
                  </p>
                  <p className="text-sm text-surface-800/40 mt-1">
                    Try a different search term.
                  </p>
                </div>
              ) : (
                <div className="grid gap-4 sm:grid-cols-2">
                  {filteredArticles.map((article) => (
                    <div
                      key={article.id}
                      className="rounded-xl border border-surface-200 bg-surface-0 p-5 hover:border-brand-200 hover:shadow-sm transition-all cursor-pointer group"
                    >
                      {/* Thumbnail placeholder */}
                      <div className="h-32 rounded-lg bg-gradient-to-br from-brand-50 to-surface-100 mb-4 flex items-center justify-center text-3xl">
                        {CATEGORY_ICONS[article.category] ?? "📄"}
                      </div>
                      <div className="flex items-center gap-2 mb-2">
                        <span className="text-xs font-medium text-brand-600 bg-brand-50 rounded px-2 py-0.5">
                          {article.category}
                        </span>
                        <span className="text-xs text-surface-800/40">
                          {article.readTime} read
                        </span>
                      </div>
                      <h3 className="font-semibold text-sm text-surface-900 group-hover:text-brand-600 transition-colors mb-2 line-clamp-2">
                        {article.title}
                      </h3>
                      <p className="text-xs text-surface-800/50 line-clamp-2 mb-3">
                        {article.excerpt}
                      </p>
                      <div className="flex items-center justify-between text-xs text-surface-800/30">
                        <span>{article.views.toLocaleString()} views</span>
                        <span>{formatDate(article.updatedAt)}</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Submit Ticket Tab */}
          {activeTab === "submit-ticket" && (
            <div>
              <h2 className="text-lg font-semibold text-surface-900 mb-4">
                Submit a Support Ticket
              </h2>
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
                {formSuccess && (
                  <div className="mb-6 rounded-lg bg-emerald-50 border border-emerald-200 p-4 flex items-center gap-3">
                    <span className="text-emerald-600 text-lg">✓</span>
                    <div>
                      <p className="text-sm font-medium text-emerald-800">
                        Ticket submitted successfully!
                      </p>
                      <p className="text-xs text-emerald-600/70">
                        We&apos;ll get back to you within a few hours.
                      </p>
                    </div>
                  </div>
                )}

                <div className="space-y-5">
                  {/* Subject */}
                  <div>
                    <label className="block text-sm font-medium text-surface-900 mb-1.5">
                      Subject <span className="text-red-500">*</span>
                    </label>
                    <input
                      type="text"
                      value={formSubject}
                      onChange={(e) => setFormSubject(e.target.value)}
                      placeholder="Brief description of your issue"
                      className="w-full rounded-lg border border-surface-200 bg-surface-0 px-4 py-2.5 text-sm text-surface-900 placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20"
                    />
                  </div>

                  {/* Category + Priority */}
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <label className="block text-sm font-medium text-surface-900 mb-1.5">
                        Category <span className="text-red-500">*</span>
                      </label>
                      <select
                        value={formCategory}
                        onChange={(e) => setFormCategory(e.target.value)}
                        className="w-full rounded-lg border border-surface-200 bg-surface-0 px-4 py-2.5 text-sm text-surface-900 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20"
                      >
                        <option>Technical</option>
                        <option>Billing</option>
                        <option>Account</option>
                        <option>Models</option>
                        <option>API</option>
                        <option>Security</option>
                        <option>Provider</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-sm font-medium text-surface-900 mb-1.5">
                        Priority <span className="text-red-500">*</span>
                      </label>
                      <select
                        value={formPriority}
                        onChange={(e) =>
                          setFormPriority(
                            e.target.value as "low" | "medium" | "high" | "urgent",
                          )
                        }
                        className="w-full rounded-lg border border-surface-200 bg-surface-0 px-4 py-2.5 text-sm text-surface-900 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20"
                      >
                        <option value="low">Low</option>
                        <option value="medium">Medium</option>
                        <option value="high">High</option>
                        <option value="urgent">Urgent</option>
                      </select>
                    </div>
                  </div>

                  {/* Description */}
                  <div>
                    <label className="block text-sm font-medium text-surface-900 mb-1.5">
                      Description <span className="text-red-500">*</span>
                    </label>
                    <textarea
                      value={formDescription}
                      onChange={(e) => setFormDescription(e.target.value)}
                      placeholder="Describe your issue in detail. Include any error messages, steps to reproduce, and relevant context."
                      rows={6}
                      className="w-full rounded-lg border border-surface-200 bg-surface-0 px-4 py-2.5 text-sm text-surface-900 placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20 resize-y"
                    />
                  </div>

                  {/* File attachment placeholder */}
                  <div>
                    <label className="block text-sm font-medium text-surface-900 mb-1.5">
                      Attachments
                    </label>
                    <div className="rounded-lg border-2 border-dashed border-surface-200 p-6 text-center hover:border-surface-300 transition-colors cursor-pointer">
                      <span className="text-2xl mb-2 block">📎</span>
                      <p className="text-sm text-surface-800/50">
                        Drag and drop files here, or click to browse
                      </p>
                      <p className="text-xs text-surface-800/30 mt-1">
                        PNG, JPG, PDF up to 10MB (attachments coming soon)
                      </p>
                    </div>
                  </div>

                  {/* Submit */}
                  <div className="flex items-center gap-3 pt-2">
                    <button
                      onClick={handleSubmitTicket}
                      disabled={
                        formSubmitting ||
                        !formSubject.trim() ||
                        !formDescription.trim()
                      }
                      className={cn(
                        "px-6 py-2.5 rounded-lg text-sm font-medium transition-colors",
                        formSubmitting ||
                          !formSubject.trim() ||
                          !formDescription.trim()
                          ? "bg-surface-200 text-surface-800/40 cursor-not-allowed"
                          : "bg-brand-500 text-white hover:bg-brand-600",
                      )}
                    >
                      {formSubmitting ? "Submitting..." : "Submit Ticket"}
                    </button>
                    <button
                      onClick={() => {
                        setFormSubject("");
                        setFormDescription("");
                        setFormCategory("Technical");
                        setFormPriority("medium");
                      }}
                      className="px-4 py-2.5 rounded-lg text-sm text-surface-800/50 hover:text-surface-900 hover:bg-surface-50 transition-colors"
                    >
                      Reset
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* My Tickets Tab */}
          {activeTab === "my-tickets" && !selectedTicket && (
            <div>
              <h2 className="text-lg font-semibold text-surface-900 mb-4">
                My Tickets
              </h2>
              {tickets.length === 0 ? (
                <div className="rounded-xl border border-surface-200 bg-surface-0 p-12 text-center">
                  <p className="text-3xl mb-3">🎫</p>
                  <p className="text-surface-800/50 font-medium">
                    No tickets yet
                  </p>
                  <p className="text-sm text-surface-800/40 mt-1">
                    Submit a ticket to get help from our team.
                  </p>
                  <button
                    onClick={() => setActiveTab("submit-ticket")}
                    className="mt-4 px-4 py-2 rounded-lg text-sm font-medium bg-brand-500 text-white hover:bg-brand-600 transition-colors"
                  >
                    Submit Ticket
                  </button>
                </div>
              ) : (
                <div className="space-y-3">
                  {tickets.map((ticket) => (
                    <button
                      key={ticket.id}
                      onClick={() => setSelectedTicket(ticket)}
                      className="w-full rounded-xl border border-surface-200 bg-surface-0 p-4 text-left hover:border-brand-200 hover:shadow-sm transition-all"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2 mb-1">
                            <span className="text-xs font-mono text-surface-800/30">
                              {ticket.id}
                            </span>
                            <span
                              className={cn(
                                "px-2 py-0.5 rounded-md text-xs font-medium",
                                STATUS_STYLES[ticket.status],
                              )}
                            >
                              {STATUS_LABELS[ticket.status]}
                            </span>
                            <span
                              className={cn(
                                "px-2 py-0.5 rounded-md text-xs font-medium",
                                PRIORITY_STYLES[ticket.priority],
                              )}
                            >
                              {ticket.priority.charAt(0).toUpperCase() +
                                ticket.priority.slice(1)}
                            </span>
                          </div>
                          <h3 className="font-medium text-sm text-surface-900 truncate">
                            {ticket.subject}
                          </h3>
                          <div className="flex items-center gap-3 mt-1.5 text-xs text-surface-800/40">
                            <span>{ticket.category}</span>
                            <span>·</span>
                            <span>
                              Updated {formatRelativeTime(ticket.updatedAt)}
                            </span>
                            <span>·</span>
                            <span>
                              {ticket.messages.length} message
                              {ticket.messages.length !== 1 ? "s" : ""}
                            </span>
                          </div>
                        </div>
                        <span className="text-surface-800/20 text-lg">
                          ›
                        </span>
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Ticket Detail */}
          {activeTab === "my-tickets" && selectedTicket && (
            <div>
              <button
                onClick={() => setSelectedTicket(null)}
                className="inline-flex items-center gap-1 text-sm text-surface-800/50 hover:text-surface-900 mb-4 transition-colors"
              >
                ← Back to tickets
              </button>
              <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
                {/* Ticket header */}
                <div className="p-5 border-b border-surface-100">
                  <div className="flex items-center gap-2 mb-2">
                    <span className="text-xs font-mono text-surface-800/30">
                      {selectedTicket.id}
                    </span>
                    <span
                      className={cn(
                        "px-2 py-0.5 rounded-md text-xs font-medium",
                        STATUS_STYLES[selectedTicket.status],
                      )}
                    >
                      {STATUS_LABELS[selectedTicket.status]}
                    </span>
                    <span
                      className={cn(
                        "px-2 py-0.5 rounded-md text-xs font-medium",
                        PRIORITY_STYLES[selectedTicket.priority],
                      )}
                    >
                      {selectedTicket.priority.charAt(0).toUpperCase() +
                        selectedTicket.priority.slice(1)}
                    </span>
                  </div>
                  <h2 className="text-lg font-semibold text-surface-900">
                    {selectedTicket.subject}
                  </h2>
                  <div className="flex items-center gap-3 mt-2 text-xs text-surface-800/40">
                    <span>{selectedTicket.category}</span>
                    <span>·</span>
                    <span>Created {formatDate(selectedTicket.createdAt)}</span>
                    <span>·</span>
                    <span>
                      Updated {formatRelativeTime(selectedTicket.updatedAt)}
                    </span>
                  </div>

                  {/* Status timeline */}
                  <div className="flex items-center gap-2 mt-4">
                    <div className="flex items-center gap-1.5">
                      <div className="w-2 h-2 rounded-full bg-blue-500" />
                      <span className="text-xs text-surface-800/50">Open</span>
                    </div>
                    {selectedTicket.status !== "open" && (
                      <>
                        <div className="flex-1 h-px bg-surface-200" />
                        <div className="flex items-center gap-1.5">
                          <div className="w-2 h-2 rounded-full bg-amber-500" />
                          <span className="text-xs text-surface-800/50">
                            In Progress
                          </span>
                        </div>
                      </>
                    )}
                    {(selectedTicket.status === "resolved" ||
                      selectedTicket.status === "closed") && (
                      <>
                        <div className="flex-1 h-px bg-surface-200" />
                        <div className="flex items-center gap-1.5">
                          <div className="w-2 h-2 rounded-full bg-emerald-500" />
                          <span className="text-xs text-surface-800/50">
                            {selectedTicket.status === "closed"
                              ? "Closed"
                              : "Resolved"}
                          </span>
                        </div>
                      </>
                    )}
                  </div>
                </div>

                {/* Messages */}
                <div className="divide-y divide-surface-100 max-h-[400px] overflow-y-auto">
                  {selectedTicket.messages.map((msg) => (
                    <div
                      key={msg.id}
                      className={cn(
                        "px-5 py-4",
                        msg.sender === "support" && "bg-surface-50/50",
                      )}
                    >
                      <div className="flex items-center gap-2 mb-2">
                        <span
                          className={cn(
                            "text-xs font-medium",
                            msg.sender === "support"
                              ? "text-brand-600"
                              : "text-surface-800/70",
                          )}
                        >
                          {msg.sender === "support" ? "Support Team" : "You"}
                        </span>
                        <span className="text-xs text-surface-800/30">
                          {formatRelativeTime(msg.createdAt)}
                        </span>
                      </div>
                      <p className="text-sm text-surface-800/70 whitespace-pre-line leading-relaxed">
                        {msg.content}
                      </p>
                    </div>
                  ))}
                </div>

                {/* Reply box + actions */}
                {selectedTicket.status !== "closed" && (
                  <div className="p-5 border-t border-surface-100">
                    <textarea
                      value={replyText}
                      onChange={(e) => setReplyText(e.target.value)}
                      placeholder="Type your reply..."
                      rows={3}
                      className="w-full rounded-lg border border-surface-200 bg-surface-0 px-4 py-2.5 text-sm text-surface-900 placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20 resize-y mb-3"
                    />
                    <div className="flex items-center gap-2">
                      <button
                        onClick={handleAddReply}
                        disabled={!replyText.trim()}
                        className={cn(
                          "px-4 py-2 rounded-lg text-sm font-medium transition-colors",
                          replyText.trim()
                            ? "bg-brand-500 text-white hover:bg-brand-600"
                            : "bg-surface-200 text-surface-800/40 cursor-not-allowed",
                        )}
                      >
                        Send Reply
                      </button>
                      {selectedTicket.status === "resolved" && (
                        <button
                          onClick={() =>
                            setSelectedTicket((prev) =>
                              prev
                                ? {
                                    ...prev,
                                    status: "open",
                                    updatedAt: new Date().toISOString(),
                                  }
                                : null,
                            )
                          }
                          className="px-4 py-2 rounded-lg text-sm text-amber-600 hover:bg-amber-50 transition-colors"
                        >
                          Reopen Ticket
                        </button>
                      )}
                      {(selectedTicket.status === "open" ||
                        selectedTicket.status === "in_progress") && (
                        <button
                          onClick={() =>
                            setSelectedTicket((prev) =>
                              prev
                                ? {
                                    ...prev,
                                    status: "closed",
                                    updatedAt: new Date().toISOString(),
                                  }
                                : null,
                            )
                          }
                          className="px-4 py-2 rounded-lg text-sm text-surface-800/50 hover:bg-surface-50 transition-colors"
                        >
                          Close Ticket
                        </button>
                      )}
                    </div>
                  </div>
                )}

                {selectedTicket.status === "closed" && (
                  <div className="p-5 border-t border-surface-100 text-center">
                    <p className="text-sm text-surface-800/40 mb-3">
                      This ticket is closed.
                    </p>
                    <button
                      onClick={() =>
                        setSelectedTicket((prev) =>
                          prev
                            ? {
                                ...prev,
                                status: "open",
                                updatedAt: new Date().toISOString(),
                              }
                            : null,
                        )
                      }
                      className="px-4 py-2 rounded-lg text-sm font-medium text-amber-600 hover:bg-amber-50 transition-colors"
                    >
                      Reopen Ticket
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Stats Sidebar */}
        <div className="lg:col-span-1">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 sticky top-6">
            <h3 className="text-sm font-semibold text-surface-900 mb-4">
              Support Stats
            </h3>
            {stats && (
              <div className="space-y-4">
                <div className="text-center p-4 rounded-lg bg-brand-50/50">
                  <div className="text-2xl font-bold text-brand-600">
                    {stats.satisfactionScore}
                  </div>
                  <div className="flex items-center justify-center gap-0.5 mt-1">
                    {[1, 2, 3, 4, 5].map((star) => (
                      <span
                        key={star}
                        className={cn(
                          "text-sm",
                          star <= Math.round(stats.satisfactionScore)
                            ? "text-amber-400"
                            : "text-surface-200",
                        )}
                      >
                        ★
                      </span>
                    ))}
                  </div>
                  <p className="text-xs text-surface-800/40 mt-1">
                    Satisfaction Score
                  </p>
                </div>

                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-surface-800/50">
                      Avg Response
                    </span>
                    <span className="text-sm font-medium text-surface-900">
                      {stats.avgResponseTime}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-surface-800/50">
                      Resolution Rate
                    </span>
                    <span className="text-sm font-medium text-emerald-600">
                      {stats.resolvedPercent}%
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-surface-800/50">
                      Open Tickets
                    </span>
                    <span className="text-sm font-medium text-surface-900">
                      {stats.openTickets}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-surface-800/50">
                      Total Resolved
                    </span>
                    <span className="text-sm font-medium text-surface-900">
                      {stats.totalResolved.toLocaleString()}
                    </span>
                  </div>
                </div>

                {/* Resolution bar */}
                <div>
                  <div className="flex items-center justify-between text-xs mb-1">
                    <span className="text-surface-800/40">Resolution</span>
                    <span className="text-surface-800/60 font-medium">
                      {stats.resolvedPercent}%
                    </span>
                  </div>
                  <div className="h-2 bg-surface-100 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-emerald-500 rounded-full transition-all"
                      style={{ width: `${stats.resolvedPercent}%` }}
                    />
                  </div>
                </div>

                <div className="border-t border-surface-100 pt-4">
                  <p className="text-xs text-surface-800/40 text-center">
                    Need urgent help? Visit our{" "}
                    <a
                      href="/community"
                      className="text-brand-600 hover:underline"
                    >
                      community forum
                    </a>
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
