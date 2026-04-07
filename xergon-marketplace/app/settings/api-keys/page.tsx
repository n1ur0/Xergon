"use client";

export default function ApiKeysPage() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-lg font-semibold text-surface-900">API Keys</h2>
        <p className="text-sm text-surface-800/50 mt-0.5">Manage API keys for programmatic access</p>
      </div>

      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="font-semibold">Your API Keys</h3>
          <button className="inline-flex items-center px-3 py-1.5 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors">
            Generate New Key
          </button>
        </div>
        <p className="text-sm text-surface-800/50">
          API keys allow you to access the Xergon API programmatically. Keep your keys secure and never share them.
        </p>
        <div className="mt-4 text-center py-8 text-surface-800/30 rounded-lg border border-dashed border-surface-200">
          No API keys yet. Click &quot;Generate New Key&quot; to create one.
        </div>
      </section>
    </div>
  );
}
