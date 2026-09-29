import { useCallback, useEffect, useRef, useState } from "react";

import { registerRedraw } from "../automation";
import type { AppSettings } from "../generated/AppSettings";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SampleDetails } from "../generated/SampleDetails";
import { reportFailure } from "../errors";
import { setHint } from "../hint";
import { msg, useT } from "../i18n";
import { api } from "../ipc";
import { clearSamples, getSampleSelection, onFocusMap, selectSamples, takePendingFocus, useSampleSelection, type MapFocus } from "../selection";
import { mapColour, onThemeChange, rgba } from "../settings/themes";
import { loadBasemap } from "./basemap";
import { hoverLines } from "./hover";
import { fitCamera, inverse, MAX_SCALE, pan, PROJECTIONS, zoomAt, type Camera, type ProjectionId, type Viewport } from "./projection";
import { MapRenderer, type MapColours } from "./renderer";
import { boxSelect, buildTrackGeometry, FixIndex, frameCamera, selectedPoints } from "./trackLayer";
import { emptyTracks, fixSampleId, type TrackPacket } from "./trackPacket";

/** The projections' names, for the picker. */
const PROJECTION_NAMES: Record<ProjectionId, string> = {
  equirectangular: msg("Equirectangular"),
  orthographic: msg("Orthographic"),
};

/** A fix within this many pixels of the pointer is hovered. */
const HOVER_RADIUS_PX = 8;


function colours(): MapColours {
  return {
    sea: rgba(mapColour("sea")),
    land: rgba(mapColour("land")),
    coast: rgba(mapColour("coast"), 0.75),
    graticule: rgba(mapColour("graticule"), 0.2),
    void: rgba(mapColour("void")),
  };
}

