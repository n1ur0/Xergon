import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  // Proxy /api/v1/* to xergon-relay
  async rewrites() {
    return [
      {
        source: "/api/v1/:path*",
        destination: `${process.env.RELAY_URL || "http://127.0.0.1:9090"}/v1/:path*`,
      },
    ];
  },
};

export default nextConfig;
