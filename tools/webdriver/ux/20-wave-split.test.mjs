/**
 * M24: Split Wave Angle (spec.md 10.5). In single view the 3D polar is drawn
 * once per wave direction as seen from the boat: 4 to 36 copies on one
 * canvas, each with its direction's samples and a big arrow, turning
 * together, clear of every panel. Offline: the analysis fixture's track has
 * its waves saved with it.
 *
 * The counts are worked out by hand from `analysis_fixture.rs`: 120 samples,
 * minute k heading 90° (k < 60) or 270°, waves from 3k° on the 90 where k is
 * not a multiple of 4, a 0.2 kn current setting north where k is not a
 * multiple of 5, which turns the heading through the water 1.8° (to 91.8°
 * and 268.2°) at 6.3 kn. So the waves come from 87° either side of the bow
 * and never from astern.
 */
import { assert } from "./harness.mjs";

const f = (id) => `[data-feature="${id}"]`;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function slide(d, index) {
  await d.run(`var el = document.querySelector(arguments[0]);
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, String(arguments[1]));
    el.dispatchEvent(new Event("input", { bubbles: true })); done(true);`, [f("view3d:wave-split-count"), index]);
}

/** The copies' frames, and whether any lies under a panel or control. */
function copies(d) {
  return d.run(`
    var view = document.querySelector(".view3d"), stage = view.getBoundingClientRect();
    var blockers = [".sidebar.left", ".sidebar.right"].map(function (s) { return view.closest(".workspace").querySelector(s); })
      .concat([".view3d-toolbar", ".view3d-side", ".wave-display-ranges"].map(function (s) { return view.querySelector(s); }))
      .filter(Boolean).map(function (el) { return el.getBoundingClientRect(); });
    var cells = Array.from(document.querySelectorAll(".wave-split-cell")).map(function (cell) {
      var r = cell.getBoundingClientRect(), arrow = cell.querySelector(".wave-split-arrow").getBoundingClientRect();
      var name = cell.querySelector(".wave-split-name").getBoundingClientRect();
      return { x: r.left - stage.left, y: r.top - stage.top, width: r.width, height: r.height,
        name: cell.querySelector(".wave-split-name").textContent, total: parseInt(cell.querySelector(".wave-split-total").textContent, 10),
        direction: cell.querySelector(".wave-split-arrow").dataset.direction, arrow: arrow.width,
        covered: arrow.right > name.left && arrow.bottom > name.top,
        hidden: blockers.some(function (b) { return r.left < b.right - 1 && r.right > b.left + 1 && r.top < b.bottom - 1 && r.bottom > b.top + 1; }) };
    });
    var none = document.querySelector(".wave-split-none");
    done({ cells: cells, none: none ? none.textContent : null,
      shown: parseInt(document.querySelector('.wave-range-footer [role="status"]').textContent, 10) });`);
}

