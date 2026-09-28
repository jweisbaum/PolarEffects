// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";

import { loadPanels, normalise, OPEN, reveal, savePanels, togglePanel } from "./layout";

describe("panel layout", () => {
  beforeEach(() => localStorage.clear());

  it("opens everything the first time", () => {
    expect(loadPanels()).toEqual(OPEN);
  });

  it("remembers what was folded, across a reload", () => {
    const folded = togglePanel(togglePanel(OPEN, "left"), "tracks");
    savePanels(folded);
    expect(loadPanels()).toEqual({ ...OPEN, left: false, tracks: false });
  });

  it("repairs a damaged or partial record rather than failing", () => {
    localStorage.setItem("pe.panels", "{not json");
    expect(loadPanels()).toEqual(OPEN);
    expect(normalise({ left: false, orc: "no" as unknown as boolean })).toEqual({ ...OPEN, left: false });
  });

  it("reveals a section together with the dock it sits in", () => {
    const hidden = { ...OPEN, left: false, tracks: false, right: false, plot: false };
    expect(reveal(hidden, "tracks")).toEqual({ ...hidden, left: true, tracks: true });
    expect(reveal(hidden, "plot")).toEqual({ ...hidden, right: true, plot: true });
    expect(reveal(hidden, "right")).toEqual({ ...hidden, right: true });
    expect(reveal(OPEN, "orc")).toBe(OPEN);
  });
});
