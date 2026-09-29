import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

import { needsOutline } from "../colourContrast";
import { reportFailure } from "../errors";
import type { PolarCurve } from "../generated/PolarCurve";
import type { PolarPlotResult } from "../generated/PolarPlotResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { useT } from "../i18n";
import { api } from "../ipc";
import { SPEED_FACTOR, SPEED_SYMBOL } from "../polar/view3d";
import { useSampleSelection } from "../selection";
import { onThemeChange } from "../settings/themes";
import { DOT_EXCLUDED, DOT_FILTERED, dotSampleId, emptyDots, type DotPacket } from "./dotPacket";
import {
  ANGLE_TICKS, axisLabels, fitLayout, maxBoatSpeed, nearestPoint, project as projectPoint, speedTicks,
  type Hover, type SourceStyle,
} from "./plotGeometry";

/** A point within this many pixels of the pointer counts as hovered. */
const HOVER_DISTANCE_PX = 16;
/** Beyond this many dots each is a small square rather than a circle: far cheaper to fill. */
const MANY_DOTS = 20_000;

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
 * The largest boat speed the plot must hold: every source curve, every dot
 * and the blend. The drawing and the hover hit-test both size the fan with
 * it, so a hovered point is where it is drawn (M6 carry).
 */
export function plotMaxBsp(result: PolarPlotResult | null, dots: DotPacket): number {
  if (!result) return maxBoatSpeed([], dots);
  return Math.max(maxBoatSpeed(result.curves, dots), maxBoatSpeed(result.blend, null));
}

/** The blend's curves with their label in the interface language. */
function blendCurves(result: PolarPlotResult | null, t: (key: string) => string): PolarCurve[] {
  return (result?.blend ?? []).map((curve) => ({ ...curve, label: t(curve.label) }));
}

/**
 * A speed in the display unit, for text: at most `digits` decimals, trailing
 * zeros dropped. Everything the plot is given is in knots; this is the one
 * place its text leaves them.
 */
export function displaySpeed(knots: number, unit: SpeedUnit, digits: number): string {
  return String(Number((knots * SPEED_FACTOR[unit]).toFixed(digits)));
}

/**
 * Draws the plot: radial BSP rings (round numbers in the display unit) and
 * angular TWA spokes, every curve in its source colour, the blend thicker,
 * sample dots in their track's colour (filtered ones dimmed, excluded ones
 * hollow, selected ones ringed), and the hovered point.
 */
