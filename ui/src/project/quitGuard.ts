/**
 * Quitting through the save guard (spec.md 3.3).
 *
 * Rust stops every way out — the Quit item, Cmd/Ctrl-Q, the window's close
 * button, the platform's own quit — while the open project has unsaved
 * changes, and asks the frontend. This answers: it runs the same
 * `mayReplaceProject` decision New, Open and Close use, and only then tells
 * Rust to exit, carrying the "Don't save" answer so Rust can check it.
 */

import { mayReplaceProject, type UnsavedChoice } from "./saveGuard";

/** What quitting needs from the shell, so the decision is testable alone. */
export interface QuitDeps {
  /** The open project as Rust has it now, not as the view last saw it. */
  current: () => Promise<{ dirty: boolean } | null>;
  ask: () => Promise<UnsavedChoice>;
  /** Saves (Save As for a never-saved project); false if it did not complete. */
  save: () => Promise<boolean>;
  quit: (discardUnsaved: boolean) => Promise<void>;
}

/** Runs the guard and quits if it allows; returns whether it quit. */
export async function quitThroughGuard(deps: QuitDeps): Promise<boolean> {
  const project = await deps.current();
  const decision = await mayReplaceProject(project, deps.ask, deps.save);
  if (!decision.proceed) return false;
  await deps.quit(decision.discardUnsaved);
  return true;
}
