/** Excluding or filtering out an outlier brings the 3D and 2D scales back to the data left (asked 2026-10-04). */
import { assert, newProject } from "./harness.mjs";

const f = id => `[data-feature="${id}"]`;
async function type(d, id, value) {
  await d.run(`document.querySelector(arguments[0]).focus(); done(true);`, [f(id)]);
  await d.type(f(id), String(value));
}
/** The largest speed the 3D axes label, in knots. Labels move when the view draws, which a throttled background window does late: so draw first. */
const largest = d => d.run(`var c = document.querySelector(".view3d canvas"); if (c && c.__peRedraw) c.__peRedraw();
  done(Math.max(0, ...Array.from(document.querySelectorAll(".view3d-labels span"))
  .map(function (s) { var m = /^([0-9.]+) kn$/.exec(s.textContent); return m ? Number(m[1]) : 0; })));`);

/** How far apart the 0° and 180° labels are on screen, in pixels. */
const onScreen = d => d.run(`var c = document.querySelector(".view3d canvas"); if (c && c.__peRedraw) c.__peRedraw();
  var at = {};
  Array.from(document.querySelectorAll(".view3d-labels span")).forEach(function (s) { if (s.textContent === "0°" || s.textContent === "180°") at[s.textContent] = s.getBoundingClientRect(); });
  done(at["0°"] && at["180°"] ? Math.hypot(at["0°"].x - at["180°"].x, at["0°"].y - at["180°"].y) : 0);`);

