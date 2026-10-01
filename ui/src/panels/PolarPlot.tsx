import { useBoatApi } from "../boats/context";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

import { needsOutline } from "../colourContrast";
import DayBandLegend from "../DayBandLegend";
import { reportFailure } from "../errors";
import type { BlendCell } from "../generated/BlendCell";
import type { PolarCurve } from "../generated/PolarCurve";
import type { PolarPlotResult } from "../generated/PolarPlotResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { useT } from "../i18n";

import BlendCellTooltip from "../polar/BlendCellTooltip";
import { SPEED_FACTOR, SPEED_SYMBOL } from "../polar/view3d";
import { useSampleSelection } from "../selection";
import { onThemeChange } from "../settings/themes";
import { DOT_EXCLUDED, DOT_FILTERED, dotSampleId, emptyDots, type DotPacket } from "./dotPacket";
import {
  ANGLE_TICKS, FULL_ANGLE_TICKS, axisLabels, crossings, dotFill, fitLayout, maxBoatSpeed, measureBetween, nearestCrossing, nearestPoint,
  project as projectPoint, speedTicks, unproject,
  type Crossing, type DotColourMode, type Hover, type PolarPoint, type SourceStyle,
} from "./plotGeometry";

/** A point within this many pixels of the pointer counts as hovered. */
const HOVER_DISTANCE_PX = 16;
/** How close a hovered blend point must be to an output-grid value to be that cell's, degrees or knots. */
const ON_GRID = 1e-6;
/** A click this close to a curve on the measuring spoke pins the curve's own value, pixels. */
const PIN_SNAP_PX = 8;
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

/** What the Measure tool has on the plot (spec.md 9.2): all in knots and degrees. */
interface Measuring {
  /** Where the pointer is. */
  cursor: PolarPoint;
  /** Every curve's value on the pointer's spoke, fastest first. */
  crossings: Crossing[];
  /** The pinned point A, if any. */
  pin: PolarPoint | null;
}

/** A signed number for a difference: "+0.70", "−0.60" (a true minus sign), and a bare "0.00" for none. */
export function signed(value: number, digits: number): string {
  const text = Math.abs(value).toFixed(digits);
  if (Number(text) === 0) return text;
  return `${value > 0 ? "+" : "−"}${text}`;
}

/**
 * Draws the plot: radial BSP rings (round numbers in the display unit) and
 * angular TWA spokes, every curve in its source colour, the blend thicker,
 * sample dots in their track's colour or their band's of the local solar
 * day (filtered ones dimmed, excluded ones hollow, selected ones ringed),
 * and the hovered point.
 */
