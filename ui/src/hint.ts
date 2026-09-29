/**
 * The status bar's one line: a hint, or the last error, whichever is newer
 * (spec.md 3.2). Copied from VectorEffects.
 *
 * A store outside React: every panel writes to it, and only the status bar's
 * span subscribes, so a hint or an error re-renders one span rather than the
 * shell.
 */

import { useSyncExternalStore } from "react";

import { t } from "./i18n";

/**
 * A line kept untranslated: its English key (marked with `msg`) and its
 * parameters, translated whenever it is shown, so a language switch
 * relabels a hint or an error already on the status line (spec.md 3.5). A
 * parameter that is itself text to translate (an undo label, a feature's
 * name) is a `Deferred` too.
 */
export interface Deferred {
  key: string;
  params?: Record<string, string | number | Deferred>;
}

/** Text for the status line: already final (a name, a path) or deferred. */
export type Line = string | Deferred;

/** Defers `t(key, params)` to the moment the line is drawn. */
export function later(key: string, params?: Deferred["params"]): Deferred {
  return params ? { key, params } : { key };
}

/** A line in the language on screen now. */
export function lineText(line: Line): string {
  if (typeof line === "string") return line;
  const params = line.params
    ? Object.fromEntries(Object.entries(line.params).map(([name, value]) =>
      [name, typeof value === "object" ? lineText(value) : value]))
    : undefined;
  return t(line.key, params);
}

/** Two lines say the same, whether or not they are the same object. */
function same(a: Line | null, b: Line | null): boolean {
  return a === b || (typeof a === "object" && typeof b === "object" && JSON.stringify(a) === JSON.stringify(b));
}

export interface HintSnapshot {
  /** What the control under the pointer would like the user to know. */
  hint: Line | null;
  /** The last error, until the next hint or a clear. */
  error: Line | null;
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
  if (same(next.hint, snapshot.hint) && same(next.error, snapshot.error) && next.errorDetail === snapshot.errorDetail
    && next.failure === snapshot.failure) return;
  snapshot = next;
  for (const listener of listeners) listener();
}

/**
 * Sets the hint. The error stands until the next hint *change*: a refusal is
 * more recent than the hint it interrupted.
 */
export function setHint(hint: Line | null): void {
  const keep = same(hint, snapshot.hint);
  publish({ hint, error: keep ? snapshot.error : null, errorDetail: keep ? snapshot.errorDetail : null,
    failure: keep ? snapshot.failure : undefined });
}

/** Reports an error, which shows in place of the hint until the hint changes. */
export function reportError(error: Line | null, detail: string | null = null, failure?: unknown): void {
  publish({ ...snapshot, error, errorDetail: error === null ? null : detail, failure: error === null ? undefined : failure });
}

/** What the status bar shows, in the language on screen: the error if there is one, else the hint. */
export function shown(state: HintSnapshot): { text: string; kind: "error" | "hint"; detail: string | null } | null {
  if (state.error !== null) return { text: lineText(state.error), kind: "error", detail: state.errorDetail };
  if (state.hint !== null) return { text: lineText(state.hint), kind: "hint", detail: null };
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
