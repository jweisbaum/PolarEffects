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

/** Error kinds a Retry cannot change: the address, or a format this build does not read. */
export const NO_RETRY = new Set(["tracker-address", "tracker-legacy", "tracker-unsupported"]);

/** What the address box's tooltip says for each tracker. */
export function addressHint(tracker: TrackerId): string {
  switch (tracker) {
    case "yellowbrick": return t("The event's address, such as a yb.tl link, or its race key");
    case "geovoile": return t("The race's Geovoile viewer address, such as vendeeglobe.geovoile.com/2016/tracker/; add ?leg=2 for the second leg");
    case "bluewater": return t("The race's Blue Water Tracks address");
  }
}

/** The address of another leg of a race in legs: the event's address with its `leg` set. */
export function legAddress(url: string, leg: number): string {
  const [base] = url.split("?");
  return `${base}?leg=${leg}`;
}

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
