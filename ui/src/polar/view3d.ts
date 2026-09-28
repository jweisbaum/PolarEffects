/**
 * What the 3D view shows, worked out from the packed scene (spec.md 10.1–
 * 10.3) without three.js, so it can be tested in Node: which dots are drawn
 * and how, the surfaces, the camera presets, the axis guides, and what a
 * selection holds.
 *
 * A dot is named everywhere by its **global index**: node `k` is `k`, sample
 * `k` is `nodes.count + k`. The scene draws only the dots the toggles leave
 * on, so `DotBuild.refs` maps each drawn dot back to its global index.
 */
import type { PolarNodeRef } from "../generated/PolarNodeRef";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { CARTESIAN_TWA_SCALE, place, type Layout, type PolarGrid } from "./geometry3d";
import { BLEND_SOURCE, FLAG_EXCLUDED, FLAG_FILTERED, sampleId, type ScenePacket } from "./scenePacket";
import { SHAPE_CROSS, SHAPE_DISC, SHAPE_RING, type SurfaceInput, type View } from "./scene3d";

export type ColourMode = "source" | "hs" | "current" | "time";

/** The show toggles (spec.md 10.2). */
export interface Toggles {
  samples: boolean;
  nodes: boolean;
  surfaces: boolean;
  /** Samples removed by the filters, drawn dimmed. */
  filtered: boolean;
}

export const DEFAULT_TOGGLES: Toggles = { samples: true, nodes: true, surfaces: true, filtered: false };

/** The dots to draw. */
export interface DotBuild {
  points: Float32Array;
  colors: Float32Array;
  shapes: Float32Array;
  /** The global index of each drawn dot. */
  refs: Uint32Array;
}

