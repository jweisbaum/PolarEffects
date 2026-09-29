/**
 * French and German through the whole interface (M17b): the start screen in
 * French from its own picker, then, in each language, every main area
 * photographed — the project window with each left section open and a
 * track's filters unfolded, 3D, Compare, Blend settings, Export, the weather
 * pre-flight, Settings and Help — and twenty words a sailor would type in
 * the feature search, each flashing the control it is meant to find
 * (`ui/src/help/sailor-queries.json`, also checked in `features.test.ts`).
 *
 * The pictures are for a person (or an agent) to read for untranslated or
 * clipped text; the assertions hold the words on screen to the language
 * and each flash to its control.
 */
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert, newProject, setLanguageInSettings } from "./harness.mjs";

/** 2025-07-26T12:00Z. */
const START = 1_753_531_200;

/** One boat, a fix every 10 minutes for a day off the Solent. */
function race() {
  const features = [];
  for (let k = 0; k <= 144; k += 1) {
    features.push({
      type: "Feature",
      geometry: { type: "Point", coordinates: [-1.3 - 1.5 * Math.sin(k / 46), 50.6 + 0.1 * Math.cos(k / 23)] },
      properties: { time: START + 600 * k, boat: "Ux Boat" },
    });
  }
  return JSON.stringify({ type: "FeatureCollection", features });
}

/** What each language must show where a word is checked. */
const WORDS = {
  fr: { map: "Carte", compare: "Comparer", orc: "Polaires ORC", filters: "Filtres de points", settings: "Réglages" },
  de: { map: "Karte", compare: "Vergleich", orc: "ORC-Polaren", filters: "Punktfilter", settings: "Einstellungen" },
};

let dir = null;

/** Closes whatever dialog, help window or popup is open. */
async function closeAll(d) {
  for (let i = 0; i < 3; i += 1) {
    if (await d.exists(".help-dialog")) {
      await d.click('[data-feature="help:close"]');
      await d.waitGone(".help-dialog");
    } else if (await d.exists(".modal-backdrop")) {
      await d.key("Escape", ".modal-backdrop [role=dialog]");
      await d.waitGone(".modal-backdrop", { timeoutMs: 5000 }).catch(() => undefined);
    } else {
      return;
    }
  }
}

async function areas(t, language) {
  const d = t.driver;
  const words = WORDS[language];
  await d.click('[data-feature="stage:map"]');
  assert.equal(await d.text('[data-feature="stage:map"]'), words.map);
  assert.ok((await d.text('[data-feature="nav:orc"]')).includes(words.orc), "the ORC section heading is translated");
  await d.run(`document.querySelector(".sidebar.left").scrollTop = 0; done(true);`);
  await t.shot(`${language}-project-orc-and-polar-files`);
  await d.run(`document.querySelector('[data-feature="tracks:manoeuvre"]').scrollIntoView({ block: "center" }); done(true);`);
  assert.ok((await d.text(".sidebar.left")).includes(words.filters), "the track filters are translated");
  const sideways = await d.run(
    `var wide = [];
     [document.querySelector(".sidebar.left")].concat(Array.prototype.slice.call(document.querySelectorAll(".sidebar.left *")))
       .forEach(function (el) {
         var s = getComputedStyle(el);
         if ((s.overflowX === "auto" || s.overflowX === "scroll") && el.scrollWidth > el.clientWidth + 1) wide.push(el.className + " " + el.scrollWidth + ">" + el.clientWidth);
       });
     if (wide.length) {
       var side = document.querySelector(".sidebar.left").getBoundingClientRect().right;
       Array.prototype.forEach.call(document.querySelectorAll(".sidebar.left *"), function (el) {
         var r = el.getBoundingClientRect();
         if (r.right > side + 1 && wide.length < 12) wide.push(el.tagName + "." + el.className + "[" + (el.getAttribute("data-feature") || "") + "] " + Math.round(r.width));
       });
     }
     done(wide);`);
  assert.deepEqual(sideways, [], `nothing in the navigation scrolls sideways: ${JSON.stringify(sideways)}`);
  await t.shot(`${language}-project-tracks-and-filters`);
  await d.run(`document.querySelector('[data-feature="tracks:no-tide"]').scrollIntoView({ block: "end" }); done(true);`);
  await t.shot(`${language}-project-environment-filters`);

  await d.click('[data-feature="stage:3d"]');
  await d.waitFor("canvas.view3d-canvas", { visible: true });
  await new Promise((r) => setTimeout(r, 1500));
  await t.shot(`${language}-3d`);

  await d.click('[data-feature="stage:compare"]');
  assert.equal(await d.text('[data-feature="stage:compare"]'), words.compare);
  await d.waitFor('[data-feature="compare:summary"]', { timeoutMs: 30_000 });
  await new Promise((r) => setTimeout(r, 1500));
  await t.shot(`${language}-compare`);
  await d.click('[data-feature="stage:map"]');

  await d.click('[data-feature="sources:blend-settings"]');
  await d.waitFor('[data-feature="blend-settings:twa"]');
  await t.shot(`${language}-blend-settings`);
  await closeAll(d);

  await d.click('[data-feature="sources:export"]');
  await d.waitFor('[data-feature="export:preview"]');
  await t.shot(`${language}-export`);
  await closeAll(d);

  await d.click('[data-feature="tracks:fetch-weather"]');
  await d.waitFor('[data-feature="env-fetch:hourly"]', { timeoutMs: 30_000 });
  await new Promise((r) => setTimeout(r, 1500));
  await t.shot(`${language}-weather-preflight`);
  await closeAll(d);

  await d.click('[data-feature="shell:settings"]');
  await d.waitFor(".modal.settings h2", { text: words.settings });
  await t.shot(`${language}-settings`);
  await d.click('[data-feature="settings:close"]');
  await d.waitGone(".modal.settings");

  await d.click('[data-feature="shell:help"]');
  await d.waitFor(".help-dialog");
  await t.shot(`${language}-help`);
  await closeAll(d);
}