export default {
  name: "outlier scales",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Outlier");
    // One sample at 30 kn in 35 kn of wind among 6 kn ones in 10 kn.
    await d.queueDialog([t.path("tools/webdriver/fixtures/outlier.csv")]);
    await d.click(f("tracks:import-file"));
    await d.waitFor(".track-import", { visible: true });
    await d.click(".track-import .modal-actions button.primary");
    await d.waitGone(".track-import");
    await d.waitFor(".track-list", { text: "outlier" });
    await d.click(f("tracks:filters"));
    await type(d, "tracks:min-bsp", "");
    await type(d, "tracks:manoeuvre", "");
    await d.waitFor(".track-list", { text: "8 of 8 samples used" });
    await d.waitFor(".wave-range-footer", { text: "8 samples shown" });
    const wide = await largest(d);
    assert.ok(wide >= 30, `the outlier is on the scale first: ${wide} kn`);
    await t.shot("3d-with-outlier");
    const spread = await onScreen(d);
    await d.click(f("stage:2d"));
    await d.waitFor(".polar-plot-stage canvas", { visible: true });
    await new Promise(r => setTimeout(r, 500));
    await t.shot("2d-with-outlier");
    await d.click(f("stage:3d"));

    await type(d, "tracks:max-bsp", 10);
    await d.waitFor(".track-list", { text: "7 of 8 samples used" });
    await d.waitFor(".wave-range-footer", { text: "7 samples shown" });
    let narrow = 0;
    for (let i = 0; i < 50 && (narrow = await largest(d)) >= 30; i++) await new Promise(r => setTimeout(r, 100));
    const texts = await d.run(`done(Array.from(document.querySelectorAll(".view3d-labels span")).map(function (s) { return s.textContent; }));`);
    assert.ok(narrow > 0 && narrow <= 12, `the scales fit what is left: ${narrow} kn; labels ${JSON.stringify(texts)}`);
    // The camera follows: the 0°–180° labels keep roughly the room they had.
    let after = 0;
    for (let i = 0; i < 20 && (after = await onScreen(d)) < spread * 0.6; i++) await new Promise(r => setTimeout(r, 100));
    assert.ok(after >= spread * 0.6, `the polar still fills the view: ${after} px against ${spread} px`);
    await t.shot("3d-outlier-filtered");
    await d.click(f("stage:2d"));
    await d.waitFor(".polar-plot-stage canvas", { visible: true });
    await new Promise(r => setTimeout(r, 500));
    await t.shot("2d-outlier-filtered");

    // Excluded rather than filtered: hidden, and the scales with it, unless
    // "Excluded points" is ticked (asked 2026-10-04).
    await d.click(f("stage:3d"));
    await type(d, "tracks:max-bsp", "");
    await d.waitFor(".track-list", { text: "8 of 8 samples used" });
    let back = 0;
    for (let i = 0; i < 50 && (back = await largest(d)) < 30; i++) await new Promise(r => setTimeout(r, 100));
    assert.ok(back >= 30, `the outlier is back on the scale: ${back} kn`);
    // Box round the outlier: every sample is at 90°, it alone past 20 kn.
    // First let the camera finish following the box (two equal readings).
    for (let i = 0, last = -1, now = 0; i < 30 && (now = await onScreen(d)) !== last; i++) {
      last = now;
      await new Promise(r => setTimeout(r, 200));
    }
    await d.click(f("view3d:tool-box"));
    const picked = await d.run(`
      var c = document.querySelector(".view3d canvas"); if (c && c.__peRedraw) c.__peRedraw();
      var spans = Array.from(document.querySelectorAll(".view3d-labels span"));
      var at = function (text) { return spans.filter(function (s) { return s.textContent === text; })
        .map(function (s) { return s.getBoundingClientRect(); }).sort(function (a, b) { return b.x - a.x; })[0]; };
      var twenty = at("20 kn"), thirty = at("30 kn");
      var canvas = document.querySelector(".view3d canvas");
      var y = twenty.y + twenty.height / 2, x1 = twenty.x + twenty.width + 4, x2 = thirty.x + thirty.width + 60;
      var fire = function (type, x, yy, buttons) { canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: yy, button: 0, buttons: buttons, pointerId: 1 })); };
      fire("pointerdown", x1, y - 30, 1); fire("pointermove", (x1 + x2) / 2, y, 1); fire("pointermove", x2, y + 30, 1); fire("pointerup", x2, y + 30, 0);
      setTimeout(function () { done(document.querySelector(".view3d-selection h3").textContent); }, 300);`);
    assert.equal(picked, "1 selected", `the box picks the outlier alone`);
    await d.click(f("view3d:exclude"));
    await d.waitFor(".view3d-selection h3", { text: "Nothing selected" });
    let hidden = 0;
    for (let i = 0; i < 50 && (hidden = await largest(d)) >= 30; i++) await new Promise(r => setTimeout(r, 100));
    assert.ok(hidden > 0 && hidden <= 12, `an excluded outlier is hidden, scales and all: ${hidden} kn`);
    await t.shot("3d-outlier-excluded");
    await d.click(f("view3d:show-excluded"));
    let shown = 0;
    for (let i = 0; i < 50 && (shown = await largest(d)) < 30; i++) await new Promise(r => setTimeout(r, 100));
    assert.ok(shown >= 30, `shown again, it is on the scale: ${shown} kn`);
    await t.shot("3d-excluded-shown");
    await d.click(f("view3d:show-excluded"));
    for (let i = 0; i < 50 && (hidden = await largest(d)) >= 30; i++) await new Promise(r => setTimeout(r, 100));
    assert.ok(hidden <= 12, `and hidden again: ${hidden} kn`);
    // A track sample's speed is over the ground: its tooltip says SOG, not
    // BSP (asked 2026-10-04). The fixture has no current, so nothing is STW.
    await d.click(f("view3d:tool-rotate"));
    const tip = await d.run(`
      var c = document.querySelector(".view3d canvas"); if (c.__peRedraw) c.__peRedraw();
      var at = c.__peInspectPolar().at(0, 90, 10, 6.0), r = c.getBoundingClientRect();
      c.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: r.left + at[0], clientY: r.top + at[1], buttons: 0, pointerId: 1 }));
      setTimeout(function () { var t = document.querySelector(".view3d-tooltip"); done(t ? Array.from(t.querySelectorAll("dt")).map(function (e) { return e.textContent; }) : null); }, 300);`);
    assert.ok(tip && tip.includes("SOG") && !tip.includes("BSP"), `the sample's tooltip says SOG: ${JSON.stringify(tip)}`);
    await t.shot("3d-sample-tooltip");
    await d.click(f("stage:2d"));
    await d.waitFor(".polar-plot-stage canvas", { visible: true });
    await new Promise(r => setTimeout(r, 500));
    await t.shot("2d-outlier-excluded");
    await d.click(f("plot:show-excluded"));
    await new Promise(r => setTimeout(r, 500));
    await t.shot("2d-excluded-shown");
  },
};
