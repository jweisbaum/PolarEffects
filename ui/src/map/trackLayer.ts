/**
 * What the map draws for tracks (spec.md 9.1), worked out without a GPU so
 * it can be tested: the line geometry with a colour per fix (filtered fixes
 * dimmed), a spatial index for hovering, box selection, and framing a set of
 * fixes.
 *
 * Longitudes arrive unwrapped along each track (`trackPacket.ts`), so a
 * track crossing the antimeridian is one continuous line from 179° to 181°;
 * the renderer draws it once per 360° copy of the flat map in view, and the
 * globe projects it per vertex, where 181° is simply 179°W.
 */
import { FIX_EXCLUDED, FIX_FILTERED, fixSampleId, type TrackPacket } from "./trackPacket";
import { forward, wrapLon, type Camera, type ProjectionId, type Viewport } from "./projection";

/** How opaque a fix is drawn: kept, excluded by hand, or filtered out (dimmed). */
export const ALPHA_KEPT = 0.95;
export const ALPHA_EXCLUDED = 0.5;
export const ALPHA_FILTERED = 0.22;

export interface TrackGeometry {
  /** (lon, lat) per fix: the packet's own array. */
  vertices: Float32Array;
  /** RGBA per fix, 0–1. */
  colours: Float32Array;
  /** Segment end-point pairs, within each track only. */
  indices: Uint32Array;
}

