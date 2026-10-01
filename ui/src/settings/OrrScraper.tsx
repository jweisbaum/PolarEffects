import { useEffect, useState } from "react";

import { describeError } from "../errors";
import type { OrrCatalogueInfo } from "../generated/OrrCatalogueInfo";
import type { OrrProgress } from "../generated/OrrProgress";
import { useT } from "../i18n";
import { api } from "../ipc";

/** An explicit, cancellable catalogue refresh; opening Settings never fetches. */
export default function OrrScraper() {
  const t = useT();
  const [year, setYear] = useState(String(new Date().getUTCFullYear()));
  const [status, setStatus] = useState<OrrProgress | null>(null);
  const [info, setInfo] = useState<OrrCatalogueInfo | null>(null);
  const [failure, setFailure] = useState<unknown>(null);
  const [starting, setStarting] = useState(false);
  const running = status?.running ?? false;
  useEffect(() => {
    let active = true;
    const refresh = () => { void api.orrScrapeStatus().then((next) => { if (active) setStatus(next); }).catch((error) => { if (active) setFailure(error); }); };
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => { active = false; clearInterval(timer); };
  }, []);
  useEffect(() => {
    let active = true;
    void api.orrCatalogueInfo().then((next) => { if (active) setInfo(next); }).catch((error) => { if (active) setFailure(error); });
    return () => { active = false; };
  }, [running]);
  const parsed = Number(year);
  const valid = Number.isInteger(parsed) && parsed >= 2018 && parsed <= new Date().getUTCFullYear();
  const start = async () => {
    if (!valid) return;
    setStarting(true); setFailure(null);
    try { setStatus(await api.startOrrScrape(parsed)); }
    catch (error) { setFailure(error); }
    finally { setStarting(false); }
  };
  return <section className="orr-scraper" data-section="settings:orr">
    <h3>{t("ORR polars")}</h3>
    <p className="muted">{t("Download complete public ORR certificates, ratings and offshore/short-course polars from RegattaMan. Repeated downloads update existing certificates without duplicates.")}</p>
    {info && <p>{t("{count} local polar variants", { count: info.records })}</p>}
    <label className="settings-field">
      {t("Certificate year")}
      <input type="number" min={2018} max={new Date().getUTCFullYear()} step={1} data-feature="settings:orr-year"
        value={year} disabled={running || starting} onChange={(event) => setYear(event.target.value)} />
    </label>
    <div className="settings-buttons">
      <button data-feature="settings:orr-scrape" disabled={!valid || running || starting} onClick={() => void start()}>{t("Scrape ORR polars")}</button>
      <button data-feature="settings:orr-cancel" disabled={!running} onClick={() => void api.cancelOrrScrape().catch(setFailure)}>{t("Cancel download")}</button>
    </div>
    {status && <div role="status">
      {running ? <>
        <progress max={Math.max(1, status.total)} value={status.done} />
        <p>{status.total === 0 ? t("Reading the ORR certificate list…") : t("Certificates processed: {done} of {total}", { done: status.done, total: status.total })}</p>
      </> : status.cancelled ? <p>{t("Download cancelled. The previous catalogue is unchanged.")}</p>
        : status.total > 0 && <p>{t("Added {added}, updated {updated}, skipped {failed}.", { added: status.added, updated: status.updated, failed: status.failed })}</p>}
      {(status.error || status.failures.length > 0) && <details>
        <summary data-feature="settings:orr-details">{t("Download details")}</summary>
        {status.error && <p className="modal-error">{status.error}</p>}
        {status.failures.map((text, i) => <p key={i} className="muted">{text}</p>)}
      </details>}
    </div>}
    {failure !== null && <p className="modal-error" role="alert" title={describeError(failure).detail}>{describeError(failure).text}</p>}
  </section>;
}
