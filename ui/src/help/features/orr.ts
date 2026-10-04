import { msg } from "../../i18n";
import type { Feature } from "../features";

const description = msg("Download complete public ORR certificates, ratings and offshore/short-course polars from RegattaMan. Repeated downloads update existing certificates without duplicates.");
const settings = { topic: "settings", reveal: ["settings:", "settings:orr"] };
const orcDescription = msg("Download every country's valid ORC certificates of the current year from ORC's own service, data.orc.org: about 60 MB, in a minute or two. A certificate the catalogue already holds is updated, never stored twice.");
const orcSettings = { topic: "settings", reveal: ["settings:", "settings:orc"] };
const features: Feature[] = [
  { id: "settings:orr-year", label: msg("Certificate year"), description, keywords: ["ORR", msg("polars")], ...settings },
  { id: "settings:orr-scrape", label: msg("Scrape ORR polars"), description, keywords: ["ORR", msg("download")], ...settings },
  { id: "settings:orr-cancel", label: msg("Cancel download"), description: msg("Download cancelled. The previous catalogue is unchanged."), keywords: ["ORR", msg("cancel")], ...settings },
  { id: "settings:orr-details", label: msg("Download details"), description, keywords: ["ORR", msg("download")], ...settings, landing: "settings:orr-scrape" },
  { id: "settings:orr-schedule", label: msg("Download automatically"), description: msg("Choose when the ORR catalogue is downloaded by itself: never, when PolarExplorer starts, or when it quits."), keywords: ["ORR", msg("schedule"), msg("automatic"), msg("download")], ...settings },
  { id: "settings:orc-scrape", label: msg("Scrape ORC polars"), description: orcDescription, keywords: ["ORC", msg("download"), msg("polars")], ...orcSettings },
  { id: "settings:orc-cancel", label: msg("Cancel download"), description: msg("Download cancelled. The previous catalogue is unchanged."), keywords: ["ORC", msg("cancel")], ...orcSettings },
  { id: "settings:orc-details", label: msg("Download details"), description: orcDescription, keywords: ["ORC", msg("download")], ...orcSettings, landing: "settings:orc-scrape" },
  { id: "settings:orc-schedule", label: msg("Download automatically"), description: msg("Choose when the ORC catalogue is downloaded by itself: never, when PolarExplorer starts, or when it quits."), keywords: ["ORC", msg("schedule"), msg("automatic"), msg("download")], ...orcSettings },
];
export default features;
