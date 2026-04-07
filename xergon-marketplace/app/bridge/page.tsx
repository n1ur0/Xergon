"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ChainStatus = "healthy" | "syncing" | "down";
type TxStatus = "pending" | "confirmed" | "failed";

interface ChainInfo {
  id: string;
  name: string;
  symbol: string;
  status: ChainStatus;
  tvl: string;
  logo: string;
}

interface BridgeTx {
  id: string;
  hash: string;
  amount: string;
  token: string;
  from: string;
  to: string;
  status: TxStatus;
  date: string;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const CHAINS: ChainInfo[] = [
  { id: "ergo", name: "Ergo", symbol: "ERG", status: "healthy", tvl: "$12.4M", logo: "ERG" },
  { id: "ethereum", name: "Ethereum", symbol: "ETH", status: "healthy", tvl: "$8.2M", logo: "ETH" },
  { id: "polygon", name: "Polygon", symbol: "POL", status: "syncing", tvl: "$3.1M", logo: "POL" },
  { id: "solana", name: "Solana", symbol: "SOL", status: "healthy", tvl: "$5.7M", logo: "SOL" },
];

const MOCK_TXS: BridgeTx[] = [
  { id: "b1", hash: "0x7f3a...b2c1", amount: "1,500", token: "ERG", from: "Ergo", to: "Ethereum", status: "confirmed", date: "2025-11-20 14:32" },
  { id: "b2", hash: "0x2d8e...a4f7", amount: "0.5", token: "XGT", from: "Ethereum", to: "Ergo", status: "confirmed", date: "2025-11-20 13:15" },
  { id: "b3", hash: "0x9c1b...e8d3", amount: "3,200", token: "ERG", from: "Ergo", to: "Polygon", status: "pending", date: "2025-11-20 12:48" },
  { id: "b4", hash: "0x4e6f...c2a9", amount: "12.5", token: "XGT", from: "Solana", to: "Ergo", status: "confirmed", date: "2025-11-20 11:22" },
  { id: "b5", hash: "0x1a5d...f7b8", amount: "800", token: "ERG", from: "Ergo", to: "Solana", status: "failed", date: "2025-11-20 10:05" },
  { id: "b6", hash: "0x8b2c...d1e4", amount: "5,000", token: "ERG", from: "Ergo", to: "Ethereum", status: "confirmed", date: "2025-11-20 09:30" },
  { id: "b7", hash: "0x3f9a...a5c2", amount: "0.8", token: "XGT", from: "Ethereum", to: "Polygon", status: "pending", date: "2025-11-20 08:55" },
  { id: "b8", hash: "0x6d4e...b8f1", amount: "2,100", token: "ERG", from: "Polygon", to: "Ergo", status: "confirmed", date: "2025-11-19 22:10" },
];

const TOKENS = [
  { symbol: "ERG", name: "Ergo", balance: "12,456.78" },
  { symbol: "XGT", name: "Xergon Token", balance: "89,234.12" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function statusDot(status: ChainStatus): string {
  switch (status) {
    case "healthy": return "bg-green-500";
    case "syncing": return "bg-yellow-500";
    case "down": return "bg-red-500";
  }
}

function statusLabel(status: ChainStatus): string {
  switch (status) {
    case "healthy": return "Healthy";
    case "syncing": return "Syncing";
    case "down": return "Down";
  }
}

function txStatusColor(status: TxStatus): string {
  switch (status) {
    case "pending": return "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-300";
    case "confirmed": return "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300";
    case "failed": return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300";
  }
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function BridgeCardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 animate-pulse">
      <div className="flex items-center gap-3">
        <div className="h-10 w-10 rounded-full bg-surface-200" />
        <div className="flex-1">
          <div className="h-4 w-24 rounded bg-surface-200" />
          <div className="h-3 w-16 rounded bg-surface-200 mt-1" />
        </div>
      </div>
    </div>
  );
}

function BridgeFormSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-4 animate-pulse">
      <div className="h-5 w-32 rounded bg-surface-200" />
      <div className="h-10 w-full rounded-lg bg-surface-200" />
      <div className="h-10 w-full rounded-lg bg-surface-200" />
      <div className="h-10 w-full rounded-lg bg-surface-200" />
      <div className="h-10 w-full rounded-lg bg-surface-200" />
      <div className="h-10 w-full rounded-lg bg-surface-200" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Chain status indicator
// ---------------------------------------------------------------------------

function ChainStatusDot({ status }: { status: ChainStatus }) {
  return (
    <span className="relative flex h-3 w-3">
      {status === "syncing" && (
        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-yellow-400 opacity-75" />
      )}
      <span className={`relative inline-flex rounded-full h-3 w-3 ${statusDot(status)}`} />
    </span>
  );
}

// ---------------------------------------------------------------------------
// Inner component
// ---------------------------------------------------------------------------

function BridgeContent() {
  const [loading, setLoading] = useState(true);
  const [chains, setChains] = useState<ChainInfo[]>([]);
  const [txs, setTxs] = useState<BridgeTx[]>([]);
  const [sourceChain, setSourceChain] = useState("ergo");
  const [targetChain, setTargetChain] = useState("ethereum");
  const [amount, setAmount] = useState("");
  const [selectedToken, setSelectedToken] = useState("ERG");
  const [dismissAlert, setDismissAlert] = useState(false);

  // Simulate fetch
  useEffect(() => {
    const timer = setTimeout(() => {
      setChains(CHAINS);
      setTxs(MOCK_TXS);
      setLoading(false);
    }, 700);
    return () => clearTimeout(timer);
  }, []);

  // Compute fees
  const fees = useMemo(() => {
    const amt = parseFloat(amount) || 0;
    const bridgeFee = amt > 0 ? (amt * 0.001).toFixed(2) : "0.00";
    const gasCost = sourceChain === "solana" ? "0.00025 SOL" : sourceChain === "ergo" ? "0.002 ERG" : "0.003 ETH";
    const estimatedTime = sourceChain === "solana" || targetChain === "solana" ? "~1 min" : "~5 min";
    return { bridgeFee, gasCost, estimatedTime };
  }, [amount, sourceChain, targetChain]);

  // Total TVL
  const totalTvl = useMemo(() => {
    return chains.reduce((sum, c) => {
      const val = parseFloat(c.tvl.replace(/[$,M]/g, ""));
      return sum + val;
    }, 0);
  }, [chains]);

  // Bridge health
  const bridgeHealth = useMemo(() => {
    const downCount = chains.filter((c) => c.status === "down").length;
    const syncCount = chains.filter((c) => c.status === "syncing").length;
    if (downCount > 0) return { label: "Degraded", color: "text-red-600" };
    if (syncCount > 0) return { label: "Partial", color: "text-yellow-600" };
    return { label: "Operational", color: "text-green-600" };
  }, [chains]);

  // Swap chains
  const swapChains = useCallback(() => {
    setSourceChain(targetChain);
    setTargetChain(sourceChain);
  }, [sourceChain, targetChain]);

  const selectedTokenInfo = TOKENS.find((t) => t.symbol === selectedToken);

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Page header */}
      <div className="space-y-1">
        <h1 className="text-2xl font-bold text-surface-900">Cross-Chain Bridge</h1>
        <p className="text-sm text-surface-800/60">
          Transfer assets seamlessly across Ergo, Ethereum, Polygon, and Solana
        </p>
      </div>

      {/* Alert banner */}
      {!dismissAlert && (
        <div className="flex items-center justify-between rounded-xl border border-brand-200 bg-brand-50 px-4 py-3 dark:bg-brand-950/20">
          <div className="flex items-center gap-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-600 shrink-0"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
            <p className="text-sm text-brand-800 dark:text-brand-200">
              Polygon bridge is currently syncing. Transfers may take longer than usual.
            </p>
          </div>
          <button
            type="button"
            onClick={() => setDismissAlert(true)}
            className="shrink-0 text-brand-600 hover:text-brand-700 transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </div>
      )}

      {/* Bridge status cards */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold text-surface-900">Connected Chains</h2>
          <div className="flex items-center gap-2">
            <span className="text-xs text-surface-800/40">Bridge Status:</span>
            <span className={`text-xs font-semibold ${bridgeHealth.color}`}>{bridgeHealth.label}</span>
          </div>
        </div>

        {loading ? (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
            {Array.from({ length: 4 }, (_, i) => (
              <BridgeCardSkeleton key={i} />
            ))}
          </div>
        ) : (
          <>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
              {chains.map((chain) => (
                <div
                  key={chain.id}
                  className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-2"
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <div className="flex h-8 w-8 items-center justify-center rounded-full bg-surface-100 text-xs font-bold text-surface-800 dark:bg-surface-200">
                        {chain.logo}
                      </div>
                      <div>
                        <p className="text-sm font-semibold text-surface-900">{chain.name}</p>
                        <p className="text-xs text-surface-800/40">{chain.symbol}</p>
                      </div>
                    </div>
                    <ChainStatusDot status={chain.status} />
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-surface-800/40">TVL</span>
                    <span className="font-medium text-surface-900">{chain.tvl}</span>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-surface-800/40">Status</span>
                    <span className="text-surface-800/70">{statusLabel(chain.status)}</span>
                  </div>
                </div>
              ))}
            </div>

            {/* Summary row */}
            <div className="flex flex-wrap gap-4 text-xs text-surface-800/50">
              <span>Total TVL: <strong className="text-surface-900">${totalTvl.toFixed(1)}M</strong></span>
              <span>Active chains: <strong className="text-surface-900">{chains.filter((c) => c.status !== "down").length}/{chains.length}</strong></span>
              <span>24h volume: <strong className="text-surface-900">$1.2M</strong></span>
            </div>
          </>
        )}
      </section>

      {/* Network diagram */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h3 className="text-sm font-semibold text-surface-900 mb-4">Network Topology</h3>
        <div className="flex items-center justify-center gap-3 flex-wrap">
          {/* Ergo */}
          <div className="flex flex-col items-center gap-1">
            <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-brand-100 text-sm font-bold text-brand-700 dark:bg-brand-900/30 dark:text-brand-300">
              ERG
            </div>
            <span className="text-xs font-medium text-surface-800">Ergo</span>
            <span className="text-xs text-surface-800/40">Source</span>
          </div>

          {/* Connector lines */}
          <div className="flex flex-col items-center gap-1 px-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="32" height="8" viewBox="0 0 32 8" className="text-surface-300">
              <line x1="0" y1="4" x2="28" y2="4" stroke="currentColor" strokeWidth="2" strokeDasharray="4 2" />
              <polygon points="28,0 32,4 28,8" fill="currentColor" />
            </svg>
          </div>

          {/* Bridge */}
          <div className="flex flex-col items-center gap-1">
            <div className="flex h-14 w-14 items-center justify-center rounded-xl bg-accent-500/10 border border-accent-500/30">
              <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-accent-500">
                <path d="M4 12h16" /><path d="M12 4l-8 8 8 8" /><path d="M20 12l-8-8 8-8" transform="rotate(0 16 12)" />
                <path d="M4 12l8-8" /><path d="M4 12l8 8" />
              </svg>
            </div>
            <span className="text-xs font-semibold text-accent-600">Bridge</span>
            <span className="text-xs text-surface-800/40">Xergon Relay</span>
          </div>

          {/* Connector lines */}
          <div className="flex flex-col items-center gap-1 px-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="32" height="8" viewBox="0 0 32 8" className="text-surface-300">
              <line x1="0" y1="4" x2="28" y2="4" stroke="currentColor" strokeWidth="2" strokeDasharray="4 2" />
              <polygon points="28,0 32,4 28,8" fill="currentColor" />
            </svg>
          </div>

          {/* Target chain (dynamic) */}
          <div className="flex flex-col items-center gap-1">
            <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-surface-100 text-sm font-bold text-surface-800 dark:bg-surface-200">
              {chains.find((c) => c.id === targetChain)?.logo ?? "???"}
            </div>
            <span className="text-xs font-medium text-surface-800">{chains.find((c) => c.id === targetChain)?.name ?? "Target"}</span>
            <span className="text-xs text-surface-800/40">Destination</span>
          </div>
        </div>
      </section>

      {/* Bridge form + Security info */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Bridge form */}
        <div className="lg:col-span-2">
          {loading ? (
            <BridgeFormSkeleton />
          ) : (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-4">
              <h2 className="text-lg font-semibold text-surface-900">Bridge Assets</h2>

              {/* Source chain */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-surface-800/70">From</label>
                <select
                  value={sourceChain}
                  onChange={(e) => setSourceChain(e.target.value)}
                  className="field-input"
                >
                  {chains.map((c) => (
                    <option key={c.id} value={c.id}>{c.name} ({c.symbol})</option>
                  ))}
                </select>
              </div>

              {/* Swap button */}
              <div className="flex justify-center">
                <button
                  type="button"
                  onClick={swapChains}
                  className="flex h-8 w-8 items-center justify-center rounded-full border border-surface-200 bg-surface-50 hover:bg-surface-100 transition-colors"
                  title="Swap chains"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-surface-800"><polyline points="17 1 21 5 17 9"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><polyline points="7 23 3 19 7 15"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/></svg>
                </button>
              </div>

              {/* Target chain */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-surface-800/70">To</label>
                <select
                  value={targetChain}
                  onChange={(e) => setTargetChain(e.target.value)}
                  className="field-input"
                >
                  {chains.filter((c) => c.id !== sourceChain).map((c) => (
                    <option key={c.id} value={c.id}>{c.name} ({c.symbol})</option>
                  ))}
                </select>
              </div>

              {/* Token selector */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-surface-800/70">Token</label>
                <div className="flex gap-2">
                  {TOKENS.map((t) => (
                    <button
                      key={t.symbol}
                      type="button"
                      onClick={() => setSelectedToken(t.symbol)}
                      className={`flex-1 rounded-lg border px-3 py-2 text-sm font-medium transition-colors ${
                        selectedToken === t.symbol
                          ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-950/20"
                          : "border-surface-200 bg-surface-0 text-surface-800 hover:bg-surface-50"
                      }`}
                    >
                      <span>{t.symbol}</span>
                      <span className="block text-xs text-surface-800/40 mt-0.5">Bal: {t.balance}</span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Amount */}
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-surface-800/70">Amount</label>
                <div className="relative">
                  <input
                    type="number"
                    placeholder="0.00"
                    value={amount}
                    onChange={(e) => setAmount(e.target.value)}
                    className="field-input pr-20"
                    min="0"
                    step="any"
                  />
                  <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-surface-800/40 font-mono">
                    {selectedToken}
                  </span>
                </div>
                {selectedTokenInfo && (
                  <p className="text-xs text-surface-800/40">
                    Available: {selectedTokenInfo.balance} {selectedTokenInfo.symbol}
                  </p>
                )}
              </div>

              {/* Estimated fees */}
              <div className="rounded-lg bg-surface-50 p-3 space-y-2 dark:bg-surface-100/50">
                <h4 className="text-xs font-semibold text-surface-900">Estimated Fees</h4>
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div>
                    <span className="text-surface-800/40">Bridge Fee</span>
                    <p className="font-medium text-surface-900">{fees.bridgeFee} {selectedToken}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/40">Gas Cost</span>
                    <p className="font-medium text-surface-900">{fees.gasCost}</p>
                  </div>
                  <div>
                    <span className="text-surface-800/40">Est. Time</span>
                    <p className="font-medium text-surface-900">{fees.estimatedTime}</p>
                  </div>
                </div>
              </div>

              {/* Submit */}
              <button
                type="button"
                disabled={!amount || parseFloat(amount) <= 0}
                className="w-full rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                Bridge {selectedToken}
              </button>
            </div>
          )}
        </div>

        {/* Security info */}
        <div className="space-y-4">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3">
            <div className="flex items-center gap-2">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-accent-500"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
              <h3 className="text-sm font-semibold text-surface-900">Security Info</h3>
            </div>
            <div className="space-y-2 text-xs">
              <div className="flex justify-between">
                <span className="text-surface-800/50">Lock Period</span>
                <span className="font-medium text-surface-900">~15 min</span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Min Amount</span>
                <span className="font-medium text-surface-900">10 {selectedToken}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Max Amount</span>
                <span className="font-medium text-surface-900">100,000 {selectedToken}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Fee</span>
                <span className="font-medium text-surface-900">0.1%</span>
              </div>
              <div className="pt-2 border-t border-surface-200">
                <p className="text-surface-800/50 mb-1">Supported Tokens</p>
                <div className="flex gap-1 flex-wrap">
                  {TOKENS.map((t) => (
                    <span key={t.symbol} className="rounded-md bg-surface-100 px-2 py-0.5 font-medium text-surface-800 dark:bg-surface-200">
                      {t.symbol}
                    </span>
                  ))}
                </div>
              </div>
              <div className="pt-2 border-t border-surface-200">
                <p className="text-surface-800/50 mb-1">Supported Chains</p>
                <div className="flex gap-1 flex-wrap">
                  {chains.map((c) => (
                    <span key={c.id} className="rounded-md bg-surface-100 px-2 py-0.5 font-medium text-surface-800 dark:bg-surface-200">
                      {c.name}
                    </span>
                  ))}
                </div>
              </div>
            </div>
          </div>

          {/* Quick info */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-2">
            <h3 className="text-sm font-semibold text-surface-900">How It Works</h3>
            <ol className="space-y-2 text-xs text-surface-800/60">
              <li className="flex items-start gap-2">
                <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-brand-100 text-xs font-bold text-brand-700 dark:bg-brand-900/30 dark:text-brand-300">1</span>
                <span>Lock assets on the source chain via smart contract</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-brand-100 text-xs font-bold text-brand-700 dark:bg-brand-900/30 dark:text-brand-300">2</span>
                <span>Xergon relay validators verify and attest the lock</span>
              </li>
              <li className="flex items-start gap-2">
                <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-brand-100 text-xs font-bold text-brand-700 dark:bg-brand-900/30 dark:text-brand-300">3</span>
                <span>Mint equivalent assets on the target chain</span>
              </li>
            </ol>
          </div>
        </div>
      </div>

      {/* Transaction history */}
      <section className="space-y-3">
        <h2 className="text-lg font-semibold text-surface-900">Transaction History</h2>
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-surface-200 bg-surface-50 dark:bg-surface-100/50">
                  <th className="px-4 py-3 text-left font-medium text-surface-800/50">Hash</th>
                  <th className="px-4 py-3 text-left font-medium text-surface-800/50">Amount</th>
                  <th className="px-4 py-3 text-left font-medium text-surface-800/50">From</th>
                  <th className="px-4 py-3 text-left font-medium text-surface-800/50">To</th>
                  <th className="px-4 py-3 text-left font-medium text-surface-800/50">Status</th>
                  <th className="px-4 py-3 text-left font-medium text-surface-800/50">Date</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-surface-200">
                {loading ? (
                  Array.from({ length: 5 }, (_, i) => (
                    <tr key={i} className="animate-pulse">
                      <td className="px-4 py-3"><div className="h-3 w-24 rounded bg-surface-200" /></td>
                      <td className="px-4 py-3"><div className="h-3 w-16 rounded bg-surface-200" /></td>
                      <td className="px-4 py-3"><div className="h-3 w-16 rounded bg-surface-200" /></td>
                      <td className="px-4 py-3"><div className="h-3 w-16 rounded bg-surface-200" /></td>
                      <td className="px-4 py-3"><div className="h-3 w-14 rounded-full bg-surface-200" /></td>
                      <td className="px-4 py-3"><div className="h-3 w-20 rounded bg-surface-200" /></td>
                    </tr>
                  ))
                ) : (
                  txs.map((tx) => (
                    <tr key={tx.id} className="hover:bg-surface-50 dark:hover:bg-surface-100/30 transition-colors">
                      <td className="px-4 py-3 font-mono text-surface-800">{tx.hash}</td>
                      <td className="px-4 py-3">
                        <span className="font-medium text-surface-900">{tx.amount}</span>
                        <span className="text-surface-800/40 ml-1">{tx.token}</span>
                      </td>
                      <td className="px-4 py-3 text-surface-800">{tx.from}</td>
                      <td className="px-4 py-3 text-surface-800">{tx.to}</td>
                      <td className="px-4 py-3">
                        <span className={`inline-flex items-center rounded-full px-2 py-0.5 font-medium capitalize ${txStatusColor(tx.status)}`}>
                          {tx.status}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-surface-800/50 whitespace-nowrap">{tx.date}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </section>
    </main>
  );
}

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

export default function BridgePage() {
  return (
    <Suspense
      fallback={
        <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
          <div className="animate-pulse space-y-2">
            <div className="h-7 w-44 rounded bg-surface-200" />
            <div className="h-4 w-72 rounded bg-surface-200" />
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
            {Array.from({ length: 4 }, (_, i) => (
              <BridgeCardSkeleton key={i} />
            ))}
          </div>
          <BridgeFormSkeleton />
        </main>
      }
    >
      <BridgeContent />
    </Suspense>
  );
}
