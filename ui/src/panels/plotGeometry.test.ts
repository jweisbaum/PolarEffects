import { describe, expect, it } from "vitest";

import type { PolarCurve } from "../generated/PolarCurve";
import { DOT_EXCLUDED, DOT_FILTERED, DOT_THROUGH_WATER, withoutExcluded, type DotPacket } from "./dotPacket";
import { DAY_BANDS } from "../dayBand";
import {
  axisLabels, crossings, curveSpeedAt, dotFill, fitLayout, labelsOverlap, maxBoatSpeed, measureBetween, nearestCrossing, nearestPoint,
  niceTicks, project, speedTicks, unproject,
} from "./plotGeometry";

/** One dot of one source, as the packet carries it. */
const dot = (sourceId: number, twa: number, tws: number, bsp: number, flags = 0): DotPacket => ({
  count: 1, sources: [sourceId], points: Float32Array.from([twa, tws, bsp]), source: Uint32Array.from([0]),
  ids: Uint32Array.from([1, 0]), flags: Uint32Array.from([flags]),
});

const curve = (label: string, colour: string, tws: number, points: [number, number][]): PolarCurve => ({
  source_id: 1,
  label,
  colour,
  tws,
  points: points.map(([twa, bsp]) => ({ twa, bsp })),
});

describe("project", () => {
  it("puts 0 degrees up, 90 to the right and 180 down", () => {
    const layout = fitLayout(400, 400, 10);
    const up = project(0, 10, layout);
    const right = project(90, 10, layout);
    const down = project(180, 10, layout);
    expect(up.x).toBeCloseTo(layout.centerX);
    expect(up.y).toBeLessThan(layout.centerY);
    expect(right.x).toBeGreaterThan(layout.centerX);
    expect(right.y).toBeCloseTo(layout.centerY);
    expect(down.x).toBeCloseTo(layout.centerX);
    expect(down.y).toBeGreaterThan(layout.centerY);
  });

  it("centres the 0° axis on the stage's middle line, keeping the fan clear of the edges (asked 2026-10-02)", () => {
    // 600 wide, 400 high, 28 px kept clear: the fan may be 172 px high (half of 344)
    // and, centred, 272 px wide; the height decides.
    const stage = fitLayout(600, 400, 10, 28, false, true);
    expect(stage.centerX).toBe(300);
    expect(stage.centerY).toBe(200);
    expect(stage.scale).toBeCloseTo(17.2);
    // A narrow stage: the half width decides.
    expect(fitLayout(200, 400, 10, 28, false, true).scale).toBeCloseTo(7.2);
    // The panel keeps the fan against its left edge.
    expect(fitLayout(600, 400, 10, 28, false).centerX).toBe(28);
  });

  it("places a zero-speed point at the centre whatever the angle", () => {
    const layout = fitLayout(400, 400, 10);
    for (const twa of [0, 45, 90, 180]) {
      const { x, y } = project(twa, 0, layout);
      expect(x).toBeCloseTo(layout.centerX);
      expect(y).toBeCloseTo(layout.centerY);
    }
  });
});

describe("fitLayout", () => {
  it("fits the fan's radius (width and half the height) inside the padded box", () => {
    const layout = fitLayout(300, 200, 8, 20);
    const top = project(0, 8, layout);
    const bottom = project(180, 8, layout);
    const right = project(90, 8, layout);
    expect(top.y).toBeGreaterThanOrEqual(0);
    expect(bottom.y).toBeLessThanOrEqual(200);
    expect(right.x).toBeLessThanOrEqual(300);
  });

  it("gives a zero scale for a domain with no speed at all", () => {
    expect(fitLayout(300, 200, 0).scale).toBe(0);
  });
});

