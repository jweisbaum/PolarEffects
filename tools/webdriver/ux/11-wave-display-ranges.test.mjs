import { assert, colourPixels } from "./harness.mjs";

const f = id => `[data-feature="${id}"]`;
const count = (d, n) => d.waitFor('.wave-range-footer [role="status"]', { text: `${n} samples shown` });

async function checkSliders(d) {
  const measured = await d.run(`
    var view = document.querySelector('.view3d'), vr = view.getBoundingClientRect();
    var panel = document.querySelector('.wave-display-ranges'), pr = panel.getBoundingClientRect();
    var styles = getComputedStyle(view);
    var left = parseFloat(styles.getPropertyValue('--dock-left')) || 0;
    var right = parseFloat(styles.getPropertyValue('--dock-right')) || 0;
    var rows = Array.from(panel.querySelectorAll('.wave-range-slider')).map(row => {
      var inputs = Array.from(row.querySelectorAll('input')), r = row.getBoundingClientRect();
      var centres = inputs.map(input => r.left + 7 + (r.width - 14) * Number(input.value) / Number(input.max));
      var overlap = Math.abs(centres[0] - centres[1]) < 1;
      return { sameRow: inputs[0].getBoundingClientRect().y === inputs[1].getBoundingClientRect().y,
        reachable: inputs.map((input, i) => document.elementFromPoint(
          centres[i] + (overlap ? (i === 0 ? -3 : 3) : 0), r.top + r.height / 2) === input) };
    });
    done({ bottomGap: vr.bottom - pr.bottom,
      centreOffset: pr.left + pr.width / 2 - (vr.left + (vr.width + left - right) / 2),
      rows, inSide: Boolean(panel.closest('.view3d-side')) });
  `);
  assert.ok(Math.abs(measured.bottomGap - 12) < 1, "wave controls sit at the bottom of the view");
  assert.ok(Math.abs(measured.centreOffset) < 1, "wave controls are centred between the side panels");
  assert.equal(measured.inSide, false, "wave controls are outside the right-side control stack");
  assert.equal(measured.rows.length, 3);
  for (const row of measured.rows) {
    assert.equal(row.sameRow, true, "both handles share a single row");
    assert.deepEqual(row.reachable, [true, true], "each handle receives pointer input, including when touching");
  }
}

