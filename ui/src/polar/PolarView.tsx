import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

import { registerRedraw } from "../automation";
import { needsOutline } from "../colourContrast";
import { reportFailure } from "../errors";
import type { AppSettings } from "../generated/AppSettings";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { setHint } from "../hint";
import { msg, useT } from "../i18n";
import { api } from "../ipc";
import { focusMap, selectSamples, useSampleSelection } from "../selection";
import { onThemeChange } from "../settings/themes";
import EditPanel, { cellCode } from "./EditPanel";
import { editSource, useEditFocus } from "./editFocus";
import { place, type Layout } from "./geometry3d";
import { PolarScene } from "./scene3d";
import { emptyScene, ScenePacketError, type ScenePacket } from "./scenePacket";
import {
  availableModes, buildGuides, buildSurfaces, combine, DEFAULT_TOGGLES, drawnOnly, editCells, emptyKeys,
  exclusionTargets, focusIndex, hasFiltered, keysOf, mergeDots, nodeDots, nodesAtCells, sampleDots, presetView, range, resolveKeys, sampleIdsOf, sceneBounds,
  SPEED_FACTOR, SPEED_SYMBOL, summarise, type CameraPreset, type ColourMode, type Focus, type GuideLabel, type SelectionKeys,
  type Toggles,
} from "./view3d";

type Tool = "rotate" | "lasso" | "box" | "drag";

/** Shift snaps a dragged node to this step, knots (spec.md 10.4). */
export const DRAG_SNAP_KN = 0.05;
/** A drag along a BSP axis seen end-on moves this many pixels per knot, straight up. */
const FALLBACK_PX_PER_KN = 20;

/**
 * The boat speed a node dragged by (`dx`, `dy`) pixels reaches: the drag
 * projected on the screen direction of one knot more BSP (`axis`, pixels),
 * snapped to 0.05 kn with Shift, and kept to 0–60 kn.
 */
export function draggedSpeed(start: number, axis: readonly [number, number], dx: number, dy: number, snap: boolean): number {
  let [ax, ay] = axis;
  if (Math.hypot(ax, ay) < 5) [ax, ay] = [0, -FALLBACK_PX_PER_KN];
  let value = start + (dx * ax + dy * ay) / (ax * ax + ay * ay);
  if (snap) value = Math.round(value / DRAG_SNAP_KN) * DRAG_SNAP_KN;
  return Math.round(Math.min(60, Math.max(0, value)) * 1000) / 1000;
}

/** A pointer that moved less than this, in pixels, clicked rather than dragged. */
const CLICK_SLOP_PX = 4;
/** A click picks the nearest dot within this many pixels. */
const PICK_RADIUS_PX = 8;

const LAYOUT_NAMES: Record<Layout, string> = { tower: msg("Polar tower"), cartesian: msg("Cartesian") };
const MODE_NAMES: Record<ColourMode, string> = {
  source: msg("By source"), hs: msg("By wave height"), current: msg("By current speed"), time: msg("By time"),
};
const CAMERAS: readonly { id: CameraPreset; label: string; tip: string }[] = [
  { id: "top", label: msg("Top"), tip: msg("Look down the wind-speed axis: the classic polar diagram") },
  { id: "side", label: msg("Side"), tip: msg("Look across the wind-speed axis: each wind speed a level") },
  { id: "iso", label: msg("Isometric"), tip: msg("The three-quarter view") },
];
const TOOLS: readonly { id: Tool; label: string; tip: string }[] = [
  { id: "rotate", label: msg("Rotate"), tip: msg("Drag to turn the view, right-drag to pan, scroll to zoom; click a dot to select it") },
  { id: "lasso", label: msg("Lasso"), tip: msg("Draw round dots to select them; Shift adds to the selection") },
  { id: "box", label: msg("Box"), tip: msg("Drag a box round dots to select them; Shift adds to the selection") },
  { id: "drag", label: msg("Drag"), tip: msg("Drag a node of the source being edited to change its boat speed; Shift snaps to 0.05 kn") },
];

let dragGestures = 0;

