/**
 * The band of the local solar day a dot carries in its flags (spec.md 9.2,
 * 10.2). Rust decides the band (`pe_tracks::daytime`); these read the codes
 * from the very bytes the Rust packing tests pin, so the two sides cannot
 * disagree about which code is which band.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { DAY_BANDS, dayBand, dayBandRgb } from "./dayBand";
import { unpackDots } from "./panels/dotPacket";
import { unpackScene } from "./polar/scenePacket";

function bytes(path: string): ArrayBuffer {
  const file = readFileSync(new URL(path, import.meta.url));
  return file.buffer.slice(file.byteOffset, file.byteOffset + file.byteLength) as ArrayBuffer;
}

describe("dayBand", () => {
  it("reads bits 8 and 9 of the flags, whatever the other bits hold", () => {
    expect(dayBand(0)).toBe(0);
    expect(dayBand(3)).toBe(0);
    expect(dayBand(1 << 8)).toBe(1);
    expect(dayBand((2 << 8) | 3)).toBe(2);
    expect(dayBand((3 << 8) | 0xfffffc00)).toBe(3);
  });

  it("names the bands in the order Rust codes them", () => {
    expect(DAY_BANDS.map((band) => band.id)).toEqual(["night", "morning", "afternoon", "evening"]);
    expect(DAY_BANDS.map((band) => band.hours)).toEqual(["21:00–05:00", "05:00–12:00", "12:00–17:00", "17:00–21:00"]);
  });

  it("reads the afternoon sample of the 3D fixture and the morning and evening dots of the 2D one", () => {
    const scene = unpackScene(bytes("./polar/fixtures/scene-v5.bin"));
    expect(DAY_BANDS[dayBand(scene.samples.flags[0]!)]!.id).toBe("afternoon");
    const dots = unpackDots(bytes("./panels/fixtures/dots-v2.bin"));
    expect([...dots.flags].map((flags) => DAY_BANDS[dayBand(flags)]!.id)).toEqual(["morning", "evening"]);
  });

  it("gives each band its own colour", () => {
    expect(new Set(DAY_BANDS.map((band) => band.colour)).size).toBe(4);
    expect(dayBandRgb(0)).toEqual([0, 0x72 / 255, 0xb2 / 255]);
  });
});
