/**
 * ORC search by field (spec.md 5.2, D26): "Search by field" starts folded →
 * unfolded, a designer and a model typed a key at a time → the results are
 * that designer's boats of that model, updated as each key lands → folded,
 * the heading counts two fields in use → Clear.
 */
import { assert, newProject } from "./harness.mjs";

/** Types `text` one character at a time, as a person would. */
async function typeKeys(d, selector, text) {
  for (let end = 1; end <= text.length; end += 1) {
    await d.type(selector, text.slice(0, end));
  }
}

export default {
  name: "orc search by field",
  async run(t) {
    const d = t.driver;
    await newProject(d, "ORC fields");
    const toggle = '[data-feature="orc:fields"]';
    await d.waitFor(toggle, { visible: true });
    assert.equal(await d.run(
      `done(document.querySelector('[data-feature="orc:fields"]').getAttribute("aria-expanded"));`), "false");
    assert.equal(await d.exists('[data-feature="orc:field-designer"]'), false, "folded by default");

    await d.click(toggle);
    await d.waitFor('[data-feature="orc:field-designer"]', { visible: true });
    await t.shot("unfolded");

    await typeKeys(d, '[data-feature="orc:field-designer"]', "frers");
    await typeKeys(d, '[data-feature="orc:field-model"]', "swan 112");
    await d.waitFor(".orc-results li .orc-name", { text: "Eratosthenes" });
    // Each row's first meta line: sail number · model · year · builder.
    const metas = await d.run(
      `done([...document.querySelectorAll(".orc-results li")].map((li) => li.querySelector(".orc-meta").textContent));`);
    assert.ok(metas.length >= 1, "some results");
    for (const meta of metas) assert.match(meta, /Swan 112/, `every result is a Swan 112: ${meta}`);
    const count = await d.text(".orc-count");
    assert.match(count, /certificate/);
    await t.shot("designer-and-model");

    await d.click(toggle);
    await d.waitGone('[data-feature="orc:field-designer"]');
    assert.match(await d.text(toggle), /Search by field \(2\)/);
    await t.shot("folded-with-count");

    await d.click(toggle);
    await d.waitFor('[data-feature="orc:fields-clear"]', { visible: true });
    await d.click('[data-feature="orc:fields-clear"]');
    await d.waitGone(".orc-results");
    assert.equal(await d.run(
      `done(document.querySelector('[data-feature="orc:field-model"]').value);`), "");
    await t.shot("cleared");
  },
};
