/**
 * The small polar in each ORC search result (spec.md 5.2): one line per wind
 * speed Rust sends (light, medium, strong), starboard half, wind from the
 * top. Pure geometry, so it is tested without a DOM.
 */
import type { OrcThumbCurve } from "../generated/OrcThumbCurve";

/** The thumbnail's size in CSS pixels. */
export const THUMB_WIDTH = 26;
export const THUMB_HEIGHT = 44;
const MARGIN = 2;

/**
 * An SVG path per curve, all on one scale: the fastest point touches the
 * edge. The pole is at the left middle; TWA 0° is straight up, 180° straight
 * down.
 */
export function thumbPaths(curves: OrcThumbCurve[]): string[] {
  const fastest = Math.max(0, ...curves.flatMap((curve) => curve.bsp));
  if (!(fastest > 0)) return curves.map(() => "");
  const radius = Math.min(THUMB_WIDTH - 2 * MARGIN, THUMB_HEIGHT / 2 - MARGIN);
  const scale = radius / fastest;
  const cy = THUMB_HEIGHT / 2;
  return curves.map((curve) =>
    curve.twa
      .map((twa, i) => {
        const angle = (twa * Math.PI) / 180;
        const r = curve.bsp[i]! * scale;
        const x = MARGIN + r * Math.sin(angle);
        const y = cy - r * Math.cos(angle);
        return `${i === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(" "));
}
