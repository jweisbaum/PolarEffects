import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";

import { ACCEL, isAccel } from "./chords";
import type { AppInfo } from "./generated/AppInfo";
import type { AppSettings } from "./generated/AppSettings";
import type { ProjectSummary } from "./generated/ProjectSummary";
import Help from "./help/Help";
import HelpMenu from "./help/HelpMenu";
import { onReveal } from "./help/highlight";
import { describeError, reportFailure } from "./errors";
import { reportError, shown, useHint } from "./hint";
import { isBusy, useBusy } from "./busy";
import { msg, setLanguage, useT } from "./i18n";
import { api, ENV_CHANGED, ENV_PROGRESS, IpcError, QUIT_REQUESTED } from "./ipc";
import type { EnvJobsStatus } from "./generated/EnvJobsStatus";
import { currentEnvJobs, envJobsBusy, setEnvJobs, useEnvJobs } from "./jobs";
import ConfirmDialog from "./project/ConfirmDialog";
import MapView from "./map/MapView";
import LeftNav from "./panels/LeftNav";
import PolarPlot from "./panels/PolarPlot";
import PolarView from "./polar/PolarView";
import RightPanel from "./panels/RightPanel";
import { loadPanels, reveal, savePanels, togglePanel, type PanelState } from "./panels/layout";
import { pickProjectToOpen, pickProjectToSave } from "./project/dialogs";
import LoadingScreen from "./project/LoadingScreen";
import NewProjectDialog from "./project/NewProjectDialog";
import ProjectMenu from "./project/ProjectMenu";
import { quitThroughGuard } from "./project/quitGuard";
import { mayReplaceProject, mayStopJobs, type UnsavedChoice } from "./project/saveGuard";
import StartScreen from "./project/StartScreen";
import UnsavedChangesDialog from "./project/UnsavedChangesDialog";
import SettingsDialog from "./settings/SettingsDialog";
import { applyTheme } from "./settings/themes";
import Placeholder from "./stage/Placeholder";
import StageSwitcher, { type Stage } from "./stage/StageSwitcher";
import { onFocusMap, resetSelection } from "./selection";

/**
 * Application shell (spec.md 3). The help window wraps everything so F1 and
 * the native menu's Help reach it from the start screen and the project
 * window alike; the loading page sits over both.
 */
export default function App() {
  return <Help><Shell /><LoadingScreen /></Help>;
}

/** Cmd/Ctrl-N on the start screen: the new-project form is already there, so go to its name. */
function focusNewProjectName() {
  const input = document.querySelector<HTMLInputElement>('[data-feature="new:name"] input');
  input?.focus();
  input?.select();
}

/** An error on the status line: translated by its kind, the English kept for the tooltip. */
const report = reportFailure;

