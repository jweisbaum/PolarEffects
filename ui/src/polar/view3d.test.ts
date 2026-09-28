import { describe, expect, it } from "vitest";

import { place } from "./geometry3d";
import { BLEND_SOURCE, FLAG_EXCLUDED, FLAG_FILTERED, type ScenePacket } from "./scenePacket";
import { SHAPE_CROSS, SHAPE_DISC, SHAPE_RING } from "./scene3d";
import {
  availableModes, buildDots, buildGuides, buildSurfaces, combine, DEFAULT_TOGGLES, drawnOnly, exclusionTargets, hasFiltered, keysOf,
  presetView, ramp, resolveKeys, sampleIdsOf, sceneBounds, summarise, surfaceGrid, ticks,
} from "./view3d";

/**
 * Two polar sources (ids 10 and 20) and a track (id 30): three nodes, the
 * second excluded, and two samples, the first excluded and filtered, with
 * Hs on the second only.
 */
function packet(): ScenePacket {
  return {
    timeOrigin: 0,
    sources: [
      { id: 10, colour: "#ff0000", kind: "orc" },
      { id: 20, colour: "#00ff00", kind: "polar_file" },
      { id: 30, colour: "#0000ff", kind: "track" },
    ],
    nodes: {
      count: 3,
      points: Float32Array.from([52, 6, 6, 90, 12, 8, 45, 10, 5]),
      source: Uint32Array.from([0, 0, 1]),
      cell: Uint32Array.from([1, 2 | (1 << 16), 0]),
      flags: Uint32Array.from([0, FLAG_EXCLUDED, 0]),
    },
    samples: {
      count: 2,
      points: Float32Array.from([120, 14, 9, 150, 16, 10]),
      source: Uint32Array.from([2, 2]),
      ids: Uint32Array.from([5, 0, 6, 1]),
      hs: Float32Array.from([Number.NaN, 2]),
      current: Float32Array.from([Number.NaN, Number.NaN]),
      time: Float32Array.from([0, 600]),
      flags: Uint32Array.from([FLAG_EXCLUDED | FLAG_FILTERED, 0]),
    },
    surfaces: [
      { source: 0, twa: Float32Array.from([52, 90]), tws: Float32Array.from([6, 12]), bsp: Float32Array.from([6, 7, Number.NaN, 8]) },
    ],
  };
}

describe("the dots drawn (spec.md 10.2, 10.3)", () => {
  it("draws nodes then unfiltered samples by default, crosses for excluded nodes", () => {
    const dots = buildDots(packet(), DEFAULT_TOGGLES, "source");
    // The filtered sample is hidden until "show filtered".
    expect([...dots.refs]).toEqual([0, 1, 2, 4]);
    expect([...dots.shapes]).toEqual([SHAPE_DISC, SHAPE_CROSS, SHAPE_DISC, SHAPE_DISC]);
    expect([...dots.colors.subarray(0, 3)]).toEqual([1, 0, 0]);
    expect([...dots.colors.subarray(9, 12)]).toEqual([0, 0, 1]);
    expect([...dots.points.subarray(9, 12)]).toEqual([150, 16, 10]);
  });

  it("shows filtered samples dimmed and excluded samples hollow when asked", () => {
    const dots = buildDots(packet(), { ...DEFAULT_TOGGLES, filtered: true, nodes: false }, "source");
    expect([...dots.refs]).toEqual([3, 4]);
    expect(dots.shapes[0]).toBe(SHAPE_RING);
    // Blue dimmed toward grey: 0.35 × 1 + 0.65 × 0.5.
    expect(dots.colors[2]).toBeCloseTo(0.675, 5);
    expect(dots.colors[0]).toBeCloseTo(0.325, 5);
  });

  it("hides what its toggle turns off", () => {
    expect(buildDots(packet(), { ...DEFAULT_TOGGLES, samples: false }, "source").refs.length).toBe(3);
    expect(buildDots(packet(), { ...DEFAULT_TOGGLES, samples: false, nodes: false }, "source").refs.length).toBe(0);
  });

  it("colours samples along the ramp by a value, grey where it is missing, nodes by source", () => {
    const dots = buildDots(packet(), { ...DEFAULT_TOGGLES, filtered: true }, "hs");
    // Sample 1 (Hs 2) is the only value: the middle of the ramp.
    expect([...dots.colors.subarray(12, 15)].map((v) => Number(v.toFixed(3)))).toEqual(ramp(0.5).map((v) => Number(v.toFixed(3))));
    // Sample 0 has no Hs: grey, dimmed because it is filtered (grey stays grey).
    expect(dots.colors[9]).toBeCloseTo(0.35 * 0.55 + 0.65 * 0.5, 4);
    expect([...dots.colors.subarray(0, 3)]).toEqual([1, 0, 0]);
  });

  it("offers a colour mode only when some sample has its value", () => {
    expect(availableModes(packet())).toEqual({ source: true, hs: true, current: false, time: true });
    const none = packet();
    none.samples = { ...none.samples, count: 0, hs: new Float32Array(0), time: new Float32Array(0), flags: new Uint32Array(0) };
    expect(availableModes(none)).toEqual({ source: true, hs: false, current: false, time: false });
    expect(hasFiltered(packet())).toBe(true);
    expect(hasFiltered(none)).toBe(false);
  });
});

