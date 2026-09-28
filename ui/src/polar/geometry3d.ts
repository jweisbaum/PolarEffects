/**
 * Geometry for the 3D polar view (spec.md 10.1), kept free of three.js so it
 * can be tested and measured in Node.
 *
 * Model space follows the spec. **Polar tower** (default): x = BSP·sin(TWA),
 * y = BSP·cos(TWA), z = TWS, so each TWS is a classic polar curve and the
 * stack is a surface. **Cartesian**: x = TWA, y = TWS, z = BSP. The scene
 * scales TWA in the Cartesian layout so both layouts have similar extents.
 *
 * Everything is flat typed arrays: 200,000 dots are 600,000 floats, and a
 * per-dot object would cost more to build than to draw.
 */

export type Layout = "tower" | "cartesian";

/** Degrees of TWA per model unit in the Cartesian layout. */
export const CARTESIAN_TWA_SCALE = 10;

const RAD = Math.PI / 180;

/** Model-space position of one (TWA°, TWS kn, BSP kn) triple. */
export function place(twa: number, tws: number, bsp: number, layout: Layout): [number, number, number] {
  if (layout === "cartesian") return [twa / CARTESIAN_TWA_SCALE, tws, bsp];
  return [bsp * Math.sin(twa * RAD), bsp * Math.cos(twa * RAD), tws];
}

/**
 * Positions for every dot. `samples` holds (TWA, TWS, BSP) triples; the
 * result holds (x, y, z) triples in the same order, so a dot's index is its
 * sample's index everywhere (selection, colours, exclusions).
 */
export function dotPositions(
  samples: Float32Array,
  layout: Layout,
  out?: Float32Array<ArrayBuffer>,
): Float32Array<ArrayBuffer> {
  const n = Math.floor(samples.length / 3);
  const positions = out && out.length >= n * 3 ? out : new Float32Array(n * 3);
  // `place` inlined: a tuple per dot is 200,000 allocations per rebuild.
  const tower = layout === "tower";
  for (let i = 0; i < n; i++) {
    const twa = samples[i * 3]!, tws = samples[i * 3 + 1]!, bsp = samples[i * 3 + 2]!;
    if (tower) {
      positions[i * 3] = bsp * Math.sin(twa * RAD);
      positions[i * 3 + 1] = bsp * Math.cos(twa * RAD);
      positions[i * 3 + 2] = tws;
    } else {
      positions[i * 3] = twa / CARTESIAN_TWA_SCALE;
      positions[i * 3 + 1] = tws;
      positions[i * 3 + 2] = bsp;
    }
  }
  return positions;
}

/** A polar source as a grid: `bsp[j][i]` is BSP at `tws[j]`, `twa[i]`; null is an empty cell. */
export interface PolarGrid {
  twa: readonly number[];
  tws: readonly number[];
  bsp: readonly (readonly (number | null)[])[];
}

/** An indexed triangle mesh and the grid lines drawn on it. */
export interface SurfaceMesh {
  positions: Float32Array<ArrayBuffer>;
  /** Triangles, three vertex indices each. */
  triangles: Uint32Array<ArrayBuffer>;
  /** Grid line segments, two vertex indices each. */
  lines: Uint32Array<ArrayBuffer>;
}

/**
 * The surface through a polar grid. A quad is drawn only where all four
 * corners have a value, so an empty cell leaves a hole rather than a wall
 * down to zero. Lines follow each TWS curve and each TWA ray.
 */
export function surfaceMesh(grid: PolarGrid, layout: Layout): SurfaceMesh {
  const ni = grid.twa.length;
  const nj = grid.tws.length;
  const positions = new Float32Array(ni * nj * 3);
  const has = new Uint8Array(ni * nj);
  for (let j = 0; j < nj; j++) {
    for (let i = 0; i < ni; i++) {
      const v = grid.bsp[j]?.[i];
      const k = j * ni + i;
      if (v === null || v === undefined || !Number.isFinite(v)) continue;
      has[k] = 1;
      const [x, y, z] = place(grid.twa[i]!, grid.tws[j]!, v, layout);
      positions[k * 3] = x;
      positions[k * 3 + 1] = y;
      positions[k * 3 + 2] = z;
    }
  }
  const triangles: number[] = [];
  const lines: number[] = [];
  for (let j = 0; j < nj; j++) {
    for (let i = 0; i < ni; i++) {
      const a = j * ni + i;
      if (i + 1 < ni && has[a] && has[a + 1]) lines.push(a, a + 1);
      if (j + 1 < nj && has[a] && has[a + ni]) lines.push(a, a + ni);
      if (i + 1 < ni && j + 1 < nj) {
        const b = a + 1, c = a + ni, d = a + ni + 1;
        if (has[a] && has[b] && has[c] && has[d]) triangles.push(a, b, d, a, d, c);
      }
    }
  }
  return { positions, triangles: Uint32Array.from(triangles), lines: Uint32Array.from(lines) };
}

