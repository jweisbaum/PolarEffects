/**
 * The typed command surface.
 *
 * Types under `./generated` are produced from the Rust definitions by
 * `npm run bindings` and must never be hand-edited. This module is the only
 * place `invoke` is called, so every IPC failure is normalised into one type.
 */
import { invoke } from "@tauri-apps/api/core";

import type { AppErrorPayload } from "./generated/AppErrorPayload";
import type { AppInfo } from "./generated/AppInfo";
import type { BoatInput } from "./generated/BoatInput";
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

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isErrorPayload(raw)) throw new IpcError(raw);
    // A command that panicked, or a Tauri-level failure, arrives as a bare string.
    throw new IpcError({ kind: "unknown", message: String(raw) });
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
};
