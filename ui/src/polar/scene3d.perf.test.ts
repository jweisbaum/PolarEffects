/**
 * The spec.md 13 edit-to-view budget at the 512 × 512 grid, timed. Run
 * alone and serially by `npm run ui:perf` (vitest.perf.config.ts), never
 * in the parallel default suite, where other workers' load made a 100 ms
 * bound flaky (M17b review).
 */
import { describe, expect, it } from "vitest";

import { PolarScene, type RendererLike } from "./scene3d";
import { surfaceGrid } from "./view3d";

function make() {
  const renderer: RendererLike = {
    setPixelRatio: () => undefined, getPixelRatio: () => 1, setSize: () => undefined, render: () => undefined, dispose: () => undefined,
  };
  const scene = new PolarScene({} as HTMLCanvasElement, renderer);
  scene.resize(200, 200);
  return { scene, renderer };
}

const three = (n: number) => new Float32Array(n * 3);

describe("the spec.md 13 budget at the 512 × 512 grid (M17a)", () => {
  it("rebuilds two 512 × 512 surfaces from packet arrays in under 100 ms", () => {
    const n = 512;
    const twa = Float32Array.from({ length: n }, (_, i) => (i * 180) / (n - 1));
    const tws = Float32Array.from({ length: n }, (_, j) => 4 + (j * 66) / (n - 1));
    const bsp = Float32Array.from({ length: n * n }, (_, k) => (k % 97 === 0 ? Number.NaN : 4 + (k % 13) / 5));
    const { scene } = make();
    const times: number[] = [];
    for (let run = 0; run < 5; run++) {
      // As PolarView does on every edit: the packet's arrays to the scene's input, then setData.
      const t0 = performance.now();
      scene.setData({
        samples: three(0), colors: three(0), layout: "tower",
        surfaces: [
          { grid: surfaceGrid(twa, tws, bsp), color: "#4e79a7" },
          { grid: surfaceGrid(twa, tws, bsp), color: "#ffffff", opaque: true },
        ],
      });
      times.push(performance.now() - t0);
    }
    console.log(`two 512 × 512 surfaces, input + setData: ${times.map((t) => t.toFixed(0)).join(", ")} ms`);
    // The first run pays for compiling the loops; an edit is a later one.
    expect(Math.min(...times.slice(1))).toBeLessThan(100);
  });
});
