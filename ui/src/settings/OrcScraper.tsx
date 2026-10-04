import { useEffect, useState } from "react";

import { describeError } from "../errors";
import type { AppSettings } from "../generated/AppSettings";
import type { OrcCatalogueInfo } from "../generated/OrcCatalogueInfo";
import type { OrcProgress } from "../generated/OrcProgress";
import { useT } from "../i18n";
import { api } from "../ipc";
import CatalogueSchedule from "./CatalogueSchedule";

/** A day as `YYYY-MM-DD` (UTC): the same in every language. */
const day = (epochSeconds: number) => new Date(epochSeconds * 1000).toISOString().slice(0, 10);

/**
 * The ORC catalogue's download from ORC's own service (spec.md 5.4):
 * started and cancelled by hand, or by the schedule chosen here. Opening
 * Settings never fetches.
 */
export default function OrcScraper({ settings, onSettings }: {
  settings: AppSettings;
  onSettings: (settings: AppSettings) => void;
}) {
  const t = useT();
  const [status, setStatus] = useState<OrcProgress | null>(null);
  const [info, setInfo] = useState<OrcCatalogueInfo | null>(null);
  const [failure, setFailure] = useState<unknown>(null);
  const [starting, setStarting] = useState(false);
  const running = status?.running ?? false;
  useEffect(() => {
    let active = true;
    const refresh = () => { void api.orcScrapeStatus().then((next) => { if (active) setStatus(next); }).catch((error) => { if (active) setFailure(error); }); };
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => { active = false; clearInterval(timer); };
  }, []);
  useEffect(() => {
    let active = true;
    void api.orcCatalogueInfo().then((next) => { if (active) setInfo(next); }).catch((error) => { if (active) setFailure(error); });
    return () => { active = false; };
  }, [running]);
  const start = async () => {
    setStarting(true); setFailure(null);
    try { setStatus(await api.startOrcScrape()); }
    catch (error) { setFailure(error); }
    finally { setStarting(false); }
  };
  return <section className="orc-scraper" data-section="settings:orc">
    <h3>{t("ORC polars")}</h3>
    <p className="muted">{t("Download every country's valid ORC certificates of the current year from ORC's own service, data.orc.org: about 60 MB, in a minute or two. A certificate the catalogue already holds is updated, never stored twice.")}</p>
    {info && <p>
      {t("{count} certificates in the catalogue.", { count: info.records })}
      {info.scraped > 0 && info.scraped_at !== null && <> {t("{count} of them downloaded from ORC, last on {date}.", { count: info.scraped, date: day(info.scraped_at) })}</>}
    </p>}
    <div className="settings-buttons">
      <button data-feature="settings:orc-scrape" disabled={running || starting} onClick={() => void start()}>{t("Scrape ORC polars")}</button>
      <button data-feature="settings:orc-cancel" disabled={!running} onClick={() => void api.cancelOrcScrape().catch(setFailure)}>{t("Cancel download")}</button>
    </div>
    <CatalogueSchedule catalogue="orc" feature="settings:orc-schedule" settings={settings} onSettings={onSettings} onError={setFailure} />
    {status && <div role="status">
      {running ? <>
        <progress max={Math.max(1, status.total)} value={status.done} />
        <p>{status.total === 0 ? t("Reading the list of countries…")
          : t("Countries downloaded: {done} of {total} ({certificates} certificates)", { done: status.done, total: status.total, certificates: status.certificates })}</p>
      </> : status.cancelled ? <p>{t("Download cancelled. The previous catalogue is unchanged.")}</p>
        : status.total > 0 && <p>{t("Added {added}, updated {updated}, removed {removed} that ORC no longer lists, skipped {failed}.",
          { added: status.added, updated: status.updated, removed: status.removed, failed: status.failed })}</p>}
      {(status.error || status.failures.length > 0) && <details>
        <summary data-feature="settings:orc-details">{t("Download details")}</summary>
        {status.error && <p className="modal-error">{status.error}</p>}
        {status.failures.map((text, i) => <p key={i} className="muted">{text}</p>)}
      </details>}
    </div>}
    {failure !== null && <p className="modal-error" role="alert" title={describeError(failure).detail}>{describeError(failure).text}</p>}
  </section>;
}
