import type { NextConfig } from "next";
import * as path from 'path';

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
  webpack: (config, { isServer }) => {
    if (!isServer) {
      // Provide empty polyfills for Node.js modules that shouldn't be in browser
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
        os: false,
        path: false,
        http: false,
        https: false,
        crypto: false,
        stream: false,
        buffer: false,
        net: false,
        tls: false,
        child_process: false,
        worker_threads: false,
      };
      
      // Force @xergon/sdk to use browser version
      config.resolve.alias['@xergon/sdk'] = path.resolve(
        __dirname,
        '../xergon-sdk/dist/browser.js'
      );
    }
    return config;
  },
};

export default nextConfig;
