/**
 * The driver's parsing, which is where a wrong answer is silent.
 *
 * Run with `npm run tools:test`. Node's own runner, so the repository gains no
 * dependency for four assertions.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import { portFromLine, safeName, visibleRect } from "./client.mjs";

test("reads the port from the line the plugin announces", () => {
  assert.equal(portFromLine("[webdriver] listening on port 49557"), 49557);
  // The Tauri CLI colours and indents its own output; the plugin's line is a
  // bare println, so it arrives clean.
  assert.equal(portFromLine("  [webdriver] listening on port 1  "), 1);
});

test("ignores every other line that mentions a port", () => {
  // The app's log is full of these, and matching one points the driver at
  // nothing — anchored on purpose.
  for (const line of [
    "Local:   http://localhost:5173/",
    "note: [webdriver] listening on port 123 (from an old run)",
    "[webdriver] listening on port",
    "listening on port 49557",
    "[webdriver] listening on port abc",
    "",
  ]) {
    assert.equal(portFromLine(line), null, line);
  }
});

test("a picture's name cannot leave the shots directory", () => {
  assert.equal(safeName("../../etc/passwd"), "etc-passwd");
  assert.equal(safeName("fr flash"), "fr-flash");
  assert.equal(safeName(""), "shot");
  assert.equal(safeName(undefined, "driver"), "driver");
  assert.equal(safeName("01-start_screen"), "01-start_screen");
});

test("a canvas is painted only where the window shows it", () => {
  const viewport = { width: 1440, height: 900 };
  const plot = { left: 1100, top: 700, width: 300, height: 260 };
  // Nothing clips it: the window alone cuts its bottom.
  assert.deepEqual(visibleRect(plot, [], viewport), { left: 1100, top: 700, width: 300, height: 200 });
  // A side panel scrolled so the plot runs under its bottom edge, above
  // the status bar: cut at the panel, not at the window (M17a).
  const panel = { left: 1100, top: 44, right: 1440, bottom: 812 };
  assert.deepEqual(visibleRect(plot, [panel], viewport), { left: 1100, top: 700, width: 300, height: 112 });
  // Scrolled out of the panel altogether: not painted.
  assert.equal(visibleRect({ ...plot, top: 850 }, [panel], viewport), null);
  // Nested clips: the tighter one wins on each side.
  assert.deepEqual(
    visibleRect(plot, [panel, { left: 1120, top: 0, right: 1300, bottom: 2000 }], viewport),
    { left: 1120, top: 700, width: 180, height: 112 },
  );
});
