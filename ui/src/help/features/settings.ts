import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The Settings dialog (spec.md 3.4). `settings:` opens it (App); the second
 * step brings the section into view once the dialog is mounted.
 */
const open = (section: string) => ["settings:", `settings:${section}`];

const features: Feature[] = [
  { id: "settings:theme", label: msg("Theme"), description: msg("Choose the colours of the whole application."),
    keywords: [msg("colours"), msg("appearance"), msg("dark mode")], topic: "settings", reveal: open("appearance") },
  { id: "settings:language", label: msg("Language"), description: msg("Choose the language of the interface and help."),
    keywords: [msg("translation"), msg("French"), msg("German"), msg("English")], topic: "settings", reveal: open("appearance") },
  { id: "settings:speed-unit", label: msg("Boat and wind speed"), description: msg("The unit speeds are shown in: knots, m/s or km/h."),
    keywords: [msg("units"), msg("knots"), "BSP", "TWS"], topic: "settings", reveal: open("units") },
  { id: "settings:wave-unit", label: msg("Wave height"), description: msg("The unit wave heights are shown in: metres or feet."),
    keywords: [msg("units"), msg("waves"), msg("metres"), msg("feet")], topic: "settings", reveal: open("units") },
  { id: "settings:distance-unit", label: msg("Distance"), description: msg("The unit distances are shown in: nautical miles or kilometres."),
    keywords: [msg("units"), msg("nautical miles"), msg("kilometres")], topic: "settings", reveal: open("units") },
  { id: "settings:autosave", label: msg("Autosave"), description: msg("What happens to unsaved work between saves."),
    keywords: [msg("recovery"), msg("backup"), msg("crash")], topic: "settings", reveal: open("autosave") },
  { id: "settings:cache-location", label: msg("Chunk cache location"), description: msg("Keep downloaded wind, wave and current data in another folder."),
    keywords: [msg("cache"), msg("folder"), msg("disk")], topic: "settings", reveal: open("cache") },
  { id: "settings:cache-default", label: msg("Use the default folder"), description: msg("Keep the chunk cache in the application's own cache folder."),
    keywords: [msg("cache"), msg("folder"), msg("reset")], topic: "settings", reveal: open("cache") },
  { id: "settings:cache-limit", label: msg("Cache size limit"), description: msg("How large the chunk cache may grow, in GB."),
    keywords: [msg("cache"), msg("disk space"), msg("gigabytes")], topic: "settings", reveal: open("cache") },
  { id: "settings:cache-clear", label: msg("Clear cache"), description: msg("Delete the downloaded data. Projects keep every value they use."),
    keywords: [msg("cache"), msg("disk space"), msg("delete")], topic: "settings", reveal: open("cache") },
  { id: "settings:concurrency", label: msg("Concurrent requests"), description: msg("How many downloads run at once."),
    keywords: [msg("network"), msg("parallel"), msg("download")], topic: "settings", reveal: open("network") },
  { id: "settings:timeout", label: msg("Request timeout"), description: msg("How long one download may take before it is abandoned."),
    keywords: [msg("network"), msg("seconds"), msg("download")], topic: "settings", reveal: open("network") },
  { id: "settings:close", label: msg("Close the settings"), description: msg("Close the Settings dialog."),
    keywords: [msg("done")], topic: "settings", reveal: ["settings:"] },
];

export default features;
