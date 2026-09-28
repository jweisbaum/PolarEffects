import { useCallback, useEffect, useRef, useState } from "react";

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
import { msg, useT } from "../i18n";
import { api } from "../ipc";
import { envJobOf, useEnvJobs } from "../jobs";
import { pickTrackFiles } from "../project/dialogs";
import { focusMap } from "../selection";
import EnvFetchDialog from "./EnvFetchDialog";
import TrackImportDialog from "./TrackImportDialog";
import TrackerImportDialog from "./TrackerImportDialog";
import type { TrackerId } from "./trackerImport";
import type { TrackImportResult } from "../generated/TrackImportResult";
import { dateRange, describeImportLine, describeTrackFailure, envStatusText, fromLocalInput, toLocalInput } from "./trackImport";

/**
 * The Tracks section of the left navigation (spec.md 7.1): the tracker
 * buttons (YellowBrick opens the tracker dialog, spec.md 7.2; Geovoile and
 * Blue Water arrive in later versions), File… for GeoJSON and CSV, the result of the last import, and every track with its colour, boat,
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
  const [tracker, setTracker] = useState<TrackerId | null>(null);
  const [imported, setImported] = useState<TrackImportLine[]>([]);
  const [failures, setFailures] = useState<TrackImportFailure[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  // The fetch pre-flight: which tracks, and whether to fetch every sample again.
  const [fetching, setFetching] = useState<{ ids: number[]; restart: boolean } | null>(null);
  const closeFetch = useCallback(() => setFetching(null), []);
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

  /** After any import, file or tracker: the summary, and the environment fetch's pre-flight. */
  const afterImport = (result: TrackImportResult) => {
    onProject(result.project);
    setImported(result.imported);
    setFailures(result.failures);
    const count = result.imported.length;
    if (count > 0) setHint(count === 1 ? t("Imported 1 track.") : t("Imported {count} tracks.", { count }));
    // Importing starts the environment fetch (spec.md 7.5), after its
    // pre-flight (spec.md 13).
    if (count > 0) setFetching({ ids: result.imported.map((line) => line.source_id), restart: false });
  };

  const remove = (id: number) => { api.removeSource(id).then(onProject).catch(reportFailure); };

  return (
    <>
      <div className="section-actions">
        <button data-feature="tracks:yellowbrick" onClick={() => setTracker("yellowbrick")}
          title={t("Import boats from a YellowBrick race: paste its link or race key")}>
          {t("YellowBrick…")}
        </button>
        <button data-feature="tracks:geovoile" disabled title={later}>{t("Geovoile…")}</button>
        <button data-feature="tracks:bluewater" disabled title={later}>{t("Blue Water…")}</button>
        <button data-feature="tracks:import-file" onClick={() => void chooseFiles()}
          title={t("Import GeoJSON and CSV tracks; several files at once")}>
          {t("File…")}
        </button>
      </div>
      {tracks.length > 0 && <EnvOptions project={project} onProject={onProject} />}
      {imported.length > 0 && (
        <ul className="track-import-summary" aria-label={t("Last import")}>
          {imported.map((line, k) => <li key={k}>{describeImportLine(line)}</li>)}
        </ul>
      )}
      {failures.length > 0 && (
        <ul className="import-failures" role="alert" aria-label={t("Files or boats that were not imported")}>
          {failures.map((failure, k) => <li key={k} title={failure.message}>{describeTrackFailure(failure)}</li>)}
        </ul>
      )}
      {tracks.length === 0
        ? <p className="muted placeholder">{t("No tracks in this project yet.")}</p>
        : <ul className="track-list">
          {tracks.map((source) => (
            <TrackItem key={source.id} source={source} track={source.track!} open={open === source.id}
              onToggle={() => setOpen(open === source.id ? null : source.id)}
              onRemove={() => remove(source.id)} onProject={onProject}
              onRefetch={() => setFetching({ ids: [source.id], restart: source.track!.env_status === "ready" })} />
          ))}
        </ul>}
      {inspections !== null && (
        <TrackImportDialog inspections={inspections} onCancel={() => setInspections(null)}
          onDone={(result) => {
            setInspections(null);
            afterImport(result);
          }} />
      )}
      {tracker !== null && (
        <TrackerImportDialog tracker={tracker} onCancel={() => setTracker(null)}
          onDone={(result) => {
            setTracker(null);
            afterImport(result);
          }} />
      )}
      {fetching !== null && <EnvFetchDialog sourceIds={fetching.ids} restart={fetching.restart} onClose={closeFetch} />}
    </>
  );
}

