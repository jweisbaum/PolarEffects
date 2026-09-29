import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The ORC polars section of the left navigation (spec.md 5). */
const section = ["section:orc"];
const results = ["section:orc", "orc:results"];
/** The "Search by field" boxes: unfolded first. */
const byField = ["section:orc", "orc:fields"];

const features: Feature[] = [
  { id: "orc:search", label: msg("Search the ORC catalogue"),
    description: msg("Find certificates by name, sail number, country, model, builder, designer or year."),
    keywords: [msg("certificate"), msg("sail number"), msg("boat"), msg("sister ship"), "VPP"], topic: "orc", reveal: section },
  { id: "orc:fields", label: msg("Search by field"),
    description: msg("Show one search box per field: boat name, sail number, country, model, builder, designer, year built and certificate year."),
    keywords: [msg("field"), msg("advanced search"), msg("filter")], topic: "orc", reveal: section },
  { id: "orc:field-name", label: msg("Boat name"),
    description: msg("Only certificates whose boat name has words starting with these."),
    keywords: [msg("field"), msg("boat"), msg("name")], topic: "orc", reveal: byField },
  { id: "orc:field-sail", label: msg("Sail number"),
    description: msg("Only certificates with this sail number, with or without its country."),
    keywords: [msg("field"), msg("sail number")], topic: "orc", reveal: byField },
  { id: "orc:country", label: msg("Country"),
    description: msg("Only certificates from one country."),
    keywords: [msg("filter"), msg("nation")], topic: "orc", reveal: byField },
  { id: "orc:field-model", label: msg("Model / type"),
    description: msg("Only certificates whose model or type has words starting with these."),
    keywords: [msg("field"), msg("class"), msg("sister ship")], topic: "orc", reveal: byField },
  { id: "orc:field-builder", label: msg("Builder"),
    description: msg("Only certificates whose builder has words starting with these."),
    keywords: [msg("field"), msg("yard")], topic: "orc", reveal: byField },
  { id: "orc:field-designer", label: msg("Designer"),
    description: msg("Only certificates whose designer has words starting with these."),
    keywords: [msg("field"), msg("naval architect")], topic: "orc", reveal: byField },
  { id: "orc:year-from", label: msg("Built from"),
    description: msg("Only boats built in or after this year."),
    keywords: [msg("filter"), msg("year"), msg("age")], topic: "orc", reveal: byField },
  { id: "orc:year-to", label: msg("Built until"),
    description: msg("Only boats built in or before this year."),
    keywords: [msg("filter"), msg("year"), msg("age")], topic: "orc", reveal: byField },
  { id: "orc:field-certificate-year", label: msg("Certificate year"),
    description: msg("Only certificates of this year, or of a decade by its first three digits."),
    keywords: [msg("field"), msg("year"), msg("certificate")], topic: "orc", reveal: byField },
  { id: "orc:fields-clear", label: msg("Clear the field search"),
    description: msg("Empty every per-field box at once. The search box above is kept."),
    keywords: [msg("reset"), msg("field"), msg("clear")], topic: "orc", reveal: byField },
  { id: "orc:add", label: msg("Add ORC polar"),
    description: msg("Add a certificate to the project as a source, with the next colour."),
    keywords: [msg("add"), msg("certificate"), "VPP"], topic: "orc", reveal: results },
  { id: "orc:remove", label: msg("Remove ORC polar"),
    description: msg("Remove an ORC polar from the project. Undo puts it back."),
    keywords: [msg("delete"), msg("remove")], topic: "orc", reveal: section },
];

export default features;