/**
 * Projects model-space points to screen pixels with a column-major 4×4
 * model-view-projection matrix (three.js `Matrix4.elements`). Points behind
 * the camera or outside the depth range get NaN, so no lasso can take them.
 */
export function project(
  positions: Float32Array,
  mvp: ArrayLike<number>,
  width: number,
  height: number,
  out?: Float32Array<ArrayBuffer>,
): Float32Array<ArrayBuffer> {
  const n = Math.floor(positions.length / 3);
  const screen = out && out.length >= n * 2 ? out : new Float32Array(n * 2);
  const m = mvp;
  const m0 = m[0]!, m1 = m[1]!, m3 = m[3]!, m4 = m[4]!, m5 = m[5]!, m7 = m[7]!;
  const m8 = m[8]!, m9 = m[9]!, m11 = m[11]!, m12 = m[12]!, m13 = m[13]!, m15 = m[15]!;
  const m2 = m[2]!, m6 = m[6]!, m10 = m[10]!, m14 = m[14]!;
  const hw = width / 2, hh = height / 2;
  for (let i = 0; i < n; i++) {
    const x = positions[i * 3]!, y = positions[i * 3 + 1]!, z = positions[i * 3 + 2]!;
    const w = m3 * x + m7 * y + m11 * z + m15;
    const cz = m2 * x + m6 * y + m10 * z + m14;
    if (w <= 0 || cz < -w || cz > w) {
      screen[i * 2] = NaN;
      screen[i * 2 + 1] = NaN;
      continue;
    }
    const cx = (m0 * x + m4 * y + m8 * z + m12) / w;
    const cy = (m1 * x + m5 * y + m9 * z + m13) / w;
    screen[i * 2] = (cx + 1) * hw;
    // Screen y grows downward, as pointer events report it.
    screen[i * 2 + 1] = (1 - cy) * hh;
  }
  return screen;
}

/** Even-odd point-in-polygon test. */
export function inside(px: number, py: number, polygon: ArrayLike<number>): boolean {
  let hit = false;
  const n = polygon.length / 2;
  for (let i = 0, j = n - 1; i < n; j = i++) {
    const xi = polygon[i * 2]!, yi = polygon[i * 2 + 1]!;
    const xj = polygon[j * 2]!, yj = polygon[j * 2 + 1]!;
    if (yi > py !== yj > py && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

/**
 * Indices of the projected points inside a screen-space lasso, given as
 * flat (x, y) vertex pairs. A bounding-box test rejects most points before
 * the polygon test runs.
 */
export function lassoSelect(screen: Float32Array, polygon: ArrayLike<number>): Uint32Array<ArrayBuffer> {
  const n = Math.floor(screen.length / 2);
  if (polygon.length < 6) return new Uint32Array(0);
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (let i = 0; i < polygon.length; i += 2) {
    const x = polygon[i]!, y = polygon[i + 1]!;
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  const hits: number[] = [];
  for (let i = 0; i < n; i++) {
    const x = screen[i * 2]!, y = screen[i * 2 + 1]!;
    // NaN fails every comparison, so points behind the camera drop out here.
    if (!(x >= minX && x <= maxX && y >= minY && y <= maxY)) continue;
    if (inside(x, y, polygon)) hits.push(i);
  }
  return Uint32Array.from(hits);
}

/** A small deterministic generator (mulberry32), so test and benchmark data repeat exactly. */
export function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** A plausible boat's speed: rises with TWS, dips upwind and dead downwind. */
export function syntheticBsp(twa: number, tws: number, scale = 1): number {
  const s = Math.min(tws, 25) / 25;
  const angle = Math.sin(Math.min(Math.max(twa, 30), 170) * RAD);
  return scale * (2 + 8 * Math.sqrt(s) * (0.55 + 0.45 * angle));
}

/** `n` synthetic samples as (TWA, TWS, BSP) triples with scatter, for tests and the benchmark. */
export function syntheticSamples(n: number, seed = 1): Float32Array<ArrayBuffer> {
  const rnd = random(seed);
  const out = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    const twa = 30 + rnd() * 150;
    const tws = 4 + rnd() * 26;
    out[i * 3] = twa;
    out[i * 3 + 1] = tws;
    out[i * 3 + 2] = syntheticBsp(twa, tws) * (0.8 + rnd() * 0.3);
  }
  return out;
}

/** A synthetic polar grid on the usual 5° × 2 kn axes. */
export function syntheticGrid(scale: number): PolarGrid {
  const twa = Array.from({ length: 37 }, (_, i) => i * 5);
  const tws = Array.from({ length: 14 }, (_, j) => 4 + j * 2);
  const bsp = tws.map(s => twa.map(a => (a < 30 ? null : syntheticBsp(a, s, scale))));
  return { twa, tws, bsp };
}
