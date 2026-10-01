/** M19: offline catalogue, filters, full-circle plots, corrections and priorities. */
import { assert, colourPixels, distinctColours } from "./harness.mjs";

const f = (id) => `[data-feature="${id}"]`;
async function choose(d, id, value) {
  await d.run(`var el = document.querySelector(arguments[0]); el.value = arguments[1]; el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`, [f(id), value]);
}
async function number(d, id, value) { await d.type(f(id), String(value)); await d.key("Enter", f(id)); }

export default {
  name: "polar analysis",
  async run(t) {
    const d = t.driver;
    await d.open(t.path("tools/webdriver/fixtures/analysis.wpsproj"));
    await d.waitFor(".track-list li", { text: "Minute samples" });
    assert.equal(await d.exists(f("tracks:export-grib")), false);

    await choose(d, "orc:catalogue", "orr");
    await d.waitFor(".orc-count", { text: "of 600" });
    const first = await d.text(".orc-results li:first-child .orc-name");
    await d.click(f("orc:next-page"));
    await d.waitFor(".orc-count", { text: "51–100" });
    // The page number changes before the asynchronous search returns its rows.
    let next = first;
    for (let attempt = 0; attempt < 100 && (next === null || next === first); attempt++) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      next = await d.text(".orc-results li:first-child .orc-name");
    }
    assert.notEqual(next, null);
    assert.notEqual(next, first);
    await d.click(f("orc:fields"));
    await number(d, "orc-measure:loa-min", 12);
    await number(d, "orc-measure:loa-max", 13);
    await d.type(f("orc:search"), "Phoenix");
    await d.waitFor(".orc-results li .orc-name", { text: "PHOENIX" });
    await d.type(f("orc:field-builder"), "TPI");
    await d.waitFor(".orc-results li .orc-meta", { text: "1994 · TPI" });
    await d.click(f("orc:fields"));
    await d.click(f("orc:add"));
    await d.waitFor(".orc-added li", { text: "PHOENIX" });
    await d.waitFor('.orc-results li:first-child [data-feature="orc:add"][disabled]');
    await t.shot("orr-measurements-and-import");

    await d.click(f("shell:asymmetric"));
    await d.waitFor('[data-feature="shell:asymmetric"]:checked:not(:disabled)');
    await d.run(`var toggle = document.querySelector('[data-feature="shell:asymmetric"]'); toggle.focus();
      toggle.dispatchEvent(new KeyboardEvent("keydown", { key: "z",
      metaKey: /Mac/.test(navigator.platform), ctrlKey: !/Mac/.test(navigator.platform), bubbles: true })); done(true);`);
    await d.waitFor('[data-feature="shell:asymmetric"]:not(:checked):not(:disabled)');
    await d.click(f("sources:blend-settings"));
    assert.ok((await d.text(f("blend-settings:twa"))).endsWith("180"), "undo restores the symmetric grid");
    await d.key("Escape");
    await d.waitGone(".blend-settings");
    await d.run(`var toggle = document.querySelector('[data-feature="shell:asymmetric"]'); toggle.focus();
      toggle.dispatchEvent(new KeyboardEvent("keydown", { key: "z", shiftKey: true,
      metaKey: /Mac/.test(navigator.platform), ctrlKey: !/Mac/.test(navigator.platform), bubbles: true })); done(true);`);
    await d.waitFor('[data-feature="shell:asymmetric"]:checked:not(:disabled)');
    await d.click(f("sources:blend-settings"));
    assert.equal(await d.exists('.blend-settings [data-feature="shell:asymmetric"]'), false);
    assert.ok((await d.text(f("blend-settings:twa"))).endsWith("360"), "redo restores the full-circle grid");
    await choose(d, "blend-settings:interpolation", "monotone_spline");
    await choose(d, "blend-settings:twa-step", "10");
    await choose(d, "blend-settings:tws-step", "5");
    assert.ok((await d.run(`done(document.querySelector('[data-feature="blend-settings:twa"]').value);`)).endsWith("360"));
    await t.shot("full-circle-settings");
    await d.click(".blend-settings .modal-actions button.primary");
    await d.waitGone(".blend-settings");
    await d.click(f("stage:3d"));
    await d.waitFor("canvas.view3d-canvas", { visible: true });
    let angles = [];
    for (let i = 0; i < 80 && angles.length !== 12; i++) {
      angles = await d.run(`done(Array.from(document.querySelectorAll('.view3d-labels span'), el => el.textContent).filter(text => text.endsWith('°')));`);
      if (angles.length !== 12) await new Promise(resolve => setTimeout(resolve, 150));
    }
    assert.deepEqual(angles, ["0°", "30°", "60°", "90°", "120°", "150°", "180°", "150°", "120°", "90°", "60°", "30°"]);
    let pixels = 0;
    for (let i = 0; i < 80 && pixels < 5000; i++) {
      pixels = await colourPixels(d, "canvas.view3d-canvas", "#e64b83", 80);
      if (pixels < 5000) await new Promise((r) => setTimeout(r, 150));
    }
    assert.ok(pixels > 5000, "the full-circle surfaces have arrived and are painted");
    assert.ok(await distinctColours(d, "canvas.view3d-canvas") > 8);
    await t.shot("full-circle-plots");
    await d.click(f("plot:full-size"));
    await d.waitFor(".polar-plot-overlay canvas", { visible: true });
    await t.shot("asymmetric-2d-angle-labels");
    await d.click(f("plot:close"));
    await d.waitGone(".polar-plot-overlay");

    await d.click(f("sources:blend-edit"));
    await d.waitFor(".blend-correction .view3d-edit-table-wrap input");
    const cell = '.blend-correction input[aria-label="BSP at 270° and 10 kn"]';
    await d.type(cell, "12.34");
    await d.key("Enter", cell);
    await d.waitFor(".blend-correction td.edited input");
    await d.run(`document.querySelector(arguments[0]).scrollIntoView({ block: "center", inline: "nearest" }); done(true);`, [cell]);
    await t.shot("port-blend-correction");
    await d.click('.blend-correction [data-feature="edit:done"]');
    await d.waitGone(".blend-correction");

    await d.click(f("tracks:filters"));
    await number(d, "tracks:tack-window", 60);
    await number(d, "tracks:stop-speed", 0.5);
    await number(d, "tracks:stop-window", 60);
    await d.click(f("tracks:unknown-wave"));
    await t.shot("individual-point-filters");
    await d.click(f("tracks:filters"));
    await d.click(f("view3d:global-filters"));
    await d.click(f("view3d:global-filters-enabled"));
    await d.waitFor(f("global-filters:unknown-current"));
    await d.click(f("global-filters:unknown-current"));
    await choose(d, "global-filters:utc-unit", "minutes");
    await number(d, "global-filters:utc-interval", 2);
    assert.equal(await d.exists(f("global-filters:time-start")), false);
    await choose(d, "view3d:colour", "time");
    await d.waitFor(".view3d-ramp", { text: "UTC" });
    assert.match(await d.text(".view3d-ramp"), /2026-09-30/);
    await t.shot("global-filters-and-utc-colour");
    await d.click(f("view3d:global-filters"));
    await d.click(f("view3d:priority-filters"));
    await d.click(f("priority:add"));
    await d.click(f("priority:group"));
    await d.click(f("priority-filters:unknown-wave"));
    await d.click(f("priority:group"));
    await d.click(f("priority:add"));
    await t.shot("per-cell-priority-groups");

    await d.click(f("shell:settings"));
    await d.run(`document.querySelector('[data-section="settings:orr"]').scrollIntoView({ block: "start" }); done(true);`);
    await d.waitFor(".orr-scraper", { text: "600 local polar variants" });
    assert.match(await d.text(".orr-scraper"), /complete public ORR certificates, ratings/);
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:orr-scrape"]').disabled);`), false);
    await t.shot("orr-refresh-settings");
    await d.click(f("settings:close"));
  },
};
