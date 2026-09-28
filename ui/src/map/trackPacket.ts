/**
 * The map's tracks as Rust sends them: one binary buffer (spec.md 9.1).
 *
 * # Wire layout, version 1
 *
 * The frontend's copy; the Rust side's is the module documentation of
 * `crates/pe-app/src/map_tracks.rs`, and both are held to the same bytes by
 * `fixtures/tracks-v1.bin`. Every value is little-endian and 4 bytes wide,
 * so each array below is a view into the buffer, not a copy.
 *
 * ```text
 * header, 4 × u32 (16 bytes)
 *   0  magic    0x544d4550 (the bytes "PEMT")
 *   1  version  1
 *   2  T        tracks
 *   3  F        fixes, all tracks together
 * tracks, T × 5 u32
 *   id_lo, id_hi, colour 0x00RRGGBB, first (index of its first fix), count
 * fixes (structure of arrays, tracks one after another)
 *   f32 [F × 2]  longitude, latitude, degrees; longitudes unwrapped along
 *                each track (each within 180° of the one before), the first
 *                of each track in [-180, 180)
 *   u32 [F × 2]  sample id, lo then hi
 *   u32 [F]      flags: bit 0 excluded by hand, bit 1 filtered out
 * ```
 */

export const TRACKS_MAGIC = 0x544d4550;
export const TRACKS_VERSION = 1;
export const TRACKS_HEADER_BYTES = 16;
export const FIX_EXCLUDED = 1;
export const FIX_FILTERED = 2;

export interface PacketTrack {
  /** Source id; ids stay below 2^53 (pe-core `MAX_ID`), so a number holds them exactly. */
  id: number;
  /** `#rrggbb`. */
  colour: string;
  /** Index of its first fix in the fix arrays. */
  first: number;
  count: number;
}

export interface TrackPacket {
  tracks: PacketTrack[];
  fixes: {
    count: number;
    /** (unwrapped longitude, latitude) pairs. */
    points: Float32Array;
    /** Sample ids as (lo, hi) pairs. */
    ids: Uint32Array;
    flags: Uint32Array;
  };
}

/** A buffer that is not a track packet this build can read. */
export class TrackPacketError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TrackPacketError";
  }
}

/** The sample id of fix `index`. */
export function fixSampleId(packet: TrackPacket, index: number): number {
  return packet.fixes.ids[index * 2]! + packet.fixes.ids[index * 2 + 1]! * 2 ** 32;
}

/** Reads a packed track buffer; throws `TrackPacketError` on anything malformed. */
export function unpackTracks(buffer: ArrayBuffer): TrackPacket {
  if (buffer.byteLength < TRACKS_HEADER_BYTES) throw new TrackPacketError(`${buffer.byteLength} bytes hold no header`);
  const view = new DataView(buffer);
  const word = (i: number) => view.getUint32(i * 4, true);
  if (word(0) !== TRACKS_MAGIC) throw new TrackPacketError("this is not a track packet");
  if (word(1) !== TRACKS_VERSION) throw new TrackPacketError(`track packet version ${word(1)} is not ${TRACKS_VERSION}`);
  const t = word(2), f = word(3);
  const expected = TRACKS_HEADER_BYTES + t * 20 + f * 20;
  if (buffer.byteLength !== expected) {
    throw new TrackPacketError(`a packet of ${t} tracks and ${f} fixes is ${expected} bytes, not ${buffer.byteLength}`);
  }
  const head = new Uint32Array(buffer, TRACKS_HEADER_BYTES, t * 5);
  const tracks: PacketTrack[] = [];
  let next = 0;
  for (let k = 0; k < t; k++) {
    const first = head[k * 5 + 3]!, count = head[k * 5 + 4]!;
    if (first !== next || first + count > f) throw new TrackPacketError(`track ${k} does not follow the one before`);
    next = first + count;
    tracks.push({
      id: head[k * 5]! + head[k * 5 + 1]! * 2 ** 32,
      colour: `#${head[k * 5 + 2]!.toString(16).padStart(6, "0")}`,
      first,
      count,
    });
  }
  if (next !== f) throw new TrackPacketError(`the tracks hold ${next} fixes, not ${f}`);
  let offset = TRACKS_HEADER_BYTES + t * 20;
  const points = new Float32Array(buffer, offset, f * 2);
  offset += f * 8;
  const ids = new Uint32Array(buffer, offset, f * 2);
  offset += f * 8;
  const flags = new Uint32Array(buffer, offset, f);
  return { tracks, fixes: { count: f, points, ids, flags } };
}

/** No tracks, for before the first answer arrives. */
export function emptyTracks(): TrackPacket {
  const buffer = new ArrayBuffer(TRACKS_HEADER_BYTES);
  const view = new DataView(buffer);
  view.setUint32(0, TRACKS_MAGIC, true);
  view.setUint32(4, TRACKS_VERSION, true);
  return unpackTracks(buffer);
}
