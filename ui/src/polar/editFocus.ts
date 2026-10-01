/**
 * Which source is being edited (spec.md 10.4): Edit in the source list sets
 * it, the shell switches to the 3D stage, and the 3D view shows that source
 * opaque with its editing tools. Not project data: never saved, never in
 * the history.
 */
import { useMemo, useSyncExternalStore } from "react";
import { useBoatId } from "../boats/context";

function createEditing() {
  let focus: number | null = null;
  const listeners = new Set<() => void>();
  const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
  return {
    editSource: (id: number | null) => { if (focus === id && id === null) return; focus = id; for (const listener of listeners) listener(); },
    editFocus: () => focus,
    subscribe,
    onEditSource: (listener: (id: number) => void) => subscribe(() => { if (focus !== null) listener(focus); }),
  };
}
const stores = new Map<number | undefined, ReturnType<typeof createEditing>>();
function editing(id?: number) {
  let held = stores.get(id);
  if (!held) { held = createEditing(); stores.set(id, held); }
  return held;
}
export const editSource = (id: number | null) => editing().editSource(id);
export const editFocus = () => editing().editFocus();
export const onEditSource = (listener: (id: number) => void) => editing().onEditSource(listener);
export function resetEditing() { for (const held of stores.values()) held.editSource(null); }
export function useBoatEditing() { const id = useBoatId(); return useMemo(() => editing(id), [id]); }
export function useEditFocus(): number | null {
  const held = useBoatEditing();
  return useSyncExternalStore(held.subscribe, held.editFocus);
}
