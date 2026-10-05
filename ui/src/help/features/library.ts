import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The read-only SYRF track library (asked 2026-10-04): its folders, and searching and importing what they hold. */
const reveal = ["settings:", "settings:library"];
const keywords = ["SYRF", "GeoJSON", msg("library")];
const features: Feature[] = [
  { id: "settings:library-geojson", label: msg("GeoJSON track directory"), description: msg("The folder of track files the boat metadata names."), keywords, topic: "settings", reveal },
  { id: "settings:library-geojson-browse", label: msg("Choose GeoJSON track directory"), description: msg("Choose GeoJSON track directory"), keywords, topic: "settings", reveal },
  { id: "settings:library-metadata", label: msg("Boat metadata directory"), description: msg("Leave empty to use the application data directory"), keywords, topic: "settings", reveal },
  { id: "settings:library-metadata-browse", label: msg("Choose boat metadata directory"), description: msg("Choose boat metadata directory"), keywords, topic: "settings", reveal },
  { id: "settings:library-save", label: msg("Save library settings"), description: msg("Save the library's folders and scraping choices."), keywords, topic: "settings", reveal },
  { id: "settings:library-schedule", label: msg("Run track scraper"), description: msg("Scrape finished races only on demand, or at startup or shutdown."), keywords: [...keywords, msg("scrape"), "YellowBrick", "Geovoile", "Blue Water"], topic: "settings", reveal },
  { id: "settings:library-yb-user-key", label: msg("YellowBrick user key"), description: msg("With the device ID, lists races from YellowBrick's catalogue to scrape."), keywords: [...keywords, "YellowBrick", msg("credentials")], topic: "settings", reveal },
  { id: "settings:library-yb-device-id", label: msg("YellowBrick device ID (UDID)"), description: msg("With the user key, lists races from YellowBrick's catalogue to scrape."), keywords: [...keywords, "YellowBrick", msg("credentials")], topic: "settings", reveal },
  { id: "settings:library-urls", label: msg("Race URLs (optional, one per line)"), description: msg("Scrape these races; leave empty to discover races."), keywords: [...keywords, msg("scrape")], topic: "settings", reveal },
  { id: "settings:library-scrape", label: msg("Scrape tracks now"), description: msg("Scrape finished YellowBrick, Geovoile and Blue Water races into the library now."), keywords: [...keywords, msg("scrape"), "YellowBrick", "Geovoile", "Blue Water"], topic: "settings", reveal },
  { id: "settings:library-cancel", label: msg("Cancel scrape"), description: msg("Cancel the scrape; races already saved stay in the library"), keywords: [...keywords, msg("scrape")], topic: "settings", reveal },
  { id: "settings:library-details", landing: "settings:library-scrape", label: msg("Scrape details"), description: msg("What the last scrape could not do."), keywords: [...keywords, msg("scrape")], topic: "settings", reveal },
  { id: "tracks:boat-search", label: msg("Search tracks by vessel details"), description: msg("Search all vessel fields, including name, model, class, make and builder"), keywords: ["SYRF", "GeoJSON", "boat name", "model", "class", "make", "builder", "vessel", "sail number"], topic: "tracks", reveal: ["panel:left", "section:tracks"] },
  { id: "tracks:boat-import", label: msg("Import this boat track; weather can be fetched afterwards"), description: msg("Import this boat track; weather can be fetched afterwards"), keywords: ["SYRF", "GeoJSON"], topic: "tracks", reveal: ["panel:left", "section:tracks"], landing: "tracks:boat-search" },
  { id: "tracks:boat-more", label: msg("More boat tracks"), description: msg("The next hundred tracks of the search; they load on their own as the list is scrolled to its end."), keywords: [msg("page"), msg("scroll"), msg("library")], topic: "tracks", reveal: ["panel:left", "section:tracks"], landing: "tracks:boat-search" },
];
export default features;
