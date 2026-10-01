/**
 * Polar files: Import… (the native picker answered by the dev-only queue,
 * `ui/src/automation.ts`) → the file's row → a curve in its colour on the
 * 2D plot → the 3D stage draws.
 */
import { assert, colourPixels, distinctColours, newProject, toHex } from "./harness.mjs";

export default {
  name: "polar import",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Polar import");
    await d.queueDialog([t.path("polar_examples/polars/Farr 40.txt")]);
    await d.click('[data-feature="polar-files:import"]');
    await d.waitFor(".polar-file-list li", { text: "Farr 40" });
    assert.equal(await d.count(".polar-file-list li"), 1);
    const colour = toHex(await d.run(
      `done(getComputedStyle(document.querySelector(".polar-file-list .swatch")).backgroundColor);`));
    await t.shot("imported");

    const plot = ".polar-plot-panel canvas";
    await d.waitFor(plot, { visible: true });
    let curve = 0;
    for (let i = 0; i < 50 && curve < 50; i += 1) {
      curve = await colourPixels(d, plot, colour, 30);
      if (curve < 50) await new Promise((r) => setTimeout(r, 200));
    }
    assert.ok(curve >= 50, `the 2D plot shows a curve in ${colour} (${curve} pixels)`);
    await t.shot("plot-curve");
    await d.click('[data-feature="plot:full-size"]');
    await d.waitFor(".polar-plot-overlay canvas", { visible: true });
    assert.equal(await d.exists('[data-feature="stage:map"]'), false);
    await t.shot("full-size-without-tracks");
    await d.click('[data-feature="plot:close"]');
    await d.waitGone(".polar-plot-overlay");

    await d.click('[data-feature="stage:3d"]');
    await d.waitFor("canvas.view3d-canvas", { visible: true });
    // The surface is drawn in the blend's colour over the file's nodes: wait
    // for the scene to arrive and cover part of the view, not only the guides.
    const blend = toHex(await d.run(
      `done(getComputedStyle(document.querySelector('[data-feature="sources:blend-colour"]')).backgroundColor);`));
    let surface = 0;
    for (let i = 0; i < 75 && surface < 5000; i += 1) {
      surface = await colourPixels(d, "canvas.view3d-canvas", blend, 40);
      if (surface < 5000) await new Promise((r) => setTimeout(r, 200));
    }
    assert.ok(surface >= 5000, `the 3D view draws the surface in ${blend} (${surface} pixels)`);
    assert.ok((await distinctColours(d, "canvas.view3d-canvas")) >= 8, "the 3D view is not blank");
    await t.shot("stage-3d");
  },
};
