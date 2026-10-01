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
import { useMemo, useSyncExternalStore } from "react";
import { useBoatId } from "./boats/context";

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

function createSelection() {
let state: SampleSelection = { ids: new Set(), origin: null, version: 0 };
const listeners = new Set<() => void>();
const focusListeners = new Set<(focus: MapFocus) => void>();
let pendingFocus: MapFocus | null = null;
const selectSamples = (ids: Iterable<number>, origin: SelectionOrigin) => {
  state = { ids: new Set(ids), origin, version: state.version + 1 };
  for (const listener of listeners) listener();
};
return {
  selectSamples,
  clearSamples: (origin: SelectionOrigin) => { if (state.ids.size) selectSamples([], origin); },
  getSampleSelection: () => state,
  subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
  focusMap: (focus: MapFocus) => { pendingFocus = focus; for (const listener of focusListeners) listener(focus); },
  onFocusMap: (listener: (focus: MapFocus) => void) => { focusListeners.add(listener); return () => { focusListeners.delete(listener); }; },
  takePendingFocus: () => { const focus = pendingFocus; pendingFocus = null; return focus; },
  reset: () => { state = { ids: new Set(), origin: null, version: 0 }; pendingFocus = null; for (const listener of listeners) listener(); },
};
}
const stores = new Map<number | undefined, ReturnType<typeof createSelection>>();
function selection(id?: number) {
  let held = stores.get(id);
  if (!held) { held = createSelection(); stores.set(id, held); }
  return held;
}
export const selectSamples = (ids: Iterable<number>, origin: SelectionOrigin) => selection().selectSamples(ids, origin);
export const clearSamples = (origin: SelectionOrigin) => selection().clearSamples(origin);
export const getSampleSelection = () => selection().getSampleSelection();
export const focusMap = (focus: MapFocus) => selection().focusMap(focus);
export const onFocusMap = (listener: (focus: MapFocus) => void) => selection().onFocusMap(listener);
export const takePendingFocus = () => selection().takePendingFocus();
export function useBoatSelection() {
  const id = useBoatId();
  return useMemo(() => selection(id), [id]);
}
export function useSampleSelection(): SampleSelection {
  const held = useBoatSelection();
  return useSyncExternalStore(held.subscribe, held.getSampleSelection);
}
export function resetSelection(): void { for (const held of stores.values()) held.reset(); }
