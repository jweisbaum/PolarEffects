import type { AppSettings } from "../generated/AppSettings";
import type { ScrapeSchedule } from "../generated/ScrapeSchedule";
import { useT } from "../i18n";
import { api } from "../ipc";

/**
 * When a certificate catalogue is downloaded without being asked for by
 * hand (spec.md 5.4): never, when the application starts, or when it quits.
 * Saved through its own command, as every setting is.
 */
export default function CatalogueSchedule({ catalogue, feature, settings, onSettings, onError }: {
  catalogue: "orc" | "orr";
  /** The control's `data-feature` id. */
  feature: string;
  settings: AppSettings;
  onSettings: (settings: AppSettings) => void;
  onError: (error: unknown) => void;
}) {
  const t = useT();
  const value = catalogue === "orc" ? settings.catalogues.orc_schedule : settings.catalogues.orr_schedule;
  return <>
    <label className="settings-field">
      {t("Download automatically")}
      <select data-feature={feature} value={value}
        onChange={(event) => void api.setCatalogueSchedule(catalogue, event.target.value as ScrapeSchedule).then(onSettings).catch(onError)}>
        <option value="on_demand">{t("Manually only")}</option>
        <option value="startup">{t("On startup")}</option>
        <option value="shutdown">{t("On shutdown")}</option>
      </select>
    </label>
    <p className="muted">{t("An automatic download is skipped when this catalogue was downloaded less than a day ago. On shutdown, quitting waits for the download; quit again to stop it and quit at once.")}</p>
  </>;
}
