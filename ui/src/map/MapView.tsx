import { useCallback, useEffect, useRef, useState } from "react";

import type { AppSettings } from "../generated/AppSettings";
import { reportFailure } from "../errors";
import { setHint } from "../hint";
import { msg, useT } from "../i18n";
import { api } from "../ipc";
import { mapColour, onThemeChange, rgba } from "../settings/themes";
import { parseBasemap, type Basemap } from "./format";
import { fitCamera, pan, PROJECTIONS, zoomAt, type Camera, type ProjectionId, type Viewport } from "./projection";
import { MapRenderer, type MapColours } from "./renderer";

/** The projections' names, for the picker. */
const PROJECTION_NAMES: Record<ProjectionId, string> = {
  equirectangular: msg("Equirectangular"),
  orthographic: msg("Orthographic"),
};

/** The basemap is read once per launch, whichever stage asks first. */
let basemapPromise: Promise<Basemap> | null = null;
function loadBasemap(): Promise<Basemap> {
  basemapPromise ??= api.basemap().then(parseBasemap);
  basemapPromise.catch(() => { basemapPromise = null; });
  return basemapPromise;
}

function colours(): MapColours {
  return {
    sea: rgba(mapColour("sea")),
    land: rgba(mapColour("land")),
    coast: rgba(mapColour("coast"), 0.75),
    graticule: rgba(mapColour("graticule"), 0.2),
    void: rgba(mapColour("void")),
  };
}

/**
 * The world map stage (spec.md 9.1): a WebGL2 canvas with VectorEffects'
 * embedded basemap, in equirectangular or orthographic projection. Drag pans
 * the flat map or turns the globe; the wheel zooms. The projection is a
 * setting, remembered for the person.
 */
export default function MapView({ settings, onSettings }: {
  settings: AppSettings | null;
  onSettings: (settings: AppSettings) => void;
}) {
  const t = useT();
  const projection: ProjectionId = settings?.projection ?? "equirectangular";
  const canvas = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<MapRenderer | null>(null);
  const camera = useRef<Camera | null>(null);
  const view = useRef<Viewport>({ width: 1, height: 1 });
  const frame = useRef(0);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const projectionRef = useRef(projection);
  const [unavailable, setUnavailable] = useState<string | null>(null);

  const draw = useCallback(() => {
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      const element = canvas.current;
      if (!element || !renderer.current) return;
      const ratio = window.devicePixelRatio || 1;
      const width = Math.max(1, element.clientWidth);
      const height = Math.max(1, element.clientHeight);
      if (element.width !== Math.round(width * ratio) || element.height !== Math.round(height * ratio)) {
        element.width = Math.round(width * ratio);
        element.height = Math.round(height * ratio);
      }
      view.current = { width, height };
      camera.current ??= fitCamera(projectionRef.current, view.current);
      renderer.current.render(projectionRef.current, camera.current, view.current, ratio, colours());
    });
  }, []);

  const fit = useCallback(() => {
    camera.current = fitCamera(projectionRef.current, view.current);
    draw();
  }, [draw]);

  useEffect(() => {
    projectionRef.current = projection;
    draw();
  }, [projection, draw]);

  useEffect(() => {
    const element = canvas.current;
    const gl = element?.getContext("webgl2", { antialias: true, alpha: false }) ?? null;
    if (!element || !gl) {
      setUnavailable(msg("The map needs WebGL2, which this system does not offer."));
      return;
    }
    let live = true;
    loadBasemap()
      .then((basemap) => {
        if (!live) return;
        renderer.current = new MapRenderer(gl, basemap);
        draw();
      })
      .catch((error: unknown) => {
        if (live) setUnavailable(msg("The map could not be drawn."));
        reportFailure(error);
      });
    const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => draw());
    resize?.observe(element);
    const offTheme = onThemeChange(draw);
    return () => {
      live = false;
      resize?.disconnect();
      offTheme();
      cancelAnimationFrame(frame.current);
      frame.current = 0;
      renderer.current?.dispose();
      renderer.current = null;
    };
  }, [draw]);

  // Wheel zoom needs a non-passive listener to stop the page scrolling.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (!camera.current) return;
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      const factor = Math.exp(-event.deltaY * 0.0015);
      camera.current = zoomAt(projectionRef.current, camera.current, view.current, factor,
        event.clientX - rect.left, event.clientY - rect.top);
      draw();
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, [draw]);

  const choose = (next: ProjectionId) => {
    api.setProjection(next).then(onSettings).catch(reportFailure);
  };

  return (
    <div className="map-stage">
      <canvas
        ref={canvas}
        className="map-canvas"
        onPointerEnter={() => setHint(t("Drag to move the map; scroll to zoom."))}
        onPointerLeave={() => setHint(null)}
        onPointerDown={(event) => {
          drag.current = { x: event.clientX, y: event.clientY };
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          if (!drag.current || !camera.current) return;
          camera.current = pan(camera.current, event.clientX - drag.current.x, event.clientY - drag.current.y);
          drag.current = { x: event.clientX, y: event.clientY };
          draw();
        }}
        onPointerUp={() => { drag.current = null; }}
        onPointerCancel={() => { drag.current = null; }}
      />
      {unavailable !== null && <p className="map-unavailable muted">{t(unavailable)}</p>}
      <div className="map-controls">
        <select data-feature="map:projection" aria-label={t("Projection")} title={t("How the round Earth is drawn flat")}
          value={projection} onChange={(event) => choose(event.target.value as ProjectionId)}>
          {PROJECTIONS.map((id) => <option key={id} value={id}>{t(PROJECTION_NAMES[id])}</option>)}
        </select>
        <button data-feature="map:fit" onClick={fit} title={t("Show the whole world")}>{t("Fit the world")}</button>
      </div>
    </div>
  );
}
