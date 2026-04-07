"use client";

import { useState } from "react";
import Link from "next/link";

interface Concept {
  id: string;
  title: string;
  icon: string;
  summary: string;
  details: string;
  links?: { label: string; href: string }[];
}

const CONCEPTS: Concept[] = [
  {
    id: "providers",
    title: "Compute Providers",
    icon: "GPU",
    summary:
      "Individuals and organizations that contribute GPU compute power to the Xergon network. Providers run inference nodes, earn ERG for completed requests, and build reputation over time.",
    details:
      "Providers register their GPU nodes on the network by staking ERG tokens. They specify supported models, GPU capabilities, pricing, and availability regions. When a user sends an inference request, the relay routes it to the best available provider based on latency, cost, quality score, and model availability. Providers earn ERG for each successfully completed request, minus the network fee. Providers can monitor their earnings, view request metrics, and manage their nodes through the Provider Dashboard.",
    links: [
      { label: "Become a Provider", href: "/become-provider" },
      { label: "Provider Dashboard", href: "/provider" },
    ],
  },
  {
    id: "ergo",
    title: "Ergo Integration",
    icon: "ERG",
    summary:
      "All payments, staking, and governance actions are settled on the Ergo blockchain. This provides transparency, immutability, and trustless verification.",
    details:
      "Xergon uses Ergo's UTXO model for all on-chain operations. Users pay for inference via ErgoPay, which generates a signed transaction that deducts from their wallet. Providers stake ERG to register nodes and demonstrate commitment. Storage rent (a native Ergo feature) is used to maintain agent boxes that hold provider state. Governance proposals and votes are also recorded on-chain. The Ergo blockchain provides the cryptographic foundation for trustless interactions between users and providers.",
    links: [
      { label: "Pricing", href: "/pricing" },
      { label: "Transactions", href: "/transactions" },
    ],
  },
  {
    id: "storage-rent",
    title: "Storage Rent",
    icon: "BOX",
    summary:
      "Ergo's native storage rent mechanism automatically maintains agent boxes on-chain. Provider state boxes are kept alive through small rent payments funded by staking rewards.",
    details:
      "On the Ergo blockchain, UTXOs (called boxes) have a storage cost. If a box is not accessed or updated within a certain period, it is consumed and its value returned to the owner. Xergon leverages this by creating agent boxes that hold provider registration data, reputation scores, and staking balances. These boxes are automatically updated (renewed) as providers receive requests and earn rewards. The storage rent system ensures stale or inactive providers are naturally pruned from the network without requiring manual intervention.",
  },
  {
    id: "p2p",
    title: "P2P Network",
    icon: "NET",
    summary:
      "Xergon operates a peer-to-peer network where providers communicate directly. Requests are routed through the relay, but inference traffic flows peer-to-peer for optimal performance.",
    details:
      "The Xergon network uses a gossip protocol for peer discovery and status broadcasting. When a new provider joins, it announces its presence to known peers, which propagate the information across the network. The relay layer handles request routing, authentication, and metering, but the actual inference traffic can flow directly between the user's client and the provider node. This hybrid architecture reduces latency and prevents the relay from becoming a bottleneck. Providers use heartbeats to signal their health, and the network automatically routes around failed nodes.",
  },
  {
    id: "reputation",
    title: "Reputation System",
    icon: "REP",
    summary:
      "Providers are scored based on response quality, latency, uptime, and successful completion rate. Higher reputation leads to more requests and better earnings.",
    details:
      "Every completed request contributes to a provider's reputation score. The scoring system considers: response latency (faster = better), error rate (fewer errors = better), uptime (consistent availability = better), and user feedback. Reputation scores are stored in provider agent boxes on-chain, making them transparent and tamper-proof. Higher-reputation providers receive priority routing for premium requests and can charge higher prices. Conversely, low-reputation providers receive fewer requests and may be automatically delisted if their score falls below a threshold. The system is designed to incentivize high-quality, reliable service.",
    links: [
      { label: "Leaderboard", href: "/leaderboard" },
    ],
  },
  {
    id: "coalescing",
    title: "Request Coalescing",
    icon: "BAT",
    summary:
      "Similar inference requests are intelligently batched together to maximize GPU utilization and reduce per-request costs.",
    details:
      "When multiple users submit similar requests (same model, similar prompts), the relay can coalesce them into a single batched request to a provider. This is particularly effective for common use cases like summarization, translation, or general Q&A. The provider processes the batch efficiently using GPU parallelism, and results are fanned back to individual users. Request coalescing reduces costs by amortizing the fixed overhead of model loading and warmup across multiple requests. Users benefit from lower prices and providers benefit from higher GPU utilization.",
  },
  {
    id: "circuit-breaker",
    title: "Circuit Breaker",
    icon: "CB",
    summary:
      "Automatic fault tolerance that detects failing providers and reroutes requests. Prevents cascade failures and ensures high availability.",
    details:
      "The circuit breaker pattern monitors provider health in real-time. If a provider's error rate exceeds a threshold (e.g., 5% over a rolling window), the circuit breaker opens and stops routing new requests to that provider. After a cooldown period, it enters a half-open state, sending a limited number of test requests. If these succeed, the circuit closes and normal traffic resumes. If failures persist, the circuit stays open. This system prevents cascade failures where one problematic provider degrades the entire network. Users experience minimal disruption as requests are automatically rerouted to healthy providers.",
  },
  {
    id: "governance",
    title: "Governance",
    icon: "DAO",
    summary:
      "Xergon is governed by its community through on-chain proposals and token-weighted voting. ERG holders can propose changes, vote on network parameters, and shape the platform's future.",
    details:
      "Any ERG holder can submit a governance proposal, which includes a title, description, and executable parameters (e.g., changing network fees, adding new models, modifying reputation thresholds). Proposals go through a discussion period followed by a voting period. Votes are cast on-chain and weighted by ERG holdings. Approved proposals are automatically executed by smart contracts. Governance covers network fees, model additions/removals, reputation algorithm parameters, and treasury allocation. The system ensures the community has direct control over the platform's evolution.",
    links: [
      { label: "Commitments", href: "/commitments" },
    ],
  },
];

