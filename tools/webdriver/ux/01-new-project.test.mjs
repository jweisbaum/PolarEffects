/** Start screen → 3D; Map is available only while imported tracks exist. */
import { assert, newProject } from "./harness.mjs";

// Real layout checks: jsdom cannot measure a canvas or catch viewport changes.
async function verifyPanelOverlays(d, canvasSelector, controls) {
  const measure = () => d.run(`
    requestAnimationFrame(() => requestAnimationFrame(() => {
      var rect = el => { var r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; };
      var canvas = document.querySelector(arguments[0]);
      done({ stage: rect(document.querySelector(".centre-stage")),
        workspace: rect(document.querySelector(".workspace")),
        canvas: rect(canvas), buffer: [canvas.width, canvas.height] });
    }));`, [canvasSelector]);
  const initial = await measure();
  assert.deepEqual(initial.stage, initial.workspace, "the drawing stage fills the workspace behind both panels");
  for (const side of ["left", "right", "left", "right"]) {
    await d.click(`[data-feature="dock:${side}"]`);
    assert.deepEqual(await measure(), initial, `${canvasSelector}: toggling ${side} leaves the stage and drawing buffer unchanged`);
  }
  for (const selector of [".sidebar.left", ".sidebar.right", ...controls]) {
    assert.ok(await d.run(`
      var el = document.querySelector(arguments[0]), r = el.getBoundingClientRect();
      done(el.contains(document.elementFromPoint(r.left + r.width / 2, r.top + Math.min(20, r.height / 2))));
    `, [selector]), `${selector} receives pointer input above the drawing`);
  }
}

export default {
  name: "new project",
  async run(t) {
    const d = t.driver;
    await d.waitFor('[data-feature="new:create"]');
    await t.shot("start-screen");
    // The name only, here and in the New Project dialog (asked 2026-10-04).
    const fields = (scope) => d.run(`done(document.querySelectorAll(arguments[0] + " .new-project-form input, " + arguments[0] + " .new-project-form textarea").length);`, [scope]);
    assert.equal(await fields(".start"), 1, "the start screen asks for the project name only");
    await newProject(d, "Farr 40 study");
    assert.equal(await d.text('[data-feature="stage:3d"][aria-selected="true"]'), "3D");
    assert.equal(await d.exists('[data-feature="stage:map"]'), false);
    assert.ok((await d.texts('[data-feature="shell:rename"]')).some((s) => s.includes("Farr 40 study")),
      "the title bar names the project");
    assert.ok(await d.exists('[data-feature="nav:polar-files"]'), "the left navigation is there");
    await t.shot("project-window-3d");
    const order = await d.run(`
      var box = id => document.querySelector('[data-feature="' + id + '"]').getBoundingClientRect();
      var search = box("shell:search"), project = box("shell:project-menu"), settings = box("shell:settings");
      done(search.right < project.left && project.right < settings.left);`);
    assert.ok(order, "Project sits between Search and Settings");
    await d.click('[data-feature="shell:project-menu"]');
    assert.ok(await d.run(`var r = document.querySelector(".project-menu-items").getBoundingClientRect();
      done(r.left >= 0 && r.right <= window.innerWidth);`), "the menu stays within the window");
    await t.shot("project-menu");
    await d.click('[data-feature="project:new"]');
    await d.waitFor(".modal-backdrop");
    // A new project starts dirty, so New asks about its changes first.
    if (!(await d.exists(".modal .new-project-form"))) {
      await d.click(".modal button.danger");
      await d.waitFor(".modal .new-project-form");
    }
    assert.equal(await fields(".modal"), 1, "the New Project dialog asks for the project name only");
    await t.shot("new-project-dialog");
    await d.key("Escape");
    await d.waitGone(".modal-backdrop");
    assert.ok((await d.texts('[data-feature="shell:rename"]')).some((s) => s.includes("Farr 40 study")),
      "cancelling the dialog keeps the project open");

    for (const feature of ["tracks:yellowbrick", "sources:blend-settings"]) {
      await d.click(`[data-feature="${feature}"]`);
      await d.waitFor(".modal-backdrop");
      assert.ok(await d.run(`done(Boolean(document.elementFromPoint(20, 20).closest(".modal-backdrop")));`), "panel dialogs cover the title bar as well as the drawing");
      await d.key("Escape");
      await d.waitGone(".modal-backdrop");
    }
    // The 2D plot is a stage of its own (asked 2026-10-02), second in the switch.
    assert.deepEqual(await d.run(`done(Array.from(document.querySelectorAll('.stage-switcher [role="tab"]')).map(function (b) { return b.textContent; }));`),
      ["3D", "2D", "Compare"], "Map comes last once there are tracks");
    await d.click('[data-feature="stage:2d"]');
    // An empty project has no plot to draw yet: the stage says so.
    await d.waitFor(".polar-plot-stage .plot-placeholder", { visible: true });
    assert.equal(await d.exists(".polar-plot-overlay"), false);
    assert.equal(await d.exists('[data-feature="plot:full-size"]'), false);
    assert.equal(await d.exists('[data-feature="panel:plot"]'), false, "the right panel no longer holds the plot");
    await t.shot("plot-stage");
    assert.equal(await d.exists("canvas.map-canvas"), false);
    await d.click('[data-feature="stage:3d"]');
    await d.waitGone(".polar-plot-stage");

    await d.open(t.path("tools/webdriver/fixtures/analysis.wpsproj"));
    await d.waitFor('[data-feature="stage:map"]');
    assert.deepEqual(await d.texts('.stage-switcher [role="tab"]'), ["3D", "2D", "Compare", "Map"]);
    assert.ok(await d.exists('[data-feature="stage:3d"][aria-selected="true"]'));
    await verifyPanelOverlays(d, "canvas.view3d-canvas", [".view3d-toolbar", ".view3d-side"]);
    await t.shot("3d-overlay-panels");
    await d.click('[data-feature="stage:map"]');
    await d.waitFor("canvas.map-canvas");
    await verifyPanelOverlays(d, "canvas.map-canvas", [".map-controls"]);
    await t.shot("map-with-tracks");
    await d.click('[data-feature="stage:compare"]');
    await d.waitFor("canvas.compare-canvas");
    await verifyPanelOverlays(d, "canvas.compare-canvas", [".compare-toolbar", ".compare-side"]);
    await t.shot("compare-overlay-panels");
    await d.click('[data-feature="stage:map"]');
    await d.waitFor("canvas.map-canvas");
    await d.click('[data-feature="tracks:remove"]');
    await d.waitGone('[data-feature="stage:map"]');
    await d.waitFor('[data-feature="stage:3d"][aria-selected="true"]');
    assert.equal(await d.exists("canvas.map-canvas"), false);
    await t.shot("last-track-removed");
    await d.run(`window.dispatchEvent(new KeyboardEvent("keydown", { key: "z",
      metaKey: /Mac/.test(navigator.platform), ctrlKey: !/Mac/.test(navigator.platform), bubbles: true })); done(true);`);
    await d.waitFor('[data-feature="stage:map"]');
    assert.ok(await d.exists('[data-feature="stage:3d"][aria-selected="true"]'), "undo restores Map without reopening it");
  },
};
