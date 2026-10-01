import { createContext, useContext, useMemo, useRef, type ReactNode } from "react";
import { onReveal } from "../help/highlight";
import { api, boatApi } from "../ipc";

const Context = createContext<{ id: number | undefined; api: typeof api; active: boolean }>({ id: undefined, api, active: true });

/** Requests retain this boat's identity even after its tab is hidden. */
export function BoatProvider({ id, active = true, children }: { id: number; active?: boolean; children: ReactNode }) {
  const scopedApi = useMemo(() => boatApi(id), [id]);
  const value = useMemo(() => ({ id, api: scopedApi, active }), [id, active, scopedApi]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useBoatApi() { return useContext(Context).api; }
export function useBoatId() { return useContext(Context).id; }

/** Hidden boat workspaces retain their state but never receive help actions. */
export function useBoatReveal() {
  const value = useContext(Context).active;
  const active = useRef(value);
  active.current = value;
  return useMemo(() => (step: string, handler: Parameters<typeof onReveal>[1]) => onReveal(step, handler, () => active.current), []);
}