describe("surfaces", () => {
  it("turns a TWA-major grid into the scene's TWS-major one, with holes", () => {
    const grid = surfaceGrid(Float32Array.from([52, 90]), Float32Array.from([6, 12]), Float32Array.from([6, 7, Number.NaN, 8]));
    expect(grid).toEqual({ twa: [52, 90], tws: [6, 12], bsp: [[6, null], [7, 8]] });
  });

  it("colours each surface as its source and the blend opaque", () => {
    const p = packet();
    p.surfaces.push({ source: BLEND_SOURCE, twa: Float32Array.from([45]), tws: Float32Array.from([10]), bsp: Float32Array.from([5]) });
    const surfaces = buildSurfaces(p, "#ffffff");
    expect(surfaces.map((s) => [s.color, s.opaque])).toEqual([["#ff0000", false], ["#ffffff", true]]);
  });
});

describe("the selection (spec.md 10.3)", () => {
  it("counts, averages and breaks down by source", () => {
    const summary = summarise(packet(), [0, 1, 4]);
    expect(summary.count).toBe(3);
    expect(summary.meanTwa).toBeCloseTo((52 + 90 + 150) / 3, 5);
    expect(summary.meanTws).toBeCloseTo((6 + 12 + 16) / 3, 5);
    expect(summary.meanBsp).toBeCloseTo((6 + 8 + 10) / 3, 5);
    expect(summary.bySource).toEqual([{ sourceId: 10, count: 2 }, { sourceId: 30, count: 1 }]);
    expect(summary.excluded).toBe(1);
    expect(summary.included).toBe(2);
    expect(summary.samples).toBe(1);
    expect(summarise(packet(), []).count).toBe(0);
  });

  it("names nodes by source and grid place, samples by id", () => {
    expect(exclusionTargets(packet(), [1, 2, 4])).toEqual({
      nodes: [{ source_id: 10, twa_index: 2, tws_index: 1 }, { source_id: 20, twa_index: 0, tws_index: 0 }],
      samples: [2 ** 32 + 6],
    });
  });

  it("replaces on a plain pick, adds or toggles with Shift", () => {
    expect(combine([1, 2], [3], false)).toEqual([3]);
    expect(combine([1, 2], [3, 0], true)).toEqual([0, 1, 2, 3]);
    expect(combine([1, 2], [2], true)).toEqual([1]);
    expect(combine([1, 2], [2, 3], true)).toEqual([1, 2, 3]);
  });

  it("survives a refetch that reorders the dots", () => {
    const before = packet();
    const keys = keysOf(before, [1, 4]);
    expect([...keys.nodes.get(10)!]).toEqual([2 | (1 << 16)]);
    expect([...keys.samples]).toEqual([2 ** 32 + 6]);
    const after = packet();
    // The first node is gone (its source hidden): everything shifts down one.
    after.nodes = {
      count: 2, points: after.nodes.points.slice(3), source: after.nodes.source.slice(1),
      cell: after.nodes.cell.slice(1), flags: after.nodes.flags.slice(1),
    };
    expect(resolveKeys(after, keys)).toEqual([0, 3]);
  });
});

