/**
 * Geometry and hit-testing for the 2D polar plot (spec.md 9.2), kept free of
 * the DOM so it can be unit tested directly.
 *
 * The projection matches the 3D "polar tower" layout (`ui/src/polar/geometry3d.ts`):
 * TWA 0° points up, 90° points right, 180° points down — `x = BSP·sin(TWA)`,
 * `y = -BSP·cos(TWA)` before the canvas's own y-down flip. Symmetric mode draws
 * the starboard half; asymmetric mode centres a full circle with independent
 * port values at 180–360 degrees.
 */

import type { PolarCurve } from "../generated/PolarCurve";
import { DAY_BANDS, dayBand } from "../dayBand";
import { DOT_THROUGH_WATER, dotSourceId, type DotPacket } from "./dotPacket";

const RAD = Math.PI / 180;

/** Where the fan's centre sits and how many pixels one knot of BSP is. */
export interface PlotLayout {
  centerX: number;
  centerY: number;
  /** Pixels per knot of boat speed. */
  scale: number;
}

/**
 * A layout that fits a fan of `maxBsp` knots' radius into `width` ×
 * `height`, with `padding` pixels kept clear on every side. In the panel a
 * symmetric fan sits against the left edge, its 0°–180° axis there; on the
 * stage (`centred`, asked 2026-10-02) that axis runs down the middle, as a
 * full circle's does.
 */
