import { afterEach, describe, expect, it } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import {
  compareChoice, compareSource, DEFAULT_THRESHOLD_KN, onCompareSource, resetCompareChoices, setCompareChoice, swapOperands,
} from "./compareState";

function project(id: number, sources: { id: number; kind: string }[]): ProjectSummary {
  return { id, sources } as unknown as ProjectSummary;
}

afterEach(() => resetCompareChoices());

describe("the Compare stage's choices (view state)", () => {
  it("starts with the first source against the blend", () => {
    expect(compareChoice(project(1, [{ id: 3, kind: "orc" }]))).toEqual({
      a: { kind: "polar", source_id: 3 }, b: { kind: "blend" }, percent: false, thresholdKn: DEFAULT_THRESHOLD_KN,
    });
    expect(compareChoice(project(1, [])).a).toEqual({ kind: "blend" });
  });

  it("keeps each project's choice apart, and swaps", () => {
    const p = project(1, [{ id: 3, kind: "orc" }, { id: 4, kind: "track" }]);
    setCompareChoice(p, { b: { kind: "segment", source_id: 4 }, percent: true });
    swapOperands(p);
    expect(compareChoice(p)).toMatchObject({ a: { kind: "segment", source_id: 4 }, b: { kind: "polar", source_id: 3 }, percent: true });
    expect(compareChoice(project(2, [{ id: 3, kind: "orc" }])).percent).toBe(false);
  });

  it("falls back when an operand's source is removed", () => {
    const p = project(1, [{ id: 3, kind: "orc" }, { id: 4, kind: "track" }]);
    setCompareChoice(p, { a: { kind: "segment", source_id: 4 }, b: { kind: "polar", source_id: 3 } });
    expect(compareChoice(project(1, [{ id: 4, kind: "track" }])).b).toEqual({ kind: "blend" });
    expect(compareChoice(project(1, [{ id: 3, kind: "orc" }])).a).toEqual({ kind: "polar", source_id: 3 });
  });

  it("opens a source as A against the blend and tells the shell", () => {
    const p = project(1, [{ id: 3, kind: "orc" }, { id: 4, kind: "track" }]);
    setCompareChoice(p, { b: { kind: "polar", source_id: 3 } });
    let opened = 0;
    const off = onCompareSource(() => { opened += 1; });
    compareSource(p, 4);
    off();
    expect(opened).toBe(1);
    expect(compareChoice(p)).toMatchObject({ a: { kind: "segment", source_id: 4 }, b: { kind: "blend" } });
  });
});
