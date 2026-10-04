/**
 * M20: the renamed application, weights from 0 to 1, dots coloured by the
 * time of day, the blend's tooltip in 3D and on the plot, and the Measure
 * tool. Offline: the analysis fixture holds one polar file and one track
 * whose minute samples start at 12:00 UTC at 1°W, so 11:56 local solar
 * time: the first four are morning, the rest afternoon.
 */
import { assert, colourPixels } from "./harness.mjs";

const f = (id) => `[data-feature="${id}"]`;
const MORNING = "#e69f00", AFTERNOON = "#009e73", BLEND = "#e0457b";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function choose(d, id, value) {
  await d.run(`var el = document.querySelector(arguments[0]); el.value = arguments[1];
    el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`, [f(id), value]);
}

/** Sends a mouse event to the full-size plot's canvas at a fraction of its box; answers the client point. */
function plotMouse(d, type, fx, fy) {
  return d.run(`var c = document.querySelector(".polar-plot-stage canvas"), r = c.getBoundingClientRect();
    var x = r.left + r.width * arguments[1], y = r.top + r.height * arguments[2];
    c.dispatchEvent(new MouseEvent(arguments[0], { bubbles: true, clientX: x, clientY: y }));
    done({ x: x, y: y });`, [type, fx, fy]);
}

/**
 * Moves the pointer over the 3D canvas where a pixel has the blend's own
 * colour, skipping `skip` such pixels first so a retry lands elsewhere.
 */
function hoverBlendSurface(d, skip) {
  return d.run(`
    var c = document.querySelector("canvas.view3d-canvas"); c.__peRedraw?.();
    var copy = document.createElement("canvas"); copy.width = c.width; copy.height = c.height;
    var ctx = copy.getContext("2d"); ctx.drawImage(c, 0, 0);
    var data = ctx.getImageData(0, 0, copy.width, copy.height).data, rect = c.getBoundingClientRect();
    var hex = arguments[0], rgb = [1, 3, 5].map(function (i) { return parseInt(hex.slice(i, i + 2), 16); });
    var seen = 0;
    for (var y = 0; y < copy.height; y += 3) for (var x = 0; x < copy.width; x += 3) {
      var i = 4 * (y * copy.width + x);
      var cx = rect.left + x / copy.width * rect.width, cy = rect.top + y / copy.height * rect.height;
      if (cx < 390 || cx > 1050 || cy < 150 || cy > 680) continue;
      if (!rgb.every(function (v, k) { return Math.abs(data[i + k] - v) < 6; })) continue;
      if (seen++ < arguments[1]) continue;
      c.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: cx, clientY: cy, buttons: 0 }));
      done({ x: cx, y: cy }); return;
    }
    done(false);`, [BLEND, skip]);
}

export default {
  name: "time of day, blend tooltip, measure and the 0–1 weight",
  async run(t) {
    const d = t.driver;
    assert.equal(await d.run(`done(document.title);`), "PolarExplorer");
    await d.open(t.path("tools/webdriver/fixtures/analysis.wpsproj"));
    await d.waitFor(".track-list li", { text: "Minute samples" });

    // Weights run from 0 to 1 (the fixture, schema 3, migrates on open).
    assert.equal(await d.run(`var s = document.querySelector('[data-feature="sources:weight"]'); done(s.min + "–" + s.max);`), "0–1");

    // --- 3D: colour the dots by time of day ---
    await d.waitFor("canvas.view3d-canvas", { visible: true });
    await choose(d, "view3d:colour", "timeOfDay");
    await d.waitFor(".view3d-bands li", { text: "Night 21:00–05:00" });
    assert.deepEqual(await d.texts(".view3d-bands li"),
      ["Night 21:00–05:00", "Morning 05:00–12:00", "Afternoon 12:00–17:00", "Evening 17:00–21:00"]);
    await d.click(f("view3d:show-surfaces")); // the dots alone, so their colours are not seen through a surface
    assert.ok(await colourPixels(d, "canvas.view3d-canvas", AFTERNOON) > 0, "afternoon samples are drawn green");
    await t.shot("3d-dots-by-time-of-day");
    await d.click(f("view3d:show-surfaces"));
    await choose(d, "view3d:colour", "source");

    // --- 3D: hover the blend's surface ---
    let hovered = false;
    for (let attempt = 0; attempt < 12 && !await d.exists(".blend-cell-tooltip"); attempt++) {
      hovered = await hoverBlendSurface(d, attempt * 40);
      await pause(250);
    }
    assert.ok(hovered, "the blend's surface is drawn in its colour");
    await d.waitFor(".blend-cell-tooltip", { text: "Blend" });
    const tip = await d.text(".blend-cell-tooltip");
    assert.match(tip, /TWA\d+°TWS[\d.]+ knBSP[\d.]+ kn/);
    await t.shot("3d-blend-cell-tooltip");

    // --- The full-size plot: dots by time of day ---
    await d.click(f("stage:2d"));
    await d.waitFor(".polar-plot-stage canvas", { visible: true });
    await d.run(`var el = document.querySelector('.polar-plot-stage [data-feature="plot:colour"]'); el.value = "timeOfDay";
      el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`);
    await d.waitFor(".polar-plot-stage .polar-plot-bands li", { text: "Evening 17:00–21:00" });
    await pause(200);
    const morning = await colourPixels(d, ".polar-plot-stage canvas", MORNING);
    const afternoon = await colourPixels(d, ".polar-plot-stage canvas", AFTERNOON);
    assert.ok(afternoon > 0, "afternoon dots are green on the plot");
    assert.ok(morning + afternoon > 0);
    await t.shot("plot-dots-by-time-of-day");

    // --- Measure: every curve at the pointer's angle, then a pinned point ---
    await d.click('.polar-plot-stage [data-feature="plot:measure"]');
    assert.equal(await d.run(`done(document.querySelector('.polar-plot-stage [data-feature="plot:measure"]').getAttribute("aria-pressed"));`), "true");
    // The fan's centre is on the left edge at mid height: 90° is straight right.
    await plotMouse(d, "mousemove", 0.75, 0.5);
    await d.waitFor(".polar-plot-measure h4", { text: "At TWA 90°" });
    const rows = await d.texts(".polar-plot-measure li");
    assert.ok(rows.length >= 1, "a curve crosses the 90° spoke");
    assert.ok(rows.some((row) => row.includes("pointed at")), "one curve is the one compared against");
    assert.equal(await d.exists(".polar-plot-tooltip"), false, "the hover is off while measuring");
    await plotMouse(d, "click", 0.75, 0.5);
    await plotMouse(d, "mousemove", 0.7, 0.3);
    await d.waitFor(".polar-plot-measure-pin", { text: "B − A:" });
    assert.match(await d.text(".polar-plot-measure-pin"), /A: [\d.]+ kn at 90°B − A: [+−]?[\d.]+ kn \([+−]?[\d.]+%\), \d+° apart/);
    await t.shot("plot-measure-pinned");

    // Escape lets the pin go and leaves the overlay open; the next closes it.
    await d.key("Escape");
    await d.waitGone(".polar-plot-measure-pin");
    assert.ok(await d.exists(".polar-plot-stage"), "the first Escape only unpins");
    await d.click('.polar-plot-stage [data-feature="plot:measure"]');
    await d.waitGone(".polar-plot-measure");
    await d.click(f("stage:3d"));
    await d.waitGone(".polar-plot-stage");
  },
};
