// @vitest-environment happy-dom
/**
 * The ORC section of Settings (spec.md 5.4): a scrape is started and
 * cancelled by hand, its progress is the service's own, and the schedule is
 * saved through its command. Opening Settings fetches nothing.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppSettings } from "../generated/AppSettings";
import type { OrcCatalogueInfo } from "../generated/OrcCatalogueInfo";
import type { OrcProgress } from "../generated/OrcProgress";
import type { ScrapeSchedule } from "../generated/ScrapeSchedule";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const held = vi.hoisted(() => {
  const idle: OrcProgress = { running: false, done: 0, total: 0, certificates: 0, added: 0, updated: 0, removed: 0, failed: 0, failures: [], cancelled: false, error: null };
  const info: OrcCatalogueInfo = {
    records: 18135, scraped: 0, scraped_at: null, source: "jieter/orc-data", commit: "c2ca870c", commit_date: "2026-09-28",
    build_date: "2026-09-28", countries: ["GBR"], year_min: 1900, year_max: 2026,
  };
  return {
    idle,
    status: idle,
    info,
    orcScrapeStatus: vi.fn(async () => held.status),
    orcCatalogueInfo: vi.fn(async () => held.info),
    startOrcScrape: vi.fn(async () => { held.status = { ...held.idle, running: true }; return held.status; }),
    cancelOrcScrape: vi.fn(async () => undefined),
    setCatalogueSchedule: vi.fn(async (catalogue: "orc" | "orr", schedule: ScrapeSchedule) =>
      ({ catalogues: { orc_schedule: "on_demand", orr_schedule: "on_demand", [`${catalogue}_schedule`]: schedule } }) as unknown as AppSettings),
  };
});

vi.mock("../ipc", () => ({
  api: {
    orcScrapeStatus: held.orcScrapeStatus,
    orcCatalogueInfo: held.orcCatalogueInfo,
    startOrcScrape: held.startOrcScrape,
    cancelOrcScrape: held.cancelOrcScrape,
    setCatalogueSchedule: held.setCatalogueSchedule,
  },
  IpcError: class extends Error {},
}));

import OrcScraper from "./OrcScraper";

const settings = { catalogues: { orc_schedule: "on_demand", orr_schedule: "startup" } } as unknown as AppSettings;
let root: Root;
let host: HTMLDivElement;
let onSettings: ReturnType<typeof vi.fn>;
const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });
const button = (feature: string) => host.querySelector<HTMLButtonElement>(`[data-feature="${feature}"]`)!;

beforeEach(() => {
  vi.useFakeTimers();
  held.status = held.idle;
  held.info = { ...held.info, scraped: 0, scraped_at: null };
  for (const mock of [held.orcScrapeStatus, held.orcCatalogueInfo, held.startOrcScrape, held.cancelOrcScrape, held.setCatalogueSchedule]) mock.mockClear();
  onSettings = vi.fn();
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => { act(() => root.unmount()); host.remove(); vi.useRealTimers(); });

describe("OrcScraper", () => {
  it("opens without fetching anything, and says what the catalogue holds", async () => {
    act(() => root.render(<OrcScraper settings={settings} onSettings={onSettings} />));
    await flush();
    expect(held.startOrcScrape).not.toHaveBeenCalled();
    expect(host.querySelector(".orc-scraper")!.textContent).toContain("18135 certificates in the catalogue");
    expect(host.querySelector(".orc-scraper")!.textContent).not.toContain("downloaded from ORC");
    expect(button("settings:orc-scrape").disabled).toBe(false);
    expect(button("settings:orc-cancel").disabled).toBe(true);

    held.info = { ...held.info, records: 19001, scraped: 4200, scraped_at: Date.UTC(2026, 9, 14, 10, 51) / 1000 };
    act(() => root.render(<OrcScraper key="again" settings={settings} onSettings={onSettings} />));
    await flush();
    expect(host.querySelector(".orc-scraper")!.textContent).toContain("4200 of them downloaded from ORC, last on 2026-10-14");
  });

  it("starts a scrape by hand, shows the countries done, and cancels", async () => {
    act(() => root.render(<OrcScraper settings={settings} onSettings={onSettings} />));
    await flush();
    await act(async () => { button("settings:orc-scrape").click(); });
    await flush();
    expect(held.startOrcScrape).toHaveBeenCalledTimes(1);
    expect(button("settings:orc-scrape").disabled).toBe(true);
    expect(host.querySelector('[role="status"]')!.textContent).toContain("Reading the list of countries");

    held.status = { ...held.idle, running: true, done: 3, total: 33, certificates: 2140 };
    await act(async () => { vi.advanceTimersByTime(1000); });
    await flush();
    expect(host.querySelector('[role="status"]')!.textContent).toContain("Countries downloaded: 3 of 33 (2140 certificates)");
    expect(host.querySelector("progress")!.value).toBe(3);

    await act(async () => { button("settings:orc-cancel").click(); });
    expect(held.cancelOrcScrape).toHaveBeenCalledTimes(1);
    held.status = { ...held.idle, cancelled: true, total: 33, done: 3 };
    await act(async () => { vi.advanceTimersByTime(1000); });
    await flush();
    expect(host.querySelector('[role="status"]')!.textContent).toContain("Download cancelled. The previous catalogue is unchanged.");
  });

  it("says what a finished scrape added and what it could not read", async () => {
    held.status = { ...held.idle, total: 33, done: 33, certificates: 17650, added: 420, updated: 31, removed: 3, failed: 2, failures: ["03440004L2V: no R90 list", "RUS: not answering"] };
    act(() => root.render(<OrcScraper settings={settings} onSettings={onSettings} />));
    await flush();
    const status = host.querySelector('[role="status"]')!;
    expect(status.textContent).toContain("Added 420, updated 31, removed 3 that ORC no longer lists, skipped 2.");
    expect(status.querySelector("details")!.textContent).toContain("RUS: not answering");
    expect(button("settings:orc-details")).not.toBeNull();
  });

  it("saves the schedule through its command and hands the settings on", async () => {
    act(() => root.render(<OrcScraper settings={settings} onSettings={onSettings} />));
    await flush();
    const select = host.querySelector<HTMLSelectElement>('[data-feature="settings:orc-schedule"]')!;
    expect(select.value).toBe("on_demand");
    expect([...select.options].map((option) => option.textContent)).toEqual(["Manually only", "On startup", "On shutdown"]);
    await act(async () => {
      select.value = "shutdown";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await flush();
    expect(held.setCatalogueSchedule).toHaveBeenCalledWith("orc", "shutdown");
    expect(onSettings).toHaveBeenCalledTimes(1);
    expect((onSettings.mock.calls[0]![0] as AppSettings).catalogues.orc_schedule).toBe("shutdown");
  });
});
