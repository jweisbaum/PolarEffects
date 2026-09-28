import { describe, expect, it } from "vitest";

import { formatAxis, parseAxis } from "./axes";

describe("output grid axes", () => {
  it("reads values separated by spaces, commas or semicolons", () => {
    expect(parseAxis("0, 30 35;40\n42.5", "twa")).toEqual({ values: [0, 30, 35, 40, 42.5] });
    expect(formatAxis([4, 6, 12.5])).toBe("4, 6, 12.5");
    expect(parseAxis(formatAxis([4, 6, 12.5]), "tws")).toEqual({ values: [4, 6, 12.5] });
  });

  it("refuses what a polar file could not hold, naming the values", () => {
    const key = (text: string, kind: "twa" | "tws" = "twa") => parseAxis(text, kind).error?.key;
    expect(key("")).toBe("Enter at least one value.");
    expect(key("40 abc")).toBe("“{value}” is not a number.");
    expect(key("-5")).toBe("“{value}” is not a number.");
    expect(key("181")).toBe("{value} is outside 0 to {max}.");
    expect(key("71", "tws")).toBe("{value} is outside 0 to {max}.");
    expect(key("45 40")).toBe("{first} and {second} are not in increasing order.");
    expect(key("40 40")).toBe("{first} and {second} are not in increasing order.");
    // The M4 carry: two values closer than 0.01 would be written as one.
    const close = parseAxis("40, 40.004", "twa").error;
    expect(close?.key).toBe("{first} and {second} are closer than 0.01: a polar file could not tell them apart.");
    expect(close?.params).toEqual({ first: 40, second: 40.004 });
    expect(key("40.125")).toBe("{value} has more than two decimals.");
    expect(key(Array.from({ length: 513 }, (_, k) => String(k / 100)).join(" "))).toBe("At most {max} values.");
  });

  it("accepts values 0.01 apart", () => {
    expect(parseAxis("42, 42.01, 42.02", "twa")).toEqual({ values: [42, 42.01, 42.02] });
  });
});
