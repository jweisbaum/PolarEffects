/**
 * Export reanalysis GRIB… (M16, spec.md 7.8) through the real dev build,
 * with no network: the archives are small Zarr stores written here and
 * served by a local server that honours `Range` and answers slowly, so the
 * export's progress is seen.
 *
 * - a twelve-hour Channel track, unfolded, opens the dialog: the area
 *   (the track's box plus 2° on the 0.25° grid), 13 hourly times, what the
 *   waves and the current would download and the file's size;
 * - ticking the current and choosing every 3 hours change the figures;
 * - Save… (the path queued for the native dialog) runs the export with its
 *   progress, then closes with a hint; the file is a GRIB2 of 5 times × 4
 *   messages (wind and waves, 3-hourly from 06Z to 18Z).
 */
import { mkdtemp, readFile, rm, writeFile, mkdir } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert, newProject } from "./harness.mjs";

/** 2020-07-27T06:00Z. */
const START = 1_595_829_600;
const LATS = Array.from({ length: 49 }, (_, i) => 56 - 0.25 * i);
const LONS = Array.from({ length: 60 }, (_, j) => 345 + 0.25 * j);
const STEPS = 24;

function f4(values) {
  const out = Buffer.alloc(values.length * 4);
  values.forEach((v, i) => out.writeFloatLE(v, i * 4));
  return out;
}

/** An uncompressed Zarr v2 store of `(time, latitude, longitude)` arrays, a chunk per hour from 2020-07-27T00Z. */
async function writeStore(dir, epochUnix, epochText, arrays) {
  const put = async (path, body) => {
    await mkdir(join(dir, path, ".."), { recursive: true });
    await writeFile(join(dir, path), body);
  };
  const meta = (shape, chunks, dtype) => JSON.stringify({
    zarr_format: 2, shape, chunks, dtype, compressor: null, filters: null, order: "C",
    fill_value: dtype === "<f4" ? "NaN" : null,
  });
  await put(".zgroup", JSON.stringify({ zarr_format: 2 }));
  await put(".zattrs", "{}");
  for (const [name, values] of [["latitude", LATS], ["longitude", LONS]]) {
    await put(`${name}/.zarray`, meta([values.length], [values.length], "<f4"));
    await put(`${name}/.zattrs`, JSON.stringify({ _ARRAY_DIMENSIONS: [name] }));
    await put(`${name}/0`, f4(values));
  }
  const first = (START - 6 * 3600 - epochUnix) / 3600;
  const hours = Buffer.alloc(STEPS * 8);
  for (let k = 0; k < STEPS; k += 1) hours.writeBigInt64LE(BigInt(first + k), k * 8);
  await put("time/.zarray", meta([STEPS], [STEPS], "<i8"));
  await put("time/.zattrs", JSON.stringify({ _ARRAY_DIMENSIONS: ["time"], units: `hours since ${epochText}` }));
  await put("time/0", hours);
  for (const [name, value] of arrays) {
    await put(`${name}/.zarray`, meta([STEPS, LATS.length, LONS.length], [1, LATS.length, LONS.length], "<f4"));
    await put(`${name}/.zattrs`, JSON.stringify({ _ARRAY_DIMENSIONS: ["time", "latitude", "longitude"] }));
    for (let step = 0; step < STEPS; step += 1) {
      const values = [];
      for (const lat of LATS) for (const lon of LONS) values.push(value(step, lat, lon));
      await put(`${name}/${step}.0.0`, f4(values));
    }
  }
}

/** WeatherBench2 wind and ARCO-ERA5 waves; land north-east of the Channel. */
async function writeArchives(root) {
  await writeStore(join(root, "wb2-era5-1h"), -347_155_200, "1959-01-01", [
    ["10m_u_component_of_wind", (s, lat) => 5 + s / 4 + (lat - 50) / 2],
    ["10m_v_component_of_wind", (s, _lat, lon) => -3 + (lon - 355) / 3],
  ]);
  await writeStore(join(root, "arco-era5"), -2_208_988_800, "1900-01-01", [
    ["significant_height_of_combined_wind_waves_and_swell",
      (s, lat, lon) => (lat > 51 && lon > 357 ? NaN : 1 + s / 20)],
    ["mean_wave_direction", (_s, _lat, lon) => lon - 100],
  ]);
}

