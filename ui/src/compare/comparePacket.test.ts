/**
 * The frontend half of the comparison transport: it reads the very bytes
 * the Rust packing test pins (`fixtures/compare-v1.bin`), and refuses
 * malformed ones.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { CLASS_A_ONLY, CLASS_BOTH, CLASS_ZERO_ROW, ComparePacketError, HEADER_BYTES, unpackCompare } from "./comparePacket";

function fixture(): ArrayBuffer {
  const bytes = readFileSync(new URL("./fixtures/compare-v1.bin", import.meta.url));
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

const f32 = (x: number) => Math.fround(x);

describe("unpackCompare", () => {
  it("reads the comparison the Rust test packed", () => {
    const c = unpackCompare(fixture());
    expect([...c.twa]).toEqual([0, 90]);
    expect([...c.tws]).toEqual([6, 12]);
    expect([...c.a]).toEqual([0, 0, 7.5, 8]);
    expect(Number.isNaN(c.b[1]!)).toBe(true);
    expect(c.b[2]).toBe(7);
    expect(c.deltaKn[2]).toBe(0.5);
    expect(c.deltaPct[2]).toBe(f32(7.142857));
    expect([...c.cls]).toEqual([CLASS_ZERO_ROW, CLASS_ZERO_ROW, CLASS_BOTH, CLASS_A_ONLY]);
    expect([c.overlap, c.aOnly, c.bOnly]).toEqual([1, 1, 0]);
    expect(c.thresholdKn).toBe(f32(0.05));
    expect(c.kn).toEqual({ meanAbs: 0.5, maxAbs: 0.5, min: 0.5, max: 0.5, maxCell: 2 });
    expect(c.pct.maxCell).toBe(2);
    expect(c.regions).toEqual([{ tws: 0, firstTwa: 1, lastTwa: 1, faster: "a" }]);
  });

  it("refuses a short, foreign, truncated or padded buffer", () => {
    expect(() => unpackCompare(new ArrayBuffer(8))).toThrow(ComparePacketError);
    const foreign = fixture();
    new DataView(foreign).setUint32(0, 1, true);
    expect(() => unpackCompare(foreign)).toThrow(/not a comparison/);
    expect(() => unpackCompare(fixture().slice(0, HEADER_BYTES + 8))).toThrow(/ends at byte/);
    const padded = new Uint8Array(fixture().byteLength + 4);
    padded.set(new Uint8Array(fixture()));
    expect(() => unpackCompare(padded.buffer)).toThrow(/bytes after/);
  });

  it("refuses a region off the grid", () => {
    const bad = fixture();
    // The region's last TWA index is the second-to-last word.
    new DataView(bad).setUint32(bad.byteLength - 8, 9, true);
    expect(() => unpackCompare(bad)).toThrow(/region 0/);
  });
});
