/**
 * The MCP server's handshake: it starts over stdio, answers `initialize`,
 * and lists its tools, without starting the application (that happens on
 * the first tool call). Then the held application's lifecycle
 * (`holder.mjs`), whose races orphaned a detached app before. Run with
 * `npm run tools:test`.
 */
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

import { launch } from "./client.mjs";
import { AppHolder } from "./holder.mjs";

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

// ---- The held application's lifecycle (`holder.mjs`) ----------------------
//
// A fake launcher, so the races can be staged exactly: each launch waits for
// the test to let it finish, honours its abort signal the way `launch` does
// (stopping its tree, recorded as `cancelled`), and hands back a fake driver
// whose `ready()` and `close()` are recorded too.

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function fakes({ readyFails = false } = {}) {
  const log = { launches: 0, cancelled: 0, closed: 0, drivers: [] };
  const gates = [];
  const readyGates = [];
  const launcher = ({ signal }) => {
    log.launches += 1;
    const gate = deferred();
    gates.push(gate);
    signal.addEventListener("abort", () => {
      log.cancelled += 1;
      gate.reject(new Error("the launch was cancelled"));
    }, { once: true });
    return gate.promise.then(() => {
      const readyGate = deferred();
      readyGates.push(readyGate);
      const driver = {
        child: {},
        ready: ({ signal: s } = {}) => {
          if (readyFails) return Promise.reject(new Error("the application opened no window"));
          s?.addEventListener("abort", () => readyGate.reject(new Error("aborted")), { once: true });
          return readyGate.promise;
        },
        close: async () => {
          log.closed += 1;
        },
      };
      log.drivers.push(driver);
      return driver;
    });
  };
  return { log, gates, readyGates, launcher };
}

const tick = () => new Promise((r) => setImmediate(r));

test("stop during a launch cancels it and says so", async () => {
  const { log, launcher } = fakes();
  const holder = new AppHolder({ launch: launcher });
  const pending = holder.get();
  await tick();
  assert.equal(await holder.stop(), "stopped while starting");
  await assert.rejects(pending, /cancelled/);
  assert.equal(log.cancelled, 1, "the launch saw the cancel and stopped its tree");
  assert.equal(log.closed, 0, "nothing had been started to close");
  assert.equal(await holder.stop(), "nothing was running");
});

test("stop while the started app is not yet ready closes it", async () => {
  const { log, gates, launcher } = fakes();
  const holder = new AppHolder({ launch: launcher });
  const pending = holder.get();
  await tick();
  gates[0].resolve();
  await tick();
  assert.equal(await holder.stop(), "stopped while starting");
  await assert.rejects(pending);
  assert.equal(log.closed, 1, "the app the launch had started is closed, not orphaned");
  assert.equal(holder.driver, null);
});

test("a failed ready() closes what was started, and the next call starts afresh", async () => {
  const { log, gates, launcher } = fakes({ readyFails: true });
  const holder = new AppHolder({ launch: launcher });
  const first = holder.get();
  await tick();
  gates[0].resolve();
  await assert.rejects(first, /no window/);
  assert.equal(log.closed, 1);
  assert.equal(holder.driver, null);
  const second = holder.get();
  await tick();
  assert.equal(log.launches, 2);
  gates[1].resolve();
  await assert.rejects(second);
  assert.equal(log.closed, 2);
});

test("callers during a start share one launch, and stop afterwards stops it", async () => {
  const { log, gates, readyGates, launcher } = fakes();
  const holder = new AppHolder({ launch: launcher });
  const a = holder.get();
  const b = holder.get();
  await tick();
  assert.equal(log.launches, 1);
  gates[0].resolve();
  await tick();
  readyGates[0].resolve();
  assert.equal(await a, await b);
  assert.equal(await holder.stop(), "stopped");
  assert.equal(log.closed, 1);
});

test("the real launch refuses an already-cancelled start without spawning", async () => {
  await assert.rejects(launch({ signal: AbortSignal.abort() }), (error) => error.name === "AbortError");
});
