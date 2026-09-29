import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

import { describeError, reportFailure } from "../errors";
import type { GribExportStatus } from "../generated/GribExportStatus";
import type { GribPreview } from "../generated/GribPreview";
import { setHint } from "../hint";
import { useT } from "../i18n";
import { api, GRIB_PROGRESS } from "../ipc";
import { pickGribPath } from "../project/dialogs";
import { formatBytes } from "./trackImport";

type Interval = "hourly" | "three_hourly";

/** A latitude as `52.75°N`. */
export function formatLat(lat: number): string {
  return `${Math.abs(lat).toFixed(2)}°${lat < 0 ? "S" : "N"}`;
}

/** A longitude in [−180, 180) as `7.00°W`. */
export function formatLon(lon: number): string {
  return `${Math.abs(lon).toFixed(2)}°${lon < 0 ? "W" : "E"}`;
}

/** Epoch seconds as `2020-07-27 11:00 UTC`. */
export function formatUtc(seconds: number): string {
  return `${new Date(seconds * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC`;
}

/**
 * Export reanalysis GRIB… on a track (spec.md 7.8): the 10 m wind over the
 * track's area (its bounding box plus 2°, on ERA5's 0.25° grid) at every
 * hour or every third hour from its first fix to its last, with its waves
 * and current if ticked, written to a `.grib2` file.
 *
 * It shows the area, the times, the file's size and what each part will
 * download (only the blocks of each hour's field holding the area's rows;
 * nothing is kept on disk, D27) before anything is fetched. Save… asks
 * where, then the export runs in the background with its progress here;
 * Cancel export stops it and writes nothing. Its three choices are tagged
 * and registered, landing on the track's Export reanalysis GRIB… button;
 * its answer buttons are not, as in every transient dialog (spec.md 3.6).
 */
