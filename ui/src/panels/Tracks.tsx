import { useBoatApi } from "../boats/context";
import { useCallback, useEffect, useRef, useState } from "react";

import { useLiveEdit } from "./useLiveEdit";
import { reportFailure } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import type { TrackFileInspection } from "../generated/TrackFileInspection";
import type { TrackFilters } from "../generated/TrackFilters";
import type { TrackImportFailure } from "../generated/TrackImportFailure";
import type { TrackImportLine } from "../generated/TrackImportLine";
import type { TrackSummary } from "../generated/TrackSummary";
import { useBoatReveal } from "../boats/context";
import type { Units } from "../generated/Units";
import { DEFAULT_UNITS, shownValue, speedUnit, storedValue, waveUnit } from "./filterUnits";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";

import { envJobOf, useEnvJobs } from "../jobs";
import { pickTrackFiles } from "../project/dialogs";
import { useBoatSelection } from "../selection";
import BoatTrackSearch from "./BoatTrackSearch";
import EnvFetchDialog from "./EnvFetchDialog";
import TrackImportDialog from "./TrackImportDialog";
import TrackerImportDialog from "./TrackerImportDialog";
import type { TrackerId } from "./trackerImport";
import type { TrackImportResult } from "../generated/TrackImportResult";
import { dateRange, describeImportLine, describeTrackFailure, envStatusText, fromLocalInput, toLocalInput } from "./trackImport";

/**
 * The Tracks section of the left navigation (spec.md 7.1): the tracker
 * buttons (YellowBrick, Geovoile and Blue Water Tracks all open the shared
 * tracker dialog, spec.md 7.2), File… for GeoJSON and CSV, the result of
 * the last import, and every track with its colour, boat, event, dates,
 * samples used, weather status and actions. Each track unfolds to its
 * sample filters and heading and speed derivation (spec.md 7.4, 7.6),
 * every change one undo.
 *
 * Importing never fetches weather (D24): a track's Fetch weather…, or
 * Fetch weather for selected tracks… over the ticked tracks, opens the
 * fetch's pre-flight when the user wants it.
 */
