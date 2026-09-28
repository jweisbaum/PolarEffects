/**
 * Geometry and hit-testing for the 2D polar plot (spec.md 9.2), kept free of
 * the DOM so it can be unit tested directly.
 *
 * The projection matches the 3D "polar tower" layout (`ui/src/polar/geometry3d.ts`):
 * TWA 0° points up, 90° points right, 180° points down — `x = BSP·sin(TWA)`,
 * `y = -BSP·cos(TWA)` before the canvas's own y-down flip. TWA is folded to
 * [0, 180] everywhere in this codebase, so only the starboard half is drawn;
 * that also matches the tower view rather than inventing a mirrored butterfly
 * this codebase does not otherwise use.
 */

import type { PolarCurve } from "../generated/PolarCurve";
import { dotSourceId, type DotPacket } from "./dotPacket";

const RAD = Math.PI / 180;

/** Where the fan's centre sits and how many pixels one knot of BSP is. */
export interface PlotLayout {
  centerX: number;
  centerY: number;
  /** Pixels per knot of boat speed. */
  scale: number;
}

/** A layout that fits a fan of `maxBsp` knots' radius into `width` × `height`, with `padding` pixels kept clear on every side. */
export function fitLayout(width: number, height: number, maxBsp: number, padding = 28): PlotLayout {
  const usableWidth = Math.max(width - padding * 2, 1);
  const usableHeight = Math.max(height - padding * 2, 1);
  // The fan spans one radius across (0° to 180° is straight up to straight
  // down: height = 2 × radius) and one radius wide (90° is the widest point).
  const radius = maxBsp > 0 ? Math.min(usableWidth, usableHeight / 2) : 1;
  const scale = maxBsp > 0 ? radius / maxBsp : 0;
  return { centerX: padding, centerY: height / 2, scale };
}

/** The canvas position of one (TWA, BSP) point. */
export function project(twa: number, bsp: number, layout: PlotLayout): { x: number; y: number } {
  const r = bsp * layout.scale;
  const rad = twa * RAD;
  return { x: layout.centerX + r * Math.sin(rad), y: layout.centerY - r * Math.cos(rad) };
}

/** The highest BSP any curve or dot reaches, or 0 if none has a point. */
export function maxBoatSpeed(curves: readonly PolarCurve[], dots: DotPacket | null): number {
  let max = 0;
  for (const curve of curves) for (const point of curve.points) if (point.bsp > max) max = point.bsp;
  if (dots) for (let k = 0; k < dots.count; k++) if (dots.points[k * 3 + 2]! > max) max = dots.points[k * 3 + 2]!;
  return max;
}

/**
 * "Nice" round tick values from 0 up to (and just past) `max`, stepping by
 * 1, 2 or 5 times a power of ten so there are roughly `targetCount` of them.
 */
export function niceTicks(max: number, targetCount = 5): number[] {
  if (!(max > 0) || !Number.isFinite(max)) return [];
  const rawStep = max / targetCount;
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const residual = rawStep / magnitude;
  const step = (residual > 5 ? 10 : residual > 2 ? 5 : residual > 1 ? 2 : 1) * magnitude;
  const ticks: number[] = [];
  for (let value = step; value <= max + step / 2; value += step) {
    ticks.push(Math.round(value * 1e6) / 1e6);
  }
  return ticks;
}

/** The angular gridlines a classic polar diagram draws, degrees. */
export const ANGLE_TICKS: readonly number[] = [0, 30, 60, 90, 120, 150, 180];

/** What hovering one point on the plot shows (spec.md 9.2). */
export interface Hover {
  label: string;
  colour: string;
  twa: number;
  tws: number;
  bsp: number;
  /** Canvas position, for the tooltip and the highlighted dot. */
  x: number;
  y: number;
}

/** A source's label and colour, for looking up a dot's (spec.md 9.2). */
export interface SourceStyle {
  label: string;
  colour: string;
}

/**
 * The nearest curve point or dot to `(px, py)`, within `maxDistance` pixels,
 * or `null` when nothing is that close.
 */
export function nearestPoint(
  curves: readonly PolarCurve[],
  dots: DotPacket | null,
  sourcesById: ReadonlyMap<number, SourceStyle>,
  px: number,
  py: number,
  layout: PlotLayout,
  maxDistance: number,
): Hover | null {
  let best: Hover | null = null;
  let bestDistance = maxDistance;
  const consider = (label: string, colour: string, twa: number, tws: number, bsp: number) => {
    const { x, y } = project(twa, bsp, layout);
    const distance = Math.hypot(x - px, y - py);
    if (distance <= bestDistance) {
      bestDistance = distance;
      best = { label, colour, twa, tws, bsp, x, y };
    }
  };
  for (const curve of curves) {
    for (const point of curve.points) consider(curve.label, curve.colour, point.twa, curve.tws, point.bsp);
  }
  if (dots) {
    for (let k = 0; k < dots.count; k++) {
      const style = sourcesById.get(dotSourceId(dots, k));
      if (style) consider(style.label, style.colour, dots.points[k * 3]!, dots.points[k * 3 + 1]!, dots.points[k * 3 + 2]!);
    }
  }
  return best;
}
