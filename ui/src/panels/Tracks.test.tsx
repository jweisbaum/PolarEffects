// @vitest-environment happy-dom
/**
 * The Tracks section (spec.md 7.1, 7.3, 7.6): File… inspects the chosen
 * files, the dialog maps a CSV's columns and picks boats, Import sends the
 * answers to Rust, and a track's filters and derivation are edited through
 * their commands.
 */
import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import type { TrackFileInspection } from "../generated/TrackFileInspection";
import type { TrackSummary } from "../generated/TrackSummary";
import { TEST_BLEND } from "../testBlend";

const calls: [string, unknown][] = [];
/** A response that is a failure, as Rust sends one. */
class Rejection { constructor(readonly error: unknown) {} }
const record = (name: string) => (...args: unknown[]) => {
  calls.push([name, args]);
  const response = responses[name];
  if (response instanceof Promise) return response;
  return response instanceof Rejection ? Promise.reject(response.error) : Promise.resolve(response);
};
const responses: Record<string, unknown> = {};
vi.mock("../ipc", () => ({
  api: new Proxy({}, { get: (_t, name: string) => record(name) }),
  TRACKER_PROGRESS: "tracker://progress",
  TRACKER_LISTED: "tracker://listed",
  LIBRARY_METADATA: "library://metadata", LIBRARY_SCRAPE: "library://scrape",
}));
/** The Tauri event handlers the components listen with, by event name. */
const handlers = new Map<string, (e: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (e: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return Promise.resolve(() => { if (handlers.get(name) === handler) handlers.delete(name); });
  },
}));
vi.mock("../project/dialogs", () => ({ pickTrackFiles: () => Promise.resolve(["/races/log.csv", "/races/fleet.geojson"]) }));
const focus = vi.fn();
vi.mock("../selection", () => ({ useBoatSelection: () => ({ focusMap: (f: unknown) => focus(f) }) }));

const { default: Tracks } = await import("./Tracks");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

const TRACK: TrackSummary = {
  origin: "file", boat_name: "Alpha", event_title: "fleet.geojson", start: 1_753_531_200, end: 1_753_617_600,
  samples: 120, filtered: 20, excluded: 0, used: 100, with_wind: 0, env_status: "not_fetched", env_fetched: 0, env_interval: null, no_tide: 0, max_gap_s: 10_800,
  prefer_heading: "given", prefer_speed: "given", supplied_wind: 0, supplied_heading: 0, supplied_speed: 0, downloaded_wind_only: false, environment_filters: false,
  filters: { max_awa_change: null, max_wind_speed_change: null, max_wind_direction_change: null, time_start: null, time_end: null, min_bsp: 1, max_bsp: null, max_heading_change: 30, heading_from: null, heading_to: null, cog_from: null, cog_to: null, vmg_min: null, vmg_max: null, twd_from: null, twd_to: null, exclude_tacks: false,
    tws_min: null, tws_max: null, twa_min: null, twa_max: null, hs_min: null, hs_max: null, current_min: null, current_max: null,
    wave_mode: "off", wave_sectors: [], wave_min: null, wave_max: null, wave_from: null, wave_to: null, exclude_no_tide: false, exclude_unknown_wave: false, exclude_unknown_current: false },
};
const SOURCE: SourceSummary = {
  id: 5, kind: "track", label: "Alpha", colour: "#e15759", visible: true, weight: 1, count: 120, used: 100,
  polar_file: null, orr: null, orc: null, track: TRACK, edits: 0,
};
const project = (sources: SourceSummary[]): ProjectSummary => ({
  id: 1, name: "P", path: null, dirty: false, revision: 1, boat_name: "", boat_notes: "", sources,
  can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false, blend: TEST_BLEND,
});

const CSV: TrackFileInspection = {
  path: "/races/log.csv", file: "log.csv", kind: "csv", boats: [], failure: {
    file: "log.csv", line: null, column: null, feature: null, reason: "no-mapping", message: "",
  },
  csv: {
    header: ["when", "lat", "lon"], rows: [["26/07/2025 12:00", "50", "-1"]], row_count: 1,
    mapping: { time: null, lat: 1, lon: 2, heading: null, speed: null, boat: null, time_format: "auto", custom_format: "", speed_unit: "kn", wind_speed_unit: "kn", tws: null, twd: null },
  },
};
const GEOJSON: TrackFileInspection = {
  path: "/races/fleet.geojson", file: "fleet.geojson", kind: "geojson", csv: null, failure: null,
  boats: [
    { name: "Alpha", fixes: 3, start: 1_753_531_200, end: 1_753_532_400 },
    { name: "Bravo", fixes: 3, start: 1_753_531_200, end: 1_753_532_400 },
  ],
};

