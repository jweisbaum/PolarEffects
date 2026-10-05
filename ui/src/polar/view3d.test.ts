import { describe, expect, it } from "vitest";

import { dayBandRgb } from "../dayBand";
import { place } from "./geometry3d";
import { NO_WAVE_RANGES } from "./waveRanges";
import { BLEND_SOURCE, FLAG_EDITED, FLAG_EXCLUDED, FLAG_FILTERED, type ScenePacket } from "./scenePacket";
import { SHAPE_CROSS, SHAPE_DISC, SHAPE_RING, SHAPE_SQUARE, type View } from "./scene3d";
import {
  availableModes, buildDots, buildGuides, buildSurfaces, combine, DEFAULT_TOGGLES, drawnOnly, editCells, exclusionTargets,
  FADED_OPACITY, focusIndex, hasFiltered, keysOf, nearestIndex, nodesAtCells, presetView, ramp, rescaleView, type Bounds, resolveKeys, sampleIdsOf, sceneBounds, summarise,
  hasExcluded, mergeDots, nodeDots, sampleDots, surfaceGrid, ticks, type Toggles,
} from "./view3d";

/**
 * Two polar sources (ids 10 and 20) and a track (id 30): three nodes, the
 * second excluded, and two samples, the first excluded and filtered, with
 * Hs on the second only.
 */
/** Every dot drawn, the excluded ones too: what they look like when shown. */
const WITH_EXCLUDED: Toggles = { ...DEFAULT_TOGGLES, excluded: true };

function packet(): ScenePacket {
  return {
    timeOrigin: 0,
    samplesKey: 0,
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
      wavePeriod: new Float32Array(0), waveAngle: new Float32Array(0), waveWindAngle: new Float32Array(0), waveBearing: new Float32Array(0),
      count: 2,
      points: Float32Array.from([120, 14, 9, 150, 16, 10]),
      source: Uint32Array.from([2, 2]),
      ids: Uint32Array.from([5, 0, 6, 1]),
      hs: Float32Array.from([Number.NaN, 2]),
      current: Float32Array.from([Number.NaN, Number.NaN]),
      time: Float32Array.from([0, 600]),
      // The first sample was sailed at night (band 0), the second in the
      // afternoon (band 2, bits 8–9).
      flags: Uint32Array.from([FLAG_EXCLUDED | FLAG_FILTERED, 2 << 8]),
    },
    surfaces: [
      { source: 0, twa: Float32Array.from([52, 90]), tws: Float32Array.from([6, 12]), bsp: Float32Array.from([6, 7, Number.NaN, 8]) },
    ],
  };
}

