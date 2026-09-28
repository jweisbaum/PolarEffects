/**
 * The frontend half of the scene transport: it reads the very bytes the Rust
 * packing test pins (`fixtures/scene-v1.bin`), and refuses malformed ones.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { BLEND_SOURCE, FLAG_EXCLUDED, FLAG_FILTERED, HEADER_BYTES, sampleId, ScenePacketError, unpackScene } from "./scenePacket";

function fixture(): ArrayBuffer {
  const bytes = readFileSync(new URL("./fixtures/scene-v1.bin", import.meta.url));
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

const f32 = (x: number) => Math.fround(x);

describe("unpackScene", () => {
  it("reads the scene the Rust test packed", () => {
    const scene = unpackScene(fixture());
    expect(scene.timeOrigin).toBe(1_753_000_000);
    expect(scene.sources).toEqual([
      { id: 7, colour: "#4e79a7", kind: "orc" },
      { id: 3 * 2 ** 32 + 9, colour: "#e15759", kind: "track" },
    ]);
    expect(scene.nodes.count).toBe(2);
    expect([...scene.nodes.points]).toEqual([52, 6, f32(5.9), 90, 12, f32(8.4)]);
    expect([...scene.nodes.source]).toEqual([0, 0]);
    expect(scene.nodes.cell[1]! & 0xffff).toBe(2);
    expect(scene.nodes.cell[1]! >>> 16).toBe(1);
    expect([...scene.nodes.flags]).toEqual([0, FLAG_EXCLUDED]);

    expect(scene.samples.count).toBe(1);
    expect([...scene.samples.points]).toEqual([135, 14.25, 9.5]);
    expect(scene.samples.source[0]).toBe(1);
    expect(sampleId(scene.samples.ids, 0)).toBe(2 ** 32 + 5);
    expect(scene.samples.hs[0]).toBe(1.5);
    expect(Number.isNaN(scene.samples.current[0])).toBe(true);
    expect(scene.samples.time[0]).toBe(600);
    expect(scene.samples.flags[0]).toBe(FLAG_EXCLUDED | FLAG_FILTERED);

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
    new DataView(newer).setUint32(4, 2, true);
    expect(() => unpackScene(newer)).toThrow(/version 2/);
    const longer = new Uint8Array(good.byteLength + 4);
    longer.set(new Uint8Array(good));
    expect(() => unpackScene(longer.buffer)).toThrow(/after its last surface/);
    const badSource = good.slice(0);
    // The first node's source index (word 22) past the two sources.
    new DataView(badSource).setUint32(22 * 4, 5, true);
    expect(() => unpackScene(badSource)).toThrow(/names source 5/);
  });
});
