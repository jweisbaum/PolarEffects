/**
 * M22: quitting and the catalogues' shutdown schedule (spec.md 5.4).
 *
 * Offline (the default suite): with both catalogues on "Manually only",
 * quitting quits. Nothing is fetched.
 *
 * With PE_TEST_LIVE=1: the ORC catalogue set to download on shutdown makes
 * the first quit wait and say so; a second quit stops the download and the
 * application exits, with nothing stored. This asks ORC's service for its
 * country list and at most the start of one country.
 */
import { access } from "node:fs/promises";
import { join } from "node:path";

import { assert, newProject } from "./harness.mjs";

const LIVE = process.env.PE_TEST_LIVE === "1";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Whether the application still answers. */
const alive = (d) => Promise.race([
  d.invoke("orc_scrape_status").then(() => true, () => false),
  pause(5_000).then(() => false),
]);

/** Waits for the application to have gone; answers whether it did. */
async function gone(d, withinMs) {
  for (let waited = 0; waited < withinMs; waited += 500) {
    if (!await alive(d)) return true;
    await pause(500);
  }
  return false;
}

export default {
  name: LIVE ? "a shutdown download makes quitting wait, and a second quit stops it" : "quitting quits when no download is scheduled",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Quitting");
    assert.equal((await d.invoke("orc_scrape_status")).running, false);

    if (!LIVE) {
      // The project is unsaved: the guard's answer is "don't save".
      await d.invoke("quit_app", { discardUnsaved: true }).catch(() => undefined);
      assert.ok(await gone(d, 20_000), "the application quit: nothing was scheduled");
      return;
    }

    await d.invoke("set_catalogue_schedule", { catalogue: "orc", schedule: "shutdown" });
    await d.invoke("quit_app", { discardUnsaved: true });
    // Still here, downloading, and saying so.
    await d.waitFor(".statusbar .hint", { text: "Updating the catalogues before quitting. Quit again to stop and quit now.", timeoutMs: 20_000 });
    assert.equal((await d.invoke("orc_scrape_status")).running, true);
    await t.shot("quitting-waits-for-the-download");

    // Quit again: the download stops and the application goes.
    await d.invoke("quit_app", { discardUnsaved: true }).catch(() => undefined);
    assert.ok(await gone(d, 60_000), "the application quit once the download had stopped");
    // A download is stored whole or not at all.
    await assert.rejects(access(join(d.automationRoot, "config", "orc-catalogue.bin")),
      "a cancelled download stores nothing");
  },
};
