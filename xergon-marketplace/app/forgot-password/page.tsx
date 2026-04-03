"use client";

import { useState } from "react";
import Link from "next/link";
import { toast } from "sonner";
import { API_BASE } from "@/lib/api/config";

export default function ForgotPasswordPage() {
  const [email, setEmail] = useState("");
  const [loading, setLoading] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      const res = await fetch(`${API_BASE}/auth/forgot-password`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email }),
      });

      if (!res.ok) {
        const body = await res.json().catch(() => ({ error: { message: "Request failed" } }));
        throw new Error(body.error?.message ?? "Request failed");
      }

      setSent(true);
      toast.success("If the email exists, a reset link was generated");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Request failed";
      setError(message);
      toast.error(message);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex min-h-[calc(100vh-3.5rem)] items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-2xl font-bold">Forgot password</h1>
          <p className="mt-2 text-sm text-surface-800/60">
            Enter your email to receive a password reset link
          </p>
        </div>

        {sent ? (
          <div className="rounded-xl border border-accent-500/30 bg-accent-500/10 p-6 text-center">
            <p className="text-sm text-accent-600 font-medium mb-4">
              Check your email for a reset link
            </p>
            <p className="text-sm text-surface-800/50 mb-4">
              If an account exists with that email, you will receive a password reset link shortly.
            </p>
            <Link
              href="/signin"
              className="inline-block rounded-lg bg-brand-600 px-6 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
            >
              Back to sign in
            </Link>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            {error && (
              <div className="rounded-lg border border-danger-500/30 bg-danger-500/10 px-4 py-2 text-sm text-danger-600">
                {error}
              </div>
            )}

            <div>
              <label htmlFor="email" className="mb-1 block text-sm font-medium text-surface-800/70">
                Email
              </label>
              <input
                id="email"
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20"
                placeholder="you@example.com"
              />
            </div>

            <button
              type="submit"
              disabled={loading}
              className="w-full rounded-lg bg-brand-600 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
            >
              {loading ? "Sending..." : "Send reset link"}
            </button>

            <p className="text-center text-sm text-surface-800/50">
              Remember your password?{" "}
              <Link href="/signin" className="font-medium text-brand-600 hover:underline">
                Sign in
              </Link>
            </p>
          </form>
        )}
      </div>
    </div>
  );
}
