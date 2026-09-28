/**
 * What the tracker dialog says and how it searches (spec.md 7.2), kept free
 * of React so it can be tested.
 */
import type { TrackerBoatRow } from "../generated/TrackerBoatRow";
import { fold, t } from "../i18n";

/** The trackers the app knows, by their wire names. */
export type TrackerId = "yellowbrick" | "geovoile" | "bluewater";

/** Brand names: the same in every language. */
export const TRACKER_NAMES: Record<TrackerId, string> = {
  yellowbrick: "YellowBrick",
  geovoile: "Geovoile",
  bluewater: "Blue Water Tracks",
};

/**
 * The boats whose name, sail number, model or division contains every word
 * of the query, whatever the case and accents; all of them for an empty one.
 */
export function filterBoats(boats: TrackerBoatRow[], query: string): TrackerBoatRow[] {
  const words = fold(query).split(/\s+/).filter((w) => w !== "");
  if (words.length === 0) return boats;
  return boats.filter((b) => {
    const text = fold([b.name, b.sail ?? "", b.model ?? "", b.division ?? ""].join(" "));
    // Sail numbers are written with and without spaces: GBR 1124, GBR1124.
    const compact = text.replace(/[\s/-]+/g, "");
    return words.every((w) => text.includes(w) || compact.includes(w.replace(/[/-]+/g, "")));
  });
}

/** A tracker's boat status in the interface language; an unknown one as the tracker wrote it. */
export function boatStatusText(status: string | null): string {
  switch (status) {
    case null: return "";
    case "RACING": return t("Racing");
    case "FINISHED": return t("Finished");
    case "RETIRED": return t("Retired");
    case "DNS": return t("Did not start");
    case "DNF": return t("Did not finish");
    default: return status;
  }
}
