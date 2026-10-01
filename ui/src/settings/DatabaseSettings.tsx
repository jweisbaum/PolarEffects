import { useEffect, useState } from "react";
import type { AppSettings } from "../generated/AppSettings";
import type { DatabaseSettings as Preferences } from "../generated/DatabaseSettings";
import type { DatabaseProgress } from "../generated/DatabaseProgress";
import { describeError } from "../errors";
import { useT } from "../i18n";
import { api } from "../ipc";
import { pickDatabaseExport, pickLibraryDirectory } from "../project/dialogs";

export default function DatabaseSettings({ settings, onSettings }: { settings: AppSettings; onSettings: (s: AppSettings) => void }) {
  const t = useT();
  const [draft, setDraft] = useState(settings.database);
  const [status, setStatus] = useState<DatabaseProgress | null>(null);
  const [connection, setConnection] = useState<"untested" | "testing" | "ok" | "failed">("untested");
  const [failure, setFailure] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  useEffect(() => setDraft(settings.database), [settings.database]);
  useEffect(() => {
    let active = true;
    const refresh = () => void api.databaseJobStatus().then(s => { if (active) setStatus(s); }).catch(e => { if (active) setFailure(e); });
    refresh(); const timer = setInterval(refresh, 1000);
    return () => { active = false; clearInterval(timer); };
  }, []);
  const disabled = busy || status?.running || connection === "testing";
  const change = (patch: Partial<Preferences>) => { setDraft(old => ({ ...old, ...patch })); setSaved(false); setConnection("untested"); };
  const persist = async () => { const next = await api.setDatabaseSettings(draft); onSettings(next); setSaved(true); };
  const action = async (task: () => Promise<void>) => { setBusy(true); setFailure(null); try { await task(); } catch (e) { setFailure(e); } finally { setBusy(false); } };
  const test = () => action(async () => { setConnection("testing"); try { await api.testDatabaseConnection(draft); setConnection("ok"); } catch (e) { setConnection("failed"); throw e; } });
  const start = (operation: "metadata" | "scrape" | "export") => action(async () => {
    const path = operation === "export" ? await pickDatabaseExport() : null;
    if (operation === "export" && !path) return;
    await persist(); setStatus(await api.startDatabaseJob(operation, path));
  });
  const browse = (field: "geojson_directory" | "metadata_directory") => action(async () => { const path = await pickLibraryDirectory(); if (path) change({ [field]: path }); });
  return <section className="database-settings" data-section="settings:database">
    <h3>{t("PostgreSQL track library")}</h3>
    <label className="settings-field">{t("Database host")}<input data-feature="settings:db-host" title={t("Database host")} value={draft.host} disabled={disabled} onChange={e => change({ host: e.target.value })} /></label>
    <label className="settings-field">{t("Database port")}<input data-feature="settings:db-port" title={t("Database port")} type="number" min={1} max={65535} value={draft.port} disabled={disabled} onChange={e => change({ port: Number(e.target.value) })} /></label>
    <label className="settings-field">{t("Database name")}<input data-feature="settings:db-name" title={t("Database name")} value={draft.name} disabled={disabled} onChange={e => change({ name: e.target.value })} /></label>
    <label className="settings-field">{t("Database user")}<input data-feature="settings:db-user" title={t("Database user")} value={draft.user} disabled={disabled} onChange={e => change({ user: e.target.value })} /></label>
    <label className="settings-field">{t("Database password")}<input data-feature="settings:db-password" title={t("Database password")} type="password" autoComplete="off" value={draft.password} disabled={disabled} onChange={e => change({ password: e.target.value })} /></label>
    <label className="settings-field">{t("Verify TLS certificate")}<input data-feature="settings:db-tls" title={t("Verify TLS certificate")} type="checkbox" checked={draft.tls} disabled={disabled} onChange={e => change({ tls: e.target.checked })} /></label>
    <div className="settings-buttons"><button data-feature="settings:db-test" title={t("Test the connection and required database tables")} disabled={disabled} onClick={() => void test()}>{t("Test connection")}</button>
      <span role="status" className={`database-connection ${connection}`}>{connection === "ok" ? t("Connection successful") : connection === "failed" ? t("Connection failed") : connection === "testing" ? t("Testing connection…") : t("Connection not tested")}</span></div>
    <label className="settings-field">{t("GeoJSON track directory")}<input data-feature="settings:db-geojson" title={t("GeoJSON track directory")} value={draft.geojson_directory} disabled={disabled} onChange={e => change({ geojson_directory: e.target.value })} /></label>
    <button data-feature="settings:db-geojson-browse" title={t("Choose GeoJSON track directory")} disabled={disabled} onClick={() => void browse("geojson_directory")}>{t("Choose GeoJSON track directory")}</button>
    <label className="settings-field">{t("Boat metadata directory")}<input data-feature="settings:db-metadata" title={t("Leave empty to use the application data directory")} placeholder={t("Application data directory")} value={draft.metadata_directory} disabled={disabled} onChange={e => change({ metadata_directory: e.target.value })} /></label>
    <button data-feature="settings:db-metadata-browse" title={t("Choose boat metadata directory")} disabled={disabled} onClick={() => void browse("metadata_directory")}>{t("Choose boat metadata directory")}</button>
    <p className="muted">{t("Local boat searches include only YellowBrick, Geovoile, Blue Water, old Geovoile, Regadata and America's Cup records.")}</p>
    <div className="settings-buttons"><button data-feature="settings:db-download" title={t("Download boat metadata")} disabled={disabled} onClick={() => void start("metadata")}>{t("Download boat metadata")}</button></div>
    <label className="settings-field">{t("Run track scraper")}<select data-feature="settings:db-schedule" title={t("Run track scraper")} disabled={disabled} value={draft.scrape_schedule} onChange={e => change({ scrape_schedule: e.target.value as Preferences["scrape_schedule"] })}>
      <option value="on_demand">{t("Only on demand")}</option><option value="startup">{t("On startup")}</option><option value="shutdown">{t("On shutdown")}</option>
    </select></label>
    <p className="muted">{t("Only finished races are scraped. Ongoing, future and unverified races are skipped.")}</p>
    <label className="settings-field">{t("YellowBrick user key")}<input data-feature="settings:yb-user-key" title={t("YellowBrick user key")} type="password" autoComplete="off" value={draft.yellowbrick_user_key} disabled={disabled} onChange={e => change({ yellowbrick_user_key: e.target.value })} /></label>
    <label className="settings-field">{t("YellowBrick device ID (UDID)")}<input data-feature="settings:yb-device-id" title={t("YellowBrick device ID (UDID)")} type="password" autoComplete="off" value={draft.yellowbrick_device_id} disabled={disabled} onChange={e => change({ yellowbrick_device_id: e.target.value })} /></label>
    <p className="muted">{t("YellowBrick credentials resolve race codes from the public catalogue. Only races listed as free are associated with the account. Leave both fields empty to use known database URLs.")}</p>
    <label className="database-urls">{t("Race URLs (optional, one per line)")}<textarea data-feature="settings:db-urls" title={t("Leave empty to discover races and use existing database URLs")} value={draft.scrape_urls} disabled={disabled} onChange={e => change({ scrape_urls: e.target.value })} /></label>
    <p className="muted">{t("Scraping saves tracks, related database records and searchable metadata. Completed races with existing files are skipped during discovery. Explicit URLs refresh those races.")}</p>
    <p className="muted">{t("Shutdown scraping keeps the app open until it finishes or you cancel it.")}</p>
    <div className="settings-buttons"><button data-feature="settings:db-scrape" title={t("Scrape tracks now")} disabled={disabled} onClick={() => void start("scrape")}>{t("Scrape tracks now")}</button>
      <button data-feature="settings:db-cancel" title={t("Cancel database operation")} disabled={!status?.running} onClick={() => void api.cancelDatabaseJob().catch(setFailure)}>{t("Cancel database operation")}</button></div>
    <label className="settings-field">{t("pg_dump executable (optional)")}<input data-feature="settings:db-dump-tool" title={t("Full database export uses PostgreSQL client tools; leave empty to find them automatically")} value={draft.pg_dump} disabled={disabled} onChange={e => change({ pg_dump: e.target.value })} /></label>
    <p className="muted">{t("Export includes the entire database as SQL, without source ownership or access grants. The source password is not needed to restore it; destination server authentication still applies.")}</p>
    <div className="settings-buttons"><button data-feature="settings:db-export" title={t("Export entire database…")} disabled={disabled} onClick={() => void start("export")}>{t("Export entire database…")}</button>
      <button data-feature="settings:db-save" title={t("Save database settings")} disabled={disabled} onClick={() => void action(persist)}>{t("Save database settings")}</button>{saved && <span role="status">{t("Database settings saved")}</span>}</div>
    {status && (status.running || status.operation || status.error) && <div role="status">
      {status.running && <progress max={Math.max(1, status.total)} value={status.total ? status.done : undefined} />}
      <p>{status.cancelled ? t("Database operation cancelled") : status.running ? t("Database operation running…") : status.error ? t("Database operation failed") : t("Database operation finished")}</p>
      {status.current && <p className="database-current">{status.current}</p>}
      <p>{t("Tracks: {tracks}; skipped races: {skipped}; failures: {failed}", { tracks: status.tracks, skipped: status.skipped, failed: status.failed })}</p>
      {(status.error || status.failures.length > 0) && <details open><summary data-feature="settings:db-details">{t("Database operation details")}</summary>{status.error && <p className="modal-error">{status.error}</p>}{status.failures.map((s, i) => <p key={i}>{s}</p>)}</details>}
    </div>}
    {failure !== null && <p className="modal-error" role="alert">{describeError(failure).text}<br />{describeError(failure).detail}</p>}
  </section>;
}
