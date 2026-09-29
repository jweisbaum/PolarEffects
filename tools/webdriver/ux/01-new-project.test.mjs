/** Start screen → New project → the project window on its Map stage. */
import { assert, newProject } from "./harness.mjs";

export default {
  name: "new project",
  async run(t) {
    const d = t.driver;
    await d.waitFor('[data-feature="new:create"]');
    await t.shot("start-screen");
    await newProject(d, "Farr 40 study");
    assert.equal(await d.text('[data-feature="stage:map"][aria-selected="true"]'), "Map");
    assert.ok((await d.texts('[data-feature="shell:rename"]')).some((s) => s.includes("Farr 40 study")),
      "the title bar names the project");
    assert.ok(await d.exists('[data-feature="nav:polar-files"]'), "the left navigation is there");
    await t.shot("project-window-map");
  },
};