function Shell() {
  const t = useT();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [stage, setStage] = useState<Stage>("map");
  const [status, setStatus] = useState<string | null>(null);
  // Non-null while the New Project dialog is up; the boolean is the answer the
  // user already gave about unsaved changes, carried through to the command.
  const [creating, setCreating] = useState<{ discardUnsaved: boolean } | null>(null);
  // Set while the unsaved-changes prompt is up: holds the resolver the dialog's
  // buttons complete, which is what lets the guard read as a plain `await`.
  const [askUnsaved, setAskUnsaved] = useState<{ name: string; resolve: (choice: UnsavedChoice) => void } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  // Set while "cancel the running fetch?" is up, holding the answer's resolver.
  const [askStopJobs, setAskStopJobs] = useState<((stop: boolean) => void) | null>(null);
  /** Which panels are open: the person's, remembered in `localStorage` (spec.md 3.2). */
  const [panels, setPanels] = useState<PanelState>(loadPanels);
  /** The polar plot shown full size over the Map stage, on demand (spec.md 9.2). */
  const [plotFull, setPlotFull] = useState(false);

  useLayoutEffect(() => applyTheme(settings?.theme), [settings?.theme]);
  // The settings file is the authority on the language (spec.md 3.5).
  useEffect(() => {
    if (settings) setLanguage(settings.language);
  }, [settings?.language]);

  useEffect(() => {
    // A settings file that will not load costs the preferences, not the
    // launch: the backend already falls back to defaults.
    void api.appSettings().then(setSettings).catch(() => undefined);
    void api.appInfo().then(setInfo).catch(() => undefined);
    // A project may already be open after a reload of the frontend.
    void api.projectSummary().then((open) => { if (open) setProject(open); }).catch(() => undefined);
  }, []);

  /**
   * The environment fetch (spec.md 7.7): its queue for the status bar and
   * track list, a fresh summary whenever it writes into the project (at
   * most every 300 ms), and its failures on the status line.
   */
  useEffect(() => {
    let refresh: number | null = null;
    const onStatus = (status: EnvJobsStatus | null) => {
      // A backend that is not there yet (the frontend alone) answers nothing.
      if (!status?.tracks) return;
      const before = currentEnvJobs().failure;
      setEnvJobs(status);
      const failure = status.failure;
      if (failure && JSON.stringify(failure) !== JSON.stringify(before)) {
        reportError(t("The environment fetch of {label} stopped: {reason}", { label: failure[0] ?? "", reason: failure[1] ?? "" }), failure[1] ?? null);
      }
    };
    void api.envJobs().then(onStatus).catch(() => undefined);
    const progress = listen<EnvJobsStatus>(ENV_PROGRESS, (event) => onStatus(event.payload)).catch(() => null);
    const changed = listen(ENV_CHANGED, () => {
      if (refresh !== null) return;
      refresh = window.setTimeout(() => {
        refresh = null;
        void api.projectSummary().then((open) => { if (open) setProject(open); }).catch(() => undefined);
      }, 300);
    }).catch(() => null);
    return () => {
      if (refresh !== null) window.clearTimeout(refresh);
      void progress.then((off) => off?.());
      void changed.then((off) => off?.());
    };
  }, []);

  const toggle = useCallback((panel: keyof PanelState) => {
    setPanels((current) => {
      const next = togglePanel(current, panel);
      savePanels(next);
      return next;
    });
  }, []);

  /**
   * The search's reveal steps for what the shell hides (spec.md 3.6):
   * `panel:<name>` opens a folded panel, `section:<name>` a folded section
   * and its panel, `stage:<name>` switches the centre stage, and `settings:`
   * opens Settings (the dialog itself scrolls to the section).
   */
  useEffect(() => {
    const open = (name: keyof PanelState) => setPanels((current) => {
      const next = reveal(current, name);
      if (next !== current) savePanels(next);
      return next;
    });
    const offs = [
      onReveal("panel:left", () => open("left")),
      onReveal("panel:right", () => open("right")),
      onReveal("section:orc", () => open("orc")),
      onReveal("section:polar-files", () => open("polarFiles")),
      onReveal("section:tracks", () => open("tracks")),
      onReveal("section:sources", () => open("sources")),
      onReveal("section:plot", () => open("plot")),
      onReveal("stage:map", () => setStage("map")),
      onReveal("stage:3d", () => setStage("3d")),
      onReveal("stage:compare", () => setStage("compare")),
      onReveal("overlay:plot", () => { setStage("map"); setPlotFull(true); }),
      onReveal("settings:", () => setShowSettings(true)),
    ];
    return () => { for (const off of offs) off(); };
  }, []);

  // "Show on map" from a polar view or the track list switches to the map,
  // which frames what was asked for.
  useEffect(() => onFocusMap(() => setStage("map")), []);

  const flash = useCallback((message: string) => {
    setStatus(message);
    reportError(null);
    window.setTimeout(() => setStatus((current) => (current === message ? null : current)), 2500);
  }, []);

  /** Puts the application into a project, or back to the start screen. */
  const enter = useCallback((next: ProjectSummary | null) => {
    reportError(null);
    setRenaming(null);
    setCreating(null);
    setPlotFull(false);
    // A selection names samples of the project it was made in.
    resetSelection();
    setProject(next);
  }, []);

  // Both report whether the project actually reached disk. A cancelled
  // destination dialog is not a save, and the guard has to be able to tell.
  const saveAs = useCallback(async (): Promise<boolean> => {
    if (!project) return false;
    try {
      const path = await pickProjectToSave(project.name);
      if (path === null) return false;
      const saved = await api.saveProjectAs(path);
      setProject(saved);
      flash(t("Saved to {path}", { path: saved.path ?? path }));
      return true;
    } catch (err) {
      report(err);
      return false;
    }
  }, [project, flash]);

  const save = useCallback(async (): Promise<boolean> => {
    if (!project) return false;
    // Never saved: Save As rather than failing (spec.md 3.3).
    if (project.path === null) return saveAs();
    try {
      setProject(await api.saveProject());
      flash(t("Saved"));
      return true;
    } catch (err) {
      if (err instanceof IpcError && err.kind === "never-saved") return saveAs();
      report(err);
      return false;
    }
  }, [project, saveAs, flash]);

  const ask = useCallback((name: string) => new Promise<UnsavedChoice>((resolve) => {
    setAskUnsaved((current) => {
      // A second prompt would strand the first one's promise unresolved.
      if (current !== null) {
        resolve("cancel");
        return current;
      }
      return { name, resolve };
    });
  }), []);

  /** Asks to cancel a running fetch (spec.md 3.3); resolves once it has stopped. */
  const stopJobs = useCallback(() => mayStopJobs(
    envJobsBusy(),
    () => new Promise<boolean>((resolve) => setAskStopJobs(() => (stop: boolean) => { setAskStopJobs(null); resolve(stop); })),
    async () => {
      await api.cancelEnvFetch(null);
      // The fetch stops at its next chunk read and writes what it finished.
      for (let waited = 0; envJobsBusy() && waited < 15_000; waited += 100) {
        await new Promise((r) => setTimeout(r, 100));
      }
    },
  ), []);

  /**
   * Asks to cancel a running fetch, then about unsaved changes, and says
   * whether the open project may be replaced.
   */
  const mayReplace = useCallback(async () => {
    if (!(await stopJobs())) return { proceed: false } as const;
    // The fetch may have written into the project since it was last shown.
    const current = project ? await api.projectSummary().catch(() => project) : project;
    return mayReplaceProject(current, () => ask(project?.name ?? t("This project")), save);
  }, [project, ask, save, stopJobs]);

  const answerUnsaved = useCallback((choice: UnsavedChoice) => {
    askUnsaved?.resolve(choice);
    setAskUnsaved(null);
  }, [askUnsaved]);

  const startNewProject = useCallback(async () => {
    const decision = await mayReplace();
    if (decision.proceed) setCreating({ discardUnsaved: decision.discardUnsaved });
  }, [mayReplace]);

  const openPath = useCallback(async (path: string, discardUnsaved: boolean) => {
    enter(await api.openProject(path, discardUnsaved));
  }, [enter]);

  const openProject = useCallback(async () => {
    // Asked before the file dialog: nothing is discarded by asking, since the
    // answer travels with the open call, so cancelling the dialog costs nothing.
    const decision = await mayReplace();
    if (!decision.proceed) return;
    try {
      const path = await pickProjectToOpen();
      if (path !== null) await openPath(path, decision.discardUnsaved);
    } catch (err) {
      report(err);
    }
  }, [mayReplace, openPath]);

  const openRecent = useCallback(async (path: string) => {
    const decision = await mayReplace();
    if (!decision.proceed) return;
    try {
      await openPath(path, decision.discardUnsaved);
    } catch (err) {
      report(err);
    }
  }, [mayReplace, openPath]);

  const closeProject = useCallback(async () => {
    const decision = await mayReplace();
    if (!decision.proceed) return;
    try {
      await api.closeProject(decision.discardUnsaved);
      enter(null);
    } catch (err) {
      report(err);
    }
  }, [mayReplace, enter]);

  /**
   * Quit and the window's close button (spec.md 3.3). Rust asks only when
   * there is something to lose; the latest guard is read through a ref, since
   * the listener is registered once.
   */
  const quitDeps = useRef({ ask, save, project });
  quitDeps.current = { ask, save, project };
  useEffect(() => {
    let quitting = false;
    const pending = listen(QUIT_REQUESTED, () => {
      if (quitting) return;
      quitting = true;
      void quitThroughGuard({
        current: () => api.projectSummary(),
        ask: () => quitDeps.current.ask(quitDeps.current.project?.name ?? t("This project")),
        save: () => quitDeps.current.save(),
        quit: (discardUnsaved) => api.quitApp(discardUnsaved),
      }).catch(report).finally(() => { quitting = false; });
    }).catch(() => null);
    return () => { void pending.then((off) => off?.()); };
  }, []);

  // Escape closes the full-size polar plot overlay, like every other overlay.
  useEffect(() => {
    if (!plotFull) return;
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") setPlotFull(false); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [plotFull]);

  // Standard shortcuts (spec.md 3.2).
  const modal = creating !== null || askUnsaved !== null || askStopJobs !== null || showSettings;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // A dialog is a decision in progress; saving or undoing behind it would
      // change the very thing being decided about.
      if (modal || !isAccel(event)) return;
      const key = event.key.toLowerCase();
      const target = event.target as HTMLElement | null;
      const typing = target !== null && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName);
      if (key === ",") {
        event.preventDefault();
        setShowSettings(true);
      } else if (key === "n") {
        event.preventDefault();
        // The start screen has the form inline; the project window asks
        // about unsaved changes, then opens the dialog.
        if (project) void startNewProject();
        else focusNewProjectName();
      } else if (key === "o") {
        event.preventDefault();
        void openProject();
      } else if (!project) {
        return;
      } else if (key === "s") {
        event.preventDefault();
        void (event.shiftKey ? saveAs() : save());
      } else if (key === "w") {
        event.preventDefault();
        void closeProject();
      } else if (key === "z" && !typing) {
        // In a text field, Cmd-Z undoes the typing, not the project.
        event.preventDefault();
        void (event.shiftKey ? api.redo() : api.undo()).then((next) => {
          const label = event.shiftKey ? project.redo_label : project.undo_label;
          setProject(next);
          if (label) flash(event.shiftKey ? t("Redone: {action}", { action: t(label) }) : t("Undone: {action}", { action: t(label) }));
        }).catch(report);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [modal, project, startNewProject, openProject, save, saveAs, closeProject, flash]);

  const settingsDialog = showSettings && settings !== null && (
    <SettingsDialog settings={settings} onSettings={setSettings} onClose={() => setShowSettings(false)} />
  );
  const unsavedDialog = <>
    {askUnsaved !== null && <UnsavedChangesDialog name={askUnsaved.name} onChoose={answerUnsaved} />}
    {askStopJobs !== null && (
      <ConfirmDialog title={t("Cancel the environment fetch?")}
        body={t("A wind, wave and current fetch is running for this project. It stops first; the samples it already fetched stay in the project, and Refetch environment resumes it later.")}
        confirmLabel={t("Cancel the fetch")} onConfirm={() => askStopJobs(true)} onCancel={() => askStopJobs(false)} />
    )}
  </>;

  if (!project) {
    return <>
      <StartScreen onOpened={enter} onSettings={() => setShowSettings(true)} onPreferences={setSettings} />
      {settingsDialog}
      {unsavedDialog}
    </>;
  }

  return (
    <div className="app">
      <div className="titlebar">
        <ProjectMenu
          onNew={() => void startNewProject()}
          onOpen={() => void openProject()}
          onOpenRecent={(path) => void openRecent(path)}
          onSave={() => void save()}
          onSaveAs={() => void saveAs()}
          onClose={() => void closeProject()}
        />
        {/* The name is edited where it is shown: click it, type, Enter. It undoes. */}
        {renaming !== null ? (
          <input
            className="project-name"
            autoFocus
            value={renaming}
            aria-label={t("Project name")}
            title={t("Type a new name; Enter to keep it, Esc to cancel")}
            data-feature="shell:rename"
            onChange={(event) => setRenaming(event.target.value)}
            onBlur={() => {
              const name = renaming.trim();
              setRenaming(null);
              if (name.length > 0 && name !== project.name) void api.renameProject(name).then(setProject).catch(report);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
              if (event.key === "Escape") setRenaming(null);
            }}
          />
        ) : (
          <button className="project-name" onClick={() => setRenaming(project.name)}
            title={t("Click to rename the project")} data-feature="shell:rename">
            {project.name}
            {project.dirty && <span className="dirty" aria-label={t("Unsaved changes")}> •</span>}
          </button>
        )}
        <span className="spacer" />
        <StageSwitcher stage={stage} onStage={(next) => { setStage(next); if (next !== "map") setPlotFull(false); }} />
        <span className="spacer" />
        <HelpMenu />
        <button className="settings" onClick={() => setShowSettings(true)} data-feature="shell:settings"
          title={t("Settings ({chord})", { chord: `${ACCEL}+,` })} aria-label={t("Settings")}>
          ⚙
        </button>
      </div>

      <div className="workspace" style={{
        "--dock-left": panels.left ? "var(--sidebar-left)" : "0px",
        "--dock-right": panels.right ? "var(--sidebar-right)" : "0px",
      } as CSSProperties}>
        {panels.left && <aside className="sidebar left"><LeftNav project={project} onProject={setProject} panels={panels} onToggle={toggle} /></aside>}
        <main className="centre-stage" aria-label={t("Stage")}>
          {stage === "map" && <MapView project={project} settings={settings} onSettings={setSettings} />}
          {stage === "3d" && <PolarView project={project} settings={settings} onProject={setProject} />}
          {stage === "compare" && <Placeholder title={msg("Compare")}
            body={msg("Comparing two polars arrives in a later version.")} />}
          {stage === "map" && plotFull && (
            <div className="polar-plot-overlay" role="dialog" aria-label={t("Polar plot")}>
              <PolarPlot project={project} variant="overlay" onClose={() => setPlotFull(false)} />
            </div>
          )}
        </main>
        {panels.right && (
          <aside className="sidebar right">
            <RightPanel project={project} onProject={setProject} panels={panels} onToggle={toggle}
              onFullSizePlot={() => { setStage("map"); setPlotFull(true); }} />
          </aside>
        )}
        <DockToggle side="left" open={panels.left}
          labels={[msg("Show the navigation"), msg("Hide the navigation")]} onToggle={() => toggle("left")} />
        <DockToggle side="right" open={panels.right}
          labels={[msg("Show the sources and polar plot"), msg("Hide the sources and polar plot")]} onToggle={() => toggle("right")} />
      </div>

      <div className="statusbar" data-feature="shell:statusbar" title={t("Hints, errors and work in progress")}>
        <BusySpinner />
        <EnvJobsIndicator />
        <StatusHint status={status} />
        <span className="spacer" />
        {project.path !== null && <span className="muted path" title={project.path}>{project.path}</span>}
        {info && <span className="muted">v{info.version}</span>}
      </div>

      {settingsDialog}
      {creating !== null && (
        <NewProjectDialog discardUnsaved={creating.discardUnsaved}
          onCreated={(created) => { setCreating(null); enter(created); }}
          onCancel={() => setCreating(null)} />
      )}
      {unsavedDialog}
    </div>
  );
}

const DOCK_GLYPH = {
  left: { open: "◀", closed: "▶" },
  right: { open: "▶", closed: "◀" },
} as const;

/**
 * A panel's toggle: a tab on the border between the panel and the stage,
 * which stays put whether the panel is open or closed. Copied from
 * VectorEffects.
 */
function DockToggle({ side, open, labels, onToggle }: {
  side: "left" | "right";
  open: boolean;
  /** What the tab says while the panel is closed, and while it is open. English, via `msg`. */
  labels: [show: string, hide: string];
  onToggle: () => void;
}) {
  const t = useT();
  const label = t(open ? labels[1] : labels[0]);
  return (
    <button className={`dock-toggle ${side}`} data-feature={`dock:${side}`} onClick={onToggle}
      title={label} aria-label={label} aria-expanded={open}>
      {DOCK_GLYPH[side][open ? "open" : "closed"]}
    </button>
  );
}

/** The status bar's spinner: turning while a long command runs. */
function BusySpinner() {
  const t = useT();
  const busy = useBusy();
  const on = isBusy(busy);
  return (
    <span className={on ? "busy-spinner on" : "busy-spinner"} role="status" aria-live="polite"
      aria-label={on ? busy.labels.map((label) => t(label)).join(", ") : t("Idle")}
      title={on ? busy.labels.map((label) => t(label)).join(" · ") : undefined} />
  );
}

/**
 * The status bar's line for the environment fetch (spec.md 7.7): the track
 * being fetched and how far it is, how many wait, and Cancel. Nothing
 * while no fetch runs.
 */
function EnvJobsIndicator() {
  const t = useT();
  const jobs = useEnvJobs();
  const running = jobs.tracks.find((job) => job.state === "fetching") ?? jobs.tracks[0];
  if (running === undefined) return null;
  const waiting = jobs.tracks.length - 1;
  return (
    <span className="env-jobs" role="status" aria-live="polite">
      <span className="env-jobs-bar" aria-hidden="true"><span style={{ width: `${Math.round(running.fraction * 100)}%` }} /></span>
      {t("Fetching wind, waves and current: {label} {percent} %", { label: running.label, percent: Math.floor(running.fraction * 100) })}
      {waiting > 0 && <span className="muted">{" "}{t("({count} more waiting)", { count: waiting })}</span>}
      <button className="small" data-feature="shell:cancel-fetch" title={t("Stop every fetch; samples already fetched are kept")}
        onClick={() => { api.cancelEnvFetch(null).catch(reportFailure); }}>
        {t("Cancel fetch")}
      </button>
    </span>
  );
}

/** The status bar's middle: a flash, the error, or the hint. */
function StatusHint({ status }: { status: string | null }) {
  useT();
  const state = useHint();
  const described = state.failure !== undefined && state.error !== null ? describeError(state.failure) : null;
  const line = described ? { text: described.text, kind: "error" as const, detail: described.detail } : shown(state);
  if (status !== null) return <span className="hint accent">{status}</span>;
  if (line === null) return <span className="hint" />;
  return <span className={line.kind === "error" ? "hint error" : "hint muted"} role={line.kind === "error" ? "alert" : undefined}
    title={line.detail ?? undefined}>{line.text}</span>;
}
