// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { BoatTrackSearch as SearchResult } from "../generated/BoatTrackSearch";

const available = vi.fn<() => Promise<boolean>>();
const handlers = new Map<string, (e: { payload: { running: boolean } }) => void>();
vi.mock("@tauri-apps/api/event", () => ({ listen: (name: string, handler: (e: { payload: { running: boolean } }) => void) => { handlers.set(name, handler); return Promise.resolve(() => handlers.delete(name)); } }));
const search = vi.fn<(...args: unknown[]) => Promise<SearchResult>>();
vi.mock("../ipc", () => ({ api: { trackMetadataAvailable: () => available(), searchDatabaseBoats: (...args: unknown[]) => search(...args) }, LIBRARY_METADATA: "library://metadata", LIBRARY_SCRAPE: "library://scrape" }));
const { default: BoatTrackSearch } = await import("./BoatTrackSearch");
(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root;
let host: HTMLDivElement;

beforeEach(async () => {
  vi.useFakeTimers();
  search.mockReset();
  available.mockReset().mockResolvedValue(true);
  handlers.clear();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<BoatTrackSearch onImport={() => undefined} />));
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.useRealTimers();
});
const type = async (value: string) => act(async () => {
  const input = host.querySelector("input")!;
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
});
const tick = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

it("coalesces typing and starts searching within 50 ms", async () => {
  search.mockResolvedValue({ total: 2, hits: [], downloaded: true });
  await type("a");
  await tick(25);
  await type("assent");
  await tick(49);
  expect(search).not.toHaveBeenCalled();
  await tick(1);
  expect(search).toHaveBeenCalledTimes(1);
  expect(search).toHaveBeenCalledWith("assent", 0);
  expect(host.textContent).toContain("2 matching tracks");
});

it("never displays a slow previous query over the current results", async () => {
  let finish!: (result: SearchResult) => void;
  search.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  search.mockResolvedValue({ total: 2, hits: [], downloaded: true });
  await type("old");
  await tick(50);
  await type("new");
  await tick(50);
  await act(async () => finish({ total: 99, hits: [], downloaded: true }));
  expect(host.textContent).toContain("2 matching tracks");
  expect(host.textContent).not.toContain("99 matching tracks");
});

it("hides the heading, search input and results when no metadata file exists", async () => {
  available.mockResolvedValue(false);
  await act(async () => root.render(<BoatTrackSearch metadataDirectory="/missing" onImport={() => undefined} />));
  expect(host.innerHTML).toBe("");
  expect(search).not.toHaveBeenCalled();
});

it("reveals search when a metadata download or scrape creates the file", async () => {
  for (const name of ["library://metadata", "library://scrape"]) {
    available.mockResolvedValue(false);
    await act(async () => window.dispatchEvent(new Event("focus")));
    expect(host.innerHTML).toBe("");
    available.mockResolvedValue(true);
    await act(async () => handlers.get(name)!({ payload: { running: false } }));
    expect(host.textContent).toContain("Search tracks by vessel details");
    expect(host.querySelector('input[type="search"]')).not.toBeNull();
  }
});

it("hides existing results if the file disappears, and rechecks a new directory", async () => {
  search.mockResolvedValue({ total: 2, hits: [], downloaded: true });
  await type("assent");
  await tick(50);
  expect(host.querySelector(".boat-track-results")).not.toBeNull();
  search.mockResolvedValue({ total: 0, hits: [], downloaded: false });
  await type("missing");
  await tick(50);
  expect(host.innerHTML).toBe("");
  await act(async () => root.render(<BoatTrackSearch metadataDirectory="/new" onImport={() => undefined} />));
  expect(host.querySelector('input[type="search"]')).not.toBeNull();
});

it("searches the new metadata directory even when both directories have files", async () => {
  search.mockResolvedValue({ total: 2, hits: [], downloaded: true });
  await type("assent");
  await tick(50);
  expect(host.textContent).toContain("2 matching tracks");
  search.mockResolvedValue({ total: 3, hits: [], downloaded: true });
  await act(async () => root.render(<BoatTrackSearch metadataDirectory="/another" onImport={() => undefined} />));
  await tick(50);
  expect(host.textContent).toContain("3 matching tracks");
});
