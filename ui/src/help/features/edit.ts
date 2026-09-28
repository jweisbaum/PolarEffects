import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * Edit mode in the 3D stage (spec.md 10.4). Its controls exist only while a
 * source is being edited, so each reveals the 3D stage and starts editing
 * the first source (the statistic, the first track).
 */
const editing = ["stage:3d", "edit:open"];
const topic = "polar-3d";

const features: Feature[] = [
  { id: "view3d:tool-drag", label: msg("Drag tool"),
    description: msg("Drag a node of the source being edited to change its boat speed; Shift snaps to 0.05 kn."),
    keywords: [msg("move"), msg("node"), "BSP"], topic, reveal: editing },
  { id: "edit:done", label: msg("Leave edit mode"), description: msg("Stop editing the source; its edits stay."),
    keywords: [msg("stop editing"), msg("close")], topic, reveal: editing },
  { id: "edit:hide-others", label: msg("Hide other sources"),
    description: msg("While a source is edited, hide the other sources instead of fading them."),
    keywords: [msg("fade"), msg("focus")], topic, reveal: editing },
  { id: "edit:statistic", label: msg("Segment statistic"),
    description: msg("How a track's polar segment sums up the boat speeds in each cell: 90th or 75th percentile, median or mean."),
    keywords: [msg("percentile"), msg("median"), msg("mean"), msg("polar segment")], topic,
    reveal: ["stage:3d", "edit:open-track"] },
  { id: "edit:scale-percent", label: msg("Scale percentage"),
    description: msg("The percentage the Scale tool applies to the selected cells."),
    keywords: [msg("percent"), msg("faster"), msg("slower")], topic, reveal: editing },
  { id: "edit:scale", label: msg("Scale selection"),
    description: msg("Make the selected cells faster or slower by a percentage."),
    keywords: [msg("percent"), msg("multiply")], topic, reveal: editing },
  { id: "edit:smooth", label: msg("Smooth selection"),
    description: msg("Smooth the selected cells over their neighbours on the grid."),
    keywords: [msg("average"), msg("kernel")], topic, reveal: editing },
  { id: "edit:reset", label: msg("Reset selection"),
    description: msg("Put the selected cells back to the source's own values."),
    keywords: [msg("undo edits"), msg("restore")], topic, reveal: editing },
  { id: "edit:reset-all", label: msg("Reset all edits"),
    description: msg("Clear every edit of the source being edited."),
    keywords: [msg("undo edits"), msg("restore"), msg("clear")], topic, reveal: editing },
  { id: "edit:table", label: msg("Polar table"),
    description: msg("The edited source's polar as a TWA × TWS table; typing a value edits that cell."),
    keywords: [msg("grid"), msg("type a value"), msg("cells")], topic, reveal: editing },
];

export default features;
