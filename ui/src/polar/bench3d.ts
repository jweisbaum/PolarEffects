/**
 * Development-only benchmark for the 3D view budget (spec.md 13: 60 fps with
 * 200,000 dots and 20 surfaces). Served by the Vite dev server at
 * `/bench3d.html`; never part of the app bundle, whose only entry is
 * `index.html`.
 *
 * It builds the scene, orbits the camera for five seconds while counting
 * frames, then times a lasso over every dot. Results go to `window.__bench`
 * and the page, so a person or a DevTools-protocol script can read them.
 */
import { PolarScene } from "./scene3d";
import { random, syntheticGrid, syntheticSamples } from "./geometry3d";

const DOTS = Number(new URLSearchParams(location.search).get("dots") ?? 200_000);
const SURFACES = Number(new URLSearchParams(location.search).get("surfaces") ?? 20);
const SECONDS = Number(new URLSearchParams(location.search).get("seconds") ?? 5);

interface Result {
  dots: number;
  surfaces: number;
  renderer: string;
  buildDotsMs: number;
  buildSurfacesMs: number;
  firstFrameMs: number;
  fps: number;
  frameP50Ms: number;
  frameP95Ms: number;
  frameP99Ms: number;
  frameMaxMs: number;
  framesOver20Ms: number;
  frames: number;
  lassoMs: number;
  lassoSelected: number;
  width: number;
  height: number;
}

declare global {
  interface Window { __bench?: Result | { error: string } }
}

function percentile(sorted: number[], p: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))] ?? NaN;
}

async function run(): Promise<Result> {
  const canvas = document.querySelector("canvas")!;
  const scene = new PolarScene(canvas);
  const { width, height } = canvas.getBoundingClientRect();
  scene.resize(width, height);

  const samples = syntheticSamples(DOTS, 1);
  const rnd = random(2);
  const palette = Array.from({ length: 50 }, () => [rnd(), rnd(), rnd()] as const);
  const colors = new Float32Array(DOTS * 3);
  for (let i = 0; i < DOTS; i++) colors.set(palette[i % 50]!, i * 3);
  const surfaces = Array.from({ length: SURFACES }, (_, k) => ({
    grid: syntheticGrid(0.85 + (0.3 * k) / Math.max(1, SURFACES - 1)),
    color: `#${Math.floor(rnd() * 0xffffff).toString(16).padStart(6, "0")}`,
  }));
  const build = scene.setData({ samples, colors, surfaces, layout: "tower" });

  const gl = scene.renderer.getContext();
  const info = gl.getExtension("WEBGL_debug_renderer_info");
  const renderer = info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : String(gl.getParameter(gl.RENDERER));

  const t0 = performance.now();
  scene.render();
  gl.finish();
  const firstFrameMs = performance.now() - t0;

  const frames: number[] = [];
  await new Promise<void>(resolve => {
    let last = performance.now();
    const start = last;
    const tick = (now: number) => {
      frames.push(now - last);
      last = now;
      scene.orbit(Math.PI / 4 + ((now - start) / 1000) * 0.6);
      scene.render();
      if (now - start < SECONDS * 1000) requestAnimationFrame(tick);
      else resolve();
    };
    requestAnimationFrame(tick);
  });
  frames.shift();
  const sorted = [...frames].sort((a, b) => a - b);
  const total = frames.reduce((a, b) => a + b, 0);

  // A lasso across the middle third of the view, timed over ten runs.
  const lasso = [width / 3, height / 3, (2 * width) / 3, height / 3, (2 * width) / 3, (2 * height) / 3, width / 3, (2 * height) / 3];
  let selected: Uint32Array = new Uint32Array(0);
  const l0 = performance.now();
  for (let i = 0; i < 10; i++) selected = scene.lasso(lasso);
  const lassoMs = (performance.now() - l0) / 10;
  scene.setSelection(selected);
  scene.render();

  return {
    dots: DOTS, surfaces: SURFACES, renderer,
    buildDotsMs: build.dots, buildSurfacesMs: build.surfaces, firstFrameMs,
    fps: (frames.length / total) * 1000, frameP50Ms: percentile(sorted, 0.5), frameP95Ms: percentile(sorted, 0.95),
    frameP99Ms: percentile(sorted, 0.99), frameMaxMs: sorted[sorted.length - 1] ?? NaN,
    framesOver20Ms: frames.filter(f => f > 20).length, frames: frames.length,
    lassoMs, lassoSelected: selected.length, width, height,
  };
}

run().then(
  result => {
    window.__bench = result;
    document.querySelector("pre")!.textContent = JSON.stringify(result, null, 2);
    document.title = "bench:done";
  },
  (error: unknown) => {
    window.__bench = { error: String(error) };
    document.title = "bench:failed";
  },
);
