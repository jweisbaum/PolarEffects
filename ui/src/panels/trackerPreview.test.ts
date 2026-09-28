/** The tracker dialog's search and map preview (spec.md 7.2). */
import { describe, expect, it } from "vitest";

import type { TrackerBoatRow } from "../generated/TrackerBoatRow";
import { boatStatusText, filterBoats } from "./trackerImport";
import { coastPath, frameOf, linePath, toView, VIEW_H, VIEW_W } from "./trackerPreview";

const row = (id: string, name: string, sail: string | null, division: string | null): TrackerBoatRow => ({
  id, name, sail, model: null, division, status: null, fixes: 1, first: null, last: null, preview: [],
});

describe("filterBoats", () => {
  const boats = [row("1", "Café Crème", "FRA 1234", "IRC 2"), row("2", "Bravo", "GBR/1124", "IRC Class 3")];

  it("matches every word, whatever the case and accents", () => {
    expect(filterBoats(boats, "cafe").map((b) => b.id)).toEqual(["1"]);
    expect(filterBoats(boats, "irc class 3").map((b) => b.id)).toEqual(["2"]);
    expect(filterBoats(boats, "  ").map((b) => b.id)).toEqual(["1", "2"]);
    expect(filterBoats(boats, "zulu")).toEqual([]);
  });

  it("finds a sail number with or without its space or slash", () => {
    expect(filterBoats(boats, "fra1234").map((b) => b.id)).toEqual(["1"]);
    expect(filterBoats(boats, "GBR1124").map((b) => b.id)).toEqual(["2"]);
    expect(filterBoats(boats, "gbr/1124").map((b) => b.id)).toEqual(["2"]);
  });
});

describe("boatStatusText", () => {
  it("translates the statuses it knows and keeps the others", () => {
    expect(boatStatusText("RETIRED")).toBe("Retired");
    expect(boatStatusText("OCS")).toBe("OCS");
    expect(boatStatusText(null)).toBe("");
  });
});

describe("the preview frame", () => {
  it("holds a fleet crossing the antimeridian in one piece", () => {
    const frame = frameOf([[179.5, -40, -179.5, -41]])!;
    expect(frame.shift).toBe(true);
    const [xWest] = toView(frame, 179.5, -40);
    const [xEast] = toView(frame, -179.5, -41);
    expect(xEast).toBeGreaterThan(xWest);
    expect(xEast - xWest).toBeLessThan(VIEW_W);
  });

  it("puts every point inside the picture without distorting it", () => {
    const frame = frameOf([[14.5, 35.9, 15.5, 38.2], [12.4, 37.8]])!;
    expect(frame.shift).toBe(false);
    for (const [lon, lat] of [[14.5, 35.9], [15.5, 38.2], [12.4, 37.8]] as const) {
      const [x, y] = toView(frame, lon, lat);
      expect(x).toBeGreaterThan(0); expect(x).toBeLessThan(VIEW_W);
      expect(y).toBeGreaterThan(0); expect(y).toBeLessThan(VIEW_H);
    }
    // One degree of longitude is cos(latitude) of one of latitude.
    const cos = Math.cos((((frame.north + frame.south) / 2) * Math.PI) / 180);
    const perLon = VIEW_W / (frame.east - frame.west), perLat = VIEW_H / (frame.north - frame.south);
    expect(perLon / perLat).toBeCloseTo(cos, 6);
    expect(linePath(frame, [14.5, 35.9, 15.5, 38.2])).toMatch(/^M[\d.]+ [\d.]+L[\d.]+ [\d.]+$/);
  });

  it("is nothing without positions", () => {
    expect(frameOf([[], []])).toBeNull();
  });

  it("draws only the coast near the fleet", () => {
    const frame = frameOf([[14.5, 35.9, 15.5, 36.2]])!;
    const lod = {
      marker: 50, triVertices: new Float32Array(), triIndices: new Uint32Array(),
      lineVertices: new Float32Array([14.4, 35.8, 14.6, 35.85, -70, 40, -71, 41]),
      lineIndices: new Uint32Array([0, 1, 2, 3]),
    };
    expect(coastPath(frame, lod).split("M")).toHaveLength(2);
  });
});
