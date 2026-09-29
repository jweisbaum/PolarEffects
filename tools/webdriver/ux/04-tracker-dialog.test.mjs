/**
 * The YellowBrick dialog under the real dev build (StrictMode): an address
 * served by a local fixture server (recorded Middle Sea Race 2024
 * responses, no live network) → the boat list → the positions finish → one
 * Cancel only → a boat imports as a track. a8cf1dd was this dialog hanging
 * on "Downloading" in development only.
 */
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";

import { assert, colourPixels, newProject, toHex } from "./harness.mjs";

const FIXTURES = new URL("../../../crates/pe-trackers/tests/fixtures/yellowbrick/", import.meta.url);

async function serveFixtures() {
  const routes = new Map([
    ["/JSON/rmsr2024/RaceSetup", await readFile(new URL("rmsr2024-RaceSetup.json", FIXTURES))],
    ["/BIN/rmsr2024/AllPositions3", await readFile(new URL("rmsr2024-AllPositions3-first3.bin", FIXTURES))],
  ]);
  const requests = [];
  const server = createServer((req, res) => {
    const path = (req.url ?? "").split("?")[0];
    requests.push(path);
    const body = routes.get(path);
    // The positions a moment after the list, so the dialog's "listed, still
    // downloading" state is really passed through.
    const delay = path.startsWith("/BIN/") ? 1500 : 0;
    setTimeout(() => {
      res.writeHead(body ? 200 : 404, { "content-length": body ? body.length : 0 });
      res.end(body ?? undefined);
    }, delay);
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  return { server, requests, origin: `http://127.0.0.1:${server.address().port}` };
}

let fixture = null;

export default {
  name: "tracker dialog",
  async setup() {
    fixture = await serveFixtures();
    return {
      env: { PE_DRIVER_YELLOWBRICK: fixture.origin },
      teardown: () => new Promise((done) => fixture.server.close(done)),
    };
  },
  async run(t) {
    const d = t.driver;
    await newProject(d, "Middle Sea");
    await d.click('[data-feature="tracks:yellowbrick"]');
    await d.waitFor(".modal.tracker-import");
    await t.shot("dialog-open");
    await d.type('[data-feature="tracker-import:url"]', "https://yb.tl/rmsr2024");
    await d.click('[data-feature="tracker-import:open"]');
    await d.waitFor('[data-feature="tracker-import:boats"] tbody tr', { timeoutMs: 20_000 });
    await t.shot("boats-listed");
    // The positions arrive and the dialog leaves "Downloading" (the a8cf1dd hang).
    await d.waitGone(".tracker-import .tracker-progress", { timeoutMs: 20_000 });
    const rows = await d.count('[data-feature="tracker-import:boats"] tbody tr');
    assert.ok(rows >= 3, `the boat list shows the event's boats (${rows})`);
    // Exactly one Cancel, found by structure rather than by its English
    // label: the dialog's answer buttons carry no data-feature (they are
    // exempt from the help registry), so every untagged, non-primary button
    // in the dialog is a dismiss button, and the only one must be the first
    // in its action row. A second Cancel (a8cf1dd) would be a second one.
    const dismiss = await d.run(
      `var dialog = document.querySelector(".modal.tracker-import");
       var untagged = dialog.querySelectorAll("button:not([data-feature]):not(.primary)");
       var first = dialog.querySelector(".modal-actions > button:first-child");
       done({ count: untagged.length, isFirst: untagged.length === 1 && untagged[0] === first });`);
    assert.deepEqual(dismiss, { count: 1, isFirst: true }, "exactly one Cancel, the action row's first button");
    assert.ok(fixture.requests.includes("/JSON/rmsr2024/RaceSetup"), `the fixture server was asked: ${fixture.requests}`);
    await t.shot("positions-loaded");

    const first = await d.run(
      `var box = Array.prototype.find.call(document.querySelectorAll('[data-feature="tracker-import:boats"] tbody input[type=checkbox]'),
         function (b) { return !b.disabled; });
       if (!box) { done(null); return; } box.click(); done(box.getAttribute("aria-label"));`);
    assert.ok(first, "a boat with positions can be ticked");
    await d.click(".modal.tracker-import button.primary", { text: "Import tracks" });
    await d.waitGone(".modal.tracker-import", { timeoutMs: 20_000 });
    await d.waitFor(".left-nav", { text: first });
    await t.shot("track-imported");

    // The track on the map, framed: its colour is on the map canvas.
    await d.click('[data-feature="map:fit-tracks"]');
    const colour = toHex(await d.run(
      `done(getComputedStyle(document.querySelector('[data-feature="sources:colour"]')).backgroundColor);`));
    let drawn = 0;
    for (let i = 0; i < 50 && drawn < 200; i += 1) {
      drawn = await colourPixels(d, "canvas.map-canvas", colour, 30);
      if (drawn < 200) await new Promise((r) => setTimeout(r, 200));
    }
    // Drawn two device pixels wide since M17a; the count is logged to
    // compare runs.
    assert.ok(drawn >= 200, `the track is drawn on the map in ${colour} (${drawn} pixels)`);
    console.log(`track pixels in ${colour}: ${drawn}`);
    await t.shot("track-on-map");
  },
};