export default function Tracks({ project, onProject, units = DEFAULT_UNITS }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
  /** The display units (Settings): the filters are shown and typed in them. */
  units?: Units;
}) {
  const api = useBoatApi();
  const onReveal = useBoatReveal();
  const t = useT();
  const [inspections, setInspections] = useState<TrackFileInspection[] | null>(null);
  const [tracker, setTracker] = useState<TrackerId | null>(null);
  const [imported, setImported] = useState<TrackImportLine[]>([]);
  const [failures, setFailures] = useState<TrackImportFailure[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  // The tracks ticked for Fetch weather for selected tracks…
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());
  // The fetch pre-flight: which tracks, and whether to fetch every sample again.
  const [fetching, setFetching] = useState<{ ids: number[]; restart: boolean; fromSelection?: boolean } | null>(null);
  const closeFetch = useCallback(() => setFetching(null), []);
  const tracks = project.sources.filter((s) => s.track !== null);
  // A ticked track already queued or fetching is left out: asking again
  // would only queue it twice (M17a).
  const jobs = useEnvJobs();
  const selectedIds = tracks.filter((s) => selected.has(s.id) && envJobOf(jobs, s.id) === undefined).map((s) => s.id);
  const clearSelection = useCallback(() => setSelected(new Set()), []);
  const select = (id: number, on: boolean) => setSelected((old) => {
    const next = new Set(old);
    if (on) next.add(id); else next.delete(id);
    return next;
  });
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

  /**
   * After any import, file or tracker: the summary and a hint. No weather
   * is fetched (D24); the user asks for it per track.
   */
  const afterImport = (result: TrackImportResult) => {
    onProject(result.project);
    setImported(result.imported);
    setFailures(result.failures);
    const count = result.imported.length;
    if (count > 0) {
      setHint(count === 1
        ? later(msg("Imported 1 track. Fetch its weather from the track list when you want it."))
        : later(msg("Imported {count} tracks. Fetch their weather from the track list when you want it."), { count }));
    }
  };

  const remove = (id: number) => { api.removeSource(id).then(onProject).catch(reportFailure); };

  return (
    <>
      <div className="section-actions">
        <button data-feature="tracks:yellowbrick" onClick={() => setTracker("yellowbrick")}
          title={t("Import boats from a YellowBrick race: paste its link or race key")}>
          {t("YellowBrick…")}
        </button>
        <button data-feature="tracks:geovoile" onClick={() => setTracker("geovoile")}
          title={t("Import boats from a Geovoile race: paste its viewer address")}>
          {t("Geovoile…")}
        </button>
        <button data-feature="tracks:bluewater" onClick={() => setTracker("bluewater")}
          title={t("Import boats from a Blue Water Tracks race: paste its race link")}>
          {t("Blue Water…")}
        </button>
        <button data-feature="tracks:import-file" onClick={() => void chooseFiles()}
          title={t("Import GeoJSON and CSV tracks; several files at once")}>
          {t("File…")}
        </button>
      </div>
      <BoatTrackSearch key={project.id} onImport={afterImport} />
      {tracks.length > 0 && (
        <div className="section-actions">
          <button data-feature="tracks:select-all" disabled={tracks.every(source => selected.has(source.id))}
            title={t("Select all imported tracks for weather download")}
            onClick={() => setSelected(new Set(tracks.map(source => source.id)))}>
            {t("Select all")}
          </button>
          <button data-feature="tracks:fetch-weather-selected" disabled={selectedIds.length === 0}
            title={selectedIds.length === 0
              ? t("Tick tracks in the list first, then fetch their wind, waves and current together")
              : t("Fetch the wind, waves and current the ticked tracks' samples do not have yet")}
            onClick={() => setFetching({ ids: selectedIds, restart: false, fromSelection: true })}>
            {selectedIds.length === 0
              ? t("Fetch weather for selected tracks…")
              : t("Fetch weather for {count} selected tracks…", { count: selectedIds.length })}
          </button>
        </div>
      )}
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
              selected={selected.has(source.id)} onSelect={(on) => select(source.id, on)}
              onToggle={() => setOpen(open === source.id ? null : source.id)}
              onRemove={() => remove(source.id)} onProject={onProject} units={units}
              onRefetch={() => setFetching({ ids: [source.id], restart: source.track!.env_status === "ready" })}
              />
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
      {fetching !== null && (
        <EnvFetchDialog sourceIds={fetching.ids} restart={fetching.restart} onClose={closeFetch}
          onStarted={fetching.fromSelection ? clearSelection : undefined} />
      )}
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
  const api = useBoatApi();
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

function TrackItem({ source, track, open, selected, onSelect, onToggle, onRemove, onProject, onRefetch, units }: {
  source: SourceSummary;
  units: Units;
  track: TrackSummary;
  open: boolean;
  selected: boolean;
  onSelect: (on: boolean) => void;
  onToggle: () => void;
  onRemove: () => void;
  onProject: (project: ProjectSummary) => void;
  onRefetch: () => void;
}) {
  const { focusMap } = useBoatSelection();
  const api = useBoatApi();
  const t = useT();
  const job = envJobOf(useEnvJobs(), source.id);
  return (
    <li className="track-item">
      <div className="track-row">
        <input type="checkbox" data-feature="tracks:select" checked={selected}
          title={t("Tick to fetch this track's weather with the other ticked tracks")}
          aria-label={t("Select {track}", { track: source.label })} onChange={(e) => onSelect(e.target.checked)} />
        <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
        <span className="polar-file-text">
          <span className="polar-file-label" title={track.event_title}>{source.label}</span>
          <span className="muted polar-file-meta">
            {track.event_title} · {dateRange(track.start, track.end)}
          </span>
          <span className="muted polar-file-meta">
            {t("{used} of {count} samples used", { used: source.used ?? 0, count: track.samples })}
            {" · "}
            {t("Weather: {status}", { status: envStatusText(track, job) })}
          </span>
        </span>
        {job
          ? <button className="small" data-feature="tracks:cancel-fetch" title={t("Stop this track's fetch; samples already fetched are kept")}
            onClick={() => { api.cancelEnvFetch([source.id]).catch(reportFailure); }}>{t("Cancel fetch")}</button>
          : <button className="small" data-feature="tracks:fetch-weather" onClick={onRefetch}
            title={track.env_status === "ready"
              ? t("Fetch this track's wind, waves and current again, every sample")
              : t("Fetch the wind, waves and current this track's samples do not have yet")}>
            {t("Fetch weather…")}
          </button>}
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
          <TrackFiltersEditor id={source.id} track={track} onProject={onProject} units={units} />
          <DerivationEditor id={source.id} track={track} onProject={onProject} />
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

/**
 * A number box that applies valid edits after a short typing pause. Blur flushes. `value` is stored (knots,
 * metres, degrees); `factor` converts it to what the box shows and back, so
 * a speed or a wave height is typed in the display unit. Empty means no bound.
 */
export function NumberField({ feature, label, title, value, min, max, step, factor = 1, onCommit }: {
  feature: string; label: string; title: string; value: number | null;
  min: number; max: number; step: number; factor?: number; onCommit: (value: number | null) => void;
}) {
  const [text, setText] = useState(shownValue(value, factor));
  const [shown, setShown] = useState<[number | null, number]>([value, factor]);
  if (shown[0] !== value || shown[1] !== factor) {
    setShown([value, factor]);
    setText(shownValue(value, factor));
  }
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const callback = useRef(onCommit);
  callback.current = onCommit;
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  const commit = () => {
    if (timer.current) clearTimeout(timer.current);
    const next = storedValue(text, value, factor);
    if (next === undefined) { setText(shownValue(value, factor)); return; }
    if (next !== value) onCommit(next);
  };
  return (
    <label className="track-field">
      {label}
      <input type="number" data-feature={feature} value={text} min={min} max={Number((max * factor).toFixed(3))} step={step} title={title}
        onChange={(e) => {
          const input = e.target.value;
          setText(input);
          if (timer.current) clearTimeout(timer.current);
          const next = storedValue(input, value, factor);
          if (!e.target.validity.badInput && next !== undefined && next !== value &&
              (next === null || (next >= min / factor && next <= max))) {
            timer.current = setTimeout(() => callback.current(next), 120);
          }
        }} onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} />
    </label>
  );
}

/**
 * A UTC date and time that applies when complete. An unreadable entry
 * goes back to the stored value on blur.
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
    <label className="track-field track-time-field">
      {label}
      <input type="datetime-local" step={1} data-feature={feature} value={text} title={title}
        onChange={(e) => {
          const text = e.target.value;
          setText(text);
          const next = fromLocalInput(text);
          if (next !== value && (next !== null || text === "")) onCommit(next);
        }} onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} />
    </label>
  );
}

function TrackFiltersEditor({ id, track, onProject, units }: {
  id: number; track: TrackSummary; onProject: (project: ProjectSummary) => void; units: Units;
}) {
  const api = useBoatApi();
  return <SampleFiltersEditor filters={track.filters} track={track} units={units}
    onChange={(filters) => api.setTrackFilters(id, filters).then(onProject)} />;
}

/** Stable feature families shared by individual, global and priority filters. */
function filterFeature(prefix: string, name: string): string { return `${prefix}:${name}`; }

export function SampleFiltersEditor({ filters, onChange, units, track, prefix = "tracks" }: {
  filters: TrackFilters; onChange: (filters: TrackFilters) => Promise<unknown>; units: Units;
  track?: TrackSummary; prefix?: "tracks" | "global-filters" | "priority-filters";
}) {
  const t = useT();
  const [intervalUnit, setIntervalUnit] = useState("seconds");
  const speed = speedUnit(units);
  const wave = waveUnit(units);
  const [f, update] = useLiveEdit(filters, onChange);
  const set = (change: Partial<TrackFilters>) => { void update((previous) => ({ ...previous, ...change })); };
  const origins = [
    { id: "any", label: t("Given or derived") },
    { id: "given", label: t("Given only") },
    { id: "derived", label: t("Derived only") },
  ];
  const needsWind = t("These filters read the environment, which this track does not have yet; with one set, samples without it are left out.");
  return (
    <fieldset className="track-filters">
      <legend>{t("Sample filters")}</legend>
      {track && <p className="muted">
        {t("{filtered} of {count} samples filtered out; they stay in the project and show dimmed.", { filtered: track.filtered, count: track.samples })}
      </p>}
      {prefix === "tracks" && <TimeField feature={filterFeature(prefix, "time-start")} label={t("From (UTC)")} value={f.time_start}
        title={t("Leave out samples before this time, such as before the start")}
        onCommit={(v) => set({ time_start: v })} />}
      {prefix === "tracks" && <TimeField feature={filterFeature(prefix, "time-end")} label={t("To (UTC)")} value={f.time_end}
        title={t("Leave out samples after this time, such as after the finish")}
        onCommit={(v) => set({ time_end: v })} />}
      <NumberField feature={filterFeature(prefix, "min-bsp")} label={t("Minimum BSP ({unit})", { unit: speed.symbol })} value={f.min_bsp} min={0} max={100} step={0.5} factor={speed.factor}
        title={t("Leave out samples slower than this; empty for no minimum")} onCommit={(v) => set({ min_bsp: v })} />
      <NumberField feature={filterFeature(prefix, "max-bsp")} label={t("Maximum BSP ({unit})", { unit: speed.symbol })} value={f.max_bsp} min={0} max={100} step={0.5} factor={speed.factor}
        title={t("Leave out samples faster than this; empty for no maximum")} onCommit={(v) => set({ max_bsp: v })} />
      <NumberField feature={filterFeature(prefix, "manoeuvre")} label={t("Direction change (°)")} value={f.max_heading_change} min={1} max={180} step={5}
        title={t("Exclude points when heading changes by more than this from the previous or next point. Empty disables the filter.")}
        onCommit={(v) => set({ max_heading_change: v })} />
      <NumberField feature={filterFeature(prefix, "awa-change")} label={t("AWA change (°)")} value={f.max_awa_change} min={0} max={180} step={5}
        title={t("Exclude points when apparent wind angle changes by more than this from the previous or next point.")} onCommit={(v) => set({ max_awa_change: v })} />
      <NumberField feature={filterFeature(prefix, "wind-speed-change")} label={t("Wind speed change ({unit})", { unit: speed.symbol })} value={f.max_wind_speed_change} min={0} max={200} step={0.5} factor={speed.factor}
        title={t("Exclude points when true wind speed changes by more than this from the previous or next point.")} onCommit={(v) => set({ max_wind_speed_change: v })} />
      <NumberField feature={filterFeature(prefix, "wind-direction-change")} label={t("Wind direction change (°)")} value={f.max_wind_direction_change} min={0} max={180} step={5}
        title={t("Exclude points when true wind direction changes by more than this from the previous or next point.")} onCommit={(v) => set({ max_wind_direction_change: v })} />
      <NumberField feature={filterFeature(prefix, "tack-window")} label={t("Tack/gybe window (s)")} value={f.tack_gybe_padding_s} min={0} max={86400} step={60}
        title={t("Exclude points before and after a tack or gybe. Empty disables the filter.")} onCommit={(v) => set({ tack_gybe_padding_s: v })} />
      <NumberField feature={filterFeature(prefix, "stop-speed")} label={t("Stop speed ({unit})", { unit: speed.symbol })} value={f.stop_speed_kn} min={0} max={100} step={0.1} factor={speed.factor}
        title={t("Treat ground speeds at or below this as stops. Empty disables the filter.")} onCommit={(v) => set({ stop_speed_kn: v })} />
      <NumberField feature={filterFeature(prefix, "stop-window")} label={t("Stop window (s)")} value={f.stop_padding_s} min={0} max={86400} step={60}
        title={t("Exclude points this many seconds before and after each stop.")} onCommit={(v) => set({ stop_padding_s: v ?? 0 })} />
      <NumberField feature={filterFeature(prefix, "utc-interval")} label={intervalUnit === "minutes" ? t("Timestamp interval (min)") : t("Timestamp interval (s)")} value={f.utc_interval_s} min={intervalUnit === "minutes" ? 1 / 60 : 1} max={86400} step={1} factor={intervalUnit === "minutes" ? 1 / 60 : 1}
        title={t("Keep times aligned to this interval from UTC midnight: 60 for minutes, 3600 for hours. Empty keeps every time.")} onCommit={(v) => set({ utc_interval_s: v === null ? null : Math.round(v) })} />
      <label className="track-field track-choice-field">{t("Interval unit")}
        <select data-feature={filterFeature(prefix, "utc-unit")} value={intervalUnit} onChange={(e) => setIntervalUnit(e.target.value)}>
          <option value="seconds">{t("Seconds")}</option><option value="minutes">{t("Minutes")}</option>
        </select>
      </label>
      <label className="track-field track-choice-field">
        {t("Heading")}
        <select data-feature={filterFeature(prefix, "heading-origin")} value={f.heading_origin}
          title={t("Keep samples whose heading the track gave, was derived, or either")}
          onChange={(e) => set({ heading_origin: e.target.value })}>
          {origins.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
        </select>
      </label>
      <label className="track-field track-choice-field">
        {t("Speed")}
        <select data-feature={filterFeature(prefix, "speed-origin")} value={f.speed_origin}
          title={t("Keep samples whose speed the track gave, was derived, or either")}
          onChange={(e) => set({ speed_origin: e.target.value })}>
          {origins.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
        </select>
      </label>
      <fieldset className="track-env-filters">
        <legend>{t("Wind, waves and current")}</legend>
        {track?.env_status === "not_fetched" && track.supplied_wind === 0 && <p className="muted">{needsWind}</p>}
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "unknown-wave")} checked={f.exclude_unknown_wave}
            onChange={(e) => set({ exclude_unknown_wave: e.target.checked })} />
          {t("Exclude unknown waves")}
        </label>
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "unknown-current")} checked={f.exclude_unknown_current}
            onChange={(e) => set({ exclude_unknown_current: e.target.checked })} />
          {t("Exclude unknown current")}
        </label>
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "tws-min")} label={t("TWS from ({unit})", { unit: speed.symbol })} value={f.tws_min} min={0} max={200} step={1} factor={speed.factor}
            title={t("Leave out samples in less wind than this; empty for no minimum")} onCommit={(v) => set({ tws_min: v })} />
          <NumberField feature={filterFeature(prefix, "tws-max")} label={t("to ({unit})", { unit: speed.symbol })} value={f.tws_max} min={0} max={200} step={1} factor={speed.factor}
            title={t("Leave out samples in more wind than this; empty for no maximum")} onCommit={(v) => set({ tws_max: v })} />
        </div>
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "twa-min")} label={t("TWA from (°)")} value={f.twa_min} min={0} max={180} step={5}
            title={t("Leave out samples closer to the wind than this")} onCommit={(v) => set({ twa_min: v })} />
          <NumberField feature={filterFeature(prefix, "twa-max")} label={t("to (°)")} value={f.twa_max} min={0} max={180} step={5}
            title={t("Leave out samples further off the wind than this")} onCommit={(v) => set({ twa_max: v })} />
        </div>
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "hs-min")} label={t("Wave height from ({unit})", { unit: wave.symbol })} value={f.hs_min} min={0} max={100} step={0.5} factor={wave.factor}
            title={t("Leave out samples in smaller waves than this")} onCommit={(v) => set({ hs_min: v })} />
          <NumberField feature={filterFeature(prefix, "hs-max")} label={t("to ({unit})", { unit: wave.symbol })} value={f.hs_max} min={0} max={100} step={0.5} factor={wave.factor}
            title={t("Leave out samples in bigger waves than this")} onCommit={(v) => set({ hs_max: v })} />
        </div>
        <label className="track-field track-choice-field">
          {t("Wave direction")}
          <select data-feature={filterFeature(prefix, "wave-mode")} value={f.wave_mode}
            title={t("Keep samples by where the waves come from: off the bow by sector or angle, or by compass direction")}
            onChange={(e) => set({ wave_mode: e.target.value })}>
            <option value="off">{t("Any")}</option>
            <option value="sectors">{t("Sectors off the bow")}</option>
            <option value="relative">{t("Angle off the bow")}</option>
            <option value="cog">{t("Angle to COG")}</option>
            <option value="absolute">{t("Compass direction")}</option>
          </select>
        </label>
        {f.wave_mode === "sectors" && (
          <fieldset className="track-wave-sectors" data-feature={filterFeature(prefix, "wave-sectors")}>
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
        {(f.wave_mode === "relative" || f.wave_mode === "cog") && (
          <div className="track-range">
            <NumberField feature={filterFeature(prefix, "wave-min")} label={t("Off the bow from (°)")} value={f.wave_min} min={0} max={180} step={5}
              title={t("0° is head seas, 180° following seas")} onCommit={(v) => set({ wave_min: v })} />
            <NumberField feature={filterFeature(prefix, "wave-max")} label={t("to (°)")} value={f.wave_max} min={0} max={180} step={5}
              title={t("0° is head seas, 180° following seas")} onCommit={(v) => set({ wave_max: v })} />
          </div>
        )}
        {f.wave_mode === "absolute" && (
          <div className="track-range">
            <NumberField feature={filterFeature(prefix, "wave-from")} label={t("From the compass (°)")} value={f.wave_from} min={0} max={360} step={10}
              title={t("Waves coming from this direction, clockwise to the next")} onCommit={(v) => set({ wave_from: v })} />
            <NumberField feature={filterFeature(prefix, "wave-to")} label={t("to (°)")} value={f.wave_to} min={0} max={360} step={10}
              title={t("Waves coming from up to this direction")} onCommit={(v) => set({ wave_to: v })} />
          </div>
        )}
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "current-min")} label={t("Current from ({unit})", { unit: speed.symbol })} value={f.current_min} min={0} max={100} step={0.1} factor={speed.factor}
            title={t("Leave out samples in less current than this")} onCommit={(v) => set({ current_min: v })} />
          <NumberField feature={filterFeature(prefix, "current-max")} label={t("to ({unit})", { unit: speed.symbol })} value={f.current_max} min={0} max={100} step={0.1} factor={speed.factor}
            title={t("Leave out samples in more current than this")} onCommit={(v) => set({ current_max: v })} />
        </div>
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "no-tide")} checked={f.exclude_no_tide}
            title={t("Leave out samples whose current comes from a source without tides")}
            onChange={(e) => set({ exclude_no_tide: e.target.checked })} />
          {t("Leave out currents without tide ({count})", { count: track?.no_tide ?? 0 })}
        </label>
      </fieldset>
    </fieldset>
  );
}

