import { assert, newProject, toHex } from "./harness.mjs";

const f = id => `[data-feature="${id}"]`;
const used = (d, n) => d.waitFor(".track-list", { text: `${n} of 12 samples used` });
async function choose(d, id, value) {
  await d.run(`var e = document.querySelector(arguments[0]); e.value = arguments[1]; e.dispatchEvent(new Event('change', { bubbles: true })); done(true);`, [f(id), value]);
}
async function type(d, id, value) {
  await d.run(`document.querySelector(arguments[0]).focus(); done(true);`, [f(id)]);
  await d.type(f(id), String(value)); // Deliberately no Enter or blur.
}

export default {
  name: "live analysis and supplied wind",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Live wind filters");
    await d.queueDialog([t.path("tools/webdriver/fixtures/supplied-wind.csv")]);
    await d.click(f("tracks:import-file"));
    await d.waitFor(".track-import", { visible: true });
    assert.equal(await d.text(f("track-import:tws")), "5");
    assert.equal(await d.text(f("track-import:twd")), "6");
    await t.shot("supplied-wind-column-mapping");
    await d.click(".track-import .modal-actions button.primary");
    await d.waitGone(".track-import");
    await d.waitFor(".track-list", { text: "supplied-wind" });
    await d.click(f("tracks:filters"));
    await type(d, "tracks:min-bsp", "");
    await type(d, "tracks:manoeuvre", "");
    await used(d, 12);
    await d.waitFor(".wave-range-footer", { text: "12 samples shown" });
    // The track provides wind, so the choice is offered (asked 2026-10-02); it provides headings and speeds too.
    assert.ok(await d.exists(f("tracks:heading-source")) && await d.exists(f("tracks:speed-source")));
    await choose(d, "tracks:wind-source", "derived");
    await d.waitFor(".wave-range-footer", { text: "0 samples shown" });
    await choose(d, "tracks:wind-source", "given");
    await d.waitFor(".wave-range-footer", { text: "12 samples shown" });
    await d.run(`document.querySelector('[data-feature="tracks:wind-source"]').scrollIntoView({ block: 'center' }); done(true);`);
    await t.shot("supplied-wind-selected");

    await type(d, "tracks:wind-speed-change", 2);
    await used(d, 9);
    assert.equal(await d.run(`done(document.activeElement.dataset.feature);`), "tracks:wind-speed-change", "blend updates while input still has focus");
    await t.shot("live-wind-change-filter");
    await type(d, "tracks:wind-speed-change", "");
    await used(d, 12);
    await type(d, "tracks:wind-direction-change", 10);
    await used(d, 9);
    await type(d, "tracks:wind-direction-change", "");
    await used(d, 12);
    // The fixture turns once, from 90° to 270° in a northerly: one tack, two
    // samples either side of it.
    await d.click(f("tracks:tacks"));
    await used(d, 10);
    await d.click(f("tracks:tacks"));
    await used(d, 12);
    // VMG: every sample is on a beam reach (0) but the one with the wind
    // from 30°, at 120° off the wind: 6.2 × cos 120° = −3.1 kn.
    await type(d, "tracks:vmg-min", 0);
    await used(d, 11);
    await type(d, "tracks:vmg-min", "");
    await type(d, "tracks:vmg-max", -1);
    await used(d, 1);
    await type(d, "tracks:vmg-max", "");
    await used(d, 12);
    // COG: the first six samples head east.
    await type(d, "tracks:cog-from", 80);
    await type(d, "tracks:cog-to", 100);
    await used(d, 6);
    await type(d, "tracks:cog-from", "");
    await used(d, 12);
    await type(d, "tracks:manoeuvre", 30);
    await used(d, 10);
    await type(d, "tracks:manoeuvre", "");
    await used(d, 12);
    await type(d, "tracks:awa-change", 20);
    // AWA changes at the observed stop, tack and wind shift.
    await used(d, 4);
    await d.waitFor(".wave-range-footer", { text: "4 samples shown" });
    await type(d, "tracks:awa-change", "");
    await used(d, 12);
    await d.click(f("tracks:filters"));
    await d.click(f("view3d:show-surfaces"));
    const colour = toHex(await d.run(`done(getComputedStyle(document.querySelector('.track-list .swatch')).backgroundColor);`));
    // Find a real painted dot, then send a pointer move at that canvas position.
    const hoverDot = () => d.run(`
      var c = document.querySelector('canvas.view3d-canvas'); c.__peRedraw?.();
      var copy = document.createElement('canvas'); copy.width = c.width; copy.height = c.height;
      var ctx = copy.getContext('2d'); ctx.drawImage(c, 0, 0);
      var data = ctx.getImageData(0, 0, copy.width, copy.height).data, rect = c.getBoundingClientRect();
      var hex = arguments[0], rgb = [1,3,5].map(i => parseInt(hex.slice(i,i+2),16));
      for (var y = 0; y < copy.height; y++) for (var x = 0; x < copy.width; x++) {
        var i = 4*(y*copy.width+x), cx = rect.left+x/copy.width*rect.width, cy=rect.top+y/copy.height*rect.height;
        if (cx < 390 || cx > 1050 || cy < 150 || cy > 680) continue;
        if (rgb.every((v,k) => Math.abs(data[i+k]-v)<5)) {
          c.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX:cx, clientY:cy, buttons:0 }));
          done({ x:cx, y:cy, rgb: Array.from(data.slice(i,i+3)) }); return;
        }
      }
      done(false);`, [colour]);
    let hovered = null;
    for (let attempt = 0; attempt < 10 && !await d.exists('.view3d-tooltip'); attempt++) {
      hovered = await hoverDot();
      await new Promise(resolve => setTimeout(resolve, 200));
    }
    assert.ok(hovered, "found an actual sample dot");
    await d.waitFor('.view3d-tooltip', { text: "supplied-wind" });
    await t.shot("dot-hover-details");
    assert.ok(!(await d.text('.titlebar')).includes("360°"));
  },
};
