import { createContext, useContext, useMemo, type ReactNode } from "react";
import type { View } from "../polar/scene3d";
import type { Layout } from "../polar/geometry3d";

export type HoverPoint = { twa: number; tws: number };
type SyncEvent = { kind: "camera"; boat: number; view: View; layout: Layout } | { kind: "hover"; boat: number; point: HoverPoint | null };

export function createFleetSync() {
  const listeners = new Set<(event: SyncEvent) => void>();
  let camera: Extract<SyncEvent, { kind: "camera" }> | null = null;
  return {
    get camera() { return camera; },
    publish(event: SyncEvent) {
      if (event.kind === "camera") camera = event;
      for (const listener of listeners) listener(event);
    },
    subscribe(listener: (event: SyncEvent) => void) {
      listeners.add(listener);
      if (camera) listener(camera);
      return () => { listeners.delete(listener); };
    },
  };
}

const Context = createContext<ReturnType<typeof createFleetSync> | null>(null);
export function FleetSyncProvider({ enabled, children }: { enabled: boolean; children: ReactNode }) {
  const sync = useMemo(() => enabled ? createFleetSync() : null, [enabled]);
  return <Context.Provider value={sync}>{children}</Context.Provider>;
}
export function useFleetSync() { return useContext(Context); }

/** Compare like wind conditions, never by unrelated sample/source ids. */
export function correspondingDot(points: Float32Array, point: HoverPoint): number {
  let best = -1, score = Infinity;
  for (let i = 0; i < points.length; i += 3) {
    const distance = Math.abs(points[i]! - point.twa) % 360;
    const angle = Math.min(distance, 360 - distance);
    const wind = Math.abs(points[i + 1]! - point.tws);
    if (angle > 5 || wind > 1) continue;
    const next = angle * angle / 25 + wind * wind;
    if (next < score) { best = i / 3; score = next; }
  }
  return best;
}
