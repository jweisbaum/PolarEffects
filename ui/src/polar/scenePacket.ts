/**
 * The 3D scene as Rust sends it: one binary buffer, not JSON (plan.md M7).
 *
 * # Wire layout, version 4
 *
 * The frontend's copy of the layout; the Rust side's is the module
 * documentation of `crates/pe-app/src/polar3d.rs`. Both are held to the same
 * bytes by `fixtures/scene-v5.bin` and `fixtures/scene-v5-flags.bin`
 * (written by a Rust test, read by `scenePacket.test.ts`). Every value is
 * little-endian and 4 bytes wide except the time origin and the samples
 * key, and every section starts on a 4-byte boundary, so each array below
 * is a view into the buffer, not a copy.
 *
 * ```text
 * header, 12 × u32 (48 bytes)
 *   0  magic       0x44334550 (the bytes "PE3D")
 *   1  version     4
 *   2  S           sources
 *   3  N           polar nodes
 *   4  M           samples
 *   5  F           surfaces
 *   6–7 time_origin i64, UTC epoch seconds: sample times are relative to it
 *   8–9 samples_key u64 (below 2^53): names the samples section
 *   10 samples_mode 0 full, 1 flags only
 *   11 reserved, 0
 * sources, S × 4 u32
 *   id_lo, id_hi, colour 0x00RRGGBB, kind (0 ORC, 1 polar file, 2 track, 3 ORR)
 * nodes (structure of arrays)
 *   f32 [N × 3]  TWA °, TWS kn, BSP kn
 *   u32 [N]      source, an index into the sources section
 *   u32 [N]      cell: TWA index | TWS index << 16, in the source's own grid
 *   u32 [N]      flags
 * samples, full (structure of arrays)
 *   f32 [M × 3]  TWA °, TWS kn, BSP kn
 *   u32 [M]      source index
 *   u32 [M × 2]  sample id, lo then hi
 *   f32 [M]      Hs, metres (NaN: none)
 *   f32 [M]      current speed, knots (NaN: none)
 *   f32 [M]      time, seconds since time_origin
 *   f32 [M]      wave period, seconds (NaN: none)
 *   f32 [M]      wave/bow angle, degrees (NaN: none)
 *   f32 [M]      wave/wind angle, degrees (NaN: none)
 *   f32 [M]      wave bearing, degrees clockwise from the bow, 0–360: where
 *                the waves come from as seen from the boat (NaN: none)
 *   u32 [M]      flags
 * samples, flags only
 *   u32 [M]      flags
 * surfaces, F times
 *   u32 source index (0xFFFFFFFF: the blend), u32 ni, u32 nj
 *   f32 [ni] TWA axis, f32 [nj] TWS axis
 *   f32 [ni × nj] BSP, TWA-major (i × nj + j), NaN for an empty cell
 * ```
 *
 * Flags: bit 0 excluded (spec.md 10.3), bit 1 filtered out (spec.md 7.6),
 * bit 2 edited (a node whose cell holds an override, spec.md 10.4), bit 3 a
 * sample placed through the water (corrected for current; otherwise its
 * speed is the track's own, over the ground). Bits
 * 8–9 of a sample's flags are its band of the local solar day
 * (`../dayBand.ts`; spec.md 10.2).
 *
 * A flags-only scene answers a request that named the samples key the view
 * already holds: no sample moved, so only their flags travel, and
 * `unpackScene` takes everything else of the samples from the scene held.
 */

export const SCENE_MAGIC = 0x44334550;
export const SCENE_VERSION = 5;
export const HEADER_BYTES = 48;
export const BLEND_SOURCE = 0xffffffff;
export const FLAG_EXCLUDED = 1;
export const FLAG_FILTERED = 2;
export const FLAG_THROUGH_WATER = 8;
export const FLAG_EDITED = 4;
export const SAMPLES_FULL = 0;
export const SAMPLES_FLAGS_ONLY = 1;

export type SourceKindCode = "orc" | "polar_file" | "track" | "orr";
const KINDS: readonly SourceKindCode[] = ["orc", "polar_file", "track", "orr"];

export interface PacketSource {
  id: number;
  /** `#rrggbb`. */
  colour: string;
  kind: SourceKindCode;
}

