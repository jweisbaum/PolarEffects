import { describe, expect, it } from "vitest";

import type { PolarCurve } from "../generated/PolarCurve";
import type { DotPacket } from "./dotPacket";
import {
  axisLabels, fitLayout, labelsOverlap, maxBoatSpeed, nearestPoint, niceTicks, project, speedTicks,
} from "./plotGeometry";

/** One dot of one source, as the packet carries it. */
const dot = (sourceId: number, twa: number, tws: number, bsp: number): DotPacket => ({
  count: 1, sources: [sourceId], points: Float32Array.from([twa, tws, bsp]), source: Uint32Array.from([0]),
  ids: Uint32Array.from([1, 0]), flags: Uint32Array.from([0]),
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
    expect(hit).toEqual({ label: "Track", colour: "#e15759", twa: 40, tws: 10, bsp: 6, x, y });
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
