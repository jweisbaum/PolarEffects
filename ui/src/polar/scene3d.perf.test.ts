/**
 * The spec.md 13 edit-to-view budget at the 512 × 512 grid, timed. Run
 * alone and serially by `npm run ui:perf` (vitest.perf.config.ts), never
 * in the parallel default suite, where other workers' load made a 100 ms
 * bound flaky (M17b review).
 */
import { describe, expect, it } from "vitest";

import { PolarScene, type RendererLike } from "./scene3d";
import { DEFAULT_TOGGLES, sampleDots, surfaceGrid } from "./view3d";
import { packSynthetic } from "./benchPacket";
import { unpackScene } from "./scenePacket";
import { NO_WAVE_RANGES, type WaveRanges } from "./waveRanges";

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
  it("updates three display ranges over 200k samples in under 100 ms", () => {
    const packet = unpackScene(packSynthetic(Float32Array.from({ length: 200_000 * 3 }, (_, k) => k % 3 === 0 ? k % 180 : 6 + k % 12)));
    for (let k = 0; k < packet.samples.count; k++) {
      packet.samples.hs[k] = (k % 10) / 10;
      packet.samples.waveAngle[k] = k % 180;
      packet.samples.wavePeriod[k] = 5 + k % 15;
    }
    const ranges: WaveRanges = { hs: { min: 0.2, max: 0.8 }, waveAngle: { min: 30, max: 150 }, wavePeriod: { min: 7, max: 16 } };
    const { scene } = make();
    try {
      for (const [label, limits] of [["all samples", NO_WAVE_RANGES], ["three wave ranges", ranges]] as const) {
        const times: number[] = [];
        for (let run = 0; run < 5; run++) {
          const t0 = performance.now();
          const dots = sampleDots(packet, DEFAULT_TOGGLES, "source", null, limits);
          scene.setData({ samples: dots.points, colors: dots.colors, shapes: dots.shapes, layout: "tower", surfaces: [] });
          times.push(performance.now() - t0);
          expect(dots.refs.length).toBeGreaterThan(0);
          if (limits === ranges) expect(dots.refs.length).toBeLessThan(packet.samples.count);
        }
        console.log(`200k samples, ${label}, dots + setData: ${times.map(t => t.toFixed(0)).join(", ")} ms`);
        expect(Math.min(...times.slice(1))).toBeLessThan(100);
      }
    } finally { scene.dispose(); }
  });

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
