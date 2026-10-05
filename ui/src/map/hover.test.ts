import { describe, expect, it } from "vitest";

import type { SampleDetails } from "../generated/SampleDetails";
import { hoverLines, utc } from "./hover";

const sample: SampleDetails = {
  source_id: 1, sample_id: 2, t: 1_753_531_200, lat: 50, lon: -1, heading: 92.4, heading_origin: "derived",
  speed: 6.0, speed_origin: "given", bsp: 6.0, tws: null, twa: null, hs: null, current_speed: null,
  current_toward: null, filtered: false, excluded: false,
};

describe("the map's hover", () => {
  it("shows time, SOG and heading, and dashes for the environment not fetched yet", () => {
    const lines = hoverLines(sample, { speed: "kn", wave_height: "m", distance: "nm" });
    expect(lines.map((l) => [l.label, l.value])).toEqual([
      ["Time", "2025-07-26 12:00:00 UTC"],
      ["SOG", "6.0 kn (given)"],
      ["Heading", "92° (derived)"],
      ["TWS", "–"], ["TWA", "–"], ["Hs", "–"], ["Current", "–"],
    ]);
  });

  it("uses the display units once the environment is there", () => {
    const full = { ...sample, tws: 10, twa: 45, hs: 1.5, current_speed: 1, current_toward: 270 };
    const lines = hoverLines(full, { speed: "kmh", wave_height: "ft", distance: "km" });
    expect(lines.find((l) => l.label === "TWS")!.value).toBe("18.5 km/h");
    expect(lines.find((l) => l.label === "Hs")!.value).toBe("4.9 ft");
    expect(lines.find((l) => l.label === "Current")!.value).toBe("1.9 km/h toward 270°");
    expect(utc(0)).toBe("1970-01-01 00:00:00 UTC");
  });
});
