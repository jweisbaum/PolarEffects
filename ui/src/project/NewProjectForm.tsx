import { useState } from "react";

import { useT } from "../i18n";

/** What the form hands over: the project's name. */
export interface NewProjectRequest {
  name: string;
}

/**
 * The one field a new project is created with (spec.md 4.2), its name. The
 * boat is named afterwards, on its tab. Shared by the start screen and the
 * New Project dialog rather than written twice, as in VectorEffects.
 */
export default function NewProjectForm({
  disabled = false,
  submitLabel,
  onSubmit,
}: {
  disabled?: boolean;
  submitLabel: string;
  onSubmit: (request: NewProjectRequest) => void;
}) {
  const t = useT();
  // Null until the person types: the default name then follows the
  // language, so switching it on the start screen relabels it too (M17b).
  const [typed, setName] = useState<string | null>(null);
  const name = typed ?? t("Untitled Project");
  const blank = name.trim().length === 0;

  return (
    <form
      className="new-project-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (!blank) onSubmit({ name: name.trim() });
      }}
    >
      <label data-feature="new:name" title={t("The project's name, shown in the title bar")}>
        {t("Project name")}
        <input value={name} onChange={(e) => setName(e.target.value)} spellCheck={false} />
      </label>
      <button className="primary" type="submit" disabled={disabled || blank} data-feature="new:create"
        title={t("Create the project and open it")}>
        {submitLabel}
      </button>
    </form>
  );
}
