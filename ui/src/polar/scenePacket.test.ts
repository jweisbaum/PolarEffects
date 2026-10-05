/**
 * The frontend half of the scene transport: it reads the very bytes the Rust
 * packing test pins (`fixtures/scene-v5.bin`, and its flags-only twin), and
 * refuses malformed ones.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  BLEND_SOURCE, FLAG_EDITED, FLAG_EXCLUDED, FLAG_FILTERED, FLAG_THROUGH_WATER, HEADER_BYTES, sampleId, ScenePacketError, unpackScene,
  unpackSplit,
} from "./scenePacket";

function fixture(name = "scene-v5.bin"): ArrayBuffer {
  const bytes = readFileSync(new URL(`./fixtures/${name}`, import.meta.url));
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

const f32 = (x: number) => Math.fround(x);

describe("unpackScene", () => {
  it("reads the scene the Rust test packed", () => {
    const scene = unpackScene(fixture());
    expect(scene.timeOrigin).toBe(1_753_000_000);
    expect(scene.samplesKey).toBe(0x0010_0304 * 2 ** 32 + 0x0506_0708);
    expect(scene.sources).toEqual([
      { id: 7, colour: "#4e79a7", kind: "orc" },
      { id: 3 * 2 ** 32 + 9, colour: "#e15759", kind: "track" },
    ]);
    expect(scene.nodes.count).toBe(2);
    expect([...scene.nodes.points]).toEqual([52, 6, f32(5.9), 90, 12, f32(8.4)]);
    expect([...scene.nodes.source]).toEqual([0, 0]);
    expect(scene.nodes.cell[1]! & 0xffff).toBe(2);
    expect(scene.nodes.cell[1]! >>> 16).toBe(1);
    expect([...scene.nodes.flags]).toEqual([FLAG_EDITED, FLAG_EXCLUDED]);

    expect(scene.samples.count).toBe(1);
    expect([...scene.samples.points]).toEqual([135, 14.25, 9.5]);
    expect(scene.samples.source[0]).toBe(1);
    expect(sampleId(scene.samples.ids, 0)).toBe(2 ** 32 + 5);
    expect(scene.samples.hs[0]).toBe(1.5);
    expect(scene.samples.wavePeriod[0]).toBe(8.5);
    expect(scene.samples.waveAngle[0]).toBe(30);
    expect(scene.samples.waveWindAngle[0]).toBe(15);
    // 30° off the bow, on the port side: 330° clockwise.
    expect(scene.samples.waveBearing[0]).toBe(330);
    expect(Number.isNaN(scene.samples.current[0])).toBe(true);
    expect(scene.samples.time[0]).toBe(600);
    // Bits 8–9 hold the day band: afternoon (2).
    expect(scene.samples.flags[0]).toBe(FLAG_EXCLUDED | FLAG_FILTERED | FLAG_THROUGH_WATER | (2 << 8));

    expect(scene.surfaces).toHaveLength(2);
    const [first, blend] = scene.surfaces;
    expect(first!.source).toBe(0);
    expect([...first!.twa]).toEqual([52, 90]);
    expect([...first!.tws]).toEqual([6, 12]);
    expect([...first!.bsp].slice(0, 3)).toEqual([f32(5.9), f32(7.3), f32(6.8)]);
    expect(Number.isNaN(first!.bsp[3])).toBe(true);
    expect(blend!.source).toBe(BLEND_SOURCE);
    expect([...blend!.bsp]).toEqual([6]);
  });

  it("views the buffer rather than copying it", () => {
    const buffer = fixture();
    const scene = unpackScene(buffer);
    expect(scene.nodes.points.buffer).toBe(buffer);
    expect(scene.nodes.points.byteOffset).toBe(HEADER_BYTES + 2 * 16);
  });

  it("refuses a buffer that is short, foreign, of another version, or too long", () => {
    const good = fixture();
    expect(() => unpackScene(new ArrayBuffer(8))).toThrow(ScenePacketError);
    expect(() => unpackScene(good.slice(0, good.byteLength - 4))).toThrow(/ends at byte/);
    const foreign = good.slice(0);
    new DataView(foreign).setUint32(0, 0x12345678, true);
    expect(() => unpackScene(foreign)).toThrow(/not a 3D scene/);
    const newer = good.slice(0);
    new DataView(newer).setUint32(4, 6, true);
    expect(() => unpackScene(newer)).toThrow(/version 6/);
    const longer = new Uint8Array(good.byteLength + 4);
    longer.set(new Uint8Array(good));
    expect(() => unpackScene(longer.buffer)).toThrow(/after its last surface/);
    const badSource = good.slice(0);
    // The first node's source index (word 26) past the two sources.
    new DataView(badSource).setUint32(26 * 4, 5, true);
    expect(() => unpackScene(badSource)).toThrow(/names source 5/);
  });

  it("takes a flags-only scene's samples from the scene held, with the new flags", () => {
    const held = unpackScene(fixture());
    const changed = { ...held, samples: { ...held.samples, flags: Uint32Array.from([0]) } };
    const delta = unpackScene(fixture("scene-v5-flags.bin"), changed);
    expect(delta.samples.points).toBe(held.samples.points);
    expect([...delta.samples.flags]).toEqual([FLAG_EXCLUDED | FLAG_FILTERED | FLAG_THROUGH_WATER | (2 << 8)]);
    // Flags that did not change keep the very samples held.
    expect(unpackScene(fixture("scene-v5-flags.bin"), held).samples).toBe(held.samples);
    expect(delta.surfaces).toHaveLength(2);
    // Without the scene it updates, or with another, it is refused.
    expect(() => unpackScene(fixture("scene-v5-flags.bin"))).toThrow(/does not match/);
    const other = { ...held, samplesKey: 1 };
    expect(() => unpackScene(fixture("scene-v5-flags.bin"), other)).toThrow(/does not match/);
  });
});

describe("unpackSplit (spec.md 10.5)", () => {
  /** "PE3W" v1, four copies, blends for copies 0 and 2 on a 2 × 1 grid. */
  function packet(words: number[]): ArrayBuffer {
    const out = new DataView(new ArrayBuffer(words.length * 4));
    words.forEach((w, k) => (Number.isInteger(w) && w >= 0 && w < 2 ** 32 ? out.setUint32(k * 4, w, true) : out.setFloat32(k * 4, w, true)));
    return out.buffer;
  }
  const magic = 0x57334550;
  const body = [0, 2, 1, 45.5, 90.5, 12.5, 5.5, 6.5, 2, 2, 1, 45.5, 90.5, 12.5, Number.NaN, 8.5];

  it("reads one surface per copy that has a blend", () => {
    const split = unpackSplit(packet([magic, 1, 4, 2, ...body]));
    expect(split.count).toBe(4);
    expect(split.surfaces.map((s) => s.cell)).toEqual([0, 2]);
    expect([...split.surfaces[0]!.twa]).toEqual([45.5, 90.5]);
    expect([...split.surfaces[0]!.tws]).toEqual([12.5]);
    expect([...split.surfaces[0]!.bsp]).toEqual([5.5, 6.5]);
    expect(split.surfaces[1]!.bsp[0]).toBeNaN();
    expect(unpackSplit(packet([magic, 1, 8, 0])).surfaces).toEqual([]);
  });

  it("refuses what is not a split packet", () => {
    expect(() => unpackSplit(packet([0x44334550, 1, 4, 2, ...body]))).toThrow(/PE3W/);
    expect(() => unpackSplit(packet([magic, 2, 4, 2, ...body]))).toThrow(/version/);
    expect(() => unpackSplit(packet([magic, 1, 2, 2, ...body]))).toThrow(/copy 2 of 2/);
    expect(() => unpackSplit(packet([magic, 1, 4, 2, ...body, 0]))).toThrow(/after/);
    expect(() => unpackSplit(packet([magic, 1, 4, 3, ...body]))).toThrow(/short/);
  });
});
