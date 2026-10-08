// @vitest-environment happy-dom
/**
 * How fast the tracker dialog shows a big event (plan.md M14b): the Fastnet
 * 2025's 444 boats with a full map preview each, as the IPC hands them over.
 * Prints the time from a metadata answer to a pickable table, from the
 * requested download to the table and map, to tick one boat and to search.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { TrackerBoatRow } from "../generated/TrackerBoatRow";
import type { TrackerEventView } from "../generated/TrackerEventView";

let answer: (view: TrackerEventView) => void = () => undefined;
vi.mock("../ipc", () => ({
  api: {
    trackerEvent: () => {
      return new Promise<TrackerEventView>((resolve) => { answer = resolve; });
    },
    cancelTrackerEvent: () => Promise.resolve(),
    importTrackerBoats: () => Promise.resolve(),
  },
  TRACKER_PROGRESS: "tracker://progress",
  TRACKER_LISTED: "tracker://listed",
}));
const handlers = new Map<string, (e: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (e: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return Promise.resolve(() => { if (handlers.get(name) === handler) handlers.delete(name); });
  },
}));
vi.mock("../map/basemap", () => ({ loadBasemap: () => new Promise(() => undefined) }));

const { default: TrackerImportDialog } = await import("./TrackerImportDialog");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
}, 30_000);
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

const BOATS = 444;
const POINTS = 64;

/** A Fastnet-like fleet: Cowes to the Rock and back to Cherbourg. */
function fleet(): TrackerEventView {
  const boats: TrackerBoatRow[] = Array.from({ length: BOATS }, (_, k) => {
    const preview: number[] = [];
    for (let p = 0; p < POINTS; p++) {
      const s = p / (POINTS - 1);
      preview.push(-1.3 - 8 * Math.sin(Math.PI * s) + 0.01 * k, 50.7 + 0.8 * Math.sin(Math.PI * s) - 0.01 * k);
    }
    return {
      id: String(k + 1), name: `Boat ${k + 1}`, sail: `GBR ${1000 + k}`, model: "JPK 1080", division: `IRC ${k % 5}`, classes: [`IRC ${k % 5}`],
      status: "FINISHED", fixes: 1609, first: 1_753_531_200, last: 1_753_963_200, preview,
    };
  });
  return {
    tracker: "yellowbrick", key: "fastnet2025", url: "yb.tl/fastnet2025", title: "Rolex Fastnet 2025",
    start: 1_753_531_200, stop: 1_754_136_000, fallback: false, leg: null, legs: null, cached: false, positions: true, boats,
  };
}

const timed = async (what: () => Promise<void>) => {
  const start = performance.now();
  await what();
  return performance.now() - start;
};

it("shows, ticks and searches a 444-boat event interactively", async () => {
  await act(async () => root.render(<TrackerImportDialog tracker="yellowbrick" onDone={() => undefined} onCancel={() => undefined} />));
  const url = host.querySelector('[data-feature="tracker-import:url"]') as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(url, "yb.tl/fastnet2025");
    url.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { (host.querySelector('[data-feature="tracker-import:open"]') as HTMLElement).click(); });
  const view = fleet();
  // The boat list first, as YellowBrick's RaceSetup gives it.
  const listing: TrackerEventView = {
    ...view, positions: false, boats: view.boats.map((b) => ({ ...b, fixes: 0, first: null, last: null, preview: [] })),
  };
  const listed = await timed(() => act(async () => { answer(listing); }));
  expect(host.querySelectorAll("tbody tr")).toHaveLength(BOATS);
  expect((host.querySelector(".modal-actions button.primary") as HTMLButtonElement).disabled).toBe(true);
  await act(async () => { (host.querySelector("tbody input") as HTMLElement).click(); });
  await act(async () => { (host.querySelector(".modal-actions button.primary") as HTMLElement).click(); });
  const show = await timed(() => act(async () => { answer(view); await Promise.resolve(); }));
  expect(host.querySelectorAll("tbody tr")).toHaveLength(BOATS);
  expect(host.querySelector(".tracker-preview-line:not(.chosen)")!.getAttribute("data-lines")).toBe(String(BOATS - 1));
  expect(host.querySelector(".tracker-preview-line.chosen")!.getAttribute("data-lines")).toBe("1");
  await act(async () => { (host.querySelector("tbody input") as HTMLElement).click(); });
  const tick = await timed(() => act(async () => { (host.querySelector("tbody input") as HTMLElement).click(); }));
  expect(host.querySelector(".tracker-preview-line.chosen")!.getAttribute("data-lines")).toBe("1");
  const search = host.querySelector('[data-feature="tracker-import:search"]') as HTMLInputElement;
  const find = await timed(() => act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(search, "gbr 1100");
    search.dispatchEvent(new Event("input", { bubbles: true }));
  }));
  expect(host.querySelectorAll("tbody tr")).toHaveLength(1);
  // eslint-disable-next-line no-console
  console.log(`M14b | UI, ${BOATS} boats × ${POINTS} preview points: boat list ${listed.toFixed(0)} ms, then table and map ${show.toFixed(0)} ms, tick one ${tick.toFixed(0)} ms, search ${find.toFixed(0)} ms`);
  // happy-dom is several times slower than a webview; these bounds only
  // catch a return to quadratic work.
  expect(listed).toBeLessThan(6000);
  expect(show).toBeLessThan(6000);
  expect(tick).toBeLessThan(1000);
}, 30_000);
