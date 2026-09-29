import { describe, expect, it } from "vitest";

import { diverging, hex, legendGradient, oklab, POLES, scaleHalfWidth, type Scheme } from "./diverging";

const distance = (x: string, y: string) => {
  const [a, b] = [oklab(x), oklab(y)];
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) * 100;
};

describe("the diverging scale", () => {
  for (const scheme of ["dark", "light"] as Scheme[]) {
    it(`runs from B's pole through a grey zero to A's pole (${scheme})`, () => {
      const p = POLES[scheme];
      expect(hex(diverging(0, scheme))).toBe(p.mid);
      expect(hex(diverging(1, scheme))).toBe(p.a);
      expect(hex(diverging(-1, scheme))).toBe(p.b);
      expect(hex(diverging(5, scheme))).toBe(p.a);
      expect(hex(diverging(Number.NaN, scheme))).toBe(p.mid);
      // No hue at zero: the midpoint's chroma is near nil.
      const [, a, b] = oklab(p.mid);
      expect(Math.hypot(a, b)).toBeLessThan(0.01);
    });

    it(`moves lightness steadily away from zero on both arms, equally (${scheme})`, () => {
      const mid = oklab(POLES[scheme].mid)[0];
      let lastA = 0, lastB = 0;
      for (let k = 1; k <= 10; k++) {
        const t = k / 10;
        const la = Math.abs(oklab(hex(diverging(t, scheme)))[0] - mid);
        const lb = Math.abs(oklab(hex(diverging(-t, scheme)))[0] - mid);
        expect(la).toBeGreaterThan(lastA);
        expect(lb).toBeGreaterThan(lastB);
        // The poles are within 0.03 of each other in lightness.
        expect(Math.abs(la - lb)).toBeLessThan(0.035);
        [lastA, lastB] = [la, lb];
      }
    });

    it(`keeps its poles apart and away from the flash orange (${scheme})`, () => {
      const p = POLES[scheme];
      expect(distance(p.a, p.b)).toBeGreaterThan(20);
      expect(distance(p.a, "#ff8a1f")).toBeGreaterThan(10);
      expect(distance(p.single, p.mid)).toBeGreaterThan(8);
    });
  }

  it("is symmetric about zero and never zero wide", () => {
    expect(scaleHalfWidth(-0.4, 1.2)).toBe(1.2);
    expect(scaleHalfWidth(-2, 1)).toBe(2);
    expect(scaleHalfWidth(0, 0)).toBe(1);
    expect(scaleHalfWidth(Number.NaN, Number.NaN)).toBe(1);
  });

  it("draws the legend from B faster to A faster", () => {
    const g = legendGradient("dark", 3);
    expect(g).toBe(`linear-gradient(to right, ${POLES.dark.b} 0.0%, ${POLES.dark.mid} 50.0%, ${POLES.dark.a} 100.0%)`);
  });
});
