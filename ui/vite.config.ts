import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { configDefaults } from "vitest/config";

// Everything must resolve from the bundle: no CDN, no remote fonts, no runtime
// network access of any kind (invariant 4). `tools/check-offline.sh` enforces it.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // The port is fixed so the Tauri config's devUrl can name it, and
  // `PE_DEV_PORT` moves both together: a WebDriver run (D25) then starts
  // beside a `tauri dev` somebody is already using, rather than failing on
  // the port in use or killing what holds it.
  server: { port: Number(process.env.PE_DEV_PORT ?? 5173), strictPort: true },
  // A driver run keeps its dependency pre-bundle apart from the person's own
  // dev server, so two servers never rewrite one cache under each other.
  ...(process.env.PE_VITE_CACHE_DIR ? { cacheDir: process.env.PE_VITE_CACHE_DIR } : {}),
  build: {
    outDir: "dist",
    target: "es2022",
    sourcemap: true,
    // Fail the build rather than silently emitting a remote reference.
    rollupOptions: { external: [] },
  },
  // Node by default: these are pure-logic tests. A component test opts into a
  // DOM per file with `// @vitest-environment happy-dom`. happy-dom rather
  // than jsdom: jsdom 27's CSS parser is a CommonJS build that `require()`s an
  // ES module, which only Node 20.19+ and 22.12+ allow, and the machine this
  // is developed on runs 21.
  // Timed tests (`*.perf.test.ts`) run alone, serially, in `npm run ui:perf`
  // (vitest.perf.config.ts): beside the parallel suite's other workers a
  // 100 ms budget measures the machine's load, not the code (M17b review).
  test: {
    environment: "node",
    include: ["src/**/*.test.ts?(x)"],
    exclude: [...configDefaults.exclude, "src/**/*.perf.test.ts?(x)"],
  },
});
