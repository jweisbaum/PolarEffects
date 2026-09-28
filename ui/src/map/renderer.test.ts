import { describe, expect, it } from "vitest";

import { copiesInView, graticule } from "./renderer";

describe("the flat map's copies", () => {
  const view = { width: 800, height: 400 };

  it("needs one copy when the world fits around the centre", () => {
    expect(copiesInView({ lon: 0, lat: 0, scale: 3 }, view)).toEqual([0]);
  });

  it("adds the copy across the antimeridian when the view reaches it", () => {
    expect(copiesInView({ lon: 170, lat: 0, scale: 10 }, view)).toEqual([0, 360]);
    expect(copiesInView({ lon: -170, lat: 0, scale: 10 }, view)).toEqual([-360, 0]);
  });

  it("covers a view wider than the world", () => {
    expect(copiesInView({ lon: 0, lat: 0, scale: 1 }, { width: 1000, height: 400 })).toEqual([-360, 0, 360]);
  });
});

describe("the graticule", () => {
  it("is line segments on whole degrees, every 30°", () => {
    const lines = graticule();
    expect(lines.length % 4).toBe(0);
    const lons = new Set<number>();
    for (let i = 0; i < lines.length; i += 4) if (lines[i] === lines[i + 2]) lons.add(lines[i]!);
    expect([...lons].sort((a, b) => a - b)).toEqual([-180, -150, -120, -90, -60, -30, 0, 30, 60, 90, 120, 150]);
  });
});
