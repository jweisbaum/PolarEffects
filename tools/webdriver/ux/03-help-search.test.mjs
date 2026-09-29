/**
 * The title bar's help search in each language: a feature's label → its
 * result → the orange flash around the control.
 */
import { assert, newProject, setLanguageInSettings } from "./harness.mjs";

const LABELS = { en: "Fit the world", fr: "Voir le monde entier", de: "Ganze Welt zeigen" };

export default {
  name: "help search",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Help search");
    for (const [language, label] of Object.entries(LABELS)) {
      if (language !== "en") {
        await setLanguageInSettings(d, language);
        await d.click('[data-feature="settings:close"]');
        await d.waitGone(".modal.settings");
      }
      await d.click('[data-feature="shell:search"]');
      await d.type('[data-feature="shell:search"]', label);
      await d.waitFor(".help-menu-popup li", { text: label });
      await t.shot(`${language}-results`);
      await d.click(".help-menu-popup li", { text: label });
      await d.waitFor(".feature-flash", { visible: true, timeoutMs: 5000 });
      const inside = await d.run(
        `var f = document.querySelector(".feature-flash").getBoundingClientRect();
         var b = document.querySelector('[data-feature="map:fit"]').getBoundingClientRect();
         done(f.left <= b.left && f.top <= b.top && f.right >= b.right && f.bottom >= b.bottom);`);
      assert.equal(inside, true, `the flash surrounds Fit the world (${language})`);
      await t.shot(`${language}-flash`);
      await d.waitGone(".feature-flash", { timeoutMs: 10_000 });
    }
  },
};
