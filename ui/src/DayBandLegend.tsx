import { DAY_BANDS } from "./dayBand";
import { useT } from "./i18n";

/**
 * The four bands of the local solar day and their hours, each beside its
 * colour: the legend of "by time of day" in the 3D view and the 2D plot
 * (spec.md 9.2, 10.2).
 */
export default function DayBandLegend({ className }: { className: string }) {
  const t = useT();
  return (
    <ul className={`day-bands ${className}`}
      title={t("Local solar time where each sample was sailed: UTC shifted by its longitude, one hour for every 15°")}>
      {DAY_BANDS.map((band) => (
        <li key={band.id}><span className="day-band-swatch" style={{ background: band.colour }} />{t(band.label)} {band.hours}</li>
      ))}
    </ul>
  );
}
