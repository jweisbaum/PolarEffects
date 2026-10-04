/**
 * Split Wave Angle (spec.md 10.5): which copy a sample belongs to, how the
 * copies are laid out, and which dots answer a hover in the other copies.
 * References are worked by hand on the compass rose.
 */
import { describe, expect, it } from "vitest";

import {
  bucketCentre, uncrowded, cellAt, cellGrid, cellRect, CELL_EVERY, CELL_NONE, correspondingInCells, waveBucket, waveCells,
  WAVE_SPLIT_COUNTS,
} from "./waveSplit";

describe("waveBucket", () => {
  it("offers 4, 8, 16, 18, 24 and 36 directions", () => {
    expect([...WAVE_SPLIT_COUNTS]).toEqual([4, 8, 16, 18, 24, 36]);
  });

  it("centres a bucket on the bow, so no boundary is ever on 0°", () => {
    for (const count of WAVE_SPLIT_COUNTS) {
      // Just either side of the bow is one bucket, the first.
      expect(waveBucket(0, count, "from")).toBe(0);
      expect(waveBucket(0.001, count, "from")).toBe(0);
      expect(waveBucket(359.999, count, "from")).toBe(0);
      // Its edges are half a bucket either side, and the same for every bucket.
      const width = 360 / count;
      expect(waveBucket(width / 2 - 0.001, count, "from")).toBe(0);
      expect(waveBucket(width / 2, count, "from")).toBe(1);
      expect(waveBucket(360 - width / 2, count, "from")).toBe(0);
      expect(waveBucket(360 - width / 2 - 0.001, count, "from")).toBe(count - 1);
      // No boundary, (k + ½) widths, lands on 0° or 360°.
      for (let k = 0; k < count; k++) expect(((k + 0.5) * width) % 360).not.toBe(0);
      // Every bucket is centred on a whole number of widths.
      for (let k = 0; k < count; k++) {
        expect(bucketCentre(k, count)).toBe(k * width);
        expect(waveBucket(k * width, count, "from")).toBe(k);
      }
    }
  });

  it("names the four quarters by where the waves come from", () => {
    // Four directions: bow, starboard beam, stern, port beam.
    expect([0, 44.9, 315, 330].map((b) => waveBucket(b, 4, "from"))).toEqual([0, 0, 0, 0]);
    expect([45, 90, 134.9].map((b) => waveBucket(b, 4, "from"))).toEqual([1, 1, 1]);
    expect([135, 180, 224.9].map((b) => waveBucket(b, 4, "from"))).toEqual([2, 2, 2]);
    expect([225, 270, 314.9].map((b) => waveBucket(b, 4, "from"))).toEqual([3, 3, 3]);
    // The circle closes: a full turn is the bow again.
    expect(waveBucket(360, 4, "from")).toBe(0);
    expect(waveBucket(720 + 90, 4, "from")).toBe(1);
  });

  it("turns the direction round for To: waves from the bow go to the stern", () => {
    expect(waveBucket(0, 4, "to")).toBe(2);
    expect(waveBucket(90, 4, "to")).toBe(3);
    expect(waveBucket(180, 4, "to")).toBe(0);
    expect(waveBucket(270, 8, "to")).toBe(2);
    for (const count of WAVE_SPLIT_COUNTS) {
      // Waves from dead astern go to the bow, in the bucket that straddles 0°.
      expect(waveBucket(180, count, "to")).toBe(0);
      expect(waveBucket(179.999, count, "to")).toBe(0);
      expect(waveBucket(180.001, count, "to")).toBe(0);
    }
  });

  it("has no bucket for a sample with no wave direction", () => {
    expect(waveBucket(Number.NaN, 8, "from")).toBe(-1);
    expect(waveBucket(Number.POSITIVE_INFINITY, 8, "to")).toBe(-1);
  });
});

describe("waveCells", () => {
  it("puts polar nodes in every copy, samples in their direction's, and directionless samples in none", () => {
    // Two nodes, then samples with bearings 10°, 100°, none, 350°.
    const bearings = Float32Array.from([10, 100, Number.NaN, 350]);
    const refs = Uint32Array.from([0, 1, 2, 3, 4, 5]);
    expect([...waveCells(2, bearings, refs, 4, "from")]).toEqual([CELL_EVERY, CELL_EVERY, 0, 1, CELL_NONE, 0]);
    expect([...waveCells(2, bearings, refs, 4, "to")]).toEqual([CELL_EVERY, CELL_EVERY, 2, 3, CELL_NONE, 2]);
    // Only the dots drawn are given a copy, in the order drawn.
    expect([...waveCells(2, bearings, Uint32Array.from([1, 5, 3]), 4, "from")]).toEqual([CELL_EVERY, 0, 1]);
  });
});

