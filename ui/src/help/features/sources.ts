import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The source list in the right panel (spec.md 8). */
const list = ["section:sources"];
const picker = ["section:sources", "sources:colours"];

const features: Feature[] = [
  { id: "sources:blend-colour", label: msg("Blend colour"),
    description: msg("Click the Blend entry's swatch to change the colour the blend is drawn in."),
    keywords: [msg("blend"), msg("colour"), msg("color")], topic: "blend", reveal: list },
  { id: "sources:blend-visible", label: msg("Show the blend"),
    description: msg("Show or hide the blend in every plot. Export is not affected."),
    keywords: [msg("blend"), msg("visibility")], topic: "blend", reveal: list },
  { id: "sources:blend-settings", label: msg("Blend settings"),
    description: msg("The output grid and how sources are blended."),
    keywords: [msg("output grid"), msg("blend")], topic: "blend", reveal: list },
  { id: "sources:export", label: msg("Export the polar"),
    description: msg("Write the blend as an Expedition, Adrena or CSV polar."),
    keywords: [msg("export"), msg("save"), "Expedition", "Adrena", "CSV"], topic: "blend", reveal: list },
  { id: "sources:reorder", label: msg("Reorder sources"),
    description: msg("Drag a source by its handle to move it in the list, or use the arrow keys."),
    keywords: [msg("drag"), msg("order"), msg("move")], topic: "sources", reveal: list },
  { id: "sources:colour", label: msg("Source colour"),
    description: msg("Click the swatch to give a source another colour."),
    keywords: [msg("colour"), msg("color"), msg("swatch")], topic: "sources", reveal: list },
  { id: "sources:palette", label: msg("Colour palette"),
    description: msg("The sixteen colours new sources take, in order."),
    keywords: [msg("colour"), msg("color"), msg("palette")], topic: "sources", reveal: picker },
  { id: "sources:custom-colour", label: msg("Custom colour"),
    description: msg("Pick any colour for a source with the system colour picker."),
    keywords: [msg("colour"), msg("color"), msg("picker")], topic: "sources", reveal: picker },
  { id: "sources:visible", label: msg("Show or hide a source"),
    description: msg("A hidden source is left out of the blend and of every plot."),
    keywords: [msg("visibility"), msg("hide"), msg("exclude")], topic: "sources", reveal: list },
  { id: "sources:rename", label: msg("Rename source"),
    description: msg("Click a source's name to rename it in place."),
    keywords: [msg("rename"), msg("label")], topic: "sources", reveal: list },
  { id: "sources:weight", label: msg("Source weight"),
    description: msg("How much a source counts in the blend, from 0 to 2."),
    keywords: [msg("weight"), msg("blend"), msg("slider")], topic: "sources", reveal: list },
  { id: "sources:edit", label: msg("Edit source"),
    description: msg("Open one source in the 3D view to edit its polar, with its table."),
    keywords: [msg("edit"), "3D"], topic: "sources", reveal: list },
  { id: "sources:compare", label: msg("Compare source"),
    description: msg("Compare one source with another or with the blend. Arrives in a later version."),
    keywords: [msg("difference"), msg("compare")], topic: "sources", reveal: list },
  { id: "sources:remove", label: msg("Remove source"),
    description: msg("Remove a source from the project. Undo puts it back."),
    keywords: [msg("delete"), msg("remove")], topic: "sources", reveal: list },
];

export default features;