/**
 * The project's current settings (spec.md 7.5, 7.5.1): whether the polar is
 * fed from water-relative values where a current was found, and whether the
 * global merged current includes Stokes drift (for the next fetch). Each
 * change is one undo.
 */
function EnvOptions({ project, onProject }: { project: ProjectSummary; onProject: (project: ProjectSummary) => void }) {
  const t = useT();
  return (
    <div className="track-env-options">
      <label title={t("Take the current out of boat speed and wind where a current was found, so the polar is through the water")}>
        <input type="checkbox" data-feature="tracks:use-corrected" checked={project.use_corrected}
          onChange={(e) => { api.setUseCorrected(e.target.checked).then(onProject).catch(reportFailure); }} />
        {t("Correct for current")}
      </label>
      <label title={t("Add Stokes drift to the global merged current; applies to the next fetch")}>
        <input type="checkbox" data-feature="tracks:stokes-drift" checked={project.stokes_drift}
          onChange={(e) => { api.setStokesDrift(e.target.checked).then(onProject).catch(reportFailure); }} />
        {t("Include Stokes drift")}
      </label>
    </div>
  );
}

function TrackItem({ source, track, open, onToggle, onRemove, onProject, onRefetch }: {
  source: SourceSummary;
  track: TrackSummary;
  open: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onProject: (project: ProjectSummary) => void;
  onRefetch: () => void;
}) {
  const t = useT();
  const later = t("Arrives in a later version");
  const job = envJobOf(useEnvJobs(), source.id);
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
            {t("Environment: {status}", { status: envStatusText(track, job) })}
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
            {job
              ? <button className="small" data-feature="tracks:cancel-fetch" title={t("Stop this track's fetch; samples already fetched are kept")}
                onClick={() => { api.cancelEnvFetch([source.id]).catch(reportFailure); }}>{t("Cancel fetch")}</button>
              : <button className="small" data-feature="tracks:refetch" onClick={onRefetch}
                title={track.env_status === "ready"
                  ? t("Fetch this track's wind, waves and current again, every sample")
                  : t("Fetch the wind, waves and current this track's samples do not have yet")}>
                {t("Refetch environment")}
              </button>}
            <button className="small" data-feature="tracks:export-grib" disabled title={later}>{t("Export reanalysis GRIB…")}</button>
          </div>
        </div>
      )}
    </li>
  );
}

/** The wave sectors off the bow (spec.md 7.6). */
const WAVE_SECTORS: [string, string][] = [
  ["head", msg("Head (under 30°)")],
  ["bow", msg("Bow (30–60°)")],
  ["beam", msg("Beam (60–120°)")],
  ["quarter", msg("Quarter (120–150°)")],
  ["following", msg("Following (from 150°)")],
];

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

/**
 * A UTC date and time that commits on Enter or leaving the field, so one
 * edit is one undo entry and a half-typed time is never sent (and never
 * refused) while typing. An unreadable entry goes back to the stored value.
 */
