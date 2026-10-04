import { assert, colourPixels } from "./harness.mjs";
const f = id => `[data-feature="${id}"]`;
const pane = i => `.boat-pane:not([hidden]):nth-of-type(${i + 1})`;
function sameView(a,b,message) { const aa=[...a.position,...a.target],bb=[...b.position,...b.target];assert.ok(aa.every((v,i)=>Math.abs(v-bb[i])<1e-8), `${message}: ${JSON.stringify([a,b])}`); }
async function cameras(d) { return d.run(`done(Array.from(document.querySelectorAll('.boat-pane:not([hidden]) canvas.view3d-canvas')).map(c => c.__peInspectPolar().view));`); }
function northUp(view) {
  assert.ok(Math.abs(view.position[0] - view.target[0]) < 1e-8, "0° is straight up, without azimuth rotation");
  assert.ok(view.position[2] > view.target[2], "initial camera looks down on the polar");
  assert.ok(view.position[1] < view.target[1], "0° faces the top of the screen");
}
async function populated(d, count) {
  await d.waitFor(`.boat-grid.boats-${count}`);
  for (let n = 0; n < 60; n++) {
    if (await d.run(`var c = Array.from(document.querySelectorAll('.boat-pane:not([hidden]) canvas.view3d-canvas')); done(c.length === arguments[0] && c.every(c => c.__peInspectPolar && c.__peInspectPolar().points.length > 0));`, [count])) return;
    await new Promise(r => setTimeout(r, 100));
  }
  throw new Error(`expected ${count} populated canvases`);
}
export default {
  name: "boat tabs and linked comparison",
  async run(t) {
    const d = t.driver;
    await d.open(t.path("tools/webdriver/fixtures/boats.wpsproj"));
    await d.waitFor('.boat-tabs [role="tab"]', {text:"Delta"});
    assert.equal(await d.count('.boat-tabs [role="tab"]'), 4);
    await populated(d, 1);
    northUp((await cameras(d))[0]);
    assert.equal(await d.count('.titlebar ' + f("boats:add")), 1);
    assert.equal(await d.text('.titlebar ' + f("boats:add")), "Add Polar");
    // The layout switch: three icons in the top bar, in Add Polar's row, each named for a pointer.
    assert.deepEqual(await d.run(`done(["single","split","four"].map(function (name) {
      var b = document.querySelector('.titlebar [data-feature="boats:' + name + '"]');
      return [b.title, b.textContent, !!b.querySelector("svg"), b.getAttribute("aria-pressed")]; }));`),
    [["Single view", "", true, "true"], ["Split view", "", true, "false"], ["Four-way view", "", true, "false"]]);
    assert.ok(await d.run(`var add = document.querySelector('[data-feature="boats:add"]').getBoundingClientRect(),
      one = document.querySelector('[data-feature="boats:single"]').getBoundingClientRect();
      done(one.left >= add.right && Math.abs((one.top + one.bottom) / 2 - (add.top + add.bottom) / 2) < 2);`), "the icons follow Add Polar on its row");
    // Each tab carries the × that closes it; there is no row of buttons above the tabs.
    assert.equal(await d.count('.boat-actions'), 0);
    assert.deepEqual(await d.run(`done(Array.from(document.querySelectorAll('.boat-tab')).map(function (tab) {
      var x = tab.querySelector('[data-feature="boats:delete"]'); return [tab.querySelector('[role="tab"]').textContent, x.textContent, x.title]; }));`),
    [["Alpha", "×", "Delete Alpha"], ["Bravo", "×", "Delete Bravo"], ["Charlie", "×", "Delete Charlie"], ["Delta", "×", "Delete Delta"]]);
    assert.equal(await d.count('.boat-pane:not([hidden]) .dock-toggle'), 2, "single view has both panel toggles");
    // Export all sits at the bottom right, just before the version.
    assert.ok(await d.run(`var bar = document.querySelector('.statusbar'), all = bar.querySelector('[data-feature="boats:export-all"]'),
      version = bar.lastElementChild; done(!!all && /^v[0-9]/.test(version.textContent)
        && all.getBoundingClientRect().right <= version.getBoundingClientRect().left
        && all.getBoundingClientRect().left > bar.getBoundingClientRect().width / 2);`), "Export all is beside the version");
    assert.equal(await d.count(f("boats:rename")), 0);
    assert.ok(await d.run(`var title=document.querySelector('.titlebar .project-name').getBoundingClientRect(), add=document.querySelector('[data-feature="boats:add"]').getBoundingClientRect(); done(add.left>=title.right && Math.abs(add.top-title.top)<12);`), "Add boat is beside the project name");
    await d.run(`document.querySelector('.boat-tabs [role="tab"]').dispatchEvent(new MouseEvent('dblclick',{bubbles:true})); done(true);`);
    await d.type(f("boats:name"), "Alpha renamed"); await d.key("Enter", f("boats:name"));
    await d.waitFor('.boat-tabs [role="tab"]', {text:"Alpha renamed"});
    await d.click(f("boats:split")); await populated(d, 2);
    assert.equal(await d.count('.boat-tabs-row'), 0, "split navigation uses the dropdowns only");
    assert.equal(await d.count('.dock-toggle'), 0, "split view has no panel toggles");
    assert.equal(await d.count('.boat-pane:not([hidden]) [data-feature="stage:map"]'), 0);
    await d.click(`${pane(0)} ${f("view3d:camera-top")}`);
    await new Promise(r => setTimeout(r, 150));
    let views = await cameras(d); sameView(views[0], views[1], "camera presets synchronize");
    // A real OrbitControls pointer gesture, not an application sync hook.
    await d.run(`var c = document.querySelector('.boat-pane:not([hidden]) canvas.view3d-canvas'), r = c.getBoundingClientRect();
      c.setPointerCapture = function() {}; c.releasePointerCapture = function() {};
      for (var e of [['pointerdown',0],['pointermove',50],['pointerup',50]]) c.dispatchEvent(new PointerEvent(e[0], { bubbles:true, pointerId:7, pointerType:'mouse', button:0, buttons:e[0] === 'pointerup' ? 0 : 1, clientX:r.left+r.width/2+e[1], clientY:r.top+r.height/2 })); done(true);`);
    await new Promise(r => setTimeout(r, 150));
    const rotated = await cameras(d); assert.notDeepEqual(rotated[0], views[0]); sameView(rotated[0], rotated[1], "orbit synchronizes");
    await t.shot("two-boats-linked");
    await d.click(f("boats:four")); await populated(d, 4);
    assert.equal(await d.count('.boat-tabs-row'), 0);
    await d.click(`${pane(0)} ${f("view3d:camera-iso")}`);
    await new Promise(r => setTimeout(r, 150));
    views = await cameras(d); for (const view of views) sameView(view, views[0], "four synchronized views");
    // Hover a projected visible point in the first pane.
    const hovered = await d.run(`var c = document.querySelector('.boat-pane:not([hidden]) canvas.view3d-canvas'), r = c.getBoundingClientRect(), p = c.__peInspectPolar().points;
      for (var i=0;i<p.length;i+=2) { var x=r.left+p[i],y=r.top+p[i+1]; if(document.elementFromPoint(x,y)!==c) continue;
        c.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,pointerType:'mouse',clientX:x,clientY:y})); done(true); return; } done(false);`);
    assert.ok(hovered, "a data point is reachable in four-way view");
    await d.waitFor('.view3d-tooltip');
    assert.equal(await d.count('.boat-pane:not([hidden]) .view3d-tooltip'),4,"hover reaches all four boats");
    await t.shot("four-boats-linked-hover");
    assert.equal(await d.count('.dock-toggle'), 0, "four-way view has no panel toggles");
    // Hover the blend's surface in one pane: every pane with a blend at that wind shows its own cell.
    await d.run(`var c = document.querySelector('.boat-pane:not([hidden]) canvas.view3d-canvas');
      c.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, pointerType: 'mouse', relatedTarget: document.body })); done(true);`);
    await d.waitGone('.view3d-tooltip');
    let onBlend = false;
    for (let attempt = 0; attempt < 12 && !await d.exists('.blend-cell-tooltip'); attempt++) {
      onBlend = await d.run(`
        var c = document.querySelector('.boat-pane:not([hidden]) canvas.view3d-canvas'); c.__peRedraw?.();
        var copy = document.createElement("canvas"); copy.width = c.width; copy.height = c.height;
        var ctx = copy.getContext("2d"); ctx.drawImage(c, 0, 0);
        var data = ctx.getImageData(0, 0, copy.width, copy.height).data, rect = c.getBoundingClientRect();
        var rgb = [0xe0, 0x45, 0x7b], seen = 0;
        for (var y = 0; y < copy.height; y += 3) for (var x = 0; x < copy.width; x += 3) {
          var i = 4 * (y * copy.width + x);
          if (!rgb.every(function (v, k) { return Math.abs(data[i + k] - v) < 6; })) continue;
          var cx = rect.left + x / copy.width * rect.width, cy = rect.top + y / copy.height * rect.height;
          if (document.elementFromPoint(cx, cy) !== c) continue;
          if (seen++ < arguments[0]) continue;
          c.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse", clientX: cx, clientY: cy, buttons: 0 }));
          done(true); return;
        }
        done(false);`, [attempt * 25]);
      await new Promise(r => setTimeout(r, 300));
    }
    assert.ok(onBlend, "the first pane draws a blend surface to hover");
    await d.waitFor(`${pane(0)} .blend-cell-tooltip`);
    await new Promise(r => setTimeout(r, 400));
    const cells = await d.texts('.boat-pane:not([hidden]) .blend-cell-tooltip');
    assert.ok(cells.length >= 2, `the blend tooltip reaches the linked panes: ${cells.length}`);
    const wind = cells.map(text => (text.match(/TWA\d+°TWS[\d.]+ kn/) ?? [text])[0]);
    assert.ok(wind.every(w => w === wind[0]), `every pane shows the same wind: ${wind.join(" | ")}`);
    await t.shot("four-boats-linked-blend-hover");
    // Leaving the surface clears them all.
    await d.run(`var c = document.querySelector('.boat-pane:not([hidden]) canvas.view3d-canvas');
      c.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, pointerType: 'mouse', relatedTarget: document.body })); done(true);`);
    await d.waitGone('.blend-cell-tooltip');
    await d.click(`${pane(0)} ${f("boats:comparison-controls")}`);
    assert.ok(await d.run(`done(document.querySelector('.boat-pane:not([hidden]) .wave-display-ranges').getBoundingClientRect().height > 0);`));
    await d.click(`${pane(0)} ${f("boats:comparison-controls")}`);
    await d.click(f("boats:single")); await populated(d, 1);
    await d.click(f("boats:add")); await d.waitFor('.boat-tabs [role="tab"]', {text:"Polar 5"});
    assert.equal(await d.count('.boat-tabs [role="tab"]'),5);
    assert.equal(await d.count('.boat-pane:not([hidden]) .track-list li'),0,"new boat is empty");
    northUp((await cameras(d))[0]);
    // The × on a tab asks before it closes the tab; Cancel keeps it.
    const close = (n) => `.boat-tab:nth-child(${n}) ${f("boats:delete")}`;
    await d.click(close(5));
    await d.waitFor('[role="dialog"]', { text: "Delete Polar 5" });
    assert.ok((await d.text('[role="dialog"]')).includes("Remove this polar and its sources from the project?"));
    await t.shot("close-tab-asks-first");
    await d.click('[role="dialog"] button:not(.danger)');
    await d.waitGone('[role="dialog"]');
    assert.equal(await d.count('.boat-tabs [role="tab"]'), 5, "cancelling keeps the tab");
    await d.click(close(5)); await d.click('[role="dialog"] button.danger');
    await d.waitGone('.boat-tab:nth-child(5)');
    assert.equal(await d.count('.boat-tabs [role="tab"]'),4);
    await d.click('.boat-tabs-row ' + f("boats:restore")); await d.waitFor('.boat-tabs [role="tab"]',{text:"Polar 5"});
    // Closing the first tab, which is not the one on show, keeps the title and the tab on show.
    const title = await d.text('.titlebar .project-name');
    await d.click('.boat-tabs [role="tab"]', {text:"Polar 5"});
    await d.waitFor('.boat-tabs [role="tab"][aria-selected="true"]', {text:"Polar 5"});
    await d.click(close(1)); await d.click('[role="dialog"] button.danger');
    await d.waitGone('.boat-tab:nth-child(5)');
    assert.equal((await d.texts('.boat-tabs [role="tab"]'))[0], "Bravo");
    assert.equal(await d.text('.boat-tabs [role="tab"][aria-selected="true"]'), "Polar 5", "the tab on show stays on show");
    assert.equal(await d.text('.titlebar .project-name'), title, "deleting the first boat retains the fleet title");
    // The comparison layouts compare what is left, and have nothing to close.
    await d.click('.boat-tabs [role="tab"]', {text:"Bravo"}); await populated(d, 1);
    await d.click(f("boats:split")); await populated(d, 2);
    assert.equal(await d.count(f("boats:delete")), 0);
    assert.equal(await d.run(`done(document.querySelector('.boat-pane-picker').options.length);`), 4, "the closed tab is not offered to compare");
    await d.click(f("boats:single")); await populated(d, 1);
    await d.click(f("boats:restore")); await d.waitFor('.boat-tab:nth-child(5)');
    await d.click('.boat-tabs [role="tab"]',{text:"Alpha renamed"}); await populated(d, 1);
    await t.shot("restored-deleted-boat");
    await d.click(f("boats:export-all")); await d.waitFor('.boat-export-dialog');
    await t.shot("export-all-dialog"); await d.click(f("boats:export-close"));
    // Existing boats survive switching away and back.
    await d.click('.boat-tabs [role="tab"]', {text:"Bravo"});
    await d.waitFor('.boat-pane:not([hidden]) .track-list li',{text:"Minute samples"});
    await populated(d, 1);
    // The opaque blend can obscure the samples at this camera angle. Check
    // its surface first, then hide surfaces to inspect the actual track dots.
    assert.ok(await colourPixels(d, '.boat-pane:not([hidden]) canvas.view3d-canvas', '#e13e7e', 40) > 100, "the restored boat renders its blend");
    await t.shot("independent-boat-tab");
    await d.click('.boat-pane:not([hidden]) ' + f("view3d:show-surfaces"));
    assert.ok(await colourPixels(d, '.boat-pane:not([hidden]) canvas.view3d-canvas', '#f28e2b', 35) > 0, "the restored boat renders its track points");
  }
};
