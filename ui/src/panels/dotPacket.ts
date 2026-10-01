/**
 * The 2D polar plot's sample dots as Rust sends them: one binary buffer,
 * not JSON (plan.md M13). In "All" every sample with wind is a dot, and
 * 50 tracks of 10,000 fixes as JSON objects would be tens of megabytes.
 *
 * # Wire layout, version 2
 *
 * The frontend's copy; the Rust side's is the module documentation of
 * `crates/pe-app/src/polar_plot.rs`. Both are held to the same bytes by
 * `fixtures/dots-v2.bin`. Little-endian, every value 4 bytes wide, so each
 * array is a view into the buffer.
 *
 * ```text
 * header, 4 × u32 (16 bytes)
 *   0  magic    0x44324550 (the bytes "PE2D")
 *   1  version  2
 *   2  S        sources
 *   3  M        dots
 * sources, S × 2 u32   id lo, id hi
 * f32 [M × 3]  TWA °, TWS kn, BSP kn
 * u32 [M]      source index
 * u32 [M × 2]  sample id, lo then hi
 * u32 [M]      flags: bit 0 excluded, bit 1 filtered out; bits 8–9 the
 *              band of the local solar day (`../dayBand.ts`; spec.md 9.2)
 * ```
 */

export const DOTS_MAGIC = 0x44324550;
export const DOTS_VERSION = 2;
export const DOT_EXCLUDED = 1;
export const DOT_FILTERED = 2;

export interface DotPacket {
  count: number;
  /** Source ids, by index. */
  sources: number[];
  /** (TWA, TWS, BSP) triples. */
  points: Float32Array;
  source: Uint32Array;
  /** (lo, hi) pairs. */
  ids: Uint32Array;
  flags: Uint32Array;
}

/** A buffer that is not a dots packet this build can read. */
export class DotPacketError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DotPacketError";
  }
}

/** Reads packed dots; throws `DotPacketError` on anything malformed. */
export function unpackDots(buffer: ArrayBuffer): DotPacket {
  if (buffer.byteLength < 16) throw new DotPacketError(`dots of ${buffer.byteLength} bytes have no header`);
  const view = new DataView(buffer);
  const word = (i: number) => view.getUint32(i * 4, true);
  if (word(0) !== DOTS_MAGIC) throw new DotPacketError("these are not plot dots");
  if (word(1) !== DOTS_VERSION) throw new DotPacketError(`dots version ${word(1)} is not ${DOTS_VERSION}`);
  const s = word(2), m = word(3);
  const expected = 16 + s * 8 + m * 28;
  if (buffer.byteLength !== expected) throw new DotPacketError(`dots of ${buffer.byteLength} bytes, not ${expected}`);
  let offset = 16;
  const u32 = (length: number) => { const out = new Uint32Array(buffer, offset, length); offset += length * 4; return out; };
  const rawSources = u32(s * 2);
  const sources = Array.from({ length: s }, (_, k) => rawSources[k * 2]! + rawSources[k * 2 + 1]! * 2 ** 32);
  const points = new Float32Array(buffer, offset, m * 3);
  offset += m * 12;
  const source = u32(m);
  for (let k = 0; k < m; k++) {
    if (source[k]! >= s) throw new DotPacketError(`dot ${k} names source ${source[k]} of ${s}`);
  }
  return { count: m, sources, points, source, ids: u32(m * 2), flags: u32(m) };
}

/** No dots, for before the first answer. */
export function emptyDots(): DotPacket {
  return {
    count: 0, sources: [], points: new Float32Array(0), source: new Uint32Array(0),
    ids: new Uint32Array(0), flags: new Uint32Array(0),
  };
}

/** A dot's sample id. */
export function dotSampleId(dots: DotPacket, k: number): number {
  return dots.ids[k * 2]! + dots.ids[k * 2 + 1]! * 2 ** 32;
}

/** A dot's source id. */
export function dotSourceId(dots: DotPacket, k: number): number {
  return dots.sources[dots.source[k]!]!;
}
