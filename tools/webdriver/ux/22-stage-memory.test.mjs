/** Leaving the 3D or 2D stage for the map or Compare and coming back finds it as it was left (asked 2026-10-04). */
import { assert, newProject } from "./harness.mjs";

const f = id => `[data-feature="${id}"]`;
/** The 3D camera, as the view holds it. */
const camera = d => d.run(`var c = document.querySelector(".stage-slot:not([hidden]) .view3d canvas");
  done(c && c.__peInspectPolar ? c.__peInspectPolar().view : null);`);

export default {
  name: "stage memory",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Stage memory");
    await d.queueDialog([t.path("tools/webdriver/fixtures/outlier.csv")]);
    await d.click(f("tracks:import-file"));
    await d.waitFor(".track-import", { visible: true });
    await d.click(".track-import .modal-actions button.primary");
    await d.waitGone(".track-import");
    await d.waitFor(".track-list", { text: "outlier" });

    await d.click(f("view3d:camera-side"));
    await d.click(f("view3d:show-surfaces"));
    const before = await camera(d);
    assert.ok(before, "the 3D view reports its camera");
    await t.shot("3d-as-left");
    await d.click(f("stage:2d"));
    await d.waitFor(".polar-plot-stage canvas", { visible: true });
    await d.run(`var el = document.querySelector('[data-feature="plot:colour"]'); el.value = "timeOfDay";
      el.dispatchEvent(new Event("change", { bubbles: true })); done(true);`);

    for (const away of ["stage:map", "stage:compare"]) {
      await d.click(f(away));
      await new Promise(r => setTimeout(r, 400));
      await d.click(f("stage:3d"));
      const after = await camera(d);
      assert.deepEqual(after, before, `the camera is where it was after ${away}`);
      assert.equal(await d.run(`done(document.querySelector('.stage-slot:not([hidden]) [data-feature="view3d:show-surfaces"]').checked);`), false,
        `Surfaces stays unticked after ${away}`);
      await d.click(f("stage:2d"));
      assert.equal(await d.run(`done(document.querySelector('.stage-slot:not([hidden]) [data-feature="plot:colour"]').value);`), "timeOfDay",
        `the 2D plot keeps its colouring after ${away}`);
    }
    await d.click(f("stage:3d"));
    await new Promise(r => setTimeout(r, 300));
    await t.shot("3d-after-map-and-compare");
  },
};