function cssColour(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * The 3D polar stage (spec.md 10.1–10.3): every visible polar source as a
 * translucent surface over its own grid, its nodes and every sample as dots,
 * in the polar-tower or Cartesian layout, with preset cameras, an axis
 * legend in the display units, and selection by click, Shift-click, lasso
 * or box. Exclude and Include are undoable changes made in Rust; the view
 * refetches the scene whenever the project's revision moves.
 *
 * The WebGL renderer, its geometries and the orbit controls are disposed on
 * unmount, as the map disposes its own.
 */
export default function PolarView({ project, settings, onProject }: {
  project: ProjectSummary;
  settings: AppSettings | null;
  onProject: (summary: ProjectSummary) => void;
}) {
  const t = useT();
  const canvas = useRef<HTMLCanvasElement>(null);
  const labelsHost = useRef<HTMLDivElement>(null);
  const scene = useRef<PolarScene | null>(null);
  const frame = useRef(0);
  const labels = useRef<GuideLabel[]>([]);
  const gesture = useRef<{ x: number; y: number; path: number[] } | null>(null);
  const request = useRef(0);
  const fitted = useRef(false);
  const selectionKeys = useRef<SelectionKeys>(emptyKeys());
  const shared = useSampleSelection();

  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [packet, setPacket] = useState<ScenePacket>(emptyScene);
  const [layout, setLayout] = useState<Layout>("tower");
  const [toggles, setToggles] = useState<Toggles>(DEFAULT_TOGGLES);
  const [mode, setMode] = useState<ColourMode>("source");
  const [tool, setTool] = useState<Tool>("rotate");
  const [selection, setSelection] = useState<number[]>([]);
  const [path, setPath] = useState<number[] | null>(null);
  const [hideOthers, setHideOthers] = useState(false);
  const [dragValue, setDragValue] = useState<{ x: number; y: number; bsp: number } | null>(null);
  const held = useRef<ScenePacket | null>(null);
  const drag = useRef<{
    start: number; axis: [number, number]; x: number; y: number; cell: { twa_index: number; tws_index: number };
    gesture: string; busy: boolean; pending: number | null; source: number;
  } | null>(null);
  const unit = settings?.units.speed ?? "kn";
  const requested = useEditFocus();
  // Edit mode needs the source to exist: a removed source ends it.
  const focus = requested !== null && project.sources.some((s) => s.id === requested) ? requested : null;
  useEffect(() => { if (requested !== null && focus === null) editSource(null); }, [requested, focus]);

  /** Renders now, and moves the axis labels to where their points now are. */
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

  /** Paints on the next frame, once. */
  const draw = useCallback(() => {
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      paint();
    });
  }, [paint]);

  // The scene lives as long as the stage: made on mount, disposed on unmount.
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
    // The WebDriver screenshot's synchronous redraw (development builds only).
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

  // A new project is framed afresh when its first dots arrive.
  useEffect(() => { fitted.current = false; }, [project.id]);

  // Refetches on every document change (`revision` moves with every command,
  // undo and redo included) and on switching project.
  // Only the samples' flags travel when no sample moved (the scene held
  // names its samples key); a scene that no longer matches is fetched whole.
  // A scene held from another project is never a base: it is dropped in the
  // same effect, before the fetch, so no effect order can send its key.
  const heldProject = useRef<number | null>(null);
  useEffect(() => {
    const id = ++request.current;
    if (heldProject.current !== project.id) {
      held.current = null;
      heldProject.current = project.id;
    }
    const base = held.current;
    void api.polarScene(focus, base)
      .catch((error: unknown) => {
        if (error instanceof ScenePacketError && base !== null) return api.polarScene(focus, null);
        throw error;
      })
      .then((next) => {
        if (request.current !== id) return;
        held.current = next;
        setPacket(next);
      })
      .catch((error: unknown) => { if (request.current === id) reportFailure(error); });
  }, [project.id, project.revision, focus]);

  // Leaving edit mode puts the Drag tool down.
  useEffect(() => {
    if (focus === null && tool === "drag") chooseTool("rotate");
  }, [focus]);

  /**
   * Sets the selection, remembering it by stable keys for the next refetch,
   * and shares its samples with the map (spec.md 9.1).
   */
  const select = useCallback((next: number[]) => {
    selectionKeys.current = keysOf(packet, next);
    setSelection(next);
    selectSamples(sampleIdsOf(packet, next), "3d");
  }, [packet]);

  // A refetched scene keeps the selection: dots are matched by source and
  // cell, or sample id, not by position in the buffer.
  useEffect(() => {
    setSelection(resolveKeys(packet, selectionKeys.current));
  }, [packet]);

  // A selection made on the map (or elsewhere) selects those samples here.
  useEffect(() => {
    if (shared.origin === "3d" || shared.origin === null) return;
    selectionKeys.current = { nodes: new Map(), samples: new Set(shared.ids) };
    setSelection(resolveKeys(packet, selectionKeys.current));
    // Only a new shared selection re-selects; a refetch keeps its own.
  }, [shared.version]);

  const bounds = useMemo(() => sceneBounds(packet, layout), [packet, layout]);
  const modes = useMemo(() => availableModes(packet), [packet]);
  const filteredExist = useMemo(() => hasFiltered(packet), [packet]);
  // A mode whose data has gone (the tracks were removed) falls back to source colours.
  const shownMode: ColourMode = modes[mode] ? mode : "source";
  const focused = useMemo(() => focusIndex(packet, focus), [packet, focus]);
  // The blend surface is drawn in the Blend entry's colour (spec.md 8; M7
  // carry), opaque (spec.md 10.1).
  const blendColour = project.blend.colour;
  // A blend colour lost on the background (a white blend on Paper) keeps
  // its grid lines in the theme's text colour, as an outline.
  const blendLine = needsOutline(blendColour, cssColour("--inset", "#1f2c3c")) ? cssColour("--text", "#d6e6f5") : undefined;
  const focusStyle: Focus | null = useMemo(
    () => (focus === null ? null : { index: focused, hideOthers }),
    [focus, focused, hideOthers],
  );
  // The samples' dots are rebuilt only when they, their colours or how they
  // are shown change: an edit of a polar node rebuilds the nodes alone
  // (spec.md 13, plan.md M13).
  const colourKey = packet.sources.map((s) => `${s.id}${s.colour}`).join(",");
  const sampleDotsBuilt = useMemo(
    () => sampleDots(packet, toggles, shownMode, focusStyle),
    [packet.samples, colourKey, packet.nodes.count, toggles, shownMode, focusStyle],
  );
  const dots = useMemo(
    () => mergeDots(nodeDots(packet, toggles, focusStyle), sampleDotsBuilt),
    [packet, toggles, focusStyle, sampleDotsBuilt],
  );
  // Only what is drawn is counted and acted on: a dot the toggles hide
  // cannot be seen to be selected.
  const acting = useMemo(
    () => drawnOnly(selection, dots.refs, packet.nodes.count + packet.samples.count),
    [selection, dots, packet],
  );

  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    current.setData({
      samples: dots.points, colors: dots.colors, shapes: dots.shapes, layout,
      surfaces: toggles.surfaces ? buildSurfaces(packet, blendColour, focusStyle, blendLine) : [],
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
    if (!fitted.current && (packet.nodes.count > 0 || packet.samples.count > 0)) {
      current.setView(presetView("iso", bounds, current.camera.fov));
      fitted.current = true;
    }
    draw();
  }, [dots, packet, layout, toggles.surfaces, bounds, unit, draw, focusStyle, blendColour, blendLine]);

  // Selection is by global index; the scene highlights by drawn index.
  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    const local = new Map<number, number>();
    dots.refs.forEach((g, d) => local.set(g, d));
    current.setSelection(selection.flatMap((g) => (local.has(g) ? [local.get(g)!] : [])));
    draw();
  }, [selection, dots, draw]);

  const camera = (preset: CameraPreset) => {
    const current = scene.current;
    if (!current) return;
    current.setView(presetView(preset, bounds, current.camera.fov));
    draw();
  };

  const chooseLayout = (next: Layout) => {
    setLayout(next);
    const current = scene.current;
    if (current) current.setView(presetView("iso", sceneBounds(packet, next), current.camera.fov));
  };

  function chooseTool(next: Tool) {
    setTool(next);
    scene.current?.setRotateEnabled(next === "rotate");
  }

  /** Sends a dragged node's speed: one call in flight, the newest value next. */
  const sendDrag = (value: number) => {
    const d = drag.current;
    if (!d) return;
    if (d.busy) { d.pending = value; return; }
    d.busy = true;
    api.editPolar(d.source, { type: "drag", bsp: value }, [d.cell], d.gesture)
      .then(onProject)
      .catch(reportFailure)
      .finally(() => {
        d.busy = false;
        if (d.pending !== null) {
          const next = d.pending;
          d.pending = null;
          if (drag.current === d) sendDrag(next);
          else {
            // The pointer is up: the last value still lands, in the same gesture.
            d.busy = true;
            api.editPolar(d.source, { type: "drag", bsp: next }, [d.cell], d.gesture)
              .then(onProject).catch(reportFailure).finally(() => { d.busy = false; });
          }
        }
      });
  };

  /** Starts dragging the focused source's node under the pointer, if there is one. */
  const startDrag = (x: number, y: number): boolean => {
    const current = scene.current;
    if (!current || focus === null || focused < 0) return false;
    const hit = current.pick(x, y, PICK_RADIUS_PX);
    if (hit < 0) return false;
    const g = dots.refs[hit]!;
    if (g >= packet.nodes.count || packet.nodes.source[g] !== focused) return false;
    const [twa, tws, bsp] = [packet.nodes.points[g * 3]!, packet.nodes.points[g * 3 + 1]!, packet.nodes.points[g * 3 + 2]!];
    const from = current.toScreen(...place(twa, tws, bsp, layout));
    const to = current.toScreen(...place(twa, tws, bsp + 1, layout));
    const axis: [number, number] = from && to ? [to[0] - from[0], to[1] - from[1]] : [0, 0];
    const cell = packet.nodes.cell[g]!;
    dragGestures += 1;
    drag.current = {
      start: bsp, axis, x, y, cell: { twa_index: cell & 0xffff, tws_index: cell >>> 16 },
      gesture: `drag-${dragGestures}`, busy: false, pending: null, source: focus,
    };
    select([g]);
    return true;
  };

  const toGlobal = (locals: ArrayLike<number>) => Array.from(locals, (d) => dots.refs[d]!);

  const onPointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    if (tool === "drag") {
      if (startDrag(x, y)) event.currentTarget.setPointerCapture?.(event.pointerId);
      return;
    }
    gesture.current = { x, y, path: [x, y] };
    if (tool !== "rotate") event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const d = drag.current;
    if (d) {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      const value = draggedSpeed(d.start, d.axis, x - d.x, y - d.y, event.shiftKey);
      setDragValue({ x, y, bsp: value });
      sendDrag(value);
      return;
    }
    const g = gesture.current;
    if (!g || tool === "rotate") return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    if (tool === "lasso") g.path.push(x, y);
    else g.path = [g.x, g.y, x, g.y, x, y, g.x, y];
    setPath([...g.path]);
  };
  const onPointerUp = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (drag.current) {
      drag.current = null;
      setDragValue(null);
      return;
    }
    const g = gesture.current;
    gesture.current = null;
    setPath(null);
    const current = scene.current;
    if (!g || !current) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    const add = event.shiftKey;
    if (Math.hypot(x - g.x, y - g.y) < CLICK_SLOP_PX) {
      const hit = current.pick(x, y, PICK_RADIUS_PX);
      if (hit >= 0) select(combine(selection, [dots.refs[hit]!], add));
      else if (!add) select([]);
      return;
    }
    if (tool === "lasso") select(combine(selection, toGlobal(current.lasso(g.path)), add));
    if (tool === "box") select(combine(selection, toGlobal(current.box(g.x, g.y, x, y)), add));
  };

  const summary = useMemo(() => summarise(packet, acting), [packet, acting]);
  const selectedCells = useMemo(
    () => new Set(editCells(packet, acting, focused).map((c) => cellCode(c.twa_index, c.tws_index))),
    [packet, acting, focused],
  );
  const selectCells = (codes: number[], add: boolean) => {
    const globals = nodesAtCells(packet, focused, new Set(codes));
    select(add ? combine(selection, globals, true) : combine([], globals, false));
  };
  const labelOf = useMemo(() => new Map(project.sources.map((s) => [s.id, s])), [project.sources]);

  const exclude = (excluded: boolean) => {
    const targets = exclusionTargets(packet, acting);
    api.setExcluded(targets.nodes, targets.samples, excluded).then(onProject).catch(reportFailure);
  };

  const factor = SPEED_FACTOR[unit];
  const symbol = SPEED_SYMBOL[unit];
  const speed = (knots: number) => `${(knots * factor).toFixed(1)} ${symbol}`;
  const legendRange = shownMode === "source" ? null
    : range(shownMode === "hs" ? packet.samples.hs : shownMode === "current" ? packet.samples.current : packet.samples.time);
  const unavailableModeTip = msg("Colouring by wave height, current or time needs track samples with their wind, which arrives with the environment fetch");
  const noFilteredTip = msg("No sample with wind is filtered out");
  const empty = packet.nodes.count === 0 && packet.samples.count === 0;

  return (
    <div className="view3d" tabIndex={-1}
      onKeyDown={(event) => { if (event.key === "Escape" && selection.length > 0) { event.stopPropagation(); select([]); } }}>
      <canvas ref={canvas} className={`view3d-canvas tool-${tool}`}
        onPointerEnter={() => setHint(t("Drag to turn, right-drag to pan, scroll to zoom. Click a dot to select it; Shift adds."))}
        onPointerLeave={() => setHint(null)}
        onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp}
        onPointerCancel={() => { gesture.current = null; drag.current = null; setPath(null); setDragValue(null); }} />
      <div className="view3d-labels" ref={labelsHost} aria-hidden="true" />
      {path && path.length >= 4 && (
        <svg className="view3d-gesture" aria-hidden="true">
          <polygon points={path.join(" ")} />
        </svg>
      )}
      {dragValue && (
        <div className="view3d-drag-value" style={{ left: dragValue.x + 12, top: dragValue.y - 12 }}>
          {t("BSP {bsp} {unit}", { bsp: (dragValue.bsp * factor).toFixed(2), unit: symbol })}
        </div>
      )}
      {unavailable !== null && <p className="view3d-unavailable muted">{t(unavailable)}</p>}
      {unavailable === null && empty && (
        <p className="view3d-empty muted">{t("The 3D view shows the project's visible polars and samples. Add an ORC polar or a polar file to see one.")}</p>
      )}

      <div className="view3d-toolbar">
        <select data-feature="view3d:layout" aria-label={t("Layout")} value={layout}
          title={t("Polar tower: angle and radius are TWA and BSP, height is TWS. Cartesian: TWA, TWS and BSP on straight axes.")}
          onChange={(event) => chooseLayout(event.target.value as Layout)}>
          {(Object.keys(LAYOUT_NAMES) as Layout[]).map((id) => <option key={id} value={id}>{t(LAYOUT_NAMES[id])}</option>)}
        </select>
        <span className="view3d-group" role="group" aria-label={t("Camera")}>
          {CAMERAS.map((c) => (
            <button key={c.id} className="small" data-feature={`view3d:camera-${c.id}`} title={t(c.tip)} onClick={() => camera(c.id)}>
              {t(c.label)}
            </button>
          ))}
        </span>
        <span className="view3d-group" role="group" aria-label={t("Tool")}>
          {TOOLS.filter((tl) => tl.id !== "drag" || focus !== null).map((tl) => (
            <button key={tl.id} className={tool === tl.id ? "small selected" : "small"} aria-pressed={tool === tl.id}
              data-feature={`view3d:tool-${tl.id}`} title={t(tl.tip)} onClick={() => chooseTool(tl.id)}>
              {t(tl.label)}
            </button>
          ))}
        </span>
      </div>

      {focus !== null && (
        <EditPanel project={project} sourceId={focus} unit={unit} selected={selectedCells} hideOthers={hideOthers}
          onHideOthers={setHideOthers} onSelectCells={selectCells} onProject={onProject}
          onDone={() => editSource(null)} />
      )}

      <div className="view3d-side">
        <fieldset className="view3d-show">
          <legend>{t("Show")}</legend>
          <label title={t("Every track sample as a dot")}>
            <input type="checkbox" data-feature="view3d:show-samples" checked={toggles.samples}
              onChange={(event) => setToggles({ ...toggles, samples: event.target.checked })} />
            {t("Samples")}
          </label>
          <label title={t("The grid points of every visible polar source")}>
            <input type="checkbox" data-feature="view3d:show-nodes" checked={toggles.nodes}
              onChange={(event) => setToggles({ ...toggles, nodes: event.target.checked })} />
            {t("Polar nodes")}
          </label>
          <label title={t("Each visible polar source as a translucent surface")}>
            <input type="checkbox" data-feature="view3d:show-surfaces" checked={toggles.surfaces}
              onChange={(event) => setToggles({ ...toggles, surfaces: event.target.checked })} />
            {t("Surfaces")}
          </label>
          <label title={filteredExist ? t("Samples the filters remove, drawn dimmed") : t(noFilteredTip)}>
            <input type="checkbox" data-feature="view3d:show-filtered" checked={toggles.filtered} disabled={!filteredExist}
              onChange={(event) => setToggles({ ...toggles, filtered: event.target.checked })} />
            {t("Filtered samples")}
          </label>
          <label className="view3d-colour">
            {t("Colour")}
            <select data-feature="view3d:colour" value={shownMode}
              title={modes.hs && modes.current && modes.time ? t("What the dots' colour shows") : t(unavailableModeTip)}
              onChange={(event) => setMode(event.target.value as ColourMode)}>
              {(Object.keys(MODE_NAMES) as ColourMode[]).map((id) => (
                <option key={id} value={id} disabled={!modes[id]}>{t(MODE_NAMES[id])}</option>
              ))}
            </select>
          </label>
          {legendRange && (
            <div className="view3d-ramp">
              <span>{legendRange[0].toFixed(1)}</span><span className="view3d-ramp-bar" /><span>{legendRange[1].toFixed(1)}</span>
            </div>
          )}
        </fieldset>

        <div className="view3d-legend" aria-label={t("Axes")}>
          {layout === "tower"
            ? t("Angle: TWA (°) · Radius: BSP ({unit}) · Height: TWS ({unit})", { unit: symbol })
            : t("Across: TWA (°) · Depth: TWS ({unit}) · Height: BSP ({unit})", { unit: symbol })}
        </div>

        <section className="view3d-selection" aria-label={t("Selection")}>
          <h3>{summary.count === 0 ? t("Nothing selected") : t("{count} selected", { count: summary.count })}</h3>
          {summary.count > 0 && (
            <>
              <div>{t("Mean TWA {twa}°, TWS {tws}, BSP {bsp}", {
                twa: summary.meanTwa.toFixed(0), tws: speed(summary.meanTws), bsp: speed(summary.meanBsp),
              })}</div>
              <ul className="view3d-breakdown">
                {summary.bySource.map((entry) => {
                  const source = labelOf.get(entry.sourceId);
                  return (
                    <li key={entry.sourceId}>
                      <span className="swatch" style={{ background: source?.colour }} />
                      {t("{label}: {count}", { label: source?.label ?? t("Unknown source"), count: entry.count })}
                    </li>
                  );
                })}
              </ul>
            </>
          )}
          <div className="view3d-actions">
            <button className="small" data-feature="view3d:exclude" disabled={summary.included === 0}
              title={t("Remove the selected dots from the blend (undoable)")} onClick={() => exclude(true)}>
              {t("Exclude")}
            </button>
            <button className="small" data-feature="view3d:include" disabled={summary.excluded === 0}
              title={t("Put the selected dots back into the blend (undoable)")} onClick={() => exclude(false)}>
              {t("Include")}
            </button>
            <button className="small" data-feature="view3d:show-on-map" disabled={summary.samples === 0}
              title={t("Show the selected samples on the map")}
              onClick={() => { selectSamples(sampleIdsOf(packet, acting), "3d"); focusMap({ kind: "selection" }); }}>
              {t("Show on map")}
            </button>
            <button className="small" data-feature="view3d:clear-selection" disabled={summary.count === 0}
              title={t("Select nothing (Escape)")} onClick={() => select([])}>
              {t("Clear")}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