describe("niceTicks", () => {
  it("steps by 1, 2 or 5 times a power of ten", () => {
    expect(niceTicks(9.4)).toEqual([2, 4, 6, 8, 10]);
    expect(niceTicks(23)).toEqual([5, 10, 15, 20, 25]);
    expect(niceTicks(0.9)).toEqual([0.2, 0.4, 0.6, 0.8, 1]);
  });

  it("is empty for a non-positive or non-finite maximum", () => {
    expect(niceTicks(0)).toEqual([]);
    expect(niceTicks(-5)).toEqual([]);
    expect(niceTicks(NaN)).toEqual([]);
  });
});

describe("maxBoatSpeed", () => {
  it("is the highest BSP across every curve and dot", () => {
    const curves = [curve("A", "#111", 10, [[40, 5], [90, 8]]), curve("B", "#222", 10, [[40, 3]])];
    const dots = dot(1, 90, 10, 12);
    expect(maxBoatSpeed(curves, dots)).toBe(12);
    expect(maxBoatSpeed([], null)).toBe(0);
  });

  it("drops excluded dots unless they are shown (asked 2026-10-04)", () => {
    const three: DotPacket = {
      count: 3, sources: [7], points: Float32Array.from([40, 10, 5, 90, 10, 30, 120, 10, 6]),
      source: Uint32Array.from([0, 0, 0]), ids: Uint32Array.from([1, 0, 2, 0, 3, 0]),
      flags: Uint32Array.from([0, DOT_EXCLUDED, DOT_FILTERED]),
    };
    const kept = withoutExcluded(three);
    expect(kept.count).toBe(2);
    expect([...kept.points]).toEqual([40, 10, 5, 120, 10, 6]);
    expect([...kept.ids]).toEqual([1, 0, 3, 0]);
    expect([...kept.flags]).toEqual([0, DOT_FILTERED]);
    expect(kept.sources).toEqual([7]);
    // The rings reach the fastest dot shown: 30 kn with the excluded one, 6 without.
    expect(maxBoatSpeed([], three)).toBe(30);
    expect(maxBoatSpeed([], kept)).toBe(6);
    // Nothing excluded: the same packet back.
    expect(withoutExcluded(kept)).toBe(kept);
  });
});

describe("nearestPoint", () => {
  const curves = [curve("A", "#4e79a7", 10, [[40, 5], [90, 8]])];
  const layout = fitLayout(400, 400, 10);

  it("finds the closest curve point within the distance limit", () => {
    const { x, y } = project(90, 8, layout);
    const hit = nearestPoint(curves, null, new Map(), x + 1, y - 1, layout, 10);
    expect(hit).not.toBeNull();
    expect(hit?.twa).toBe(90);
    expect(hit?.bsp).toBe(8);
    expect(hit?.label).toBe("A");
  });

  it("returns null beyond the distance limit", () => {
    expect(nearestPoint(curves, null, new Map(), 0, 0, layout, 1)).toBeNull();
  });

  it("looks a dot's source up by id for its label and colour", () => {
    const dots = dot(7, 40, 10, 6);
    const styles = new Map([[7, { label: "Track", colour: "#e15759" }]]);
    const { x, y } = project(40, 6, layout);
    const hit = nearestPoint([], dots, styles, x, y, layout, 5);
    expect(hit).toEqual({ label: "Track", colour: "#e15759", twa: 40, tws: 10, bsp: 6, speed: "SOG", blend: false, x, y });
  });

  it("calls a dot's speed SOG, or STW where it was corrected for current, and a curve's BSP (asked 2026-10-04)", () => {
    const styles = new Map([[7, { label: "Track", colour: "#e15759" }]]);
    const { x, y } = project(40, 6, layout);
    expect(nearestPoint([], dot(7, 40, 10, 6), styles, x, y, layout, 5)?.speed).toBe("SOG");
    expect(nearestPoint([], dot(7, 40, 10, 6, DOT_THROUGH_WATER), styles, x, y, layout, 5)?.speed).toBe("STW");
    const curves = [curve("A", "#111", 10, [[40, 6]])];
    expect(nearestPoint(curves, null, styles, x, y, layout, 5)?.speed).toBe("BSP");
  });

  it("ignores a dot whose source is not in the lookup", () => {
    const dots = dot(99, 40, 10, 6);
    const { x, y } = project(40, 6, layout);
    expect(nearestPoint([], dots, new Map(), x, y, layout, 5)).toBeNull();
  });
});

