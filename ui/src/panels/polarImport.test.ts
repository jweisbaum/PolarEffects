/**
 * Failed polar files read in the interface language (spec.md 6,
 * invariant 7): every reason code Rust can send (`polar-reasons.json`,
 * checked against Rust by `pe-app/tests/polar_import.rs`) has its own line.
 */
import { afterEach, describe, expect, it } from "vitest";

import { setLanguage } from "../i18n";
import { describeAxes, describeFailure, reasonText } from "./polarImport";
import REASONS from "./polar-reasons.json";

afterEach(() => setLanguage("en"));

describe("polar import messages", () => {
  it("have a line of their own for every reason Rust sends", () => {
    const fallback = reasonText("zq-no-such-reason");
    for (const code of REASONS) expect(reasonText(code), code).not.toBe(fallback);
    expect(new Set(REASONS.map(reasonText)).size).toBe(REASONS.length);
  });

  it("name the file, line and column, in the interface language", () => {
    const failure = { file: "boat.csv", line: 2, column: 6, reason: "not-a-number", message: "boat.csv, line 2, column 6: \"x\" is not a number" };
    expect(describeFailure(failure)).toBe("boat.csv, line 2, column 6: this is not a number");
    setLanguage("fr");
    expect(describeFailure(failure)).toBe("boat.csv, ligne 2, colonne 6 : ce n’est pas un nombre");
    setLanguage("de");
    expect(describeFailure({ ...failure, line: null, column: null, reason: "unreadable" }))
      .toBe("boat.csv: die Datei konnte nicht gelesen werden");
  });

  it("describe a file's axes by range and count", () => {
    expect(describeAxes({ format: "adrena", file_name: "a.pol", twa: [0, 42.5, 180], tws: [6, 8, 20] }))
      .toBe("TWA 0–180° (3) · TWS 6–20 kn (3)");
    expect(describeAxes({ format: "csv", file_name: "a.csv", twa: [41.833], tws: [10] }))
      .toBe("TWA 41.83° (1) · TWS 10 kn (1)");
  });
});
