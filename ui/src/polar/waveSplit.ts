/**
 * Split Wave Angle (spec.md 10.5): the 3D view drawn as one copy per wave
 * direction, each holding only the samples whose waves came from (or went
 * to) that direction as seen from the boat. This module is the arithmetic
 * of it, free of three.js and the DOM so it can be tested in Node: which
 * copy a sample belongs to, where the copies sit, and which dots answer a
 * hover in the other copies.
 */
import type { HoverPoint } from "../boats/synchronization";

/** How many directions the split offers. */
export const WAVE_SPLIT_COUNTS = [4, 8, 16, 18, 24, 36] as const;

/** Whether a copy is the direction the waves come from, or the one they go to. */
export type WaveSense = "from" | "to";

/** A dot drawn in every copy: a polar node, which has no waves of its own. */
export const CELL_EVERY = -1;
/** A dot drawn in no copy: a sample whose wave direction is not known. */
export const CELL_NONE = -2;

/**
 * The copy a wave direction belongs to, or -1 for none. `bearing` is where
 * the waves come from as seen from the boat, degrees clockwise from the bow.
 *
 * **Copy 0 is centred on the bow** and every copy on a whole number of
 * widths from it, so the boundaries are at half widths and none is ever on
 * 0°: waves from just either side of the bow are one population, and a
 * boundary there would cut it in two by noise.
 */
export function waveBucket(bearing: number, count: number, sense: WaveSense): number {
  if (!Number.isFinite(bearing)) return -1;
  const width = 360 / count;
  const direction = sense === "to" ? bearing + 180 : bearing;
  const turned = (((direction + width / 2) % 360) + 360) % 360;
  // A direction a rounding short of a full turn is the bow's bucket, not one past the last.
  return Math.min(count - 1, Math.floor(turned / width));
}

/** The direction a copy is centred on, degrees clockwise from the bow. */
export function bucketCentre(bucket: number, count: number): number {
  return bucket * (360 / count);
}

/**
 * The copy of each drawn dot. `refs` are the dots' global indices (nodes
 * first, then samples) and `bearings` the samples' wave bearings.
 */
export function waveCells(nodeCount: number, bearings: Float32Array, refs: Uint32Array, count: number, sense: WaveSense): Int16Array {
  const cells = new Int16Array(refs.length);
  for (let d = 0; d < refs.length; d++) {
    const g = refs[d]!;
    if (g < nodeCount) { cells[d] = CELL_EVERY; continue; }
    const bucket = waveBucket(bearings[g - nodeCount]!, count, sense);
    cells[d] = bucket < 0 ? CELL_NONE : bucket;
  }
  return cells;
}

/** How many samples each copy holds. */
export function cellTotals(cells: Int16Array, count: number): Uint32Array {
  const totals = new Uint32Array(count);
  for (let d = 0; d < cells.length; d++) {
    const cell = cells[d]!;
    if (cell >= 0 && cell < count) totals[cell]! += 1;
  }
  return totals;
}

/** A rectangle in CSS pixels from the canvas's top left. */
export interface Rect { x: number; y: number; width: number; height: number }

/**
 * How many copies across and down: the grid whose places are largest on
 * their shorter side, so no copy is a sliver. Among equals, the one with the
 * fewest empty places, then the one with more columns: a polar seen from
 * above is taller than it is wide.
 */
export function cellGrid(count: number, width: number, height: number): { cols: number; rows: number } {
  let best = { cols: 1, rows: Math.max(1, count) };
  let bestSide = -1, bestEmpty = Infinity;
  for (let cols = 1; cols <= count; cols++) {
    const rows = Math.ceil(count / cols);
    const side = Math.min(width / cols, height / rows);
    const empty = cols * rows - count;
    // Sides are compared to a hundredth of a pixel, so equal ones are equal.
    const better = side > bestSide + 0.01 || (Math.abs(side - bestSide) <= 0.01 && empty <= bestEmpty);
    if (better) { best = { cols, rows }; bestSide = side; bestEmpty = empty; }
  }
  return best;
}

/** Where copy `index` sits, in reading order inside `region`. */
export function cellRect(index: number, count: number, region: Rect): Rect {
  const { cols, rows } = cellGrid(count, region.width, region.height);
  const width = region.width / cols, height = region.height / rows;
  return { x: region.x + (index % cols) * width, y: region.y + Math.floor(index / cols) * height, width, height };
}

/** The copy under a point, or -1: outside the region, or a place past the last copy. */
export function cellAt(x: number, y: number, count: number, region: Rect): number {
  if (x < region.x || y < region.y || x >= region.x + region.width || y >= region.y + region.height) return -1;
  const { cols, rows } = cellGrid(count, region.width, region.height);
  const col = Math.min(cols - 1, Math.floor((x - region.x) / (region.width / cols)));
  const row = Math.min(rows - 1, Math.floor((y - region.y) / (region.height / rows)));
  const index = row * cols + col;
  return index < count ? index : -1;
}

/**
 * For a point hovered in copy `own`: in every other copy, the sample of
 * that copy nearest it in wind (within 5° and 1 kn, as the linked panes of
 * split view compare, `correspondingDot`), or -1. So the same wind can be
 * read across wave directions. `points` are the drawn dots' (TWA, TWS, BSP).
 */
export function correspondingInCells(points: Float32Array, cells: Int16Array, count: number, point: HoverPoint, own: number): Int32Array {
  const found = new Int32Array(count).fill(-1);
  const scores = new Float64Array(count).fill(Infinity);
  for (let d = 0; d < cells.length; d++) {
    const cell = cells[d]!;
    if (cell < 0 || cell >= count || cell === own) continue;
    const distance = Math.abs(points[d * 3]! - point.twa) % 360;
    const angle = Math.min(distance, 360 - distance);
    const wind = Math.abs(points[d * 3 + 1]! - point.tws);
    if (angle > 5 || wind > 1) continue;
    const score = angle * angle / 25 + wind * wind;
    if (score < scores[cell]!) { scores[cell] = score; found[cell] = d; }
  }
  return found;
}

/**
 * Which labels to keep so that none is written over another: each in turn,
 * unless its box meets one already kept. A copy of the view is small, and
 * the axis labels that had room in the whole view no longer do. A label off
 * the view (`null`) is not kept and is in nothing's way.
 */
export function uncrowded(boxes: readonly (Rect | null)[]): boolean[] {
  const kept: Rect[] = [];
  return boxes.map((box) => {
    if (!box) return false;
    const clear = kept.every((other) => box.x >= other.x + other.width || other.x >= box.x + box.width
      || box.y >= other.y + other.height || other.y >= box.y + box.height);
    if (clear) kept.push(box);
    return clear;
  });
}
