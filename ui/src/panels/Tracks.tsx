import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import type { TrackFileInspection } from "../generated/TrackFileInspection";
import type { TrackFilters } from "../generated/TrackFilters";
import type { TrackImportFailure } from "../generated/TrackImportFailure";
import type { TrackImportLine } from "../generated/TrackImportLine";
import type { TrackSummary } from "../generated/TrackSummary";
import { onReveal } from "../help/highlight";
import { setHint } from "../hint";
import { useT } from "../i18n";
import { api } from "../ipc";
import { pickTrackFiles } from "../project/dialogs";
import { focusMap } from "../selection";
import TrackImportDialog from "./TrackImportDialog";
import { dateRange, describeImportLine, describeTrackFailure, envStatusText, fromLocalInput, toLocalInput } from "./trackImport";

/**
 * The Tracks section of the left navigation (spec.md 7.1): the tracker
 * buttons (their imports arrive in later versions), File… for GeoJSON and
 * CSV, the result of the last import, and every track with its colour, boat,
 * event, dates, samples used, environment status and actions. Each track
 * unfolds to its sample filters and heading and speed derivation (spec.md
 * 7.4, 7.6), every change one undo.
 */
export default function Tracks({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const t = useT();
  const [inspections, setInspections] = useState<TrackFileInspection[] | null>(null);
  const [imported, setImported] = useState<TrackImportLine[]>([]);
  const [failures, setFailures] = useState<TrackImportFailure[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  const tracks = project.sources.filter((s) => s.track !== null);
  const later = t("Arrives in a later version");
  // The search's `track:details` step unfolds the first track's filters.
  const first = useRef<number | null>(null);
  first.current = tracks[0]?.id ?? null;
  useEffect(() => onReveal("track:details", () => { if (first.current !== null) setOpen(first.current); }), []);

  const chooseFiles = async () => {
    try {
      const paths = await pickTrackFiles();
      if (paths.length === 0) return;
      setInspections(await api.inspectTrackFiles(paths));
    } catch (error) {
      reportFailure(error);
    }
  };

  const remove = (id: number) => { api.removeSource(id).then(onProject).catch(reportFailure); };

  return (
    <>
      <div className="section-actions">
        <button data-feature="tracks:yellowbrick" disabled title={later}>{t("YellowBrick…")}</button>
        <button data-feature="tracks:geovoile" disabled title={later}>{t("Geovoile…")}</button>
        <button data-feature="tracks:bluewater" disabled title={later}>{t("Blue Water…")}</button>
        <button data-feature="tracks:import-file" onClick={() => void chooseFiles()}
          title={t("Import GeoJSON and CSV tracks; several files at once")}>
          {t("File…")}
        </button>
      </div>
      {imported.length > 0 && (
        <ul className="track-import-summary" aria-label={t("Last import")}>
          {imported.map((line, k) => <li key={k}>{describeImportLine(line)}</li>)}
        </ul>
      )}
      {failures.length > 0 && (
        <ul className="import-failures" role="alert" aria-label={t("Files that were not imported")}>
          {failures.map((failure, k) => <li key={k} title={failure.message}>{describeTrackFailure(failure)}</li>)}
        </ul>
      )}
      {tracks.length === 0
        ? <p className="muted placeholder">{t("No tracks in this project yet.")}</p>
        : <ul className="track-list">
          {tracks.map((source) => (
            <TrackItem key={source.id} source={source} track={source.track!} open={open === source.id}
              onToggle={() => setOpen(open === source.id ? null : source.id)}
              onRemove={() => remove(source.id)} onProject={onProject} />
          ))}
        </ul>}
      {inspections !== null && (
        <TrackImportDialog inspections={inspections} onCancel={() => setInspections(null)}
          onDone={(result) => {
            setInspections(null);
            onProject(result.project);
            setImported(result.imported);
            setFailures(result.failures);
            const count = result.imported.length;
            if (count > 0) setHint(count === 1 ? t("Imported 1 track.") : t("Imported {count} tracks.", { count }));
          }} />
      )}
    </>
  );
}

function TrackItem({ source, track, open, onToggle, onRemove, onProject }: {
  source: SourceSummary;
  track: TrackSummary;
  open: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onProject: (project: ProjectSummary) => void;
}) {
  const t = useT();
  const later = t("Arrives with the environment fetch");
  return (
    <li className="track-item">
      <div className="track-row">
        <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
        <span className="polar-file-text">
          <span className="polar-file-label" title={track.event_title}>{source.label}</span>
          <span className="muted polar-file-meta">
            {track.event_title} · {dateRange(track.start, track.end)}
          </span>
          <span className="muted polar-file-meta">
            {t("{used} of {count} samples used", { used: source.used ?? 0, count: track.samples })}
            {" · "}
            {t("Environment: {status}", { status: envStatusText(track) })}
          </span>
        </span>
        <button className="icon-button" data-feature="tracks:show-on-map" disabled={!source.visible}
          title={source.visible ? t("Show this track on the map") : t("Hidden tracks are not on the map")}
          aria-label={t("Show on map")} onClick={() => focusMap({ kind: "track", sourceId: source.id })}>
          ⌖
        </button>
        <button className="icon-button" data-feature="tracks:filters" aria-expanded={open}
          title={t("Filters and heading and speed derivation")} aria-label={t("Filters")} onClick={onToggle}>
          {open ? "▾" : "▸"}
        </button>
        <button className="icon-button" data-feature="tracks:remove" onClick={onRemove}
          title={t("Remove this track from the project (undoable)")} aria-label={t("Remove")}>
          ✕
        </button>
      </div>
      {open && (
        <div className="track-details">
          <TrackFiltersEditor id={source.id} track={track} onProject={onProject} />
          <DerivationEditor id={source.id} track={track} onProject={onProject} />
          <div className="section-actions">
            <button className="small" data-feature="tracks:refetch" disabled title={later}>{t("Refetch environment")}</button>
            <button className="small" data-feature="tracks:export-grib" disabled title={later}>{t("Export reanalysis GRIB…")}</button>
          </div>
        </div>
      )}
    </li>
  );
}

/** A number field that commits on Enter or leaving it; empty is "no bound". */
function NumberField({ feature, label, title, value, min, max, step, onCommit }: {
  feature: string; label: string; title: string; value: number | null;
  min: number; max: number; step: number; onCommit: (value: number | null) => void;
}) {
  const [text, setText] = useState(value === null ? "" : String(value));
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    setText(value === null ? "" : String(value));
  }
  const commit = () => {
    const trimmed = text.trim();
    const next = trimmed === "" ? null : Number(trimmed);
    if (next !== null && !Number.isFinite(next)) { setText(value === null ? "" : String(value)); return; }
    if (next !== value) onCommit(next);
  };
  return (
    <label className="track-field">
      {label}
      <input type="number" data-feature={feature} value={text} min={min} max={max} step={step} title={title}
        onChange={(e) => setText(e.target.value)} onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} />
    </label>
  );
}

