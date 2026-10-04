import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { assert } from "./harness.mjs";
const base = new URL("../../../crates/pe-trackers/tests/fixtures/yellowbrick/", import.meta.url);
let fixture;
export default {
  name: "tracker creates model-matched boat tabs",
  async setup() {
    const setup = JSON.parse((await readFile(new URL("rmsr2024-RaceSetup.json", base))).toString("latin1"));
    setup.teams = setup.teams.slice(0,2);
    setup.teams[0].model = "Farr 40"; setup.teams[0].mmsi = "123456789";
    setup.teams[0].loa = "12.2 m"; setup.teams[0].class = "IRC 1";
    setup.teams[1].name = setup.teams[0].name; delete setup.teams[1].model;
    const routes = new Map([
      ["/JSON/rmsr2024/RaceSetup", Buffer.from(JSON.stringify(setup), "latin1")],
      ["/BIN/rmsr2024/AllPositions3", await readFile(new URL("rmsr2024-AllPositions3-first3.bin", base))],
    ]);
    const requests=[]; const server=createServer((req,res)=>{ requests.push(req.url); const body=routes.get(req.url.split("?")[0]); res.writeHead(body?200:404); res.end(body); });
    await new Promise(done=>server.listen(0,"127.0.0.1",done));
    fixture={requests,server};
    return {env:{PE_DRIVER_YELLOWBRICK:`http://127.0.0.1:${server.address().port}`},teardown:()=>new Promise(done=>server.close(done))};
  },
  async run(t) {
    const d=t.driver;
    await d.click('[data-feature="start:tracker"]');
    await d.waitFor('[data-feature="boats:tracker-url"]');
    await d.type('[data-feature="boats:tracker-url"]',"https://yb.tl/rmsr2024");
    await d.click('[data-feature="boats:tracker-open"]');
    await d.waitFor('.boat-import-report tbody tr',{timeoutMs:60000});
    const report=await d.run(`done(Array.from(document.querySelectorAll('.boat-import-report tbody tr')).map(r=>Array.from(r.cells).map(c=>c.textContent)));`);
    assert.equal(report.length,2); assert.ok(Number(report[0][2])>0,"explicit identical model adds polars");
    assert.equal(Number(report[1][2]),0,"same boat name without model never adds polars");
    assert.ok(Number(report[0][3])>0); assert.ok(Number(report[1][3])>0);
    for (const detail of ["123456789", "Farr 40", "12.2 m", "IRC 1"]) assert.ok(report[0][0].includes(detail), `boat list includes ${detail}`);
    await t.shot("model-match-report");
    assert.equal(await d.invoke("project_summary"), null, "boat list leaves the start screen session empty");
    await d.click('[data-feature="boats:tracker-cancel"]');
    await d.waitGone('.tracker-project');
    assert.equal(await d.invoke("project_summary"), null, "cancel does not open the preview");
    await d.waitFor('[data-feature="start:tracker"]');
    await d.click('[data-feature="start:tracker"]');
    await d.waitFor('[data-feature="boats:tracker-url"]');
    await d.type('[data-feature="boats:tracker-url"]',"https://yb.tl/rmsr2024");
    await d.click('[data-feature="boats:tracker-open"]');
    await d.waitFor('.boat-import-report tbody tr',{timeoutMs:60000});
    await d.click('[data-feature="boats:tracker-close"]');
    await d.waitFor('.boat-tab:nth-child(2) [role="tab"]'); assert.equal(await d.count('.boat-tabs [role="tab"]'),2);
    await d.waitFor('.track-list li'); await t.shot("tracker-project-open");
    assert.ok(fixture.requests.some(r=>r.includes('AllPositions3')));
    // Each imported track exposes weather download after the race import.
    assert.ok(await d.count('[data-feature="tracks:fetch-weather"]')>0);
    await d.click('.titlebar [data-feature="boats:add"]');
    await d.waitFor('.boat-tabs [role="tab"]',{text:"Polar 3"});
    assert.equal(await d.count('.boat-tabs [role="tab"]'),3,"tracker projects can add boats");
    const previous = await d.invoke("project_summary");
    await d.click('[data-feature="shell:project-menu"]');
    await d.click('[data-feature="project:tracker"]');
    await d.waitFor('[role="dialog"][aria-label="Unsaved changes"] button.danger');
    await d.click('[role="dialog"][aria-label="Unsaved changes"] button.danger');
    await d.waitFor('[data-feature="boats:tracker-match"]');
    await d.run(`var s=document.querySelector('[data-feature="boats:tracker-match"]'); s.value='exact_boat'; s.dispatchEvent(new Event('change',{bubbles:true})); done(true);`);
    await d.type('[data-feature="boats:tracker-url"]',"https://yb.tl/rmsr2024");
    await t.shot("exact-boat-option");
    await d.click('[data-feature="boats:tracker-open"]');
    await d.waitFor('.boat-import-report tbody tr',{timeoutMs:60000});
    const exact=await d.run(`done(Array.from(document.querySelectorAll('.boat-import-report tbody tr')).map(r=>Array.from(r.cells).map(c=>c.textContent)));`);
    assert.equal(exact.length,2); assert.equal(Number(exact[0][2]),0,"the model alone never adds polars in exact-boat mode");
    assert.ok(Number(exact[0][3])>0,"the entrant retains its own race track");
    await t.shot("exact-boat-report");
    assert.deepEqual(await d.invoke("project_summary"), previous, "review does not replace unsaved work");
    await d.click('[data-feature="boats:tracker-cancel"]');
    await d.waitGone('.tracker-project');
    assert.deepEqual(await d.invoke("project_summary"), previous, "cancel preserves the current project");
    assert.equal(await d.count('.boat-tabs [role="tab"]'),3,"cancel retains the added boat");
    await t.shot("cancel-keeps-current-project");
  }
};
