// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TEST_BLEND } from "../testBlend";
const api = vi.hoisted(() => ({ openTrackerProject: vi.fn(), boatImportStatus: vi.fn(), cancelBoatImport: vi.fn(), confirmTrackerProject: vi.fn(), discardTrackerProject: vi.fn() }));
vi.mock("../ipc", () => ({ api }));
const { default: TrackerProjectDialog } = await import("./TrackerProjectDialog");
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;
const opened = vi.fn();
const closed = vi.fn();
beforeEach(async () => {
  vi.resetAllMocks();
  api.boatImportStatus.mockResolvedValue(null);
  api.openTrackerProject.mockResolvedValue({ project: { id: 1, name: "Race", boat_name: "Alpha", boat_notes: "", revision: 1, path: null, dirty: true, sources: [], can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false, blend: TEST_BLEND },
    boats: [{ boat: "Alpha", model: "j109", polars: 2, tracks: 1, missing_tracks: 0, warnings: [],
      details: { model: "J/109", mmsi: "123456789", builder: "J Boats", loa: "10.9 m", sailNumber: "GBR1234", CustomField: "Original value", empty: "" } }], warnings: [] });
  api.confirmTrackerProject.mockResolvedValue({ id: 1, name: "Race" });
  api.discardTrackerProject.mockResolvedValue(undefined);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<TrackerProjectDialog discardUnsaved onOpened={opened} onClose={closed} />));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
async function showBoats() {
  const url = host.querySelector<HTMLInputElement>('[data-feature="boats:tracker-url"]')!;
  await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(url, "https://boats.invalid/race"); url.dispatchEvent(new Event("input", { bubbles: true })); });
  await act(async () => host.querySelector<HTMLButtonElement>('[data-feature="boats:tracker-open"]')!.click());
}
it("passes the exact-boat choice and displays original boat details in the report", async () => {
  // Geovoile beside the other two (asked 2026-10-03).
  expect([...host.querySelectorAll<HTMLOptionElement>('[data-feature="boats:tracker-provider"] option')].map(o => o.value))
    .toEqual(["yellowbrick", "geovoile", "bluewater"]);
  const mode = host.querySelector<HTMLSelectElement>('[data-feature="boats:tracker-match"]')!;
  expect(mode.value).toBe("identical_model");
  await act(async () => { mode.value = "exact_boat"; mode.dispatchEvent(new Event("change", { bubbles: true })); });
  await showBoats();
  expect(api.openTrackerProject).toHaveBeenCalledWith("yellowbrick", "https://boats.invalid/race", true, "exact_boat");
  const report = host.querySelector('.boat-import-report')!.textContent;
  for (const value of ["J/109", "123456789", "J Boats", "10.9 m", "GBR1234", "CustomField", "Original value"]) expect(report).toContain(value);
  expect(host.querySelectorAll('details .tracker-boat-details dt')).toHaveLength(6);
  expect(opened).not.toHaveBeenCalled();
  await act(async () => host.querySelector<HTMLButtonElement>('[data-feature="boats:tracker-close"]')!.click());
  expect(api.confirmTrackerProject).toHaveBeenCalledWith(1);
  expect(opened).toHaveBeenCalledWith(expect.objectContaining({ id: 1 }));
  expect(closed).toHaveBeenCalledOnce();
});

it.each(["button", "Escape"])("cancels the boat list with %s without opening its project", async method => {
  await showBoats();
  await act(async () => {
    if (method === "button") host.querySelector<HTMLButtonElement>('[data-feature="boats:tracker-cancel"]')!.click();
    else host.querySelector('[role="dialog"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  expect(api.discardTrackerProject).toHaveBeenCalledWith(1);
  expect(api.confirmTrackerProject).not.toHaveBeenCalled();
  expect(opened).not.toHaveBeenCalled();
  expect(closed).toHaveBeenCalledOnce();
});

it("keeps the boat list open when confirmation fails, so it can still be cancelled", async () => {
  await showBoats();
  api.confirmTrackerProject.mockRejectedValue(new Error("Project changed"));
  await act(async () => host.querySelector<HTMLButtonElement>('[data-feature="boats:tracker-close"]')!.click());
  expect(opened).not.toHaveBeenCalled();
  expect(closed).not.toHaveBeenCalled();
  expect(host.querySelector('[role="alert"]')).not.toBeNull();
  await act(async () => host.querySelector<HTMLButtonElement>('[data-feature="boats:tracker-cancel"]')!.click());
  expect(api.discardTrackerProject).toHaveBeenCalledWith(1);
  expect(closed).toHaveBeenCalledOnce();
});
