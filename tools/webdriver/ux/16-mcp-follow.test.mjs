/**
 * M21: the MCP service in the real application (spec.md 3.7, D29). Switched
 * on in Settings, an MCP client — the official SDK's, over Streamable HTTP
 * with the token Settings shows — imports a polar file and changes its
 * weight, and the interface follows: the source appears, the slider moves,
 * the status bar says who is driving. Then off means off: the port refuses.
 *
 * Offline: the client talks to 127.0.0.1 only, and nothing it asks for
 * downloads anything.
 */
import { readFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

import { assert, newProject } from "./harness.mjs";

const f = (id) => `[data-feature="${id}"]`;

/** A port nothing holds right now. */
function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/** A tool's structured answer; throws with the tool's own words when it refused. */
async function call(client, name, args = {}) {
  const result = await client.callTool({ name, arguments: args });
  if (result.isError) throw new Error(`${name}: ${result.content?.map((c) => c.text).join(" ")}`);
  return result.structuredContent ?? result;
}

export default {
  name: "an MCP client drives the application and the interface follows",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Driven");
    assert.equal(await d.exists(".mcp-badge"), false, "no badge while nothing is connected");

    // --- Settings: a free port, then on ---
    const port = await freePort();
    await d.click(f("shell:settings"));
    await d.waitFor('[data-section="settings:mcp"] h3', { text: "MCP service" });
    await d.run(`document.querySelector('[data-section="settings:mcp"]').scrollIntoView({ block: "start" }); done(true);`);
    assert.equal(await d.exists('[data-section="settings:mcp"] code'), false, "no address or token while off");
    assert.ok((await d.text(".mcp-chatgpt")).includes("ChatGPT"));
    await d.type(f("settings:mcp-port"), String(port));
    await d.key("Enter", f("settings:mcp-port"));
    // The port is saved by Rust before the switch is touched.
    const settingsFile = join(d.automationRoot, "config", "settings.json");
    for (let waited = 0; waited < 10_000; waited += 100) {
      const now = await readFile(settingsFile, "utf8").then(JSON.parse).catch(() => null);
      if (now?.mcp?.port === port) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    await d.click(f("settings:mcp-enable"));
    await d.waitFor('[data-section="settings:mcp"] code', { text: `http://127.0.0.1:${port}/mcp` });
    assert.equal(await d.text(".mcp-connected"), "No client connected");
    const buttons = await d.texts(f("settings:mcp-add"));
    assert.ok(buttons.includes("Add to Claude Code") && buttons.includes("Add to Codex"), buttons.join(", "));
    await t.shot("settings-mcp-on");

    // The token is in the run's own settings file, owner-only, and is the
    // one the section shows.
    const saved = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(saved.mcp.enabled, true);
    assert.equal(saved.mcp.port, port);
    assert.equal(saved.mcp.token.length, 43);
    const shown = await d.texts('[data-section="settings:mcp"] code');
    assert.ok(shown.includes(saved.mcp.token), "Settings shows the token a client needs");
    await d.click(f("settings:close"));
    await d.waitGone(".modal.settings");

    // --- The guards, from outside ---
    const url = `http://127.0.0.1:${port}/mcp`;
    const init = { method: "POST", headers: { "content-type": "application/json" }, body: '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' };
    assert.equal((await fetch(url, init)).status, 401, "no token, no service");
    assert.equal((await fetch(url, { ...init, headers: { ...init.headers, authorization: "Bearer nope" } })).status, 401);

    // --- A client connects and works ---
    const transport = new StreamableHTTPClientTransport(new URL(url), {
      requestInit: { headers: { Authorization: `Bearer ${saved.mcp.token}` } },
    });
    const client = new Client({ name: "ux-test", version: "0" });
    await client.connect(transport);
    try {
      assert.equal(client.getServerVersion()?.name, "PolarExplorer");
      assert.ok(client.getInstructions()?.startsWith("Today is "));
      const tools = (await client.listTools()).tools.map((tool) => tool.name);
      assert.ok(tools.length >= 50, `${tools.length} tools`);
      assert.ok(tools.includes("polarexplorer_guide") && tools.includes("blend_cell"));

      const status = await call(client, "project_status");
      assert.equal(status.project.name, "Driven", "the client sees the project the person made");
      await d.waitFor(".mcp-badge", { text: "MCP: project_status" });

      // An import through the tool appears in the interface, unasked.
      const imported = await call(client, "polar_files_import", { paths: [t.path("polar_examples/polars/Farr 40.txt")] });
      assert.equal(imported.failures.length, 0);
      const source = imported.project.sources[0].id;
      await d.waitFor(".polar-file-list li", { text: "Farr 40" });

      // A weight set by the client moves the person's slider.
      assert.equal(await d.text(".source-weight"), "1.00");
      await call(client, "source_set", { source, weight: 0.4 });
      await d.waitFor(".source-weight", { text: "0.40" });
      await d.waitFor(".mcp-badge", { text: "MCP: source_set" });
      assert.equal(await d.run(`done(document.querySelector('[data-feature="sources:weight"]').value);`), "0.4");

      // What stands behind a cell, read through the tool.
      const grid = await call(client, "polar_read", {});
      const twa = grid.twa.find((angle) => angle === 90) ?? grid.twa[Math.floor(grid.twa.length / 2)];
      const cell = await call(client, "blend_cell", { twa, tws: grid.tws[3] });
      assert.equal(cell.contributors.length, 1);
      assert.equal(cell.contributors[0].source_id, source);

      // The view goes where the client sends it, and a picture comes back.
      await call(client, "view_stage", { stage: "compare" });
      await d.waitFor('[data-feature="stage:compare"][aria-selected="true"]');
      await call(client, "view_stage", { stage: "3d" });
      await d.waitFor('[data-feature="stage:3d"][aria-selected="true"]');
      await d.waitFor("canvas.view3d-canvas", { visible: true });
      const picture = await client.callTool({ name: "screenshot", arguments: {} });
      assert.notEqual(picture.isError, true, JSON.stringify(picture.content).slice(0, 300));
      const image = picture.content.find((part) => part.type === "image");
      assert.equal(image?.mimeType, "image/png");
      assert.ok(Buffer.from(image.data, "base64").length > 2000, "a real picture of the stage");
      await t.shot("interface-followed-the-client");

      // The client's edit is the person's to undo, and the client's own.
      await call(client, "undo");
      await d.waitFor(".source-weight", { text: "1.00" });

      // A refusal is words a model can read, and changes nothing.
      const refused = await client.callTool({ name: "source_set", arguments: { source, weight: 1.5 } });
      assert.equal(refused.isError, true);
      assert.ok(refused.content[0].text.includes("weight"));
      assert.equal(await d.text(".source-weight"), "1.00");

      // A second boat has a stage of its own: asked for by boat, it is set
      // on that boat's tab and the first boat's stays where it was.
      const first = status.project.id;
      await call(client, "boat_add", { name: "Second" });
      const second = (await call(client, "boats_list")).tabs.find((tab) => tab.name === "Second").id;
      await d.waitFor(f("boats:tab"), { text: "Second" });
      await call(client, "view_stage", { stage: "compare", boat: second });
      await d.waitFor(`.boat-pane.active[data-boat-id="${second}"] [data-feature="stage:compare"][aria-selected="true"]`);
      await call(client, "view_boat", { boat: first });
      await d.waitFor(`.boat-pane.active[data-boat-id="${first}"] [data-feature="stage:3d"][aria-selected="true"]`);
      await call(client, "view_stage", { stage: "compare", boat: second });
      await d.waitFor(`.boat-pane.active[data-boat-id="${second}"]`);
      await t.shot("second-boat-on-compare");

      // The boat on show is removed by the client: its pane goes with it
      // and the first boat is shown, with nothing reported as failed.
      await call(client, "boat_remove", { boat: second });
      await d.waitGone(`[data-boat-id="${second}"]`);
      await d.waitFor(`.boat-pane.active[data-boat-id="${first}"]`);
      assert.deepEqual(await d.texts(f("boats:tab")), ["Driven"]);
      assert.equal(await d.exists(".statusbar .hint.error"), false, "a boat removed by a client is not a failure");
      await t.shot("second-boat-removed");
    } finally {
      // Ending the session is what tells the service the client has gone: a
      // client that only drops its connection stays counted until the
      // session's five-minute idle timeout (spec.md 3.7).
      await transport.terminateSession().catch(() => undefined);
      await client.close().catch(() => undefined);
    }
    await d.waitGone(".mcp-badge", { timeoutMs: 20_000 });

    // --- Off means off, at once, for a client that is still connected ---
    const lingering = new Client({ name: "ux-lingering", version: "0" });
    await lingering.connect(new StreamableHTTPClientTransport(new URL(url), {
      requestInit: { headers: { Authorization: `Bearer ${saved.mcp.token}` } },
    }));
    await call(lingering, "project_status");
    await d.waitFor(".mcp-badge", { text: "MCP: project_status" });
    await d.click(f("shell:settings"));
    await d.waitFor(f("settings:mcp-enable"));
    await d.click(f("settings:mcp-enable"));
    await d.waitGone('[data-section="settings:mcp"] code');
    await d.waitGone(".mcp-badge", { timeoutMs: 5_000 });
    await lingering.close().catch(() => undefined);
    const off = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(off.mcp.enabled, false);
    assert.equal(off.mcp.token, "", "the token is forgotten");
    await assert.rejects(fetch(url, init), "nothing listens once the service is off");
    await d.click(f("settings:close"));
  },
};