describe("the grid of copies", () => {
  it("lays the copies out as near to square as the space allows", () => {
    // A wide stage: 1200 × 600.
    expect(cellGrid(4, 1200, 600)).toEqual({ cols: 4, rows: 1 });
    expect(cellGrid(8, 1200, 600)).toEqual({ cols: 4, rows: 2 });
    expect(cellGrid(16, 1200, 600)).toEqual({ cols: 6, rows: 3 });
    expect(cellGrid(18, 1200, 600)).toEqual({ cols: 6, rows: 3 });
    expect(cellGrid(24, 1200, 600)).toEqual({ cols: 8, rows: 3 });
    expect(cellGrid(36, 1200, 600)).toEqual({ cols: 9, rows: 4 });
    // A square stage.
    expect(cellGrid(4, 800, 800)).toEqual({ cols: 2, rows: 2 });
    expect(cellGrid(16, 800, 800)).toEqual({ cols: 4, rows: 4 });
    expect(cellGrid(36, 800, 800)).toEqual({ cols: 6, rows: 6 });
    // A tall one.
    expect(cellGrid(8, 400, 800)).toEqual({ cols: 2, rows: 4 });
    // Never fewer places than copies, whatever the shape.
    for (const count of WAVE_SPLIT_COUNTS) {
      for (const [w, h] of [[1200, 600], [300, 900], [1, 1], [0, 0], [5000, 40]] as const) {
        const { cols, rows } = cellGrid(count, w, h);
        expect(cols * rows).toBeGreaterThanOrEqual(count);
        expect(cols).toBeGreaterThanOrEqual(1);
      }
    }
  });

  it("places copy k in reading order inside the region, and finds the copy under a point", () => {
    const region = { x: 100, y: 50, width: 800, height: 400 };
    // 8 copies in 800 × 400: four across, two down, each 200 × 200.
    expect(cellRect(0, 8, region)).toEqual({ x: 100, y: 50, width: 200, height: 200 });
    expect(cellRect(3, 8, region)).toEqual({ x: 700, y: 50, width: 200, height: 200 });
    expect(cellRect(4, 8, region)).toEqual({ x: 100, y: 250, width: 200, height: 200 });
    expect(cellRect(7, 8, region)).toEqual({ x: 700, y: 250, width: 200, height: 200 });
    expect(cellAt(101, 51, 8, region)).toBe(0);
    expect(cellAt(899, 449, 8, region)).toBe(7);
    expect(cellAt(300, 250, 8, region)).toBe(5);
    // Outside the region, or in a place of the grid past the last copy, is no copy.
    expect(cellAt(99, 100, 8, region)).toBe(-1);
    expect(cellAt(500, 451, 8, region)).toBe(-1);
    // Sixteen copies in 1200 × 600 are six across and three down: two places stay empty.
    const wide = { x: 0, y: 0, width: 1200, height: 600 };
    expect(cellAt(1199, 599, 16, wide)).toBe(-1);
    expect(cellAt(799, 599, 16, wide)).toBe(15);
  });
});

describe("correspondingInCells", () => {
  it("finds, in every other copy, the dot nearest the hovered wind", () => {
    // (TWA, TWS, BSP) per dot; the copy each is in.
    const points = Float32Array.from([
      90, 10, 7,   // 0: copy 0, the hovered one
      91, 10.3, 6, // 1: copy 1, near
      95, 10, 5,   // 2: copy 1, farther
      90, 16, 8,   // 3: copy 2, too far in wind speed
      88, 9.5, 9,  // 4: every copy (a node): not a sample of any copy's own
      270, 10, 4,  // 5: copy 3, the other side: not near
    ]);
    const cells = Int16Array.from([0, 1, 1, 2, CELL_EVERY, 3]);
    const found = correspondingInCells(points, cells, 4, { twa: 90, tws: 10 }, 0);
    expect([...found]).toEqual([-1, 1, -1, -1]);
    // From copy 1's side, copy 0's dot answers.
    expect([...correspondingInCells(points, cells, 4, { twa: 91, tws: 10.3 }, 1)]).toEqual([0, -1, -1, -1]);
  });
});

describe("uncrowded", () => {
  it("keeps each label in turn unless it would be written over one already kept", () => {
    const box = (x: number, y: number, width = 30, height = 12) => ({ x, y, width, height });
    // Speed labels along a short radius: 2 kn, 4 kn, 6 kn … 10 px apart, each 30 px wide.
    expect(uncrowded([box(0, 0), box(10, 0), box(20, 0), box(30, 0), box(40, 0)])).toEqual([true, false, false, true, false]);
    // A line below is clear of the one above; a label off the view (null) is neither kept nor in the way.
    expect(uncrowded([box(0, 0), null, box(0, 12), box(5, 6)])).toEqual([true, false, true, false]);
    // Room for all: all kept.
    expect(uncrowded([box(0, 0), box(40, 0), box(80, 0)])).toEqual([true, true, true]);
  });
});
