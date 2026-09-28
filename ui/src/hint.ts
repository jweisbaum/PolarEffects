/**
 * The status bar's one line: a hint, or the last error, whichever is newer
 * (spec.md 3.2). Copied from VectorEffects.
 *
 * A store outside React: every panel writes to it, and only the status bar's
 * span subscribes, so a hint or an error re-renders one span rather than the
 * shell.
 */

import { useSyncExternalStore } from "react";

export interface HintSnapshot {
  /** What the control under the pointer would like the user to know. */
  hint: string | null;
  /** The last error, until the next hint or a clear. */
  error: string | null;
  /**
   * The error's `kind` discriminant: shown as the line's tooltip for a bug
   * report, never in the line (a `[bad-option]` reads as machine trouble).
   */
  errorKind: string | null;
}

let snapshot: HintSnapshot = { hint: null, error: null, errorKind: null };
const listeners = new Set<() => void>();

function publish(next: HintSnapshot) {
  if (next.hint === snapshot.hint && next.error === snapshot.error && next.errorKind === snapshot.errorKind) return;
  snapshot = next;
  for (const listener of listeners) listener();
}

/**
 * Sets the hint. The error stands until the next hint *change*: a refusal is
 * more recent than the hint it interrupted.
 */
export function setHint(hint: string | null): void {
  const keep = hint === snapshot.hint;
  publish({ hint, error: keep ? snapshot.error : null, errorKind: keep ? snapshot.errorKind : null });
}

/** Reports an error, which shows in place of the hint until the hint changes. */
export function reportError(error: string | null, kind: string | null = null): void {
  publish({ ...snapshot, error, errorKind: error === null ? null : kind });
}

/** What the status bar shows: the error if there is one, else the hint. */
export function shown(state: HintSnapshot): { text: string; kind: "error" | "hint"; detail: string | null } | null {
  if (state.error !== null) return { text: state.error, kind: "error", detail: state.errorKind };
  if (state.hint !== null) return { text: state.hint, kind: "hint", detail: null };
  return null;
}

/** The current snapshot, for the one component that renders it. */
export function useHint(): HintSnapshot {
  return useSyncExternalStore(subscribe, () => snapshot);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** For tests: the snapshot as it stands. */
export function currentHint(): HintSnapshot {
  return snapshot;
}
