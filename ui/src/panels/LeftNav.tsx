import { useT } from "../i18n";
import type { PanelState } from "./layout";
import Section from "./Section";

/**
 * The left navigation (spec.md 3.2): ORC polars (§5), Polar files (§6) and
 * Tracks (§7), in that order, each foldable. Their contents arrive with the
 * milestones that build them (M5, M4, M8–M12); for now each says what it is
 * for.
 */
export default function LeftNav({ panels, onToggle }: {
  panels: PanelState;
  onToggle: (panel: keyof PanelState) => void;
}) {
  const t = useT();
  return (
    <nav className="left-nav" aria-label={t("Sources to add")}>
      <Section feature="nav:orc" title={t("ORC polars")} tooltip={t("Search the ORC catalogue and add certificates")}
        open={panels.orc} onToggle={() => onToggle("orc")}>
        <p className="muted placeholder">{t("No ORC polars in this project yet.")}</p>
      </Section>
      <Section feature="nav:polar-files" title={t("Polar files")} tooltip={t("Import Expedition and Adrena polars")}
        open={panels.polarFiles} onToggle={() => onToggle("polarFiles")}>
        <p className="muted placeholder">{t("No polar files in this project yet.")}</p>
      </Section>
      <Section feature="nav:tracks" title={t("Tracks")} tooltip={t("Import race tracks from trackers and files")}
        open={panels.tracks} onToggle={() => onToggle("tracks")}>
        <p className="muted placeholder">{t("No tracks in this project yet.")}</p>
      </Section>
    </nav>
  );
}
