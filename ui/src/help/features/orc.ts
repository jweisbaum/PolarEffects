import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The ORC polars section of the left navigation (spec.md 5). */
const section = ["section:orc"];
const results = ["section:orc", "orc:results"];

const features: Feature[] = [
  { id: "orc:search", label: msg("Search the ORC catalogue"),
    description: msg("Find certificates by name, sail number, country, model, builder, designer or year."),
    keywords: [msg("certificate"), msg("sail number"), msg("boat"), msg("sister ship"), "VPP"], topic: "orc", reveal: section },
  { id: "orc:year-from", label: msg("Built from"),
    description: msg("Only boats built in or after this year."),
    keywords: [msg("filter"), msg("year"), msg("age")], topic: "orc", reveal: section },
  { id: "orc:year-to", label: msg("Built until"),
    description: msg("Only boats built in or before this year."),
    keywords: [msg("filter"), msg("year"), msg("age")], topic: "orc", reveal: section },
  { id: "orc:country", label: msg("Country"),
    description: msg("Only certificates from one country."),
    keywords: [msg("filter"), msg("nation")], topic: "orc", reveal: section },
  { id: "orc:add", label: msg("Add ORC polar"),
    description: msg("Add a certificate to the project as a source, with the next colour."),
    keywords: [msg("add"), msg("certificate"), "VPP"], topic: "orc", reveal: results },
  { id: "orc:remove", label: msg("Remove ORC polar"),
    description: msg("Remove an ORC polar from the project. Undo puts it back."),
    keywords: [msg("delete"), msg("remove")], topic: "orc", reveal: section },
];

export default features;