export interface PacketSurface {
  /** Index into `sources`, or `BLEND_SOURCE`. */
  source: number;
  twa: Float32Array;
  tws: Float32Array;
  /** TWA-major: `bsp[i * tws.length + j]`; NaN is an empty cell. */
  bsp: Float32Array;
}

export interface ScenePacket {
  timeOrigin: number;
  /** Names the samples section; a later request names it to get only the flags. */
  samplesKey: number;
  sources: PacketSource[];
  nodes: {
    count: number;
    /** (TWA, TWS, BSP) triples. */
    points: Float32Array;
    source: Uint32Array;
    /** TWA index | TWS index << 16. */
    cell: Uint32Array;
    flags: Uint32Array;
  };
  samples: {
    count: number;
    points: Float32Array;
    source: Uint32Array;
    /** (lo, hi) pairs. */
    ids: Uint32Array;
    hs: Float32Array;
    current: Float32Array;
    time: Float32Array;
    wavePeriod: Float32Array;
    waveAngle: Float32Array;
    waveWindAngle: Float32Array;
    /** Where the waves come from as seen from the boat, degrees clockwise from the bow (spec.md 10.5). */
    waveBearing: Float32Array;
    flags: Uint32Array;
  };
  surfaces: PacketSurface[];
}

/** A buffer that is not a scene this build can read. */
export class ScenePacketError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ScenePacketError";
  }
}

/** A sample id from its (lo, hi) words; ids never approach 2^53. */
export function sampleId(ids: Uint32Array, index: number): number {
  return ids[index * 2]! + ids[index * 2 + 1]! * 2 ** 32;
}

/**
 * Reads a packed scene; throws `ScenePacketError` on anything malformed. A
 * flags-only scene needs `held`, the scene whose samples it updates: the
 * same key and the same number of samples.
 */
export function unpackScene(buffer: ArrayBuffer, held?: ScenePacket): ScenePacket {
  if (buffer.byteLength < HEADER_BYTES) throw new ScenePacketError(`a scene of ${buffer.byteLength} bytes has no header`);
  const view = new DataView(buffer);
  const word = (i: number) => view.getUint32(i * 4, true);
  if (word(0) !== SCENE_MAGIC) throw new ScenePacketError("this is not a 3D scene");
  if (word(1) !== SCENE_VERSION) throw new ScenePacketError(`scene version ${word(1)} is not ${SCENE_VERSION}`);
  const [s, n, m, f] = [word(2), word(3), word(4), word(5)];
  const timeOrigin = Number(view.getBigInt64(24, true));
  const samplesKey = word(8) + word(9) * 2 ** 32;
  const mode = word(10);
  if (mode !== SAMPLES_FULL && mode !== SAMPLES_FLAGS_ONLY) throw new ScenePacketError(`samples mode ${mode} is unknown`);

  let offset = HEADER_BYTES;
  const take = <T>(make: (at: number, length: number) => T, length: number): T => {
    const end = offset + length * 4;
    if (end > buffer.byteLength) throw new ScenePacketError(`the scene ends at byte ${buffer.byteLength}, before ${end}`);
    const out = make(offset, length);
    offset = end;
    return out;
  };
  const u32 = (length: number) => take((at, len) => new Uint32Array(buffer, at, len), length);
  const f32 = (length: number) => take((at, len) => new Float32Array(buffer, at, len), length);

  const rawSources = u32(s * 4);
  const sources: PacketSource[] = [];
  for (let k = 0; k < s; k++) {
    const kind = KINDS[rawSources[k * 4 + 3]!];
    if (!kind) throw new ScenePacketError(`source ${k} has the unknown kind ${rawSources[k * 4 + 3]}`);
    sources.push({
      id: rawSources[k * 4]! + rawSources[k * 4 + 1]! * 2 ** 32,
      colour: `#${rawSources[k * 4 + 2]!.toString(16).padStart(6, "0")}`,
      kind,
    });
  }

  const nodes = { count: n, points: f32(n * 3), source: u32(n), cell: u32(n), flags: u32(n) };
  let samples: ScenePacket["samples"];
  if (mode === SAMPLES_FLAGS_ONLY) {
    if (!held || held.samplesKey !== samplesKey || held.samples.count !== m) {
      throw new ScenePacketError("a flags-only scene does not match the samples held");
    }
    const flags = u32(m);
    // Flags that did not change either (an edit of a polar source) keep the
    // very samples held, so the view need not redraw them.
    let same = true;
    for (let k = 0; k < m && same; k++) same = flags[k] === held.samples.flags[k];
    samples = same ? held.samples : { ...held.samples, flags };
  } else {
    samples = {
      count: m, points: f32(m * 3), source: u32(m), ids: u32(m * 2),
      hs: f32(m), current: f32(m), time: f32(m), wavePeriod: f32(m), waveAngle: f32(m), waveWindAngle: f32(m), waveBearing: f32(m), flags: u32(m),
    };
  }
  for (const [what, indices] of [["node", nodes.source], ["sample", samples.source]] as const) {
    for (let k = 0; k < indices.length; k++) {
      if (indices[k]! >= s) throw new ScenePacketError(`${what} ${k} names source ${indices[k]} of ${s}`);
    }
  }

  const surfaces: PacketSurface[] = [];
  for (let k = 0; k < f; k++) {
    const head = u32(3);
    const source = head[0]!, ni = head[1]!, nj = head[2]!;
    if (source !== BLEND_SOURCE && source >= s) throw new ScenePacketError(`surface ${k} names source ${source} of ${s}`);
    surfaces.push({ source, twa: f32(ni), tws: f32(nj), bsp: f32(ni * nj) });
  }
  if (offset !== buffer.byteLength) {
    throw new ScenePacketError(`the scene has ${buffer.byteLength - offset} bytes after its last surface`);
  }
  return { timeOrigin, samplesKey, sources, nodes, samples, surfaces };
}