export default {
  name: "the 3D view split by wave direction",
  async run(t) {
    const d = t.driver;
    await d.open(t.path("tools/webdriver/fixtures/analysis.wpsproj"));
    await d.waitFor(".track-list li", { text: "Minute samples" });
    await d.waitFor("canvas.view3d-canvas", { visible: true });

    // On first load the origin sits in the middle of what the panels and
    // the view's controls leave in sight, with all of the polar there
    // (asked 2026-10-02 and 2026-10-03).
    const framed = await d.run(`var c = document.querySelector("canvas.view3d-canvas"), r = c.getBoundingClientRect(), p = c.__peInspectPolar().points;
      var view = c.parentElement, ws = view.closest(".workspace");
      var box = function (s, root) { var el = (root || view).querySelector(s); return el ? el.getBoundingClientRect() : null; };
      var left = box(".sidebar.left", ws), right = box(".sidebar.right", ws), side = box(".view3d-side"), bar = box(".view3d-toolbar"), waves = box(".wave-display-ranges");
      var free = { l: left ? left.right : r.left, r: Math.min(right ? right.left : r.right, side ? side.left : r.right), t: bar ? bar.bottom : r.top, b: waves ? waves.top : r.bottom };
      var xs = [], ys = [];
      for (var i = 0; i < p.length; i += 2) if (isFinite(p[i])) { xs.push(r.left + p[i]); ys.push(r.top + p[i + 1]); }
      var dots = { l: Math.min.apply(null, xs), r: Math.max.apply(null, xs), t: Math.min.apply(null, ys), b: Math.max.apply(null, ys) };
      var origin = c.__peInspectPolar().at(0, 0, 0, 0);
      done({ free: free, dots: dots, origin: [r.left + origin[0], r.top + origin[1]] });`);
    const mid = (a, b) => (a + b) / 2;
    const { free, dots } = framed;
    assert.ok(dots.l >= free.l - 2 && dots.r <= free.r + 2 && dots.t >= free.t - 2 && dots.b <= free.b + 2, `every dot in sight: ${JSON.stringify(framed)}`);
    // Within the few pixels of margin the view keeps beside its side panel.
    assert.ok(Math.abs(framed.origin[0] - mid(free.l, free.r)) < 8 && Math.abs(framed.origin[1] - mid(free.t, free.b)) < 8,
      `the origin in the middle: ${JSON.stringify(framed)}`);
    await t.shot("first-load-centred");

    // Beside the Box tool, off, with nothing else of it showing.
    assert.equal(await d.text(f("view3d:wave-split")), "Split Wave Angle");
    assert.equal(await d.run(`done(document.querySelector('[data-feature="view3d:wave-split"]').getAttribute("aria-pressed"));`), "false");
    assert.equal(await d.exists(f("view3d:wave-split-count")), false);
    assert.equal(await d.exists(".wave-split-cell"), false);

    await d.click(f("view3d:wave-split"));
    await d.waitFor(".wave-split-count", { text: "8 directions" });
    let seen = await copies(d);
    assert.equal(seen.cells.length, 8);
    assert.deepEqual(seen.cells.map((cell) => cell.name),
      ["From 0°", "From 45°", "From 90°", "From 135°", "From 180°", "From 225°", "From 270°", "From 315°"]);
    assert.ok(seen.cells.every((cell) => !cell.hidden), "no copy lies under a panel or the toolbar");
    assert.ok(seen.cells.every((cell) => cell.arrow >= 30 && !cell.covered), "every copy has its arrow, clear of its name");
    assert.deepEqual(seen.cells.map((cell) => cell.total), [22, 23, 11, 0, 0, 0, 11, 23], "each copy holds its direction's samples");
    // The thirty with no waves are in no copy, and the controls say so.
    assert.equal(seen.shown, 120);
    assert.equal(seen.none, "30 without a wave direction");
    await t.shot("eight-directions");

    // Four: bow, starboard beam, stern, port beam.
    await slide(d, 0);
    await d.waitFor(".wave-split-count", { text: "4 directions" });
    seen = await copies(d);
    assert.deepEqual(seen.cells.map((cell) => cell.direction), ["0", "90", "180", "270"]);
    assert.deepEqual(seen.cells.map((cell) => cell.total), [44, 24, 0, 22]);
    await t.shot("four-directions-from");

    // To: where the waves go is the opposite quarter.
    await d.run(`var el = document.querySelector(arguments[0]); el.value = "to";
      el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`, [f("view3d:wave-split-sense")]);
    await d.waitFor(".wave-split-name", { text: "To 0°" });
    seen = await copies(d);
    assert.deepEqual(seen.cells.map((cell) => cell.total), [0, 22, 44, 24], "To is From turned half round");
    await t.shot("four-directions-to");
    await d.run(`var el = document.querySelector(arguments[0]); el.value = "from";
      el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`, [f("view3d:wave-split-sense")]);

    // The wave filters still decide what there is to split.
    await d.run(`var el = document.querySelector('[data-feature="view3d:wave-height-min"]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, String(Number(el.max) * 0.6));
      el.dispatchEvent(new Event("input", { bubbles: true })); done(true);`);
    await pause(300);
    const filtered = await copies(d);
    assert.ok(filtered.shown > 0 && filtered.shown < 90, "raising the least wave height shows fewer samples");
    assert.equal(filtered.cells.reduce((sum, cell) => sum + cell.total, 0), filtered.shown, "and the copies hold exactly those");
    assert.equal(filtered.none, null, "a sample with no waves is outside a narrowed range");
    await d.click(f("view3d:wave-ranges-reset"));

    // Hovering a sample in one copy marks the same wind in the others that have it.
    await slide(d, 1);
    await d.waitFor(".wave-split-count", { text: "8 directions" });
    await d.click(f("view3d:show-surfaces"));
    await d.click(f("view3d:show-nodes"));
    // (The scene is fetched again after the ranges were reset, which clears a
    // hover made too soon: hover until one holds.)
    let marks = 0;
    for (let attempt = 0; attempt < 20 && marks === 0; attempt++) {
      await d.run(`
        var c = document.querySelector("canvas.view3d-canvas"), p = c.__peInspectPolar().points;
        var cells = Array.from(document.querySelectorAll(".wave-split-cell")).map(function (el) {
          return { rect: el.getBoundingClientRect(), total: parseInt(el.querySelector(".wave-split-total").textContent, 10) }; });
        var busiest = cells.reduce(function (a, b) { return b.total > a.total ? b : a; });
        for (var i = 0; i < p.length; i += 2) {
          if (!isFinite(p[i])) continue;
          var x = busiest.rect.left + p[i], y = busiest.rect.top + p[i + 1];
          if (document.elementFromPoint(x, y) !== c) continue;
          c.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse", clientX: x + arguments[0] % 2, clientY: y, buttons: 0 }));
          break;
        }
        done(true);`, [attempt]);
      await pause(400);
      marks = await d.count(".wave-split-mark");
    }
    // Waves from 45° were met at 6.0 kn in a 10 kn beam wind; so were those from the four other directions with samples.
    assert.ok(await d.exists('.view3d [role="tooltip"]'), "the hovered sample's tooltip");
    assert.equal(marks, 4, "the same wind is marked in the four other copies that have it");
    await t.shot("hover-marks-the-same-wind");
    await d.click(f("view3d:show-surfaces"));
    await d.click(f("view3d:show-nodes"));

    // Each copy's blend is its own: with the dots out of the way, the blend
    // under a sample's place in the 45° copy counts the track, the one in
    // the 135° copy (no sample) only the polar file, and the marks in the
    // other copies carry their own blends' speeds.
    await d.click(f("view3d:show-samples"));
    await d.click(f("view3d:show-nodes"));
    // A canvas point over the 90°, 10 kn cell of a copy's blend: the blend's
    // own grid is the output grid (linear mode), so that cell is node (2, 1).
    const blendOf = async (copy) => {
      let found = null;
      for (let attempt = 0; attempt < 20 && !found; attempt++) {
        found = await d.run(`
          var c = document.querySelector("canvas.view3d-canvas"), i = c.__peInspectPolar();
          var stage = c.getBoundingClientRect();
          var tried = [];
          for (var bsp = 5.2; bsp <= 7.0; bsp += 0.1) {
            var at = i.at(arguments[0], 90, 10, bsp);
            if (!at) continue;
            var x = at[0], y = at[1];
            var hit = i.surfaceAt(x, y);
            tried.push(bsp.toFixed(1) + ":" + JSON.stringify(hit));
            if (hit && hit.twaIndex === 2 && hit.twsIndex === 1) {
              c.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse",
                clientX: stage.left + x + arguments[1] % 2, clientY: stage.top + y, buttons: 0 }));
              done({ bsp: bsp });
              return;
            }
          }
          done({ tried: tried });`, [copy, attempt]);
        await pause(400);
        if (found && found.tried) { if (attempt === 19) throw new Error("no point over the cell; seen " + found.tried.join(" ")); found = null; }
        else if (found && !(await d.exists(".blend-cell-tooltip"))) found = null;
      }
      assert.ok(found, "a point over the 90° 10 kn cell of copy " + copy);
      return d.run(`var tip = document.querySelector(".blend-cell-tooltip");
        done({ text: tip ? tip.textContent : null, marks: Array.from(document.querySelectorAll(".wave-split-mark")).map(function (m) { return m.textContent; }) });`);
    };
    const beam = await blendOf(1);
    assert.ok(beam.text && beam.text.includes("Minute samples") && beam.text.includes("Independent sides"), beam.text);
    assert.equal(beam.marks.length, 7, "every other copy has the file's blend at least");
    assert.ok(new Set(beam.marks).size > 1, "the copies' blends differ: " + beam.marks.join(", "));
    await t.shot("each-copy-blends-its-own-samples");
    const quarter = await blendOf(3);
    assert.ok(quarter.text && !quarter.text.includes("Minute samples") && quarter.text.includes("Independent sides"), quarter.text);
    assert.equal(await d.count(".wave-split-arc"), 8, "each copy shows its share of the circle");
    await d.click(f("view3d:show-samples"));
    await d.click(f("view3d:show-nodes"));

    // Thirty-six copies, and the room a closed panel gives back.
    await slide(d, 5);
    await d.waitFor(".wave-split-count", { text: "36 directions" });
    seen = await copies(d);
    assert.equal(seen.cells.length, 36);
    assert.ok(seen.cells.every((cell) => !cell.hidden));
    assert.ok(seen.cells.every((cell) => !cell.covered && cell.arrow >= 16), "small copies keep the arrow clear of the name");
    // Painted on the frame after the thirty-six blends arrive.
    let labelled = -1;
    for (let attempt = 0; attempt < 50 && labelled !== 0; attempt++) {
      await pause(200);
      labelled = await d.run(`done(Array.from(document.querySelectorAll(".view3d-labels span")).filter(function (s) { return s.style.display !== "none"; }).length);`);
    }
    assert.equal(labelled, 0, "and have no axis labels");
    assert.equal(seen.cells.reduce((sum, cell) => sum + cell.total, 0), 90);
    const narrow = seen.cells[0];
    await t.shot("thirty-six-directions");
    await d.click(f("dock:left"));
    await pause(400);
    seen = await copies(d);
    assert.ok(seen.cells.every((cell) => !cell.hidden));
    assert.ok(seen.cells[0].width * seen.cells[0].height > narrow.width * narrow.height, "the copies use the room a closed panel leaves");
    await d.click(f("dock:left"));

    // The camera is one: a preset turns every copy, and the frames stay put.
    await d.click(f("view3d:camera-iso"));
    await pause(300);
    await t.shot("thirty-six-isometric");

    // Off again: one view, nothing left behind.
    await d.click(f("view3d:wave-split"));
    await d.waitGone(".wave-split-cell");
    assert.equal(await d.exists(f("view3d:wave-split-count")), false);

    // A pane of split view does not offer it.
    await d.open(t.path("tools/webdriver/fixtures/boats.wpsproj"));
    await d.waitFor('.boat-tabs [role="tab"]', { text: "Delta" });
    assert.equal(await d.count(f("view3d:wave-split")), 1);
    await d.click(f("boats:split"));
    await d.waitFor(".boat-grid.boats-2");
    assert.equal(await d.count(f("view3d:wave-split")), 0, "split view has no wave split");
    assert.equal(await d.exists(".statusbar .hint.error"), false);
  },
};
