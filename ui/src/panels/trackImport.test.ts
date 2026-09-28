import { describe, expect, it } from "vitest";

import type { CsvMappingInput } from "../generated/CsvMappingInput";
import { assign, dateRange, describeImportLine, describeTrackFailure, fromLocalInput, roleOf, toLocalInput } from "./trackImport";

const mapping: CsvMappingInput = {
  time: 0, lat: 1, lon: 2, heading: null, speed: 3, boat: null,
  time_format: "auto", custom_format: "", speed_unit: "kn",
};

describe("the track import's words", () => {
  it("says where a file failed: line and column, or feature", () => {
    expect(describeTrackFailure({ file: "a.csv", line: 3, column: 12, feature: null, reason: "not-a-number", message: "" }))
      .toBe("a.csv, line 3, column 12: this is not a number");
    expect(describeTrackFailure({ file: "b.geojson", line: null, column: null, feature: 0, reason: "no-time", message: "" }))
      .toBe("b.geojson, feature 1: this position has no time");
    expect(describeTrackFailure({ file: "c.csv", line: null, column: null, feature: null, reason: "what", message: "" }))
      .toBe("c.csv: the file could not be imported");
  });

  it("summarises an import, sorted and merged fixes included", () => {
    expect(describeImportLine({
      source_id: 7, file: "a.csv", label: "Alpha", fixes: 3, out_of_order: 1, duplicates: 1,
      heading_given: 0, heading_derived: 3, speed_given: 2, speed_derived: 1,
    })).toBe("Alpha: 3 positions · 1 out of order, sorted · 1 duplicate times merged · heading 0 given, 3 derived · speed 2 given, 1 derived");
  });

  it("maps a column to one role at a time", () => {
    expect(roleOf(mapping, 3)).toBe("speed");
    const moved = assign(mapping, "heading", 3);
    expect(moved.heading).toBe(3);
    expect(moved.speed).toBeNull();
    expect(assign(mapping, "boat", null).boat).toBeNull();
  });

  it("reads and writes times in UTC", () => {
    expect(toLocalInput(1_753_531_200)).toBe("2025-07-26T12:00");
    expect(fromLocalInput("2025-07-26T12:00")).toBe(1_753_531_200);
    expect(fromLocalInput("")).toBeNull();
    expect(toLocalInput(null)).toBe("");
    expect(dateRange(1_753_531_200, 1_753_531_200 + 86_400)).toBe("2025-07-26 – 2025-07-27");
    expect(dateRange(null, null)).toBe("–");
  });
});
