/**
 * The comparison as Rust sends it (spec.md 11): one binary buffer, not
 * JSON — a large output grid is a million values per array.
 *
 * # Wire layout, version 1
 *
 * The frontend's copy of the layout; the Rust side's is the module
 * documentation of `crates/pe-app/src/compare.rs`. Both are held to the same
 * bytes by `fixtures/compare-v1.bin` (written by a Rust test, read by
 * `comparePacket.test.ts`). Every value is little-endian and 4 bytes wide.
 *
 * ```text
 * header, 20 × u32 (80 bytes)
 *   0  magic       0x4d434550 (the bytes "PECM")
 *   1  version     1
 *   2  ni          TWA values
 *   3  nj          TWS values
 *   4  R           regions
 *   5  overlap     cells both cover
 *   6  a_only      cells only A covers
 *   7  b_only      cells only B covers
 *   8  u32 the cell of the largest |Δ| kn, i × nj + j (0xFFFFFFFF: none)
 *   9  u32 the cell of the largest |Δ| %
 *   10 f32 threshold, kn
 *   11–14 f32 mean |Δ|, max |Δ|, min Δ, max Δ, kn (NaN: no compared cell)
 *   15–18 f32 the same in percent of B
 *   19 reserved, 0
 * f32 [ni] TWA axis, f32 [nj] TWS axis
 * f32 [ni × nj] A, then B, then Δ kn, then Δ %; TWA-major (i × nj + j),
 *               NaN for no value
 * u32 [ni × nj] class: 0 neither, 1 A only, 2 B only, 3 both, 4 the 0° row
 * regions, R × 4 u32: TWS index, first TWA index, last TWA index,
 *               faster (1 A, 2 B)
 * ```
 */

export const COMPARE_MAGIC = 0x4d434550;
export const COMPARE_VERSION = 1;
export const HEADER_BYTES = 80;
export const NO_CELL = 0xffffffff;

export const CLASS_NEITHER = 0;
export const CLASS_A_ONLY = 1;
export const CLASS_B_ONLY = 2;
export const CLASS_BOTH = 3;
export const CLASS_ZERO_ROW = 4;

/** Statistics of Δ in one unit; NaN where no cell is compared. */
export interface DeltaStats {
  meanAbs: number;
  maxAbs: number;
  min: number;
  max: number;
  /** The cell (i × nj + j) of the largest |Δ|, or -1. */
  maxCell: number;
}

/** A run of TWA cells at one TWS where one operand is faster (spec.md 11). */
export interface Region {
  tws: number;
  firstTwa: number;
  lastTwa: number;
  faster: "a" | "b";
}

export interface ComparePacket {
  twa: Float32Array;
  tws: Float32Array;
  /** TWA-major: `a[i * tws.length + j]`; NaN is no value. */
  a: Float32Array;
  b: Float32Array;
  deltaKn: Float32Array;
  deltaPct: Float32Array;
  /** One `CLASS_*` per cell. */
  cls: Uint32Array;
  overlap: number;
  aOnly: number;
  bOnly: number;
  thresholdKn: number;
  kn: DeltaStats;
  pct: DeltaStats;
  regions: Region[];
}

/** A buffer that is not a comparison this build can read. */
export class ComparePacketError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ComparePacketError";
  }
}

/** Reads a packed comparison; throws `ComparePacketError` on anything malformed. */
export function unpackCompare(buffer: ArrayBuffer): ComparePacket {
  if (buffer.byteLength < HEADER_BYTES) throw new ComparePacketError(`a comparison of ${buffer.byteLength} bytes has no header`);
  const view = new DataView(buffer);
  const word = (i: number) => view.getUint32(i * 4, true);
  const real = (i: number) => view.getFloat32(i * 4, true);
  if (word(0) !== COMPARE_MAGIC) throw new ComparePacketError("this is not a comparison");
  if (word(1) !== COMPARE_VERSION) throw new ComparePacketError(`comparison version ${word(1)} is not ${COMPARE_VERSION}`);
  const [ni, nj, r] = [word(2), word(3), word(4)];
  const cells = ni * nj;
  const cell = (w: number) => (w === NO_CELL || w >= cells ? -1 : w);
  const stats = (at: number, maxCell: number): DeltaStats => ({
    meanAbs: real(at), maxAbs: real(at + 1), min: real(at + 2), max: real(at + 3), maxCell,
  });

  let offset = HEADER_BYTES;
  const take = <T>(make: (at: number, length: number) => T, length: number): T => {
    const end = offset + length * 4;
    if (end > buffer.byteLength) throw new ComparePacketError(`the comparison ends at byte ${buffer.byteLength}, before ${end}`);
    const out = make(offset, length);
    offset = end;
    return out;
  };
  const u32 = (length: number) => take((at, len) => new Uint32Array(buffer, at, len), length);
  const f32 = (length: number) => take((at, len) => new Float32Array(buffer, at, len), length);

  const twa = f32(ni);
  const tws = f32(nj);
  const a = f32(cells), b = f32(cells), deltaKn = f32(cells), deltaPct = f32(cells);
  const cls = u32(cells);
  for (let k = 0; k < cells; k++) {
    if (cls[k]! > CLASS_ZERO_ROW) throw new ComparePacketError(`cell ${k} has the unknown class ${cls[k]}`);
  }
  const raw = u32(r * 4);
  const regions: Region[] = [];
  for (let k = 0; k < r; k++) {
    const [j, first, last, side] = [raw[k * 4]!, raw[k * 4 + 1]!, raw[k * 4 + 2]!, raw[k * 4 + 3]!];
    if (j >= nj || first > last || last >= ni || (side !== 1 && side !== 2)) {
      throw new ComparePacketError(`region ${k} is not on the grid`);
    }
    regions.push({ tws: j, firstTwa: first, lastTwa: last, faster: side === 1 ? "a" : "b" });
  }
  if (offset !== buffer.byteLength) {
    throw new ComparePacketError(`the comparison has ${buffer.byteLength - offset} bytes after its last region`);
  }
  return {
    twa, tws, a, b, deltaKn, deltaPct, cls,
    overlap: word(5), aOnly: word(6), bOnly: word(7), thresholdKn: real(10),
    kn: stats(11, cell(word(8))), pct: stats(15, cell(word(9))), regions,
  };
}

/** An empty comparison, for before the first answer arrives: no cell, no statistic. */
export function emptyCompare(): ComparePacket {
  const buffer = new ArrayBuffer(HEADER_BYTES);
  const view = new DataView(buffer);
  view.setUint32(0, COMPARE_MAGIC, true);
  view.setUint32(4, COMPARE_VERSION, true);
  view.setUint32(32, NO_CELL, true);
  view.setUint32(36, NO_CELL, true);
  for (let w = 11; w <= 18; w++) view.setFloat32(w * 4, Number.NaN, true);
  return unpackCompare(buffer);
}
