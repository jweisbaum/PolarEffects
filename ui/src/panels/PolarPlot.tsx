import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

import { reportFailure } from "../errors";
import type { PolarCurve } from "../generated/PolarCurve";
import type { PolarPlotResult } from "../generated/PolarPlotResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";
import { api } from "../ipc";
import { onThemeChange } from "../settings/themes";
import {
  ANGLE_TICKS, fitLayout, maxBoatSpeed, nearestPoint, niceTicks, project as projectPoint,
  type Hover, type SourceStyle,
} from "./plotGeometry";

/** A point within this many pixels of the pointer counts as hovered. */
const HOVER_DISTANCE_PX = 16;

/** Draws one curve's polyline; empty curves (no points in range) draw nothing. */
function strokeCurve(ctx: CanvasRenderingContext2D, curve: PolarCurve, layout: ReturnType<typeof fitLayout>, width: number) {
  if (curve.points.length === 0) return;
  ctx.strokeStyle = curve.colour;
  ctx.lineWidth = width;
  ctx.beginPath();
  curve.points.forEach((point, index) => {
    const { x, y } = projectPoint(point.twa, point.bsp, layout);
    if (index === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

/**
 * Draws the plot: radial BSP rings and angular TWA spokes, every curve in
 * its source colour, the blend thicker, dots, and the hovered point.
 */
function draw(canvas: HTMLCanvasElement, result: PolarPlotResult | null, hover: Hover | null) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.max(1, Math.round(width * dpr));
  canvas.height = Math.max(1, Math.round(height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (width <= 1 || height <= 1) return;

  const style = getComputedStyle(canvas);
  const line = style.getPropertyValue("--border-subtle").trim() || "#3a506a";
  const muted = style.getPropertyValue("--muted").trim() || "#b3c9de";
  const accent = style.getPropertyValue("--accent").trim() || "#8fb8de";

  const curves = result?.curves ?? [];
  const dots = result?.dots ?? [];
  const blend = result?.blend ?? null;
  const maxBsp = Math.max(maxBoatSpeed(curves, dots), blend ? maxBoatSpeed([blend], []) : 0);
  if (maxBsp <= 0) return;
  const layout = fitLayout(width, height, maxBsp);

  ctx.font = "10px sans-serif";
  ctx.textBaseline = "middle";
  ctx.lineWidth = 1;
  ctx.strokeStyle = line;
  ctx.fillStyle = muted;
  for (const tick of niceTicks(maxBsp)) {
    const r = tick * layout.scale;
    ctx.beginPath();
    ctx.arc(layout.centerX, layout.centerY, r, -Math.PI / 2, Math.PI / 2);
    ctx.stroke();
    ctx.fillText(String(tick), layout.centerX + r + 3, layout.centerY);
  }
  for (const angle of ANGLE_TICKS) {
    const edge = projectPoint(angle, maxBsp, layout);
    ctx.beginPath();
    ctx.moveTo(layout.centerX, layout.centerY);
    ctx.lineTo(edge.x, edge.y);
    ctx.stroke();
    const label = projectPoint(angle, maxBsp * 1.06, layout);
    ctx.textAlign = angle === 0 || angle === 180 ? "center" : "left";
    ctx.fillText(`${angle}°`, label.x, label.y);
  }

  for (const curve of curves) strokeCurve(ctx, curve, layout, 1.5);
  // The blend is drawn thicker (spec.md 9.2); it is a hook until M14, so
  // this only ever runs once something upstream actually fills it in.
  if (blend) strokeCurve(ctx, blend, layout, 3);

  for (const dot of dots) {
    const { x, y } = projectPoint(dot.twa, dot.bsp, layout);
    ctx.fillStyle = line;
    ctx.beginPath();
    ctx.arc(x, y, 2, 0, Math.PI * 2);
    ctx.fill();
  }

  if (hover) {
    ctx.strokeStyle = accent;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(hover.x, hover.y, 5, 0, Math.PI * 2);
    ctx.stroke();
  }
}

/**
 * The 2D polar plot (spec.md 9.2): boat speed against true wind angle, one
 * curve per visible polar source at the chosen true wind speed (or one per
 * source per wind speed it has, in "All"), sample dots within a band of the
 * slice, and the blend thicker. Rust computes every point; this only draws
 * and hit-tests what it is given.
 *
 * Refetches whenever the project changes — `revision` is bumped by every
 * command, including undo and redo, and by a source's colour, visibility,
 * weight or label — and whenever the chosen wind speed changes.
 */
export default function PolarPlot({ project, variant, onFullSize, onClose }: {
  project: ProjectSummary;
  /** `"panel"` in the right panel; `"overlay"` full size over the map. */
  variant: "panel" | "overlay";
  /** Panel variant only: opens the full-size overlay. */
  onFullSize?: () => void;
  /** Overlay variant only: closes it. */
  onClose?: () => void;
}) {
  const t = useT();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [result, setResult] = useState<PolarPlotResult | null>(null);
  const [tws, setTws] = useState<number | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const request = useRef(0);

  const visibleCount = useMemo(
    () => project.sources.filter((source) => source.visible && source.kind !== "track").length,
    [project.sources],
  );

  // Refetches on `project.revision`, which every edit bumps — a source's
  // colour, visibility, weight or label, undo and redo, adding or removing a
  // source — and on `project.id`, so switching projects does not show a
  // stale plot at the same revision number. `tws` refetches the slice.
  useEffect(() => {
    const id = ++request.current;
    void api.polarPlot(tws)
      .then((next) => { if (request.current === id) setResult(next); })
      .catch((err) => { if (request.current === id) reportFailure(err); });
  }, [project.id, project.revision, tws]);

  const sourcesById = useMemo(() => {
    const map = new Map<number, SourceStyle>();
    for (const source of project.sources) map.set(source.id, { label: source.label, colour: source.colour });
    return map;
  }, [project.sources]);

  const redraw = useCallback(() => {
    if (canvas.current) draw(canvas.current, result, hover);
  }, [result, hover]);

  useEffect(redraw, [redraw]);

  useEffect(() => {
    const element = wrap.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(redraw);
    observer.observe(element);
    return () => observer.disconnect();
  }, [redraw]);

  useEffect(() => onThemeChange(redraw), [redraw]);

  const onMove = useCallback((event: MouseEvent<HTMLCanvasElement>) => {
    if (!result) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const layout = fitLayout(rect.width, rect.height, Math.max(maxBoatSpeed(result.curves, result.dots), 1));
    const hit = nearestPoint(
      result.curves, result.dots, sourcesById,
      event.clientX - rect.left, event.clientY - rect.top, layout, HOVER_DISTANCE_PX,
    );
    setHover(hit);
  }, [result, sourcesById]);

  const domainMin = result?.tws_min ?? null;
  const domainMax = result?.tws_max ?? null;
  const hasDomain = domainMin !== null && domainMax !== null;
  const sliderValue = tws ?? (hasDomain ? Math.round(((domainMin as number) + (domainMax as number)) / 2) : 0);
  const hasAnyPoint = (result?.curves ?? []).some((curve) => curve.points.length > 0) || (result?.dots.length ?? 0) > 0;

  return (
    <div className={`polar-plot polar-plot-${variant}`}>
      <div className="polar-plot-controls">
        <label className="polar-plot-all">
          <input type="checkbox" data-feature="plot:all" checked={tws === null}
            title={t("Show every wind speed a source has, instead of one slice")}
            onChange={(event) => setTws(event.target.checked ? null : sliderValue)} />
          {t("All")}
        </label>
        <input type="range" data-feature="plot:tws" min={domainMin ?? 0} max={domainMax ?? 30} step={0.5}
          value={sliderValue} disabled={tws === null || !hasDomain}
          aria-label={t("Wind speed")} title={t("The true wind speed the plot slices at")}
          onChange={(event) => setTws(Number(event.target.value))} />
        <span className="polar-plot-tws-value">
          {tws === null ? t("All") : t("{tws} kn", { tws: tws.toFixed(1) })}
        </span>
        {variant === "panel" && (
          <button className="small" data-feature="plot:full-size" onClick={onFullSize}
            title={t("Open the polar plot full size over the map")}>
            {t("Full size")}
          </button>
        )}
        {variant === "overlay" && (
          <button className="small" data-feature="plot:close" onClick={onClose} title={t("Close")} aria-label={t("Close")}>
            {t("Close")}
          </button>
        )}
      </div>
      {visibleCount === 0 ? (
        <div className="plot-placeholder muted">{t("The polar plot appears here once the project has a source.")}</div>
      ) : (
        <div className="polar-plot-canvas-wrap" ref={wrap}>
          <canvas ref={canvas} onMouseMove={onMove} onMouseLeave={() => setHover(null)} />
          {hover && (
            <div className="polar-plot-tooltip" style={{ left: hover.x + 10, top: hover.y + 10 }}>
              <strong style={{ color: hover.colour }}>{hover.label}</strong>
              <div>{t("TWA {twa}°, TWS {tws} kn, BSP {bsp} kn", {
                twa: hover.twa.toFixed(0), tws: hover.tws.toFixed(1), bsp: hover.bsp.toFixed(2),
              })}</div>
            </div>
          )}
          {result && !hasAnyPoint && (
            <div className="polar-plot-empty muted">{t("No source has data at this wind speed.")}</div>
          )}
        </div>
      )}
    </div>
  );
}
