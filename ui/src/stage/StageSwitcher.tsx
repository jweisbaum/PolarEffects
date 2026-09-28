import { msg, useT } from "../i18n";

export type Stage = "map" | "3d" | "compare";

const STAGES: ReadonlyArray<{ id: Stage; label: string; tip: string }> = [
  { id: "map", label: msg("Map"), tip: msg("The world map with the tracks") },
  { id: "3d", label: msg("3D"), tip: msg("The polar as a 3D surface, with the samples") },
  { id: "compare", label: msg("Compare"), tip: msg("The difference between two polars") },
];

/** The centre stage's switch (spec.md 3.2): Map (default), 3D or Compare. */
export default function StageSwitcher({ stage, onStage }: { stage: Stage; onStage: (stage: Stage) => void }) {
  const t = useT();
  return (
    <div className="stage-switcher" role="tablist" aria-label={t("Stage")}>
      {STAGES.map((s) => (
        <button key={s.id} role="tab" aria-selected={stage === s.id} data-feature={`stage:${s.id}`}
          className={stage === s.id ? "selected" : undefined} title={t(s.tip)} onClick={() => onStage(s.id)}>
          {t(s.label)}
        </button>
      ))}
    </div>
  );
}
