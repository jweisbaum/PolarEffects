import { useEffect, useState } from "react";

import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";
import EditPanel from "../polar/EditPanel";

/** The normal cell tools applied to the blend's correction overlay. */
export default function BlendCorrectionDialog({ project, onProject, onClose, unit = "kn" }: {
  unit?: import("../generated/SpeedUnit").SpeedUnit;
  project: ProjectSummary;
  onProject: (summary: ProjectSummary) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());
  useEffect(() => {
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);
  return <div className="modal-backdrop" onClick={onClose}>
    <div className="modal blend-correction" role="dialog" aria-label={t("Correct blend")} onClick={(event) => event.stopPropagation()}>
      <p className="muted">{t("Corrections apply after blending. Reset a cell to restore its calculated value.")}</p>
      <EditPanel unit={unit} project={project} sourceId={0} selected={selected} hideOthers={false} onHideOthers={() => {}}
        onSelectCells={(codes, add) => setSelected((old) => new Set([...(add ? old : []), ...codes]))}
        onProject={onProject} onDone={onClose} />
    </div>
  </div>;
}