export default function ConceptsPage() {
  const [openCards, setOpenCards] = useState<Set<string>>(new Set());

  const toggleCard = (id: string) => {
    setOpenCards((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  return (
    <div className="space-y-8">
      <section>
        <h1 className="text-3xl font-bold text-surface-900 mb-2">
          Key Concepts
        </h1>
        <p className="text-lg text-surface-800/60">
          Understand the core concepts that power the Xergon decentralized AI
          inference network.
        </p>
      </section>

      <div className="space-y-3">
        {CONCEPTS.map((concept) => (
          <div
            key={concept.id}
            className="rounded-xl border border-surface-200 overflow-hidden bg-surface-0"
          >
            <button
              onClick={() => toggleCard(concept.id)}
              className="w-full flex items-start gap-4 px-5 py-4 text-left hover:bg-surface-50 transition-colors"
            >
              <div className="flex-shrink-0 h-10 w-10 rounded-lg bg-brand-50 dark:bg-brand-950/30 flex items-center justify-center text-xs font-bold font-mono text-brand-600">
                {concept.icon}
              </div>
              <div className="flex-1 min-w-0">
                <h2 className="font-semibold text-surface-900">
                  {concept.title}
                </h2>
                <p className="text-sm text-surface-800/60 mt-0.5">
                  {concept.summary}
                </p>
              </div>
              <svg
                className={`w-5 h-5 text-surface-800/30 transition-transform shrink-0 mt-1 ${
                  openCards.has(concept.id) ? "rotate-180" : ""
                }`}
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </button>
            {openCards.has(concept.id) && (
              <div className="border-t border-surface-200 px-5 py-4">
                <p className="text-sm text-surface-800/70 leading-relaxed mb-4">
                  {concept.details}
                </p>
                {concept.links && concept.links.length > 0 && (
                  <div className="flex flex-wrap gap-2">
                    {concept.links.map((link) => (
                      <Link
                        key={link.href}
                        href={link.href}
                        className="inline-flex items-center gap-1 text-sm font-medium text-brand-600 hover:text-brand-700 transition-colors"
                      >
                        {link.label}
                        <span className="text-brand-400">&rarr;</span>
                      </Link>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        ))}
      </div>

      <section className="flex flex-wrap gap-3">
        <Link
          href="/docs/getting-started"
          className="px-4 py-2 rounded-lg border border-surface-200 text-sm font-medium text-surface-800 hover:border-brand-300 hover:text-brand-700 transition-colors"
        >
          Getting Started
        </Link>
        <Link
          href="/docs/api-reference"
          className="px-4 py-2 rounded-lg border border-surface-200 text-sm font-medium text-surface-800 hover:border-brand-300 hover:text-brand-700 transition-colors"
        >
          API Reference
        </Link>
      </section>
    </div>
  );
}
