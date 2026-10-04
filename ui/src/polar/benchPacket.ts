/**
 * Development-only: packs synthetic samples in the scene wire layout
 * (`scenePacket.ts`) so the benchmarks can time unpacking the size Rust
 * sends at the spec.md 13 scale. The app never packs; Rust does
 * (`crates/pe-app/src/polar3d.rs`).
 */
import { HEADER_BYTES, SCENE_MAGIC, SCENE_VERSION } from "./scenePacket";

/** One track source and `samples.length / 3` samples, no nodes or surfaces. */
export function packSynthetic(samples: Float32Array): ArrayBuffer {
  const m = samples.length / 3;
  const buffer = new ArrayBuffer(HEADER_BYTES + 16 + m * 14 * 4);
  const view = new DataView(buffer);
  [SCENE_MAGIC, SCENE_VERSION, 1, 0, m, 0].forEach((v, i) => view.setUint32(i * 4, v, true));
  let at = HEADER_BYTES;
  [30, 0, 0x4e79a7, 2].forEach((v) => { view.setUint32(at, v, true); at += 4; });
  new Float32Array(buffer, at, m * 3).set(samples);
  const ids = new Uint32Array(buffer, at + m * 16, m * 2);
  for (let k = 0; k < m; k++) ids[k * 2] = k;
  return buffer;
}
