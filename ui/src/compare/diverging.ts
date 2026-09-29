/**
 * The Compare stage's diverging scale (spec.md 11): ΔBSP = A − B, centred on
 * zero, orange where A is faster and blue where B is faster.
 *
 * Blue and orange, not red and green, so the two sides stay apart for the
 * common colour-vision deficiencies (protan, deutan and tritan simulated
 * ΔE ≥ 20 between the poles, checked with the dataviz validator), and
 * neither is the feature search's flash orange (`#ff8a1f`): the poles are
 * duller and, on dark themes, lighter or, on light themes, darker.
 *
 * Perceptually even: each arm is a straight line in OKLab from a neutral
 * grey midpoint (no hue at zero) to its pole, the two poles at the same
 * lightness, so equal |Δ| on either side reads as equally strong. The
 * midpoint and poles are chosen per scheme so the scale is readable on the
 * theme's background: on a dark theme the arms grow lighter away from zero,
 * on a light one darker.
 */

export type Scheme = "dark" | "light";

interface Poles {
  /** Δ = 0: a grey with no hue. */
  mid: string;
  /** A faster (Δ > 0). */
  a: string;
  /** B faster (Δ < 0). */
  b: string;
  /** A cell only one operand covers: neutral grey, drawn with a hatch. */
  single: string;
  /** The hatch drawn over it. */
  hatch: string;
}

export const POLES: Record<Scheme, Poles> = {
  // Poles at OKLCH L 0.66 and 0.65 on the dark themes' backgrounds.
  dark: { mid: "#4d4f53", a: "#c2803f", b: "#5a92d2", single: "#7c7e82", hatch: "#c8cacd" },
  // Poles at OKLCH L 0.55 and 0.52; 3:1 or more on Paper's white.
  light: { mid: "#e3e1dc", a: "#a9570f", b: "#2c6aa8", single: "#b4b2ac", hatch: "#57554f" },
};

type Lab = [number, number, number];

function toLinear(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function fromLinear(c: number): number {
  const v = c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055;
  return Math.min(1, Math.max(0, v));
}

/** `#rrggbb` → OKLab. */
export function oklab(hex: string): Lab {
  const n = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => toLinear(c / 255)) as Lab;
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/** OKLab → sRGB channels in 0–1, clipped to the gamut. */
export function srgb([L, A, B]: Lab): [number, number, number] {
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return [
    fromLinear(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    fromLinear(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    fromLinear(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

/** Channels in 0–1 → `#rrggbb`. */
export function hex(rgb: readonly [number, number, number]): string {
  return `#${rgb.map((c) => Math.round(c * 255).toString(16).padStart(2, "0")).join("")}`;
}

const cache = new Map<Scheme, { mid: Lab; a: Lab; b: Lab }>();
function labs(scheme: Scheme) {
  let found = cache.get(scheme);
  if (!found) {
    const p = POLES[scheme];
    found = { mid: oklab(p.mid), a: oklab(p.a), b: oklab(p.b) };
    cache.set(scheme, found);
  }
  return found;
}

/**
 * The colour of a difference `t` in −1 (B fastest) … +1 (A fastest), as
 * sRGB channels in 0–1. Outside that range it is held at the pole; NaN is
 * the midpoint.
 */
export function diverging(t: number, scheme: Scheme): [number, number, number] {
  const { mid, a, b } = labs(scheme);
  const x = Number.isFinite(t) ? Math.max(-1, Math.min(1, t)) : 0;
  const pole = x >= 0 ? a : b;
  const f = Math.abs(x);
  return srgb([mid[0] + (pole[0] - mid[0]) * f, mid[1] + (pole[1] - mid[1]) * f, mid[2] + (pole[2] - mid[2]) * f]);
}

/**
 * The half-width of the scale: the largest |Δ| shown, so the scale is
 * symmetric about zero and both ends mean the same size of difference.
 * Never zero, so an all-equal comparison still has a scale.
 */
export function scaleHalfWidth(min: number, max: number): number {
  const m = Math.max(Math.abs(Number.isFinite(min) ? min : 0), Math.abs(Number.isFinite(max) ? max : 0));
  return m > 0 ? m : 1;
}

/** The theme's scheme, from the document (`applyTheme` sets it). */
export function currentScheme(): Scheme {
  if (typeof document === "undefined") return "dark";
  return document.documentElement.dataset.themeScheme === "light" ? "light" : "dark";
}

/** CSS stops for the legend bar, B faster at the left. */
export function legendGradient(scheme: Scheme, steps = 9): string {
  const stops: string[] = [];
  for (let k = 0; k < steps; k++) {
    const t = -1 + (2 * k) / (steps - 1);
    stops.push(`${hex(diverging(t, scheme))} ${((k / (steps - 1)) * 100).toFixed(1)}%`);
  }
  return `linear-gradient(to right, ${stops.join(", ")})`;
}
