import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";

import { describeError, reportFailure } from "../errors";
import type { TrackerBoatRow } from "../generated/TrackerBoatRow";
import type { TrackerEventView } from "../generated/TrackerEventView";
import type { TrackerProgress } from "../generated/TrackerProgress";
import type { TrackImportResult } from "../generated/TrackImportResult";
import { useT } from "../i18n";
import { api, TRACKER_PROGRESS } from "../ipc";
import { loadBasemap } from "../map/basemap";
import type { Basemap } from "../map/format";
import { mapColour } from "../settings/themes";
import { dateRange, formatBytes } from "./trackImport";
import { addressHint, boatStatusText, filterBoats, legAddress, NO_RETRY, TRACKER_NAMES, type TrackerId } from "./trackerImport";
import { coastPath, frameOf, linePath, lodFor, VIEW_H, VIEW_W } from "./trackerPreview";

type Phase =
  | { kind: "address" }
  | { kind: "downloading"; progress: TrackerProgress | null }
  | { kind: "failed"; text: string; detail: string; retry: boolean }
  | { kind: "event"; event: TrackerEventView };

/**
 * The shared tracker dialog (spec.md 7.2): paste the event's address, and
 * the app downloads **every** boat's full track (a job with progress and
 * Cancel, spec.md 7.7), then shows the event's title and dates, a table of
 * boats (name, sail number, model, division, positions, status) with a
 * search box and a map preview, and imports the ticked boats as one track
 * each, one undo. The event stays in memory for the session, so opening it
 * again downloads nothing.
 *
 * Its controls exist only once the dialog is open, so their registry
 * entries land on the tracker's button (`landing`); its answer buttons
 * (Cancel, Import) go untagged as in every transient dialog (spec.md 3.6).
 */
