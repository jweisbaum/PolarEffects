import { useEffect, useState } from "react";
import type { AppSettings } from "../generated/AppSettings";
import type { LibrarySettings as Preferences } from "../generated/LibrarySettings";
import { describeError } from "../errors";
import { useT } from "../i18n";
import { api } from "../ipc";
import { scrapeStarted, useLibraryScrape } from "../library/scrape";
import { pickLibraryDirectory } from "../project/dialogs";

/**
 * The SYRF track library (asked 2026-10-04): the folders it is read from,
 * and scraping finished YellowBrick, Geovoile and Blue Water races into them
 * by hand or on a schedule. A scraped race is its track files and its
 * search records in the boat metadata; nothing goes to a database.
 */
export default function LibrarySettings({ settings, onSettings }: { settings: AppSettings; onSettings: (s: AppSettings) => void }) {
  const t = useT();
  const [draft, setDraft] = useState(settings.library);
  const [failure, setFailure] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  const status = useLibraryScrape();
  const running = status?.running === true;
  useEffect(() => setDraft(settings.library), [settings.library]);
  const change = (patch: Partial<Preferences>) => { setDraft(old => ({ ...old, ...patch })); setSaved(false); };
  const persist = async () => { onSettings(await api.setLibrarySettings(draft)); setSaved(true); };
  const action = async (task: () => Promise<void>) => { setFailure(null); try { await task(); } catch (e) { setFailure(e); } };
  const browse = (field: "geojson_directory" | "metadata_directory") => action(async () => {
    const path = await pickLibraryDirectory(); if (path) change({ [field]: path });
  });
  // Scraping uses the settings on screen: they are saved first.
  const scrape = () => action(async () => { await persist(); scrapeStarted(await api.startLibraryScrape()); });
  return <section className="library-settings" data-section="settings:library">
    <h3>{t("Track library")}</h3>
    <label className="settings-field">{t("GeoJSON track directory")}<input data-feature="settings:library-geojson" title={t("GeoJSON track directory")} value={draft.geojson_directory} disabled={running} onChange={e => change({ geojson_directory: e.target.value })} /></label>
    <button data-feature="settings:library-geojson-browse" title={t("Choose GeoJSON track directory")} disabled={running} onClick={() => void browse("geojson_directory")}>{t("Choose GeoJSON track directory")}</button>
    <label className="settings-field">{t("Boat metadata directory")}<input data-feature="settings:library-metadata" title={t("Leave empty to use the application data directory")} placeholder={t("Application data directory")} value={draft.metadata_directory} disabled={running} onChange={e => change({ metadata_directory: e.target.value })} /></label>
    <button data-feature="settings:library-metadata-browse" title={t("Choose boat metadata directory")} disabled={running} onClick={() => void browse("metadata_directory")}>{t("Choose boat metadata directory")}</button>
    <p className="muted">{t("Local boat searches include only YellowBrick, Geovoile, Blue Water, old Geovoile, Regadata and America's Cup records.")}</p>

    <label className="settings-field">{t("Run track scraper")}<select data-feature="settings:library-schedule" title={t("Run track scraper")} disabled={running} value={draft.scrape_schedule}
      onChange={e => change({ scrape_schedule: e.target.value as Preferences["scrape_schedule"] })}>
      <option value="on_demand">{t("Only on demand")}</option><option value="startup">{t("On startup")}</option><option value="shutdown">{t("On shutdown")}</option>
    </select></label>
    <p className="muted">{t("Only finished races are scraped. Ongoing, future and unverified races are skipped.")}</p>
    <label className="settings-field">{t("YellowBrick user key")}<input data-feature="settings:library-yb-user-key" title={t("YellowBrick user key")} type="password" autoComplete="off" value={draft.yellowbrick_user_key} disabled={running} onChange={e => change({ yellowbrick_user_key: e.target.value })} /></label>
    <label className="settings-field">{t("YellowBrick device ID (UDID)")}<input data-feature="settings:library-yb-device-id" title={t("YellowBrick device ID (UDID)")} type="password" autoComplete="off" value={draft.yellowbrick_device_id} disabled={running} onChange={e => change({ yellowbrick_device_id: e.target.value })} /></label>
    <p className="muted">{t("YellowBrick credentials list races from its catalogue; only races listed as free are associated with the account. Leave both empty to scrape the races the library already holds and those listed below.")}</p>
    <label className="library-urls">{t("Race URLs (optional, one per line)")}<textarea data-feature="settings:library-urls" title={t("Leave empty to discover races")} value={draft.scrape_urls} disabled={running} onChange={e => change({ scrape_urls: e.target.value })} /></label>
    <p className="muted">{t("Scraping saves each finished race's tracks in the GeoJSON directory and its boats in the boat metadata, so the Tracks panel finds them. Races already complete in the library are skipped unless listed above.")}</p>
    <p className="muted">{t("Shutdown scraping keeps the app open until it finishes or you cancel it.")}</p>

    <div className="settings-buttons">
      <button data-feature="settings:library-save" title={t("Save library settings")} disabled={running} onClick={() => void action(persist)}>{t("Save library settings")}</button>
      <button data-feature="settings:library-scrape" title={t("Scrape tracks now")} disabled={running} onClick={() => void scrape()}>{t("Scrape tracks now")}</button>
      <button data-feature="settings:library-cancel" title={t("Cancel the scrape; races already saved stay in the library")} disabled={!running} onClick={() => void api.cancelLibraryScrape().catch(setFailure)}>{t("Cancel scrape")}</button>
      {saved && <span role="status">{t("Library settings saved")}</span>}
    </div>
    {status && (status.running || status.done > 0 || status.error || status.cancelled) && <div role="status" className="library-scrape-status">
      {status.running && <progress max={Math.max(1, status.total)} value={status.total ? status.done : undefined} />}
      <p>{status.cancelled ? t("Scrape cancelled") : status.running ? t("Scraping races…") : status.error ? t("Scrape failed") : t("Scrape finished")}</p>
      {status.current && <p className="library-current">{status.current}</p>}
      <p>{t("Tracks: {tracks}; skipped races: {skipped}; failures: {failed}", { tracks: status.tracks, skipped: status.skipped, failed: status.failed })}</p>
      {(status.error || status.failures.length > 0) && <details><summary data-feature="settings:library-details">{t("Scrape details")}</summary>
        {status.error && <p className="modal-error">{status.error}</p>}{status.failures.map((s, i) => <p key={i}>{s}</p>)}</details>}
    </div>}
    {failure !== null && <p className="modal-error" role="alert">{describeError(failure).text}<br />{describeError(failure).detail}</p>}
  </section>;
}
