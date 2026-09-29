/**
 * What the Compare stage shows, worked out from the packed comparison
 * (spec.md 11) without three.js or React, so it can be tested in Node: the
 * operands' names and colours, the A, B and difference surfaces, the
 * summary's regions and the heat map's cells. Nothing here computes a
 * difference: every Δ comes from Rust.
 */
import type { CompareOperand } from "../generated/CompareOperand";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { place, type Layout, type PolarGrid } from "../polar/geometry3d";
import type { SurfaceInput } from "../polar/scene3d";
import type { Bounds } from "../polar/view3d";
import { CLASS_A_ONLY, CLASS_B_ONLY, CLASS_BOTH, type ComparePacket } from "./comparePacket";
import { diverging, POLES, scaleHalfWidth, type Scheme } from "./diverging";

/** An operand as the pickers and the legend name it. */
export interface OperandInfo {
  /** Its colour, `#rrggbb`: the source's, or the Blend entry's. */
  colour: string;
  /** The source's label; null for the blend (the view says "Blend"). */
  label: string | null;
  /** `orc`, `polar_file`, `track` or `blend`. */
  kind: string;
  /** A hidden source: not in the blend or the plots, but still comparable. */
  hidden: boolean;
}

export function operandInfo(project: ProjectSummary, operand: CompareOperand): OperandInfo {
  if (operand.kind === "blend") return { colour: project.blend.colour, label: null, kind: "blend", hidden: false };
  const source = project.sources.find((s) => s.id === operand.source_id);
  return {
    colour: source?.colour ?? "#888888",
    label: source?.label ?? null,
    kind: source?.kind ?? "polar_file",
    hidden: source ? !source.visible : false,
  };
}

/** A picker's value for an operand, and back. */
export function operandKey(operand: CompareOperand): string {
  return operand.kind === "blend" ? "blend" : `${operand.kind}:${operand.source_id}`;
}

export function parseOperandKey(key: string): CompareOperand {
  if (key === "blend") return { kind: "blend" };
  const [kind, id] = key.split(":");
  return { kind: kind === "segment" ? "segment" : "polar", source_id: Number(id) };
}

/** Δ in the chosen unit: knots, or percent of B. */
export function deltaOf(packet: ComparePacket, percent: boolean): Float32Array {
  return percent ? packet.deltaPct : packet.deltaKn;
}

/** A grid of one packet array in the scene's shape: `bsp[j][i]`, null for no value. */
export function gridOf(packet: ComparePacket, values: Float32Array, keep?: (k: number) => boolean): PolarGrid {
  const ni = packet.twa.length, nj = packet.tws.length;
  const bsp: (number | null)[][] = [];
  for (let j = 0; j < nj; j++) {
    const row: (number | null)[] = [];
    for (let i = 0; i < ni; i++) {
      const k = i * nj + j;
      const v = values[k]!;
      row.push(Number.isFinite(v) && (keep === undefined || keep(k)) ? v : null);
    }
    bsp.push(row);
  }
  return { twa: [...packet.twa], tws: [...packet.tws], bsp };
}

