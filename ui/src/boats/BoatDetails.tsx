import { msg, useT } from "../i18n";

const FIELDS: Record<string, string> = {
  model: msg("Model"), boatmodel: msg("Model"), class: msg("Class"), boatclass: msg("Class"),
  type: msg("Type"), boattype: msg("Type"), builder: msg("Builder"), make: msg("Make"),
  length: msg("Length"), loa: msg("Length overall"), mmsi: msg("MMSI"),
  sailnumber: msg("Sail number"), sailno: msg("Sail number"), sail: msg("Sail number"),
  division: msg("Division"), trackerboatid: msg("Tracker boat ID"), trackerurl: msg("Original URL"),
  matchedmodel: msg("Matched model"),
};
const PRIMARY = ["model", "class", "type", "builder", "make", "loa", "length", "mmsi", "sailnumber", "sailno", "sail"];
const fieldKey = (key: string) => (key.split(".").at(-1) ?? key).toLowerCase().replace(/[^a-z0-9]/g, "").replace(/^(boat|vessel)/, "");

/** Preserve provider field labels for metadata whose vocabulary is not ours. */
export default function BoatDetails({ details }: { details: Record<string, string | undefined> }) {
  const t = useT();
  const fields = Object.entries(details).filter((entry): entry is [string, string] => Boolean(entry[1]?.trim()));
  if (!fields.length) return null;
  const seen = new Set<string>();
  const primary = fields.filter(([key, value]) => {
    if (!PRIMARY.includes(fieldKey(key))) return false;
    const identity = `${FIELDS[fieldKey(key)]}:${value.trim()}`;
    if (seen.has(identity)) return false;
    seen.add(identity); return true;
  })
    .sort(([a], [b]) => PRIMARY.indexOf(fieldKey(a)) - PRIMARY.indexOf(fieldKey(b)));
  const list = (values: [string, string][]) => <dl className="tracker-boat-details" aria-label={t("Boat details")}>
    {values.map(([key, value]) => {
      const label = FIELDS[fieldKey(key)];
      return <div key={key}><dt>{label ? t(label) : key}</dt><dd>{value}</dd></div>;
    })}
  </dl>;
  return <>{list(primary)}<details data-feature="boats:tracker-details"><summary>{t("All boat details")}</summary>{list(fields)}</details></>;
}
