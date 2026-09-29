import { describe, expect, it } from "vitest";
import {
  hatchLines,
  dotPositions, inside, lassoSelect, place, project, surfaceMesh, syntheticGrid, syntheticSamples,
} from "./geometry3d";

describe("placing a polar point (spec.md 10.1)", () => {
  it("puts beam reach at x = BSP in the tower and TWS up", () => {
    const [x, y, z] = place(90, 12, 10, "tower");
    expect(x).toBeCloseTo(10, 12);
    expect(y).toBeCloseTo(0, 12);
    expect(z).toBe(12);
  });

  it("puts dead downwind at y = -BSP", () => {
    const [x, y] = place(180, 12, 8, "tower");
    expect(x).toBeCloseTo(0, 12);
    expect(y).toBeCloseTo(-8, 12);
  });

  it("uses TWA, TWS, BSP axes in the Cartesian layout", () => {
    expect(place(120, 14, 9, "cartesian")).toEqual([12, 14, 9]);
  });

  it("keeps sample order in the flat position array", () => {
    const samples = Float32Array.from([90, 10, 5, 180, 20, 7]);
    const p = dotPositions(samples, "tower");
    expect(p.length).toBe(6);
    expect(p[0]).toBeCloseTo(5, 5);
    expect(p[4]).toBeCloseTo(-7, 5);
    expect(p[5]).toBe(20);
  });
});

describe("the surface of a polar grid", () => {
  // 37 TWA × 14 TWS; TWA below 30° is empty, leaving 31 filled columns.
  const mesh = surfaceMesh(syntheticGrid(1), "tower");

  it("reads each node's value TWA-major and places its vertex TWS-major", () => {
    // bsp[i * nj + j]: (0°: 1 at 6 kn, 2 at 12 kn), (90°: 3, 4).
    const small = surfaceMesh({ twa: [0, 90], tws: [6, 12], bsp: Float32Array.from([1, 2, 3, 4]) }, "tower");
    const vertex = (k: number) => [...small.positions.slice(k * 3, k * 3 + 3)].map((v) => Math.round(v * 1e6) / 1e6);
    // Vertex j * ni + i.
    expect(vertex(0)).toEqual([0, 1, 6]);
    expect(vertex(1)).toEqual([3, 0, 6]);
    expect(vertex(2)).toEqual([0, 2, 12]);
    expect(vertex(3)).toEqual([4, 0, 12]);
    expect([...small.triangles]).toEqual([0, 1, 3, 0, 3, 2]);
  });

  it("draws a quad only where all four corners exist", () => {
    // 30 quads per row × 13 rows, two triangles each.
    expect(mesh.triangles.length).toBe(30 * 13 * 2 * 3);
  });

  it("draws grid lines along each TWS curve and each TWA ray", () => {
    // 30 segments on each of 14 curves + 13 segments on each of 31 rays.
    expect(mesh.lines.length / 2).toBe(30 * 14 + 13 * 31);
  });
});

describe("the hatch over cells only one operand covers (spec.md 11)", () => {
  it("crosses each drawn quad whose corners are all marked, and no other", () => {
    // 3 × 2 nodes, all with a value but (2, 1): two quads could be drawn,
    // only the first is whole.
    // TWA-major: (30°: 5, 6), (60°: 6, 7), (90°: 7, empty).
    const grid = { twa: [30, 60, 90], tws: [8, 12], bsp: [5, 6, 6, 7, 7, Number.NaN] };
    expect([...hatchLines(grid, [1, 1, 0, 1, 1, 0])]).toEqual([0, 4, 1, 3]);
    expect([...hatchLines(grid, [0, 1, 0, 1, 1, 0])]).toEqual([]); // a mixed quad is not hatched
    expect([...hatchLines(grid, [0, 1, 1, 0, 1, 1])]).toEqual([]); // the second quad is not whole
    expect([...hatchLines(grid, new Uint8Array(6))]).toEqual([]);
  });
});

describe("projection and the lasso (spec.md 10.3)", () => {
  const identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

  it("maps clip space to pixels with y downward", () => {
    const s = project(Float32Array.from([0, 0, 0, 1, 1, 0, -1, -1, 0]), identity, 200, 100);
    expect([...s]).toEqual([100, 50, 200, 0, 0, 100]);
  });

  it("drops points outside the depth range", () => {
    const s = project(Float32Array.from([0, 0, 2]), identity, 200, 100);
    expect(Number.isNaN(s[0])).toBe(true);
    expect(lassoSelect(s, [0, 0, 200, 0, 200, 100, 0, 100])).toEqual(new Uint32Array(0));
  });

  it("tests points against a concave polygon", () => {
    // An L shape: the notch at the top right is outside.
    const l = [0, 0, 10, 0, 10, 5, 5, 5, 5, 10, 0, 10];
    expect(inside(2, 2, l)).toBe(true);
    expect(inside(7, 2, l)).toBe(true);
    expect(inside(2, 7, l)).toBe(true);
    expect(inside(7, 7, l)).toBe(false);
  });

  it("selects exactly the points inside, by index", () => {
    const screen = Float32Array.from([1, 1, 7, 7, 2, 8, 50, 50, NaN, NaN]);
    const l = [0, 0, 10, 0, 10, 5, 5, 5, 5, 10, 0, 10];
    expect([...lassoSelect(screen, l)]).toEqual([0, 2]);
    expect(lassoSelect(screen, [0, 0, 1, 1])).toEqual(new Uint32Array(0));
  });

  it("selects a fixed share of 200,000 synthetic dots repeatably", () => {
    const samples = syntheticSamples(200_000, 7);
    const positions = dotPositions(samples, "cartesian");
    // Orthographic top view: x = TWA/10 in 3..18, y = TWS in 4..30 → scale to 0..1.
    const mvp = [2 / 20, 0, 0, 0, 0, 2 / 30, 0, 0, 0, 0, -0.01, 0, -1, -1, 0, 1];
    const screen = project(positions, mvp, 1000, 1000);
    // Everything with TWA in 90..120 and TWS in 10..16 (a rectangle).
    const box = [450, 1000 - 10 * (1000 / 30), 600, 1000 - 10 * (1000 / 30), 600, 1000 - 16 * (1000 / 30), 450, 1000 - 16 * (1000 / 30)];
    const hits = lassoSelect(screen, box);
    let expected = 0;
    for (let i = 0; i < 200_000; i++) {
      const twa = samples[i * 3]!, tws = samples[i * 3 + 1]!;
      if (twa > 90 && twa < 120 && tws > 10 && tws < 16) expected++;
    }
    expect(Math.abs(hits.length - expected)).toBeLessThanOrEqual(5);
  });
});