function draw(canvas: HTMLCanvasElement, result: PolarPlotResult | null, dots: DotPacket, hover: Hover | null,
  colours: ReadonlyMap<number, SourceStyle>, selected: ReadonlySet<number>, unit: SpeedUnit) {
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
  const blend = result?.blend ?? [];
  const maxBsp = plotMaxBsp(result, dots);
  if (maxBsp <= 0) return;
  const layout = fitLayout(width, height, maxBsp);

  ctx.font = "10px sans-serif";
  ctx.textBaseline = "top";
  ctx.textAlign = "left";
  ctx.lineWidth = 1;
  ctx.strokeStyle = line;
  ctx.fillStyle = muted;
  const factor = SPEED_FACTOR[unit];
  for (const tick of speedTicks(maxBsp, factor)) {
    const r = tick.knots * layout.scale;
    ctx.beginPath();
    ctx.arc(layout.centerX, layout.centerY, r, -Math.PI / 2, Math.PI / 2);
    ctx.stroke();
  }
  for (const angle of ANGLE_TICKS) {
    const edge = projectPoint(angle, maxBsp, layout);
    ctx.beginPath();
    ctx.moveTo(layout.centerX, layout.centerY);
    ctx.lineTo(edge.x, edge.y);
    ctx.stroke();
  }
  for (const label of axisLabels(layout, maxBsp, (text) => ctx.measureText(text).width, 10, factor)) {
    ctx.fillText(label.text, label.x, label.y);
  }

  for (const curve of curves) strokeCurve(ctx, curve, layout, 1.5);
  // The blend is drawn thicker (spec.md 9.2), in the Blend entry's colour,
  // over a contrasting outline when that colour is lost on the background.
  const background = style.getPropertyValue("--surface").trim() || "#253447";
  const text = style.getPropertyValue("--text").trim() || "#d6e6f5";
  for (const curve of blend) {
    if (needsOutline(curve.colour, background)) strokeCurve(ctx, { ...curve, colour: text }, layout, 5);
    strokeCurve(ctx, curve, layout, 3);
  }

  const many = dots.count > MANY_DOTS;
  const dotColours = dots.sources.map((id) => colours.get(id)?.colour ?? line);
  for (let k = 0; k < dots.count; k++) {
    const { x, y } = projectPoint(dots.points[k * 3]!, dots.points[k * 3 + 2]!, layout);
    const colour = dotColours[dots.source[k]!]!;
    const flags = dots.flags[k]!;
    ctx.globalAlpha = flags & DOT_FILTERED ? 0.3 : 0.85;
    if (many) {
      ctx.fillStyle = colour;
      if (flags & DOT_EXCLUDED) ctx.globalAlpha *= 0.4;
      ctx.fillRect(x - 1, y - 1, 2, 2);
      continue;
    }
    ctx.beginPath();
    ctx.arc(x, y, 2.5, 0, Math.PI * 2);
    if (flags & DOT_EXCLUDED) {
      ctx.strokeStyle = colour;
      ctx.lineWidth = 1;
      ctx.stroke();
    } else {
      ctx.fillStyle = colour;
      ctx.fill();
    }
  }
  ctx.globalAlpha = 1;
  if (selected.size > 0) {
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1.5;
    for (let k = 0; k < dots.count; k++) {
      if (!selected.has(dotSampleId(dots, k))) continue;
      const { x, y } = projectPoint(dots.points[k * 3]!, dots.points[k * 3 + 2]!, layout);
      ctx.beginPath();
      ctx.arc(x, y, 4.5, 0, Math.PI * 2);
      ctx.stroke();
    }
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
export default function PolarPlot({ project, variant, unit = "kn", onFullSize, onClose }: {
  project: ProjectSummary;
  /** The display speed unit (Settings); every value arrives and is kept in knots. */
  unit?: SpeedUnit;
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
  const [dots, setDots] = useState<DotPacket>(emptyDots);
  const [tws, setTws] = useState<number | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const [showFiltered, setShowFiltered] = useState(false);
  const request = useRef(0);
  const selection = useSampleSelection();

  const visibleCount = useMemo(
    () => project.sources.filter((source) => source.visible).length,
    [project.sources],
  );
  const tracksShown = project.sources.some((source) => source.visible && source.kind === "track");

  // Refetches on `project.revision`, which every edit bumps — a source's
  // colour, visibility, weight or label, undo and redo, adding or removing a
  // source — and on `project.id`, so switching projects does not show a
  // stale plot at the same revision number. `tws` refetches the slice.
  useEffect(() => {
    const id = ++request.current;
    void Promise.all([api.polarPlot(tws), api.polarPlotDots(tws, showFiltered)])
      .then(([next, nextDots]) => {
        if (request.current !== id) return;
        setResult(next);
        setDots(nextDots);
      })
      .catch((err) => { if (request.current === id) reportFailure(err); });
  }, [project.id, project.revision, tws, showFiltered]);

  const sourcesById = useMemo(() => {
    const map = new Map<number, SourceStyle>();
    for (const source of project.sources) map.set(source.id, { label: source.label, colour: source.colour });
    return map;
  }, [project.sources]);

  const redraw = useCallback(() => {
    if (canvas.current) draw(canvas.current, result, dots, hover, sourcesById, selection.ids, unit);
  }, [result, dots, hover, sourcesById, selection, unit]);

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
    const layout = fitLayout(rect.width, rect.height, Math.max(plotMaxBsp(result, dots), 1));
    const hit = nearestPoint(
      [...result.curves, ...blendCurves(result, t)], dots, sourcesById,
      event.clientX - rect.left, event.clientY - rect.top, layout, HOVER_DISTANCE_PX,
    );
    setHover(hit);
  }, [result, dots, sourcesById, t]);

  const domainMin = result?.tws_min ?? null;
  const domainMax = result?.tws_max ?? null;
  const hasDomain = domainMin !== null && domainMax !== null;
  const sliderValue = tws ?? (hasDomain ? Math.round(((domainMin as number) + (domainMax as number)) / 2) : 0);
  const hasAnyPoint = [...(result?.curves ?? []), ...(result?.blend ?? [])].some((curve) => curve.points.length > 0)
    || dots.count > 0;

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
        <span className="polar-plot-tws-value"
          title={tracksShown && tws !== null ? t("Sample dots within {band} {unit} of this wind speed (Settings)", {
            band: displaySpeed(result?.band_kn ?? 1, unit, 2), unit: SPEED_SYMBOL[unit],
          }) : undefined}>
          {tws === null ? t("All") : t("{tws} {unit}", { tws: displaySpeed(tws, unit, 1), unit: SPEED_SYMBOL[unit] })}
        </span>
        <label className="polar-plot-filtered"
          title={tracksShown ? t("Also draw the samples the filters take out, dimmed") : t("No visible track has samples to filter")}>
          <input type="checkbox" data-feature="plot:show-filtered" checked={showFiltered} disabled={!tracksShown}
            onChange={(event) => setShowFiltered(event.target.checked)} />
          {t("Filtered")}
        </label>
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
              <div>{t("TWA {twa}°, TWS {tws} {unit}, BSP {bsp} {unit}", {
                twa: hover.twa.toFixed(0), tws: (hover.tws * SPEED_FACTOR[unit]).toFixed(1),
                bsp: (hover.bsp * SPEED_FACTOR[unit]).toFixed(2), unit: SPEED_SYMBOL[unit],
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
