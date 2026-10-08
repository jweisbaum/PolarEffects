/** Metadata loads, both position feeds fail, then Retry recovers via KML. */
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";

import { assert, newProject } from "./harness.mjs";

const FIXTURES = new URL("../../../crates/pe-trackers/tests/fixtures/yellowbrick/", import.meta.url);
let recover = false;
const requests = [];

export default {
  name: "tracker position failure and retry",
  async setup() {
    recover = false;
    requests.length = 0;
    const setup = await readFile(new URL("rmsr2024-RaceSetup.json", FIXTURES));
    const kml = await readFile(new URL("rmsr2024-3teams.kml", FIXTURES));
    const server = createServer((req, res) => {
      requests.push(req.url);
      const body = req.url === "/JSON/rmsr2024/RaceSetup" ? setup
        : req.url === "/rmsr2024.kml" && recover ? kml : null;
      setTimeout(() => {
        res.writeHead(body ? 200 : 503, { "content-length": body?.length ?? 0 });
        res.end(body ?? undefined);
      }, body === setup ? 0 : 200);
    });
    await new Promise(done => server.listen(0, "127.0.0.1", done));
    return {
      env: { PE_DRIVER_YELLOWBRICK: `http://127.0.0.1:${server.address().port}` },
      teardown: () => new Promise(done => server.close(done)),
    };
  },
  async run(t) {
    const d = t.driver;
    await newProject(d, "Tracker retry");
    await d.click('[data-feature="tracks:yellowbrick"]');
    await d.type('[data-feature="tracker-import:url"]', "https://yb.tl/rmsr2024");
    await d.click('[data-feature="tracker-import:open"]');
    await d.waitFor('[data-feature="tracker-import:boats"] tbody tr');
    await d.type('[data-feature="tracker-import:search"]', "NACIRA");
    await d.click('[data-feature="tracker-import:boats"] tbody input[type=checkbox]');
    assert.deepEqual(requests, ["/JSON/rmsr2024/RaceSetup"], "no tracks before Import tracks");
    await d.click('.tracker-import button.primary');
    await d.waitFor('.tracker-import [role=alert]', { timeoutMs: 25_000 });
    assert.match(await d.text('.tracker-import [role=alert]'), /boat list loaded, but the track positions/);
    assert.equal(await d.exists('.tracker-import .tracker-progress'), false, "a failed download is no longer busy");
    const selection = () => d.run(`done({
      query: document.querySelector('[data-feature="tracker-import:search"]').value,
      checked: document.querySelector('[data-feature="tracker-import:boats"] tbody input').checked,
      disabled: document.querySelector('.tracker-import button.primary').disabled
    });`);
    assert.deepEqual(await selection(), { query: "NACIRA", checked: true, disabled: false });
    await t.shot("positions-unavailable-list-kept");

    recover = true;
    await d.click('[data-feature="tracker-import:retry"]');
    assert.deepEqual(await selection(), { query: "NACIRA", checked: true, disabled: true });
    await d.waitGone('.tracker-import', { timeoutMs: 25_000 });
    await d.waitFor('.left-nav', { text: "12 NACIRA 69" });
    assert.match(await d.text('.track-list'), /1670/);
    assert.equal(requests.filter(p => p === "/JSON/rmsr2024/RaceSetup").length, 3);
    assert.equal(requests.filter(p => p === "/BIN/rmsr2024/AllPositions3").length, 8);
    assert.equal(requests.filter(p => p === "/rmsr2024.kml").length, 5);
    await t.shot("retry-recovered-via-kml");
    await t.shot("track-imported");
  },
};
