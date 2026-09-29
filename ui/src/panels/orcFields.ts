/**
 * The ORC section's "Search by field" boxes (spec.md 5.2): what they hold,
 * how many are in use, and what `orc_search` receives from them.
 *
 * Whether the disclosure is open is a viewer's convenience, kept in
 * `localStorage` like the panels' layout (`layout.ts`), never in a project.
 */

import type { OrcFilters } from "../generated/OrcFilters";

/** The text of every per-field box, as typed. */
export interface FieldQueries {
  name: string;
  sail_no: string;
  /** A three-letter country code, or "" for all. */
  country: string;
  model: string;
  builder: string;
  designer: string;
  /** Year built, from and to, inclusive. */
  year_from: string;
  year_to: string;
  certificate_year: string;
}

export const NO_FIELDS: FieldQueries = {
  name: "", sail_no: "", country: "", model: "", builder: "", designer: "",
  year_from: "", year_to: "", certificate_year: "",
};

/** A year box's value as a filter: a whole number, or none. */
export function year(text: string): number | null {
  const value = Number.parseInt(text, 10);
  return Number.isFinite(value) ? value : null;
}

/** What `orc_search` receives for these boxes. */
export function toFilters(fields: FieldQueries): OrcFilters {
  return {
    year_min: year(fields.year_from),
    year_max: year(fields.year_to),
    country: fields.country === "" ? null : fields.country,
    name: fields.name.trim(),
    sail_no: fields.sail_no.trim(),
    model: fields.model.trim(),
    builder: fields.builder.trim(),
    designer: fields.designer.trim(),
    certificate_year: fields.certificate_year.trim(),
  };
}

/**
 * How many fields are in use, for the collapsed disclosure's count. Year
 * built is one field, however many of its two ends are set.
 */
export function activeCount(fields: FieldQueries): number {
  const filters = toFilters(fields);
  const texts = [filters.name, filters.sail_no, filters.model, filters.builder, filters.designer,
    filters.certificate_year].filter((text) => text !== "").length;
  const country = filters.country === null ? 0 : 1;
  const built = filters.year_min !== null || filters.year_max !== null ? 1 : 0;
  return texts + country + built;
}

const KEY = "pe.orcFields";

/** Whether the disclosure was left open. Closed by default; storage may be absent or refuse. */
export function loadFieldsOpen(): boolean {
  try {
    return window.localStorage.getItem(KEY) === "open";
  } catch {
    return false;
  }
}

/** Remembers whether the disclosure is open, if storage allows. */
export function saveFieldsOpen(open: boolean): void {
  try {
    window.localStorage.setItem(KEY, open ? "open" : "closed");
  } catch {
    // A private window, or storage refused: it lasts the session.
  }
}
