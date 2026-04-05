import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  // Proxy /api/v1/* and /ws/* to xergon-relay
  async rewrites() {
    return [
      {
        source: "/api/v1/:path*",
        destination: `${process.env.RELAY_URL || "http://127.0.0.1:9090"}/v1/:path*`,
      },
      {
        source: "/ws/:path*",
        destination: `${process.env.RELAY_URL || "http://127.0.0.1:9090"}/ws/:path*`,
      },
    ];
  },
};

export default nextConfig;
