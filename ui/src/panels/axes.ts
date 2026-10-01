/**
 * Output grid axes as a person types them (spec.md 12.2): the Blend
 * settings grid editor and the export dialog's custom grid.
 *
 * The rules are the ones Rust holds the grid to (`validate_grid_axis` in
 * pe-core), checked here first so the dialog can say what is wrong before
 * anything is sent: at least one value, at most 512, each in range with at
 * most two decimals, and strictly increasing by at least 0.01. Export
 * writes axis values with two decimals (spec.md 6), so two values closer
 * than that would be written as one and the file would not read back
 * (plan.md M14, carried from M4).
 */

import { msg } from "../i18n";

/** The most values an axis may have (a polar file's limit). */
export const MAX_AXIS_VALUES = 512;

/** The largest value each axis may hold. */
export const AXIS_MAX: Readonly<Record<AxisKind, number>> = { twa: 180, tws: 70 };

export type AxisKind = "twa" | "tws";

/** A refusal: an English message key and its parameters, translated where shown. */
export interface AxisError {
  key: string;
  params: Record<string, string | number>;
}

export type AxisResult = { values: number[]; error?: undefined } | { values?: undefined; error: AxisError };

const EPSILON = 1e-9;

/** Reads a list typed with spaces, commas or semicolons between values; the decimal point is ".". */
export function parseAxis(text: string, kind: AxisKind, asymmetric = false): AxisResult {
  const words = text.split(/[\s,;]+/).filter((word) => word.length > 0);
  if (words.length === 0) return { error: { key: msg("Enter at least one value."), params: {} } };
  if (words.length > MAX_AXIS_VALUES) {
    return { error: { key: msg("At most {max} values."), params: { max: MAX_AXIS_VALUES } } };
  }
  const max = kind === "twa" && asymmetric ? 360 : AXIS_MAX[kind];
  const values: number[] = [];
  for (const word of words) {
    const value = Number(word);
    if (!/^[+]?\d*\.?\d+$/.test(word) || !Number.isFinite(value)) {
      return { error: { key: msg("“{value}” is not a number."), params: { value: word } } };
    }
    if (value < 0 || value > max) {
      return { error: { key: msg("{value} is outside 0 to {max}."), params: { value: word, max } } };
    }
    values.push(value);
  }
  for (let k = 1; k < values.length; k++) {
    const [first, second] = [values[k - 1]!, values[k]!];
    if (second <= first) {
      return { error: { key: msg("{first} and {second} are not in increasing order."), params: { first, second } } };
    }
    if (second - first < 0.01 - EPSILON) {
      return { error: { key: msg("{first} and {second} are closer than 0.01: a polar file could not tell them apart."), params: { first, second } } };
    }
  }
  for (const value of values) {
    if (Math.abs(value * 100 - Math.round(value * 100)) > 1e-6) {
      return { error: { key: msg("{value} has more than two decimals."), params: { value } } };
    }
  }
  return { values };
}

/** An axis as the editor shows it: values separated by ", ". */
export function formatAxis(values: readonly number[]): string {
  return values.map((value) => String(value)).join(", ");
}
