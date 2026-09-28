import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The world map stage (spec.md 9.1). */
const map = ["stage:map"];

const features: Feature[] = [
  { id: "map:projection", label: msg("Projection"), description: msg("Draw the map flat or as a globe."),
    keywords: [msg("globe"), msg("orthographic"), msg("equirectangular"), msg("flat map")], topic: "map", reveal: map },
  { id: "map:fit", label: msg("Fit the world"), description: msg("Show the whole world again."),
    keywords: [msg("zoom out"), msg("reset view"), msg("whole world")], topic: "map", reveal: map },
];

export default features;
