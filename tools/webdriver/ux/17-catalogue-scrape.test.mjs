/**
 * M22: the certificate catalogues' downloads in Settings (spec.md 5.4). The
 * ORC section says what the catalogue holds and offers its download, each
 * catalogue has its own schedule, and a schedule is saved by Rust the moment
 * it is chosen.
 *
 * Offline: nothing here starts a download. Opening Settings fetches nothing,
 * which the idle progress and the unchanged catalogue count show.
 */
import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { assert, newProject } from "./harness.mjs";

const f = (id) => `[data-feature="${id}"]`;

async function choose(d, id, value) {
  await d.run(`var el = document.querySelector(arguments[0]); el.value = arguments[1];
    el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`, [f(id), value]);
}

/** The schedules as Rust saved them, once they are what is expected. */
async function saved(d, expected) {
  const file = join(d.automationRoot, "config", "settings.json");
  let now = null;
  for (let waited = 0; waited < 10_000; waited += 100) {
    now = await readFile(file, "utf8").then(JSON.parse).catch(() => null);
    if (JSON.stringify(now?.catalogues) === JSON.stringify(expected)) return now.catalogues;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return now?.catalogues;
}

export default {
  name: "ORC and ORR downloads and their schedules in Settings",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Catalogues");
    await d.click(f("shell:settings"));
    await d.waitFor('[data-section="settings:orc"] h3', { text: "ORC polars" });
    await d.run(`document.querySelector('[data-section="settings:orc"]').scrollIntoView({ block: "start" }); done(true);`);

    // What the catalogue holds: the embedded one alone, nothing downloaded.
    await d.waitFor('[data-section="settings:orc"] p', { text: "certificates in the catalogue." });
    const orc = await d.text('[data-section="settings:orc"]');
    assert.match(orc, /1\d{4} certificates in the catalogue\./, orc);
    assert.ok(!orc.includes("downloaded from ORC"), "nothing was downloaded by opening Settings");
    assert.ok(orc.includes("data.orc.org"), "the section names where it downloads from");
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:orc-scrape"]').disabled);`), false);
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:orc-cancel"]').disabled);`), true);
    assert.equal(await d.exists('[data-section="settings:orc"] progress'), false, "no download is running");

    // Each catalogue has its own schedule, manual until chosen otherwise.
    for (const id of ["settings:orc-schedule", "settings:orr-schedule"]) {
      assert.deepEqual(await d.run(`var s = document.querySelector(arguments[0]);
        done({ value: s.value, options: [...s.options].map(function (o) { return o.textContent; }) });`, [f(id)]),
      { value: "on_demand", options: ["Manually only", "On startup", "On shutdown"] });
    }
    await choose(d, "settings:orc-schedule", "startup");
    assert.deepEqual(await saved(d, { orc_schedule: "startup", orr_schedule: "on_demand" }),
      { orc_schedule: "startup", orr_schedule: "on_demand" });
    await choose(d, "settings:orr-schedule", "shutdown");
    assert.deepEqual(await saved(d, { orc_schedule: "startup", orr_schedule: "shutdown" }),
      { orc_schedule: "startup", orr_schedule: "shutdown" });
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:orc-schedule"]').value);`), "startup");
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:orr-schedule"]').value);`), "shutdown");
    await t.shot("orc-section-and-schedules");
    await d.run(`document.querySelector('[data-section="settings:orr"]').scrollIntoView({ block: "start" }); done(true);`);
    await t.shot("orr-section-and-schedule");

    // Back to manual, so stopping this run starts nothing.
    await choose(d, "settings:orc-schedule", "on_demand");
    await choose(d, "settings:orr-schedule", "on_demand");
    assert.deepEqual(await saved(d, { orc_schedule: "on_demand", orr_schedule: "on_demand" }),
      { orc_schedule: "on_demand", orr_schedule: "on_demand" });
    // Only when asked for (PE_TEST_LIVE=1): the download itself, from ORC's
    // service, into this run's own folder. About 60 MB and a minute or two.
    if (process.env.PE_TEST_LIVE === "1") {
      await d.run(`document.querySelector('[data-section="settings:orc"]').scrollIntoView({ block: "start" }); done(true);`);
      const count = async () => Number((await d.text('[data-section="settings:orc"]')).match(/(\d+) certificates in the catalogue/)[1]);
      const before = await count();
      await d.click(f("settings:orc-scrape"));
      await d.waitFor('[data-section="settings:orc"] [role="status"]', { text: "Countries downloaded", timeoutMs: 120_000 });
      await t.shot("orc-download-running");
      await d.waitFor('[data-section="settings:orc"] [role="status"]', { text: "Added", timeoutMs: 600_000 });
      const first = await d.text('[data-section="settings:orc"] [role="status"]');
      const added = Number(first.match(/Added (\d+)/)[1]);
      assert.ok(added > 3000, first);
      await d.waitFor('[data-section="settings:orc"] p', { text: "downloaded from ORC" });
      const after = await count();
      // Never more than was added: a certificate the catalogue already had
      // took its place rather than a second one.
      assert.ok(after > before && after - before <= added, `${before} → ${after}, added ${added}`);
      assert.match(first, /updated 0, removed 0 that ORC no longer lists/, first);
      await t.shot("orc-download-finished");

      // Again: nothing is stored twice.
      await d.click(f("settings:orc-scrape"));
      await d.waitFor('[data-section="settings:orc"] [role="status"]', { text: "Countries downloaded", timeoutMs: 120_000 });
      await d.waitFor('[data-section="settings:orc"] [role="status"]', { text: "Added 0,", timeoutMs: 600_000 });
      // Minutes apart, ORC's list is the same: nothing added, changed or removed.
      assert.match(await d.text('[data-section="settings:orc"] [role="status"]'), /Added 0, updated 0, removed 0 that ORC no longer lists, skipped \d+\./);
      assert.equal(await count(), after, "a second download adds no certificate");
      await t.shot("orc-download-again");
    }
    await d.click(f("settings:close"));
    await d.waitGone(".modal.settings");

    // The ORC panel searches what was downloaded as it searches the rest.
    if (process.env.PE_TEST_LIVE === "1") {
      await d.type(f("orc:search"), String(new Date().getUTCFullYear()));
      await d.waitFor(".orc-results li .orc-meta", { text: String(new Date().getUTCFullYear()) });
      await t.shot("orc-panel-with-this-years-certificates");
    }
    assert.equal(await d.exists(".statusbar .hint.error"), false);
  },
};
