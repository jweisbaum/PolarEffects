import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The Tracks section of the left navigation (spec.md 7.1, 7.4, 7.6). A
 * track's filters and derivation show once the track is unfolded, which the
 * `track:details` step does for the first track. The import dialog's
 * controls exist only once files are chosen, so they land on File…
 * (`landing`), and each description says so.
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
  { id: "track-import:time", label: msg("Time column"), description: msg("Inside the track import dialog (choose files with File… first): the CSV column the times are read from."),
    keywords: ["CSV", msg("column"), msg("mapping")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:lat", label: msg("Latitude column"), description: msg("Inside the track import dialog (choose files with File… first): the CSV column the latitudes are read from."),
    keywords: ["CSV", msg("column"), msg("mapping")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:lon", label: msg("Longitude column"), description: msg("Inside the track import dialog (choose files with File… first): the CSV column the longitudes are read from."),
    keywords: ["CSV", msg("column"), msg("mapping")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:heading", label: msg("Heading column"), description: msg("Inside the track import dialog (choose files with File… first): the CSV column with the heading or COG, if any."),
    keywords: ["CSV", "COG", msg("mapping")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:speed", label: msg("Speed column"), description: msg("Inside the track import dialog (choose files with File… first): the CSV column with the SOG or boat speed, if any."),
    keywords: ["CSV", "SOG", msg("mapping")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:boat", label: msg("Boat column"), description: msg("Inside the track import dialog (choose files with File… first): the CSV column naming the boat, for a file with several boats."),
    keywords: ["CSV", msg("boat"), msg("mapping")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:time-format", label: msg("Time format"), description: msg("Inside the track import dialog (choose files with File… first): how the CSV's times are written."),
    keywords: ["ISO 8601", msg("epoch"), msg("date")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:custom-format", label: msg("Custom time format"), description: msg("Inside the track import dialog (choose files with File… first): a pattern such as %d/%m/%Y %H:%M for the times."),
    keywords: [msg("pattern"), msg("date")], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:speed-unit", label: msg("Speed unit"), description: msg("Inside the track import dialog (choose files with File… first): the unit of the CSV's speed column."),
    keywords: [msg("knots"), "km/h", "m/s"], topic, reveal: section, landing: "tracks:import-file" },
  { id: "track-import:boats", label: msg("Boats in this file"), description: msg("Inside the track import dialog (choose files with File… first): tick the boats to import from a file with several."),
    keywords: [msg("boat picker"), msg("choose")], topic, reveal: section, landing: "tracks:import-file" },
];

export default features;
