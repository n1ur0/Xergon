"use client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface DigestModelEntry {
  name: string;
  provider: string;
  price: string;
}

export interface DigestPriceChange {
  model: string;
  oldPrice: string;
  newPrice: string;
  direction: "up" | "down";
}

export interface DigestProviderUpdate {
  provider: string;
  status: "Healthy" | "Degraded" | "Down";
  uptime: string;
}

export interface DigestCommunityEntry {
  type: "review" | "forum" | "rental";
  text: string;
}

export interface DigestData {
  date: string;
  recipientName: string;
  newModels: DigestModelEntry[];
  priceChanges: DigestPriceChange[];
  providerUpdates: DigestProviderUpdate[];
  communityActivity: DigestCommunityEntry[];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function DigestEmail({ data }: { data: DigestData }) {
  const hasNewModels = data.newModels.length > 0;
  const hasPriceChanges = data.priceChanges.length > 0;
  const hasProviderUpdates = data.providerUpdates.length > 0;
  const hasCommunity = data.communityActivity.length > 0;

  return (
    <div
      className="max-w-[600px] mx-auto font-sans text-sm"
      style={{ backgroundColor: "#ffffff", borderRadius: "12px", overflow: "hidden", border: "1px solid #e5e7eb" }}
    >
      {/* Header / Branding */}
      <div
        className="px-6 py-5"
        style={{ backgroundColor: "#2563eb" }}
      >
        <h1 className="text-xl font-bold text-white">Xergon Network</h1>
        <p className="text-blue-100 text-sm mt-0.5">
          Your daily digest — {data.date}
        </p>
      </div>

      {/* Greeting */}
      <div className="px-6 py-4" style={{ borderBottom: "1px solid #f3f4f6" }}>
        <p className="text-gray-700">
          Hello {data.recipientName || "there"},
        </p>
        <p className="text-gray-500 mt-1">
          Here&apos;s what&apos;s happening on Xergon Network today.
        </p>
      </div>

      {/* Sections */}
      <div className="px-6 py-4 space-y-5">
        {/* New Models */}
        {hasNewModels && (
          <Section title="New Models" icon="🆕">
            {data.newModels.map((model, i) => (
              <div key={i} className="flex items-center justify-between py-1.5" style={{ borderBottom: "1px solid #f3f4f6" }}>
                <span className="font-medium text-gray-800">{model.name}</span>
                <span className="text-xs text-gray-500">{model.provider} · {model.price}</span>
              </div>
            ))}
          </Section>
        )}

        {/* Price Changes */}
        {hasPriceChanges && (
          <Section title="Price Changes" icon="💰">
            {data.priceChanges.map((change, i) => (
              <div key={i} className="flex items-center justify-between py-1.5" style={{ borderBottom: "1px solid #f3f4f6" }}>
                <span className="font-medium text-gray-800">{change.model}</span>
                <span className={`text-xs font-medium ${change.direction === "down" ? "text-green-600" : "text-red-600"}`}>
                  {change.oldPrice} → {change.newPrice}
                </span>
              </div>
            ))}
          </Section>
        )}

        {/* Provider Updates */}
        {hasProviderUpdates && (
          <Section title="Provider Updates" icon="🖥️">
            {data.providerUpdates.map((update, i) => (
              <div key={i} className="flex items-center justify-between py-1.5" style={{ borderBottom: "1px solid #f3f4f6" }}>
                <span className="font-medium text-gray-800">{update.provider}</span>
                <span className={`text-xs font-medium ${
                  update.status === "Healthy" ? "text-green-600" :
                  update.status === "Degraded" ? "text-amber-600" :
                  "text-red-600"
                }`}>
                  {update.status} · {update.uptime} uptime
                </span>
              </div>
            ))}
          </Section>
        )}

        {/* Community Activity */}
        {hasCommunity && (
          <Section title="Community Activity" icon="💬">
            {data.communityActivity.map((entry, i) => (
              <div key={i} className="text-gray-600 py-1.5" style={{ borderBottom: "1px solid #f3f4f6" }}>
                {entry.text}
              </div>
            ))}
          </Section>
        )}

        {/* Empty state */}
        {!hasNewModels && !hasPriceChanges && !hasProviderUpdates && !hasCommunity && (
          <p className="text-gray-400 text-center py-4">
            Nothing new to report today. Check back tomorrow!
          </p>
        )}
      </div>

      {/* CTA */}
      <div className="px-6 py-4 text-center" style={{ borderTop: "1px solid #f3f4f6" }}>
        <a
          href="https://xergon.network"
          className="inline-block px-6 py-2.5 rounded-lg text-sm font-medium text-white"
          style={{ backgroundColor: "#2563eb", textDecoration: "none" }}
        >
          Open Marketplace
        </a>
      </div>

      {/* Footer */}
      <div
        className="px-6 py-3 text-xs text-gray-400"
        style={{ borderTop: "1px solid #f3f4f6", backgroundColor: "#f9fafb" }}
      >
        <div className="flex items-center justify-between">
          <span>Xergon Network — Decentralized AI Marketplace</span>
          <div className="flex items-center gap-3">
            <a href="https://xergon.network/settings/notifications" className="text-blue-600 hover:underline" style={{ textDecoration: "none", color: "#2563eb" }}>
              Manage Preferences
            </a>
            <a href="https://xergon.network/api/email/unsubscribe" className="text-blue-600 hover:underline" style={{ textDecoration: "none", color: "#2563eb" }}>
              Unsubscribe
            </a>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Plain text fallback
// ---------------------------------------------------------------------------

export function DigestPlainText({ data }: { data: DigestData }) {
  const lines: string[] = [
    "========================================",
    "XERGON NETWORK — DAILY DIGEST",
    `Date: ${data.date}`,
    "========================================",
    "",
    `Hello ${data.recipientName || "there"},`,
    "Here's what's happening on Xergon Network today.",
    "",
  ];

  if (data.newModels.length > 0) {
    lines.push("NEW MODELS", "-----------");
    for (const m of data.newModels) {
      lines.push(`  - ${m.name} (${m.provider}) — ${m.price}`);
    }
    lines.push("");
  }

  if (data.priceChanges.length > 0) {
    lines.push("PRICE CHANGES", "-------------");
    for (const p of data.priceChanges) {
      const arrow = p.direction === "down" ? "↓" : "↑";
      lines.push(`  - ${p.model}: ${p.oldPrice} ${arrow} ${p.newPrice}`);
    }
    lines.push("");
  }

  if (data.providerUpdates.length > 0) {
    lines.push("PROVIDER UPDATES", "----------------");
    for (const u of data.providerUpdates) {
      lines.push(`  - ${u.provider}: ${u.status} (${u.uptime} uptime)`);
    }
    lines.push("");
  }

  if (data.communityActivity.length > 0) {
    lines.push("COMMUNITY ACTIVITY", "------------------");
    for (const a of data.communityActivity) {
      lines.push(`  - ${a.text}`);
    }
    lines.push("");
  }

  lines.push("========================================");
  lines.push("Open Marketplace: https://xergon.network");
  lines.push("Manage Preferences: https://xergon.network/settings/notifications");
  lines.push("Unsubscribe: https://xergon.network/api/email/unsubscribe");
  lines.push("Xergon Network — Decentralized AI Marketplace");
  lines.push("========================================");

  return <pre className="whitespace-pre-wrap text-xs text-gray-600 font-mono bg-gray-50 p-4 rounded-lg">{lines.join("\n")}</pre>;
}

// ---------------------------------------------------------------------------
// Section helper
// ---------------------------------------------------------------------------

function Section({ title, icon, children }: { title: string; icon: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="font-semibold text-gray-900 mb-2 text-sm">
        {icon} {title}
      </h3>
      <div>{children}</div>
    </div>
  );
}
