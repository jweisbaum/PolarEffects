import { useEffect, useState } from "react";
import { api } from "../ipc";
import { describeError } from "../errors";
import { useT } from "../i18n";
import type { BoatImportProgress } from "../generated/BoatImportProgress";
import type { TrackerProjectResult } from "../generated/TrackerProjectResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { BoatMatchMode } from "../generated/BoatMatchMode";
import BoatDetails from "./BoatDetails";

export default function TrackerProjectDialog({ discardUnsaved, onOpened, onClose }: {
  discardUnsaved: boolean; onOpened: (project: ProjectSummary) => void; onClose: () => void;
}) {
  const t = useT();
  const [tracker, setTracker] = useState("yellowbrick");
  const [url, setUrl] = useState("");
  const [matchMode, setMatchMode] = useState<BoatMatchMode>("identical_model");
  const [busy, setBusy] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [settling, setSettling] = useState(false);
  const [progress, setProgress] = useState<BoatImportProgress | null>(null);
  const [result, setResult] = useState<TrackerProjectResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    if (!busy) return;
    let live = true;
    const poll = () => void api.boatImportStatus().then(p => { if (live) setProgress(p); }).catch(() => undefined);
    poll(); const timer = window.setInterval(poll, 300);
    return () => { live = false; window.clearInterval(timer); };
  }, [busy]);
  const open = async () => {
    setBusy(true); setCancelling(false); setError(null);
    try { setResult(await api.openTrackerProject(tracker, url.trim(), discardUnsaved, matchMode)); }
    catch (e) { setError(e); }
    finally { setBusy(false); }
  };
  const close = async () => {
    if (busy || settling) return;
    setSettling(true); setError(null);
    try {
      if (result) await api.discardTrackerProject(result.project.id);
      onClose();
    } catch (e) { setError(e); }
    finally { setSettling(false); }
  };
  const confirm = async () => {
    if (!result || settling) return;
    setSettling(true); setError(null);
    try { onOpened(await api.confirmTrackerProject(result.project.id)); onClose(); }
    catch (e) { setError(e); }
    finally { setSettling(false); }
  };
  return <div className="modal-backdrop"><section className={`modal tracker-project${result ? " has-report" : ""}`} role="dialog" aria-modal="true" aria-label={t("Open project from tracker")}
    onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); void close(); } }}>
    <h2>{t("Open project from tracker")}</h2>
    {!result && <>
      <p>{t("Create one boat tab per entry, with its race track and matching local polars and historical tracks. Names alone never qualify a match.")}</p>
      <label>{t("Tracker")} <select data-feature="boats:tracker-provider" value={tracker} disabled={busy} onChange={e => setTracker(e.target.value)}>
        <option value="yellowbrick">{t("YellowBrick")}</option><option value="geovoile">{t("Geovoile")}</option><option value="bluewater">{t("Blue Water Tracks")}</option>
      </select></label>
      <label>{t("Race URL")} <input autoFocus data-feature="boats:tracker-url" value={url} disabled={busy} onChange={e => setUrl(e.target.value)} type="url" /></label>
      <label>{t("Match additional data")} <select data-feature="boats:tracker-match" value={matchMode} disabled={busy} onChange={e => setMatchMode(e.target.value as BoatMatchMode)}>
        <option value="identical_model">{t("Identical models")}</option><option value="exact_boat">{t("Exact boat only")}</option>
      </select></label>
      <p className="muted">{matchMode === "exact_boat"
        ? t("Exact boat requires matching MMSI or a sail number corroborated by builder and length. Conflicting details reject a match; the race track is always included.")
        : t("Include polars and historical tracks from other boats of the same verified model.")}</p>
      <p className="muted">{t("Historical tracks use the boat metadata and GeoJSON directories in Settings. Weather can be downloaded after import.")}</p>
    </>}
    {busy && <div role="status"><progress max={1} value={progress?.fraction ?? 0} />
      <p>{cancelling ? t("Cancelling…") : progress?.phase === "matching" ? t("Matching boat {done} of {total}: {boat}", { done: (progress?.done ?? 0) + 1, total: progress?.total ?? 0, boat: progress?.current ?? "" }) : t("Downloading tracker boats and tracks…")}</p>
    </div>}
    {result && <>
      <p>{t("Created {count} boat tabs.", { count: result.boats.length })}</p>
      <div className="boat-import-report"><table><thead><tr><th>{t("Boat")}</th><th>{t("Matched model")}</th><th>{t("Polars")}</th><th>{t("Tracks")}</th><th>{t("Unavailable tracks")}</th></tr></thead>
        <tbody>{result.boats.map((boat, i) => <tr key={i}><td><strong>{boat.boat}</strong><BoatDetails details={boat.details} /></td><td>{boat.model ?? t("No verified model")}</td><td>{boat.polars}</td><td>{boat.tracks}</td><td>{boat.missing_tracks}</td></tr>)}</tbody></table></div>
      {(result.warnings.length > 0 || result.boats.some(b => b.warnings.length > 0)) && <details data-feature="boats:tracker-warnings"><summary>{t("Import details")}</summary>
        <ul>{[...result.warnings, ...result.boats.flatMap(b => b.warnings.map(w => `${b.boat}: ${w}`))].map((w,i) => <li key={i}>{w}</li>)}</ul></details>}
    </>}
    {error !== null && <p role="alert" className="error" title={describeError(error).detail}>{describeError(error).text}</p>}
    <div className="modal-actions">
      {busy ? <button data-feature="boats:tracker-cancel" disabled={cancelling} onClick={() => { setCancelling(true); void api.cancelBoatImport().catch(setError); }}>{t("Cancel")}</button>
        : result ? <>
          <button autoFocus data-feature="boats:tracker-cancel" disabled={settling} onClick={() => void close()}>{t("Cancel")}</button>
          <button className="primary" data-feature="boats:tracker-close" disabled={settling} onClick={() => void confirm()}>{t("Open project")}</button>
        </> : <button data-feature="boats:tracker-close" disabled={settling} onClick={() => void close()}>{t("Cancel")}</button>}
      {!result && <button className="primary" data-feature="boats:tracker-open" disabled={busy || !url.trim()} onClick={() => void open()}>{t("Open tracker project")}</button>}
    </div>
  </section></div>;
}
