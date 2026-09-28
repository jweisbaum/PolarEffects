import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";
import type { PanelState } from "./layout";
import Section from "./Section";
import SourceList from "./SourceList";

/**
 * The right panel (spec.md 3.2): the source list (§8) above the 2D polar plot
 * (§9.2). The plot arrives in M6.
 */
export default function RightPanel({ project, onProject, panels, onToggle }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
  panels: PanelState;
  onToggle: (panel: keyof PanelState) => void;
}) {
  const t = useT();
  return (
    <div className="right-panel">
      <Section feature="panel:sources" title={t("Sources")} tooltip={t("Every source, with its colour, visibility and weight")}
        open={panels.sources} onToggle={() => onToggle("sources")}>
        <SourceList project={project} onProject={onProject} />
      </Section>
      <Section feature="panel:plot" title={t("Polar plot")} tooltip={t("Boat speed against wind angle for one wind speed")}
        open={panels.plot} onToggle={() => onToggle("plot")}>
        <div className="plot-placeholder muted">{t("The polar plot appears here once the project has a source.")}</div>
      </Section>
    </div>
  );
}
