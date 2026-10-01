/**
 * The interface following the MCP service (spec.md 3.7): what each
 * `document://changed` does to the shell.
 */
import { describe, expect, it, vi } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import { TEST_BLEND } from "../testBlend";
import { applyDocumentChanged, onShowBoat, onShowStage, showBoat, showStage } from "./follow";

const project = (id: number, revision: number): ProjectSummary => ({
  id, name: "P", path: null, dirty: true, revision, boat_name: "", boat_notes: "",
  sources: [], can_undo: true, can_redo: false, undo_label: null, redo_label: null,
  use_corrected: true, stokes_drift: false, blend: TEST_BLEND,
});

function actions(currentId: number | null) {
  return { currentId, enter: vi.fn(), update: vi.fn() };
}

describe("applyDocumentChanged", () => {
  it("an edit replaces the summary and nothing else", () => {
    const a = actions(1);
    const next = project(1, 8);
    applyDocumentChanged({ project: next, opened: false }, a);
    expect(a.update).toHaveBeenCalledWith(next);
    expect(a.enter).not.toHaveBeenCalled();
  });

  it("a newly opened project is entered, as the application's own opening enters it", () => {
    const a = actions(1);
    const next = project(1, 0);
    applyDocumentChanged({ project: next, opened: true }, a);
    expect(a.enter).toHaveBeenCalledWith(next);
    expect(a.update).not.toHaveBeenCalled();
  });

  it("a closed project shows the start screen", () => {
    const a = actions(1);
    applyDocumentChanged({ project: null, opened: true }, a);
    expect(a.enter).toHaveBeenCalledWith(null);
  });

  it("a project the shell does not hold is entered even when the service did not say it opened", () => {
    // The first boat was removed, so the file's root is another boat; or
    // the frontend was reloaded and holds nothing yet.
    for (const held of [1, null]) {
      const a = actions(held);
      const next = project(2, 3);
      applyDocumentChanged({ project: next, opened: false }, a);
      expect(a.enter).toHaveBeenCalledWith(next);
      expect(a.update).not.toHaveBeenCalled();
    }
  });
});

describe("showBoat", () => {
  it("tells whoever shows the boat tabs, until they stop listening", () => {
    const seen: number[] = [];
    const off = onShowBoat((id) => seen.push(id));
    showBoat(7);
    off();
    showBoat(8);
    expect(seen).toEqual([7]);
  });
});

describe("showStage", () => {
  it("sets the named boat's stage and no other's, whichever is on show", () => {
    const first: string[] = [], second: string[] = [];
    const offs = [onShowStage(1, (stage) => first.push(stage)), onShowStage(2, (stage) => second.push(stage))];
    showStage(2, "compare");
    expect(second).toEqual(["compare"]);
    expect(first).toEqual([]);
    for (const off of offs) off();
    showStage(2, "map");
    expect(second).toEqual(["compare"]);
    // What nobody took is not kept for a later boat of another id.
    const third: string[] = [];
    onShowStage(3, (stage) => third.push(stage))();
    expect(third).toEqual([]);
  });

  it("keeps a stage for a boat whose view is not mounted yet, and hands it over once", () => {
    // `view_stage` for a boat whose tab was never opened: the tab is shown,
    // its workspace mounts, and only then is there a stage to set.
    showStage(9, "plot");
    const seen: string[] = [];
    const off = onShowStage(9, (stage) => seen.push(stage));
    expect(seen).toEqual(["plot"]);
    off();
    onShowStage(9, (stage) => seen.push(stage))();
    expect(seen).toEqual(["plot"]);
  });
});
