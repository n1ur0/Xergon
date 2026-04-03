export default function NotFound() {
  return (
    <div className="mx-auto flex min-h-[60vh] max-w-md flex-col items-center justify-center px-4">
      <h2 className="text-xl font-bold">404 — Not Found</h2>
      <p className="mt-2 text-sm text-surface-800/60">
        This page does not exist.
      </p>
      <a
        href="/"
        className="mt-4 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
      >
        Back to Playground
      </a>
    </div>
  );
}