/** An empty scene, for before the first answer arrives. */
export function emptyScene(): ScenePacket {
  const buffer = new ArrayBuffer(HEADER_BYTES);
  const view = new DataView(buffer);
  view.setUint32(0, SCENE_MAGIC, true);
  view.setUint32(4, SCENE_VERSION, true);
  return unpackScene(buffer);
}

/** "PE3W": the split blends packet (spec.md 10.5; `pe-app/src/polar3d.rs`, `pack_split`). */
export const SPLIT_MAGIC = 0x57334550;
export const SPLIT_VERSION = 1;

/** The blend of one copy of a split view. */
export interface SplitSurface {
  /** Which copy, 0 ≤ cell < count. */
  cell: number;
  twa: Float32Array;
  tws: Float32Array;
  /** TWA-major: `bsp[i * tws.length + j]`; NaN is an empty cell. */
  bsp: Float32Array;
}

/** The blends of a split view: one surface per copy whose blend has something to say. */
export interface SplitPacket {
  count: number;
  surfaces: SplitSurface[];
}

/**
 * Reads the split blends packet: a 4-word header (magic "PE3W", version,
 * copies, surfaces), then surfaces laid out as the scene's, each naming
 * its copy where the scene's name a source.
 */
export function unpackSplit(buffer: ArrayBuffer): SplitPacket {
  if (buffer.byteLength < 16) throw new ScenePacketError(`a split of ${buffer.byteLength} bytes has no header`);
  const view = new DataView(buffer);
  const word = (i: number) => view.getUint32(i * 4, true);
  if (word(0) !== SPLIT_MAGIC) throw new ScenePacketError("this is not a split blends packet (PE3W)");
  if (word(1) !== SPLIT_VERSION) throw new ScenePacketError(`split version ${word(1)} is not ${SPLIT_VERSION}`);
  const [count, f] = [word(2), word(3)];
  let offset = 16;
  const take = (length: number) => {
    const at = offset, end = at + length * 4;
    if (end > buffer.byteLength) throw new ScenePacketError(`the split is short: it ends at byte ${buffer.byteLength}, before ${end}`);
    offset = end;
    return at;
  };
  const u32 = (length: number) => new Uint32Array(buffer, take(length), length);
  const f32 = (length: number) => new Float32Array(buffer, take(length), length);
  const surfaces: SplitSurface[] = [];
  for (let k = 0; k < f; k++) {
    const head = u32(3);
    const cell = head[0]!, ni = head[1]!, nj = head[2]!;
    if (cell >= count) throw new ScenePacketError(`surface ${k} is for copy ${cell} of ${count}`);
    surfaces.push({ cell, twa: f32(ni), tws: f32(nj), bsp: f32(ni * nj) });
  }
  if (offset !== buffer.byteLength) throw new ScenePacketError(`the split has ${buffer.byteLength - offset} bytes after its last surface`);
  return { count, surfaces };
}
