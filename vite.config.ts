import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  // Monaco's workers, and the reason this is here rather than left to the
  // default.
  //
  // Vite's `worker.format` defaults to `iife`, which **cannot code-split**. The
  // TypeScript worker pulls in the whole of `typescript.js` on top of Monaco's
  // own language services, and bundling that into one IIFE fails outright with
  // "UMD and IIFE output formats are not supported for code-splitting builds" —
  // a build error, but only once the editor is actually opened.
  //
  // `es` lets Vite emit the worker as a module with its own chunks, which is
  // also what `lib/monaco.ts` needs: it imports each worker through the `?worker`
  // form, so Vite emits real same-origin files rather than blob URLs. A blob URL
  // is refused by `script-src 'self'`, and — this is the part that matters — the
  // refusal is *silent*: Monaco falls back to doing the work on the main thread
  // and looks merely slow rather than broken.
  worker: {
    format: "es",
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore Rust build output (workspace root `target/`)
      //    and never watch the crates being compiled by `tauri dev`
      ignored: ["**/target/**", "**/src-tauri/**", "**/crates/**"],
    },
  },
}));
