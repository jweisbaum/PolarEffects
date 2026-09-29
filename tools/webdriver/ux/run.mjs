/**
 * The agentic UX suite (D25): `npm run ux [-- <filter>…]`.
 *
 * Runs every `NN-name.test.mjs` beside this file (or those whose name
 * contains one of the filters) against the real development build — Vite,
 * React StrictMode and the Rust back end, started with the WebDriver feature
 * — one fresh application and one fresh data root per test. Each step's
 * picture lands in `target/ux-shots/<test>/`; a failing test also leaves a
 * `failure.png` there. Exits non-zero if any test failed.
 *
 * The development build on purpose: a8cf1dd was a dialog that hung only
 * there, under StrictMode's double effects, and a suite over the built
 * bundle would have passed.
 */

import { spawn } from "node:child_process";
import { readdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { launch, ROOT } from "../client.mjs";
import { context, shotsDir } from "./harness.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const filters = process.argv.slice(2);
const verbose = Boolean(process.env.PE_DRIVER_VERBOSE);

/**
 * The compilation `tauri dev` is about to do, done first and once: the
 * launch waits for a port line and cannot tell "still compiling" from
 * "hung". The same flags the CLI passes, so it warms the artifacts it uses.
 */
async function prebuild() {
  const cargo = spawn("cargo", ["build", "-p", "pe-app", "--features", "webdriver"], {
    cwd: ROOT,
    stdio: ["ignore", "inherit", "inherit"],
    env: { ...process.env, CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL ?? "0" },
  });
  const [code] = await new Promise((done) => cargo.once("exit", (...a) => done(a)));
  if (code !== 0) throw new Error(`the application did not build (cargo exited ${code})`);
}

const files = (await readdir(here))
  .filter((f) => /^\d+-.+\.test\.mjs$/.test(f))
  .filter((f) => filters.length === 0 || filters.some((x) => f.includes(x)))
  .sort();
if (files.length === 0) {
  console.error(`no UX test matches ${filters.join(", ")}`);
  process.exit(2);
}

console.log("building the application with the WebDriver feature…");
await prebuild();

const results = [];
for (const file of files) {
  const test = (await import(pathToFileURL(join(here, file)).href)).default;
  const name = file.replace(/\.test\.mjs$/, "");
  const started = Date.now();
  await rm(shotsDir(name), { recursive: true, force: true });
  let driver = null;
  let prepared = null;
  let t = null;
  try {
    prepared = (await test.setup?.()) ?? null;
    driver = await launch({
      env: { ...(test.env ?? {}), ...(prepared?.env ?? {}) },
      onLog: verbose ? (line) => process.stderr.write(`  | ${line}\n`) : undefined,
    });
    await driver.ready();
    t = context(name, driver);
    await test.run(t);
    results.push({ name, ok: true, ms: Date.now() - started, shots: t.shots });
    console.log(`ok   ${name} (${((Date.now() - started) / 1000).toFixed(1)} s, ${t.shots.length} shots)`);
  } catch (error) {
    let picture = null;
    if (driver) {
      picture = await driver.screenshot(join(shotsDir(name), "failure.png")).catch(() => null);
    }
    results.push({ name, ok: false, ms: Date.now() - started, error });
    console.log(`FAIL ${name}: ${error instanceof Error ? error.stack ?? error.message : String(error)}`);
    if (picture) console.log(`     the window when it failed: ${picture}`);
  } finally {
    await driver?.close();
    await prepared?.teardown?.();
  }
}

const failed = results.filter((r) => !r.ok);
console.log(
  `\n${results.length - failed.length} passed, ${failed.length} failed; pictures in ${join(ROOT, "target", "ux-shots")}`,
);
process.exit(failed.length === 0 ? 0 : 1);
