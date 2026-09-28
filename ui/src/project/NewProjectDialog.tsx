import { useEffect, useState } from "react";

import { describeError } from "../errors";
import { api } from "../ipc";
import type { ProjectSummary } from "../generated/ProjectSummary";
import NewProjectForm, { type NewProjectRequest } from "./NewProjectForm";
import { useT } from "../i18n";

/**
 * Creates a project without going back to the start screen. Copied from
 * VectorEffects.
 *
 * `discardUnsaved` is the answer the user already gave to the unsaved-changes
 * prompt, carried through to the command that acts on it. Nothing is dropped
 * before this dialog's Create: cancel here and the old project is still open,
 * unsaved.
 */
export default function NewProjectDialog({
  discardUnsaved,
  onCreated,
  onCancel,
}: {
  discardUnsaved: boolean;
  onCreated: (project: ProjectSummary) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onCancel]);

  const create = (request: NewProjectRequest) => {
    setBusy(true);
    setError(null);
    api
      .newProject(request.name, request.boat, discardUnsaved)
      .then(onCreated)
      .catch((err: unknown) => setError(err ?? new Error("unknown")))
      .finally(() => setBusy(false));
  };

  return (
    <div className="modal-backdrop" onClick={busy ? undefined : onCancel}>
      <div className="modal" role="dialog" aria-label={t("New project")} onClick={(e) => e.stopPropagation()}>
        <h2>{t("New project")}</h2>
        <div className="start-new">
          <NewProjectForm disabled={busy} submitLabel={t("Create project")} onSubmit={create} />
        </div>
        {error !== null && <p className="error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}
        <div className="modal-actions">
          <button onClick={onCancel} disabled={busy} title={t("Close without creating a project")}>
            {t("Cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}