function draw(canvas: HTMLCanvasElement, result: PolarPlotResult | null, dots: DotPacket, hover: Hover | null,
  colours: ReadonlyMap<number, SourceStyle>, selected: ReadonlySet<number>, unit: SpeedUnit, asymmetric: boolean,
  dotColour: DotColourMode, measuring: Measuring | null) {
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
  const layout = fitLayout(width, height, maxBsp, 28, asymmetric);

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
    ctx.arc(layout.centerX, layout.centerY, r, -Math.PI / 2, asymmetric ? Math.PI * 1.5 : Math.PI / 2);
    ctx.stroke();
  }
  for (const angle of asymmetric ? FULL_ANGLE_TICKS : ANGLE_TICKS) {
    const edge = projectPoint(angle, maxBsp, layout);
    ctx.beginPath();
    ctx.moveTo(layout.centerX, layout.centerY);
    ctx.lineTo(edge.x, edge.y);
    ctx.stroke();
  }
  for (const label of axisLabels(layout, maxBsp, (text) => ctx.measureText(text).width, 10, factor, asymmetric)) {
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
    const colour = dotFill(dots, k, dotColour, dotColours);
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

  // The Measure tool: a spoke at the pointer's wind angle, a ring at its
  // boat speed, a mark where the spoke crosses each curve, and the pinned
  // point A with the line from it to the pointer.
  if (measuring) {
    const { cursor, pin } = measuring;
    const outer = Math.max(maxBsp, cursor.bsp);
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 3]);
    const edge = projectPoint(cursor.twa, outer, layout);
    ctx.beginPath();
    ctx.moveTo(layout.centerX, layout.centerY);
    ctx.lineTo(edge.x, edge.y);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(layout.centerX, layout.centerY, cursor.bsp * layout.scale, -Math.PI / 2, asymmetric ? Math.PI * 1.5 : Math.PI / 2);
    ctx.stroke();
    ctx.setLineDash([]);
    for (const crossing of measuring.crossings) {
      const at = projectPoint(cursor.twa, crossing.bsp, layout);
      ctx.beginPath();
      ctx.arc(at.x, at.y, 3.5, 0, Math.PI * 2);
      ctx.fillStyle = crossing.colour;
      ctx.fill();
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = text;
      ctx.stroke();
    }
    if (pin) {
      const a = projectPoint(pin.twa, pin.bsp, layout);
      const b = projectPoint(cursor.twa, cursor.bsp, layout);
      ctx.strokeStyle = accent;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(a.x - 5, a.y);
      ctx.lineTo(a.x + 5, a.y);
      ctx.moveTo(a.x, a.y - 5);
      ctx.lineTo(a.x, a.y + 5);
      ctx.stroke();
      ctx.fillStyle = text;
      ctx.textAlign = "left";
      ctx.textBaseline = "bottom";
      ctx.fillText("A", a.x + 6, a.y - 3);
    }
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
  /** `"panel"` in the right panel; `"overlay"` full size over the current view. */
  variant: "panel" | "overlay";
  /** Panel variant only: opens the full-size overlay. */
  onFullSize?: () => void;
  /** Overlay variant only: closes it. */
  onClose?: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [result, setResult] = useState<PolarPlotResult | null>(null);
  const [dots, setDots] = useState<DotPacket>(emptyDots);
  const [tws, setTws] = useState<number | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  // The blend cell behind a hovered blend point (spec.md 9.2). `blendAsked`
  // names the cell last asked of Rust, so moving within one point asks
  // nothing more and an answer for a cell since left is dropped.
  const [blendCell, setBlendCell] = useState<BlendCell | null>(null);
  const blendAsked = useRef("");
  const [showFiltered, setShowFiltered] = useState(false);
  const [dotColour, setDotColour] = useState<DotColourMode>("source");
  // The Measure tool (spec.md 9.2): on or off, where the pointer is, and
  // the pinned point A. View state only; nothing of it is saved.
  const [measure, setMeasure] = useState(false);
  const [cursor, setCursor] = useState<PolarPoint | null>(null);
  const [pin, setPin] = useState<PolarPoint | null>(null);
  const request = useRef(0);
  const selection = useSampleSelection();

  const visibleCount = useMemo(
    () => project.sources.filter((source) => source.visible).length,
    [project.sources],
  );
  const tracksShown = project.sources.some((source) => source.visible && source.kind === "track");
  // Only a track's samples have a time of day; without one, by source.
  const shownDotColour: DotColourMode = tracksShown ? dotColour : "source";

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
        setHover(null);
      })
      .catch((err) => { if (request.current === id) reportFailure(err); });
  }, [project.id, project.revision, tws, showFiltered]);

  const sourcesById = useMemo(() => {
    const map = new Map<number, SourceStyle>();
    for (const source of project.sources) map.set(source.id, { label: source.label, colour: source.colour });
    return map;
  }, [project.sources]);

  // Every curve drawn, the blend's with its label in the interface language.
  const allCurves = useMemo(() => [...(result?.curves ?? []), ...blendCurves(result, t)], [result, t]);
  const measuring: Measuring | null = useMemo(
    () => (measure && cursor ? { cursor, crossings: crossings(allCurves, cursor.twa), pin } : null),
    [measure, cursor, pin, allCurves],
  );

  const redraw = useCallback(() => {
    if (canvas.current) draw(canvas.current, result, dots, hover, sourcesById, selection.ids, unit, project.blend.asymmetric, shownDotColour, measuring);
  }, [result, dots, hover, sourcesById, selection, unit, project.blend.asymmetric, shownDotColour, measuring]);

  useEffect(redraw, [redraw]);

  useEffect(() => {
    const element = wrap.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(redraw);
    observer.observe(element);
    return () => observer.disconnect();
  }, [redraw]);

  useEffect(() => onThemeChange(redraw), [redraw]);

  /** The pointer's place on the plot, or null where the fan is not (the port side of a symmetric plot). */
  const pointed = useCallback((event: MouseEvent<HTMLCanvasElement>): { point: PolarPoint; scale: number } | null => {
    const rect = event.currentTarget.getBoundingClientRect();
    const layout = fitLayout(rect.width, rect.height, Math.max(plotMaxBsp(result, dots), 1), 28, project.blend.asymmetric);
    const x = event.clientX - rect.left;
    if (!project.blend.asymmetric && x < layout.centerX - 1) return null;
    const point = unproject(x, event.clientY - rect.top, layout);
    // A symmetric plot's angles stop at 180°: a hair to the left of the axis is the axis.
    if (!project.blend.asymmetric && point.twa > 180) point.twa = point.twa > 270 ? 0 : 180;
    return { point, scale: layout.scale };
  }, [result, dots, project.blend.asymmetric]);

  const onMove = useCallback((event: MouseEvent<HTMLCanvasElement>) => {
    if (!result) return;
    if (measure) {
      // Measuring replaces the hover: the readout says more than a tooltip.
      setCursor(pointed(event)?.point ?? null);
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const layout = fitLayout(rect.width, rect.height, Math.max(plotMaxBsp(result, dots), 1), 28, project.blend.asymmetric);
    const hit = nearestPoint(
      allCurves, dots, sourcesById,
      event.clientX - rect.left, event.clientY - rect.top, layout, HOVER_DISTANCE_PX,
    );
    setHover(hit);
    // A blend point that is an output-grid cell names the sources behind
    // it; one between cells (a slice off the grid's wind speeds, a spline's
    // in-between angle) has no single cell to name and keeps the plain text.
    const on = (axis: readonly number[], value: number) => axis.findIndex((v) => Math.abs(v - value) <= ON_GRID);
    const i = hit?.blend ? on(project.blend.twa, hit.twa) : -1;
    const j = hit?.blend ? on(project.blend.tws, hit.tws) : -1;
    const key = i >= 0 && j >= 0 ? `${project.id}:${project.revision}:${i}:${j}` : "";
    if (key === blendAsked.current) return;
    blendAsked.current = key;
    setBlendCell(null);
    if (key === "") return;
    void api.blendCell(i, j)
      .then((cell) => { if (blendAsked.current === key) setBlendCell(cell); })
      // A cell that cannot be read keeps the plain tooltip.
      .catch(() => undefined);
  }, [result, dots, sourcesById, allCurves, measure, pointed, project.id, project.revision, project.blend.twa, project.blend.tws, project.blend.asymmetric, api]);
  const leave = useCallback(() => {
    setHover(null);
    setCursor(null);
    blendAsked.current = "";
    setBlendCell(null);
  }, []);

  /** Pins point A where the pointer is: on a curve's own value when the click is on its mark. */
  const onClick = useCallback((event: MouseEvent<HTMLCanvasElement>) => {
    if (!measure) return;
    const at = pointed(event);
    if (!at) return;
    const here = crossings(allCurves, at.point.twa);
    const near = here[nearestCrossing(here, at.point.bsp)];
    const snapped = near !== undefined && Math.abs(near.bsp - at.point.bsp) * at.scale <= PIN_SNAP_PX;
    setPin({ twa: at.point.twa, bsp: snapped ? near.bsp : at.point.bsp });
    setCursor(at.point);
  }, [measure, pointed, allCurves]);

  /** Turns the Measure tool on or off; off forgets the pinned point. */
  const toggleMeasure = useCallback(() => {
    setMeasure((on) => !on);
    setPin(null);
    setCursor(null);
    setHover(null);
    blendAsked.current = "";
    setBlendCell(null);
  }, []);

  // Escape lets the pinned point go, before anything else hears it (the
  // full-size overlay closes on Escape): one press unpins, the next closes.
  useEffect(() => {
    if (!measure || !pin) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopImmediatePropagation();
      event.preventDefault();
      setPin(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [measure, pin]);

  const domainMin = result?.tws_min ?? null;
  const domainMax = result?.tws_max ?? null;
  const hasDomain = domainMin !== null && domainMax !== null;
  const sliderValue = tws ?? (hasDomain ? Math.round(((domainMin as number) + (domainMax as number)) / 2) : 0);
  const hasAnyPoint = [...(result?.curves ?? []), ...(result?.blend ?? [])].some((curve) => curve.points.length > 0)
    || dots.count > 0;
  // The readout's numbers: every one arrives in knots and leaves in the display unit.
  const symbol = SPEED_SYMBOL[unit];
  const shownSpeed = (knots: number) => `${(knots * SPEED_FACTOR[unit]).toFixed(2)} ${symbol}`;
  const shownAngle = (twa: number) => Math.min(twa, 360 - twa).toFixed(0);
  const reference = measuring ? measuring.crossings[nearestCrossing(measuring.crossings, measuring.cursor.bsp)] : undefined;
  const pinned = measuring?.pin ? measureBetween(measuring.pin, measuring.cursor) : null;

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
        <label className="polar-plot-colour">
          {t("Colour")}
          <select data-feature="plot:colour" value={shownDotColour} disabled={!tracksShown}
            title={tracksShown ? t("What the dots' colour shows") : t("No visible track has samples to colour")}
            onChange={(event) => setDotColour(event.target.value as DotColourMode)}>
            <option value="source">{t("By source")}</option>
            <option value="timeOfDay">{t("By time of day")}</option>
          </select>
        </label>
        <button className="small" data-feature="plot:measure" aria-pressed={measure}
          disabled={!hasAnyPoint && !measure} onClick={toggleMeasure}
          title={t("Measure boat speeds on the plot: point to compare every curve at one wind angle, click to pin a point and measure from it")}>
          {t("Measure")}
        </button>
        {variant === "panel" && (
          <button className="small" data-feature="plot:full-size" onClick={onFullSize}
            title={t("Open the polar plot full size over the current view")}>
            {t("Full size")}
          </button>
        )}
        {variant === "overlay" && (
          <button className="small" data-feature="plot:close" onClick={onClose} title={t("Close")} aria-label={t("Close")}>
            {t("Close")}
          </button>
        )}
      </div>
      {shownDotColour === "timeOfDay" && <DayBandLegend className="polar-plot-bands" />}
      {visibleCount === 0 ? (
        <div className="plot-placeholder muted">{t("The polar plot appears here once the project has a source.")}</div>
      ) : (
        <div className="polar-plot-canvas-wrap" ref={wrap}>
          <canvas ref={canvas} className={measure ? "measuring" : undefined} onMouseMove={onMove} onMouseLeave={leave} onClick={onClick} />
          {measuring && (
            <div className="polar-plot-measure" role="status">
              <h4>{t("At TWA {twa}°", { twa: shownAngle(measuring.cursor.twa) })}</h4>
              <div className="muted">{t("Pointer: {bsp}", { bsp: shownSpeed(measuring.cursor.bsp) })}</div>
              {measuring.crossings.length === 0 ? (
                <div className="muted">{t("No curve at this angle")}</div>
              ) : (
                <ul>
                  {measuring.crossings.map((crossing, index) => (
                    <li key={index} className={crossing === reference ? "reference" : undefined}>
                      <span className="polar-plot-measure-name">
                        <span className="day-band-swatch" style={{ background: crossing.colour }} />
                        {crossing.label} · {displaySpeed(crossing.tws, unit, 1)} {symbol}
                      </span>
                      <span>{shownSpeed(crossing.bsp)}</span>
                      {crossing === reference || !reference ? (
                        <span className="muted polar-plot-measure-delta">{t("pointed at")}</span>
                      ) : (
                        <>
                          <span>{signed((crossing.bsp - reference.bsp) * SPEED_FACTOR[unit], 2)} {symbol}</span>
                          <span>{reference.bsp > 0 ? `${signed((crossing.bsp / reference.bsp - 1) * 100, 1)}%` : ""}</span>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              {measuring.pin && pinned ? (
                <div className="polar-plot-measure-pin">
                  <div>{t("A: {bsp} at {twa}°", { bsp: shownSpeed(measuring.pin.bsp), twa: shownAngle(measuring.pin.twa) })}</div>
                  <div>{pinned.ratio === null
                    ? t("B − A: {delta}, {angle}° apart", {
                      delta: `${signed(pinned.deltaBsp * SPEED_FACTOR[unit], 2)} ${symbol}`, angle: pinned.deltaTwa.toFixed(0),
                    })
                    : t("B − A: {delta} ({percent}%), {angle}° apart", {
                      delta: `${signed(pinned.deltaBsp * SPEED_FACTOR[unit], 2)} ${symbol}`,
                      percent: signed((pinned.ratio - 1) * 100, 1), angle: pinned.deltaTwa.toFixed(0),
                    })}</div>
                </div>
              ) : null}
              <div className="muted">{measuring.pin ? t("Click to move A; Esc lets it go") : t("Click to pin point A")}</div>
            </div>
          )}
          {hover && blendCell && (
            <BlendCellTooltip cell={blendCell} sources={project.sources} colour={project.blend.colour} unit={unit}
              smoothing={project.blend.smoothing} x={hover.x + 10} y={hover.y + 10} />
          )}
          {hover && !blendCell && (
            <div role="tooltip" className="polar-plot-tooltip" style={{ left: hover.x + 10, top: hover.y + 10 }}>
              <strong style={{ color: hover.colour }}>{hover.label}</strong>
              <div>{t("TWA {twa}°, TWS {tws} {unit}, BSP {bsp} {unit}", {
                twa: Math.min(hover.twa, 360 - hover.twa).toFixed(0), tws: (hover.tws * SPEED_FACTOR[unit]).toFixed(1),
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
