/**
 * M22: the catalogues' startup schedule (spec.md 5.4), from a settings file
 * that already asks for it when the application starts.
 *
 * Offline (the default suite): the ORC catalogue is set to download on
 * startup but was written a moment ago, so nothing is started: at most once
 * a day. That file is not a store the application can read, either, which
 * costs the downloaded certificates and nothing else: the catalogue it was
 * built with is all there.
 *
 * With PE_TEST_LIVE=1: no store yet, so the download starts by itself; it is
 * cancelled at once, having asked ORC's service for little more than its
 * country list, and nothing is stored.
 */
import { access, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert } from "./harness.mjs";

const LIVE = process.env.PE_TEST_LIVE === "1";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let root;

export default {
  name: LIVE ? "a startup download starts by itself and can be cancelled" : "a startup download is skipped when the catalogue is a day fresh",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-catalogue-startup-"));
    await mkdir(join(root, "config"), { recursive: true });
    await writeFile(join(root, "config", "settings.json"),
      JSON.stringify({ catalogues: { orc_schedule: "startup", orr_schedule: "on_demand" } }));
    if (!LIVE) await writeFile(join(root, "config", "orc-catalogue.bin"), "written a moment ago");
    return { env: { PE_AUTOMATION_ROOT: root }, teardown: () => rm(root, { recursive: true, force: true }) };
  },
  async run(t) {
    const d = t.driver;
    await d.waitFor('[data-feature="new:create"]');
    assert.equal((await d.invoke("app_settings")).catalogues.orc_schedule, "startup", "the schedule was read from the file");

    if (!LIVE) {
      // Given every chance to start, it has not.
      await pause(3_000);
      const status = await d.invoke("orc_scrape_status");
      assert.deepEqual([status.running, status.total, status.error], [false, 0, null]);
      const info = await d.invoke("orc_catalogue_info");
      assert.ok(info.records > 18_000, "the built-in catalogue is whole");
      assert.equal(info.scraped, 0, "an unreadable store adds nothing");
      return;
    }

    let status = await d.invoke("orc_scrape_status");
    for (let waited = 0; waited < 20_000 && !status.running; waited += 250) {
      await pause(250);
      status = await d.invoke("orc_scrape_status");
    }
    assert.equal(status.running, true, "the download started with the application");
    await d.invoke("cancel_orc_scrape");
    for (let waited = 0; waited < 120_000 && status.running; waited += 500) {
      await pause(500);
      status = await d.invoke("orc_scrape_status");
    }
    assert.deepEqual([status.running, status.cancelled], [false, true]);
    await assert.rejects(access(join(root, "config", "orc-catalogue.bin")), "a cancelled download stores nothing");
  },
};
