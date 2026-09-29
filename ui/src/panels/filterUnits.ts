/**
 * Track filters in the display units (M17b review): a filter is stored in
 * knots and metres (CLAUDE.md conventions) and shown and typed in the
 * units chosen in Settings, converted here and nowhere else.
 */
import type { Units } from "../generated/Units";
import { SPEED_FACTOR, SPEED_SYMBOL } from "../polar/view3d";

export const DEFAULT_UNITS: Units = { speed: "kn", wave_height: "m", distance: "nm" };

/** Display units per stored unit, and the symbol, for a filter's quantity. */
export interface FilterUnit { factor: number; symbol: string }

export function speedUnit(units: Units): FilterUnit {
  return { factor: SPEED_FACTOR[units.speed], symbol: SPEED_SYMBOL[units.speed] };
}

export function waveUnit(units: Units): FilterUnit {
  return units.wave_height === "ft" ? { factor: 1 / 0.3048, symbol: "ft" } : { factor: 1, symbol: "m" };
}

/** A stored value as its box shows it: three decimals at most, trailing zeros dropped. */
export function shownValue(stored: number | null, factor = 1): string {
  return stored === null ? "" : String(Number((stored * factor).toFixed(3)));
}

/**
 * What a box's text stores: null for empty, undefined for unreadable, and
 * `stored` unchanged when the text still reads as the stored value, so a
 * box left alone never commits a rounded copy of itself.
 */
export function storedValue(text: string, stored: number | null, factor = 1): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const typed = Number(trimmed);
  if (!Number.isFinite(typed)) return undefined;
  if (trimmed === shownValue(stored, factor)) return stored;
  return typed / factor;
}
