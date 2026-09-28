import { describe, expect, it } from "vitest";

import { THUMB_HEIGHT, thumbPaths } from "./orcThumb";

describe("thumbPaths", () => {
  it("puts 90° to the right of the pole and the fastest point on the edge", () => {
    const [path] = thumbPaths([{ tws: 12, twa: [0, 90, 180], bsp: [5, 10, 5] }]);
    // Radius 20 (half the height less the margin) for the 10 kn at 90°:
    // (2 + 20, 22). 0° is up, 180° down, half as far out.
    expect(path).toBe(`M2.0 12.0 L22.0 ${(THUMB_HEIGHT / 2).toFixed(1)} L2.0 32.0`);
  });

  it("draws every curve on one scale", () => {
    const [light, strong] = thumbPaths([
      { tws: 6, twa: [90], bsp: [5] },
      { tws: 20, twa: [90], bsp: [10] },
    ]);
    expect(light).toBe("M12.0 22.0");
    expect(strong).toBe("M22.0 22.0");
  });

  it("draws nothing for a curve with no speed", () => {
    expect(thumbPaths([{ tws: 6, twa: [], bsp: [] }])).toEqual([""]);
  });
});
