/**
 * The tracker dialog's map preview (spec.md 7.2): every boat's track, a
 * few dozen points each, over the basemap's coastline, framed on the fleet
 * in an equirectangular view. Kept free of React so it can be tested.
 */
import type { BasemapLod } from "../map/format";

/** The preview's drawing size, in SVG units. */
export const VIEW_W = 600;
export const VIEW_H = 280;

/** The part of the globe a preview shows. */
export interface Frame {
  west: number;
  east: number;
  south: number;
  north: number;
  /**
   * Whether longitudes west of 0 are drawn +360, so a fleet crossing the
   * antimeridian stays in one piece.
   */
  shift: boolean;
}

function span(lons: number[]): number {
  return lons.length === 0 ? 0 : Math.max(...lons) - Math.min(...lons);
}

/** The frame that holds every line (interleaved lon, lat), padded; null when there is nothing. */
export function frameOf(lines: number[][]): Frame | null {
  const lons: number[] = [];
  const lats: number[] = [];
  for (const line of lines) {
    for (let k = 0; k + 1 < line.length; k += 2) {
      lons.push(line[k]!);
      lats.push(line[k + 1]!);
    }
  }
  if (lons.length === 0) return null;
  const shifted = lons.map((lon) => (lon < 0 ? lon + 360 : lon));
  const shift = span(shifted) < span(lons);
  const xs = shift ? shifted : lons;
  let west = Math.min(...xs), east = Math.max(...xs);
  let south = Math.min(...lats), north = Math.max(...lats);
  const padLon = Math.max((east - west) * 0.08, 0.05);
  const padLat = Math.max((north - south) * 0.08, 0.05);
  west -= padLon; east += padLon;
  south = Math.max(south - padLat, -90); north = Math.min(north + padLat, 90);
  // Keep the picture undistorted: widen whichever side is short.
  const cos = Math.max(Math.cos((((south + north) / 2) * Math.PI) / 180), 0.05);
  const wide = (east - west) * cos, tall = north - south;
  if (wide / tall < VIEW_W / VIEW_H) {
    const grow = ((tall * VIEW_W) / VIEW_H / cos - (east - west)) / 2;
    west -= grow; east += grow;
  } else {
    const grow = (wide * (VIEW_H / VIEW_W) - tall) / 2;
    south -= grow; north += grow;
  }
  return { west, east, south, north, shift };
}

/** A longitude and latitude as SVG coordinates in the frame. */
export function toView(frame: Frame, lon: number, lat: number): [number, number] {
  const x = frame.shift && lon < 0 ? lon + 360 : lon;
  return [
    ((x - frame.west) / (frame.east - frame.west)) * VIEW_W,
    ((frame.north - lat) / (frame.north - frame.south)) * VIEW_H,
  ];
}

const round = (v: number) => Math.round(v * 10) / 10;

/** An SVG path through a line of interleaved lon, lat. */
export function linePath(frame: Frame, line: number[]): string {
  const parts: string[] = [];
  for (let k = 0; k + 1 < line.length; k += 2) {
    const [x, y] = toView(frame, line[k]!, line[k + 1]!);
    parts.push(`${k === 0 ? "M" : "L"}${round(x)} ${round(y)}`);
  }
  return parts.join("");
}

/** The coastline segments that touch the frame, as one SVG path. */
export function coastPath(frame: Frame, lod: BasemapLod): string {
  const v = lod.lineVertices;
  const idx = lod.lineIndices;
  const inside = (x: number, y: number) => x >= -VIEW_W && x <= 2 * VIEW_W && y >= -VIEW_H && y <= 2 * VIEW_H;
  const parts: string[] = [];
  for (let k = 0; k + 1 < idx.length; k += 2) {
    const a = idx[k]!, b = idx[k + 1]!;
    const lonA = v[2 * a]!, latA = v[2 * a + 1]!, lonB = v[2 * b]!, latB = v[2 * b + 1]!;
    const [xa, ya] = toView(frame, lonA, latA);
    const [xb, yb] = toView(frame, lonB, latB);
    // A segment jumping across the picture is one that wraps the globe.
    if (Math.abs(xa - xb) > VIEW_W / 2 && Math.abs(lonA - lonB) > 180) continue;
    if (!inside(xa, ya) && !inside(xb, yb)) continue;
    parts.push(`M${round(xa)} ${round(ya)}L${round(xb)} ${round(yb)}`);
  }
  return parts.join("");
}

/** The basemap level to draw: the finest for a small frame, the coarsest for an ocean. */
export function lodFor(frame: Frame, lods: BasemapLod[]): BasemapLod | undefined {
  if (lods.length === 0) return undefined;
  return frame.east - frame.west > 40 ? lods[0] : lods[lods.length - 1];
}