describe("the dots drawn (spec.md 10.2, 10.3)", () => {
  it("draws nodes then unfiltered samples by default, crosses for excluded nodes", () => {
    const dots = buildDots(packet(), WITH_EXCLUDED, "source");
    // The filtered sample is hidden until "show filtered".
    expect([...dots.refs]).toEqual([0, 1, 2, 4]);
    expect([...dots.shapes]).toEqual([SHAPE_DISC, SHAPE_CROSS, SHAPE_DISC, SHAPE_DISC]);
    expect([...dots.colors.subarray(0, 3)]).toEqual([1, 0, 0]);
    expect([...dots.colors.subarray(9, 12)]).toEqual([0, 0, 1]);
    expect([...dots.points.subarray(9, 12)]).toEqual([150, 16, 10]);
  });

  it("shows filtered samples dimmed and excluded samples hollow when asked", () => {
    const dots = buildDots(packet(), { ...WITH_EXCLUDED, filtered: true, nodes: false }, "source");
    expect([...dots.refs]).toEqual([3, 4]);
    expect(dots.shapes[0]).toBe(SHAPE_RING);
    // Blue dimmed toward grey: 0.35 × 1 + 0.65 × 0.5.
    expect(dots.colors[2]).toBeCloseTo(0.675, 5);
    expect(dots.colors[0]).toBeCloseTo(0.325, 5);
  });

  it("hides what its toggle turns off", () => {
    expect(buildDots(packet(), { ...WITH_EXCLUDED, samples: false }, "source").refs.length).toBe(3);
    expect(buildDots(packet(), { ...WITH_EXCLUDED, samples: false, nodes: false }, "source").refs.length).toBe(0);
  });

  it("colours samples along the ramp by a value, grey where it is missing, nodes by source", () => {
    const dots = buildDots(packet(), { ...WITH_EXCLUDED, filtered: true }, "hs");
    // Sample 1 (Hs 2) is the only value: the middle of the ramp.
    expect([...dots.colors.subarray(12, 15)].map((v) => Number(v.toFixed(3)))).toEqual(ramp(0.5).map((v) => Number(v.toFixed(3))));
    // Sample 0 has no Hs: grey, dimmed because it is filtered (grey stays grey).
    expect(dots.colors[9]).toBeCloseTo(0.35 * 0.55 + 0.65 * 0.5, 4);
    expect([...dots.colors.subarray(0, 3)]).toEqual([1, 0, 0]);
  });

  it("colours samples by their band of the local solar day, nodes by source (spec.md 10.2)", () => {
    const dots = buildDots(packet(), { ...WITH_EXCLUDED, filtered: true }, "timeOfDay");
    expect([...dots.refs]).toEqual([0, 1, 2, 3, 4]);
    // Sample 1: afternoon, undimmed.
    expect([...dots.colors.subarray(12, 15)]).toEqual([...Float32Array.from(dayBandRgb(2))]);
    // Sample 0: night, dimmed once because it is filtered.
    const night = dayBandRgb(0);
    expect(dots.colors[9]).toBeCloseTo(0.35 * night[0] + 0.65 * 0.5, 5);
    expect(dots.colors[11]).toBeCloseTo(0.35 * night[2] + 0.65 * 0.5, 5);
    // Nodes have no time: their source's colour.
    expect([...dots.colors.subarray(0, 3)]).toEqual([1, 0, 0]);
  });

  it("offers a colour mode only when some sample has its value", () => {
    expect(availableModes(packet())).toEqual({ source: true, hs: true, current: false, time: true, timeOfDay: true, wavePeriod: false, waveAngle: false, waveWindAngle: false });
    const none = packet();
    none.samples = { ...none.samples, count: 0, hs: new Float32Array(0), time: new Float32Array(0), flags: new Uint32Array(0) };
    expect(availableModes(none)).toEqual({ source: true, hs: false, current: false, time: false, timeOfDay: false, wavePeriod: false, waveAngle: false, waveWindAngle: false });
    expect(hasFiltered(packet())).toBe(true);
    expect(hasFiltered(none)).toBe(false);
  });
});

describe("the blend under the pointer (spec.md 10.1)", () => {
  it("lets only the blend's surface be picked", () => {
    const withBlend = packet();
    withBlend.surfaces = [...withBlend.surfaces,
      { source: BLEND_SOURCE, twa: Float32Array.from([45]), tws: Float32Array.from([10]), bsp: Float32Array.from([6]) }];
    expect(buildSurfaces(withBlend, "#e0457b").map((surface) => surface.pickable === true)).toEqual([false, true]);
  });

  it("finds the axis value nearest a value, the lower one on a tie", () => {
    const axis = [0, 30, 45, 52, 60];
    expect(nearestIndex(axis, 45)).toBe(2);
    expect(nearestIndex(axis, 50)).toBe(3);
    expect(nearestIndex(axis, 37.5)).toBe(1);
    expect(nearestIndex(axis, -20)).toBe(0);
    expect(nearestIndex(axis, 400)).toBe(4);
    expect(nearestIndex([], 10)).toBe(-1);
  });
});

