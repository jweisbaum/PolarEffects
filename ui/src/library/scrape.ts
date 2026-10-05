/**
 * A track library scrape as the interface follows it (asked 2026-10-04):
 * read once, then kept current by the scraper's progress event, for the
 * Settings section and the status bar alike.
 */
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { ScrapeProgress } from "../generated/ScrapeProgress";
import { api, LIBRARY_SCRAPE } from "../ipc";

const listeners = new Set<(progress: ScrapeProgress) => void>();
let latest: ScrapeProgress | null = null;
let subscribed = false;

function publish(progress: ScrapeProgress) {
  latest = progress;
  for (const listener of listeners) listener(progress);
}

function subscribe() {
  if (subscribed) return;
  subscribed = true;
  listen<ScrapeProgress>(LIBRARY_SCRAPE, (event) => publish(event.payload)).catch(() => { subscribed = false; });
  api.libraryScrapeStatus().then(publish).catch(() => undefined);
}

/** Hands on a scrape's progress the moment a command answers it, before its first event. */
export function scrapeStarted(progress: ScrapeProgress): void {
  publish(progress);
}

/** The scrape's progress, current; null until it is first read. */
export function useLibraryScrape(): ScrapeProgress | null {
  const [progress, setProgress] = useState<ScrapeProgress | null>(latest);
  useEffect(() => {
    subscribe();
    listeners.add(setProgress);
    if (latest !== null) setProgress(latest);
    return () => { listeners.delete(setProgress); };
  }, []);
  return progress;
}
