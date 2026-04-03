"use client";

export default function GlobalError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <html lang="en">
      <body style={{ margin: 0, padding: "2rem", fontFamily: "system-ui, sans-serif", background: "#f8fafc", color: "#1e293b" }}>
        <div style={{ maxWidth: "28rem", margin: "0 auto" }}>
          <h2 style={{ color: "#dc2626" }}>Something went wrong</h2>
          <p style={{ marginTop: "0.5rem", fontSize: "0.875rem", color: "#64748b" }}>
            {error.message || "An unexpected error occurred."}
          </p>
          <button
            onClick={reset}
            style={{ marginTop: "1rem", padding: "0.5rem 1rem", borderRadius: "0.5rem", background: "#1b55f5", color: "white", border: "none", cursor: "pointer" }}
          >
            Try again
          </button>
        </div>
      </body>
    </html>
  );
}
