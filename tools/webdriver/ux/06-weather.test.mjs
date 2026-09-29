/**
 * Small weather downloads, kept per sample only (M14e, D27), through the
 * real dev build and with no network:
 *
 * - an earlier version's on-disk chunk cache (seeded in the run's data
 *   root) is announced on the status line and removed, and nothing else;
 * - a five-day track's Fetch weather… shows the block-level download and
 *   "stored in the project: about N kB", and Not now fetches nothing;
 * - Settings has "Keep downloaded weather in memory for this session (MB)"
 *   in place of the chunk cache's folder and size, saved to settings.json
 *   without the old `chunk_cache` group.
 */
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert, newProject } from "./harness.mjs";

/** 2025-07-26T12:00Z, the Fastnet start. */
const START = 1_753_531_200;
const OLD_CHUNK = "cache/chunks/wb2-era5-1h/10m_u_component_of_wind/539724.0.0";

/** A boat sailing from the Solent to the Fastnet and back, a fix every 10 minutes for five days. */
function race() {
  const features = [];
  for (let k = 0; k <= 720; k += 1) {
    const f = k / 720;
    const out = f < 0.5 ? f * 2 : 2 - f * 2;
    const lon = -1.3 - 8.2 * out;
    const lat = 50.7 + 0.8 * out + 0.2 * Math.sin(k / 30);
    features.push({
      type: "Feature",
      geometry: { type: "Point", coordinates: [lon, lat] },
      properties: { time: START + 600 * k, boat: "Ux Boat" },
    });
  }
  return JSON.stringify({ type: "FeatureCollection", features });
}

let root = null;

export default {
  name: "weather download and memory",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-weather-"));
    // As an earlier version left the data root: a chunk in its cache and
    // its chunk-cache settings.
    await mkdir(join(root, OLD_CHUNK, ".."), { recursive: true });
    await writeFile(join(root, OLD_CHUNK), Buffer.alloc(3_300_000));
    await mkdir(join(root, "config"), { recursive: true });
    await writeFile(join(root, "config", "settings.json"),
      JSON.stringify({ chunk_cache: { location: "", size_limit_gb: 20 } }));
    await writeFile(join(root, "race.geojson"), race());
    return {
      env: { PE_AUTOMATION_ROOT: root },
      teardown: () => rm(root, { recursive: true, force: true }),
    };
  },
  async run(t) {
    const d = t.driver;
    await newProject(d, "Weather");
    // The old cache is announced once and removed; the data root stays.
    await d.waitFor(".statusbar", { text: "no longer kept on disk" });
    const notice = await d.text(".statusbar");
    assert.ok(notice.includes("3 MB"), notice);
    await t.shot("old-cache-removed");
    let gone = false;
    for (let i = 0; i < 50 && !gone; i += 1) {
      gone = await stat(join(root, "cache", "chunks")).then(() => false, () => true);
      if (!gone) await new Promise((r) => setTimeout(r, 100));
    }
    assert.ok(gone, "no file is left under the old chunk cache");
    assert.ok((await stat(join(root, "cache"))).isDirectory(), "only chunks/ went");

    // A five-day track, then its weather's pre-flight.
    await d.queueDialog([join(root, "race.geojson")]);
    await d.click('[data-feature="tracks:import-file"]');
    await d.waitFor(".modal-actions button.primary");
    await d.click(".modal-actions button.primary");
    await d.waitFor('[data-feature="tracks:fetch-weather"]', { timeoutMs: 30_000 });
    await d.click('[data-feature="tracks:fetch-weather"]');
    await d.waitFor("[role=dialog]", { text: "Hourly: about" });
    const text = await d.text("[role=dialog]");
    assert.ok(text.includes("721 samples"), text);
    // About 120 hours × 1.2 MB of blocks plus currents: megabytes, not the
    // 1.2 GB of whole fields; kilobytes kept.
    const hourly = Number(/Hourly: about (\d+) MB/.exec(text)?.[1]);
    assert.ok(hourly > 50 && hourly < 400, text);
    assert.ok(/Stored in the project: about \d+ kB/.test(text), text);
    // The interval legend is one line, not a word a line (M17a).
    const legend = await d.run(
      `var l = document.querySelector(".env-fetch-interval legend");
       done({ height: l.getBoundingClientRect().height, line: parseFloat(getComputedStyle(l).lineHeight) || 20 });`);
    assert.ok(legend.height < legend.line * 1.5, `the legend is ${legend.height} px tall`);
    await t.shot("fetch-estimate");
    await d.click(".modal-actions button", { text: "Not now" });
    await d.waitGone("[role=dialog]");

    // Settings: the memory for this session, where the chunk cache was.
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(".modal.settings");
    assert.ok(!(await d.exists('[data-feature="settings:cache-limit"]')), "no chunk cache setting");
    await d.run(`document.querySelector('[data-section="settings:weather"]').scrollIntoView({ block: "start" }); done(true);`);
    await d.waitFor('[data-feature="settings:weather-memory"]', { visible: true });
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:weather-memory"]').value);`), "256");
    await d.type('[data-feature="settings:weather-memory"]', "512");
    await d.key("Enter", '[data-feature="settings:weather-memory"]');
    let saved = null;
    for (let i = 0; i < 50 && saved?.weather_memory_mb !== 512; i += 1) {
      saved = JSON.parse(await readFile(join(root, "config", "settings.json"), "utf8"));
      if (saved.weather_memory_mb !== 512) await new Promise((r) => setTimeout(r, 100));
    }
    assert.equal(saved.weather_memory_mb, 512);
    assert.ok(!("chunk_cache" in saved), "the old group is not written back");
    await t.shot("settings-weather-memory");
  },
};
