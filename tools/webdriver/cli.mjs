/**
 * The automation endpoint from a shell (D25). Copied from VectorEffects and
 * extended with the element helpers.
 *
 * The MCP server (`mcp.mjs`) is the same client with a protocol around it;
 * this is for a person, and for an agent that has a shell but no MCP
 * connection — a server registered in `.mcp.json` is loaded when the client
 * starts, so it is not available in the session that added it.
 *
 *   node tools/webdriver/cli.mjs shot [name]           start the app, capture, stop; prints the path
 *   node tools/webdriver/cli.mjs eval '<js>'           run a script (hand the answer to done(…)), print it
 *   node tools/webdriver/cli.mjs click <selector>      click an element
 *   node tools/webdriver/cli.mjs type <selector> <text>  set an input's value as React sees it
 *   node tools/webdriver/cli.mjs text <selector>       print an element's text (or a field's value)
 *   node tools/webdriver/cli.mjs wait <selector> [text]  wait until it appears
 *   node tools/webdriver/cli.mjs dialog <path>…        answer the next native file dialog with these paths
 *   node tools/webdriver/cli.mjs serve                 start and hold, printing the port
 *
 * Without `PE_DRIVER_PORT` each command starts its own application (a fresh
 * data root, stopped afterwards), which is only useful for `shot` and
 * `eval`. With it — the port `serve` printed — every command talks to that
 * held application, so a flow can be driven one step at a time.
 * `PE_DRIVER_OPEN=<project.wpsproj>` opens a project before `shot`.
 * Pictures go to `target/ux-shots/cli/<name>.png`.
 */

import { join } from "node:path";

import { connect, launch, ROOT, safeName } from "./client.mjs";

const [command, ...rest] = process.argv.slice(2);
const held = process.env.PE_DRIVER_PORT;
const quiet = (line) => {
  if (process.env.PE_DRIVER_VERBOSE) process.stderr.write(`${line}\n`);
};

async function withDriver(run) {
  if (held) return run(connect(held));
  const driver = await launch({ onLog: quiet });
  try {
    await driver.ready();
    return await run(driver);
  } finally {
    await driver.close();
  }
}

const need = (value, what) => {
  if (!value) throw new Error(`${command} needs ${what}`);
  return value;
};

const print = (value) => process.stdout.write(`${typeof value === "string" ? value : JSON.stringify(value, null, 2)}\n`);

try {
  switch (command) {
    case "shot": {
      const path = await withDriver(async (driver) => {
        await driver.ready();
        if (process.env.PE_DRIVER_OPEN) await driver.open(process.env.PE_DRIVER_OPEN);
        return driver.screenshot(join(ROOT, "target", "ux-shots", "cli", `${safeName(rest[0], "driver")}.png`));
      });
      print(path);
      break;
    }
    case "eval":
      print(await withDriver((driver) => driver.run(need(rest[0], "a script"))));
      break;
    case "click":
      await withDriver((driver) => driver.click(need(rest[0], "a selector"), { text: rest[1] }));
      print("clicked");
      break;
    case "type":
      await withDriver((driver) => driver.type(need(rest[0], "a selector"), rest[1] ?? ""));
      print("typed");
      break;
    case "text":
      print(await withDriver((driver) => driver.text(need(rest[0], "a selector"))));
      break;
    case "wait":
      await withDriver((driver) => driver.waitFor(need(rest[0], "a selector"), { text: rest[1] }));
      print("present");
      break;
    case "dialog":
      print(await withDriver((driver) => driver.queueDialog(rest.length > 1 ? rest : (rest[0] ?? null))));
      break;
    case "serve": {
      const driver = await launch({ onLog: (line) => process.stderr.write(`${line}\n`) });
      await driver.ready();
      print(String(driver.port));
      process.stderr.write(`held; data root ${driver.automationRoot}. Ctrl-C stops it.\n`);
      const stop = async () => {
        await driver.close();
        process.exit(0);
      };
      process.on("SIGINT", stop);
      process.on("SIGTERM", stop);
      await new Promise(() => {});
      break;
    }
    default:
      process.stderr.write(
        "usage: cli.mjs shot [name] | eval <js> | click <sel> [text] | type <sel> <text> | " +
          "text <sel> | wait <sel> [text] | dialog <path>… | serve\n",
      );
      process.exit(2);
  }
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exit(1);
}
