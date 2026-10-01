/**
 * An MCP server over the application's automation endpoint (D25). Copied
 * from VectorEffects' `tools/webdriver/mcp.mjs` and extended.
 *
 * Exposes the real running PolarExplorer — the development build with the
 * WebDriver feature, its own Vite port and its own data root — as tools:
 * click, type, read, wait, answer a file dialog, run a script, open a
 * project, and take a picture of the whole window with its canvases (see
 * `client.mjs` for why that does not come from the endpoint's own
 * `/screenshot`).
 *
 * The application is started on the first tool call and held until `stop`
 * or until the server stops, because the endpoint's port is announced on
 * stdout and nowhere else: whoever wants to talk to it has to own the
 * process. With `PE_DRIVER_PORT` set, it talks to that application instead.
 *
 * Registered as `pe-driver` in `.mcp.json`. MCP servers load when the client
 * starts, so a session that adds or changes this cannot use it until it
 * restarts; `cli.mjs` is the same client from a shell meanwhile.
 */

import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

import { ROOT, safeName } from "./client.mjs";
import { AppHolder } from "./holder.mjs";

// The held application and any start in flight (`holder.mjs`): a start is
// never lost track of, so `stop` and the client going away both stop it.
const holder = new AppHolder({ port: process.env.PE_DRIVER_PORT || null });
const app = () => holder.get();

const server = new McpServer({ name: "pe-driver", version: "0.1.0" });

const text = (value) => ({
  content: [{ type: "text", text: typeof value === "string" ? value : JSON.stringify(value, null, 2) }],
});


server.registerTool(
  "screenshot",
  {
    title: "Screenshot the application",
    description:
      "A PNG of the whole PolarExplorer window, the map, polar plot and 3D canvases included, " +
      "saved under target/ux-shots/mcp/. Starts the app (dev build, isolated data) if it is not running.",
    inputSchema: { name: z.string().optional().describe("basename for the file") },
  },
  async ({ name }) => {
    const path = await (await app()).screenshot(
      join(ROOT, "target", "ux-shots", "mcp", `${safeName(name, "driver")}.png`),
    );
    const png = await readFile(path);
    return {
      content: [
        { type: "image", data: png.toString("base64"), mimeType: "image/png" },
        { type: "text", text: path },
      ],
    };
  },
);

server.registerTool(
  "open",
  {
    title: "Open a project",
    description:
      "Open a .wpsproj through the application's own opening (unsaved changes are discarded). " +
      "A cold start is the start screen; this or clicking Create project gets to the project window.",
    inputSchema: { path: z.string().describe("absolute path, or relative to the repository") },
  },
  async ({ path }) => text(await (await app()).open(resolve(ROOT, path))),
);

server.registerTool(
  "evaluate",
  {
    title: "Run a script in the webview",
    description:
      "Run JavaScript in the running application. The script must hand its answer to done(value) " +
      "(done is bound for you); never call window.__WEBDRIVER__.resolve yourself — a wrong id " +
      "bricks the endpoint. Keep it under 30 s: start long work and poll for it.",
    inputSchema: { script: z.string() },
  },
  async ({ script }) => text(await (await app()).run(script)),
);

server.registerTool(
  "click",
  {
    title: "Click an element",
    description:
      'Click the first element matching a CSS selector (optionally containing some text). Controls carry data-feature ids: [data-feature="polar-files:import"].',
    inputSchema: { selector: z.string(), text: z.string().optional() },
  },
  async ({ selector, text: containing }) => {
    await (await app()).click(selector, { text: containing });
    return text("clicked");
  },
);

server.registerTool(
  "type",
  {
    title: "Type into a field",
    description:
      "Set an input's, textarea's or select's value the way React sees it (native setter, then input and change).",
    inputSchema: { selector: z.string(), value: z.string() },
  },
  async ({ selector, value }) => {
    await (await app()).type(selector, value);
    return text("typed");
  },
);

server.registerTool(
  "key",
  {
    title: "Press a key",
    description: "Dispatch keydown and keyup for one key (Enter, Escape, ArrowDown…) on an element or the focused one.",
    inputSchema: { key: z.string(), selector: z.string().optional() },
  },
  async ({ key, selector }) => {
    await (await app()).key(key, selector ?? null);
    return text("sent");
  },
);

server.registerTool(
  "text",
  {
    title: "Read an element's text",
    description: "The text of the first element matching a CSS selector, or a field's value; null when absent.",
    inputSchema: { selector: z.string() },
  },
  async ({ selector }) => text(await (await app()).text(selector)),
);

server.registerTool(
  "wait",
  {
    title: "Wait for an element",
    description: "Wait until an element matching a CSS selector (optionally containing some text) exists.",
    inputSchema: {
      selector: z.string(),
      text: z.string().optional(),
      timeout_ms: z.number().int().positive().optional(),
    },
  },
  async ({ selector, text: containing, timeout_ms }) => {
    await (await app()).waitFor(selector, { text: containing, timeoutMs: timeout_ms ?? 30_000 });
    return text("present");
  },
);

server.registerTool(
  "dialog",
  {
    title: "Answer the next file dialog",
    description:
      "Queue the answer the next native file dialog gives (Open, Import…, Save As), since a native " +
      "picker is out of the webview's reach. Then click the control that opens it. An empty list cancels.",
    inputSchema: { paths: z.array(z.string()).describe("absolute paths, or relative to the repository") },
  },
  async ({ paths }) => {
    const absolute = paths.map((p) => resolve(ROOT, p));
    const answer = absolute.length === 0 ? null : absolute.length === 1 ? absolute[0] : absolute;
    return text(await (await app()).queueDialog(answer));
  },
);

server.registerTool(
  "stop",
  {
    title: "Stop the application",
    description:
      "Close the application this server started, and its data root — or cancel a start still in progress.",
  },
  async () => text(await holder.stop()),
);

let closing = null;
const shutdown = () => {
  closing ??= holder.stop().finally(() => process.exit(0));
  return closing;
};
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
// The client closing stdin is the ordinary end of a session, cold start or not.
process.stdin.on("close", shutdown);

await server.connect(new StdioServerTransport());