async function searches(t, language, queries) {
  const d = t.driver;
  const found = [];
  const surrounds = (target) => d.run(
    `var flash = document.querySelector(".feature-flash");
     var el = document.querySelector('[data-feature="' + arguments[0] + '"]');
     if (!flash || !el) { done("missing"); return; }
     var f = flash.getBoundingClientRect(), b = el.getBoundingClientRect();
     done(f.left <= b.left + 1 && f.top <= b.top + 1 && f.right >= b.right - 1 && f.bottom >= b.bottom - 1);`,
    [target]);
  for (const [n, [query, id, target]] of queries.entries()) {
    let inside = false;
    let label = null;
    // The flash follows its control frame by frame, and a window without
    // focus is given few frames: when a reveal changes the stage and the
    // panel is still filling in, the flash can trail it for its whole 2.4 s.
    // A second search, with the stage already open, is what a person does.
    for (let attempt = 0; attempt < 2 && inside !== true; attempt += 1) {
      await closeAll(d);
      if (await d.exists(".feature-flash")) await d.waitGone(".feature-flash", { timeoutMs: 10_000 });
      await d.click('[data-feature="shell:search"]');
      await d.type('[data-feature="shell:search"]', query);
      await d.waitFor('.help-menu-popup li[role="option"]');
      await new Promise((r) => setTimeout(r, 150));
      label = await d.text('.help-menu-popup li[role="option"] .help-search-label');
      await d.click('.help-menu-popup li[role="option"]');
      await d.waitFor(".feature-flash", { visible: true, timeoutMs: 10_000 });
      for (let i = 0; i < 15 && inside !== true; i += 1) {
        if (i > 0) await new Promise((r) => setTimeout(r, 100));
        inside = await surrounds(target);
      }
    }
    assert.equal(inside, true, `${language} “${query}” → ${label}: the flash surrounds ${target} (${id})`);
    await t.shot(`${language}-search-${String(n + 1).padStart(2, "0")}-${id.replace(":", "-")}`);
    found.push(`${query} → ${label}`);
    await d.waitGone(".feature-flash", { timeoutMs: 10_000 });
  }
  console.log(`${language} searches: ${found.join("; ")}`);
}

export default {
  name: "languages",
  async setup() {
    dir = await mkdtemp(join(tmpdir(), "pe-ux-languages-"));
    await writeFile(join(dir, "race.geojson"), race());
    return { teardown: () => rm(dir, { recursive: true, force: true }) };
  },
  async run(t) {
    const d = t.driver;
    const queries = JSON.parse(await readFile(t.path("ui/src/help/sailor-queries.json"), "utf8"));

    // The start screen's own picker.
    await d.waitFor('[data-feature="start:language"]');
    await d.type('[data-feature="start:language"]', "fr");
    await d.waitFor('html[lang="fr"]', { timeoutMs: 10_000 });
    await d.waitFor('[data-feature="start:new"]', { text: "Nouveau projet" });
    assert.equal(await d.text('[data-feature="new:name"] input'), "Polaire sans titre",
      "the untouched default name follows the language");
    await t.shot("fr-start");

    await newProject(d, "Langues");
    await d.queueDialog([t.path("polar_examples/polars/Farr 40.txt"), t.path("polar_examples/polars/Class 40.txt")]);
    await d.click('[data-feature="polar-files:import"]');
    await d.waitFor(".polar-file-list li", { text: "Class 40" });
    await d.queueDialog([join(dir, "race.geojson")]);
    await d.click('[data-feature="tracks:import-file"]');
    await d.waitFor(".modal-actions button.primary");
    await d.click(".modal-actions button.primary");
    await d.waitFor('[data-feature="tracks:fetch-weather"]', { timeoutMs: 30_000 });
    // Open what is folded, once: Search by field and the track's filters.
    await d.click('[data-feature="orc:fields"]');
    await d.click('[data-feature="tracks:filters"]');
    await d.waitFor('[data-feature="tracks:manoeuvre"]');

    await areas(t, "fr");
    await searches(t, "fr", queries.fr);

    await setLanguageInSettings(d, "de");
    await d.click('[data-feature="settings:close"]');
    await d.waitGone(".modal.settings");
    await areas(t, "de");
    await searches(t, "de", queries.de);

    // Back to the start screen in German.
    await d.click('[data-feature="shell:project-menu"]');
    await d.click('[data-feature="project:close"]');
    // The project was never saved: the unsaved-changes question, in German.
    await d.waitFor("[role=dialog] button", { text: "Nicht speichern" });
    await t.shot("de-unsaved-question");
    await d.click("[role=dialog] button", { text: "Nicht speichern" });
    await d.waitFor('[data-feature="start:new"]', { text: "Neues Projekt" });
    await t.shot("de-start");
  },
};
