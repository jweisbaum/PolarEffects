/**
 * The band of the local solar day a sample was sailed in (spec.md 9.2,
 * 10.2): night, morning, afternoon or evening.
 *
 * Rust decides the band (`pe_tracks::daytime`, local mean solar time: UTC
 * shifted by longitude) and packs its two-bit code into bits 8–9 of each
 * dot's flags, in both the 3D scene and the 2D dots. This only names and
 * colours what it is given; the hours below are text for the legend, not a
 * second copy of the rule.
 */
import { msg } from "./i18n";

/** Where a dot's flags hold its band's code. */
export const DAY_BAND_SHIFT = 8;

export interface DayBand {
  id: "night" | "morning" | "afternoon" | "evening";
  /** An English key; the interface translates it. */
  label: string;
  /** The band's hours of local solar time, for the legend. */
  hours: string;
  /** `#rrggbb`. */
  colour: string;
}

/**
 * The bands by code. The colours are four of the Okabe–Ito set, which stay
 * apart for the common colour-vision deficiencies and read on every bundled
 * theme: blue for night, orange for morning, green for afternoon, purple
 * for evening.
 */
export const DAY_BANDS: readonly DayBand[] = [
  { id: "night", label: msg("Night"), hours: "21:00–05:00", colour: "#0072b2" },
  { id: "morning", label: msg("Morning"), hours: "05:00–12:00", colour: "#e69f00" },
  { id: "afternoon", label: msg("Afternoon"), hours: "12:00–17:00", colour: "#009e73" },
  { id: "evening", label: msg("Evening"), hours: "17:00–21:00", colour: "#cc79a7" },
];

/** The band code (0–3, an index into `DAY_BANDS`) a dot's flags carry. */
export function dayBand(flags: number): number {
  return (flags >>> DAY_BAND_SHIFT) & 3;
}

const RGB: readonly (readonly [number, number, number])[] = DAY_BANDS.map((band) => {
  const value = Number.parseInt(band.colour.slice(1), 16);
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255] as const;
});

/** A band's colour as (r, g, b) in 0–1, for the 3D dots. */
export function dayBandRgb(code: number): readonly [number, number, number] {
  return RGB[code & 3]!;
}
