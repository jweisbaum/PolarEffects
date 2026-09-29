/**
 * Compare (spec.md 11): two polar files → a source row's Compare opens the
 * stage with that source as A and the blend as B → the summary, the legend,
 * the heat map and the 3D difference surface show what Rust computed → B is
 * picked from the colour-and-name list → Δ in percent → swap → the light
 * theme keeps the scale readable.
 */
import { assert, distinctColours, newProject } from "./harness.mjs";

/** The largest |Δ| the summary names, as a number, and its unit. */
async function summary(d) {
  return d.text(".compare-summary");
}

export default {
  name: "compare",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Compare");
    await d.queueDialog([t.path("polar_examples/polars/Farr 40.txt"), t.path("polar_examples/polars/Class 40.txt"),
      t.path("polar_examples/polars/Cal 40.txt")]);
    await d.click('[data-feature="polar-files:import"]');
    await d.waitFor(".polar-file-list li", { text: "Cal 40" });
    assert.equal(await d.count(".polar-file-list li"), 3);

    // Compare on the first source row: A is that source, B the blend.
    await d.click('[data-feature="sources:compare"]');
    await d.waitFor('[data-feature="stage:compare"][aria-selected="true"]');
    await d.waitFor('[data-feature="compare:operand-a"]');
    const a = await d.text('[data-feature="compare:operand-a"]');
    assert.match(a, /Farr 40|Class 40|Cal 40/, `A names a polar file (${a})`);
    assert.match(await d.text('[data-feature="compare:operand-b"]'), /Blend/);
    await d.waitFor(".compare-summary", { text: "cells compared" });
    let text = await summary(d);
    const compared = Number(/(\d+) cells compared/.exec(text)?.[1] ?? 0);
    assert.ok(compared > 20, `the blend and a source share many cells (${text})`);
    assert.ok(await d.count(".compare-heat-cell") > 20, "the heat map has cells");
    await d.waitFor("canvas.compare-canvas", { visible: true });
    let colours = 0;
    for (let i = 0; i < 50 && colours < 8; i += 1) {
      colours = await distinctColours(d, "canvas.compare-canvas");
      if (colours < 8) await new Promise((r) => setTimeout(r, 200));
    }
    assert.ok(colours >= 8, `the 3D comparison is drawn (${colours} colours)`);
    await t.shot("source-against-blend");

    // B: Cal 40, whose wind speeds stop at 20 kn: the other operand's
    // cells beyond are counted and hatched. Chosen by colour and name.
    await d.click('[data-feature="compare:operand-b"]');
    await d.waitFor(".compare-picker-b .compare-picker-list");
    assert.ok(await d.count(".compare-picker-b .compare-picker-list .swatch") >= 4, "each option shows its colour");
    await t.shot("picker-open");
    const other = a.includes("Cal 40") ? "Farr 40" : "Cal 40";
    await d.click(".compare-picker-b .compare-picker-list button", { text: other });
    await d.waitGone(".compare-picker-b .compare-picker-list");
    await d.waitFor('[data-feature="compare:operand-b"]', { text: other });
    await d.waitFor(".compare-regions li");
    text = await summary(d);
    assert.match(text, /Max \|Δ\| [\d.]+ kn at [\d.]+°/, `the largest difference is named (${text})`);
    const single = Number(/(\d+) only A/.exec(text)?.[1] ?? 0) + Number(/(\d+) only B/.exec(text)?.[1] ?? 0);
    assert.ok(single > 0, `cells only one covers are counted (${text})`);
    assert.equal(await d.count(".compare-heat-cell.empty[fill^='url(']"), single, "and hatched in the heat map");
    await t.shot("two-files");

    // Δ as percent of B: the legend and the summary switch unit.
    await d.click('[data-feature="compare:percent"]');
    await d.waitFor(".compare-summary", { text: "%" });
    await t.shot("percent");

    // Swap: A and B exchange names.
    await d.click('[data-feature="compare:swap"]');
    await d.waitFor('[data-feature="compare:operand-a"]', { text: other });
    await t.shot("swapped");

    // The light theme: the scale is chosen for it.
    await d.click('[data-feature="shell:settings"]');
    await d.click('[data-feature="settings:theme"]');
    await d.click('[role="option"]', { text: "Paper" });
    await d.click('[data-feature="settings:close"]');
    await d.waitGone('[data-feature="settings:close"]');
    await new Promise((r) => setTimeout(r, 500));
    await t.shot("paper-theme");
  },
};