function cssColour(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * The world map stage (spec.md 9.1): a WebGL2 canvas with VectorEffects'
 * embedded basemap, in equirectangular or orthographic projection, and every
 * visible track in its source colour, filtered fixes dimmed. Drag pans the
 * flat map or turns the globe; the wheel zooms; Shift-drag draws a box that
 * selects the fixes inside it, and that selection is the polar views' too.
 * Hovering a fix shows its time, speeds, heading and environment. The
 * projection is a setting, remembered for the person.
 */
export default function MapView({ project, settings, onSettings }: {
  project: ProjectSummary;
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
  const drag = useRef<{ x: number; y: number; box: boolean; x0: number; y0: number } | null>(null);
  const projectionRef = useRef(projection);
  const packetRef = useRef<TrackPacket>(emptyTracks());
  const index = useRef<FixIndex | null>(null);
  const hoverFrame = useRef(0);
  const hoverRequest = useRef(0);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [packet, setPacket] = useState<TrackPacket>(emptyTracks);
  const [ready, setReady] = useState(false);
  const [box, setBox] = useState<[number, number, number, number] | null>(null);
  const [hover, setHover] = useState<{ x: number; y: number; details: SampleDetails; colour: string } | null>(null);
  const [hoverFix, setHoverFix] = useState(-1);
  const selection = useSampleSelection();

  /** Draws the map now. */
  const paint = useCallback(() => {
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
  }, []);

  /** Draws the map on the next frame, once. */
  const draw = useCallback(() => {
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      paint();
    });
  }, [paint]);

  // The WebDriver screenshot's synchronous redraw (development builds only).
  useEffect(() => {
    const element = canvas.current;
    return element ? registerRedraw(element, paint) : undefined;
  }, [paint]);

  const fit = useCallback(() => {
    camera.current = fitCamera(projectionRef.current, view.current);
    draw();
  }, [draw]);

  /** Frames the selection or one track, if the map has its fixes. */
  const frameFocus = useCallback((focus: MapFocus) => {
    const current = packetRef.current;
    let points: Float32Array;
    if (focus.kind === "track") {
      const track = current.tracks.find((tr) => tr.id === focus.sourceId);
      if (!track) return false;
      points = current.fixes.points.subarray(track.first * 2, (track.first + track.count) * 2);
    } else {
      points = selectedPoints(current, getSampleSelection().ids);
    }
    const framed = frameCamera(projectionRef.current, view.current, points, MAX_SCALE);
    if (!framed) return false;
    camera.current = framed;
    draw();
    return true;
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
        setReady(true);
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
      cancelAnimationFrame(hoverFrame.current);
      frame.current = 0;
      renderer.current?.dispose();
      renderer.current = null;
    };
  }, [draw]);

  // The tracks, refetched on every document change and on switching project.
  useEffect(() => {
    let live = true;
    api.mapTracks()
      .then((next) => { if (live) setPacket(next); })
      .catch((error: unknown) => { if (live) reportFailure(error); });
    return () => { live = false; };
  }, [project.id, project.revision]);

  useEffect(() => {
    packetRef.current = packet;
    index.current = new FixIndex(packet);
    setHover(null);
    setHoverFix(-1);
    if (!ready || !renderer.current) return;
    renderer.current.setTracks(buildTrackGeometry(packet));
    // A "show on map" made before the tracks arrived is framed now.
    const focus = takePendingFocus();
    if (focus) frameFocus(focus);
    draw();
  }, [packet, ready, draw, frameFocus]);

  // The selection and the hovered fix, drawn over the tracks.
  useEffect(() => {
    if (!ready || !renderer.current) return;
    const highlight = rgba(cssColour("--accent", "#8fb8de"));
    const highlights = [{ points: selectedPoints(packet, selection.ids), colour: highlight, size: 7 }];
    if (hoverFix >= 0 && hoverFix < packet.fixes.count) {
      highlights.push({
        points: packet.fixes.points.slice(hoverFix * 2, hoverFix * 2 + 2),
        colour: rgba("#ffffff"),
        size: 9,
      });
    }
    renderer.current.setHighlights(highlights);
    draw();
  }, [packet, selection, hoverFix, ready, draw]);

  // "Show on map" from elsewhere, while the map is already open.
  useEffect(() => onFocusMap((focus) => {
    if (frameFocus(focus)) takePendingFocus();
  }), [frameFocus]);

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

  /** Finds the fix under the pointer once per frame, and asks Rust for its details. */
  const hoverAt = useCallback((x: number, y: number) => {
    cancelAnimationFrame(hoverFrame.current);
    hoverFrame.current = requestAnimationFrame(() => {
      const current = packetRef.current;
      const cam = camera.current;
      if (!cam || !index.current || current.fixes.count === 0) return;
      const place = inverse(projectionRef.current, cam, view.current, x, y);
      const found = place
        ? index.current.nearest(projectionRef.current, cam, view.current, place.lon, place.lat, x, y, HOVER_RADIUS_PX)
        : -1;
      setHoverFix(found);
      if (found < 0) { setHover(null); return; }
      const track = current.tracks.find((tr) => found >= tr.first && found < tr.first + tr.count);
      if (!track) return;
      const id = ++hoverRequest.current;
      api.sampleDetails(track.id, fixSampleId(current, found))
        .then((details) => { if (hoverRequest.current === id) setHover({ x, y, details, colour: track.colour }); })
        .catch(() => { if (hoverRequest.current === id) setHover(null); });
    });
  }, []);

  const choose = (next: ProjectionId) => {
    api.setProjection(next).then(onSettings).catch(reportFailure);
  };

  const units = settings?.units ?? { speed: "kn", wave_height: "m", distance: "nm" };
  const labelOf = new Map(project.sources.map((s) => [s.id, s.label]));
  const hasTracks = packet.tracks.length > 0;

  return (
    <div className="map-stage" tabIndex={-1}
      onKeyDown={(event) => { if (event.key === "Escape" && selection.ids.size > 0) { event.stopPropagation(); clearSamples("map"); } }}>
      <canvas
        ref={canvas}
        className="map-canvas"
        onPointerEnter={() => setHint(t("Drag to move the map; scroll to zoom. Shift-drag a box to select track positions."))}
        onPointerLeave={() => { setHint(null); setHover(null); setHoverFix(-1); cancelAnimationFrame(hoverFrame.current); }}
        onPointerDown={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          const x0 = event.clientX - rect.left, y0 = event.clientY - rect.top;
          drag.current = { x: event.clientX, y: event.clientY, box: event.shiftKey, x0, y0 };
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          const x = event.clientX - rect.left, y = event.clientY - rect.top;
          const d = drag.current;
          if (!d) { hoverAt(x, y); return; }
          if (d.box) { setBox([d.x0, d.y0, x, y]); return; }
          if (!camera.current) return;
          camera.current = pan(camera.current, event.clientX - d.x, event.clientY - d.y);
          d.x = event.clientX;
          d.y = event.clientY;
          setHover(null);
          draw();
        }}
        onPointerUp={(event) => {
          const d = drag.current;
          drag.current = null;
          setBox(null);
          if (!d?.box || !camera.current) return;
          const rect = event.currentTarget.getBoundingClientRect();
          const x = event.clientX - rect.left, y = event.clientY - rect.top;
          if (Math.hypot(x - d.x0, y - d.y0) < 4) return;
          selectSamples(boxSelect(packetRef.current, projectionRef.current, camera.current, view.current, d.x0, d.y0, x, y), "map");
        }}
        onPointerCancel={() => { drag.current = null; setBox(null); }}
      />
      {box && (
        <div className="map-box" aria-hidden="true" style={{
          left: Math.min(box[0], box[2]), top: Math.min(box[1], box[3]),
          width: Math.abs(box[2] - box[0]), height: Math.abs(box[3] - box[1]),
        }} />
      )}
      {hover && (
        <div className="polar-plot-tooltip map-tooltip" style={{ left: hover.x + 12, top: hover.y + 12 }}>
          <strong style={{ color: hover.colour }}>{labelOf.get(hover.details.source_id) ?? t("Unknown source")}</strong>
          <table>
            <tbody>
              {hoverLines(hover.details, units).map((line) => (
                <tr key={line.label}><th>{line.label}</th><td>{line.value}</td></tr>
              ))}
            </tbody>
          </table>
          {(hover.details.filtered || hover.details.excluded) && (
            <div className="muted">
              {hover.details.excluded ? t("Excluded from the blend") : t("Filtered out")}
            </div>
          )}
        </div>
      )}
      {unavailable !== null && <p className="map-unavailable muted">{t(unavailable)}</p>}
      <div className="map-controls">
        <select data-feature="map:projection" aria-label={t("Projection")} title={t("How the round Earth is drawn flat")}
          value={projection} onChange={(event) => choose(event.target.value as ProjectionId)}>
          {PROJECTIONS.map((id) => <option key={id} value={id}>{t(PROJECTION_NAMES[id])}</option>)}
        </select>
        <button data-feature="map:fit" onClick={fit} title={t("Show the whole world")}>{t("Fit the world")}</button>
        <button data-feature="map:fit-tracks" disabled={!hasTracks} title={t("Show every track")}
          onClick={() => {
            const framed = frameCamera(projectionRef.current, view.current, packetRef.current.fixes.points, MAX_SCALE);
            if (framed) { camera.current = framed; draw(); }
          }}>
          {t("Fit the tracks")}
        </button>
        {selection.ids.size > 0 && (
          <span className="map-selection muted">{t("{count} selected", { count: selection.ids.size })}</span>
        )}
        <button data-feature="map:clear-selection" disabled={selection.ids.size === 0} onClick={() => clearSamples("map")}
          title={t("Select no track positions (Escape); Shift-drag a box to select some")}>
          {t("Clear")}
        </button>
      </div>
    </div>
  );
}
