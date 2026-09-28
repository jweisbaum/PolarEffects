/**
 * The map's track layer (spec.md 9.1): the geometry the renderer draws, the
 * hover index, box selection and framing, including a track across the
 * antimeridian. The packet is the very bytes the Rust test pins.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { fitCamera } from "./projection";
import { ALPHA_EXCLUDED, ALPHA_FILTERED, ALPHA_KEPT, boxSelect, buildTrackGeometry, FixIndex, frameCamera, selectedPoints } from "./trackLayer";
import { emptyTracks, FIX_EXCLUDED, FIX_FILTERED, fixSampleId, TrackPacketError, unpackTracks } from "./trackPacket";

function fixture(): ArrayBuffer {
  const bytes = readFileSync(new URL("./fixtures/tracks-v1.bin", import.meta.url));
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

const view = { width: 800, height: 400 };

describe("the track packet", () => {
  it("reads the tracks the Rust test packed", () => {
    const packet = unpackTracks(fixture());
    expect(packet.tracks).toEqual([
      { id: 4, colour: "#4e79a7", first: 0, count: 2 },
      { id: 2 ** 32 + 9, colour: "#e15759", first: 2, count: 1 },
    ]);
    expect([...packet.fixes.points]).toEqual([179.5, 10, 180.5, 10.25, -1.5, 50]);
    expect([0, 1, 2].map((k) => fixSampleId(packet, k))).toEqual([5, 2 * 2 ** 32 + 6, 11]);
    expect([...packet.fixes.flags]).toEqual([0, FIX_FILTERED, FIX_EXCLUDED | FIX_FILTERED]);
  });

  it("refuses what is not one", () => {
    expect(() => unpackTracks(new ArrayBuffer(4))).toThrow(TrackPacketError);
    expect(() => unpackTracks(fixture().slice(0, 40))).toThrow(TrackPacketError);
    const wrong = fixture();
    new DataView(wrong).setUint32(0, 1, true);
    expect(() => unpackTracks(wrong)).toThrow(TrackPacketError);
    expect(emptyTracks().tracks).toEqual([]);
  });
});

describe("the track layer", () => {
  it("draws segments within each track only, filtered fixes dimmed", () => {
    const geometry = buildTrackGeometry(unpackTracks(fixture()));
    expect([...geometry.indices]).toEqual([0, 1]);
    const alpha = [3, 7, 11].map((i) => geometry.colours[i]!);
    expect(alpha).toEqual([ALPHA_KEPT, ALPHA_FILTERED, ALPHA_FILTERED].map(Math.fround));
    expect(ALPHA_FILTERED).toBeLessThan(ALPHA_EXCLUDED);
    expect(geometry.colours[0]).toBeCloseTo(0x4e / 255, 6);
  });

  it("finds the fix under the pointer on both sides of the antimeridian", () => {
    const packet = unpackTracks(fixture());
    const index = new FixIndex(packet);
    const camera = { lon: 180, lat: 10, scale: 20 };
    // 180.5° unwrapped is 179.5°W, half a degree east of the centre.
    expect(index.nearest("equirectangular", camera, view, -179.5, 10.25, 410, 195, 8)).toBe(1);
    expect(index.nearest("equirectangular", camera, view, 179.5, 10, 390, 200, 8)).toBe(0);
    expect(index.nearest("equirectangular", camera, view, 0, 0, 10, 10, 8)).toBe(-1);
    expect(index.nearest("orthographic", camera, view, -179.5, 10.25, 410, 195, 8)).toBe(1);
  });

  it("box-selects across the antimeridian and highlights the selection", () => {
    const packet = unpackTracks(fixture());
    const camera = { lon: 180, lat: 10, scale: 20 };
    expect(boxSelect(packet, "equirectangular", camera, view, 380, 180, 420, 210)).toEqual([5, 2 * 2 ** 32 + 6]);
    expect(boxSelect(packet, "equirectangular", camera, view, 405, 180, 420, 210)).toEqual([2 * 2 ** 32 + 6]);
    expect([...selectedPoints(packet, new Set([11]))]).toEqual([-1.5, 50]);
    expect(selectedPoints(packet, new Set()).length).toBe(0);
  });

  it("frames a race across the antimeridian across it, not round the world", () => {
    const camera = frameCamera("equirectangular", view, Float32Array.from([179.5, 10, 180.5, 10.25]), 1000)!;
    expect(camera.lon === 180 || camera.lon === -180).toBe(true);
    expect(camera.lat).toBeCloseTo(10.125, 6);
    // One degree across fills 80 % of 800 px, capped by the 0.25° tall span.
    expect(camera.scale).toBeCloseTo(0.8 * Math.min(800 / 1, 400 / 0.25), 6);
    const wrapped = frameCamera("equirectangular", view, Float32Array.from([179.5, 10, -179.5, 10]), 400)!;
    expect(Math.abs(wrapped.lon)).toBeCloseTo(180, 6);
    expect(frameCamera("equirectangular", view, new Float32Array(), 400)).toBeNull();
    const globe = frameCamera("orthographic", view, Float32Array.from([0, 0, 90, 0]), 400)!;
    expect(globe.scale).toBeGreaterThanOrEqual(fitCamera("orthographic", view).scale);
  });
});
