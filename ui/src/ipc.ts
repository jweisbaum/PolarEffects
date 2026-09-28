/**
 * The typed command surface.
 *
 * Types under `./generated` are produced from the Rust definitions by
 * `npm run bindings` and must never be hand-edited. This module is the only
 * place `invoke` is called, so every IPC failure is normalised into one type.
 */
import { invoke } from "@tauri-apps/api/core";

import { beginBusy } from "./busy";
import { msg } from "./i18n/msg";
import type { AppErrorPayload } from "./generated/AppErrorPayload";
import type { AppInfo } from "./generated/AppInfo";
import type { AppSettings } from "./generated/AppSettings";
import type { AutosaveMode } from "./generated/AutosaveMode";
import type { BoatInput } from "./generated/BoatInput";
import type { ChunkCacheSettings } from "./generated/ChunkCacheSettings";
import type { ChunkCacheStatus } from "./generated/ChunkCacheStatus";
import type { MapProjection } from "./generated/MapProjection";
import type { NetworkSettings } from "./generated/NetworkSettings";
import type { OrcCatalogueInfo } from "./generated/OrcCatalogueInfo";
import type { OrcFilters } from "./generated/OrcFilters";
import type { OrcSearchResult } from "./generated/OrcSearchResult";
import type { PolarImportResult } from "./generated/PolarImportResult";
import type { Units } from "./generated/Units";
import type { ProjectSummary } from "./generated/ProjectSummary";
import type { RecentProject } from "./generated/RecentProject";
import type { RecoveredProject } from "./generated/RecoveredProject";

/** An error raised by a Rust command, carrying its machine-readable kind. */
export class IpcError extends Error {
  readonly kind: string;

  constructor(payload: AppErrorPayload) {
    super(payload.message);
    this.name = "IpcError";
    this.kind = payload.kind;
  }
}

export function isErrorPayload(value: unknown): value is AppErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppErrorPayload).kind === "string" &&
    typeof (value as AppErrorPayload).message === "string"
  );
}

/**
 * The commands that can take long enough to want the status bar's spinner,
 * with what it says while they run. Callers do nothing; the name is enough.
 */
const LONG_RUNNING: Readonly<Record<string, string>> = {
  open_project: msg("Opening project"),
  open_recovered: msg("Recovering project"),
  save_project: msg("Saving"),
  save_project_as: msg("Saving"),
  chunk_cache_status: msg("Measuring the cache"),
  clear_chunk_cache: msg("Clearing the cache"),
  import_polar_files: msg("Importing polar files"),
  orc_catalogue_info: msg("Loading the ORC catalogue"),
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const label = LONG_RUNNING[command];
  const done = label === undefined ? null : beginBusy(label);
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isErrorPayload(raw)) throw new IpcError(raw);
    // A command that panicked, or a Tauri-level failure, arrives as a bare string.
    throw new IpcError({ kind: "unknown", message: String(raw) });
  } finally {
    done?.();
  }
}

