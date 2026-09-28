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
   * What a bug report would want and a reader would not — the backend's
   * `kind` and its English message: the line's tooltip, never the line.
   */
  errorDetail: string | null;
  /**
   * The failure itself, when it came through `reportFailure`: the status bar
   * describes it again at every render, so a language switch relabels it.
   */
  failure?: unknown;
}

let snapshot: HintSnapshot = { hint: null, error: null, errorDetail: null };
const listeners = new Set<() => void>();

function publish(next: HintSnapshot) {
  if (next.hint === snapshot.hint && next.error === snapshot.error && next.errorDetail === snapshot.errorDetail
    && next.failure === snapshot.failure) return;
  snapshot = next;
  for (const listener of listeners) listener();
}

/**
 * Sets the hint. The error stands until the next hint *change*: a refusal is
 * more recent than the hint it interrupted.
 */
export function setHint(hint: string | null): void {
  const keep = hint === snapshot.hint;
  publish({ hint, error: keep ? snapshot.error : null, errorDetail: keep ? snapshot.errorDetail : null,
    failure: keep ? snapshot.failure : undefined });
}

/** Reports an error, which shows in place of the hint until the hint changes. */
export function reportError(error: string | null, detail: string | null = null, failure?: unknown): void {
  publish({ ...snapshot, error, errorDetail: error === null ? null : detail, failure: error === null ? undefined : failure });
}

/** What the status bar shows: the error if there is one, else the hint. */
export function shown(state: HintSnapshot): { text: string; kind: "error" | "hint"; detail: string | null } | null {
  if (state.error !== null) return { text: state.error, kind: "error", detail: state.errorDetail };
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
