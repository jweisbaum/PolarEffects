import { useBoatApi } from "../boats/context";
import { listen } from "@tauri-apps/api/event";
import { memo, useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";

import { describeError, reportFailure } from "../errors";
import type { TrackerBoatRow } from "../generated/TrackerBoatRow";
import type { TrackerEventView } from "../generated/TrackerEventView";
import type { TrackerListed } from "../generated/TrackerListed";
import type { TrackerProgress } from "../generated/TrackerProgress";
import type { TrackImportResult } from "../generated/TrackImportResult";
import { t, useLanguage, useT, type Language } from "../i18n";
import { TRACKER_LISTED, TRACKER_PROGRESS } from "../ipc";
import { loadBasemap } from "../map/basemap";
import type { Basemap } from "../map/format";
import { mapColour } from "../settings/themes";
import { dateRange, formatBytes } from "./trackImport";
import { addressHint, boatStatusText, filterBoats, legAddress, NO_RETRY, TRACKER_NAMES, type TrackerId } from "./trackerImport";
import { coastPath, frameOf, linePath, lodFor, VIEW_H, VIEW_W } from "./trackerPreview";

type DownloadFailure = { text: string; detail: string; retry: boolean };

type Phase =
  | { kind: "address" }
  | { kind: "downloading"; progress: TrackerProgress | null }
  | ({ kind: "failed" } & DownloadFailure)
  // A listing has no positions; only Import tracks starts their download.
  | { kind: "event"; event: TrackerEventView; progress: TrackerProgress | null; pending?: boolean; failure?: DownloadFailure };

/**
 * The shared tracker dialog (spec.md 7.2): paste the event's address and
 * app loads only the boat list (YellowBrick's RaceSetup, Geovoile's config).
 * Import tracks downloads positions with progress and Cancel, then adds
 * the ticked boats, one track each, one undo, with no weather (D24).
 * A provider with a combined response waits for Import tracks before
 * downloading anything, then offers its boat picker.
 * The event stays in memory for the session, so opening it again
 * downloads nothing.
 *
 * Its controls exist only once the dialog is open, so their registry
 * entries land on the tracker's button (`landing`); its answer buttons
 * (Cancel, Import tracks) go untagged as in every transient dialog
 * (spec.md 3.6).
 */
export default function TrackerImportDialog({ tracker, onDone, onCancel }: {
  tracker: TrackerId;
  onDone: (result: TrackImportResult) => void;
  onCancel: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const [url, setUrl] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "address" });
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const loading = phase.kind === "downloading" || (phase.kind === "event" && phase.pending === true);
  const live = useRef(true);
  // Set on every mount, not only by useRef's initial value: StrictMode
  // (development) runs this cleanup and mounts again, and a flag left false
  // would drop every answer and leave the dialog "downloading" for ever.
  useEffect(() => {
    live.current = true;
    return () => { live.current = false; };
  }, []);
  // Which download is current: a later one (another leg, Refresh boat list)
  // makes an earlier one's answer, or its cancellation, stale.
  const generation = useRef(0);
  // The key of the current download, named by this dialog: a boat list sent
  // ahead for any other download (an earlier address, or a download a
  // closed dialog left finishing) is not this one's (M17a).
  const downloadKey = useRef("");

  const close = () => {
    live.current = false;
    ++generation.current;
    if (loading || busy) void api.cancelTrackerEvent().catch(() => undefined);
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

  // Progress, and the boat list ahead of the positions, arrive as events
  // while a download runs.
  useEffect(() => {
    if (!loading) return;
    const progress = listen<TrackerProgress>(TRACKER_PROGRESS, (e) => {
      if (!live.current) return;
      setPhase((p) => (p.kind === "downloading" || (p.kind === "event" && p.pending) ? { ...p, progress: e.payload } : p));
    }).catch(() => null);
    const listed = listen<TrackerListed>(TRACKER_LISTED, (e) => {
      if (!live.current || e.payload.download !== downloadKey.current) return;
      setPhase((p) => (p.kind === "event" && p.pending
        ? { ...p, event: e.payload.event } : p));
    }).catch(() => null);
    return () => {
      void progress.then((off) => off?.());
      void listed.then((off) => off?.());
    };
  }, [loading]);

  const download = async (refresh: boolean, address = url) => {
    if (address.trim() === "") return;
    const mine = ++generation.current;
    const key = `${DIALOG_ID}.${++downloads}`;
    downloadKey.current = key;
    setPhase({ kind: "downloading", progress: null });
    setChosen(new Set());
    setQuery("");
    try {
      const event = await api.trackerEvent(tracker, address.trim(), refresh, key, true);
      if (!live.current || mine !== generation.current) return;
      setPhase({ kind: "event", event, progress: null });
    } catch (error) {
      if (!live.current || mine !== generation.current) return;
      const kind = (error as { kind?: string }).kind ?? "";
      if (kind === "cancelled") { setPhase({ kind: "address" }); return; }
      const shown = describeError(error);
      const failure = { text: shown.text, detail: shown.detail, retry: !NO_RETRY.has(kind) };
      setPhase({ kind: "failed", ...failure });
    }
  };

  const run = async (event: TrackerEventView) => {
    if (busy || loading) return;
    const mine = ++generation.current;
    const key = `${DIALOG_ID}.${++downloads}`;
    downloadKey.current = key;
    const ids = event.boats.filter((b) => chosen.has(b.id)).map((b) => b.id);
    setBusy(true);
    let ready = event;
    try {
      if (!event.positions) {
        setPhase({ kind: "event", event, progress: null, pending: true });
        ready = await api.trackerEvent(tracker, event.url, true, key);
        if (!live.current || mine !== generation.current) return;
        setPhase({ kind: "event", event: ready, progress: null });
        // A combined response first supplies the boat picker after consent.
        if (ids.length === 0) return;
      }
      const result = await api.importTrackerBoats(ready.tracker, ready.key, ids);
      if (live.current && mine === generation.current) onDone(result);
    } catch (error) {
      if (!live.current || mine !== generation.current) return;
      if (ready.positions) { reportFailure(error); return; }
      const shown = describeError(error);
      const kind = (error as { kind?: string }).kind ?? "";
      setPhase({ kind: "event", event, progress: null,
        failure: { text: shown.text, detail: shown.detail, retry: !NO_RETRY.has(kind) } });
    } finally {
      if (live.current && mine === generation.current) setBusy(false);
    }
  };

  /** Another leg: load its boat list, or recall already downloaded tracks. */
  const openLeg = (event: TrackerEventView, leg: number) => {
    const address = legAddress(event.url, leg);
    setUrl(address);
    void download(false, address);
  };

  const title = t("Import from {tracker}", { tracker: TRACKER_NAMES[tracker] });
  const event = phase.kind === "event" ? phase.event : null;
  const failure = phase.kind === "failed" ? phase : phase.kind === "event" ? phase.failure : undefined;
  const retry = () => {
    if (event) { void run(event); return; }
    void download(true);
  };
  const progress = phase.kind === "downloading" || phase.kind === "event" ? phase.progress : null;
  // A stray click on the backdrop must not throw away a download or an
  // import under way; Cancel and Escape still end them (Cancel also stops
  // the download).
  return (
    <div className="modal-backdrop" onClick={() => { if (!loading && !busy) close(); }}>
      <div className="modal tracker-import" role="dialog" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <h2>{title}</h2>
        <form className="tracker-address" onSubmit={(e) => { e.preventDefault(); void download(false); }}>
          <label>
            {t("Event address")}
            <input data-feature="tracker-import:url" value={url} autoFocus disabled={loading || busy}
              placeholder={t("Paste the race's tracker link or key")}
              title={addressHint(tracker)}
              onChange={(e) => setUrl(e.target.value)} />
          </label>
          <button type="submit" data-feature="tracker-import:open" disabled={loading || busy || url.trim() === ""}
            title={t("Load the boat list; tracks download when you click Import tracks")}>
            {t("Open")}
          </button>
        </form>
        {loading && (
          <div className="tracker-progress" role="status">
            <p className="muted">
              {progress?.fallback
                ? t("The positions did not load; reading the KML instead…")
                : event !== null
                  ? t("Downloading the boats' positions…")
                  : t("Loading the boat list…")}
              {progress !== null && progress.bytes > 0 && ` ${formatBytes(progress.bytes)}`}
            </p>
            <div className={progress === null ? "progress-bar indeterminate" : "progress-bar"}>
              <div className="progress-fill" style={{ width: `${Math.round((progress?.fraction ?? 0.3) * 100)}%` }} />
            </div>
          </div>
        )}
        {failure && (
          <div className="import-failures" role="alert" title={failure.detail}>
            <span>{event && event.boats.length > 0 ? t("The boat list loaded, but the track positions could not be downloaded.") : failure.text}</span>
            {failure.retry && (
              <button className="small" data-feature="tracker-import:retry" onClick={retry}
                title={t("Ask the tracker again")}>
                {t("Retry")}
              </button>
            )}
          </div>
        )}
        {event && event.boats.length > 0 && (
          <EventView event={event} disabled={loading || busy} query={query} onQuery={setQuery} chosen={chosen} setChosen={setChosen}
            onRefresh={() => void download(true, event.url)} onLeg={(leg) => openLeg(event, leg)} />
        )}
        {event && !event.positions && event.boats.length === 0 && (
          <p className="muted">{t("This tracker sends the boat list with the tracks. Click Import tracks to download them, then choose your boats.")}</p>
        )}
        <div className="modal-actions">
          <button onClick={close} title={t("Import nothing")}>{t("Cancel")}</button>
          <span className="spacer" />
          {event && chosen.size > 0 && (
            <span className="muted">{t("{count} boats ticked", { count: chosen.size })}</span>
          )}
          {event && (
            <button className="primary" disabled={busy || loading || ((event.positions || event.boats.length > 0) && chosen.size === 0)}
              title={t("Download and add the ticked boats' tracks; fetch their weather later")}
              onClick={() => void run(event)}>
              {t("Import tracks")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** This page load, in download keys: a reload's downloads never match an older one's. */
const DIALOG_ID = Date.now().toString(36);
/** Downloads started from this page, across dialogs: each gets its own key. */
let downloads = 0;

/**
 * One boat of the table; memoised, so ticking a boat redraws only its row.
 * It takes the language as a prop rather than subscribing to it: 444
 * subscriptions doubled the table's first draw.
 */
const BoatRow = memo(function BoatRow({ boat, checked, loading, disabled, onToggle }: {
  boat: TrackerBoatRow;
  checked: boolean;
  loading: boolean;
  disabled: boolean;
  onToggle: (id: string, on: boolean) => void;
  language: Language;
}) {
  const empty = !loading && boat.fixes === 0;
  return (
    <tr className={empty ? "muted" : undefined}>
      <td>
        <input type="checkbox" checked={checked} disabled={empty || disabled}
          aria-label={boat.name} onChange={(e) => onToggle(boat.id, e.target.checked)} />
      </td>
      <td>{boat.name}</td>
      <td>{boat.sail ?? ""}</td>
      <td>{boat.model ?? ""}</td>
      <td>{boat.division ?? ""}</td>
      {loading
        ? <td className="muted" title={t("The positions have not loaded yet")}>—</td>
        : <td title={dateRange(boat.first, boat.last)}>{boat.fixes}</td>}
      <td>{boatStatusText(boat.status)}</td>
    </tr>
  );
});

function EventView({ event, disabled, query, onQuery, chosen, setChosen, onRefresh, onLeg }: {
  event: TrackerEventView;
  disabled: boolean;
  query: string;
  onQuery: (query: string) => void;
  chosen: ReadonlySet<string>;
  setChosen: Dispatch<SetStateAction<ReadonlySet<string>>>;
  onRefresh: () => void;
  onLeg: (leg: number) => void;
}) {
  const t = useT();
  const language = useLanguage();
  const loading = !event.positions;
  const shown = useMemo(() => filterBoats(event.boats, query), [event.boats, query]);
  const pickable = (b: TrackerBoatRow) => loading || b.fixes > 0;
  const allShown = shown.length > 0 && shown.every((b) => chosen.has(b.id) || !pickable(b));
  const toggle = useCallback((id: string, on: boolean) => {
    setChosen((old) => {
      const next = new Set(old);
      if (on) next.add(id); else next.delete(id);
      return next;
    });
  }, [setChosen]);
  const toggleShown = (on: boolean) => {
    setChosen((old) => {
      const next = new Set(old);
      for (const b of shown) {
        if (!pickable(b)) continue;
        if (on) next.add(b.id); else next.delete(b.id);
      }
      return next;
    });
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
            <select data-feature="tracker-import:leg" value={event.leg} disabled={disabled}
              title={t("This race is sailed in legs; choose a leg to load its boat list")}
              onChange={(e) => onLeg(Number(e.target.value))}>
              {Array.from({ length: event.legs }, (_, k) => k + 1).map((n) => (
                <option key={n} value={n}>{t("Leg {leg} of {legs}", { leg: n, legs: event.legs ?? n })}</option>
              ))}
            </select>
          </label>
        )}
        {event.cached && (
          <button className="small" data-feature="tracker-import:refresh" disabled={disabled} onClick={onRefresh}
            title={t("Reload the boat list; newer tracks download when you click Import tracks")}>
            {t("Refresh boat list")}
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
                <input type="checkbox" data-feature="tracker-import:select-shown" checked={allShown} disabled={disabled}
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
              <BoatRow key={boat.id} boat={boat} checked={chosen.has(boat.id)} loading={loading} disabled={disabled} onToggle={toggle}
                language={language} />
            ))}
          </tbody>
        </table>
        {shown.length === 0 && <p className="muted">{t("No boat matches the search.")}</p>}
      </div>
    </>
  );
}

/**
 * Every boat's track, the ticked ones highlighted, over the coastline.
 * Nothing until the positions are in (a list sent ahead has no preview).
 */
function TrackerPreview({ boats, chosen }: { boats: TrackerBoatRow[]; chosen: ReadonlySet<string> }) {
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
  // Two paths in all, the ticked lines drawn last, on top: one element per
  // boat made the first draw of a big fleet twice as slow (M14b).
  const [rest, ticked] = useMemo(() => {
    const a: string[] = [], b: string[] = [];
    for (const [id, d] of paths) (chosen.has(id) ? b : a).push(d);
    return [a, b];
  }, [paths, chosen]);
  if (frame === null) return null;
  return (
    <svg className="tracker-preview" viewBox={`0 0 ${VIEW_W} ${VIEW_H}`} role="img" aria-label={t("Map preview of the event's tracks")}
      style={{ background: mapColour("sea") }}>
      <path d={coast} fill="none" stroke={mapColour("coast")} strokeWidth={0.8} />
      {rest.length > 0 && <path d={rest.join("")} className="tracker-preview-line" data-lines={rest.length} />}
      {ticked.length > 0 && <path d={ticked.join("")} className="tracker-preview-line chosen" data-lines={ticked.length} />}
    </svg>
  );
}