describe("axisLabels", () => {
  /** A 10 px sans-serif advance, roughly: 6 px a character. */
  const measure = (text: string) => text.length * 6;

  it("never lets two labels overlap, whatever the size and speed range", () => {
    for (const [width, height] of [[180, 160], [260, 300], [340, 420], [600, 800], [1200, 700]] as const) {
      for (const maxBsp of [0.8, 3, 7.3, 9.9, 12, 16.4, 20, 23.7, 31, 48]) {
        const labels = axisLabels(fitLayout(width, height, maxBsp), maxBsp, measure);
        for (let i = 0; i < labels.length; i++) {
          for (let j = i + 1; j < labels.length; j++) {
            expect(labelsOverlap(labels[i]!, labels[j]!), `${width}x${height} ${maxBsp} kn: "${labels[i]!.text}" / "${labels[j]!.text}"`).toBe(false);
          }
        }
      }
    }
  });

  it("keeps both 90° and the outermost ring's value in the panel case that collided", () => {
    // The panel plot of the UX shots: 20 kn is the outermost ring and the
    // 90° label sat on the same line just past it.
    const labels = axisLabels(fitLayout(300, 260, 19.2), 19.2, measure);
    const texts = labels.map((label) => label.text);
    expect(texts).toContain("90°");
    expect(texts).toContain("20");
    const ninety = labels.find((label) => label.text === "90°")!;
    const twenty = labels.find((label) => label.text === "20")!;
    expect(ninety.y + ninety.height).toBeLessThanOrEqual(twenty.y);
  });

  it("puts every angle label outside the outermost ring", () => {
    const layout = fitLayout(400, 400, 12);
    const outer = 12 * layout.scale; // ticks 2..12, the last is 12
    for (const label of axisLabels(layout, 12, measure).filter((l) => l.text.endsWith("°"))) {
      const farthest = Math.max(
        ...[[label.x, label.y], [label.x + label.width, label.y], [label.x, label.y + label.height],
          [label.x + label.width, label.y + label.height]].map(([x, y]) => Math.hypot(x! - layout.centerX, y! - layout.centerY)),
      );
      expect(farthest).toBeGreaterThan(outer);
    }
  });

  it("drops a ring value rather than drawing it over its neighbour", () => {
    // Rings 4 px apart cannot each carry a 6–12 px label.
    const layout = { centerX: 10, centerY: 100, scale: 2 };
    const labels = axisLabels(layout, 10, measure).filter((l) => !l.text.endsWith("°"));
    expect(labels.length).toBeLessThan(niceTicks(10).length);
    expect(labels.length).toBeGreaterThan(0);
  });
});

describe("rings in the display speed unit (M17b)", () => {
  const measure = (text: string) => text.length * 6;

  it("puts round km/h or m/s values at their speed in knots", () => {
    // 10 kn is 18.52 km/h: rings every 5 km/h, the 5 km/h ring at 5 / 1.852 = 2.6998 kn.
    expect(speedTicks(10, 1.852).map((t) => t.value)).toEqual([5, 10, 15, 20]);
    expect(speedTicks(10, 1.852)[0]!.knots).toBeCloseTo(2.6998, 4);
    // 10 kn is 5.144 m/s: rings every 2 m/s, the 2 m/s ring at 7200 / 1852 = 3.8877 kn.
    expect(speedTicks(10, 1852 / 3600).map((t) => t.value)).toEqual([2, 4, 6]);
    expect(speedTicks(10, 1852 / 3600)[0]!.knots).toBeCloseTo(3.8877, 4);
    // Knots are unchanged.
    expect(speedTicks(9.4).map((t) => [t.value, t.knots])).toEqual([[2, 2], [4, 4], [6, 6], [8, 8], [10, 10]]);
  });

  it("labels each ring with its display value, where the ring is drawn", () => {
    const layout = fitLayout(400, 400, 10);
    const rings = axisLabels(layout, 10, measure, 10, 1.852).filter((l) => !l.text.endsWith("°"));
    expect(rings.map((l) => l.text)).toEqual(["5", "10", "15", "20"]);
    // The 10 km/h label starts 3 px past the ring at 10 / 1.852 kn.
    expect(rings[1]!.x).toBeCloseTo(layout.centerX + (10 / 1.852) * layout.scale + 3, 6);
  });
});