function DerivationEditor({ id, track, onProject }: {
  id: number; track: TrackSummary; onProject: (project: ProjectSummary) => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const set = (maxGapS: number, prefer: "given" | "derived") => {
    api.setTrackDerivation(id, maxGapS, prefer).then(onProject).catch(reportFailure);
  };
  const prefer = track.prefer === "derived" ? "derived" : "given";
  return (
    <fieldset className="track-filters">
      <legend>{t("Heading and speed")}</legend>
      <label className="track-field track-choice-field">
        {t("Wind source")}
        <select data-feature="tracks:wind-source" value={track.downloaded_wind_only ? "weather" : "supplied"}
          title={t("Supplied wind is used where both speed and direction are available; other points use downloaded weather.")}
          onChange={(event) => { void api.setTrackWind(id, event.target.value === "weather").then(onProject).catch(reportFailure); }}>
          <option value="supplied">{t("Supplied wind where available")}</option>
          <option value="weather">{t("Downloaded weather only")}</option>
        </select>
      </label>
      <p className="muted">{t("{count} points have supplied wind.", { count: track.supplied_wind ?? 0 })}</p>
      <NumberField feature="tracks:max-gap" label={t("Maximum gap (h)")} value={track.max_gap_s / 3600} min={0.01} max={24} step={0.5}
        title={t("Neighbours further apart in time than this are not used to derive heading and speed")}
        onCommit={(v) => { if (v !== null && v > 0) set(Math.round(v * 3600), prefer); }} />
      <label className="track-field track-choice-field">
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
