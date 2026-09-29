import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { registerRedraw } from "../automation";
import { reportFailure } from "../errors";
import type { AppSettings } from "../generated/AppSettings";
import type { CompareOperand } from "../generated/CompareOperand";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { setHint } from "../hint";
import { msg, useT } from "../i18n";
import { api } from "../ipc";
import type { Layout } from "../polar/geometry3d";
import { PolarScene } from "../polar/scene3d";
import { buildGuides, presetView, SPEED_FACTOR, SPEED_SYMBOL, type CameraPreset, type GuideLabel } from "../polar/view3d";
import { onThemeChange } from "../settings/themes";
import { CLASS_A_ONLY, CLASS_B_ONLY, CLASS_BOTH, emptyCompare, type ComparePacket } from "./comparePacket";
import { operandOf, setCompareChoice, swapOperands, useCompareChoice } from "./compareState";
import {
  compareBounds, compareSurfaces, DEFAULT_COMPARE_TOGGLES, heatRows, operandInfo, operandKey, regionRows, spanText,
  type CompareToggles, type HeatCell,
} from "./compareModel";
import { currentScheme, legendGradient, POLES, type Scheme } from "./diverging";

const LAYOUT_NAMES: Record<Layout, string> = { tower: msg("Polar tower"), cartesian: msg("Cartesian") };
const CAMERAS: readonly { id: CameraPreset; label: string; tip: string }[] = [
  { id: "top", label: msg("Top"), tip: msg("Look down the wind-speed axis: the classic polar diagram") },
  { id: "side", label: msg("Side"), tip: msg("Look across the wind-speed axis: each wind speed a level") },
  { id: "iso", label: msg("Isometric"), tip: msg("The three-quarter view") },
];
const KIND_GLYPH: Record<string, string> = { orc: "◆", polar_file: "▦", track: "〰", blend: "◎" };

