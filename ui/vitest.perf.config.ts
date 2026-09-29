/**
 * The timed tests alone (`*.perf.test.ts`): one file at a time in one
 * worker, so each measures the code rather than its neighbours' load, and
 * each holds the spec.md 13 budget itself. `npm run ui:perf`; CI runs it
 * after `ui:test`.
 */
import { defineConfig } from "vitest/config";

import base from "./vite.config";

// Spread, not mergeConfig: merging would append to the base exclude list,
// which is the list that leaves these files out.
export default defineConfig({
  ...base,
  test: {
    environment: "node",
    include: ["src/**/*.perf.test.ts?(x)"],
    exclude: [],
    fileParallelism: false,
    pool: "forks",
    poolOptions: { forks: { singleFork: true } },
  },
});
