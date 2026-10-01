import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The 2D polar plot's own controls (spec.md 9.2): the source list and the
 * "Polar plot" section heading are registered in `panels.ts`. `overlay:plot`
 * opens the full-size view over the current stage (App), the same way `settings:`
 * opens Settings.
 */
const inPanel = ["section:plot"];
const inOverlay = ["overlay:plot"];

const features: Feature[] = [
  { id: "plot:all", label: msg("All wind speeds"), description: msg("Show every wind speed a source has, instead of one slice."),
    keywords: ["TWS", msg("all")], topic: "sources", reveal: inPanel },
  { id: "plot:tws", label: msg("Wind speed slice"), description: msg("The true wind speed the polar plot slices at."),
    keywords: ["TWS", msg("slider"), msg("wind speed")], topic: "sources", reveal: inPanel },
  { id: "plot:show-filtered", label: msg("Show filtered samples in the plot"), description: msg("Also draw the samples the filters take out, dimmed."),
    keywords: [msg("filters"), msg("dots"), msg("dimmed")], topic: "sources", reveal: inPanel },
  { id: "plot:full-size", label: msg("Full-size polar plot"), description: msg("Open the polar plot full size over the current view."),
    keywords: [msg("polar diagram"), msg("overlay"), msg("map")], topic: "sources", reveal: inPanel },
  { id: "plot:close", label: msg("Close the full-size polar plot"), description: msg("Close the full-size polar plot and return to the current view."),
    keywords: [msg("done"), msg("overlay")], topic: "sources", reveal: inOverlay },
];

export default features;