export function fitLayout(width: number, height: number, maxBsp: number, padding = 28, asymmetric = false, centred = false): PlotLayout {
  const usableWidth = Math.max(width - padding * 2, 1);
  const usableHeight = Math.max(height - padding * 2, 1);
  // The fan spans one radius across (0° to 180° is straight up to straight
  // down: height = 2 × radius) and one radius wide (90° is the widest point).
  const halves = asymmetric || centred ? 2 : 1;
  const radius = maxBsp > 0 ? Math.min(usableWidth / halves, usableHeight / 2) : 1;
  const scale = maxBsp > 0 ? radius / maxBsp : 0;
  return { centerX: asymmetric || centred ? width / 2 : padding, centerY: height / 2, scale };
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
  // Only the dots drawn are given: excluded and filtered-out ones only
  // while shown, so the scale follows the checkboxes (asked 2026-10-04).
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

/**
 * The boat speed rings in the display unit: round numbers in that unit
 * (`value`), each placed at its speed in knots (`knots`), the unit every
 * polar is stored in. `factor` is display units per knot; the conversion
 * happens here, at the drawing's boundary, and nowhere else.
 */
export function speedTicks(maxKnots: number, factor = 1): { value: number; knots: number }[] {
  if (!(factor > 0)) return [];
  return niceTicks(maxKnots * factor).map((value) => ({ value, knots: value / factor }));
}

/** The angular gridlines a classic polar diagram draws, degrees. */
export const ANGLE_TICKS: readonly number[] = [0, 30, 60, 90, 120, 150, 180];
export const FULL_ANGLE_TICKS: readonly number[] = [...ANGLE_TICKS, 210, 240, 270, 300, 330];

/** One axis label as drawn: its text and the box it occupies, canvas pixels. */
export interface AxisLabel {
  text: string;
  /** Left edge of the text. */
  x: number;
  /** Top edge of the text. */
  y: number;
  width: number;
  height: number;
}

/** Whether two label boxes overlap (touching edges do not). */
export function labelsOverlap(a: AxisLabel, b: AxisLabel): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * Where the angle (TWA) and ring (BSP) labels go, laid out so no two overlap.
 *
 * Both used to sit on the 90° spoke's line, so the outermost ring's value and
 * "90°" ran into each other (M17a). Ring values now sit just under that spoke
 * and angle labels just outside the outermost ring; a ring label that would
 * still touch one already placed (rings closer together than their text is
 * wide) is left out rather than drawn over it. `measure` is the text width in
 * pixels (the canvas's `measureText` in the view, a fixed advance in tests).
 * Ring values are in the display unit (`factor` units per knot, see
 * `speedTicks`); `maxBsp` and the layout stay in knots.
 */
export function axisLabels(
  layout: PlotLayout,
  maxBsp: number,
  measure: (text: string) => number,
  fontSize = 10,
  factor = 1,
  asymmetric = false,
): AxisLabel[] {
  const gap = 3;
  const labels: AxisLabel[] = [];
  const ticks = speedTicks(maxBsp, factor);
  const outer = Math.max(maxBsp, ticks[ticks.length - 1]?.knots ?? 0) * layout.scale;
  const angleRadius = Math.max(maxBsp * 1.06 * layout.scale, outer + gap + fontSize / 2);
  for (const angle of asymmetric ? FULL_ANGLE_TICKS : ANGLE_TICKS) {
    // Each tack reads 0–180°; its full-circle angle still places the label.
    const text = `${Math.min(angle, 360 - angle)}°`;
    const width = measure(text);
    const rad = angle * RAD;
    const cx = layout.centerX + angleRadius * Math.sin(rad);
    const cy = layout.centerY - angleRadius * Math.cos(rad);
    // Centred over the spoke at 0° and 180°; otherwise starting at it, and
    // at 90° lifted clear of the ring labels under the spoke.
    const x = angle === 0 || angle === 180 ? cx - width / 2 : angle > 180 ? cx - width : cx;
    const y = angle === 90 ? cy - gap - fontSize : cy - fontSize / 2;
    labels.push({ text, x, y, width, height: fontSize });
  }
  const placed = [...labels];
  for (const tick of ticks) {
    const text = String(tick.value);
    const label: AxisLabel = {
      text,
      x: layout.centerX + tick.knots * layout.scale + gap,
      y: layout.centerY + gap,
      width: measure(text),
      height: fontSize,
    };
    if (placed.some((other) => labelsOverlap(label, other))) continue;
    placed.push(label);
  }
  return placed;
}

/** What the dots' colour shows (spec.md 9.2). */
export type DotColourMode = "source" | "timeOfDay";

/**
 * The colour dot `k` is drawn in: its track's (`bySource`, by the packet's
 * source index), or its band's of the local solar day.
 */
export function dotFill(dots: DotPacket, k: number, mode: DotColourMode, bySource: readonly string[]): string {
  if (mode === "timeOfDay") return DAY_BANDS[dayBand(dots.flags[k]!)]!.colour;
  return bySource[dots.source[k]!]!;
}

/** What hovering one point on the plot shows (spec.md 9.2). */
/** The abbreviation a point's speed is shown with. */
export type SpeedKind = "BSP" | "SOG" | "STW";

export interface Hover {
  label: string;
  colour: string;
  twa: number;
  tws: number;
  bsp: number;
  /**
   * What `bsp` is: a curve's boat speed, or a track sample's speed over the
   * ground (SOG), or through the water where it was corrected for current
   * (STW). A dot is labelled by its own (asked 2026-10-04).
   */
  speed: SpeedKind;
  /** Whether the point is on the blend's curve rather than a source's or a dot. */
  blend: boolean;
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
  const consider = (label: string, colour: string, twa: number, tws: number, bsp: number, speed: SpeedKind, blend = false) => {
    const { x, y } = project(twa, bsp, layout);
    const distance = Math.hypot(x - px, y - py);
    if (distance <= bestDistance) {
      bestDistance = distance;
      best = { label, colour, twa, tws, bsp, speed, blend, x, y };
    }
  };
  for (const curve of curves) {
    // The blend's curves are the ones with no source (`PolarCurve.source_id`).
    const blend = curve.source_id === null;
    for (const point of curve.points) consider(curve.label, curve.colour, point.twa, curve.tws, point.bsp, "BSP", blend);
  }
  if (dots) {
    for (let k = 0; k < dots.count; k++) {
      const style = sourcesById.get(dotSourceId(dots, k));
      if (style) {
        consider(style.label, style.colour, dots.points[k * 3]!, dots.points[k * 3 + 1]!, dots.points[k * 3 + 2]!,
          dots.flags[k]! & DOT_THROUGH_WATER ? "STW" : "SOG");
      }
    }
  }
  return best;
}

// ------------------------------------------------------------- measuring
//
// The Measure tool (spec.md 9.2) reads the plot back: where the pointer is
// in wind angle and boat speed, what every curve says at that angle, and
// the difference between two points. It reads the curves the plot was
// given, in knots; nothing here changes the project.

/** A point of the plot: a true wind angle (degrees) and a boat speed (knots). */
export interface PolarPoint {
  twa: number;
  bsp: number;
}

/**
 * The (TWA, BSP) a canvas position stands for: `project` inverted. The
 * angle is in [0, 360), 0° up and clockwise; the centre is 0 kn at 0°.
 */
export function unproject(x: number, y: number, layout: PlotLayout): PolarPoint {
  const dx = x - layout.centerX;
  const dy = layout.centerY - y;
  const r = Math.hypot(dx, dy);
  if (r === 0 || !(layout.scale > 0)) return { twa: 0, bsp: 0 };
  const twa = (Math.atan2(dx, dy) / RAD + 360) % 360;
  // Rounded to 1e-9°: 270° must not come back as 270.00000000000006.
  return { twa: (Math.round(twa * 1e9) / 1e9) % 360, bsp: r / layout.scale };
}

/**
 * A curve's boat speed at `twa`: its own value at one of its points, a
 * straight line in speed over angle between two (as `pe-polar` reads a
 * polar between its angles), and null outside the angles it covers.
 */
export function curveSpeedAt(curve: PolarCurve, twa: number): number | null {
  const points = curve.points;
  for (let k = 0; k < points.length; k++) {
    const point = points[k]!;
    if (point.twa === twa) return point.bsp;
    const next = points[k + 1];
    if (next && point.twa < twa && twa < next.twa) {
      return point.bsp + (next.bsp - point.bsp) * (twa - point.twa) / (next.twa - point.twa);
    }
  }
  return null;
}

/** One curve's value where the measuring spoke crosses it. */
export interface Crossing {
  label: string;
  colour: string;
  /** The curve's true wind speed, knots. */
  tws: number;
  /** Its boat speed at the spoke's angle, knots. */
  bsp: number;
  /** Whether it is the blend's curve. */
  blend: boolean;
}

/** Every curve with a value at `twa`, fastest first (ties in the order given). */
export function crossings(curves: readonly PolarCurve[], twa: number): Crossing[] {
  const out: Crossing[] = [];
  for (const curve of curves) {
    const bsp = curveSpeedAt(curve, twa);
    if (bsp !== null) out.push({ label: curve.label, colour: curve.colour, tws: curve.tws, bsp, blend: curve.source_id === null });
  }
  return out.sort((a, b) => b.bsp - a.bsp);
}

/** The crossing whose speed is nearest `bsp` (the first on a tie), or -1 when there is none. */
export function nearestCrossing(list: readonly Crossing[], bsp: number): number {
  let best = -1;
  let distance = Infinity;
  list.forEach((crossing, index) => {
    const d = Math.abs(crossing.bsp - bsp);
    if (d < distance) { distance = d; best = index; }
  });
  return best;
}

/** From point `a` to point `b`. */
export interface Measurement {
  /** `b`'s speed less `a`'s, knots. */
  deltaBsp: number;
  /** `b`'s speed over `a`'s; null when `a` has no speed. */
  ratio: number | null;
  /** The angle between them, degrees, the short way round: 0–180. */
  deltaTwa: number;
}

/** The difference between two points of the plot. */
export function measureBetween(a: PolarPoint, b: PolarPoint): Measurement {
  const turn = Math.abs(b.twa - a.twa) % 360;
  return {
    deltaBsp: b.bsp - a.bsp,
    ratio: a.bsp > 0 ? b.bsp / a.bsp : null,
    deltaTwa: turn > 180 ? 360 - turn : turn,
  };
}
