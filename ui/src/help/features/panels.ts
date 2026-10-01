import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The left navigation's sections and the right panel's (spec.md 3.2). */
const left = ["panel:left"];
const right = ["panel:right"];

const features: Feature[] = [
  { id: "nav:orc", label: msg("ORC / ORR polars"), description: msg("Search polar catalogues and add certificates"),
    keywords: [msg("certificate"), "VPP", msg("catalogue"), msg("sister ship")], topic: "orc", reveal: left },
  { id: "nav:polar-files", label: msg("Polar files"), description: msg("Import Expedition and Adrena polars."),
    keywords: [msg("import"), "Expedition", "Adrena"], topic: "polar-files", reveal: left },
  { id: "nav:tracks", label: msg("Tracks"), description: msg("Import race tracks from trackers and files."),
    keywords: [msg("race"), msg("tracker"), "YellowBrick", "Geovoile", "GPS"], topic: "tracks", reveal: left },
  { id: "panel:sources", label: msg("Sources"), description: msg("Every source, with its colour, visibility and weight."),
    keywords: [msg("source list"), msg("weight"), msg("blend")], topic: "sources", reveal: right },
  { id: "panel:plot", label: msg("Polar plot"), description: msg("Boat speed against wind angle for one wind speed."),
    keywords: [msg("polar diagram"), "TWA", "TWS", "BSP"], topic: "sources", reveal: right },
];

export default features;