describe("what the selection acts on", () => {
  it("leaves out dots the toggles hide", () => {
    // Sample 3 (global) is filtered and hidden by default; node 0 is drawn.
    const dots = buildDots(packet(), DEFAULT_TOGGLES, "source");
    expect(drawnOnly([0, 3, 4], dots.refs, 5)).toEqual([0, 4]);
    const noSamples = buildDots(packet(), { ...DEFAULT_TOGGLES, samples: false }, "source");
    expect(drawnOnly([0, 3, 4], noSamples.refs, 5)).toEqual([0]);
    expect(drawnOnly([], dots.refs, 5)).toEqual([]);
  });

  it("names the samples among the selected dots", () => {
    expect(sampleIdsOf(packet(), [0, 3, 4])).toEqual([5, 2 ** 32 + 6]);
  });

  it("re-finds 200,000 selected samples quickly", () => {
    const count = 200_000;
    const big: ScenePacket = {
      ...packet(),
      nodes: { count: 0, points: new Float32Array(), source: new Uint32Array(), cell: new Uint32Array(), flags: new Uint32Array() },
      samples: {
        count, points: new Float32Array(count * 3), source: new Uint32Array(count),
        ids: Uint32Array.from({ length: count * 2 }, (_, i) => (i % 2 === 0 ? i / 2 : 0)),
        hs: new Float32Array(count), current: new Float32Array(count), time: new Float32Array(count), flags: new Uint32Array(count),
      },
    };
    const all = Array.from({ length: count }, (_, k) => k);
    const started = performance.now();
    const keys = keysOf(big, all);
    const again = resolveKeys(big, keys);
    const elapsed = performance.now() - started;
    expect(again.length).toBe(count);
    // Measured 48 ms on the development machine with every sample
    // selected (188 ms with the string keys this replaced). The bound is a
    // guard against a gross regression on a slow machine, not a benchmark.
    expect(elapsed).toBeLessThan(400);
  });
});

describe("cameras and guides (spec.md 10.1)", () => {
  it("frames the whole scene from above, the side and three-quarters", () => {
    const bounds = sceneBounds(packet(), "tower");
    const top = presetView("top", bounds);
    expect(top.position[2]).toBeGreaterThan(bounds.max[2]);
    expect(top.position[0]).toBeCloseTo(top.target[0], 6);
    const side = presetView("side", bounds);
    expect(side.position[2]).toBeCloseTo(side.target[2], 6);
    expect(side.position[0]).toBeGreaterThan(bounds.max[0]);
    const iso = presetView("iso", bounds);
    const distance = Math.hypot(...iso.position.map((v, a) => v - iso.target[a]!));
    const radius = Math.hypot(...bounds.max.map((v, a) => v - bounds.min[a]!)) / 2;
    expect(distance).toBeGreaterThan(radius / Math.tan((20 * Math.PI) / 180));
  });

  it("bounds every dot, and the origin", () => {
    const bounds = sceneBounds(packet(), "cartesian");
    expect(bounds.min).toEqual([0, 0, 0]);
    expect(bounds.max).toEqual(place(150, 16, 10, "cartesian"));
  });

  it("chooses round ticks", () => {
    expect(ticks(10, 5)).toEqual([0, 2, 4, 6, 8, 10]);
    expect(ticks(23, 4)).toEqual([0, 10, 20]);
    expect(ticks(0)).toEqual([0]);
  });

  it("labels the axes in the display unit", () => {
    const bounds = sceneBounds(packet(), "tower");
    const knots = buildGuides(bounds, "tower", "kn").labels.map((l) => l.text);
    expect(knots).toContain("10 kn");
    expect(knots).toContain("90°");
    const metres = buildGuides(bounds, "tower", "ms").labels.map((l) => l.text);
    // 10 kn is 5.1 m/s.
    expect(metres).toContain("5.1 m/s");
    const cartesian = buildGuides(sceneBounds(packet(), "cartesian"), "cartesian", "kmh");
    expect(cartesian.segments.length).toBe(3 * 6);
    expect(cartesian.labels.map((l) => l.text)).toContain("180°");
  });
});