/** Every Rust command reachable from the frontend. */
export const api = {
  /** Product name and version, for the start screen and About panel. */
  appInfo: () => call<AppInfo>("app_info"),

  // Project lifecycle (spec.md 3.3, 4.2–4.5). Every call that drops the open
  // project takes the user's answer to "Save / Don't save / Cancel" as
  // `discardUnsaved`; without it, Rust refuses a dirty project with kind
  // "unsaved-changes".

  /** Creates a project and makes it the open one. */
  newProject: (name: string, boat: BoatInput | null = null, discardUnsaved = false) =>
    call<ProjectSummary>("new_project", { name, boat, discardUnsaved }),
  /** Opens a `.wpsproj`. A newer file fails with kind "schema-too-new". */
  openProject: (path: string, discardUnsaved = false) =>
    call<ProjectSummary>("open_project", { path, discardUnsaved }),
  /** Saves to the project's file; kind "never-saved" means ask for Save As. */
  saveProject: () => call<ProjectSummary>("save_project"),
  /** Saves to a new file, adding `.wpsproj` if missing. */
  saveProjectAs: (path: string) => call<ProjectSummary>("save_project_as", { path }),
  /** Closes the open project. */
  closeProject: (discardUnsaved = false) => call<void>("close_project", { discardUnsaved }),
  /** The open project, or null on the start screen. */
  projectSummary: () => call<ProjectSummary | null>("project_summary"),
  /** The ten most recent projects, newest first, with missing files marked. */
  recentProjects: () => call<RecentProject[]>("recent_projects"),
  /** Drops one entry from the recent list. */
  forgetRecentProject: (path: string) =>
    call<RecentProject[]>("forget_recent_project", { path }),
  /** Empties the recent list. */
  clearRecent: () => call<RecentProject[]>("clear_recent"),

  // History (spec.md 4.6).

  /** Reverses the last change. */
  undo: () => call<ProjectSummary>("undo"),
  /** Reapplies the last undone change. */
  redo: () => call<ProjectSummary>("redo"),
  /** Renames the open project (undoable). */
  renameProject: (name: string) => call<ProjectSummary>("rename_project", { name }),

  // Crash recovery (spec.md 4.5).

  /** Autosaves left by a crash, newest first. */
  recoveredProjects: () => call<RecoveredProject[]>("recovered_projects"),
  /** Opens a recovered autosave as its original project, dirty. */
  openRecovered: (id: number, discardUnsaved = false) =>
    call<ProjectSummary>("open_recovered", { id, discardUnsaved }),
  /** Deletes a recovered autosave. */
  discardRecovered: (id: number) => call<RecoveredProject[]>("discard_recovered", { id }),

  // The source list (spec.md 8). Each is one undoable change.

  /** Sets a source's colour, `#rrggbb`. */
  setSourceColour: (id: number, colour: string) =>
    call<ProjectSummary>("set_source_colour", { id, colour }),
  /** Shows or hides a source; hidden sources leave the blend and every plot. */
  setSourceVisible: (id: number, visible: boolean) =>
    call<ProjectSummary>("set_source_visible", { id, visible }),
  /**
   * Sets a source's blend weight (0–2). Calls sharing a `gesture` name, one
   * after another, are one undo entry: a slider drag.
   */
  setSourceWeight: (id: number, weight: number, gesture: string | null = null) =>
    call<ProjectSummary>("set_source_weight", { id, weight, gesture }),
  /** Renames a source. */
  setSourceLabel: (id: number, label: string) =>
    call<ProjectSummary>("set_source_label", { id, label }),
  /** Moves a source to position `to` (display order only). */
  moveSource: (id: number, to: number) => call<ProjectSummary>("move_source", { id, to }),
  /** Removes a source; undo puts it back. */
  removeSource: (id: number) => call<ProjectSummary>("remove_source", { id }),

  // Polar files (spec.md 6).

  /**
   * Parses and imports polar files, one source per file, as one undoable
   * change. A file that fails is listed with its line, column and a reason
   * code; the others still import.
   */
  importPolarFiles: (paths: string[]) => call<PolarImportResult>("import_polar_files", { paths }),

  // ORC polars (spec.md 5). The catalogue is built into the app.

  /** The catalogue's size, provenance, countries and years; loads it on first call. */
  orcCatalogueInfo: () => call<OrcCatalogueInfo>("orc_catalogue_info"),
  /** Searches the catalogue: every word must match; best first, at most `limit`. */
  orcSearch: (query: string, filters: OrcFilters, limit: number) =>
    call<OrcSearchResult>("orc_search", { query, filters, limit }),
  /**
   * Adds a certificate as an ORC source (undoable). One the project already
   * holds fails with kind "orc-duplicate" unless `allowDuplicate`.
   */
  orcAdd: (id: number, allowDuplicate = false) => call<ProjectSummary>("orc_add", { id, allowDuplicate }),

  // Settings (spec.md 3.4). Each returns the settings as saved.

  /** The settings. */
  appSettings: () => call<AppSettings>("app_settings"),
  /** Sets the interface language; the native menu follows. */
  setLanguage: (language: string) => call<AppSettings>("set_language", { language }),
  /** Sets the theme. */
  setTheme: (theme: string) => call<AppSettings>("set_theme", { theme }),
  /** Sets the display units. */
  setUnits: (units: Units) => call<AppSettings>("set_units", { units }),
  /** Sets what autosave does. */
  setAutosaveMode: (mode: AutosaveMode) => call<AppSettings>("set_autosave_mode", { mode }),
  /** Sets the chunk cache's folder (empty for the default) and size limit. */
  setChunkCache: (cache: ChunkCacheSettings) => call<AppSettings>("set_chunk_cache", { cache }),
  /** Sets the reanalysis fetcher's concurrency and timeout. */
  setNetwork: (network: NetworkSettings) => call<AppSettings>("set_network", { network }),
  /** Sets the map projection. */
  setProjection: (projection: MapProjection) => call<AppSettings>("set_projection", { projection }),
  /** Where the chunk cache is and how big it is. */
  chunkCacheStatus: () => call<ChunkCacheStatus>("chunk_cache_status"),
  /** Empties the chunk cache (lossless: samples live in projects). */
  clearChunkCache: () => call<ChunkCacheStatus>("clear_chunk_cache"),

  // The map (spec.md 9.1).

  /** The bundled basemap asset, as raw bytes. */
  basemap: async (): Promise<ArrayBuffer> => {
    const bytes = await call<ArrayBuffer | number[]>("basemap");
    // Raw responses arrive as an ArrayBuffer; older shapes as a number array.
    return bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer;
  },

  // Quitting (spec.md 3.3).

  /**
   * Quits, after the unsaved-changes guard. Rust refuses with
   * "unsaved-changes" unless the project is clean or `discardUnsaved`.
   */
  quitApp: (discardUnsaved: boolean) => call<void>("quit_app", { discardUnsaved }),
};

/** The event Rust sends when the user asks to quit or close the window. */
export const QUIT_REQUESTED = "app://quit-requested";
