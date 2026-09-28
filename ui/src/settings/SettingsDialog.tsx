/**
 * The Settings dialog (spec.md 3.4). Copied in structure from VectorEffects:
 * one section per group, each change saved at once through Rust, which
 * validates it and answers with the settings as saved — so what is shown is
 * always what is on disk.
 */

import { useEffect, useState } from "react";

import type { AppSettings } from "../generated/AppSettings";
import type { AutosaveMode } from "../generated/AutosaveMode";
import type { ChunkCacheStatus } from "../generated/ChunkCacheStatus";
import type { Units } from "../generated/Units";
import { onReveal } from "../help/highlight";
import { useT } from "../i18n";
import LanguagePicker from "../i18n/LanguagePicker";
import { describeError } from "../errors";
import { api } from "../ipc";
import ConfirmDialog from "../project/ConfirmDialog";
import { pickCacheFolder } from "../project/dialogs";
import ThemePicker from "./ThemePicker";

/** A byte count as the dialog shows it. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(0)} kB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/**
 * A whole number typed in and committed on Enter or on leaving the field,
 * so a half-typed value is never saved.
 */
function IntegerField({ value, min, max, onCommit, ...rest }: {
  value: number; min: number; max: number; onCommit: (value: number) => void;
  "data-feature": string; title: string; "aria-label": string;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const parsed = Number(text);
    if (Number.isInteger(parsed) && parsed >= min && parsed <= max) {
      if (parsed !== value) onCommit(parsed);
    } else {
      setText(String(value));
    }
  };
  return <input type="number" inputMode="numeric" min={min} max={max} step={1} value={text} {...rest}
    onChange={(event) => setText(event.target.value)} onBlur={commit}
    onKeyDown={(event) => { if (event.key === "Enter") commit(); }} />;
}

/** The dot bands offered, knots either side of the plot's wind speed (spec.md 9.2). */
const PLOT_BANDS = [0.25, 0.5, 1, 1.5, 2, 3, 5];