it("fits independent port and starboard speeds within a full-circle plot", () => {
  const layout = fitLayout(400, 300, 10, 28, true);
  const starboard = project(90, 6, layout);
  const port = project(270, 9, layout);
  expect(layout.centerX).toBe(200);
  expect(starboard.x).toBeGreaterThan(200);
  expect(port.x).toBeLessThan(200);
  expect((200 - port.x) / (starboard.x - 200)).toBeCloseTo(1.5);
  expect(port.x).toBeGreaterThanOrEqual(28);
  expect(starboard.x).toBeLessThanOrEqual(372);
  const angles = axisLabels(layout, 10, text => text.length * 6, 10, 1, true).filter(label => label.text.endsWith("°"));
  expect(angles.map(label => label.text)).toEqual(["0°", "30°", "60°", "90°", "120°", "150°", "180°", "150°", "120°", "90°", "60°", "30°"]);
  expect(angles.find(label => label.text === "90°" && label.x < layout.centerX)).toBeDefined();
  expect(angles.find(label => label.text === "90°" && label.x > layout.centerX)).toBeDefined();
});

describe("dotFill (spec.md 9.2)", () => {
  const dots = {
    count: 2, sources: [7, 9], points: Float32Array.from([45, 10, 6, 135, 10, 8]),
    source: Uint32Array.from([0, 1]), ids: Uint32Array.from([1, 0, 2, 0]),
    // Morning (1) and evening (3) in bits 8–9.
    flags: Uint32Array.from([1 << 8, (3 << 8) | 2]),
  };
  const bySource = ["#111111", "#222222"];

  it("is the dot's track colour by source", () => {
    expect(dotFill(dots, 0, "source", bySource)).toBe("#111111");
    expect(dotFill(dots, 1, "source", bySource)).toBe("#222222");
  });

  it("is the band's colour by time of day, whatever the track", () => {
    expect(dotFill(dots, 0, "timeOfDay", bySource)).toBe(DAY_BANDS[1]!.colour);
    expect(dotFill(dots, 1, "timeOfDay", bySource)).toBe(DAY_BANDS[3]!.colour);
  });
});

describe("nearestPoint and the blend (spec.md 9.2)", () => {
  const layout = fitLayout(300, 300, 7, 28);
  const curves: PolarCurve[] = [
    { source_id: 4, label: "A", colour: "#4e79a7", tws: 10, points: [{ twa: 60, bsp: 6 }] },
    { source_id: null, label: "Blend", colour: "#e0457b", tws: 10, points: [{ twa: 90, bsp: 7 }] },
  ];
  it("says whether the hovered point is the blend's", () => {
    const onBlend = project(90, 7, layout);
    expect(nearestPoint(curves, null, new Map(), onBlend.x, onBlend.y, layout, 16)?.blend).toBe(true);
    const onSource = project(60, 6, layout);
    expect(nearestPoint(curves, null, new Map(), onSource.x, onSource.y, layout, 16)?.blend).toBe(false);
  });
});

