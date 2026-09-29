/**
 * Settings: switch the language to French → the labels change on screen,
 * and the choice is saved in the run's own data root, never the person's.
 */
import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { assert, newProject, setLanguageInSettings } from "./harness.mjs";

export default {
  name: "settings language",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Settings");
    assert.equal(await d.text('[data-feature="stage:map"]'), "Map");
    await setLanguageInSettings(d, "fr");
    await d.waitFor(".modal.settings h2", { text: "Réglages" });
    await t.shot("settings-french");
    await d.click('[data-feature="settings:close"]');
    await d.waitGone(".modal.settings");
    assert.equal(await d.text('[data-feature="stage:map"]'), "Carte");
    assert.ok((await d.text('[data-feature="nav:polar-files"]')).includes("Fichiers de polaires"),
      "the left navigation's section heading is French");
    const saved = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(saved.language, "fr", "the language is saved in the run's data root");
    await t.shot("project-window-french");
  },
};
