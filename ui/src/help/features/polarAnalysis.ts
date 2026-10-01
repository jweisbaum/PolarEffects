import { msg } from "../../i18n";
import type { Feature } from "../features";

const settings = { topic: "blend", reveal: ["section:sources"], landing: "sources:blend-settings" };
const groups = { topic: "polar-3d", reveal: ["stage:3d"], landing: "view3d:priority-filters" };
const groupDescription = msg("Priority groups are tried in order for each TWA/TWS cell. The first group with enough samples supplies that cell.");
const keywords = [msg("samples"), msg("filters"), "TWA", "TWS"];

const features: Feature[] = [
  { id: "shell:asymmetric", label: msg("Asymmetric polar"), description: msg("Keep port and starboard separate in plots, blending and export."), keywords: [msg("polar"), "360°"], topic: "blend" },
  { id: "blend-settings:interpolation", label: msg("Interpolation"), description: msg("Choose linear or shape-preserving monotone spline interpolation for the blend, plots and export."), keywords: [msg("Linear"), msg("Monotone spline")], ...settings },
  { id: "blend-settings:twa-step", label: msg("TWA step"), description: msg("Set the output wind-angle spacing to 1, 2, 5 or 10 degrees."), keywords: ["TWA", msg("output grid")], ...settings },
  { id: "blend-settings:tws-step", label: msg("TWS step"), description: msg("Set the output wind-speed spacing to 1, 2, 5 or 10 knots."), keywords: ["TWS", msg("output grid")], ...settings },
  { id: "sources:blend-edit", label: msg("Correct blend"), description: msg("Corrections apply after blending. Reset a cell to restore its calculated value."), keywords: [msg("blend"), msg("edit")], topic: "blend", reveal: ["section:sources"] },
  { id: "view3d:priority-filters", label: msg("Priority filter groups"), description: groupDescription, keywords, topic: "polar-3d", reveal: ["stage:3d"] },
  { id: "priority:minimum", label: msg("Minimum samples per group"), description: groupDescription, keywords, ...groups },
  { id: "priority:group", label: msg("Edit a priority group"), description: groupDescription, keywords, ...groups },
  { id: "priority:up", label: msg("Move up"), description: groupDescription, keywords, ...groups },
  { id: "priority:down", label: msg("Move down"), description: groupDescription, keywords, ...groups },
  { id: "priority:remove", label: msg("Remove"), description: groupDescription, keywords, ...groups },
  { id: "priority:add", label: msg("Add priority group"), description: groupDescription, keywords, ...groups },
];

export default features;
