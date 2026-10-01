import { useCallback, useEffect, useState, type CSSProperties } from "react";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { AppSettings } from "../generated/AppSettings";
import { msg, useT } from "../i18n";
import { onReveal } from "../help/highlight";
import { onShowStage } from "../mcp/follow";
import MapView from "../map/MapView";
import LeftNav from "../panels/LeftNav";
import RightPanel from "../panels/RightPanel";
import PolarPlot from "../panels/PolarPlot";
import CompareView from "../compare/CompareView";
import { onCompareSource } from "../compare/compareState";
import PolarView from "../polar/PolarView";
import PolarModeToggle from "../polar/PolarModeToggle";
import { useBoatEditing } from "../polar/editFocus";
import { useBoatSelection } from "../selection";
import { loadPanels, reveal, savePanels, togglePanel, type PanelState } from "../panels/layout";
import StageSwitcher, { type Stage } from "../stage/StageSwitcher";

export default function BoatWorkspace({ project, settings, onSettings: setSettings, onProject: updateProject, split, active, visible }: {
  project: ProjectSummary; settings: AppSettings | null; onSettings: (settings: AppSettings) => void;
  onProject: (project: ProjectSummary) => void; split: boolean; active: boolean; visible: boolean;
}) {
  const t = useT();
  const [requestedStage, setStage] = useState<Stage>("3d");
  const hasTracks = project.sources.some(source => source.kind === "track");
  const stage = split || (requestedStage === "map" && !hasTracks) ? "3d" : requestedStage;
  const [singlePanels, setSinglePanels] = useState<PanelState>(loadPanels);
  const [comparisonPanels, setComparisonPanels] = useState<PanelState>(() => ({ ...loadPanels(), left: false, right: false }));
  const panels = split ? comparisonPanels : singlePanels;
  const setPanels = split ? setComparisonPanels : setSinglePanels;
  const [comparisonControls, setComparisonControls] = useState(false);
  const [plotFull, setPlotFull] = useState(false);
  const { onFocusMap } = useBoatSelection();
  const { editFocus, editSource, onEditSource } = useBoatEditing();
  useEffect(() => { if (split) setPlotFull(false); }, [split]);
  const toggle = useCallback((panel: keyof PanelState) => setPanels(current => {
    const next = togglePanel(current, panel); if (!split) savePanels(next); return next;
  }), [split, setPanels]);
  useEffect(() => onFocusMap(() => { if (!split) setStage("map"); setPlotFull(false); }), [onFocusMap, split]);
  useEffect(() => onEditSource(() => { setStage("3d"); setPlotFull(false); }), [onEditSource]);
  useEffect(() => onCompareSource(id => { if (id === project.id && !split) { setStage("compare"); setPlotFull(false); } }), [project.id, split]);
  // The MCP service's `view://stage`: this boat's stage, whether or not this
  // boat is the one on show (its tab is shown next, in the single layout).
  useEffect(() => onShowStage(project.id, next => {
    if (next === "plot") { setPlotFull(true); return; }
    setStage(next); setPlotFull(false);
  }), [project.id]);
  useEffect(() => {
    if (!active) return;
    const open = (name: keyof PanelState) => setPanels(current => reveal(current, name));
    const offs = [
      onReveal("panel:left", () => open("left")), onReveal("panel:right", () => open("right")),
      onReveal("section:orc", () => open("orc")), onReveal("section:polar-files", () => open("polarFiles")),
      onReveal("section:tracks", () => open("tracks")), onReveal("section:sources", () => open("sources")), onReveal("section:plot", () => open("plot")),
      onReveal("stage:map", () => { if (!split) setStage("map"); setPlotFull(false); }),
      onReveal("stage:3d", () => { setComparisonControls(true); setStage("3d"); setPlotFull(false); }),
      onReveal("stage:compare", () => { if (!split) setStage("compare"); setPlotFull(false); }),
      onReveal("overlay:plot", () => { if (!split) setPlotFull(true); }),
      onReveal("edit:open", () => { setStage("3d"); if (editFocus() === null && project.sources[0]) editSource(project.sources[0].id); }),
      onReveal("edit:open-track", () => { setStage("3d"); const source = project.sources.find(s => s.kind === "track"); if (source) editSource(source.id); }),
    ];
    return () => { for (const off of offs) off(); };
  }, [active, split, project.sources, editFocus, editSource]);
  useEffect(() => {
    if (!active || !plotFull) return;
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setPlotFull(false); };
    window.addEventListener("keydown", escape); return () => window.removeEventListener("keydown", escape);
  }, [active, plotFull]);
  return <>
    <div className="boat-view-toolbar">
      {split ? <span>{t("3D")}</span> : <StageSwitcher stage={stage} hasTracks={hasTracks} onStage={next => { setStage(next); setPlotFull(false); }} />}
      <PolarModeToggle project={project} onProject={updateProject} />
      {split && <button data-feature="boats:comparison-controls" aria-pressed={comparisonControls} onClick={() => setComparisonControls(value => !value)}>{t("Filters and display")}</button>}
    </div>
      <div className="workspace" style={{
        "--dock-left": panels.left ? "var(--sidebar-left)" : "0px",
        "--dock-right": panels.right ? "var(--sidebar-right)" : "0px",
      } as CSSProperties}>
        <main className="centre-stage" aria-label={t("Stage")}>
          {visible && stage === "map" && <MapView project={project} settings={settings} onSettings={setSettings} />}
          {visible && stage === "3d" && <PolarView project={project} settings={settings} onProject={updateProject} compact={split && !comparisonControls} />}
          {/* Keyed by project: nothing of one project's comparison (its answer, its framing, a hovered cell) shows under another's names. */}
          {visible && stage === "compare" && <CompareView key={project.id} project={project} settings={settings} />}
        </main>
        {panels.left && <aside className="sidebar left"><LeftNav project={project} onProject={updateProject} panels={panels} onToggle={toggle}
          {...(settings ? { units: settings.units } : {})} /></aside>}
        {panels.right && (
          <aside className="sidebar right">
            <RightPanel project={project} onProject={updateProject} panels={panels} onToggle={toggle}
              speedUnit={settings?.units.speed ?? "kn"} onFullSizePlot={() => setPlotFull(true)} />
          </aside>
        )}
        <DockToggle side="left" open={panels.left}
          labels={[msg("Show the navigation"), msg("Hide the navigation")]} onToggle={() => toggle("left")} />
        <DockToggle side="right" open={panels.right}
          labels={[msg("Show the sources and polar plot"), msg("Hide the sources and polar plot")]} onToggle={() => toggle("right")} />
        {visible && !split && plotFull && (
          <div className="polar-plot-overlay" role="dialog" aria-label={t("Polar plot")}>
            <PolarPlot project={project} variant="overlay" unit={settings?.units.speed ?? "kn"}
              onClose={() => setPlotFull(false)} />
          </div>
        )}
      </div>

  </>;
}

const DOCK_GLYPH = {
  left: { open: "◀", closed: "▶" },
  right: { open: "▶", closed: "◀" },
} as const;

/**
 * A panel's toggle: a tab on the border between the panel and the stage,
 * which stays put whether the panel is open or closed. Copied from
 * VectorEffects.
 */
function DockToggle({ side, open, labels, onToggle }: {
  side: "left" | "right";
  open: boolean;
  /** What the tab says while the panel is closed, and while it is open. English, via `msg`. */
  labels: [show: string, hide: string];
  onToggle: () => void;
}) {
  const t = useT();
  const label = t(open ? labels[1] : labels[0]);
  return (
    <button className={`dock-toggle ${side}`} data-feature={`dock:${side}`} onClick={onToggle}
      title={label} aria-label={label} aria-expanded={open}>
      {DOCK_GLYPH[side][open ? "open" : "closed"]}
    </button>
  );
}

