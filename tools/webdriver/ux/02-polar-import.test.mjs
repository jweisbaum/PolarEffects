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

    // The plot is the 2D stage now (asked 2026-10-02).
    await d.click('[data-feature="stage:2d"]');
    const plot = ".polar-plot-stage canvas";
    await d.waitFor(plot, { visible: true });
    let curve = 0;
    for (let i = 0; i < 50 && curve < 50; i += 1) {
      curve = await colourPixels(d, plot, colour, 30);
      if (curve < 50) await new Promise((r) => setTimeout(r, 200));
    }
    assert.ok(curve >= 50, `the 2D plot shows a curve in ${colour} (${curve} pixels)`);
    await t.shot("plot-curve");
    assert.equal(await d.exists('[data-feature="stage:map"]'), false);
    // On the stage a symmetric polar's 0° axis runs down the middle (asked
    // 2026-10-02): the pointer finds the fan to the right of centre and
    // nothing to its left.
    const halves = await d.run(`var c = document.querySelector(".polar-plot-stage canvas"), ctx = c.getContext("2d");
      var data = ctx.getImageData(0, 0, c.width, c.height).data, bg = [data[0], data[1], data[2]], left = 0, right = 0;
      for (var y = 0; y < c.height; y += 2) for (var x = 0; x < c.width; x += 2) {
        var i = (y * c.width + x) * 4;
        if (Math.abs(data[i] - bg[0]) + Math.abs(data[i + 1] - bg[1]) + Math.abs(data[i + 2] - bg[2]) > 60) { if (x < c.width / 2) left++; else right++; }
      }
      done({ left: left, right: right });`);
    assert.ok(halves.right > halves.left * 3, `the fan is to the right of the centre line: ${JSON.stringify(halves)}`);
    await t.shot("plot-stage-without-tracks");
    await d.click('[data-feature="stage:3d"]');
    await d.waitGone(".stage-slot:not([hidden]) .polar-plot-stage"); // kept mounted, hidden (asked 2026-10-04)

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
