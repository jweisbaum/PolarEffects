/**
 * The frontend half of the 2D dots transport: it reads the bytes the Rust
 * packing test pins (`fixtures/dots-v2.bin`), and refuses malformed ones.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { DOT_EXCLUDED, DOT_FILTERED, DotPacketError, dotSampleId, dotSourceId, unpackDots } from "./dotPacket";

function fixture(): ArrayBuffer {
  const bytes = readFileSync(new URL("./fixtures/dots-v2.bin", import.meta.url));
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

describe("unpackDots", () => {
  it("reads the dots the Rust test packed", () => {
    const dots = unpackDots(fixture());
    expect(dots.count).toBe(2);
    expect(dots.sources).toEqual([2 * 2 ** 32 + 7, 3]);
    expect([...dots.points]).toEqual([45, 10.5, 6.25, 135, 9.5, 8]);
    expect(dotSourceId(dots, 1)).toBe(3);
    expect(dotSampleId(dots, 0)).toBe(11);
    expect(dotSampleId(dots, 1)).toBe(2 ** 32 + 12);
    // Bits 8–9 hold the day band: morning (1) and evening (3).
    expect([...dots.flags]).toEqual([DOT_EXCLUDED | (1 << 8), DOT_FILTERED | (3 << 8)]);
  });

  it("refuses a buffer that is short, foreign, of another length or names no source", () => {
    const good = fixture();
    expect(() => unpackDots(new ArrayBuffer(4))).toThrow(DotPacketError);
    const foreign = good.slice(0);
    new DataView(foreign).setUint32(0, 1, true);
    expect(() => unpackDots(foreign)).toThrow(/not plot dots/);
    expect(() => unpackDots(good.slice(0, good.byteLength - 4))).toThrow(/not 88/);
    const stray = good.slice(0);
    new DataView(stray).setUint32(14 * 4, 9, true);
    expect(() => unpackDots(stray)).toThrow(/names source 9/);
  });
});
