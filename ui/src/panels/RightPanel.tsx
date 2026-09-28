import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";
import type { PanelState } from "./layout";
import Section from "./Section";

/**
 * The right panel (spec.md 3.2): the source list (§8) above the 2D polar plot
 * (§9.2). The list's controls and the plot arrive in M6 and later; the IPC
 * for every source edit already exists (`api.setSource…`).
 */
export default function RightPanel({ project, panels, onToggle }: {
  project: ProjectSummary;
  panels: PanelState;
  onToggle: (panel: keyof PanelState) => void;
}) {
  const t = useT();
  return (
    <div className="right-panel">
      <Section feature="panel:sources" title={t("Sources")} tooltip={t("Every source, with its colour, visibility and weight")}
        open={panels.sources} onToggle={() => onToggle("sources")}>
        {project.sources.length === 0
          ? <p className="muted placeholder">{t("No sources yet.")}</p>
          : <ul className="source-list">
            {project.sources.map((source) => (
              <li key={source.id} className={source.visible ? undefined : "hidden-source"}>
                <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
                <span className="source-label">{source.label}</span>
              </li>
            ))}
          </ul>}
      </Section>
      <Section feature="panel:plot" title={t("Polar plot")} tooltip={t("Boat speed against wind angle for one wind speed")}
        open={panels.plot} onToggle={() => onToggle("plot")}>
        <div className="plot-placeholder muted">{t("The polar plot appears here once the project has a source.")}</div>
      </Section>
    </div>
  );
}
