import type { Units } from "../generated/Units";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";
import type { PanelState } from "./layout";
import OrcPolars from "./OrcPolars";
import PolarFiles from "./PolarFiles";
import Section from "./Section";
import Tracks from "./Tracks";

/**
 * The left navigation (spec.md 3.2): ORC polars (§5), Polar files (§6) and
 * Tracks (§7), in that order, each foldable. Tracks import from files now;
 * the tracker imports arrive with M10–M12.
 */
export default function LeftNav({ project, onProject, panels, onToggle, units }: {
  project: ProjectSummary;
  /** The display units (Settings), for the track filters. */
  units?: Units;
  onProject: (project: ProjectSummary) => void;
  panels: PanelState;
  onToggle: (panel: keyof PanelState) => void;
}) {
  const t = useT();
  return (
    <nav className="left-nav" aria-label={t("Sources to add")}>
      <Section feature="nav:orc" title={t("ORC / ORR polars")} tooltip={t("Search polar catalogues and add certificates")}
        open={panels.orc} onToggle={() => onToggle("orc")}>
        <OrcPolars project={project} onProject={onProject} />
      </Section>
      <Section feature="nav:polar-files" title={t("Polar files")} tooltip={t("Import Expedition and Adrena polars")}
        open={panels.polarFiles} onToggle={() => onToggle("polarFiles")}>
        <PolarFiles project={project} onProject={onProject} />
      </Section>
      <Section feature="nav:tracks" title={t("Tracks")} tooltip={t("Import race tracks from trackers and files")}
        open={panels.tracks} onToggle={() => onToggle("tracks")}>
        <Tracks project={project} onProject={onProject} {...(units ? { units } : {})} />
      </Section>
    </nav>
  );
}