export default function GribExportDialog({ sourceId, label, onClose }: {
  sourceId: number;
  label: string;
  onClose: () => void;
}) {
  const t = useT();
  const [waves, setWaves] = useState(true);
  const [current, setCurrent] = useState(false);
  const [interval, chooseInterval] = useState<Interval>("hourly");
  const [preview, setPreview] = useState<GribPreview | null>(null);
  const [status, setStatus] = useState<GribExportStatus | null>(null);
  const [error, setError] = useState<unknown>(null);
  const running = status?.state === "running";
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    let live = true;
    setError(null);
    api.gribPreview(sourceId, interval, waves, current)
      .then((next) => { if (live) setPreview(next); })
      .catch((failure) => { if (live) { setPreview(null); setError(failure); } });
    return () => { live = false; };
  }, [sourceId, interval, waves, current]);

  // The export's progress, and its end.
  useEffect(() => {
    const off = listen<GribExportStatus>(GRIB_PROGRESS, (event) => {
      const next = event.payload;
      if (next.source_id !== sourceId) return;
      setStatus(next);
      if (next.state === "done") {
        setHint(next.empty_messages > 0
          ? t("Exported the reanalysis to {path}; {count} of its fields had no data (hours the archives do not have).", {
            path: next.path ?? "", count: next.empty_messages })
          : t("Exported the reanalysis to {path}", { path: next.path ?? "" }));
        close.current();
      }
    });
    return () => { void off.then((stop) => stop()); };
  }, [sourceId, t]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !running) { event.preventDefault(); onClose(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, running]);

  const save = async () => {
    const path = await pickGribPath(label);
    if (path === null) return;
    try {
      setError(null);
      setStatus(await api.startGribExport(sourceId, path, interval, waves, current));
    } catch (failure) {
      reportFailure(failure);
      setError(failure);
    }
  };

  const cancel = () => { api.cancelGribExport().catch(reportFailure); };

  const download = preview === null ? 0
    : preview.wind_bytes + (waves ? preview.waves_bytes : 0) + (current ? preview.current_bytes : 0);
  const title = t("Export reanalysis GRIB");
  return (
    <div className="modal-backdrop" onClick={() => { if (!running) onClose(); }}>
      <div className="modal modal-narrow grib-export" role="dialog" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <h2>{title}</h2>
        <p className="modal-summary">
          {preview === null
            ? t("The 10 m wind along {track}, from ERA5.", { track: label })
            : t("The 10 m wind along {track}, from ERA5: {times} times from {first} to {last}.", {
              track: label, times: preview.times, first: formatUtc(preview.first), last: formatUtc(preview.last) })}
        </p>
        {preview !== null && (
          <p className="muted grib-area">
            {t("Area: {south} to {north}, {west} to {east}; {columns} × {rows} points every 0.25°.", {
              south: formatLat(preview.south), north: formatLat(preview.north),
              west: formatLon(preview.west), east: formatLon(preview.east),
              columns: preview.ni, rows: preview.nj })}
          </p>
        )}
        <fieldset className="grib-parts" disabled={running}>
          <legend>{t("Also include")}</legend>
          <label title={t("Significant wave height and mean wave direction from ERA5; missing over land")}>
            <input type="checkbox" data-feature="grib:waves" checked={waves} onChange={(e) => setWaves(e.target.checked)} />
            {preview === null
              ? t("Wave height and direction")
              : t("Wave height and direction (about {size} to download)", { size: formatBytes(preview.waves_bytes) })}
          </label>
          <label title={t("Surface current from the same sources as a track's weather, read onto the 0.25° points; missing over land")}>
            <input type="checkbox" data-feature="grib:current" checked={current} onChange={(e) => setCurrent(e.target.checked)} />
            {preview === null
              ? t("Current")
              : t("Current (about {size} to download)", { size: formatBytes(preview.current_bytes) })}
          </label>
        </fieldset>
        <fieldset className="grib-interval" disabled={running}>
          <legend>{t("Times")}</legend>
          <label>
            <input type="radio" name="grib-interval" data-feature="grib:hourly" checked={interval === "hourly"}
              onChange={() => chooseInterval("hourly")} />
            {t("Every hour")}
          </label>
          <label>
            <input type="radio" name="grib-interval" data-feature="grib:three-hourly" checked={interval === "three_hourly"}
              onChange={() => chooseInterval("three_hourly")} />
            {t("Every 3 hours: a third of the file and of the wind and wave download")}
          </label>
        </fieldset>
        <p className="muted" role="status">
          {preview === null
            ? t("Calculating the download…")
            : t("About {download} to download; the file is about {size}.", {
              download: formatBytes(download), size: formatBytes(preview.file_bytes) })}
        </p>
        {preview !== null && preview.spool_bytes > preview.spool_warning_bytes && (
          <p className="modal-error" role="alert">
            {t("The current needs about {size} of free disk space beside the file while it is exported.", { size: formatBytes(preview.spool_bytes) })}
          </p>
        )}
        {preview !== null && preview.cached_bytes > 0 && (
          <p className="muted">{t("{size} of it was already downloaded this session.", { size: formatBytes(preview.cached_bytes) })}</p>
        )}
        {running && status !== null && (
          <div className="tracker-progress" role="status">
            <p className="muted">
              {t("Exporting… {percent}%, {size} downloaded", {
                percent: Math.round(status.fraction * 100), size: formatBytes(status.downloaded_bytes) })}
            </p>
            <div className="progress-bar">
              <div className="progress-fill" style={{ width: `${Math.round(status.fraction * 100)}%` }} />
            </div>
          </div>
        )}
        {status?.state === "cancelled" && (
          <p className="muted" role="status">{t("Export cancelled; nothing was written.")}</p>
        )}
        {status?.state === "failed" && (
          <p className="modal-error" role="alert" title={status.message ?? ""}>
            {t("The export failed and nothing was written: {reason}", { reason: status.message ?? "" })}
          </p>
        )}
        {status?.warning && <p className="muted">{t("A current source was left out: {reason}", { reason: status.warning })}</p>}
        {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}
        <p className="muted">{t("Nothing downloaded is kept on disk; only the file you choose is written.")}</p>
        <div className="modal-actions">
          {running
            ? <button onClick={cancel} title={t("Stop the export; nothing is written")}>{t("Cancel export")}</button>
            : <button onClick={onClose} title={t("Close without exporting")}>{t("Close")}</button>}
          <span className="spacer" />
          <button className="primary" disabled={running || preview === null || preview.times === 0}
            title={t("Choose where to save the file, then export")} onClick={() => void save()}>
            {t("Save…")}
          </button>
        </div>
      </div>
    </div>
  );
}