/** A static server with `Range`, 120 ms a request: the archives. */
async function serve(root) {
  const requests = [];
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent((req.url ?? "/").split("?")[0]);
    requests.push(path);
    let body;
    try {
      body = await readFile(join(root, path));
    } catch {
      res.writeHead(404, { "content-length": 0 });
      res.end();
      return;
    }
    const range = /bytes=(\d+)-(\d+)/.exec(req.headers.range ?? "");
    setTimeout(() => {
      if (range && Number(range[1]) < body.length) {
        const a = Number(range[1]);
        const b = Math.min(Number(range[2]), body.length - 1);
        res.writeHead(206, { "content-length": b - a + 1, "content-range": `bytes ${a}-${b}/${body.length}` });
        res.end(body.subarray(a, b + 1));
      } else {
        res.writeHead(200, { "content-length": body.length });
        res.end(body);
      }
    }, path.includes(".0.0") ? 120 : 0);
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  return { server, requests, origin: `http://127.0.0.1:${server.address().port}` };
}

/** A boat off the Lizard, a fix every 10 minutes from 06:00 to 18:00Z. */
function track() {
  const features = [];
  for (let k = 0; k <= 72; k += 1) {
    features.push({
      type: "Feature",
      geometry: { type: "Point", coordinates: [-5.5 + k * 0.02, 49.8 + k * 0.01] },
      properties: { time: START + 600 * k, boat: "Grib Boat" },
    });
  }
  return JSON.stringify({ type: "FeatureCollection", features });
}

let root = null;
let archive = null;

export default {
  name: "reanalysis GRIB export",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-grib-"));
    await writeArchives(join(root, "archives"));
    await writeFile(join(root, "race.geojson"), track());
    archive = await serve(join(root, "archives"));
    return {
      env: { PE_DRIVER_REANALYSIS: archive.origin },
      teardown: async () => {
        await new Promise((done) => archive.server.close(done));
        await rm(root, { recursive: true, force: true });
      },
    };
  },
  async run(t) {
    const d = t.driver;
    await newProject(d, "Grib");
    await d.queueDialog([join(root, "race.geojson")]);
    await d.click('[data-feature="tracks:import-file"]');
    await d.waitFor(".modal-actions button.primary");
    await d.click(".modal-actions button.primary");
    await d.waitFor('[data-feature="tracks:filters"]', { timeoutMs: 30_000 });
    await d.click('[data-feature="tracks:filters"]');
    await d.waitFor('[data-feature="tracks:export-grib"]', { visible: true });
    await d.click('[data-feature="tracks:export-grib"]');

    // The area: 49.8–50.52N, 5.5–4.06W plus 2°, on the 0.25° grid.
    await d.waitFor(".grib-export .grib-area", { text: "Area:" });
    await d.waitFor(".grib-export [role=status]", { text: "About" });
    let text = await d.text(".grib-export");
    assert.ok(text.includes("13 times from 2020-07-27 06:00 UTC to 2020-07-27 18:00 UTC"), text);
    assert.ok(text.includes("Area: 47.75°N to 52.75°N, 7.50°W to 2.00°W; 23 × 21 points every 0.25°."), text);
    assert.ok(/Wave height and direction \(about \d+ MB to download\)/.test(text), text);
    await t.shot("dialog-hourly-with-waves");

    // The current adds its download; every 3 hours is a third of the file.
    const before = /the file is about (\d+) kB/.exec(text)?.[1];
    await d.click('[data-feature="grib:current"]');
    await d.click('[data-feature="grib:three-hourly"]');
    await d.waitFor(".grib-export .modal-summary", { text: "5 times" });
    text = await d.text(".grib-export");
    const after = /the file is about (\d+) kB/.exec(text)?.[1];
    assert.ok(Number(after) < Number(before), `${before} → ${after}`);
    await t.shot("dialog-three-hourly-with-current");
    // The current is not served here; leave it out for the export.
    await d.click('[data-feature="grib:current"]');

    const out = join(root, "Grib Boat.grib2");
    await d.queueDialog(out);
    await d.click(".grib-export .modal-actions button.primary");
    await d.waitFor(".grib-export .tracker-progress", { text: "Exporting…" });
    await t.shot("exporting");
    await d.waitGone(".grib-export", { timeoutMs: 60_000 });
    await d.waitFor(".statusbar", { text: "Exported the reanalysis to" });
    await t.shot("exported");

    // A GRIB2 file of 5 times × wind u, v and wave height, direction.
    const bytes = await readFile(out);
    let at = 0;
    let messages = 0;
    while (at < bytes.length) {
      assert.equal(bytes.subarray(at, at + 4).toString("latin1"), "GRIB");
      assert.equal(bytes[at + 7], 2, "edition 2");
      at += Number(bytes.readBigUInt64BE(at + 8));
      messages += 1;
    }
    assert.equal(messages, 20);
    assert.ok(archive.requests.some((p) => p.startsWith("/wb2-era5-1h/10m_u_component_of_wind/")), "the wind was read");
  },
};
