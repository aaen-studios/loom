import type { NextConfig } from "next";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * `next.config.ts` runs as ESM, so `__dirname` does not exist here. It has to be
 * derived, because the value below has to be an absolute path.
 */
const here = dirname(fileURLToPath(import.meta.url));

const nextConfig: NextConfig = {
  reactStrictMode: true,

  // Nothing here benefits from advertising the framework in a response header.
  poweredByHeader: false,

  // Deliberately NOT `output: "export"`. Two things in this site need a server
  // runtime: `/download/latest` is a route handler that redirects to GitHub's
  // CDN, and the release lookup behind it is revalidated on a timer so the
  // published version does not go stale between deploys.
  turbopack: {
    // This project sits inside the desktop app's repository, which has its own
    // `bun.lock` one directory up. Left alone, Turbopack infers the *app's*
    // directory as the workspace root, warns on every build, and would happily
    // resolve a module out of the app's `node_modules` instead of this one's.
    root: here,
  },
};

export default nextConfig;
