/** Immediate dot preview of the saved wave constraints while Rust updates the blend. */
import type { ScenePacket } from "./scenePacket";

export type WaveMetric = "hs" | "waveAngle" | "wavePeriod";
export type WaveRange = import("../generated/WaveRangeInput").WaveRangeInput;
export type WaveRanges = import("../generated/WaveRangesInput").WaveRangesInput;
export const NO_WAVE_RANGES: WaveRanges = {
  hs: { min: null, max: null }, waveAngle: { min: null, max: null }, wavePeriod: { min: null, max: null },
};

export function waveRangesActive(ranges: WaveRanges): boolean {
  return Object.values(ranges).some(r => r.min !== null || r.max !== null);
}

/** Precompute limits once per redraw, without per-sample allocations. */
export function waveRangePredicate(samples: ScenePacket["samples"], ranges: WaveRanges): (k: number) => boolean {
  const active = (Object.keys(ranges) as WaveMetric[]).flatMap(key => {
    const { min, max } = ranges[key];
    return min === null && max === null ? [] : [{ values: samples[key],
      min: min === null ? -Infinity : Math.fround(min), max: max === null ? Infinity : Math.fround(max) }];
  });
  return k => {
    for (const r of active) {
      const value = r.values[k]!;
      // An unknown value cannot satisfy an active range. With no bounds it
      // remains visible, just as before the display controls were adjusted.
      if (!Number.isFinite(value) || value < r.min || value > r.max) return false;
    }
    return true;
  };
}
