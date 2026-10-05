/**
 * What hovering a fix on the map shows (spec.md 9.1): time, SOG, heading,
 * TWS, TWA, Hs and current, in the display units. A value the sample does
 * not have yet (wind, waves and current arrive with the environment) shows
 * as a dash, never as a guess.
 */
import type { SampleDetails } from "../generated/SampleDetails";
import type { Units } from "../generated/Units";
import { t } from "../i18n";
import { SPEED_FACTOR, SPEED_SYMBOL } from "../polar/view3d";

export interface HoverLine {
  label: string;
  value: string;
}

const DASH = "–";

/** UTC epoch seconds as `2025-07-26 12:00:00 UTC`. */
export function utc(seconds: number): string {
  return `${new Date(seconds * 1000).toISOString().slice(0, 19).replace("T", " ")} UTC`;
}

function speed(knots: number | null, units: Units): string {
  if (knots === null) return DASH;
  return `${(knots * SPEED_FACTOR[units.speed]).toFixed(1)} ${SPEED_SYMBOL[units.speed]}`;
}

function angle(degrees: number | null): string {
  return degrees === null ? DASH : `${degrees.toFixed(0)}°`;
}

function origin(value: string | null): string {
  if (value === "given") return t("given");
  if (value === "derived") return t("derived");
  return "";
}

/** The hover's lines for one sample. */
export function hoverLines(d: SampleDetails, units: Units): HoverLine[] {
  const withOrigin = (text: string, from: string | null) =>
    text === DASH || !from ? text : t("{value} ({origin})", { value: text, origin: origin(from) });
  const hs = d.hs === null ? DASH
    : units.wave_height === "ft" ? `${(d.hs / 0.3048).toFixed(1)} ft` : `${d.hs.toFixed(1)} m`;
  const current = d.current_speed === null ? DASH
    : d.current_toward === null ? speed(d.current_speed, units)
    : t("{speed} toward {direction}", { speed: speed(d.current_speed, units), direction: angle(d.current_toward) });
  return [
    { label: t("Time"), value: utc(d.t) },
    // The track's own speed, over the ground (asked 2026-10-04): the map
    // shows the position as sailed, never corrected for current.
    { label: "SOG", value: withOrigin(speed(d.bsp, units), d.speed_origin) },
    { label: t("Heading"), value: withOrigin(angle(d.heading), d.heading_origin) },
    { label: "TWS", value: speed(d.tws, units) },
    { label: "TWA", value: angle(d.twa) },
    { label: "Hs", value: hs },
    { label: t("Current"), value: current },
  ];
}