describe("surfaces", () => {
  it("hands the scene the packet's TWA-major arrays as they are, holes as NaN", () => {
    const twa = Float32Array.from([52, 90]), tws = Float32Array.from([6, 12]);
    const bsp = Float32Array.from([6, 7, Number.NaN, 8]);
    const grid = surfaceGrid(twa, tws, bsp);
    expect(grid.bsp).toBe(bsp);
    expect(grid.twa).toBe(twa);
    expect(grid.tws).toBe(tws);
  });

  it("colours each surface as its source and the blend opaque", () => {
    const p = packet();
    p.surfaces.push({ source: BLEND_SOURCE, twa: Float32Array.from([45]), tws: Float32Array.from([10]), bsp: Float32Array.from([5]) });
    const surfaces = buildSurfaces(p, "#ffffff");
    expect(surfaces.map((s) => [s.color, s.opaque])).toEqual([["#ff0000", false], ["#ffffff", true]]);
    // A blend lost on the background keeps its grid lines as an outline.
    const outlined = buildSurfaces(p, "#ffffff", null, "#1f2c3c");
    expect(outlined.map((s) => s.lineColor)).toEqual([undefined, "#1f2c3c"]);
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
      wavePeriod: new Float32Array(0), waveAngle: new Float32Array(0), waveWindAngle: new Float32Array(0), waveBearing: new Float32Array(0),
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

  it("can look at the origin, far enough back that every corner of the scene is in the frame (asked 2026-10-03)", () => {
    const bounds = sceneBounds(packet(), "tower");
    for (const preset of ["top", "side", "iso"] as const) {
      const view = presetView(preset, bounds, 40, "origin");
      expect(view.target).toEqual([0, 0, 0]);
      const distance = Math.hypot(...view.position);
      // The farthest corner of the bounds from the origin fits in the half angle.
      const reach = Math.hypot(...[0, 1, 2].map((a) => Math.max(Math.abs(bounds.min[a]!), Math.abs(bounds.max[a]!))));
      expect(distance * Math.tan((20 * Math.PI) / 180)).toBeGreaterThanOrEqual(reach);
    }
    // By default the middle of the bounds, as the Compare stage keeps.
    expect(presetView("top", bounds).target).not.toEqual([0, 0, 0]);
  });

  it("follows a box that shrinks or grows, keeping the angle it looks from (asked 2026-10-04)", () => {
    const wide = { min: [-30, -30, 0], max: [30, 30, 35] } as Bounds;
    const narrow = { min: [-6, -6, 0], max: [6, 6, 10] } as Bounds;
    const view = { position: [40, -60, 80], target: [1, 2, 3] } as View;
    const moved = rescaleView(view, wide, narrow);
    const k = Math.hypot(6, 6, 10) / Math.hypot(30, 30, 35);
    moved.position.forEach((v, a) => expect(v).toBeCloseTo(view.position[a]! * k, 9));
    moved.target.forEach((v, a) => expect(v).toBeCloseTo(view.target[a]! * k, 9));
    // Back again is where it started; the same box leaves it alone.
    rescaleView(moved, narrow, wide).position.forEach((v, a) => expect(v).toBeCloseTo(view.position[a]!, 9));
    expect(rescaleView(view, wide, wide)).toBe(view);
  });

  it("hides excluded dots, samples and polar points alike, unless asked to show them (asked 2026-10-04)", () => {
    const p = packet();
    // The fixture's second node and first sample are excluded.
    const refs = (toggles: Toggles) => [...mergeDots(nodeDots(p, toggles), sampleDots(p, toggles, "source")).refs];
    expect(refs({ ...DEFAULT_TOGGLES, filtered: true })).toEqual([0, 2, 4]);
    expect(refs({ ...DEFAULT_TOGGLES, filtered: true, excluded: true })).toEqual([0, 1, 2, 3, 4]);
    expect(DEFAULT_TOGGLES.excluded).toBe(false);
    expect(hasExcluded(p)).toBe(true);
    const none = packet();
    none.nodes.flags = Uint32Array.from([0, 0, 0]);
    none.samples.flags = Uint32Array.from([0, 0]);
    expect(hasExcluded(none)).toBe(false);
  });

  it("bounds every dot, and the origin", () => {
    const bounds = sceneBounds(packet(), "cartesian");
    expect(bounds.min).toEqual([0, 0, 0]);
    expect(bounds.max).toEqual(place(150, 16, 10, "cartesian"));
  });

  it("leaves excluded, filtered-out and wave-range-hidden points out of the scales (asked 2026-10-04)", () => {
    // One outlier sample at 25 kn TWS and 30 kn BSP, well past everything else.
    const withOutlier = (flags: number, hs = Number.NaN): ScenePacket => {
      const p = packet();
      const s = p.samples;
      p.samples = {
        ...s, count: 3,
        points: Float32Array.from([...s.points, 100, 25, 30]),
        source: Uint32Array.from([...s.source, 2]),
        ids: Uint32Array.from([...s.ids, 7, 0]),
        hs: Float32Array.from([...s.hs, hs]),
        current: Float32Array.from([...s.current, Number.NaN]),
        time: Float32Array.from([...s.time, 1200]),
        flags: Uint32Array.from([...s.flags, flags]),
      };
      return p;
    };
    // Neither the excluded nor the filtered-out sample of the fixture, nor
    // its excluded node (90°, 12 kn, 8 kn), widens the box any more.
    const without = place(150, 16, 10, "cartesian");
    const outlier = place(100, 25, 30, "cartesian");
    const kept = without.map((v, a) => Math.max(v, outlier[a]!));
    expect(sceneBounds(withOutlier(0), "cartesian").max).toEqual(kept);
    expect(sceneBounds(withOutlier(FLAG_EXCLUDED), "cartesian").max).toEqual(without);
    expect(sceneBounds(withOutlier(FLAG_FILTERED), "cartesian").max).toEqual(without);
    // Shown, they count again: "Excluded" and "Filtered" ticked (asked 2026-10-04).
    const shown = { excluded: true, filtered: false };
    expect(sceneBounds(withOutlier(FLAG_EXCLUDED), "cartesian", NO_WAVE_RANGES, shown).max).toEqual(kept);
    expect(sceneBounds(withOutlier(FLAG_FILTERED), "cartesian", NO_WAVE_RANGES, shown).max).toEqual(without);
    expect(sceneBounds(withOutlier(FLAG_FILTERED), "cartesian", NO_WAVE_RANGES, { excluded: false, filtered: true }).max).toEqual(kept);
    // Hidden by a wave range: its 3 m waves are above the 2.5 m kept.
    const ranges = { ...NO_WAVE_RANGES, hs: { min: 0, max: 2.5 } };
    expect(sceneBounds(withOutlier(0, 3), "cartesian", ranges).max).toEqual(without);
    expect(sceneBounds(withOutlier(0, 2), "cartesian", ranges).max).toEqual(kept);
  });

  it("does not let the blend's zero row at 0° stretch the wind scale (asked 2026-10-04)", () => {
    const p = packet();
    // The output grid runs to 30 kn of wind, but only 0° (a speed of 0,
    // head to wind) has a value there.
    p.surfaces.push({
      source: BLEND_SOURCE, twa: Float32Array.from([0, 90]), tws: Float32Array.from([10, 30]),
      bsp: Float32Array.from([0, 0, 7, Number.NaN]),
    });
    expect(sceneBounds(p, "cartesian").max).toEqual(place(150, 16, 10, "cartesian"));
  });

  it("bounds the blend surface too, which has no nodes of its own", () => {
    const p = packet();
    p.surfaces.push({
      source: BLEND_SOURCE, twa: Float32Array.from([90, 170]), tws: Float32Array.from([30]),
      bsp: Float32Array.from([12, Number.NaN]),
    });
    expect(sceneBounds(p, "cartesian").max).toEqual(place(150, 30, 12, "cartesian"));
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
    const fullTower = buildGuides(bounds, "tower", "kn", true).labels.filter(l => l.text.endsWith("°"));
    expect(fullTower.map(l => l.text)).toEqual(["0°", "30°", "60°", "90°", "120°", "150°", "180°", "150°", "120°", "90°", "60°", "30°"]);
    expect(fullTower.filter(l => l.text === "90°").map(l => Math.sign(l.at[0]))).toEqual([1, -1]);
    const fullCartesian = buildGuides(bounds, "cartesian", "kn", true).labels.filter(l => l.text.endsWith("°"));
    expect(fullCartesian.at(-1)).toMatchObject({ text: "0°", at: [36, -0.8, 0] });
    expect(fullCartesian[6]!.text).toBe("180°");
  });
});

describe("edit mode (spec.md 10.4)", () => {
  /** The packet with node 0 edited and a segment node of the track (cell 3, 4). */
  function editing(): ScenePacket {
    const base = packet();
    return {
      ...base,
      nodes: {
        count: 4,
        points: Float32Array.from([52, 6, 6, 90, 12, 8, 45, 10, 5, 90, 12, 7]),
        source: Uint32Array.from([0, 0, 1, 2]),
        cell: Uint32Array.from([1, 2 | (1 << 16), 0, 3 | (4 << 16)]),
        flags: Uint32Array.from([FLAG_EDITED, FLAG_EXCLUDED | FLAG_EDITED, 0, FLAG_EDITED]),
      },
      surfaces: [
        ...base.surfaces,
        { source: 2, twa: Float32Array.from([90]), tws: Float32Array.from([12]), bsp: Float32Array.from([7]) },
      ],
    };
  }

  it("finds the edited source in the scene, or not when it is hidden", () => {
    expect(focusIndex(packet(), 20)).toBe(1);
    expect(focusIndex(packet(), 99)).toBe(-1);
    expect(focusIndex(packet(), null)).toBe(-1);
  });

  it("draws edited nodes as squares, an excluded one still as a cross", () => {
    const dots = buildDots(editing(), WITH_EXCLUDED, "source");
    expect([...dots.shapes.subarray(0, 4)]).toEqual([SHAPE_SQUARE, SHAPE_CROSS, SHAPE_DISC, SHAPE_SQUARE]);
  });

  it("dims the other sources' dots, or leaves them out", () => {
    const faded = buildDots(editing(), WITH_EXCLUDED, "source", { index: 0, hideOthers: false });
    expect([...faded.refs]).toEqual([0, 1, 2, 3, 5]);
    expect([...faded.colors.subarray(0, 3)]).toEqual([1, 0, 0]);
    // The polar file's node (drawn third) is dimmed twice toward grey.
    const dimmed = [...faded.colors.subarray(6, 9)];
    expect(dimmed[1]).toBeLessThan(0.6);
    expect(dimmed[0]).toBeGreaterThan(0.3);
    const hidden = buildDots(editing(), WITH_EXCLUDED, "source", { index: 2, hideOthers: true });
    expect([...hidden.refs]).toEqual([3, 5]);
  });

  it("makes the edited surface opaque and fades or hides the others", () => {
    const faded = buildSurfaces(editing(), "#ffffff", { index: 2, hideOthers: false });
    expect(faded.map((s) => [s.opaque, s.opacity])).toEqual([[false, FADED_OPACITY], [true, undefined]]);
    const hidden = buildSurfaces(editing(), "#ffffff", { index: 2, hideOthers: true });
    expect(hidden).toHaveLength(1);
    expect(hidden[0]!.color).toBe("#0000ff");
    // Not in edit mode, nothing changes.
    expect(buildSurfaces(editing(), "#ffffff").map((s) => s.opaque)).toEqual([false, false]);
  });

  it("names the edited source's selected cells, and finds its nodes by cell", () => {
    const scene = editing();
    expect(editCells(scene, [0, 1, 2, 5], 0)).toEqual([{ twa_index: 1, tws_index: 0 }, { twa_index: 2, tws_index: 1 }]);
    expect(editCells(scene, [3], 2)).toEqual([{ twa_index: 3, tws_index: 4 }]);
    expect(nodesAtCells(scene, 0, new Set([2 | (1 << 16)]))).toEqual([1]);
    expect(nodesAtCells(scene, 2, new Set([3 | (4 << 16), 1]))).toEqual([3]);
  });

  it("never offers a track's segment cells for exclusion", () => {
    const scene = editing();
    expect(exclusionTargets(scene, [3, 0])).toEqual({ nodes: [{ source_id: 10, twa_index: 1, tws_index: 0 }], samples: [] });
    const summary = summarise(scene, [3]);
    expect([summary.included, summary.excluded]).toEqual([0, 0]);
  });
});

it("composes temporary wave limits with analysis flags and keeps boundary samples and their IDs", () => {
  const scene = packet();
  scene.samples = {
    ...scene.samples, count: 6,
    points: Float32Array.from([10, 6, 1, 20, 6, 2, 30, 6, 3, 40, 6, 4, 50, 6, 5, 60, 6, 6]),
    source: Uint32Array.from([2, 2, 2, 2, 2, 2]), ids: Uint32Array.from([10, 0, 11, 0, 12, 0, 13, 0, 14, 0, 15, 0]),
    hs: Float32Array.from([0.7, 1.2, 1, NaN, 1, 1]),
    waveAngle: Float32Array.from([30, 120, 90, 90, 160, 90]),
    wavePeriod: Float32Array.from([7, 9, 8, 8, 8, 12]),
    flags: Uint32Array.from([0, 0, FLAG_FILTERED, 0, 0, 0]),
  };
  const limits = { hs: { min: 0.7, max: 1.2 }, waveAngle: { min: 30, max: 120 }, wavePeriod: { min: 7, max: 9 } };
  const original = structuredClone(scene);
  const shown = sampleDots(scene, DEFAULT_TOGGLES, "source", null, limits);
  expect([...shown.refs]).toEqual([3, 4]);
  expect(sampleIdsOf(scene, [...shown.refs])).toEqual([10, 11]);
  expect(drawnOnly([3, 4, 5, 6, 7, 8], shown.refs, 9)).toEqual([3, 4]);
  expect([...sampleDots(scene, { ...DEFAULT_TOGGLES, filtered: true }, "source", null, limits).refs]).toEqual([3, 4, 5]);
  expect([...sampleDots(scene, DEFAULT_TOGGLES, "source").refs]).toEqual([3, 4, 6, 7, 8]);
  expect(scene).toEqual(original);
});