function TrackFiltersEditor({ id, track, onProject }: {
  id: number; track: TrackSummary; onProject: (project: ProjectSummary) => void;
}) {
  const t = useT();
  const f = track.filters;
  const set = (change: Partial<TrackFilters>) => {
    api.setTrackFilters(id, { ...f, ...change }).then(onProject).catch(reportFailure);
  };
  const origins = [
    { id: "any", label: t("Given or derived") },
    { id: "given", label: t("Given only") },
    { id: "derived", label: t("Derived only") },
  ];
  const needsWind = t("Needs the environment, which arrives with the reanalysis fetch");
  return (
    <fieldset className="track-filters">
      <legend>{t("Sample filters")}</legend>
      <p className="muted">
        {t("{filtered} of {count} samples filtered out; they stay in the project and show dimmed.", { filtered: track.filtered, count: track.samples })}
      </p>
      <label className="track-field">
        {t("From (UTC)")}
        <input type="datetime-local" data-feature="tracks:time-start" value={toLocalInput(f.time_start)}
          title={t("Leave out samples before this time, such as before the start")}
          onChange={(e) => set({ time_start: fromLocalInput(e.target.value) })} />
      </label>
      <label className="track-field">
        {t("To (UTC)")}
        <input type="datetime-local" data-feature="tracks:time-end" value={toLocalInput(f.time_end)}
          title={t("Leave out samples after this time, such as after the finish")}
          onChange={(e) => set({ time_end: fromLocalInput(e.target.value) })} />
      </label>
      <NumberField feature="tracks:min-bsp" label={t("Minimum BSP (kn)")} value={f.min_bsp} min={0} max={100} step={0.5}
        title={t("Leave out samples slower than this; empty for no minimum")} onCommit={(v) => set({ min_bsp: v })} />
      <NumberField feature="tracks:max-bsp" label={t("Maximum BSP (kn)")} value={f.max_bsp} min={0} max={100} step={0.5}
        title={t("Leave out samples faster than this; empty for no maximum")} onCommit={(v) => set({ max_bsp: v })} />
      <NumberField feature="tracks:manoeuvre" label={t("Manoeuvre threshold (°)")} value={f.max_heading_change} min={1} max={180} step={5}
        title={t("Leave out samples whose heading changes more than this from a neighbour: tacks and gybes. Empty to keep them.")}
        onCommit={(v) => set({ max_heading_change: v })} />
      <label className="track-field">
        {t("Heading")}
        <select data-feature="tracks:heading-origin" value={f.heading_origin}
          title={t("Keep samples whose heading the track gave, was derived, or either")}
          onChange={(e) => set({ heading_origin: e.target.value })}>
          {origins.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
        </select>
      </label>
      <label className="track-field">
        {t("Speed")}
        <select data-feature="tracks:speed-origin" value={f.speed_origin}
          title={t("Keep samples whose speed the track gave, was derived, or either")}
          onChange={(e) => set({ speed_origin: e.target.value })}>
          {origins.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
        </select>
      </label>
      <fieldset className="track-env-filters" disabled data-feature="tracks:env-filters" title={needsWind}>
        <legend>{t("Wind, waves and current")}</legend>
        <span className="muted">{t("TWS, TWA, wave height and direction, and current speed filters need the environment.")}</span>
      </fieldset>
    </fieldset>
  );
}

function DerivationEditor({ id, track, onProject }: {
  id: number; track: TrackSummary; onProject: (project: ProjectSummary) => void;
}) {
  const t = useT();
  const set = (maxGapS: number, prefer: "given" | "derived") => {
    api.setTrackDerivation(id, maxGapS, prefer).then(onProject).catch(reportFailure);
  };
  const prefer = track.prefer === "derived" ? "derived" : "given";
  return (
    <fieldset className="track-filters">
      <legend>{t("Heading and speed")}</legend>
      <NumberField feature="tracks:max-gap" label={t("Maximum gap (h)")} value={track.max_gap_s / 3600} min={0.01} max={24} step={0.5}
        title={t("Neighbours further apart in time than this are not used to derive heading and speed")}
        onCommit={(v) => { if (v !== null && v > 0) set(Math.round(v * 3600), prefer); }} />
      <label className="track-field">
        {t("Prefer")}
        <select data-feature="tracks:prefer" value={prefer}
          title={t("Use the heading and speed the track gives where it gives them, or always derive them from the positions")}
          onChange={(e) => set(track.max_gap_s, e.target.value === "derived" ? "derived" : "given")}>
          <option value="given">{t("Given values")}</option>
          <option value="derived">{t("Derived values")}</option>
        </select>
      </label>
    </fieldset>
  );
}
