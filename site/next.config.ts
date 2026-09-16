import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  // The site runs no server of its own; `x-powered-by` advertises a framework
  // for no benefit.
  poweredByHeader: false,

  turbopack: {
    // Pinned deliberately. This project lives inside the desktop app's
    // repository, which has its own `bun.lock` one level up, so Next's
    // workspace-root inference walks up and picks the *app's* directory. That
    // makes Turbopack resolve from the wrong root and warn on every build.
    // Pinning it here also keeps the site's bundle from ever pulling a module
    // out of the app's `node_modules`.
    root: __dirname,
  },
};

export default nextConfig;
