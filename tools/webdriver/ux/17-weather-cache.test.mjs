import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile, access } from "node:fs/promises";
import { join } from "node:path";
import { assert, newProject } from "./harness.mjs";

export default {
  name: "Whirlwind disk cache settings and clearing",
  async run(t) {
    const d = t.driver;
    const f = id => `[data-feature="settings:${id}"]`;
    const chosen = join(d.automationRoot, "chosen-weather-cache");
    await mkdir(chosen);
    await writeFile(join(chosen, "keep-me.txt"), "unrelated user file");
    await newProject(d, "Weather cache");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(f("cache-size"));
    assert.equal(await d.text(f("cache-size")), "10");
    await d.run(`document.querySelector(arguments[0]).scrollIntoView({block:'center'}); done(true)`, [f("cache-directory")]);
    await d.queueDialog(chosen);
    await d.click(f("cache-browse"));
    await d.waitFor(f("cache-directory"));
    assert.equal(await d.text(f("cache-directory")), chosen);
    // Saving the cache must not discard a draft in another Settings section.
    await d.type(f("library-metadata"), chosen);
    await d.type(f("cache-size"), "2");
    await d.click(f("close"));
    await d.waitGone('.modal.settings');
    const settingsFile = join(d.automationRoot, "config/settings.json");
    const saved = JSON.parse(await readFile(settingsFile, "utf8"));
    assert.deepEqual(saved.weather_cache, { directory: chosen, max_size_gb: 2 });
    assert.equal(saved.library.metadata_directory, chosen);
    const directory = join(chosen, "whirlwind-hindsight-v1");
    const chunkFile = join(directory, `${"a".repeat(64)}.wwc`);
    const bytes = Buffer.alloc(4096, 42);
    await writeFile(chunkFile, Buffer.concat([Buffer.from("PEWWC001"), createHash("sha256").update(bytes).digest(), bytes]));
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor('.weather-cache-settings', { text: "Cached on disk: 4 kB" });
    assert.equal(await d.text(f("cache-size")), "2");
    await d.run(`document.querySelector(arguments[0]).scrollIntoView({block:'center'}); done(true)`, [f("cache-directory")]);
    await t.shot("cache-directory-and-limit");
    await d.click(f("cache-clear"));
    await d.waitFor('.weather-cache-settings [role="status"]', { text: "Cache cleared" });
    assert.equal(await access(chunkFile).then(() => true, () => false), false);
    assert.equal(await readFile(join(chosen, "keep-me.txt"), "utf8"), "unrelated user file");
    assert.ok((await d.text('.weather-cache-settings')).includes("Cached on disk: 0 kB"));
    await t.shot("cleared-cache-keeps-user-files");
    await d.type(f("cache-directory"), "relative/path");
    await d.click(f("close"));
    await d.waitFor('.modal.settings [role="alert"]');
    assert.equal(JSON.parse(await readFile(settingsFile,"utf8")).weather_cache.directory, chosen);
    await d.type(f("cache-directory"), chosen);
    await d.click(f("close"));
    await d.waitGone('.modal.settings');
  },
};
