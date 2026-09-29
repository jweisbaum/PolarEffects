/**
 * The MCP server's handshake: it starts over stdio, answers `initialize`,
 * and lists its tools, without starting the application (that happens on
 * the first tool call). Run with `npm run tools:test`.
 */
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const here = dirname(fileURLToPath(import.meta.url));

test("the pe-driver MCP server lists its tools", async () => {
  const client = new Client({ name: "mcp-test", version: "0" });
  const transport = new StdioClientTransport({ command: process.execPath, args: [join(here, "mcp.mjs")] });
  await client.connect(transport);
  try {
    assert.equal(client.getServerVersion()?.name, "pe-driver");
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name).sort();
    assert.deepEqual(
      names,
      ["click", "dialog", "evaluate", "key", "open", "screenshot", "stop", "text", "type", "wait"],
    );
    // `stop` with nothing running answers without starting the application.
    const stopped = await client.callTool({ name: "stop", arguments: {} });
    assert.equal(stopped.content[0].text, "nothing was running");
  } finally {
    await client.close();
  }
});