export default function TrackerImportDialog({ tracker, onDone, onCancel }: {
  tracker: TrackerId;
  onDone: (result: TrackImportResult) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [url, setUrl] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "address" });
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const downloading = phase.kind === "downloading";
  const live = useRef(true);
  useEffect(() => () => { live.current = false; }, []);

  const close = () => {
    if (downloading) void api.cancelTrackerEvent().catch(() => undefined);
    onCancel();
  };
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); closeRef.current(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Progress arrives as events while a download runs.
  useEffect(() => {
    if (!downloading) return;
    const pending = listen<TrackerProgress>(TRACKER_PROGRESS, (e) => {
      if (live.current) setPhase((p) => (p.kind === "downloading" ? { kind: "downloading", progress: e.payload } : p));
    }).catch(() => null);
    return () => { void pending.then((off) => off?.()); };
  }, [downloading]);

  const download = async (refresh: boolean, address = url) => {
    if (address.trim() === "") return;
    setPhase({ kind: "downloading", progress: null });
    try {
      const event = await api.trackerEvent(tracker, address.trim(), refresh);
      if (!live.current) return;
      setChosen(new Set());
      setQuery("");
      setPhase({ kind: "event", event });
    } catch (error) {
      if (!live.current) return;
      const kind = (error as { kind?: string }).kind ?? "";
      if (kind === "cancelled") { setPhase({ kind: "address" }); return; }
      const shown = describeError(error);
      setPhase({ kind: "failed", text: shown.text, detail: shown.detail, retry: !NO_RETRY.has(kind) });
    }
  };

  const run = async (event: TrackerEventView) => {
    setBusy(true);
    try {
      const ids = event.boats.filter((b) => chosen.has(b.id)).map((b) => b.id);
      onDone(await api.importTrackerBoats(event.tracker, event.key, ids));
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  /** Another leg of the event shown: its address, downloaded (or recalled) at once. */
  const openLeg = (event: TrackerEventView, leg: number) => {
    const address = legAddress(event.url, leg);
    setUrl(address);
    void download(false, address);
  };

  const title = t("Import from {tracker}", { tracker: TRACKER_NAMES[tracker] });
  const event = phase.kind === "event" ? phase.event : null;
  // A stray click on the backdrop must not throw away a download or an
  // import under way; Cancel download and Escape still end them.
  return (
    <div className="modal-backdrop" onClick={() => { if (!downloading && !busy) close(); }}>
      <div className="modal tracker-import" role="dialog" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <h2>{title}</h2>
        <form className="tracker-address" onSubmit={(e) => { e.preventDefault(); void download(false); }}>
          <label>
            {t("Event address")}
            <input data-feature="tracker-import:url" value={url} autoFocus disabled={downloading}
              placeholder={t("Paste the race's tracker link or key")}
              title={addressHint(tracker)}
              onChange={(e) => setUrl(e.target.value)} />
          </label>
          <button type="submit" data-feature="tracker-import:open" disabled={downloading || url.trim() === ""}
            title={t("Download every boat's full track; an event already downloaded this session opens at once")}>
            {t("Open")}
          </button>
        </form>
        {phase.kind === "downloading" && (
          <div className="tracker-progress" role="status">
            <p className="muted">
              {phase.progress?.fallback
                ? t("The positions did not load; reading the KML instead…")
                : t("Downloading every boat's track…")}
              {phase.progress !== null && phase.progress.bytes > 0 && ` ${formatBytes(phase.progress.bytes)}`}
            </p>
            <div className={phase.progress === null ? "progress-bar indeterminate" : "progress-bar"}>
              <div className="progress-fill" style={{ width: `${Math.round((phase.progress?.fraction ?? 0.3) * 100)}%` }} />
            </div>
            <button className="small" data-feature="tracker-import:cancel-download"
              title={t("Stop the download; nothing is imported")}
              onClick={() => { void api.cancelTrackerEvent().catch(reportFailure); }}>
              {t("Cancel download")}
            </button>
          </div>
        )}
        {phase.kind === "failed" && (
          <div className="import-failures" role="alert" title={phase.detail}>
            <span>{phase.text}</span>
            {phase.retry && (
              <button className="small" data-feature="tracker-import:retry" onClick={() => void download(true)}
                title={t("Ask the tracker again")}>
                {t("Retry")}
              </button>
            )}
          </div>
        )}
        {event && (
          <EventView event={event} query={query} onQuery={setQuery} chosen={chosen} onChosen={setChosen}
            onRefresh={() => void download(true, event.url)} onLeg={(leg) => openLeg(event, leg)} />
        )}
        <div className="modal-actions">
          <button onClick={close} title={t("Import nothing")}>{t("Cancel")}</button>
          <span className="spacer" />
          {event && (
            <button className="primary" disabled={busy || chosen.size === 0}
              title={t("Add one track per ticked boat (one undo takes them all back out)")} onClick={() => void run(event)}>
              {chosen.size <= 1 ? t("Import") : t("Import {count} boats", { count: chosen.size })}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function EventView({ event, query, onQuery, chosen, onChosen, onRefresh, onLeg }: {
  event: TrackerEventView;
  query: string;
  onQuery: (query: string) => void;
  chosen: Set<string>;
  onChosen: (chosen: Set<string>) => void;
  onRefresh: () => void;
  onLeg: (leg: number) => void;
}) {
  const t = useT();
  const shown = useMemo(() => filterBoats(event.boats, query), [event.boats, query]);
  const allShown = shown.length > 0 && shown.every((b) => chosen.has(b.id) || b.fixes === 0);
  const toggle = (id: string, on: boolean) => {
    const next = new Set(chosen);
    if (on) next.add(id); else next.delete(id);
    onChosen(next);
  };
  const toggleShown = (on: boolean) => {
    const next = new Set(chosen);
    for (const b of shown) {
      if (b.fixes === 0) continue;
      if (on) next.add(b.id); else next.delete(b.id);
    }
    onChosen(next);
  };
  return (
    <>
      <div className="tracker-event-head">
        <strong>{event.title || event.key}</strong>
        <span className="muted">{dateRange(event.start, event.stop)}</span>
        <span className="muted">{t("{count} boats", { count: event.boats.length })}</span>
        {event.leg !== null && event.legs !== null && (
          <label className="tracker-leg">
            {t("Leg")}
            <select data-feature="tracker-import:leg" value={event.leg}
              title={t("This race is sailed in legs; each leg is its own event. Choose another to download it")}
              onChange={(e) => onLeg(Number(e.target.value))}>
              {Array.from({ length: event.legs }, (_, k) => k + 1).map((n) => (
                <option key={n} value={n}>{t("Leg {leg} of {legs}", { leg: n, legs: event.legs ?? n })}</option>
              ))}
            </select>
          </label>
        )}
        {event.cached && (
          <button className="small" data-feature="tracker-import:refresh" onClick={onRefresh}
            title={t("Kept from earlier in this session; download it again for newer positions")}>
            {t("Download again")}
          </button>
        )}
      </div>
      {event.fallback && <p className="muted">{t("The positions were read from the tracker's KML, because its binary did not load.")}</p>}
      <TrackerPreview boats={event.boats} chosen={chosen} />
      <input className="tracker-search" data-feature="tracker-import:search" type="search" value={query}
        placeholder={t("Search name, sail number, model or division")}
        title={t("Show only the boats whose name, sail number, model or division contains this")}
        onChange={(e) => onQuery(e.target.value)} />
      <div className="tracker-boats">
        <table data-feature="tracker-import:boats">
          <thead>
            <tr>
              <th>
                <input type="checkbox" data-feature="tracker-import:select-shown" checked={allShown}
                  title={t("Tick or untick every boat shown")} aria-label={t("Tick or untick every boat shown")}
                  onChange={(e) => toggleShown(e.target.checked)} />
              </th>
              <th>{t("Boat")}</th>
              <th>{t("Sail number")}</th>
              <th>{t("Model")}</th>
              <th>{t("Division")}</th>
              <th>{t("Positions")}</th>
              <th>{t("Status")}</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((boat) => (
              <tr key={boat.id} className={boat.fixes === 0 ? "muted" : undefined}>
                <td>
                  <input type="checkbox" checked={chosen.has(boat.id)} disabled={boat.fixes === 0}
                    aria-label={boat.name} onChange={(e) => toggle(boat.id, e.target.checked)} />
                </td>
                <td>{boat.name}</td>
                <td>{boat.sail ?? ""}</td>
                <td>{boat.model ?? ""}</td>
                <td>{boat.division ?? ""}</td>
                <td title={dateRange(boat.first, boat.last)}>{boat.fixes}</td>
                <td>{boatStatusText(boat.status)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {shown.length === 0 && <p className="muted">{t("No boat matches the search.")}</p>}
      </div>
    </>
  );
}

/** Every boat's track, the ticked ones highlighted, over the coastline. */
function TrackerPreview({ boats, chosen }: { boats: TrackerBoatRow[]; chosen: Set<string> }) {
  const t = useT();
  const [basemap, setBasemap] = useState<Basemap | null>(null);
  useEffect(() => {
    let live = true;
    loadBasemap().then((b) => { if (live) setBasemap(b); }).catch(() => undefined);
    return () => { live = false; };
  }, []);
  const frame = useMemo(() => frameOf(boats.map((b) => b.preview)), [boats]);
  const coast = useMemo(() => {
    const lod = frame && basemap ? lodFor(frame, basemap.lods) : undefined;
    return frame && lod ? coastPath(frame, lod) : "";
  }, [frame, basemap]);
  const paths = useMemo(
    () => (frame ? boats.filter((b) => b.preview.length >= 2).map((b) => [b.id, linePath(frame, b.preview)] as const) : []),
    [frame, boats],
  );
  if (frame === null) return null;
  return (
    <svg className="tracker-preview" viewBox={`0 0 ${VIEW_W} ${VIEW_H}`} role="img" aria-label={t("Map preview of the event's tracks")}
      style={{ background: mapColour("sea") }}>
      <path d={coast} fill="none" stroke={mapColour("coast")} strokeWidth={0.8} />
      {paths.filter(([id]) => !chosen.has(id)).map(([id, d]) => <path key={id} d={d} className="tracker-preview-line" />)}
      {paths.filter(([id]) => chosen.has(id)).map(([id, d]) => <path key={id} d={d} className="tracker-preview-line chosen" />)}
    </svg>
  );
}
