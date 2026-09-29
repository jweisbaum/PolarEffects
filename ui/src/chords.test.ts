import { expect, it } from "vitest";

import { chordText } from "./chords";

it("writes a chord's modifiers as each language and platform prints them", () => {
  expect(chordText(["accel", "shift", "S"], "en", true)).toBe("Cmd+Shift+S");
  expect(chordText(["accel", "shift", "S"], "en", false)).toBe("Ctrl+Shift+S");
  expect(chordText(["accel", "shift", "S"], "fr", true)).toBe("⌘+Maj+S");
  expect(chordText(["accel", "shift", "S"], "fr", false)).toBe("Ctrl+Maj+S");
  expect(chordText(["accel", "shift", "S"], "de", true)).toBe("⌘+Umschalt+S");
  expect(chordText(["accel", "F"], "de", false)).toBe("Strg+F");
  // An unknown language reads as English.
  expect(chordText(["accel", ","], "tlh", false)).toBe("Ctrl+,");
});
