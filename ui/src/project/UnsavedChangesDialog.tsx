import { useEffect } from "react";

import { useT } from "../i18n";
import type { UnsavedChoice } from "./saveGuard";

/**
 * Offers to save before the open project is replaced or closed.
 *
 * Three choices, not two: a plain confirm can only ask "discard?", which makes
 * saving a separate thing the user has to think of first. Cancel is the default
 * — Escape and a click outside both take it — because it is the only one of the
 * three that cannot lose anything.
 */
export default function UnsavedChangesDialog({
  name,
  onChoose,
}: {
  name: string;
  onChoose: (choice: UnsavedChoice) => void;
}) {
  const t = useT();
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onChoose("cancel");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onChoose]);

  return (
    <div className="modal-backdrop" onClick={() => onChoose("cancel")}>
      <div className="modal modal-narrow" role="dialog" aria-label={t("Unsaved changes")} onClick={(e) => e.stopPropagation()}>
        <h2>{t("Unsaved changes")}</h2>
        <p className="modal-summary">
          {t("“{name}” has changes that have not been saved.", { name })}
        </p>

        <div className="modal-actions">
          <button onClick={() => onChoose("discard")} className="danger" title={t("Lose the changes and go on")}>
            {t("Don’t save")}
          </button>
          <span className="spacer" />
          <button onClick={() => onChoose("cancel")} title={t("Keep the project open as it is")}>{t("Cancel")}</button>
          <button className="primary" autoFocus onClick={() => onChoose("save")} title={t("Save the project, then go on")}>
            {t("Save")}
          </button>
        </div>
      </div>
    </div>
  );
}