/** `#rrggbb` → (r, g, b) in 0–1. */
export function rgb(colour: string): [number, number, number] {
  const value = Number.parseInt(colour.slice(1), 16);
  if (!/^#[0-9a-fA-F]{6}$/.test(colour)) return [0.5, 0.5, 0.5];
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
}

/** A value with no data: neutral grey. */
const MISSING: [number, number, number] = [0.55, 0.55, 0.55];

/**
 * A perceptual ramp (viridis, five stops) for colouring by a value, `t` in
 * 0–1. Dark purple is low, yellow is high.
 */
const RAMP: readonly [number, number, number][] = [
  [0.267, 0.005, 0.329], [0.229, 0.322, 0.546], [0.128, 0.567, 0.551], [0.369, 0.789, 0.383], [0.993, 0.906, 0.144],
];
export function ramp(t: number): [number, number, number] {
  const x = Math.min(1, Math.max(0, t)) * (RAMP.length - 1);
  const i = Math.min(RAMP.length - 2, Math.floor(x));
  const f = x - i;
  const a = RAMP[i]!, b = RAMP[i + 1]!;
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

/** The sample values a colour mode reads, or null for "by source". */
function modeValues(packet: ScenePacket, mode: ColourMode): Float32Array | null {
  if (mode === "hs") return packet.samples.hs;
  if (mode === "current") return packet.samples.current;
  if (mode === "time") return packet.samples.time;
  return null;
}

/** The finite range of some values, or null when none is finite. */
export function range(values: ArrayLike<number>): [number, number] | null {
  let lo = Infinity, hi = -Infinity;
  for (let i = 0; i < values.length; i++) {
    const v = values[i]!;
    if (Number.isFinite(v)) { if (v < lo) lo = v; if (v > hi) hi = v; }
  }
  return lo <= hi ? [lo, hi] : null;
}

/**
 * Which colour modes have anything to show (spec.md 10.2). Hs and current
 * arrive with the environment (M9) and time with tracks (M8); until a sample
 * has one, its mode is offered disabled.
 */
export function availableModes(packet: ScenePacket): Record<ColourMode, boolean> {
  return {
    source: true,
    hs: range(packet.samples.hs) !== null,
    current: range(packet.samples.current) !== null,
    time: packet.samples.count > 0,
  };
}

/** Whether any sample is filtered out, so "show filtered" has something to show. */
export function hasFiltered(packet: ScenePacket): boolean {
  for (let k = 0; k < packet.samples.count; k++) if (packet.samples.flags[k]! & FLAG_FILTERED) return true;
  return false;
}

/** Dims a colour toward grey: a filtered sample (spec.md 10.2). */
function dim([r, g, b]: readonly [number, number, number]): [number, number, number] {
  return [0.35 * r + 0.65 * 0.5, 0.35 * g + 0.65 * 0.5, 0.35 * b + 0.65 * 0.5];
}

/**
 * The dots the toggles leave on, coloured by `mode`. Polar nodes have no
 * environment, so they keep their source colour in every mode. Excluded
 * samples are rings, excluded nodes crosses (spec.md 10.3).
 */
export function buildDots(packet: ScenePacket, toggles: Toggles, mode: ColourMode): DotBuild {
  const { nodes, samples } = packet;
  const sourceColours = packet.sources.map((s) => rgb(s.colour));
  const values = modeValues(packet, mode);
  const span = values ? range(values) : null;
  // Two passes over flat arrays, no per-dot allocation: this runs on every
  // edit at up to 200,000 dots (spec.md 13).
  const refs = new Uint32Array(nodes.count + samples.count);
  let n = 0;
  if (toggles.nodes) for (let k = 0; k < nodes.count; k++) refs[n++] = k;
  if (toggles.samples) {
    for (let k = 0; k < samples.count; k++) {
      if ((samples.flags[k]! & FLAG_FILTERED) && !toggles.filtered) continue;
      refs[n++] = nodes.count + k;
    }
  }
  const points = new Float32Array(n * 3);
  const colors = new Float32Array(n * 3);
  const shapes = new Float32Array(n);
  const lo = span ? span[0] : 0;
  const width = span && span[1] > span[0] ? span[1] - span[0] : 0;
  for (let d = 0; d < n; d++) {
    const g = refs[d]!;
    const isNode = g < nodes.count;
    const k = isNode ? g : g - nodes.count;
    const set = isNode ? nodes : samples;
    points[d * 3] = set.points[k * 3]!;
    points[d * 3 + 1] = set.points[k * 3 + 1]!;
    points[d * 3 + 2] = set.points[k * 3 + 2]!;
    const flags = set.flags[k]!;
    let colour: readonly [number, number, number];
    if (!isNode && values) {
      const v = values[k]!;
      colour = span && Number.isFinite(v) ? ramp(width > 0 ? (v - lo) / width : 0.5) : MISSING;
    } else {
      colour = sourceColours[set.source[k]!] ?? MISSING;
    }
    if (!isNode && flags & FLAG_FILTERED) colour = dim(colour);
    colors[d * 3] = colour[0];
    colors[d * 3 + 1] = colour[1];
    colors[d * 3 + 2] = colour[2];
    shapes[d] = flags & FLAG_EXCLUDED ? (isNode ? SHAPE_CROSS : SHAPE_RING) : SHAPE_DISC;
  }
  return { points, colors, shapes, refs: refs.slice(0, n) };
}

/** A surface's grid in the scene's shape: `bsp[j][i]`, null for an empty cell. */
export function surfaceGrid(twa: Float32Array, tws: Float32Array, bsp: Float32Array): PolarGrid {
  const nj = tws.length;
  return {
    twa: [...twa],
    tws: [...tws],
    bsp: [...tws].map((_, j) => [...twa].map((__, i) => {
      const v = bsp[i * nj + j]!;
      return Number.isFinite(v) ? v : null;
    })),
  };
}

/** Every surface to draw: one per visible polar source, the blend opaque. */
export function buildSurfaces(packet: ScenePacket, blendColour: string): SurfaceInput[] {
  return packet.surfaces.map((surface) => {
    const blend = surface.source === BLEND_SOURCE;
    return {
      grid: surfaceGrid(surface.twa, surface.tws, surface.bsp),
      color: blend ? blendColour : packet.sources[surface.source]?.colour ?? "#888888",
      opaque: blend,
    };
  });
}

// ------------------------------------------------------------------ units

/** Knots to the display unit (spec.md 3.4); stored values stay knots. */
export const SPEED_FACTOR: Record<SpeedUnit, number> = { kn: 1, ms: 1852 / 3600, kmh: 1.852 };
/** The display unit's symbol. */
export const SPEED_SYMBOL: Record<SpeedUnit, string> = { kn: "kn", ms: "m/s", kmh: "km/h" };

// ----------------------------------------------------------------- bounds

/** A model-space box. */
export interface Bounds {
  min: [number, number, number];
  max: [number, number, number];
}

/** The model-space box round every dot and surface, or a default polar-sized one when there are none. */
export function sceneBounds(packet: ScenePacket, layout: Layout): Bounds {
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  const add = (twa: number, tws: number, bsp: number) => {
    if (![twa, tws, bsp].every(Number.isFinite)) return;
    const p = place(twa, tws, bsp, layout);
    for (let a = 0; a < 3; a++) { min[a] = Math.min(min[a]!, p[a]!); max[a] = Math.max(max[a]!, p[a]!); }
  };
  for (const set of [packet.nodes, packet.samples]) {
    for (let k = 0; k < set.count; k++) add(set.points[k * 3]!, set.points[k * 3 + 1]!, set.points[k * 3 + 2]!);
  }
  if (!Number.isFinite(min[0])) {
    add(0, 0, 0);
    add(180, 20, 10);
    add(90, 0, 10);
  }
  // The origin belongs in the view: speeds and wind start at zero.
  add(0, 0, 0);
  return { min, max };
}

export type CameraPreset = "top" | "side" | "iso";

/**
 * A preset camera (spec.md 10.1): **top** looks down the vertical axis — in
 * the polar tower that is the classic polar diagram, every TWS stacked;
 * **side** looks across it, so each TWS is a level; **iso** is the three-
 * quarter view. Far enough back that the whole box fits a `fov`° lens.
 */
export function presetView(preset: CameraPreset, bounds: Bounds, fov = 40): View {
  const target: [number, number, number] = [0, 1, 2].map((a) => (bounds.min[a]! + bounds.max[a]!) / 2) as [number, number, number];
  const radius = Math.max(1, Math.hypot(...[0, 1, 2].map((a) => bounds.max[a]! - bounds.min[a]!)) / 2);
  const distance = (radius / Math.tan(((fov / 2) * Math.PI) / 180)) * 1.1;
  // `up` is +z; straight down would leave the view's roll undefined, so the
  // top view leans a hair toward -y.
  const direction: [number, number, number] = preset === "top" ? [0, -0.001, 1]
    : preset === "side" ? [1, 0, 0] : [0.6, -0.6, 0.5];
  const length = Math.hypot(...direction);
  return {
    position: [0, 1, 2].map((a) => target[a]! + (direction[a]! / length) * distance) as [number, number, number],
    target,
  };
}

// ----------------------------------------------------------------- guides

/** A tick label: where in model space, and what it says. */
export interface GuideLabel {
  at: [number, number, number];
  text: string;
}

/** Axis guide lines (segments) and their tick labels. */
export interface Guides {
  segments: Float32Array;
  labels: GuideLabel[];
}

/** Round tick values from 0 to `max`, about `count` of them. */
export function ticks(max: number, count = 5): number[] {
  if (!(max > 0)) return [0];
  const raw = max / count;
  const step = [1, 2, 5, 10].map((m) => m * 10 ** Math.floor(Math.log10(raw))).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  for (let v = 0; v <= max + 1e-9; v += step) out.push(Number(v.toFixed(6)));
  return out;
}

/**
 * The axis legend drawn in the scene (spec.md 10.1), in the display unit.
 * Polar tower: BSP rings on the floor with their speeds, TWA spokes every
 * 30° with their angles, and the TWS axis up the middle with its speeds.
 * Cartesian: the three axes from the origin with ticks.
 */
export function buildGuides(bounds: Bounds, layout: Layout, unit: SpeedUnit): Guides {
  const factor = SPEED_FACTOR[unit];
  const symbol = SPEED_SYMBOL[unit];
  const segments: number[] = [];
  const labels: GuideLabel[] = [];
  const speed = (knots: number) => `${Number((knots * factor).toFixed(1))} ${symbol}`;
  const line = (a: [number, number, number], b: [number, number, number]) => segments.push(...a, ...b);
  const maxTws = Math.max(1, layout === "tower" ? bounds.max[2] : bounds.max[1]);

  if (layout === "tower") {
    const maxBsp = Math.max(1, ...[0, 1].flatMap((a) => [Math.abs(bounds.min[a]!), Math.abs(bounds.max[a]!)]));
    for (const r of ticks(maxBsp, 4).filter((r) => r > 0)) {
      for (let a = 0; a < 180; a += 10) {
        line(place(a, 0, r, "tower"), place(a + 10, 0, r, "tower"));
      }
      labels.push({ at: place(90, 0, r, "tower"), text: speed(r) });
    }
    for (let a = 0; a <= 180; a += 30) {
      line([0, 0, 0], place(a, 0, maxBsp, "tower"));
      labels.push({ at: place(a, 0, maxBsp * 1.08, "tower"), text: `${a}°` });
    }
    line([0, 0, 0], [0, 0, maxTws]);
    for (const w of ticks(maxTws, 5).filter((w) => w > 0)) {
      line([-0.3, 0, w], [0.3, 0, w]);
      labels.push({ at: [-0.6, 0, w], text: speed(w) });
    }
  } else {
    const maxBsp = Math.max(1, bounds.max[2]);
    const xEnd = 180 / CARTESIAN_TWA_SCALE;
    line([0, 0, 0], [xEnd, 0, 0]);
    line([0, 0, 0], [0, maxTws, 0]);
    line([0, 0, 0], [0, 0, maxBsp]);
    for (let a = 0; a <= 180; a += 30) labels.push({ at: [a / CARTESIAN_TWA_SCALE, -0.8, 0], text: `${a}°` });
    for (const w of ticks(maxTws, 5).filter((w) => w > 0)) labels.push({ at: [-0.8, w, 0], text: speed(w) });
    for (const b of ticks(maxBsp, 4).filter((b) => b > 0)) labels.push({ at: [-0.8, 0, b], text: speed(b) });
  }
  return { segments: Float32Array.from(segments), labels };
}

// -------------------------------------------------------------- selection

/**
 * Stable names for selected dots that survive a refetch: a node by its
 * source and grid cell, a sample by its id. Numbers in maps and sets, not
 * strings: re-finding a selection runs over every dot on every edit, and at
 * 200,000 dots building a string per dot costs tens of milliseconds.
 */
export interface SelectionKeys {
  /** Source id → cells (TWA index | TWS index << 16). */
  nodes: Map<number, Set<number>>;
  samples: Set<number>;
}

export function emptyKeys(): SelectionKeys {
  return { nodes: new Map(), samples: new Set() };
}

/** The keys of some dots, by global index. */
export function keysOf(packet: ScenePacket, globals: readonly number[]): SelectionKeys {
  const keys = emptyKeys();
  const { nodes, samples } = packet;
  for (const g of globals) {
    if (g < nodes.count) {
      const id = packet.sources[nodes.source[g]!]!.id;
      let cells = keys.nodes.get(id);
      if (!cells) keys.nodes.set(id, (cells = new Set()));
      cells.add(nodes.cell[g]!);
    } else if (g - nodes.count < samples.count) {
      keys.samples.add(sampleId(samples.ids, g - nodes.count));
    }
  }
  return keys;
}

/** The global indices of `keys` in a (new) packet, in order; keys no longer there drop out. */
export function resolveKeys(packet: ScenePacket, keys: SelectionKeys): number[] {
  const out: number[] = [];
  const { nodes, samples } = packet;
  if (keys.nodes.size > 0) {
    const bySource = packet.sources.map((s) => keys.nodes.get(s.id));
    for (let k = 0; k < nodes.count; k++) if (bySource[nodes.source[k]!]?.has(nodes.cell[k]!)) out.push(k);
  }
  if (keys.samples.size > 0) {
    for (let k = 0; k < samples.count; k++) if (keys.samples.has(sampleId(samples.ids, k))) out.push(nodes.count + k);
  }
  return out;
}

/** The sample ids among some dots. */
export function sampleIdsOf(packet: ScenePacket, globals: readonly number[]): number[] {
  const out: number[] = [];
  for (const g of globals) {
    const k = g - packet.nodes.count;
    if (k >= 0 && k < packet.samples.count) out.push(sampleId(packet.samples.ids, k));
  }
  return out;
}

/**
 * The part of a selection that is drawn. A dot the toggles hide (samples
 * off, or a filtered sample while "show filtered" is off) must not be
 * counted or excluded by an action the person cannot see it take part in.
 */
export function drawnOnly(selection: readonly number[], refs: Uint32Array, total: number): number[] {
  if (selection.length === 0) return [];
  const drawn = new Uint8Array(total);
  for (let d = 0; d < refs.length; d++) drawn[refs[d]!] = 1;
  return selection.filter((g) => drawn[g] === 1);
}

/** What the selection panel shows (spec.md 10.3). Means are in knots and degrees. */
export interface SelectionSummary {
  count: number;
  meanTwa: number;
  meanTws: number;
  meanBsp: number;
  /** Per source, in source order: how many selected dots are its. */
  bySource: { sourceId: number; count: number }[];
  /** Selected dots currently excluded, and not. */
  excluded: number;
  included: number;
  /** Selected samples (for "show on map"). */
  samples: number;
}

export function summarise(packet: ScenePacket, selection: readonly number[]): SelectionSummary {
  const { nodes, samples } = packet;
  let twa = 0, tws = 0, bsp = 0, excluded = 0, sampleCount = 0;
  const perSource = new Array<number>(packet.sources.length).fill(0);
  for (const g of selection) {
    const isNode = g < nodes.count;
    const k = isNode ? g : g - nodes.count;
    const set = isNode ? nodes : samples;
    twa += set.points[k * 3]!;
    tws += set.points[k * 3 + 1]!;
    bsp += set.points[k * 3 + 2]!;
    perSource[set.source[k]!]! += 1;
    if (set.flags[k]! & FLAG_EXCLUDED) excluded++;
    if (!isNode) sampleCount++;
  }
  const count = selection.length;
  return {
    count,
    meanTwa: count ? twa / count : NaN,
    meanTws: count ? tws / count : NaN,
    meanBsp: count ? bsp / count : NaN,
    bySource: perSource.flatMap((c, i) => (c > 0 ? [{ sourceId: packet.sources[i]!.id, count: c }] : [])),
    excluded,
    included: count - excluded,
    samples: sampleCount,
  };
}

/** The selection as the exclude/include command takes it: polar nodes by grid place, samples by id. */
export function exclusionTargets(packet: ScenePacket, selection: readonly number[]): { nodes: PolarNodeRef[]; samples: number[] } {
  const nodes: PolarNodeRef[] = [];
  const samples: number[] = [];
  for (const g of selection) {
    if (g < packet.nodes.count) {
      const cell = packet.nodes.cell[g]!;
      nodes.push({ source_id: packet.sources[packet.nodes.source[g]!]!.id, twa_index: cell & 0xffff, tws_index: cell >>> 16 });
    } else {
      samples.push(sampleId(packet.samples.ids, g - packet.nodes.count));
    }
  }
  return { nodes, samples };
}

/** Combines a new pick with the selection: Shift adds (toggling a dot already in it), otherwise replaces. */
export function combine(current: readonly number[], picked: readonly number[], add: boolean): number[] {
  if (!add) return [...new Set(picked)].sort((a, b) => a - b);
  const set = new Set(current);
  if (picked.length === 1 && set.has(picked[0]!)) set.delete(picked[0]!);
  else for (const g of picked) set.add(g);
  return [...set].sort((a, b) => a - b);
}