export default function SettingsDialog({ settings, onSettings, onClose }: {
  settings: AppSettings;
  onSettings: (settings: AppSettings) => void;
  onClose: () => void;
}) {
  const t = useT();
  /** The last failure, described at render so a language switch relabels it. */
  const [error, setError] = useState<unknown>(null);
  const [cache, setCache] = useState<ChunkCacheStatus | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const report = (err: unknown) => setError(err ?? new Error("unknown"));
  const save = (change: Promise<AppSettings>) => {
    setError(null);
    change.then(onSettings).catch(report);
  };

  useEffect(() => {
    let live = true;
    api.chunkCacheStatus().then((status) => { if (live) setCache(status); }).catch(() => undefined);
    return () => { live = false; };
  }, [settings.chunk_cache.location]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !confirmClear) {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, confirmClear]);

  /**
   * The search's second reveal step (spec.md 3.6): `settings:` opened the
   * dialog, and `settings:<section>` brings that section into view. Each is
   * written out, not looped over, so the registry's test can read them.
   */
  useEffect(() => {
    const show = (step: string) => {
      document.querySelector(`[data-section="${step}"]`)?.scrollIntoView?.({ block: "start" });
    };
    const offs = [
      onReveal("settings:appearance", show),
      onReveal("settings:units", show),
      onReveal("settings:autosave", show),
      onReveal("settings:cache", show),
      onReveal("settings:network", show),
    ];
    return () => { for (const off of offs) off(); };
  }, []);

  const units = (change: Partial<Units>) => save(api.setUnits({ ...settings.units, ...change }));
  const chunkCache = settings.chunk_cache;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal settings" role="dialog" aria-label={t("Settings")} onClick={(event) => event.stopPropagation()}>
        <h2>{t("Settings")}</h2>
        {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}

        <section data-section="settings:appearance">
          <h3>{t("Appearance")}</h3>
          <ThemePicker value={settings.theme} onChoose={(id) => save(api.setTheme(id))} />
          <label className="settings-field">
            {t("Language")}
            <LanguagePicker feature="settings:language" onSettings={onSettings} />
          </label>
          <p className="muted">{t("Applies throughout the app, across all projects.")}</p>
        </section>

        <section data-section="settings:units">
          <h3>{t("Units")}</h3>
          <label className="settings-field">
            {t("Boat and wind speed")}
            <select data-feature="settings:speed-unit" value={settings.units.speed}
              title={t("The unit speeds are shown in. Polars are stored in knots.")}
              onChange={(e) => units({ speed: e.target.value as Units["speed"] })}>
              <option value="kn">{t("kn — knots")}</option>
              <option value="ms">{t("m/s — metres per second")}</option>
              <option value="kmh">{t("km/h — kilometres per hour")}</option>
            </select>
          </label>
          <label className="settings-field">
            {t("Wave height")}
            <select data-feature="settings:wave-unit" value={settings.units.wave_height}
              title={t("The unit wave heights are shown in")}
              onChange={(e) => units({ wave_height: e.target.value as Units["wave_height"] })}>
              <option value="m">{t("m — metres")}</option>
              <option value="ft">{t("ft — feet")}</option>
            </select>
          </label>
          <label className="settings-field">
            {t("Distance")}
            <select data-feature="settings:distance-unit" value={settings.units.distance}
              title={t("The unit distances are shown in")}
              onChange={(e) => units({ distance: e.target.value as Units["distance"] })}>
              <option value="nm">{t("nm — nautical miles")}</option>
              <option value="km">{t("km — kilometres")}</option>
            </select>
          </label>
          <label className="settings-field">
            {t("Polar plot dot band")}
            <select data-feature="settings:plot-band" value={String(settings.plot_tws_band_kn)}
              title={t("How far from the polar plot's wind speed a track sample may be and still be drawn")}
              onChange={(e) => save(api.setPlotBand(Number(e.target.value)))}>
              {PLOT_BANDS.map((band) => (
                <option key={band} value={String(band)}>{t("{band} kn either side", { band })}</option>
              ))}
            </select>
          </label>
        </section>

        <section data-section="settings:autosave">
          <h3>{t("Autosave")}</h3>
          <label className="settings-field">
            {t("Unsaved work")}
            <select data-feature="settings:autosave" value={settings.autosave}
              title={t("Every minute, or every fifty edits, whichever comes first")}
              onChange={(e) => save(api.setAutosaveMode(e.target.value as AutosaveMode))}>
              <option value="recovery">{t("Keep a recovery copy, offered back after a crash")}</option>
              <option value="save">{t("Save into the project file itself")}</option>
              <option value="off">{t("Leave it until you save")}</option>
            </select>
          </label>
        </section>

        <section data-section="settings:cache">
          <h3>{t("Chunk cache")}</h3>
          <p className="muted">
            {t("Downloaded wind, wave and current data is kept here so it is not fetched twice. Clearing it loses nothing: every value a project uses is saved in the project.")}
          </p>
          <div className="settings-field">
            <span>{t("Location")}</span>
            <span className="settings-path" title={cache?.path ?? undefined}>{cache?.path ?? "…"}</span>
          </div>
          <div className="settings-buttons">
            <button data-feature="settings:cache-location" title={t("Keep the chunk cache in another folder")}
              onClick={() => void pickCacheFolder().then((folder) => {
                if (folder !== null) save(api.setChunkCache({ ...chunkCache, location: folder }));
              }).catch(report)}>
              {t("Choose folder…")}
            </button>
            <button data-feature="settings:cache-default" disabled={chunkCache.location === ""}
              title={t("Keep the chunk cache in the application's own cache folder")}
              onClick={() => save(api.setChunkCache({ ...chunkCache, location: "" }))}>
              {t("Use the default folder")}
            </button>
          </div>
          <label className="settings-field">
            {t("Size limit (GB)")}
            <IntegerField data-feature="settings:cache-limit" aria-label={t("Size limit (GB)")}
              title={t("The oldest chunks are removed when the cache grows past this")}
              value={chunkCache.size_limit_gb} min={1} max={2000}
              onCommit={(value) => save(api.setChunkCache({ ...chunkCache, size_limit_gb: value }))} />
          </label>
          <div className="settings-field">
            <span>{t("Current size: {size}", { size: cache === null ? "…" : formatBytes(cache.bytes) })}</span>
            <button data-feature="settings:cache-clear" disabled={cache === null || cache.bytes === 0}
              title={t("Delete every cached chunk. Projects keep their data.")}
              onClick={() => setConfirmClear(true)}>
              {t("Clear cache")}
            </button>
          </div>
        </section>

        <section data-section="settings:network">
          <h3>{t("Network")}</h3>
          <label className="settings-field">
            {t("Concurrent requests")}
            <IntegerField data-feature="settings:concurrency" aria-label={t("Concurrent requests")}
              title={t("How many downloads run at once (1–32)")}
              value={settings.network.concurrency} min={1} max={32}
              onCommit={(value) => save(api.setNetwork({ ...settings.network, concurrency: value }))} />
          </label>
          <label className="settings-field">
            {t("Request timeout (s)")}
            <IntegerField data-feature="settings:timeout" aria-label={t("Request timeout (s)")}
              title={t("How long one download may take before it is abandoned (5–600 s)")}
              value={settings.network.timeout_s} min={5} max={600}
              onCommit={(value) => save(api.setNetwork({ ...settings.network, timeout_s: value }))} />
          </label>
        </section>

        <div className="modal-actions">
          <button data-feature="settings:close" onClick={onClose} title={t("Close the settings (Esc)")}>{t("Close")}</button>
        </div>
      </div>

      {confirmClear && (
        <div onClick={(event) => event.stopPropagation()}>
          <ConfirmDialog
            title={t("Clear the chunk cache?")}
            body={t("This deletes {size} of downloaded data. Projects keep every value they use; data is fetched again only if you ask for it.", { size: formatBytes(cache?.bytes ?? 0) })}
            confirmLabel={t("Clear cache")}
            onConfirm={() => {
              setConfirmClear(false);
              setError(null);
              api.clearChunkCache().then(setCache).catch(report);
            }}
            onCancel={() => setConfirmClear(false)}
          />
        </div>
      )}
    </div>
  );
}
