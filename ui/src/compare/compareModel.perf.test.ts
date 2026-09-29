/**
 * Compare at the 512 × 512 grid, timed against the spec.md 13 budget. Run
 * alone and serially by `npm run ui:perf` (vitest.perf.config.ts), never
 * in the parallel default suite (M17b review).
 */
import { describe, expect, it } from "vitest";

import { CLASS_A_ONLY, CLASS_B_ONLY, CLASS_BOTH, CLASS_NEITHER, CLASS_ZERO_ROW, emptyCompare, type ComparePacket } from "./comparePacket";
import { compareSurfaces, DEFAULT_COMPARE_TOGGLES, heatCellAt, heatImage, heatLayout, singleMarkers } from "./compareModel";
import { surfaceMesh } from "../polar/geometry3d";
import { PolarScene, SHAPE_CROSS, type RendererLike } from "../polar/scene3d";

const N = Number.NaN;

describe("Compare at 512 × 512", () => {
  it("builds and hovers a 512 × 512 comparison's heat map within the edit-to-view budget (spec.md 13)", () => {
    const n = 512;
    const cells = n * n;
    const values = (f: (k: number) => number) => Float32Array.from({ length: cells }, (_, k) => f(k));
    const big: ComparePacket = {
      ...emptyCompare(),
      twa: Float32Array.from({ length: n }, (_, i) => (i * 180) / (n - 1)),
      tws: Float32Array.from({ length: n }, (_, j) => (j * 70) / (n - 1)),
      a: values((k) => 5 + (k % 7) / 10),
      b: values((k) => (k % 11 === 0 ? N : 5 + (k % 5) / 10)),
      deltaKn: values((k) => (k % 11 === 0 ? N : (k % 7) / 10 - (k % 5) / 10)),
      deltaPct: values((k) => (k % 11 === 0 ? N : k % 13)),
      cls: Uint32Array.from({ length: cells }, (_, k) => (k < n ? CLASS_ZERO_ROW : k % 11 === 0 ? CLASS_A_ONLY : CLASS_BOTH)),
      kn: { meanAbs: 0.2, maxAbs: 0.6, min: -0.4, max: 0.6, maxCell: 0 },
    };
    const t0 = performance.now();
    const image = heatImage(big, false, "dark");
    const t1 = performance.now();
    const layout = heatLayout(image, 300);
    let found = 0;
    for (let k = 0; k < 1000; k++) if (heatCellAt(big, image, layout, false, 34 + (k % 266), 16 + (k % 470))) found++;
    const t2 = performance.now();
    const surfaces = compareSurfaces(big, DEFAULT_COMPARE_TOGGLES, { a: "#4e79a7", b: "#e15759" }, false, "dark");
    const surface = surfaces[2]!;
    const t3 = performance.now();
    const t4 = performance.now();
    for (const each of surfaces) surfaceMesh(each.grid, "tower");
    const t5 = performance.now();
    console.log(`512 × 512: meshes ${(t5 - t4).toFixed(1)} ms`);
    console.log(`512 × 512: heat image ${(t1 - t0).toFixed(1)} ms, 1000 hovers ${(t2 - t1).toFixed(1)} ms, three surfaces ${(t3 - t2).toFixed(1)} ms`);
    expect(image.rows).toBe(n - 1);
    expect(found).toBe(1000);
    expect(surface.vertexColors!.length).toBe(cells * 3);
    expect(t1 - t0).toBeLessThan(100);
    expect((t2 - t1) / 1000).toBeLessThan(1);
    // The three surfaces' input: 68-76 ms on nested rows before M17a,
    // about 35 ms on the packet's flat arrays (the difference surface's
    // colours are most of it).
    expect(t3 - t2).toBeLessThan(100);
  });

});

describe("the spec.md 13 budget at the 512 × 512 grid (M17b)", () => {
  it("builds Compare's three surfaces and their meshes in under 100 ms", () => {
    // A 512 × 512 comparison: most cells both (Δ spread over ±2 kn), a band
    // A only and a band B only (hatched, with crosses), a few neither, and
    // the 0° row, as the Rust packet classes them.
    const n = 512;
    const twa = Float32Array.from({ length: n }, (_, i) => (i * 180) / (n - 1));
    const tws = Float32Array.from({ length: n }, (_, j) => 4 + (j * 66) / (n - 1));
    const a = new Float32Array(n * n), b = new Float32Array(n * n);
    const deltaKn = new Float32Array(n * n).fill(N), deltaPct = new Float32Array(n * n).fill(N);
    const cls = new Uint32Array(n * n);
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        const k = i * n + j;
        const speed = 4 + (k % 13) / 5;
        if (i === 0) { cls[k] = CLASS_ZERO_ROW; continue; }
        if (k % 101 === 0) { cls[k] = CLASS_NEITHER; a[k] = N; b[k] = N; continue; }
        if (j < 8) { cls[k] = CLASS_A_ONLY; a[k] = speed; b[k] = N; continue; }
        if (j >= n - 8) { cls[k] = CLASS_B_ONLY; a[k] = N; b[k] = speed; continue; }
        cls[k] = CLASS_BOTH;
        a[k] = speed;
        b[k] = speed - 2 + (k % 41) / 10;
        deltaKn[k] = a[k]! - b[k]!;
        deltaPct[k] = (100 * deltaKn[k]!) / b[k]!;
      }
    }
    const big: ComparePacket = {
      ...emptyCompare(), twa, tws, a, b, deltaKn, deltaPct, cls,
      kn: { meanAbs: 1, maxAbs: 2, min: -2, max: 2, maxCell: 1 },
      pct: { meanAbs: 20, maxAbs: 80, min: -40, max: 80, maxCell: 1 },
    };
    const renderer: RendererLike = {
      setPixelRatio: () => undefined, getPixelRatio: () => 1, setSize: () => undefined, render: () => undefined, dispose: () => undefined,
    };
    const scene = new PolarScene({} as HTMLCanvasElement, renderer);
    scene.resize(200, 200);
    const times: number[] = [];
    for (let run = 0; run < 12; run++) {
      // As CompareView does whenever the packet, a toggle or the scheme changes.
      const t0 = performance.now();
      const markers = singleMarkers(big, "dark");
      scene.setData({
        samples: markers.points, colors: markers.colors, shapes: new Float32Array(markers.count).fill(SHAPE_CROSS),
        layout: "tower",
        surfaces: compareSurfaces(big, DEFAULT_COMPARE_TOGGLES, { a: "#4e79a7", b: "#f28e2b" }, false, "dark"),
      });
      times.push(performance.now() - t0);
    }
    console.log(`Compare, three 512 × 512 surfaces, input + setData: ${times.map((t) => t.toFixed(0)).join(", ")} ms`);
    expect(scene.dotCount).toBe(2 * 8 * (n - 1) - [...cls].filter((c, k) => c === CLASS_NEITHER && (k % n < 8 || k % n >= n - 8)).length);
    // Min of the later runs: the first pays for compiling the loops, and an
    // edit or a toggle is a later one.
    expect(Math.min(...times.slice(1))).toBeLessThan(100);
  });
});
