/**
 * The interface following the MCP service (spec.md 3.7).
 *
 * The service emits `document://changed` after every tool that wrote to the
 * document or opened, saved or closed a project. This applies it the way
 * the shell applies the result of its own call: an edit replaces the
 * summary and the panels refresh by revision; a different project is
 * entered, which resets what opening a project resets.
 */
import type { DocumentChanged } from "../generated/DocumentChanged";
import type { ProjectSummary } from "../generated/ProjectSummary";

export interface FollowActions {
  /** The id of the project the shell holds; null on the start screen. */
  currentId: number | null;
  /** The shell's own way into a project, or back to the start screen for null. */
  enter: (project: ProjectSummary | null) => void;
  /** The shell's own way of taking a newer summary of the project it holds. */
  update: (project: ProjectSummary) => void;
}

export function applyDocumentChanged(payload: DocumentChanged, actions: FollowActions): void {
  const { project } = payload;
  // Entered rather than updated whenever the shell does not already hold
  // this project: it opened, it closed, the file's first boat changed (the
  // summary is then another boat's), or the frontend holds nothing yet.
  if (payload.opened || project === null || project.id !== actions.currentId) {
    actions.enter(project);
    return;
  }
  actions.update(project);
}

const boatListeners = new Set<(boat: number) => void>();

/** Asks whoever shows the boat tabs to show one (`view://boat`). */
export function showBoat(boat: number): void {
  for (const listener of boatListeners) listener(boat);
}

/** Hears `showBoat`. Returns the unregistration, for an effect's cleanup. */
export function onShowBoat(listener: (boat: number) => void): () => void {
  boatListeners.add(listener);
  return () => { boatListeners.delete(listener); };
}

/** A stage the MCP service names: the three stages, or the full-size plot. */
export type ShownStage = "3d" | "map" | "compare" | "plot";

const stageListeners = new Map<number, (stage: ShownStage) => void>();
/** A stage for a boat whose view is not mounted yet, and when it was asked. */
let waitingStage: { boat: number; stage: ShownStage; at: number } | null = null;
/** How long a stage waits for its boat's view: the time a tab takes to load, not a session. */
const STAGE_WAIT_MS = 5000;

/**
 * Sets one boat's stage (`view://stage`). Each boat's view holds a stage of
 * its own, so the request goes to that boat's view whichever is on show;
 * for a boat whose tab was never opened it waits for the view to mount.
 */
export function showStage(boat: number, stage: ShownStage): void {
  const listener = stageListeners.get(boat);
  if (listener) listener(stage);
  else waitingStage = { boat, stage, at: Date.now() };
}

/** A boat's view hears `showStage` for its boat. Returns the unregistration. */
export function onShowStage(boat: number, listener: (stage: ShownStage) => void): () => void {
  stageListeners.set(boat, listener);
  if (waitingStage?.boat === boat) {
    const { stage, at } = waitingStage;
    waitingStage = null;
    if (Date.now() - at < STAGE_WAIT_MS) listener(stage);
  }
  return () => { if (stageListeners.get(boat) === listener) stageListeners.delete(boat); };
}
