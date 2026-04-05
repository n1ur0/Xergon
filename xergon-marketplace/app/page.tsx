import Link from "next/link";
import { PlaygroundSection } from "@/components/playground/PlaygroundSection";

export default function Page() {
  return (
    <>
      {/* Hero */}
      <section className="relative overflow-hidden bg-surface-950 text-white">
        <div className="absolute inset-0 bg-gradient-to-br from-brand-950/60 via-surface-950 to-surface-950" />
        <div className="relative mx-auto max-w-4xl px-6 py-28 text-center">
          <h1 className="text-4xl font-extrabold tracking-tight sm:text-6xl">
            Decentralized AI{" "}
            <span className="text-brand-400">Compute</span>
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-surface-200/80 dark:text-surface-300/80">
            Access open-source AI models powered by the Ergo blockchain. No
            lock-in, no middlemen — just transparent, pay-per-token inference on
            a trustless network.
          </p>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
            <Link
              href="/playground"
              className="rounded-lg bg-brand-600 px-6 py-2.5 text-sm font-semibold text-white shadow-lg shadow-brand-600/25 transition-colors hover:bg-brand-700"
            >
              Get Started
            </Link>
            <Link
              href="/models"
              className="rounded-lg border border-white/20 bg-white/5 px-6 py-2.5 text-sm font-semibold text-white backdrop-blur transition-colors hover:bg-white/10"
            >
              View Models
            </Link>
          </div>
        </div>
      </section>

      {/* Playground v2 */}
      <PlaygroundSection />

      {/* Features */}
      <section className="mx-auto max-w-6xl px-6 py-24">
        <h2 className="text-center text-2xl font-bold text-surface-900 sm:text-3xl">
          Why Xergon?
        </h2>
        <p className="mx-auto mt-3 max-w-xl text-center text-surface-800/60">
          Built for developers who value privacy, transparency, and fair pricing.
        </p>
        <div className="mt-14 grid gap-8 sm:grid-cols-3">
          {/* Privacy-First */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 shadow-sm transition-shadow hover:shadow-md">
            <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-brand-50 text-brand-600 dark:bg-brand-950/40 dark:text-brand-400">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={1.5}
                stroke="currentColor"
                className="h-5 w-5"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M9 12.75 11.25 15 15 9.75m-3-7.036A11.959 11.959 0 0 1 3.598 6 11.99 11.99 0 0 0 3 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285Z"
                />
              </svg>
            </div>
            <h3 className="text-lg font-semibold text-surface-900">
              Privacy-First
            </h3>
            <p className="mt-2 text-sm text-surface-800/60">
              Your prompts never touch centralized servers. Requests are routed
              through decentralized GPU nodes, keeping your data sovereign.
            </p>
          </div>

          {/* Pay-Per-Token */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 shadow-sm transition-shadow hover:shadow-md">
            <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-brand-50 text-brand-600 dark:bg-brand-950/40 dark:text-brand-400">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={1.5}
                stroke="currentColor"
                className="h-5 w-5"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M20.25 6.375c0 2.278-3.694 4.125-8.25 4.125S3.75 8.653 3.75 6.375m16.5 0c0-2.278-3.694-4.125-8.25-4.125S3.75 4.097 3.75 6.375m16.5 0v11.25c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125V6.375m16.5 0v3.75m-16.5-3.75v3.75m16.5 0v3.75C20.25 16.153 16.556 18 12 18s-8.25-1.847-8.25-4.125v-3.75m16.5 0c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125"
                />
              </svg>
            </div>
            <h3 className="text-lg font-semibold text-surface-900">
              Pay-Per-Token
            </h3>
            <p className="mt-2 text-sm text-surface-800/60">
              Only pay for what you use. Transparent token-level pricing settled
              on-chain with ERG — no hidden fees, no subscriptions.
            </p>
          </div>

          {/* Open Network */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 shadow-sm transition-shadow hover:shadow-md">
            <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-brand-50 text-brand-600 dark:bg-brand-950/40 dark:text-brand-400">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={1.5}
                stroke="currentColor"
                className="h-5 w-5"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M12 21a9.004 9.004 0 0 0 8.716-6.747M12 21a9.004 9.004 0 0 1-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 0 1 7.843 4.582M12 3a8.997 8.997 0 0 0-7.843 4.582m15.686 0A11.953 11.953 0 0 1 12 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0 1 21 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0 1 12 16.5a17.92 17.92 0 0 1-8.716-2.247m0 0A8.966 8.966 0 0 1 3 12c0-1.264.26-2.466.732-3.558"
                />
              </svg>
            </div>
            <h3 className="text-lg font-semibold text-surface-900">
              Open Network
            </h3>
            <p className="mt-2 text-sm text-surface-800/60">
              Anyone can run a GPU node and earn ERG. Open-source models, open
              protocol — a truly permissionless AI infrastructure.
            </p>
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section className="bg-surface-100">
        <div className="mx-auto max-w-6xl px-6 py-24">
          <h2 className="text-center text-2xl font-bold text-surface-900 sm:text-3xl">
            How It Works
          </h2>
          <p className="mx-auto mt-3 max-w-xl text-center text-surface-800/60">
            From wallet connection to your first inference in under two minutes.
          </p>
          <div className="mt-14 grid gap-8 sm:grid-cols-3">
            {/* Step 1 */}
            <div className="text-center">
              <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-brand-600 text-lg font-bold text-white">
                1
              </div>
              <h3 className="text-lg font-semibold text-surface-900">
                Connect Wallet
              </h3>
              <p className="mt-2 text-sm text-surface-800/60">
                Connect your Ergo wallet. Your public key is your identity -- no signups, no passwords.
              </p>
            </div>

            {/* Step 2 */}
            <div className="text-center">
              <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-brand-600 text-lg font-bold text-white">
                2
              </div>
              <h3 className="text-lg font-semibold text-surface-900">
                Pick a Model
              </h3>
              <p className="mt-2 text-sm text-surface-800/60">
                Browse open-source models: Llama, Qwen, Mistral, DeepSeek. Pick the right one for your task.
              </p>
            </div>

            {/* Step 3 */}
            <div className="text-center">
              <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-brand-600 text-lg font-bold text-white">
                3
              </div>
              <h3 className="text-lg font-semibold text-surface-900">
                Start Inferencing
              </h3>
              <p className="mt-2 text-sm text-surface-800/60">
                Send prompts via the playground or OpenAI-compatible API. Pay per token with ERG, settled on-chain.
              </p>
            </div>
          </div>
        </div>
      </section>

      {/* Become a Provider CTA */}
      <section className="bg-surface-950 text-white">
        <div className="mx-auto max-w-6xl px-6 py-24 text-center">
          <h2 className="text-2xl font-bold sm:text-3xl">
            Got a GPU?{" "}
            <span className="text-brand-400">Earn ERG.</span>
          </h2>
          <p className="mx-auto mt-4 max-w-xl text-surface-200/80">
            Turn idle mining hardware into an AI inference node. Earn ERG on top
            of block rewards -- no changes to your existing Ergo setup.
          </p>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
            <Link
              href="/become-provider"
              className="rounded-lg bg-brand-600 px-6 py-2.5 text-sm font-semibold text-white shadow-lg shadow-brand-600/25 transition-colors hover:bg-brand-700"
            >
              Become a Provider
            </Link>
            <Link
              href="/provider"
              className="rounded-lg border border-white/20 bg-white/5 px-6 py-2.5 text-sm font-semibold text-white backdrop-blur transition-colors hover:bg-white/10"
            >
              Read the Docs
            </Link>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t border-surface-200 bg-surface-0">
        <div className="mx-auto flex max-w-6xl flex-col items-center gap-4 px-6 py-8 text-center sm:flex-row sm:justify-between">
          <p className="text-sm text-surface-800/50">
            Built on{" "}
            <a
              href="https://ergoplatform.org"
              target="_blank"
              rel="noopener noreferrer"
              className="font-medium text-brand-600 hover:text-brand-700 transition-colors"
            >
              Ergo
            </a>{" "}
            &middot; Powered by{" "}
            <a
              href="https://degens.world"
              target="_blank"
              rel="noopener noreferrer"
              className="font-medium text-brand-600 hover:text-brand-700 transition-colors"
            >
              degens.world
            </a>
          </p>
          <p className="text-xs text-surface-800/30">
            &copy; {new Date().getFullYear()} Xergon Network
          </p>
        </div>
      </footer>
    </>
  );
}