/** sRGB 0–1 → linear, which three.js expects of vertex colours. */
function linear(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function hexChannels(colour: string): [number, number, number] {
  const n = Number.parseInt(colour.slice(1), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/** Which surfaces are drawn. */
export interface CompareToggles {
  a: boolean;
  b: boolean;
  delta: boolean;
}

export const DEFAULT_COMPARE_TOGGLES: CompareToggles = { a: true, b: true, delta: true };

/**
 * The difference surface: midway between A and B where both have a value,
 * coloured by Δ on the diverging scale; where only one has a value, at that
 * value in neutral grey with a hatch (spec.md 11). The 0° row is no
 * comparison and is left out.
 */
export function differenceSurface(packet: ComparePacket, percent: boolean, scheme: Scheme): SurfaceInput {
  const ni = packet.twa.length, nj = packet.tws.length;
  const heights = new Float32Array(ni * nj).fill(Number.NaN);
  const delta = deltaOf(packet, percent);
  const stats = percent ? packet.pct : packet.kn;
  const half = scaleHalfWidth(stats.min, stats.max);
  const colours = new Float32Array(ni * nj * 3);
  const hatched = new Uint8Array(ni * nj);
  const single = hexChannels(POLES[scheme].single).map(linear);
  const middle = diverging(0, scheme).map(linear);
  for (let i = 0; i < ni; i++) {
    for (let j = 0; j < nj; j++) {
      const k = i * nj + j;
      const node = j * ni + i;
      const cls = packet.cls[k]!;
      let rgb: readonly number[];
      if (cls === CLASS_BOTH) {
        heights[k] = (packet.a[k]! + packet.b[k]!) / 2;
        const d = delta[k]!;
        // Both have a value but Δ % has none (B is 0 kn): the neutral middle.
        rgb = Number.isFinite(d) ? diverging(d / half, scheme).map(linear) : middle;
      } else if (cls === CLASS_A_ONLY || cls === CLASS_B_ONLY) {
        heights[k] = cls === CLASS_A_ONLY ? packet.a[k]! : packet.b[k]!;
        rgb = single;
        hatched[node] = 1;
      } else {
        continue;
      }
      colours.set(rgb, node * 3);
    }
  }
  return {
    grid: gridOf(packet, heights),
    color: POLES[scheme].mid,
    opaque: true,
    vertexColors: colours,
    hatched,
    hatchColor: POLES[scheme].hatch,
    lineColor: POLES[scheme].hatch,
  };
}

/** Every surface to draw: A and B translucent in their colours, the difference opaque. */
export function compareSurfaces(packet: ComparePacket, toggles: CompareToggles, colours: { a: string; b: string },
  percent: boolean, scheme: Scheme): SurfaceInput[] {
  const out: SurfaceInput[] = [];
  if (toggles.a) out.push({ grid: gridOf(packet, packet.a), color: colours.a, opacity: 0.16 });
  if (toggles.b) out.push({ grid: gridOf(packet, packet.b), color: colours.b, opacity: 0.16 });
  if (toggles.delta) out.push(differenceSurface(packet, percent, scheme));
  return out;
}

/** The model-space box round both operands, with the origin; a polar-sized default when empty. */
export function compareBounds(packet: ComparePacket, layout: Layout): Bounds {
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  const add = (twa: number, tws: number, bsp: number) => {
    if (![twa, tws, bsp].every(Number.isFinite)) return;
    const p = place(twa, tws, bsp, layout);
    for (let a = 0; a < 3; a++) { min[a] = Math.min(min[a]!, p[a]!); max[a] = Math.max(max[a]!, p[a]!); }
  };
  const nj = packet.tws.length;
  for (const values of [packet.a, packet.b]) {
    for (let k = 0; k < values.length; k++) add(packet.twa[Math.floor(k / nj)]!, packet.tws[k % nj]!, values[k]!);
  }
  if (!Number.isFinite(min[0])) {
    add(0, 0, 0);
    add(180, 20, 10);
    add(90, 0, 10);
  }
  add(0, 0, 0);
  return { min, max };
}

/** One TWS row of the summary's regions (spec.md 11): TWA spans in degrees. */
export interface RegionRow {
  tws: number;
  a: [number, number][];
  b: [number, number][];
}

/** The regions where A or B is faster, grouped by wind speed, in axis order. */
export function regionRows(packet: ComparePacket): RegionRow[] {
  const rows = new Map<number, RegionRow>();
  for (const region of packet.regions) {
    let row = rows.get(region.tws);
    if (!row) rows.set(region.tws, (row = { tws: packet.tws[region.tws]!, a: [], b: [] }));
    row[region.faster].push([packet.twa[region.firstTwa]!, packet.twa[region.lastTwa]!]);
  }
  return [...rows.entries()].sort((x, y) => x[0] - y[0]).map(([, row]) => row);
}

/** A TWA span as text: one angle, or first–last. */
export function spanText([first, last]: readonly [number, number]): string {
  const f = (v: number) => `${Number(v.toFixed(2))}°`;
  return first === last ? f(first) : `${f(first)}–${f(last)}`;
}

/** One cell of the heat map. */
export interface HeatCell {
  i: number;
  j: number;
  /** Degrees and knots. */
  twa: number;
  tws: number;
  /** The packet's class of the cell. */
  cls: number;
  a: number;
  b: number;
  /** Δ in the unit shown; NaN where not compared. */
  delta: number;
  /** `#rrggbb` fill; null for a cell drawn hatched grey or left blank. */
  fill: string | null;
}

/** The heat map (TWA rows × TWS columns), the 0° row left out: it is never compared. */
export function heatRows(packet: ComparePacket, percent: boolean, scheme: Scheme): { twa: number; cells: HeatCell[] }[] {
  const ni = packet.twa.length, nj = packet.tws.length;
  const delta = deltaOf(packet, percent);
  const stats = percent ? packet.pct : packet.kn;
  const half = scaleHalfWidth(stats.min, stats.max);
  const toHex = (rgb: readonly number[]) => `#${rgb.map((c) => Math.round(c * 255).toString(16).padStart(2, "0")).join("")}`;
  const rows: { twa: number; cells: HeatCell[] }[] = [];
  for (let i = 0; i < ni; i++) {
    if (Math.abs(packet.twa[i]!) < 1e-9) continue;
    const cells: HeatCell[] = [];
    for (let j = 0; j < nj; j++) {
      const k = i * nj + j;
      const cls = packet.cls[k]!;
      const d = delta[k]!;
      cells.push({
        i, j, twa: packet.twa[i]!, tws: packet.tws[j]!, cls, a: packet.a[k]!, b: packet.b[k]!, delta: cls === CLASS_BOTH ? d : Number.NaN,
        fill: cls === CLASS_BOTH ? toHex(diverging(Number.isFinite(d) ? d / half : 0, scheme)) : null,
      });
    }
    rows.push({ twa: packet.twa[i]!, cells });
  }
  return rows;
}
