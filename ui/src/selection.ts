/**
 * The sample selection shared by the map and the polar views (spec.md 9.1,
 * 10.3): selecting samples in the 3D view highlights them on the map, and a
 * box on the map selects them in the polar views.
 *
 * Samples are named by id, which is what survives a refetch of any view's
 * data; ids stay below 2^53 (pe-core `MAX_ID`), so a number holds each one
 * exactly. Selection is not project data and is never recorded in the
 * history (spec.md 4.6).
 */
import { useSyncExternalStore } from "react";

/** Which view made the selection, so that view does not re-apply its own. */
export type SelectionOrigin = "map" | "3d" | "plot" | "tracks";

export interface SampleSelection {
  ids: ReadonlySet<number>;
  origin: SelectionOrigin | null;
  /** Bumped on every change. */
  version: number;
}

/** A request to bring something into view on the map. */
export type MapFocus = { kind: "selection" } | { kind: "track"; sourceId: number };

let state: SampleSelection = { ids: new Set(), origin: null, version: 0 };
const listeners = new Set<() => void>();
const focusListeners = new Set<(focus: MapFocus) => void>();
let pendingFocus: MapFocus | null = null;

function emit() {
  for (const listener of listeners) listener();
}

/** Replaces the selection. */
export function selectSamples(ids: Iterable<number>, origin: SelectionOrigin): void {
  state = { ids: new Set(ids), origin, version: state.version + 1 };
  emit();
}

/** Selects nothing. */
export function clearSamples(origin: SelectionOrigin): void {
  if (state.ids.size === 0) return;
  selectSamples([], origin);
}

export function getSampleSelection(): SampleSelection {
  return state;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The selection, re-rendering on every change. */
export function useSampleSelection(): SampleSelection {
  return useSyncExternalStore(subscribe, getSampleSelection);
}

/**
 * Asks for the map to show something: the shell switches to the Map stage,
 * and the map (once mounted) frames it. A request made before the map is
 * there waits for it.
 */
export function focusMap(focus: MapFocus): void {
  pendingFocus = focus;
  for (const listener of focusListeners) listener(focus);
}

/** Listens for focus requests; returns the unsubscribe. */
export function onFocusMap(listener: (focus: MapFocus) => void): () => void {
  focusListeners.add(listener);
  return () => focusListeners.delete(listener);
}

/** The request the map has not framed yet, taken (so it is framed once). */
export function takePendingFocus(): MapFocus | null {
  const focus = pendingFocus;
  pendingFocus = null;
  return focus;
}

/** For tests: back to nothing selected. */
export function resetSelection(): void {
  state = { ids: new Set(), origin: null, version: 0 };
  pendingFocus = null;
  emit();
}
