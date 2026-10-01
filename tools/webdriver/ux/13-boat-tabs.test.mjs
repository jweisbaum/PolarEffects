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
    assert.equal(await d.count(f("boats:rename")), 0);
    assert.ok(await d.run(`var title=document.querySelector('.titlebar .project-name').getBoundingClientRect(), add=document.querySelector('[data-feature="boats:add"]').getBoundingClientRect(); done(add.left>=title.right && Math.abs(add.top-title.top)<12);`), "Add boat is beside the project name");
    await d.run(`document.querySelector('.boat-tabs [role="tab"]').dispatchEvent(new MouseEvent('dblclick',{bubbles:true})); done(true);`);
    await d.type(f("boats:name"), "Alpha renamed"); await d.key("Enter", f("boats:name"));
    await d.waitFor('.boat-tabs [role="tab"]', {text:"Alpha renamed"});
    await d.click(f("boats:split")); await populated(d, 2);
    assert.equal(await d.count('.boat-tabs-row'), 0, "split navigation uses the dropdowns only");
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
    await d.click(`${pane(0)} ${f("boats:comparison-controls")}`);
    assert.ok(await d.run(`done(document.querySelector('.boat-pane:not([hidden]) .wave-display-ranges').getBoundingClientRect().height > 0);`));
    await d.click(`${pane(0)} ${f("boats:comparison-controls")}`);
    await d.click(f("boats:single")); await populated(d, 1);
    await d.click(f("boats:add")); await d.waitFor('.boat-tabs [role="tab"]', {text:"Boat 5"});
    assert.equal(await d.count('.boat-tabs [role="tab"]'),5);
    assert.equal(await d.count('.boat-pane:not([hidden]) .track-list li'),0,"new boat is empty");
    northUp((await cameras(d))[0]);
    await d.click(f("boats:delete")); await d.click('[role="dialog"] button.danger');
    await d.waitGone('.boat-tabs [role="tab"]:nth-child(5)');
    assert.equal(await d.count('.boat-tabs [role="tab"]'),4);
    await d.click(f("boats:restore")); await d.waitFor('.boat-tabs [role="tab"]',{text:"Boat 5"});
    const title = await d.text('.titlebar .project-name');
    await d.click('.boat-tabs [role="tab"]',{text:"Alpha renamed"});
    await d.click(f("boats:split")); await populated(d, 2);
    await d.click(f("boats:delete")); await d.click('[role="dialog"] button.danger');
    await d.waitGone('.boat-pane-picker option:nth-child(5)');
    await populated(d, 2);
    assert.equal(await d.count('.boat-tabs-row'),0,"deleting the first boat keeps comparison mode");
    assert.equal(await d.text('.titlebar .project-name'), title, "deleting the first boat retains the fleet title");
    await d.click(f("boats:restore")); await d.waitFor('.boat-pane-picker option:nth-child(5)');
    await populated(d, 2);
    await d.click(f("boats:single")); await d.waitFor('.boat-tabs [role="tab"]:nth-child(5)');
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