const click = async (el: Element | null) => { await act(async () => { (el as HTMLElement).click(); }); };
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const q = (selector: string) => host.querySelector(selector);

beforeEach(() => {
  for (const key of Object.keys(responses)) delete responses[key];
  calls.length = 0;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("lists a track with its boat, event, dates, samples used and environment status", async () => {
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  const text = q(".track-list")!.textContent!;
  expect(text).toContain("Alpha");
  expect(text).toContain("fleet.geojson · 2025-07-26 – 2025-07-27");
  expect(text).toContain("100 of 120 samples used");
  expect(text).toContain("Weather: not fetched");
  await click(q('[data-feature="tracks:show-on-map"]'));
  expect(focus).toHaveBeenCalledWith({ kind: "track", sourceId: 5 });
  // YellowBrick, Geovoile and Blue Water Tracks all open the tracker dialog.
  expect((q('[data-feature="tracks:yellowbrick"]') as HTMLButtonElement).disabled).toBe(false);
  expect((q('[data-feature="tracks:geovoile"]') as HTMLButtonElement).disabled).toBe(false);
  expect((q('[data-feature="tracks:bluewater"]') as HTMLButtonElement).disabled).toBe(false);
});

it("edits the filters and the derivation through their commands", async () => {
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:filters"]'));
  const min = q('[data-feature="tracks:min-bsp"]') as HTMLInputElement;
  expect(min.value).toBe("1");
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(min, "3");
    min.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { min.blur(); min.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  expect(calls.find(([n]) => n === "setTrackFilters")?.[1]).toEqual([5, { ...TRACK.filters, min_bsp: 3 }]);

  // A provided-or-derived choice is offered only for what the track gave
  // (asked 2026-10-02): this track gives nothing, so none is shown.
  expect(q('[data-feature="tracks:heading-source"]')).toBeNull();
  expect(q('[data-feature="tracks:speed-source"]')).toBeNull();
  expect(q('[data-feature="tracks:wind-source"]')).toBeNull();
  // Nor the options that are gone, nor a separate derivation section.
  for (const gone of ["tack-window", "stop-speed", "stop-window", "utc-interval", "utc-unit", "heading-origin", "speed-origin", "prefer", "max-gap"]) {
    expect(q(`[data-feature="tracks:${gone}"]`)).toBeNull();
  }
  const tacks = q('[data-feature="tracks:tacks"]') as HTMLInputElement;
  expect(tacks.checked).toBe(false);
  await act(async () => { tacks.click(); });
  expect(calls.filter(([n]) => n === "setTrackFilters").at(-1)?.[1]).toEqual([5, { ...TRACK.filters, min_bsp: 3, exclude_tacks: true }]);
  await act(async () => { tacks.click(); });
  // The environment filters are edited the same way (spec.md 7.6).
  const tws = q('[data-feature="tracks:tws-min"]') as HTMLInputElement;
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(tws, "8");
    tws.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => { tws.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  expect(calls.filter(([n]) => n === "setTrackFilters").at(-1)?.[1]).toEqual([5, { ...TRACK.filters, min_bsp: 3, tws_min: 8 }]);
  const mode = q('[data-feature="tracks:wave-mode"]') as HTMLSelectElement;
  await act(async () => {
    mode.value = "sectors";
    mode.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(calls.filter(([n]) => n === "setTrackFilters").at(-1)?.[1]).toEqual([5, { ...TRACK.filters, min_bsp: 3, tws_min: 8, wave_mode: "sectors" }]);
});

it("applies numeric filters while typing and preserves edits made during a slow response", async () => {
  let release!: (value: unknown) => void;
  responses.setTrackFilters = new Promise((resolve) => { release = resolve; });
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:filters"]'));
  const type = async (name: string, value: string) => {
    const input = q(`[data-feature="tracks:${name}"]`) as HTMLInputElement;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => { await new Promise((r) => setTimeout(r, 150)); });
  };
  await type("min-bsp", "3");
  expect(calls.filter(([name]) => name === "setTrackFilters")).toHaveLength(1);
  await type("wind-speed-change", "2");
  await type("wind-direction-change", "120");
  expect(calls.filter(([name]) => name === "setTrackFilters")).toHaveLength(1);
  await act(async () => {
    responses.setTrackFilters = project([SOURCE]);
    release(project([SOURCE]));
  });
  const sent = calls.filter(([name]) => name === "setTrackFilters");
  expect(sent).toHaveLength(2);
  expect(sent[1]![1]).toEqual([5, { ...TRACK.filters, min_bsp: 3, max_wind_speed_change: 2, max_wind_direction_change: 120 }]);
  delete responses.setTrackFilters;
});

it("imports files without fetching weather; Fetch weather starts immediately without estimating", async () => {
  responses.inspectTrackFiles = [GEOJSON];
  const line = {
    source_id: 5, file: "fleet.geojson", label: "Alpha", fixes: 3, out_of_order: 0, duplicates: 0,
    heading_given: 0, heading_derived: 3, speed_given: 0, speed_derived: 3,
  };
  responses.importTrackFiles = { project: project([SOURCE]), imported: [line], failures: [] };
  responses.startEnvFetch = { tracks: [], failure: null, warning: null };
  await act(async () => root.render(<Tracks project={project([])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:import-file"]'));
  await settle();
  await click(q(".modal-actions button.primary"));
  await settle();
  // The import dialog closed and nothing about weather was asked (D24).
  expect(q("[role=dialog]")).toBeNull();
  expect(calls.filter(([n]) => n === "envEstimate" || n === "startEnvFetch")).toEqual([]);
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:fetch-weather"]'));
  expect(calls.filter(([n]) => n === "envEstimate")).toEqual([]);
  expect(calls.find(([n]) => n === "startEnvFetch")?.[1]).toEqual([[5], false]);
  expect(q("[role=dialog]")).toBeNull();
});

it("refetches ready tracks immediately and prevents duplicate starts while queuing", async () => {
  let release!: (value: unknown) => void;
  responses.startEnvFetch = new Promise(resolve => { release = resolve; });
  const ready = { ...SOURCE, track: { ...TRACK, env_status: "ready" as const } };
  await act(async () => root.render(<Tracks project={project([ready])} onProject={() => undefined} />));
  expect(q(".track-list")!.textContent).toContain("Weather: no points to plot");
  const button = q('[data-feature="tracks:fetch-weather"]') as HTMLButtonElement;
  await click(button);
  expect(button.disabled).toBe(true);
  await click(button);
  expect(calls.filter(([n]) => n === "startEnvFetch")).toEqual([["startEnvFetch", [[5], true]]]);
  expect(calls.filter(([n]) => n === "envEstimate")).toEqual([]);
  expect(q("[role=dialog]")).toBeNull();
  await act(async () => { release({ tracks: [], failure: null, warning: null }); });
  expect(button.disabled).toBe(false);
  delete responses.startEnvFetch;
});

it("shows ready when fetched weather can place samples on the polar", async () => {
  const ready = { ...SOURCE, track: { ...TRACK, env_status: "ready" as const, env_fetched: 120, with_wind: 110 } };
  await act(async () => root.render(<Tracks project={project([ready])} onProject={() => undefined} />));
  expect(q(".track-list")!.textContent).toContain("Weather: ready");
});

it("shows a running fetch in the track list and cancels it", async () => {
  const { setEnvJobs, resetEnvJobs } = await import("../jobs");
  setEnvJobs({ tracks: [{ source_id: 5, label: "Alpha", state: "fetching", fraction: 0.42 }], failure: null, warning: null });
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  expect(q(".track-list")!.textContent).toContain("Weather: fetching 42 %");
  expect(q('[data-feature="tracks:fetch-weather"]')).toBeNull();
  await click(q('[data-feature="tracks:cancel-fetch"]'));
  expect(calls.find(([n]) => n === "cancelEnvFetch")?.[1]).toEqual([[5]]);
  await act(async () => resetEnvJobs());
});

it("maps a CSV, picks boats and imports them as one request", async () => {
  responses.inspectTrackFiles = [CSV, GEOJSON];
  responses.inspectCsvTrack = { ...CSV, failure: null, boats: [{ name: "", fixes: 1, start: 1_753_531_200, end: 1_753_531_200 }],
    csv: { ...CSV.csv!, mapping: { ...CSV.csv!.mapping, time: 0, time_format: "custom", custom_format: "%d/%m/%Y %H:%M" } } };
  responses.importTrackFiles = { project: project([SOURCE]), imported: [], failures: [] };
  const onProject = vi.fn();
  await act(async () => root.render(<Tracks project={project([])} onProject={onProject} />));
  await click(q('[data-feature="tracks:import-file"]'));
  await settle();
  expect(q("[role=dialog]")).not.toBeNull();
  // The CSV cannot import until its time column is chosen.
  expect(q(".modal-actions button.primary")!.textContent).toBe("Import 1 of 2 files");
  // The first column select is the time's.
  const time = q(".track-import-mapping select") as HTMLSelectElement;
  vi.useFakeTimers();
  await act(async () => {
    time.value = "0";
    time.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await act(async () => { vi.advanceTimersByTime(400); });
  vi.useRealTimers();
  await settle();
  expect(calls.find(([n]) => n === "inspectCsvTrack")?.[1]).toEqual([
    "/races/log.csv", { ...CSV.csv!.mapping, time: 0 },
  ]);
  expect(q(".modal-actions button.primary")!.textContent).toBe("Import");

  // Untick Bravo.
  const boxes = [...host.querySelectorAll<HTMLInputElement>(".track-import-boats input")];
  expect(boxes.length).toBe(2);
  await click(boxes[1]!);
  await click(q(".modal-actions button.primary"));
  await settle();
  const [files] = calls.find(([n]) => n === "importTrackFiles")![1] as [unknown[]];
  expect(files).toEqual([
    { path: "/races/log.csv", mapping: { ...CSV.csv!.mapping, time: 0, time_format: "custom", custom_format: "%d/%m/%Y %H:%M" }, boats: [""] },
    { path: "/races/fleet.geojson", mapping: null, boats: ["Alpha"] },
  ]);
  expect(onProject).toHaveBeenCalled();
  expect(q("[role=dialog]")).toBeNull();
});

it("applies a complete time-window edit without requiring blur", async () => {
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:filters"]'));
  const start = q('[data-feature="tracks:time-start"]') as HTMLInputElement;
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  for (const partial of ["2025-07-26T1", "2025-07-26T12:0", "2025-07-26T12:00"]) {
    await act(async () => {
      set.call(start, partial);
      start.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }
  expect(calls.filter(([n]) => n === "setTrackFilters")).toHaveLength(1);
  await act(async () => { start.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  const sent = calls.filter(([n]) => n === "setTrackFilters");
  expect(sent).toHaveLength(1);
  expect(sent[0]![1]).toEqual([5, { ...TRACK.filters, time_start: 1_753_531_200 }]);
});

const boat = (id: string, name: string, sail: string, division: string, fixes: number) => ({
  id, name, sail, model: "First 40", division, status: fixes > 0 ? "RACING" : null, fixes,
  first: fixes > 0 ? 1_729_233_205 : null, last: fixes > 0 ? 1_729_731_600 : null,
  preview: fixes > 0 ? [14.5, 35.9, 15.2, 36.4] : [],
});
const EVENT = {
  tracker: "yellowbrick", key: "rmsr2024", url: "yb.tl/rmsr2024", title: "Rolex Middle Sea Race 2024",
  start: 1_729_209_600, stop: 1_729_897_200, fallback: false, leg: null, legs: null, cached: false, positions: true,
  boats: [boat("1", "12 NACIRA 69", "ITA17498", "IRC Class 2", 1689), boat("2", "Afazik Impulse", "FRA 9967", "ORC 3", 1692),
    boat("4", "Alquimia", "ESP 1", "IRC Class 4", 0)],
};

it("downloads a YellowBrick event, searches and picks boats, and imports them without fetching weather", async () => {
  responses.trackerEvent = EVENT;
  const line = {
    source_id: 7, file: "Rolex Middle Sea Race 2024", label: "Afazik Impulse", fixes: 1692, out_of_order: 0, duplicates: 3,
    heading_given: 0, heading_derived: 1692, speed_given: 0, speed_derived: 1692,
  };
  responses.importTrackerBoats = { project: project([SOURCE]), imported: [line], failures: [] };
  await act(async () => root.render(<Tracks project={project([])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:yellowbrick"]'));
  const url = q('[data-feature="tracker-import:url"]') as HTMLInputElement;
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(url, " yb.tl/rmsr2024 ");
    url.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await click(q('[data-feature="tracker-import:open"]'));
  await settle();
  expect(calls.find(([n]) => n === "trackerEvent")?.[1]).toEqual(["yellowbrick", "yb.tl/rmsr2024", false, expect.any(String), true]);
  const dialog = q("[role=dialog]")!;
  expect(dialog.textContent).toContain("Rolex Middle Sea Race 2024");
  expect(dialog.textContent).toContain("3 boats");
  expect(dialog.querySelector(".tracker-preview-line")!.getAttribute("data-lines")).toBe("2");
  // Search by sail number, written without its space.
  const search = q('[data-feature="tracker-import:search"]') as HTMLInputElement;
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(search, "fra9967");
    search.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const rows = dialog.querySelectorAll("tbody tr");
  expect(rows).toHaveLength(1);
  expect(rows[0]!.textContent).toContain("Afazik Impulse");
  await click(rows[0]!.querySelector("input"));
  expect(dialog.querySelector(".tracker-preview-line.chosen")!.getAttribute("data-lines")).toBe("1");
  expect(q(".modal-actions button.primary")!.textContent).toBe("Import tracks");
  await click(q(".modal-actions button.primary"));
  await settle();
  expect(calls.find(([n]) => n === "importTrackerBoats")?.[1]).toEqual(["yellowbrick", "rmsr2024", ["2"]]);
  // The dialog closed; no weather is estimated or fetched (D24).
  expect(q('[data-feature="tracker-import:url"]')).toBeNull();
  expect(q("[role=dialog]")).toBeNull();
  expect(calls.filter(([n]) => n === "envEstimate" || n === "startEnvFetch")).toEqual([]);
});

it("says a tracker is not answering and retries", async () => {
  responses.trackerEvent = new Rejection({ kind: "tracker-unavailable", message: "YellowBrick answered 500" });
  await act(async () => root.render(<Tracks project={project([])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:yellowbrick"]'));
  const url = q('[data-feature="tracker-import:url"]') as HTMLInputElement;
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(url, "nosuchrace");
    url.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await click(q('[data-feature="tracker-import:open"]'));
  await settle();
  expect(q("[role=alert]")!.textContent).toContain("The tracker is not answering right now.");
  responses.trackerEvent = EVENT;
  await click(q('[data-feature="tracker-import:retry"]'));
  await settle();
  expect(calls.filter(([n]) => n === "trackerEvent").at(-1)?.[1]).toEqual(["yellowbrick", "nosuchrace", true, expect.any(String), true]);
  expect(q("[role=dialog]")!.textContent).toContain("Rolex Middle Sea Race 2024");
});

const typeAddress = async (text: string) => {
  const url = q('[data-feature="tracker-import:url"]') as HTMLInputElement;
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(url, text);
    url.dispatchEvent(new Event("input", { bubbles: true }));
  });
};

it("offers the other legs of a Geovoile race in legs and downloads the one chosen", async () => {
  const leg = (n: number) => ({
    ...EVENT, tracker: "geovoile", key: `lasolitaire.geovoile.com/2024/?leg=${n}`,
    url: `lasolitaire.geovoile.com/2024/tracker/?leg=${n}`, title: `Solitaire du Figaro (${n}/3)`, leg: n, legs: 3,
  });
  responses.trackerEvent = leg(1);
  await act(async () => root.render(<Tracks project={project([])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:geovoile"]'));
  await typeAddress("lasolitaire.geovoile.com/2024/tracker/?leg=1");
  await click(q('[data-feature="tracker-import:open"]'));
  await settle();
  const select = q('[data-feature="tracker-import:leg"]') as HTMLSelectElement;
  expect([...select.options].map((o) => o.textContent)).toEqual(["Leg 1 of 3", "Leg 2 of 3", "Leg 3 of 3"]);
  expect(select.value).toBe("1");
  responses.trackerEvent = leg(2);
  await act(async () => {
    select.value = "2";
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await settle();
  expect(calls.filter(([n]) => n === "trackerEvent").at(-1)?.[1])
    .toEqual(["geovoile", "lasolitaire.geovoile.com/2024/tracker/?leg=2", false, expect.any(String), true]);
  expect(q("[role=dialog]")!.textContent).toContain("Solitaire du Figaro (2/3)");
  // A race in one leg has no leg choice.
  responses.trackerEvent = EVENT;
  await typeAddress("yb.tl/rmsr2024");
  await click(q('[data-feature="tracker-import:open"]'));
  await settle();
  expect(q('[data-feature="tracker-import:leg"]')).toBeNull();
});

it("shows the downloaded event under StrictMode, which mounts every component twice in development", async () => {
  responses.trackerEvent = EVENT;
  await act(async () => root.render(<StrictMode><Tracks project={project([])} onProject={() => undefined} /></StrictMode>));
  await click(q('[data-feature="tracks:yellowbrick"]'));
  await typeAddress("yb.tl/rmsr2024");
  await click(q('[data-feature="tracker-import:open"]'));
  // The answer is shown, not dropped as if the dialog had closed.
  expect(q(".tracker-progress")).toBeNull();
  expect(document.body.textContent).toContain("Afazik Impulse");
});

it("keeps the dialog open on a backdrop click while an event downloads", async () => {
  responses.trackerEvent = new Promise(() => undefined);
  await act(async () => root.render(<Tracks project={project([])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:geovoile"]'));
  await typeAddress("vendeeglobe.geovoile.com/2016/tracker/");
  await click(q('[data-feature="tracker-import:open"]'));
  await click(q(".modal-backdrop"));
  expect(q("[role=dialog]")).not.toBeNull();
  expect(calls.some(([n]) => n === "cancelTrackerEvent")).toBe(false);
  // One Cancel, which also stops the download.
  expect(q('[data-feature="tracker-import:cancel-download"]')).toBeNull();
  const cancels = [...document.querySelectorAll(".tracker-import button")].filter((b) => b.textContent === "Cancel");
  expect(cancels.length).toBe(1);
  await click(cancels[0] as HTMLElement);
  expect(calls.some(([n]) => n === "cancelTrackerEvent")).toBe(true);
});

it("refuses an older Geovoile tracker clearly, without a Retry", async () => {
  responses.trackerEvent = new Rejection({ kind: "tracker-legacy", message: "this is an older Geovoile tracker" });
  await act(async () => root.render(<Tracks project={project([])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:geovoile"]'));
  await typeAddress("routedurhum.geovoile.com/2014/");
  await click(q('[data-feature="tracker-import:open"]'));
  await settle();
  expect(q("[role=alert]")!.textContent).toContain("This is an older tracker (Flash, or Geovoile before about 2016)");
  expect(q('[data-feature="tracker-import:retry"]')).toBeNull();
  // Once the dialog is idle again, a backdrop click closes it.
  await click(q(".modal-backdrop"));
  expect(q("[role=dialog]")).toBeNull();
});

const LISTING = { ...EVENT, positions: false, boats: EVENT.boats.map(b => ({ ...b, fixes: 0, first: null, last: null, preview: [] })) };
const imported = () => ({ project: project([SOURCE]), imported: [], failures: [] });
async function openListing(listing = LISTING, tracker = "yellowbrick") {
  responses.trackerEvent = listing;
  responses.importTrackerBoats = imported();
  await act(async () => root.render(<StrictMode><Tracks project={project([])} onProject={() => undefined} /></StrictMode>));
  await click(q(`[data-feature="tracks:${tracker}"]`));
  await typeAddress(listing.url);
  await click(q('[data-feature="tracker-import:open"]'));
}

it("loads only boats until Import tracks, then downloads and imports the selection without weather", async () => {
  await openListing();
  expect(q(".tracker-progress")).toBeNull();
  expect(q(".tracker-preview")).toBeNull();
  expect(q("tbody")!.textContent).toContain("—");
  await click(q('tbody input'));
  expect(calls.filter(([n]) => n === "trackerEvent")).toHaveLength(1);
  expect(calls.filter(([n]) => n === "importTrackerBoats")).toHaveLength(0);
  const button = () => q(".modal-actions button.primary") as HTMLButtonElement;
  expect(button().disabled).toBe(false);
  let answer!: (event: typeof EVENT) => void;
  responses.trackerEvent = new Promise(resolve => { answer = resolve; });
  await click(button());
  expect(calls.filter(([n]) => n === "trackerEvent").at(-1)?.[1])
    .toEqual(["yellowbrick", EVENT.url, true, expect.any(String)]);
  expect(button().disabled).toBe(true);
  expect((q('tbody input') as HTMLInputElement).disabled).toBe(true);
  expect(q(".tracker-progress")!.textContent).toContain("Downloading the boats' positions");
  const key = (calls.filter(([n]) => n === "trackerEvent").at(-1)![1] as unknown[])[3];
  await act(async () => { handlers.get("tracker://listed")!({ payload: { download: `${key}-other`, event: { ...LISTING, title: "Stray" } } }); });
  expect(q("[role=dialog]")!.textContent).not.toContain("Stray");
  await act(async () => answer(EVENT));
  expect(calls.filter(([n]) => n === "importTrackerBoats")).toEqual([["importTrackerBoats", ["yellowbrick", EVENT.key, ["1"]]]]);
  expect(q('[data-feature="tracker-import:url"]')).toBeNull();
  expect(calls.filter(([n]) => n === "envEstimate" || n === "startEnvFetch")).toEqual([]);
});

it("keeps the boat list, search and ticks after download failure and retries the selected event", async () => {
  await openListing();
  const search = () => q('[data-feature="tracker-import:search"]') as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(search(), "Afazik");
    search().dispatchEvent(new Event("input", { bubbles: true }));
  });
  const tick = () => q('tbody input') as HTMLInputElement;
  await click(tick());
  responses.trackerEvent = new Rejection({ kind: "tracker-unavailable", message: "positions and KML answered 503" });
  await click(q(".modal-actions button.primary"));
  expect(q("[role=alert]")!.textContent).toContain("The boat list loaded, but the track positions could not be downloaded.");
  expect(q("[role=alert]")!.getAttribute("title")).toContain("positions and KML answered 503");
  expect(q(".tracker-progress")).toBeNull();
  expect(search().value).toBe("Afazik");
  expect(tick().checked).toBe(true);
  expect(calls.filter(([n]) => n === "importTrackerBoats")).toHaveLength(0);
  await typeAddress("yb.tl/another-race");
  responses.trackerEvent = EVENT;
  await click(q('[data-feature="tracker-import:retry"]'));
  expect(calls.filter(([n]) => n === "trackerEvent").at(-1)?.[1])
    .toEqual(["yellowbrick", EVENT.url, true, expect.any(String)]);
  expect(calls.find(([n]) => n === "importTrackerBoats")?.[1]).toEqual(["yellowbrick", EVENT.key, ["2"]]);
  expect(q('[data-feature="tracker-import:url"]')).toBeNull();
});

it("cancels the download and never imports a late response after closing", async () => {
  await openListing();
  await click(q('tbody input'));
  let answer!: (event: typeof EVENT) => void;
  responses.trackerEvent = new Promise(resolve => { answer = resolve; });
  await click(q(".modal-actions button.primary"));
  await click(q(".modal-actions button"));
  expect(calls.filter(([n]) => n === "cancelTrackerEvent")).toHaveLength(1);
  await act(async () => answer(EVENT));
  expect(calls.filter(([n]) => n === "importTrackerBoats")).toHaveLength(0);
  expect(q('[data-feature="tracker-import:url"]')).toBeNull();
});

it("waits for Import tracks before downloading a combined Blue Water Tracks response", async () => {
  const event = { ...EVENT, tracker: "bluewater", key: "race", url: "race.bluewatertracks.com/race" };
  await openListing({ ...event, positions: false, boats: [] }, "bluewater");
  expect(q('[data-feature="tracker-import:boats"]')).toBeNull();
  expect(q("[role=dialog]")!.textContent).toContain("Click Import tracks to download them");
  expect(calls.filter(([n]) => n === "trackerEvent")).toHaveLength(1);
  responses.trackerEvent = event;
  await click(q(".modal-actions button.primary"));
  expect(q('[data-feature="tracker-import:boats"]')).not.toBeNull();
  expect(calls.filter(([n]) => n === "importTrackerBoats")).toHaveLength(0);
  await click(q('tbody input'));
  await click(q(".modal-actions button.primary"));
  expect(calls.filter(([n]) => n === "trackerEvent")).toHaveLength(2);
  expect(calls.find(([n]) => n === "importTrackerBoats")?.[1]).toEqual(["bluewater", "race", ["1"]]);
});

it("fetches the weather of the ticked tracks together", async () => {
  const other: SourceSummary = { ...SOURCE, id: 6, label: "Bravo", colour: "#4e79a7" };
  const third: SourceSummary = { ...SOURCE, id: 8, label: "Charlie", colour: "#59a14f" };
  responses.startEnvFetch = { tracks: [], failure: null, warning: null };
  await act(async () => root.render(<Tracks project={project([SOURCE, other, third])} onProject={() => undefined} />));
  const selected = q('[data-feature="tracks:fetch-weather-selected"]') as HTMLButtonElement;
  expect(selected.disabled).toBe(true);
  const boxes = host.querySelectorAll('[data-feature="tracks:select"]');
  await click(boxes[0]!);
  await click(boxes[2]!);
  expect(selected.disabled).toBe(false);
  expect(selected.textContent).toBe("Fetch weather for 2 selected tracks…");
  await click(selected);
  expect(q("[role=dialog]")).toBeNull();
  expect(calls.filter(([n]) => n === "envEstimate")).toEqual([]);
  expect(calls.find(([n]) => n === "startEnvFetch")?.[1]).toEqual([[5, 8], false]);
});

it("selects all imported tracks, fetches those not already fetching, then clears the ticks", async () => {
  const { setEnvJobs, resetEnvJobs } = await import("../jobs");
  const bravo: SourceSummary = { ...SOURCE, id: 6, label: "Bravo" };
  const charlie: SourceSummary = { ...SOURCE, id: 7, label: "Charlie" };
  responses.startEnvFetch = { tracks: [], failure: null, warning: null };
  setEnvJobs({ tracks: [{ source_id: 6, label: "Bravo", state: "fetching", fraction: 0.5 }], failure: null, warning: null });
  await act(async () => root.render(<Tracks project={project([SOURCE, bravo, charlie])} onProject={() => undefined} />));
  const boxes = () => [...host.querySelectorAll<HTMLInputElement>('[data-feature="tracks:select"]')];
  const selectAll = q('[data-feature="tracks:select-all"]') as HTMLButtonElement;
  await click(selectAll);
  expect(boxes().map(box => box.checked)).toEqual([true, true, true]);
  expect(selectAll.disabled).toBe(true);
  const button = q('[data-feature="tracks:fetch-weather-selected"]') as HTMLButtonElement;
  // Bravo is fetching: two of the three ticked tracks are asked for.
  expect(button.textContent).toBe("Fetch weather for 2 selected tracks…");
  await click(button);
  await settle();
  expect(calls.filter(([n]) => n === "envEstimate")).toEqual([]);
  expect(calls.find(([n]) => n === "startEnvFetch")?.[1]).toEqual([[5, 7], false]);
  expect(q("[role=dialog]")).toBeNull();
  expect(boxes().map((box) => box.checked)).toEqual([false, false, false]);
  expect(selectAll.disabled).toBe(false);
  expect(button.disabled).toBe(true);
  await act(async () => resetEnvJobs());
});

it("keeps the ticks on a failed start and allows retry without a modal", async () => {
  const bravo: SourceSummary = { ...SOURCE, id: 6, label: "Bravo" };
  responses.startEnvFetch = new Rejection(new Error("could not queue weather"));
  await act(async () => root.render(<Tracks project={project([SOURCE, bravo])} onProject={() => undefined} />));
  for (const box of host.querySelectorAll<HTMLInputElement>('[data-feature="tracks:select"]')) await click(box);
  const button = q('[data-feature="tracks:fetch-weather-selected"]') as HTMLButtonElement;
  await click(button);
  expect(q("[role=dialog]")).toBeNull();
  expect(button.disabled).toBe(false);
  expect([...host.querySelectorAll<HTMLInputElement>('[data-feature="tracks:select"]')].map((b) => b.checked)).toEqual([true, true]);
  responses.startEnvFetch = { tracks: [], failure: null, warning: null };
  await click(button);
  expect(calls.filter(([n]) => n === "startEnvFetch")).toHaveLength(2);
  expect([...host.querySelectorAll<HTMLInputElement>('[data-feature="tracks:select"]')].map((b) => b.checked)).toEqual([false, false]);
});

it("shows and takes the speed and wave filters in the display units, storing knots and metres (M17b)", async () => {
  const units = { speed: "kmh", wave_height: "ft", distance: "nm" } as const;
  await act(async () => root.render(<Tracks project={project([SOURCE])} onProject={() => undefined} units={units} />));
  await click(q('[data-feature="tracks:filters"]'));
  const type = async (feature: string, text: string) => {
    const input = q(`[data-feature="${feature}"]`) as HTMLInputElement;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => { input.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  };
  const min = q('[data-feature="tracks:min-bsp"]') as HTMLInputElement;
  // The stored 1 kn is 1.852 km/h, and the label names the unit.
  expect(min.value).toBe("1.852");
  expect(min.closest("label")!.textContent).toContain("BSP from (km/h)");
  // Leaving the box untouched sends nothing.
  await act(async () => { min.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  expect(calls.filter(([n]) => n === "setTrackFilters")).toEqual([]);
  // 18.52 km/h is 10 kn.
  await type("tracks:min-bsp", "18.52");
  const sent = calls.filter(([n]) => n === "setTrackFilters").at(-1)?.[1] as [number, { min_bsp: number }];
  expect(sent[1].min_bsp).toBeCloseTo(10, 12);
  // 10 ft is 3.048 m.
  await type("tracks:hs-min", "10");
  const waves = calls.filter(([n]) => n === "setTrackFilters").at(-1)?.[1] as [number, { hs_min: number }];
  expect(waves[1].hs_min).toBeCloseTo(3.048, 12);
  expect(q('[data-feature="tracks:hs-min"]')!.closest("label")!.textContent).toContain("(ft)");
});

it("offers the provided-or-derived choice of each quantity only where the track gives it, each its own command", async () => {
  const giving: SourceSummary = { ...SOURCE, track: { ...TRACK, supplied_heading: 120, supplied_speed: 0, supplied_wind: 40, prefer_heading: "derived", prefer_speed: "given" } };
  await act(async () => root.render(<Tracks project={project([giving])} onProject={() => undefined} />));
  await click(q('[data-feature="tracks:filters"]'));
  const heading = q('[data-feature="tracks:heading-source"]') as HTMLSelectElement;
  expect(heading.value).toBe("derived");
  expect(q('[data-feature="tracks:speed-source"]')).toBeNull();
  const wind = q('[data-feature="tracks:wind-source"]') as HTMLSelectElement;
  expect(wind.value).toBe("given");
  await act(async () => {
    heading.value = "given";
    heading.dispatchEvent(new Event("change", { bubbles: true }));
  });
  // The speed's choice is kept as it was.
  expect(calls.find(([n]) => n === "setTrackDerivation")?.[1]).toEqual([5, 10_800, "given", "given"]);
  await act(async () => {
    wind.value = "derived";
    wind.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(calls.find(([n]) => n === "setTrackWind")?.[1]).toEqual([5, true]);
  // A compass sector needs both bounds: the start alone is held, not sent
  // (Rust refuses half a sector); with its end, both go together.
  const typeIn = async (feature: string, value: string) => {
    const input = q(`[data-feature="${feature}"]`) as HTMLInputElement;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => { input.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
  };
  const sent = () => calls.filter(([n]) => n === "setTrackFilters").length;
  const before = sent();
  await typeIn("tracks:twd-from", "180");
  expect(sent()).toBe(before);
  expect((q('[data-feature="tracks:twd-from"]') as HTMLInputElement).value).toBe("180");
  await typeIn("tracks:twd-to", "200");
  expect(calls.filter(([n]) => n === "setTrackFilters").at(-1)?.[1]).toEqual([5, { ...TRACK.filters, twd_from: 180, twd_to: 200 }]);
});
