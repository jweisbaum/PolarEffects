/**
 * CPU cost of the 3D view's per-edit and per-lasso work at the spec.md 13
 * scale (200,000 dots, 20 surfaces). Not part of `npm run ui:test`:
 *
 *   npx vitest bench --run src/polar/geometry3d.bench.ts
 */
import { bench, describe } from "vitest";
import { dotPositions, lassoSelect, project, surfaceMesh, syntheticGrid, syntheticSamples } from "./geometry3d";

const samples = syntheticSamples(200_000, 1);
const positions = dotPositions(samples, "tower");
const grids = Array.from({ length: 20 }, (_, k) => syntheticGrid(0.85 + 0.015 * k));
// A perspective-like matrix looking down the TWS axis from above, 1280 × 800.
const mvp = [0.07, 0, 0, 0, 0, 0.11, 0, 0, 0, 0, -0.01, -0.02, 0, 0, 0.5, 1];
const screen = project(positions, mvp, 1280, 800);
const lasso: number[] = [];
for (let k = 0; k < 64; k++) {
  const a = (k / 64) * 2 * Math.PI;
  lasso.push(640 + 300 * Math.cos(a) * (0.7 + 0.3 * Math.sin(3 * a)), 400 + 250 * Math.sin(a));
}

describe("3D view, 200,000 dots and 20 surfaces", () => {
  bench("dot positions (tower)", () => { dotPositions(samples, "tower"); });
  bench("20 surface meshes", () => { for (const g of grids) surfaceMesh(g, "tower"); });
  bench("project 200k dots", () => { project(positions, mvp, 1280, 800, screen); });
  bench("lasso select over 200k projected dots (64-vertex lasso)", () => { lassoSelect(screen, lasso); });
  bench("project + lasso (one lasso gesture)", () => { lassoSelect(project(positions, mvp, 1280, 800, screen), lasso); });
});
