/**
 * The embedded basemap, read once per launch whichever view asks first: the
 * world map, or a tracker dialog's preview.
 */
import { api } from "../ipc";
import { parseBasemap, type Basemap } from "./format";

let basemapPromise: Promise<Basemap> | null = null;

/** The parsed basemap; a failed read is asked again next time. */
export function loadBasemap(): Promise<Basemap> {
  basemapPromise ??= api.basemap().then(parseBasemap);
  basemapPromise.catch(() => { basemapPromise = null; });
  return basemapPromise;
}
