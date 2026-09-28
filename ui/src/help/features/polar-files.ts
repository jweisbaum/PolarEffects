import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The Polar files section of the left navigation (spec.md 6). */
const section = ["section:polar-files"];

const features: Feature[] = [
  { id: "polar-files:import", label: msg("Import polar files"),
    description: msg("Import Expedition and Adrena polars, several files at once."),
    keywords: [msg("import"), "Expedition", "Adrena", msg("open"), ".txt", ".pol", ".csv"], topic: "polar-files", reveal: section },
  { id: "polar-files:remove", label: msg("Remove polar file"),
    description: msg("Remove an imported polar file from the project. Undo puts it back."),
    keywords: [msg("delete"), msg("remove")], topic: "polar-files", reveal: section },
];

export default features;
