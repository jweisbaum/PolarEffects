import { describe, expect, it } from "vitest";

import { interleave } from "./catalogueSearch";

describe("interleave (asked 2026-10-03)", () => {
  it("alternates the two catalogues by their own rank, the longer one going on alone", () => {
    expect(interleave(["c1", "c2", "c3", "c4"], ["r1", "r2"])).toEqual(["c1", "r1", "c2", "r2", "c3", "c4"]);
    expect(interleave([], ["r1", "r2"])).toEqual(["r1", "r2"]);
    expect(interleave(["c1"], [])).toEqual(["c1"]);
  });

  it("leaves the rows already shown where they were when either catalogue's next page arrives", () => {
    const first = interleave(["c1", "c2"], ["r1", "r2"]);
    const more = interleave(["c1", "c2", "c3", "c4"], ["r1", "r2", "r3"]);
    expect(more.slice(0, first.length)).toEqual(first);
  });
});
