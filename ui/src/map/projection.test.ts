import { describe, expect, it } from "vitest";

import { fitCamera, forward, globeRadius, inverse, pan, wrapLon, zoomAt, type Camera } from "./projection";

const view = { width: 800, height: 400 };

describe("equirectangular", () => {
  const camera: Camera = { lon: 0, lat: 0, scale: 2 };

  it("puts the centre in the middle and a degree at `scale` pixels", () => {
    expect(forward("equirectangular", camera, view, 0, 0)).toEqual({ x: 400, y: 200 });
    expect(forward("equirectangular", camera, view, 10, 5)).toEqual({ x: 420, y: 190 });
  });

  it("wraps across the antimeridian rather than the long way round", () => {
    const pacific: Camera = { lon: 179, lat: 0, scale: 2 };
    // 181°E is 179°W: two degrees east of the centre, not 358 west.
    expect(forward("equirectangular", pacific, view, -179, 0)!.x).toBeCloseTo(404);
    expect(inverse("equirectangular", pacific, view, 404, 200)!.lon).toBeCloseTo(-179);
  });

  it("round-trips, and has nothing past the poles", () => {
    const p = forward("equirectangular", camera, view, -73.5, 40.7)!;
    const back = inverse("equirectangular", camera, view, p.x, p.y)!;
    expect(back.lon).toBeCloseTo(-73.5);
    expect(back.lat).toBeCloseTo(40.7);
    expect(inverse("equirectangular", camera, view, 400, -1)).toBeNull();
  });

  it("keeps the place under the pointer while zooming", () => {
    const before = inverse("equirectangular", camera, view, 600, 100)!;
    const zoomed = zoomAt("equirectangular", camera, view, 3, 600, 100);
    const after = inverse("equirectangular", zoomed, view, 600, 100)!;
    expect(zoomed.scale).toBeCloseTo(6);
    expect(after.lon).toBeCloseTo(before.lon);
    expect(after.lat).toBeCloseTo(before.lat);
  });

  it("pans with the pointer and never past a pole", () => {
    expect(pan(camera, 20, 0).lon).toBeCloseTo(-10);
    expect(pan(camera, 0, 10000).lat).toBe(90);
    expect(pan({ lon: 175, lat: 0, scale: 1 }, -10, 0).lon).toBeCloseTo(-175);
  });
});

describe("orthographic", () => {
  const camera: Camera = { lon: 0, lat: 0, scale: 2 };
  const r = globeRadius(2);

  it("draws the centre in the middle and the equator's edge on the rim", () => {
    expect(forward("orthographic", camera, view, 0, 0)).toEqual({ x: 400, y: 200 });
    const east = forward("orthographic", camera, view, 90, 0)!;
    expect(east.x).toBeCloseTo(400 + r);
    const north = forward("orthographic", camera, view, 0, 90)!;
    expect(north.y).toBeCloseTo(200 - r);
  });

  it("hides the far side", () => {
    expect(forward("orthographic", camera, view, 180, 0)).toBeNull();
    expect(forward("orthographic", camera, view, 120, 10)).toBeNull();
    expect(forward("orthographic", { lon: 0, lat: 90, scale: 2 }, view, 45, -10)).toBeNull();
  });

  it("round-trips anywhere on the visible side, including across the antimeridian", () => {
    const tilted: Camera = { lon: 170, lat: 35, scale: 1.5 };
    for (const [lon, lat] of [[170, 35], [-175, 50], [150, -10], [-160, 70]] as const) {
      const p = forward("orthographic", tilted, view, lon, lat)!;
      expect(p, `${lon},${lat}`).not.toBeNull();
      const back = inverse("orthographic", tilted, view, p.x, p.y)!;
      expect(wrapLon(back.lon - lon)).toBeCloseTo(0, 6);
      expect(back.lat).toBeCloseTo(lat, 6);
    }
  });

  it("has nothing off the globe", () => {
    expect(inverse("orthographic", camera, view, 400 + r + 1, 200)).toBeNull();
  });

  it("zooms about the centre", () => {
    const zoomed = zoomAt("orthographic", camera, view, 2, 0, 0);
    expect(zoomed).toEqual({ lon: 0, lat: 0, scale: 4 });
  });
});

describe("fitting the world", () => {
  it("shows all of it in either projection", () => {
    const flat = fitCamera("equirectangular", view);
    expect(forward("equirectangular", flat, view, -179.99, 90)!.x).toBeGreaterThanOrEqual(0);
    expect(forward("equirectangular", flat, view, 179.99, -90)!.y).toBeLessThanOrEqual(400);
    const globe = fitCamera("orthographic", view);
    expect(2 * globeRadius(globe.scale)).toBeLessThanOrEqual(400);
  });

  it("folds longitudes into [-180, 180)", () => {
    expect(wrapLon(180)).toBe(-180);
    expect(wrapLon(-181)).toBe(179);
    expect(wrapLon(540)).toBe(-180);
    expect(wrapLon(-360)).toBe(0);
  });
});
