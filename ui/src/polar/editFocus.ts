/**
 * Which source is being edited (spec.md 10.4): Edit in the source list sets
 * it, the shell switches to the 3D stage, and the 3D view shows that source
 * opaque with its editing tools. Not project data: never saved, never in
 * the history.
 */
import { useSyncExternalStore } from "react";

let focus: number | null = null;
const listeners = new Set<() => void>();

/** Starts editing a source, or stops with null. */
export function editSource(id: number | null): void {
  if (focus === id) {
    // Asking again still brings the 3D stage forward.
    if (id !== null) for (const listener of listeners) listener();
    return;
  }
  focus = id;
  for (const listener of listeners) listener();
}

/** The source being edited, if any. */
export function editFocus(): number | null {
  return focus;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The source being edited, re-rendering on every change. */
export function useEditFocus(): number | null {
  return useSyncExternalStore(subscribe, editFocus);
}

/** Runs `listener` whenever editing starts or is asked for again; returns the unsubscribe. */
export function onEditSource(listener: (id: number) => void): () => void {
  const wrapped = () => { if (focus !== null) listener(focus); };
  listeners.add(wrapped);
  return () => listeners.delete(wrapped);
}
