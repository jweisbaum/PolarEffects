import { describe, expect, it } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import { TEST_BLEND } from "../testBlend";
import { CLASS_A_ONLY, CLASS_B_ONLY, CLASS_BOTH, CLASS_NEITHER, CLASS_ZERO_ROW, emptyCompare, type ComparePacket } from "./comparePacket";
import {
  compareSurfaces, DEFAULT_COMPARE_TOGGLES, differenceSurface, gridOf, heatRows, operandInfo, operandKey,
  parseOperandKey, regionRows, spanText,
} from "./compareModel";
import { hex, POLES } from "./diverging";

const N = Number.NaN;

/**
 * Rows 0°, 60°, 120° × columns 8, 16 kn: at 60° both (Δ +1 and −0.5), at
 * 120° A only then neither.
 */
function packet(): ComparePacket {
  return {
    ...emptyCompare(),
    twa: Float32Array.from([0, 60, 120]),
    tws: Float32Array.from([8, 16]),
    a: Float32Array.from([0, 0, 7, 8, 6, N]),
    b: Float32Array.from([0, 0, 6, 8.5, N, N]),
    deltaKn: Float32Array.from([N, N, 1, -0.5, N, N]),
    deltaPct: Float32Array.from([N, N, 100 / 6, -100 / 17, N, N]),
    cls: Uint32Array.from([CLASS_ZERO_ROW, CLASS_ZERO_ROW, CLASS_BOTH, CLASS_BOTH, CLASS_A_ONLY, CLASS_NEITHER]),
    overlap: 2, aOnly: 1, bOnly: 0,
    kn: { meanAbs: 0.75, maxAbs: 1, min: -0.5, max: 1, maxCell: 2 },
    pct: { meanAbs: 11.3, maxAbs: 100 / 6, min: -100 / 17, max: 100 / 6, maxCell: 2 },
    regions: [
      { tws: 0, firstTwa: 1, lastTwa: 1, faster: "a" },
      { tws: 1, firstTwa: 1, lastTwa: 1, faster: "b" },
    ],
  };
}

describe("the Compare stage's model", () => {
  it("reads a packet array as the scene's grid, bsp[j][i]", () => {
    expect(gridOf(packet(), packet().a).bsp).toEqual([[0, 7, 6], [0, 8, null]]);
  });

  it("draws the difference midway where both have a value, on the scale, and hatched grey where one has", () => {
    const surface = differenceSurface(packet(), false, "dark");
    // Node (i, j) is vertex j * ni + i; the 0° row is left out.
    expect(surface.grid.bsp).toEqual([[null, 6.5, 6], [null, 8.25, null]]);
    expect([...surface.hatched!]).toEqual([0, 0, 1, 0, 0, 0]);
    // 60°/8 kn is +1, the scale's end: A's pole (in linear light).
    const linear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    const a = POLES.dark.a;
    const expected = [1, 3, 5].map((k) => linear(Number.parseInt(a.slice(k, k + 2), 16) / 255));
    expect([...surface.vertexColors!.slice(3, 6)].map((v) => v.toFixed(4))).toEqual(expected.map((v) => v.toFixed(4)));
    expect(surface.opaque).toBe(true);
  });

  it("draws what the toggles leave on, in the operands' colours", () => {
    const all = compareSurfaces(packet(), DEFAULT_COMPARE_TOGGLES, { a: "#111111", b: "#222222" }, false, "light");
    expect(all.map((s) => s.color)).toEqual(["#111111", "#222222", POLES.light.mid]);
    expect(compareSurfaces(packet(), { a: false, b: true, delta: false }, { a: "#111111", b: "#222222" }, false, "light"))
      .toHaveLength(1);
  });

  it("groups the regions by wind speed and names their spans", () => {
    expect(regionRows(packet())).toEqual([
      { tws: 8, a: [[60, 60]], b: [] },
      { tws: 16, a: [], b: [[60, 60]] },
    ]);
    expect(spanText([52, 90])).toBe("52°–90°");
    expect(spanText([60, 60])).toBe("60°");
  });

  it("leaves the 0° row out of the heat map and fills only compared cells", () => {
    const rows = heatRows(packet(), false, "light");
    expect(rows.map((r) => r.twa)).toEqual([60, 120]);
    expect(rows[0]!.cells[0]!.fill).toBe(hex([...[1, 3, 5].map((k) => Number.parseInt(POLES.light.a.slice(k, k + 2), 16) / 255)] as [number, number, number]));
    expect(rows[1]!.cells.map((c) => [c.cls, c.fill])).toEqual([[CLASS_A_ONLY, null], [CLASS_NEITHER, null]]);
    expect(Number.isNaN(rows[1]!.cells[0]!.delta)).toBe(true);
    // In percent, the scale is the percentages'.
    expect(heatRows(packet(), true, "light")[0]!.cells[0]!.fill).toBe(rows[0]!.cells[0]!.fill);
    expect(CLASS_B_ONLY).toBe(2);
  });

  it("names operands by the source's colour and label, and the blend by its entry", () => {
    const project = {
      id: 1, blend: { ...TEST_BLEND, colour: "#e0457b" },
      sources: [{ id: 5, label: "Race", colour: "#4e79a7", kind: "track", visible: false }],
    } as unknown as ProjectSummary;
    expect(operandInfo(project, { kind: "blend" })).toEqual({ colour: "#e0457b", label: null, kind: "blend", hidden: false });
    expect(operandInfo(project, { kind: "segment", source_id: 5 })).toEqual({ colour: "#4e79a7", label: "Race", kind: "track", hidden: true });
    for (const operand of [{ kind: "blend" }, { kind: "segment", source_id: 5 }, { kind: "polar", source_id: 7 }] as const) {
      expect(parseOperandKey(operandKey(operand))).toEqual(operand);
    }
  });
});
