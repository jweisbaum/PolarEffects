import { useEffect, useState } from "react";

import { api, IpcError } from "../ipc";
import type { AppSettings } from "../generated/AppSettings";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { RecentProject } from "../generated/RecentProject";
import type { RecoveredProject } from "../generated/RecoveredProject";
import { openHelp } from "../help/open";
import { useT } from "../i18n";
import LanguagePicker from "../i18n/LanguagePicker";
import ConfirmDialog from "./ConfirmDialog";
import { pickProjectToOpen } from "./dialogs";
import NewProjectForm, { type NewProjectRequest } from "./NewProjectForm";

/**
 * Shown when no project is open (spec.md 3.1). Same layout and component
 * structure as VectorEffects' start screen: a centred panel with New
 * project, Open…, Recent projects and Recovered work, and the language
 * picker, Help and Settings in its header.
 *
 * No project is open here, so nothing can be lost by opening one: the save
 * guard has nothing to ask.
 */
export default function StartScreen({
  onOpened,
  onSettings,
  onPreferences,
}: {
  onOpened: (project: ProjectSummary) => void;
  onSettings: () => void;
  /** The settings after the language picker saved them. */
  onPreferences: (settings: AppSettings) => void;
}) {
  const t = useT();
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [recovered, setRecovered] = useState<RecoveredProject[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The confirmation up, if any: clearing the list, or forgetting one missing file. */
  const [confirm, setConfirm] = useState<{ kind: "clear" } | { kind: "missing"; entry: RecentProject } | null>(null);

  useEffect(() => {
    api.recentProjects().then(setRecent).catch(() => setRecent([]));
    api.recoveredProjects().then(setRecovered).catch(() => setRecovered([]));
  }, []);

  const report = (err: unknown) => setError(err instanceof IpcError ? err.message : String(err));

  const run = async (action: () => Promise<ProjectSummary>) => {
    setBusy(true);
    setError(null);
    try {
      onOpened(await action());
    } catch (err) {
      report(err);
    } finally {
      setBusy(false);
    }
  };

  const create = (request: NewProjectRequest) => void run(() => api.newProject(request.name, request.boat));
  const openFrom = (path: string) => run(() => api.openProject(path));
  const browse = async () => {
    const path = await pickProjectToOpen();
    if (path !== null) await openFrom(path);
  };

  /** The list is replaced with what the backend reports, so a refused write shows. */
  const clearRecent = async () => {
    setConfirm(null);
    setError(null);
    try {
      setRecent(await api.clearRecent());
    } catch (err) {
      report(err);
    }
  };
  const forget = async (entry: RecentProject) => {
    setConfirm(null);
    try {
      setRecent(await api.forgetRecentProject(entry.path));
    } catch (err) {
      report(err);
    }
  };

  return (
    <div className="start">
      <div className="start-panel">
        <header>
          <div className="start-header-controls">
            <LanguagePicker feature="start:language" onSettings={onPreferences} />
            <button data-feature="start:help" onClick={() => openHelp()} title={t("Open the help reference (F1)")}>
              {t("Help")}
            </button>
            <button data-feature="start:settings" onClick={onSettings} title={t("Language, theme, units, autosave, cache and network")}>
              {t("Settings")}
            </button>
          </div>
          <h1>PolarEffects</h1>
          <p className="muted">
            {t("Build a sailing polar for one boat from ORC certificates, polar files and race tracks.")}
          </p>
        </header>

        <section className="start-new" data-feature="start:new">
          <h2>{t("New project")}</h2>
          <NewProjectForm disabled={busy} submitLabel={t("Create project")} onSubmit={create} />
        </section>

        {recovered.length > 0 && (
          <section className="start-recover" data-feature="start:recover">
            <h2>{t("Recovered work")}</h2>
            <p className="muted">
              {t("PolarEffects did not close cleanly. These are snapshots of unsaved work; recovering one opens it as the project it came from, unsaved.")}
            </p>
            <ul className="recent">
              {recovered.map((entry) => (
                <li key={entry.id}>
                  <button
                    className="recent-item"
                    disabled={busy}
                    onClick={() => void run(() => api.openRecovered(entry.id))}
                    title={entry.original_path ?? t("never saved")}
                  >
                    <span className="recent-name">{entry.name}</span>
                    <span className="recent-path muted">
                      {entry.original_path ?? t("never saved")} · {new Date(entry.saved_unix_s * 1000).toLocaleString()}
                    </span>
                  </button>
                  <button
                    disabled={busy}
                    onClick={() => void api.discardRecovered(entry.id).then(setRecovered).catch(report)}
                    title={t("Delete this snapshot")}
                  >
                    {t("Discard")}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        <section className="start-open">
          <h2>{t("Open")}</h2>
          <button onClick={() => void browse()} disabled={busy} data-feature="start:browse"
            title={t("Open a saved .wpsproj project")}>
            {t("Open…")}
          </button>

          {recent.length > 0 && (
            <>
              <div className="recent-header">
                <h3 className="muted">{t("Recent projects")}</h3>
                <button
                  disabled={busy}
                  data-feature="start:clear-recent"
                  onClick={() => setConfirm({ kind: "clear" })}
                  title={t("Forget every project in this list. The projects themselves are not deleted.")}
                >
                  {t("Clear")}
                </button>
              </div>
              <ul className="recent" data-feature="start:recent">
                {recent.map((entry) => (
                  <li key={entry.path}>
                    <button
                      className={entry.exists ? "recent-item" : "recent-item missing"}
                      disabled={busy}
                      onClick={() => entry.exists ? void openFrom(entry.path) : setConfirm({ kind: "missing", entry })}
                      title={entry.exists ? entry.path : t("{path} was not found. Click to remove it from the list.", { path: entry.path })}
                    >
                      <span className="recent-name">{entry.name}</span>
                      <span className="recent-path muted">{entry.path}</span>
                      {!entry.exists && <span className="not-found">{t("Not found")}</span>}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>

        {error !== null && <p className="error" role="alert">{error}</p>}
      </div>

      {confirm?.kind === "clear" && (
        <ConfirmDialog
          title={t("Clear recent projects")}
          body={t("This forgets the list of recently opened projects. The projects themselves are not deleted.")}
          confirmLabel={t("Clear")}
          onConfirm={() => void clearRecent()}
          onCancel={() => setConfirm(null)}
        />
      )}
      {confirm?.kind === "missing" && (
        <ConfirmDialog
          title={t("Project not found")}
          body={t("{path} is no longer there. Remove it from the recent projects?", { path: confirm.entry.path })}
          confirmLabel={t("Remove")}
          onConfirm={() => void forget(confirm.entry)}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}