describe("measuring on the plot (spec.md 9.2)", () => {
  const layout = fitLayout(300, 300, 8, 28);
  const curve = (label: string, tws: number, points: [number, number][], source_id: number | null = 1): PolarCurve => ({
    source_id, label, colour: "#4e79a7", tws, points: points.map(([twa, bsp]) => ({ twa, bsp })),
  });

  it("reads a canvas point back as the wind angle and boat speed it stands for", () => {
    for (const [twa, bsp] of [[60, 5], [90, 8], [150, 2.5], [270, 3], [0, 4]] as const) {
      const at = project(twa, bsp, layout);
      const back = unproject(at.x, at.y, layout);
      expect(back.twa).toBeCloseTo(twa, 9);
      expect(back.bsp).toBeCloseTo(bsp, 9);
    }
    // The centre is no speed at all, at 0° by convention; angles stay in [0, 360).
    expect(unproject(layout.centerX, layout.centerY, layout)).toEqual({ twa: 0, bsp: 0 });
    expect(unproject(layout.centerX - 10, layout.centerY, layout).twa).toBe(270);
  });

  it("reads a curve's speed at an angle between its points, and nothing outside them", () => {
    const c = curve("A", 10, [[40, 5], [60, 7], [90, 8]]);
    expect(curveSpeedAt(c, 60)).toBe(7);
    // Half way from 40° to 60°: half way from 5 to 7 kn.
    expect(curveSpeedAt(c, 50)).toBe(6);
    expect(curveSpeedAt(c, 75)).toBe(7.5);
    expect(curveSpeedAt(c, 39.9)).toBeNull();
    expect(curveSpeedAt(c, 90.1)).toBeNull();
    expect(curveSpeedAt(curve("One", 10, [[90, 6]]), 90)).toBe(6);
    expect(curveSpeedAt(curve("One", 10, [[90, 6]]), 91)).toBeNull();
    expect(curveSpeedAt(curve("None", 10, []), 90)).toBeNull();
  });

  it("lists every curve with a value at an angle, fastest first", () => {
    const curves = [
      curve("A", 10, [[60, 6], [120, 6.5]]),
      curve("B", 10, [[100, 5], [140, 6]]),
      curve("Blend", 10, [[60, 6.6], [120, 7]], null),
    ];
    // At 90°: A half way 6 → 6.5, the blend half way 6.6 → 7, B not there.
    expect(crossings(curves, 90).map((c) => [c.label, c.bsp, c.blend])).toEqual([["Blend", 6.8, true], ["A", 6.25, false]]);
    expect(crossings(curves, 30)).toEqual([]);
  });

  it("takes the curve nearest the pointer's speed as the one compared against", () => {
    const at = crossings([curve("A", 10, [[90, 6]]), curve("B", 12, [[90, 7.5]]), curve("C", 14, [[90, 9]])], 90);
    expect(at.map((c) => c.label)).toEqual(["C", "B", "A"]);
    expect(nearestCrossing(at, 7.2)).toBe(1);
    expect(nearestCrossing(at, 20)).toBe(0);
    expect(nearestCrossing(at, 0)).toBe(2);
    expect(nearestCrossing([], 5)).toBe(-1);
  });

  it("measures from one point to another: the speed gained, its ratio and the angle between", () => {
    // 6.5 kn at 60° to 7.8 kn at 92°: +1.3 kn, 7.8 / 6.5 = 1.2, 32° apart.
    const m = measureBetween({ twa: 60, bsp: 6.5 }, { twa: 92, bsp: 7.8 });
    expect(m.deltaBsp).toBeCloseTo(1.3, 12);
    expect(m.ratio).toBeCloseTo(1.2, 12);
    expect(m.deltaTwa).toBe(32);
    // Across 0°: 350° to 10° is 20° apart, not 340°.
    expect(measureBetween({ twa: 350, bsp: 5 }, { twa: 10, bsp: 4 }).deltaTwa).toBe(20);
    // Nothing is a ratio of no speed.
    expect(measureBetween({ twa: 0, bsp: 0 }, { twa: 90, bsp: 5 }).ratio).toBeNull();
  });
});
