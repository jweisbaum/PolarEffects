import { describe, expect, it } from "vitest";

import type { PolarCurve } from "../generated/PolarCurve";
import type { PolarSampleDot } from "../generated/PolarSampleDot";
import { fitLayout, maxBoatSpeed, nearestPoint, niceTicks, project } from "./plotGeometry";

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
    const dots: PolarSampleDot[] = [{ source_id: 1, sample_id: 1, twa: 90, tws: 10, bsp: 12, filtered: false, excluded: false }];
    expect(maxBoatSpeed(curves, dots)).toBe(12);
    expect(maxBoatSpeed([], [])).toBe(0);
  });
});

describe("nearestPoint", () => {
  const curves = [curve("A", "#4e79a7", 10, [[40, 5], [90, 8]])];
  const layout = fitLayout(400, 400, 10);

  it("finds the closest curve point within the distance limit", () => {
    const { x, y } = project(90, 8, layout);
    const hit = nearestPoint(curves, [], new Map(), x + 1, y - 1, layout, 10);
    expect(hit).not.toBeNull();
    expect(hit?.twa).toBe(90);
    expect(hit?.bsp).toBe(8);
    expect(hit?.label).toBe("A");
  });

  it("returns null beyond the distance limit", () => {
    expect(nearestPoint(curves, [], new Map(), 0, 0, layout, 1)).toBeNull();
  });

  it("looks a dot's source up by id for its label and colour", () => {
    const dots: PolarSampleDot[] = [{ source_id: 7, sample_id: 1, twa: 40, tws: 10, bsp: 6, filtered: false, excluded: false }];
    const styles = new Map([[7, { label: "Track", colour: "#e15759" }]]);
    const { x, y } = project(40, 6, layout);
    const hit = nearestPoint([], dots, styles, x, y, layout, 5);
    expect(hit).toEqual({ label: "Track", colour: "#e15759", twa: 40, tws: 10, bsp: 6, x, y });
  });

  it("ignores a dot whose source is not in the lookup", () => {
    const dots: PolarSampleDot[] = [{ source_id: 99, sample_id: 1, twa: 40, tws: 10, bsp: 6, filtered: false, excluded: false }];
    const { x, y } = project(40, 6, layout);
    expect(nearestPoint([], dots, new Map(), x, y, layout, 5)).toBeNull();
  });
});