function TimeField({ feature, label, title, value, onCommit }: {
  feature: string; label: string; title: string; value: number | null; onCommit: (value: number | null) => void;
}) {
  const [text, setText] = useState(toLocalInput(value));
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    setText(toLocalInput(value));
  }
  const commit = () => {
    const next = fromLocalInput(text);
    if (next === null && text.trim() !== "") { setText(toLocalInput(value)); return; }
    if (next !== value) onCommit(next);
  };
  return (
    <label className="track-field">
      {label}
      <input type="datetime-local" data-feature={feature} value={text} title={title}
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
  const needsWind = t("These filters read the environment, which this track does not have yet; with one set, samples without it are left out.");
  return (
    <fieldset className="track-filters">
      <legend>{t("Sample filters")}</legend>
      <p className="muted">
        {t("{filtered} of {count} samples filtered out; they stay in the project and show dimmed.", { filtered: track.filtered, count: track.samples })}
      </p>
      <TimeField feature="tracks:time-start" label={t("From (UTC)")} value={f.time_start}
        title={t("Leave out samples before this time, such as before the start")}
        onCommit={(v) => set({ time_start: v })} />
      <TimeField feature="tracks:time-end" label={t("To (UTC)")} value={f.time_end}
        title={t("Leave out samples after this time, such as after the finish")}
        onCommit={(v) => set({ time_end: v })} />
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
      <fieldset className="track-env-filters">
        <legend>{t("Wind, waves and current")}</legend>
        {track.env_status === "not_fetched" && <p className="muted">{needsWind}</p>}
        <div className="track-range">
          <NumberField feature="tracks:tws-min" label={t("TWS from (kn)")} value={f.tws_min} min={0} max={200} step={1}
            title={t("Leave out samples in less wind than this; empty for no minimum")} onCommit={(v) => set({ tws_min: v })} />
          <NumberField feature="tracks:tws-max" label={t("to (kn)")} value={f.tws_max} min={0} max={200} step={1}
            title={t("Leave out samples in more wind than this; empty for no maximum")} onCommit={(v) => set({ tws_max: v })} />
        </div>
        <div className="track-range">
          <NumberField feature="tracks:twa-min" label={t("TWA from (°)")} value={f.twa_min} min={0} max={180} step={5}
            title={t("Leave out samples closer to the wind than this")} onCommit={(v) => set({ twa_min: v })} />
          <NumberField feature="tracks:twa-max" label={t("to (°)")} value={f.twa_max} min={0} max={180} step={5}
            title={t("Leave out samples further off the wind than this")} onCommit={(v) => set({ twa_max: v })} />
        </div>
        <div className="track-range">
          <NumberField feature="tracks:hs-min" label={t("Wave height from (m)")} value={f.hs_min} min={0} max={100} step={0.5}
            title={t("Leave out samples in smaller waves than this")} onCommit={(v) => set({ hs_min: v })} />
          <NumberField feature="tracks:hs-max" label={t("to (m)")} value={f.hs_max} min={0} max={100} step={0.5}
            title={t("Leave out samples in bigger waves than this")} onCommit={(v) => set({ hs_max: v })} />
        </div>
        <label className="track-field">
          {t("Wave direction")}
          <select data-feature="tracks:wave-mode" value={f.wave_mode}
            title={t("Keep samples by where the waves come from: off the bow by sector or angle, or by compass direction")}
            onChange={(e) => set({ wave_mode: e.target.value })}>
            <option value="off">{t("Any")}</option>
            <option value="sectors">{t("Sectors off the bow")}</option>
            <option value="relative">{t("Angle off the bow")}</option>
            <option value="absolute">{t("Compass direction")}</option>
          </select>
        </label>
        {f.wave_mode === "sectors" && (
          <fieldset className="track-wave-sectors" data-feature="tracks:wave-sectors">
            <legend>{t("Waves from")}</legend>
            {WAVE_SECTORS.map(([id, label]) => (
              <label key={id}>
                <input type="checkbox" checked={f.wave_sectors.includes(id)} onChange={(e) => set({
                  wave_sectors: e.target.checked ? [...f.wave_sectors, id] : f.wave_sectors.filter((s) => s !== id),
                })} />
                {t(label)}
              </label>
            ))}
          </fieldset>
        )}
        {f.wave_mode === "relative" && (
          <div className="track-range">
            <NumberField feature="tracks:wave-min" label={t("Off the bow from (°)")} value={f.wave_min} min={0} max={180} step={5}
              title={t("0° is head seas, 180° following seas")} onCommit={(v) => set({ wave_min: v })} />
            <NumberField feature="tracks:wave-max" label={t("to (°)")} value={f.wave_max} min={0} max={180} step={5}
              title={t("0° is head seas, 180° following seas")} onCommit={(v) => set({ wave_max: v })} />
          </div>
        )}
        {f.wave_mode === "absolute" && (
          <div className="track-range">
            <NumberField feature="tracks:wave-from" label={t("From the compass (°)")} value={f.wave_from} min={0} max={360} step={10}
              title={t("Waves coming from this direction, clockwise to the next")} onCommit={(v) => set({ wave_from: v })} />
            <NumberField feature="tracks:wave-to" label={t("to (°)")} value={f.wave_to} min={0} max={360} step={10}
              title={t("Waves coming from up to this direction")} onCommit={(v) => set({ wave_to: v })} />
          </div>
        )}
        <div className="track-range">
          <NumberField feature="tracks:current-min" label={t("Current from (kn)")} value={f.current_min} min={0} max={100} step={0.1}
            title={t("Leave out samples in less current than this")} onCommit={(v) => set({ current_min: v })} />
          <NumberField feature="tracks:current-max" label={t("to (kn)")} value={f.current_max} min={0} max={100} step={0.1}
            title={t("Leave out samples in more current than this")} onCommit={(v) => set({ current_max: v })} />
        </div>
        <label className="track-field">
          <input type="checkbox" data-feature="tracks:no-tide" checked={f.exclude_no_tide}
            title={t("Leave out samples whose current comes from a source without tides")}
            onChange={(e) => set({ exclude_no_tide: e.target.checked })} />
          {t("Leave out currents without tide ({count})", { count: track.no_tide })}
        </label>
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
