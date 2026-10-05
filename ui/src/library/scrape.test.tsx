// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { ScrapeProgress } from "../generated/ScrapeProgress";

const base: ScrapeProgress = { running: false, manual: true, done: 0, total: 0, tracks: 0, skipped: 0, held: 0, failed: 0, current: "", cancelled: false, error: null, failures: [] };
const api = vi.hoisted(() => ({ libraryScrapeStatus: vi.fn() }));
vi.mock("../ipc", () => ({ api, LIBRARY_SCRAPE: "library://scrape" }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => undefined)) }));
const { scrapeStarted, useLibraryScrape } = await import("./scrape");
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A scrape with nothing to fetch ends before its start command answers
 * (2026-10-05: every race already in the library): the late "running"
 * answer must not leave the status running for good. */
it("a scrape that finished before its start answered is shown finished", async () => {
  api.libraryScrapeStatus.mockResolvedValue(base);
  let seen: ScrapeProgress | null = null;
  function Probe() { seen = useLibraryScrape(); return null; }
  const host = document.createElement("div");
  const root = createRoot(host);
  await act(async () => root.render(<Probe />));
  api.libraryScrapeStatus.mockResolvedValue({ ...base, held: 1 });
  await act(async () => scrapeStarted({ ...base, running: true }));
  expect(seen).toMatchObject({ running: false, held: 1 });
  await act(async () => root.unmount());
});
