import { msg } from "../../i18n";
import type { Feature } from "../features";

const description = msg("Download complete public ORR certificates, ratings and offshore/short-course polars from RegattaMan. Repeated downloads update existing certificates without duplicates.");
const settings = { topic: "settings", reveal: ["settings:", "settings:orr"] };
const features: Feature[] = [
  { id: "orc:catalogue", label: msg("Polar catalogue"), description: msg("Choose the ORC or ORR catalogue."), keywords: ["ORC", "ORR"], topic: "orc", reveal: ["section:orc"] },
  { id: "settings:orr-year", label: msg("Certificate year"), description, keywords: ["ORR", msg("polars")], ...settings },
  { id: "settings:orr-scrape", label: msg("Scrape ORR polars"), description, keywords: ["ORR", msg("download")], ...settings },
  { id: "settings:orr-cancel", label: msg("Cancel download"), description: msg("Download cancelled. The previous catalogue is unchanged."), keywords: ["ORR", msg("cancel")], ...settings },
  { id: "settings:orr-details", label: msg("Download details"), description, keywords: ["ORR", msg("download")], ...settings, landing: "settings:orr-scrape" },
];
export default features;
