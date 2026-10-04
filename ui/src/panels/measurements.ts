import { msg } from "../i18n";

export const MEASUREMENTS = [
  { id: "loa", label: msg("Length overall (m)"), minId: "loa-min", maxId: "loa-max" },
  { id: "beam", label: msg("Beam (m)"), minId: "beam-min", maxId: "beam-max" },
  { id: "draft", label: msg("Draft (m)"), minId: "draft-min", maxId: "draft-max" },
  { id: "displacement", label: msg("Displacement (kg)"), minId: "displacement-min", maxId: "displacement-max" },
  { id: "main", label: msg("Mainsail area (m\u00b2)"), minId: "main-min", maxId: "main-max" },
  { id: "genoa", label: msg("Genoa area (m\u00b2)"), minId: "genoa-min", maxId: "genoa-max" },
  { id: "spinnaker", label: msg("Spinnaker area (m\u00b2)"), minId: "spinnaker-min", maxId: "spinnaker-max" },
  { id: "asymmetric", label: msg("Asymmetric spinnaker area (m\u00b2)"), minId: "asymmetric-min", maxId: "asymmetric-max" },
];

/** SI bounds; empty is unconstrained. Reject malformed or negative input. */
export function measurementBounds(text: string[]): (number | null)[] | null {
  const values = text.map((v) => v.trim() === "" ? null : Number(v));
  return values.every((v) => v === null || (Number.isFinite(v) && v >= 0)) ? values : null;
}