export default {
  name: "wave display ranges",
  async run(t) {
    const d = t.driver;
    await d.open(t.path("tools/webdriver/fixtures/analysis.wpsproj"));
    await d.waitFor(".track-list li", { text: "Minute samples" });
    // Use the fixture's recorded headings, independent of current correction.
    await d.click(f("sources:blend-settings"));
    await d.click(f("blend-settings:use-corrected"));
    await d.click(".blend-settings .modal-actions button.primary");
    await d.waitGone(".blend-settings");
    await count(d, 120);
    await checkSliders(d);
    for (const side of ["left", "right", "left", "right"]) {
      await d.click(f(`dock:${side}`));
      await checkSliders(d);
    }

    await t.shot("all-wave-samples");

    await d.click(f("sources:edit"));
    await d.waitFor(".view3d-edit");
    assert.ok(await d.run(`
      var edit = document.querySelector('.view3d-edit').getBoundingClientRect();
      var waves = document.querySelector('.wave-display-ranges').getBoundingClientRect();
      done(edit.bottom < waves.top);
    `), "the polar editor stays above the bottom sliders");
    await checkSliders(d);
    await t.shot("edit-with-wave-ranges");
    await d.click(f("edit:done"));
    await d.waitGone(".view3d-edit");

    await d.click(f("view3d:show-nodes"));
    await d.click(f("view3d:show-surfaces"));
    const painted = await colourPixels(d, "canvas.view3d-canvas", "#f28e2b", 50);
    assert.ok(painted > 0, "track sample dots are painted");
    await d.type(f("view3d:wave-height-max"), "0");
    await count(d, 0);
    await checkSliders(d); // Touching handles can still be grabbed on either side.
    assert.ok(await colourPixels(d, "canvas.view3d-canvas", "#f28e2b", 50) < painted, "the display redraw removes the sample dots");
    await d.click(f("view3d:wave-ranges-reset"));
    await count(d, 120);

    await d.type(f("view3d:wave-height-min"), "0.7");
    await count(d, 75); // 30 missing heights and 15 below 0.7 m.
    await d.type(f("view3d:wave-height-max"), "1.0");
    await count(d, 45);

    // Hit-test the selected rail, then deliver a held drag through React in
    // the real WebKit view. Synthetic pointers cannot acquire native capture.
    assert.ok(await d.run(`
      var middle = document.querySelector('[data-feature="view3d:wave-height-move"]');
      var r = middle.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2;
      if (document.elementFromPoint(x, y) !== middle) { done(false); return; }
      var capture = middle.setPointerCapture;
      middle.setPointerCapture = function () {};
      middle.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, pointerId: 1, button: 0, clientX: x, clientY: y }));
      middle.setPointerCapture = capture;
      window.__waveRangeDrag = { middle, x, y, pixels: middle.parentElement.getBoundingClientRect().width / 1.3 };
      done(true);
    `), "the middle section receives pointer input without covering the handles");
    assert.equal(await d.text(f("view3d:wave-height-min")), "0.7", "pressing the middle does not jump a bound");
    await d.run(`
      var p = window.__waveRangeDrag;
      p.middle.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 1, buttons: 1, clientX: p.x - 0.3 * p.pixels, clientY: p.y }));
      done(true);
    `);
    await count(d, 30); // 0.4–0.7 m admits fixture heights 0.6 and 0.7, 15 each.
    await d.waitFor(".track-list", { text: "30 of 120 samples used" }); // Blend updates before release.
    assert.ok(Math.abs(Number(await d.text(f("view3d:wave-height-min"))) - 0.4) < 1e-6);
    assert.ok(Math.abs(Number(await d.text(f("view3d:wave-height-max"))) - 0.7) < 1e-6);
    await checkSliders(d);
    await t.shot("dragged-wave-range");
    await d.run(`
      var p = window.__waveRangeDrag;
      p.middle.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, button: 0, clientX: p.x - 0.3 * p.pixels, clientY: p.y }));
      delete window.__waveRangeDrag;
      done(true);
    `);
    await d.type(f("view3d:wave-height-max"), "1.0");
    await d.type(f("view3d:wave-height-min"), "0.7");
    await count(d, 45);
    await d.type(f("view3d:wave-period-min"), "7");
    await d.type(f("view3d:wave-period-max"), "8");
    await count(d, 18);
    await d.type(f("view3d:wave-angle-min"), "30");
    await d.type(f("view3d:wave-angle-max"), "60");
    await count(d, 6); // Fixture fixes 11, 42, 77, 101, 106, 107.
    await d.waitFor(".track-list", { text: "6 of 120 samples used" });
    await d.run(`document.querySelector('.view3d-side').scrollTop = 0; done(true);`);
    await t.shot("three-wave-ranges");

    await d.click(f("tracks:filters"));
    await d.type(f("tracks:hs-min"), "0.8");
    await d.key("Enter", f("tracks:hs-min"));
    await count(d, 4); // Analysis and display ranges both apply.
    await d.click(f("view3d:show-filtered"));
    await count(d, 6); // Showing analysis-filtered dots still respects display ranges.
    await d.click(f("view3d:show-filtered"));
    await count(d, 4);
    await d.click(f("view3d:wave-ranges-reset"));
    await count(d, 60); // Analysis filters remain: 60 known heights >= 0.8; unknown heights fail that filter.
    assert.equal(await d.text(f("tracks:hs-min")), "0.8");
    await d.click(f("tracks:filters"));
    await d.run(`document.querySelector('.view3d-side').scrollTop = 0; done(true);`);
    await t.shot("reset-keeps-analysis-filter");
  },
};
