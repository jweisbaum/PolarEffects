/**
 * The map's two projections (spec.md 9.1): equirectangular and orthographic.
 *
 * Pure functions, shared by the renderer (whose shaders are the GLSL twins of
 * `forward` below) and by pointer handling, and tested without a GPU.
 *
 * A camera is a centre and a scale. The scale is screen pixels per degree of
 * latitude at the centre, in both projections, so switching projection keeps
 * the size of what is under the centre.
 */

export type ProjectionId = "equirectangular" | "orthographic";

export const PROJECTIONS: readonly ProjectionId[] = ["equirectangular", "orthographic"];

export interface Camera {
  /** Centre longitude, degrees, in [-180, 180). */
  lon: number;
  /** Centre latitude, degrees, in [-90, 90]. */
  lat: number;
  /** Pixels per degree at the centre. */
  scale: number;
}

export interface Viewport {
  width: number;
  height: number;
}

const DEG = Math.PI / 180;

/** Longitude folded into [-180, 180). */
export function wrapLon(lon: number): number {
  const wrapped = ((((lon + 180) % 360) + 360) % 360) - 180;
  return Object.is(wrapped, -0) ? 0 : wrapped;
}

const clampLat = (lat: number) => Math.max(-90, Math.min(90, lat));

/** The globe's radius in pixels at a scale: one degree of arc at the centre is `scale` pixels. */
export function globeRadius(scale: number): number {
  return scale / DEG;
}

/** A place on screen, or null where the projection does not show it (the far side of the globe). */
export function forward(
  projection: ProjectionId, camera: Camera, view: Viewport, lon: number, lat: number,
): { x: number; y: number } | null {
  if (projection === "equirectangular") {
    return {
      x: view.width / 2 + wrapLon(lon - camera.lon) * camera.scale,
      y: view.height / 2 - (lat - camera.lat) * camera.scale,
    };
  }
  const lam = (lon - camera.lon) * DEG;
  const phi = lat * DEG;
  const phi0 = camera.lat * DEG;
  const cosc = Math.sin(phi0) * Math.sin(phi) + Math.cos(phi0) * Math.cos(phi) * Math.cos(lam);
  if (cosc < 0) return null;
  const r = globeRadius(camera.scale);
  const x = r * Math.cos(phi) * Math.sin(lam);
  const y = r * (Math.cos(phi0) * Math.sin(phi) - Math.sin(phi0) * Math.cos(phi) * Math.cos(lam));
  return { x: view.width / 2 + x, y: view.height / 2 - y };
}

/** The place under a pixel, or null off the globe (or past a pole on the flat map). */
export function inverse(
  projection: ProjectionId, camera: Camera, view: Viewport, x: number, y: number,
): { lon: number; lat: number } | null {
  const dx = x - view.width / 2;
  const dy = view.height / 2 - y;
  if (projection === "equirectangular") {
    const lat = camera.lat + dy / camera.scale;
    if (lat < -90 || lat > 90) return null;
    return { lon: wrapLon(camera.lon + dx / camera.scale), lat };
  }
  const r = globeRadius(camera.scale);
  const rho = Math.hypot(dx, dy);
  if (rho > r) return null;
  if (rho < 1e-9) return { lon: wrapLon(camera.lon), lat: camera.lat };
  const c = Math.asin(rho / r);
  const phi0 = camera.lat * DEG;
  const lat = Math.asin(Math.cos(c) * Math.sin(phi0) + (dy * Math.sin(c) * Math.cos(phi0)) / rho) / DEG;
  const lon = camera.lon
    + Math.atan2(dx * Math.sin(c), rho * Math.cos(phi0) * Math.cos(c) - dy * Math.sin(phi0) * Math.sin(c)) / DEG;
  return { lon: wrapLon(lon), lat };
}

/** The scale that shows the whole world with a margin. */
export function fitScale(projection: ProjectionId, view: Viewport): number {
  if (projection === "equirectangular") return Math.max(0.1, Math.min(view.width / 360, view.height / 180));
  return Math.max(0.1, 0.45 * Math.min(view.width, view.height) * DEG);
}

/** The camera that shows the whole world. */
export function fitCamera(projection: ProjectionId, view: Viewport): Camera {
  return { lon: 0, lat: projection === "orthographic" ? 20 : 0, scale: fitScale(projection, view) };
}

/** The furthest in a person can zoom, in pixels per degree: the 50 m coastline's detail. */
export const MAX_SCALE = 400;

function clampScale(projection: ProjectionId, view: Viewport, scale: number): number {
  return Math.max(fitScale(projection, view) * 0.5, Math.min(MAX_SCALE, scale));
}

/**
 * The camera after a drag of (dx, dy) screen pixels. The flat map moves with
 * the pointer; the globe turns under it.
 */
export function pan(camera: Camera, dx: number, dy: number): Camera {
  return { ...camera, lon: wrapLon(camera.lon - dx / camera.scale), lat: clampLat(camera.lat + dy / camera.scale) };
}

/**
 * The camera after zooming by `factor` with the pointer at (x, y). On the flat
 * map the place under the pointer stays under it; the globe zooms about its
 * centre, which is where a person is looking on a globe.
 */
export function zoomAt(
  projection: ProjectionId, camera: Camera, view: Viewport, factor: number, x: number, y: number,
): Camera {
  const scale = clampScale(projection, view, camera.scale * factor);
  if (projection === "orthographic") return { ...camera, scale };
  const dx = x - view.width / 2;
  const dy = view.height / 2 - y;
  const lon = camera.lon + dx / camera.scale - dx / scale;
  const lat = camera.lat + dy / camera.scale - dy / scale;
  return { lon: wrapLon(lon), lat: clampLat(lat), scale };
}