function rgb(colour: string): [number, number, number] {
  if (!/^#[0-9a-fA-F]{6}$/.test(colour)) return [0.6, 0.6, 0.6];
  const value = Number.parseInt(colour.slice(1), 16);
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
}

/** The lines to draw: every track in its source colour, filtered fixes dimmed. */
export function buildTrackGeometry(packet: TrackPacket): TrackGeometry {
  const { fixes, tracks } = packet;
  const colours = new Float32Array(fixes.count * 4);
  let segments = 0;
  for (const track of tracks) segments += Math.max(0, track.count - 1);
  const indices = new Uint32Array(segments * 2);
  let s = 0;
  for (const track of tracks) {
    const [r, g, b] = rgb(track.colour);
    for (let k = track.first; k < track.first + track.count; k++) {
      const flags = fixes.flags[k]!;
      colours[k * 4] = r;
      colours[k * 4 + 1] = g;
      colours[k * 4 + 2] = b;
      colours[k * 4 + 3] = flags & FIX_FILTERED ? ALPHA_FILTERED : flags & FIX_EXCLUDED ? ALPHA_EXCLUDED : ALPHA_KEPT;
      if (k > track.first) {
        indices[s++] = k - 1;
        indices[s++] = k;
      }
    }
  }
  return { vertices: fixes.points, colours, indices };
}

/** The (lon, lat) of the fixes whose sample ids are in `ids`, for highlighting. */
export function selectedPoints(packet: TrackPacket, ids: ReadonlySet<number>): Float32Array {
  if (ids.size === 0) return new Float32Array();
  const out: number[] = [];
  for (let k = 0; k < packet.fixes.count; k++) {
    if (ids.has(fixSampleId(packet, k))) out.push(packet.fixes.points[k * 2]!, packet.fixes.points[k * 2 + 1]!);
  }
  return Float32Array.from(out);
}

/** Cell size of the hover index, degrees. */
const CELL_DEG = 1;

/**
 * A grid of 1° cells over the wrapped longitude and latitude of every fix,
 * so hovering looks at the handful of fixes near the pointer rather than all
 * half-million.
 */
export class FixIndex {
  private readonly cells = new Map<number, number[]>();

  constructor(private readonly packet: TrackPacket) {
    const { points, count } = packet.fixes;
    for (let k = 0; k < count; k++) {
      const key = FixIndex.key(points[k * 2]!, points[k * 2 + 1]!);
      const cell = this.cells.get(key);
      if (cell) cell.push(k);
      else this.cells.set(key, [k]);
    }
  }

  private static key(lon: number, lat: number): number {
    const i = Math.floor((wrapLon(lon) + 180) / CELL_DEG);
    const j = Math.floor((Math.max(-90, Math.min(89.999, lat)) + 90) / CELL_DEG);
    return j * 360 + i;
  }

  /**
   * The fix nearest the pointer on screen, within `radiusPx`, or -1. The
   * search box in degrees comes from the camera's scale (pixels per degree
   * at the centre), widened on the globe, where the limb compresses.
   */
  nearest(projection: ProjectionId, camera: Camera, view: Viewport, lon: number, lat: number,
    x: number, y: number, radiusPx: number): number {
    const radiusDeg = (radiusPx / camera.scale) * (projection === "orthographic" ? 3 : 1);
    const { points } = this.packet.fixes;
    let best = -1;
    let bestDistance = radiusPx;
    const consider = (k: number) => {
      const at = forward(projection, camera, view, points[k * 2]!, points[k * 2 + 1]!);
      if (!at) return;
      const distance = Math.hypot(at.x - x, at.y - y);
      if (distance <= bestDistance) { bestDistance = distance; best = k; }
    };
    if (radiusDeg > 30) {
      for (let k = 0; k < this.packet.fixes.count; k++) consider(k);
      return best;
    }
    const lonCos = Math.max(0.05, Math.cos((lat * Math.PI) / 180));
    const spanLon = Math.min(180, radiusDeg / (projection === "orthographic" ? lonCos : 1));
    for (let dlat = -radiusDeg; dlat <= radiusDeg + CELL_DEG; dlat += CELL_DEG) {
      const cellLat = lat + Math.min(dlat, radiusDeg);
      if (cellLat < -90 || cellLat > 90) continue;
      for (let dlon = -spanLon; dlon <= spanLon + CELL_DEG; dlon += CELL_DEG) {
        const cell = this.cells.get(FixIndex.key(lon + Math.min(dlon, spanLon), cellLat));
        if (cell) for (const k of cell) consider(k);
      }
    }
    return best;
  }
}

/** The sample ids of every fix drawn inside a screen rectangle. */
export function boxSelect(packet: TrackPacket, projection: ProjectionId, camera: Camera, view: Viewport,
  x0: number, y0: number, x1: number, y1: number): number[] {
  const [left, right] = x0 < x1 ? [x0, x1] : [x1, x0];
  const [top, bottom] = y0 < y1 ? [y0, y1] : [y1, y0];
  const out: number[] = [];
  const { points, count } = packet.fixes;
  for (let k = 0; k < count; k++) {
    const at = forward(projection, camera, view, points[k * 2]!, points[k * 2 + 1]!);
    if (at && at.x >= left && at.x <= right && at.y >= top && at.y <= bottom) out.push(fixSampleId(packet, k));
  }
  return out;
}

/**
 * The camera that frames some (unwrapped lon, lat) points with a margin, or
 * null when there are none. Longitudes are compared the short way round, so
 * a race across the antimeridian is framed across it, not round the world.
 */
export function frameCamera(projection: ProjectionId, view: Viewport, points: Float32Array,
  maxScale: number): Camera | null {
  const n = points.length / 2;
  if (n === 0) return null;
  // Longitudes relative to the first, the short way, so any crossing is continuous.
  const base = points[0]!;
  let west = Infinity, east = -Infinity, south = Infinity, north = -Infinity;
  for (let k = 0; k < n; k++) {
    const lon = base + (((points[k * 2]! - base + 540) % 360) + 360) % 360 - 180;
    const lat = points[k * 2 + 1]!;
    west = Math.min(west, lon); east = Math.max(east, lon);
    south = Math.min(south, lat); north = Math.max(north, lat);
  }
  const spanLon = Math.max(east - west, 0.05);
  const spanLat = Math.max(north - south, 0.05);
  const centre = { lon: wrapLon((west + east) / 2), lat: (south + north) / 2 };
  const fit = 0.8 * Math.min(view.width / spanLon, view.height / spanLat);
  if (projection === "orthographic") {
    // Pixels per degree of arc at the centre; the globe shows at most a hemisphere.
    const arc = Math.max(spanLon * Math.cos((centre.lat * Math.PI) / 180), spanLat);
    const scale = Math.min(maxScale, (0.8 * Math.min(view.width, view.height)) / Math.max(arc, 0.05));
    return { ...centre, scale: Math.max(scale, (0.45 * Math.min(view.width, view.height) * Math.PI) / 180) };
  }
  return { ...centre, scale: Math.min(maxScale, fit) };
}
