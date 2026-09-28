import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The Tracks section of the left navigation (spec.md 7.1, 7.4, 7.6). A
 * track's filters and derivation show once the track is unfolded, which the
 * `track:details` step does for the first track. The import dialog's steps
 * are found through File… (see `TrackImportDialog`).
 */
const section = ["section:tracks"];
const details = ["section:tracks", "track:details"];
const topic = "tracks";

const features: Feature[] = [
  { id: "tracks:import-file", label: msg("Import track files"),
    description: msg("Import GeoJSON and CSV tracks, several files at once, choosing the boats and mapping the columns."),
    keywords: [msg("import"), "GeoJSON", "CSV", msg("race"), msg("positions"), msg("column mapping"), msg("boat picker"),
      msg("time format"), msg("speed unit")], topic, reveal: section },
  { id: "tracks:yellowbrick", label: msg("YellowBrick import"), description: msg("Import boats from a YellowBrick race. Arrives in a later version."),
    keywords: [msg("tracker"), msg("race")], topic, reveal: section },
  { id: "tracks:geovoile", label: msg("Geovoile import"), description: msg("Import boats from a Geovoile race. Arrives in a later version."),
    keywords: [msg("tracker"), msg("race")], topic, reveal: section },
  { id: "tracks:bluewater", label: msg("Blue Water Tracks import"), description: msg("Import boats from Blue Water Tracks. Arrives in a later version."),
    keywords: [msg("tracker"), msg("race")], topic, reveal: section },
  { id: "tracks:show-on-map", label: msg("Show track on the map"), description: msg("Frame one track on the map."),
    keywords: [msg("map"), msg("zoom"), msg("track")], topic, reveal: section },
  { id: "tracks:filters", label: msg("Track filters"), description: msg("Unfold a track's sample filters and heading and speed derivation."),
    keywords: [msg("filters"), msg("settings"), msg("track")], topic, reveal: section },
  { id: "tracks:remove", label: msg("Remove track"), description: msg("Remove a track from the project. Undo puts it back."),
    keywords: [msg("delete"), msg("remove")], topic, reveal: section },
  { id: "tracks:refetch", label: msg("Refetch environment"), description: msg("Fetch a track's wind, waves and current again. Arrives with the environment fetch."),
    keywords: [msg("reanalysis"), msg("wind"), msg("resume")], topic, reveal: details },
  { id: "tracks:export-grib", label: msg("Export reanalysis GRIB"), description: msg("Write the wind along a track to a GRIB file. Arrives in a later version."),
    keywords: ["GRIB", msg("export"), msg("wind")], topic, reveal: details },
  { id: "tracks:time-start", label: msg("Time window start"), description: msg("Leave out samples before this time, such as before the start."),
    keywords: [msg("time window"), msg("start"), msg("filters")], topic, reveal: details },
  { id: "tracks:time-end", label: msg("Time window end"), description: msg("Leave out samples after this time, such as after the finish."),
    keywords: [msg("time window"), msg("finish"), msg("filters")], topic, reveal: details },
  { id: "tracks:min-bsp", label: msg("Minimum boat speed"), description: msg("Leave out samples slower than this (1 kn by default)."),
    keywords: ["BSP", msg("filters"), msg("slow")], topic, reveal: details },
  { id: "tracks:max-bsp", label: msg("Maximum boat speed"), description: msg("Leave out samples faster than this."),
    keywords: ["BSP", msg("filters"), msg("fast")], topic, reveal: details },
  { id: "tracks:manoeuvre", label: msg("Manoeuvre threshold"), description: msg("Leave out samples whose heading changes more than this between neighbours (30° by default)."),
    keywords: [msg("tack"), msg("gybe"), msg("filters")], topic, reveal: details },
  { id: "tracks:heading-origin", label: msg("Given or derived heading"), description: msg("Keep samples whose heading the track gave, was derived, or either."),
    keywords: [msg("heading"), "COG", msg("filters")], topic, reveal: details },
  { id: "tracks:speed-origin", label: msg("Given or derived speed"), description: msg("Keep samples whose speed the track gave, was derived, or either."),
    keywords: [msg("speed"), "SOG", msg("filters")], topic, reveal: details },
  { id: "tracks:env-filters", label: msg("Wind, wave and current filters"), description: msg("Filters on TWS, TWA, waves and current. Arrive with the environment fetch."),
    keywords: ["TWS", "TWA", msg("waves"), msg("current")], topic, reveal: details },
  { id: "tracks:max-gap", label: msg("Maximum gap"), description: msg("Neighbours further apart in time than this are not used to derive heading and speed (3 h by default)."),
    keywords: [msg("derivation"), msg("gap"), msg("heading"), msg("speed")], topic, reveal: details },
  { id: "tracks:prefer", label: msg("Prefer given or derived"), description: msg("Use the heading and speed a track gives, or always derive them from the positions."),
    keywords: [msg("derivation"), "COG", "SOG"], topic, reveal: details },
];

export default features;