function cssColour(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * The Compare stage (spec.md 11): two operands, A and B — any source's
 * polar, any track's polar segment, or the blend — as translucent surfaces
 * in 3D with the difference surface ΔBSP = A − B between them on a
 * diverging scale centred on zero; cells only one operand covers drawn grey
 * and hatched. Beside it: the scale's legend and range, the summary
 * (overlap, mean and max |Δ|, the regions where each is faster) and a 2D
 * heat map of Δ. Rust computes every value; the stage refetches whenever
 * the project changes, so an edit reaches it at once.
 */
export default function CompareView({ project, settings }: {
  project: ProjectSummary;
  settings: AppSettings | null;
}) {
  const t = useT();
  const canvas = useRef<HTMLCanvasElement>(null);
  const labelsHost = useRef<HTMLDivElement>(null);
  const scene = useRef<PolarScene | null>(null);
  const frame = useRef(0);
  const labels = useRef<GuideLabel[]>([]);
  const request = useRef(0);
  const fitted = useRef(false);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [packet, setPacket] = useState<ComparePacket>(emptyCompare);
  const [arrived, setArrived] = useState(false);
  const [layout, setLayout] = useState<Layout>("tower");
  const [toggles, setToggles] = useState<CompareToggles>(DEFAULT_COMPARE_TOGGLES);
  const [scheme, setScheme] = useState<Scheme>(currentScheme);
  const [hovered, setHovered] = useState<HeatCell | null>(null);
  const choice = useCompareChoice(project);
  const { a, b, percent, thresholdKn } = choice;
  const unit = settings?.units.speed ?? "kn";
  const factor = SPEED_FACTOR[unit];
  const symbol = SPEED_SYMBOL[unit];

  const paint = useCallback(() => {
    const current = scene.current;
    if (!current) return;
    current.render();
    const host = labelsHost.current;
    if (!host) return;
    labels.current.forEach((label, k) => {
      const span = host.children[k] as HTMLElement | undefined;
      if (!span) return;
      const at = current.toScreen(...label.at);
      span.style.display = at ? "" : "none";
      if (at) span.style.transform = `translate(${at[0]}px, ${at[1]}px)`;
    });
  }, []);

  const draw = useCallback(() => {
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      paint();
    });
  }, [paint]);

  // The scene lives as long as the stage.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    let made: PolarScene;
    try {
      made = new PolarScene(element);
    } catch {
      setUnavailable(msg("The 3D view needs WebGL, which this system does not offer."));
      return;
    }
    scene.current = made;
    const offRedraw = registerRedraw(element, paint);
    made.setBackground(cssColour("--inset", "#1f2c3c"));
    made.enableControls(element, draw);
    const resize = () => {
      made.resize(element.clientWidth, element.clientHeight);
      draw();
    };
    resize();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    observer?.observe(element);
    const offTheme = onThemeChange(() => {
      made.setBackground(cssColour("--inset", "#1f2c3c"));
      setScheme(currentScheme());
      draw();
    });
    return () => {
      observer?.disconnect();
      offTheme();
      offRedraw();
      cancelAnimationFrame(frame.current);
      frame.current = 0;
      made.dispose();
      scene.current = null;
    };
  }, [draw, paint]);

  useEffect(() => { fitted.current = false; }, [project.id]);

  // Refetched on every document change (an edit anywhere may move either
  // operand) and on every new choice.
  const operands = `${operandKey(a)}|${operandKey(b)}`;
  useEffect(() => {
    const id = ++request.current;
    void api.comparePolars(a, b, thresholdKn)
      .then((next) => { if (request.current === id) { setPacket(next); setArrived(true); } })
      .catch((error: unknown) => { if (request.current === id) reportFailure(error); });
  }, [project.id, project.revision, operands, thresholdKn]);

  const infoA = operandInfo(project, a);
  const infoB = operandInfo(project, b);
  const bounds = useMemo(() => compareBounds(packet, layout), [packet, layout]);

  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    const empty = new Float32Array(0);
    current.setData({
      samples: empty, colors: empty, layout,
      surfaces: compareSurfaces(packet, toggles, { a: infoA.colour, b: infoB.colour }, percent, scheme),
    });
    const guides = buildGuides(bounds, layout, unit);
    current.setGuides(guides.segments, cssColour("--muted", "#b3c9de"));
    labels.current = guides.labels;
    const host = labelsHost.current;
    if (host) {
      host.replaceChildren(...guides.labels.map((label) => {
        const span = document.createElement("span");
        span.textContent = label.text;
        return span;
      }));
    }
    // Framed once the first comparison has arrived, not round the empty one.
    if (!fitted.current && arrived) {
      current.setView(presetView("iso", bounds, current.camera.fov));
      fitted.current = true;
    }
    draw();
  }, [packet, arrived, toggles, layout, bounds, unit, percent, scheme, infoA.colour, infoB.colour, draw]);

  const camera = (preset: CameraPreset) => {
    const current = scene.current;
    if (!current) return;
    current.setView(presetView(preset, bounds, current.camera.fov));
    draw();
  };

  const chooseLayout = (next: Layout) => {
    setLayout(next);
    const current = scene.current;
    if (current) current.setView(presetView("iso", compareBounds(packet, next), current.camera.fov));
  };

  /** Δ in the unit shown, signed. */
  const delta = (value: number) => {
    if (!Number.isFinite(value)) return "—";
    const sign = value > 0 ? "+" : value < 0 ? "−" : "";
    return percent ? `${sign}${Math.abs(value).toFixed(1)} %` : `${sign}${Math.abs(value * factor).toFixed(2)} ${symbol}`;
  };
  const size = (value: number) => (!Number.isFinite(value) ? "—"
    : percent ? `${value.toFixed(1)} %` : `${(value * factor).toFixed(2)} ${symbol}`);
  const speed = (knots: number) => (Number.isFinite(knots) ? `${(knots * factor).toFixed(2)} ${symbol}` : "—");
  const windSpeed = (knots: number) => `${Number((knots * factor).toFixed(1))} ${symbol}`;

  const stats = percent ? packet.pct : packet.kn;
  const where = (cell: number) => {
    if (cell < 0) return null;
    const nj = packet.tws.length;
    return { twa: packet.twa[Math.floor(cell / nj)]!, tws: packet.tws[cell % nj]! };
  };
  const maxAt = stats ? where(stats.maxCell) : null;
  const regions = useMemo(() => regionRows(packet), [packet]);
  const heat = useMemo(() => heatRows(packet, percent, scheme), [packet, percent, scheme]);

  const [thresholdText, setThresholdText] = useState<string | null>(null);
  const shownThreshold = thresholdText ?? String(Number((thresholdKn * factor).toFixed(3)));
  const commitThreshold = (text: string) => {
    const value = Number(text.replace(",", "."));
    setThresholdText(null);
    if (Number.isFinite(value) && value >= 0) setCompareChoice(project, { thresholdKn: value / factor });
  };

  return (
    <div className="view3d compare" tabIndex={-1}>
      <canvas ref={canvas} className="view3d-canvas compare-canvas"
        onPointerEnter={() => setHint(t("Drag to turn, right-drag to pan, scroll to zoom."))}
        onPointerLeave={() => setHint(null)} />
      <div className="view3d-labels compare-labels" ref={labelsHost} aria-hidden="true" />
      {unavailable !== null && <p className="view3d-unavailable muted">{t(unavailable)}</p>}
      {unavailable === null && arrived && packet.overlap === 0 && (
        <p className="view3d-empty compare-empty muted">{t("A and B share no cell with a value: there is nothing to compare. Cells only one of them covers are drawn grey and hatched.")}</p>
      )}

      <div className="view3d-toolbar compare-toolbar">
        <select data-feature="compare:layout" aria-label={t("Layout")} value={layout}
          title={t("Polar tower: angle and radius are TWA and BSP, height is TWS. Cartesian: TWA, TWS and BSP on straight axes.")}
          onChange={(event) => chooseLayout(event.target.value as Layout)}>
          {(Object.keys(LAYOUT_NAMES) as Layout[]).map((id) => <option key={id} value={id}>{t(LAYOUT_NAMES[id])}</option>)}
        </select>
        <span className="view3d-group" role="group" aria-label={t("Camera")}>
          {CAMERAS.map((c) => (
            <button key={c.id} className="small" data-feature={`compare:camera-${c.id}`} title={t(c.tip)} onClick={() => camera(c.id)}>
              {t(c.label)}
            </button>
          ))}
        </span>
      </div>

      <div className="compare-side">
        <section className="compare-operands" aria-label={t("Operands")}>
          <OperandPicker which="a" project={project} value={a}
            onChange={(next) => setCompareChoice(project, { a: next })} />
          <button className="small compare-swap" data-feature="compare:swap" title={t("Swap A and B")}
            aria-label={t("Swap A and B")} onClick={() => swapOperands(project)}>⇅</button>
          <OperandPicker which="b" project={project} value={b}
            onChange={(next) => setCompareChoice(project, { b: next })} />
        </section>

        <fieldset className="view3d-show compare-show">
          <legend>{t("Show")}</legend>
          <label title={t("Operand A as a translucent surface in its colour")}>
            <input type="checkbox" data-feature="compare:show-a" checked={toggles.a}
              onChange={(event) => setToggles({ ...toggles, a: event.target.checked })} />
            {t("Surface A")}
          </label>
          <label title={t("Operand B as a translucent surface in its colour")}>
            <input type="checkbox" data-feature="compare:show-b" checked={toggles.b}
              onChange={(event) => setToggles({ ...toggles, b: event.target.checked })} />
            {t("Surface B")}
          </label>
          <label title={t("Midway between A and B, coloured by the difference A − B; grey and hatched where only one has a value")}>
            <input type="checkbox" data-feature="compare:show-delta" checked={toggles.delta}
              onChange={(event) => setToggles({ ...toggles, delta: event.target.checked })} />
            {t("Difference surface")}
          </label>
          <label title={t("Show the difference as a percentage of B instead of a speed")}>
            <input type="checkbox" data-feature="compare:percent" checked={percent}
              onChange={(event) => setCompareChoice(project, { percent: event.target.checked })} />
            {t("Δ as percent of B")}
          </label>
          <label className="compare-threshold"
            title={t("A cell counts as A faster or B faster only when the difference is more than this. Not saved.")}>
            {t("Threshold")}
            <input type="text" inputMode="decimal" data-feature="compare:threshold" value={shownThreshold}
              aria-label={t("Threshold")}
              onChange={(event) => setThresholdText(event.target.value)}
              onBlur={(event) => commitThreshold(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") commitThreshold(event.currentTarget.value);
                if (event.key === "Escape") setThresholdText(null);
              }} />
            <span className="muted">{symbol}</span>
          </label>
        </fieldset>

        <section className="compare-legend" aria-label={t("Difference scale")} data-feature="compare:legend">
          <div className="compare-legend-ends">
            <span>{t("B faster")}</span><span>{t("A faster")}</span>
          </div>
          <div className="compare-legend-bar" style={{ background: legendGradient(scheme) }} />
          <div className="compare-legend-ends muted">
            <span>{delta(-Math.max(Math.abs(stats.min), Math.abs(stats.max)))}</span>
            <span>0</span>
            <span>{delta(Math.max(Math.abs(stats.min), Math.abs(stats.max)))}</span>
          </div>
          <div className="muted">
            {Number.isFinite(stats.min)
              ? t("Range: {min} to {max}", { min: delta(stats.min), max: delta(stats.max) })
              : t("No compared cell")}
          </div>
          <div className="compare-legend-single">
            <span className="compare-hatch-swatch" style={hatchStyle(scheme)} />
            {t("Only one of A and B has a value")}
          </div>
        </section>

        <section className="compare-heat" aria-label={t("Difference heat map")} data-feature="compare:heat-map">
          <h3>{t("Difference heat map")}</h3>
          <HeatMap rows={heat} tws={[...packet.tws]} scheme={scheme} windSpeed={windSpeed}
            onHover={setHovered} />
          <div className="compare-heat-readout muted" aria-live="polite">
            {hovered
              ? t("TWA {twa}°, TWS {tws}: A {a}, B {b}, Δ {delta}", {
                twa: Number(hovered.twa.toFixed(2)),
                tws: windSpeed(hovered.tws),
                a: speed(hovered.a), b: speed(hovered.b),
                delta: hovered.cls === CLASS_BOTH ? delta(hovered.delta)
                  : hovered.cls === CLASS_A_ONLY ? t("only A") : hovered.cls === CLASS_B_ONLY ? t("only B") : "—",
              })
              : t("Hover a cell for its values. Rows are TWA, columns TWS.")}
          </div>
        </section>

        <section className="compare-summary" aria-label={t("Summary")} data-feature="compare:summary">
          <h3>{t("Summary")}</h3>
          <div>{t("{count} cells compared", { count: packet.overlap })}</div>
          <div className="muted">{t("{a} only A, {b} only B (hatched)", { a: packet.aOnly, b: packet.bOnly })}</div>
          <div>{t("Mean |Δ| {mean}", { mean: size(stats.meanAbs) })}</div>
          <div>
            {maxAt
              ? t("Max |Δ| {max} at {twa}°, {tws}", { max: size(stats.maxAbs), twa: Number(maxAt.twa.toFixed(2)), tws: windSpeed(maxAt.tws) })
              : t("Max |Δ| {max}", { max: size(stats.maxAbs) })}
          </div>
          <h4>{t("Where each is faster (more than {threshold})", { threshold: `${Number((packet.thresholdKn * factor).toFixed(3))} ${symbol}` })}</h4>
          {regions.length === 0
            ? <div className="muted">{t("Nowhere: A and B are within the threshold wherever both have a value.")}</div>
            : (
              <ul className="compare-regions">
                {regions.map((row) => (
                  <li key={row.tws}>
                    <span className="compare-region-tws">{windSpeed(row.tws)}</span>
                    {row.a.length > 0 && (
                      <span className="compare-region-a">{t("A faster {spans}", { spans: row.a.map(spanText).join(", ") })}</span>
                    )}
                    {row.b.length > 0 && (
                      <span className="compare-region-b">{t("B faster {spans}", { spans: row.b.map(spanText).join(", ") })}</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
        </section>
      </div>
    </div>
  );
}

function hatchStyle(scheme: Scheme) {
  const p = POLES[scheme];
  return { background: `repeating-linear-gradient(45deg, ${p.hatch} 0 1.5px, ${p.single} 1.5px 5px)` };
}

/** The 2D Δ heat map (spec.md 11): TWA rows, TWS columns, the 0° row left out. */
function HeatMap({ rows, tws, scheme, windSpeed, onHover }: {
  rows: { twa: number; cells: HeatCell[] }[];
  tws: number[];
  scheme: Scheme;
  windSpeed: (knots: number) => string;
  onHover: (cell: HeatCell | null) => void;
}) {
  const t = useT();
  const cw = 26, ch = 14, left = 34, top = 16;
  const width = left + tws.length * cw;
  const height = top + rows.length * ch;
  const p = POLES[scheme];
  const id = `compare-hatch-${scheme}`;
  return (
    <svg className="compare-heat-map" viewBox={`0 0 ${width} ${height}`} role="img"
      aria-label={t("Difference heat map")} onMouseLeave={() => onHover(null)}>
      <defs>
        <pattern id={id} width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <rect width="5" height="5" fill={p.single} />
          <line x1="0" y1="0" x2="0" y2="5" stroke={p.hatch} strokeWidth="1.5" />
        </pattern>
      </defs>
      {tws.map((speed, j) => (
        <text key={j} x={left + j * cw + cw / 2} y={11} textAnchor="middle" className="compare-heat-axis">
          {Number(speed.toFixed(1))}
        </text>
      ))}
      {rows.map((row, r) => (
        <g key={row.twa}>
          <text x={left - 4} y={top + r * ch + ch - 3} textAnchor="end" className="compare-heat-axis">{`${Number(row.twa.toFixed(1))}°`}</text>
          {row.cells.map((cell) => {
            const fill = cell.fill ?? (cell.cls === CLASS_A_ONLY || cell.cls === CLASS_B_ONLY ? `url(#${id})` : "none");
            return (
              <rect key={cell.j} x={left + cell.j * cw + 1} y={top + r * ch + 1} width={cw - 2} height={ch - 2} rx={2}
                fill={fill} className={cell.cls === CLASS_BOTH ? "compare-heat-cell" : "compare-heat-cell empty"}
                onMouseEnter={() => onHover(cell)}>
                <title>{`${Number(row.twa.toFixed(2))}° · ${windSpeed(tws[cell.j]!)}`}</title>
              </rect>
            );
          })}
        </g>
      ))}
    </svg>
  );
}

/**
 * One operand's picker (spec.md 11): the blend and every source, each by
 * its colour and name, a track standing for its polar segment. A native
 * list cannot show colours, so this is a button and a list box.
 */
function OperandPicker({ which, project, value, onChange }: {
  which: "a" | "b";
  project: ProjectSummary;
  value: CompareOperand;
  onChange: (operand: CompareOperand) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!host.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [open]);
  const info = operandInfo(project, value);
  const options: { operand: CompareOperand; colour: string; label: string; kind: string; hidden: boolean }[] = [
    { operand: { kind: "blend" }, colour: project.blend.colour, label: t("Blend"), kind: "blend", hidden: false },
    ...project.sources.map((s) => ({ operand: operandOf(s), colour: s.colour, label: s.label, kind: s.kind, hidden: !s.visible })),
  ];
  const letter = which === "a" ? "A" : "B";
  const kindName = (kind: string) => (kind === "track" ? t("polar segment") : kind === "blend" ? t("the current blend") : t("polar"));
  return (
    <div className={`compare-picker compare-picker-${which}`} ref={host}
      onKeyDown={(event) => { if (event.key === "Escape" && open) { event.stopPropagation(); setOpen(false); } }}>
      <span className="compare-picker-letter">{letter}</span>
      <button className="compare-picker-button" data-feature={`compare:operand-${which}`} aria-haspopup="listbox"
        aria-expanded={open}
        title={which === "a" ? t("Operand A: a source's polar, a track's polar segment, or the blend")
          : t("Operand B: a source's polar, a track's polar segment, or the blend")}
        onClick={() => setOpen(!open)}>
        <span className="swatch" style={{ background: info.colour }} />
        <span className="compare-picker-name">{info.label ?? t("Blend")}</span>
        <span className="muted">{kindName(info.kind)}</span>
        <span aria-hidden="true">▾</span>
      </button>
      {open && (
        <ul className="compare-picker-list" role="listbox" aria-label={which === "a" ? t("Operand A") : t("Operand B")}>
          {options.map((option) => {
            const selected = operandKey(option.operand) === operandKey(value);
            return (
              <li key={operandKey(option.operand)} role="option" aria-selected={selected}>
                <button className={selected ? "selected" : undefined}
                  onClick={() => { onChange(option.operand); setOpen(false); }}>
                  <span className="swatch" style={{ background: option.colour }} />
                  <span aria-hidden="true">{KIND_GLYPH[option.kind] ?? ""}</span>
                  <span className="compare-picker-name">{option.label}</span>
                  <span className="muted">{kindName(option.kind)}{option.hidden ? ` · ${t("hidden")}` : ""}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
