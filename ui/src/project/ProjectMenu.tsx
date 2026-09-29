import { useEffect, useId, useRef, useState } from "react";

import { chordText } from "../chords";
import type { RecentProject } from "../generated/RecentProject";
import { onReveal } from "../help/highlight";
import { msg, useT } from "../i18n";
import { api } from "../ipc";

type Action = () => void;

/**
 * The Project menu (spec.md 3.2): New…, Open…, Open Recent ▸, Save, Save
 * As…, Close. Copied from VectorEffects, with Open Recent added. Every item
 * goes through App's handlers, which hold the save guard.
 */
export default function ProjectMenu({ onNew, onOpen, onOpenRecent, onSave, onSaveAs, onClose }: {
  onNew: Action; onOpen: Action; onOpenRecent: (path: string) => void; onSave: Action; onSaveAs: Action; onClose: Action;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [recentOpen, setRecentOpen] = useState(false);
  const [recent, setRecent] = useState<RecentProject[] | null>(null);
  const id = useId();
  const container = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const firstFocus = useRef(0);
  const actions = [
    { label: msg("New…"), feature: "new", run: onNew, chord: chordText(["accel", "N"]), tip: msg("Create a new project") },
    { label: msg("Open…"), feature: "open", run: onOpen, chord: chordText(["accel", "O"]), tip: msg("Open a saved project") },
    { label: msg("Save"), feature: "save", run: onSave, chord: chordText(["accel", "S"]), tip: msg("Save the project to its file") },
    { label: msg("Save As…"), feature: "save-as", run: onSaveAs, chord: chordText(["accel", "shift", "S"]), tip: msg("Save the project to a new file") },
    { label: msg("Close"), feature: "close", run: onClose, chord: chordText(["accel", "W"]), tip: msg("Close the project and return to the start screen") },
  ];

  // The Help search opens the menu to show one of its items (spec.md 3.6).
  useEffect(() => onReveal("menu:project", () => setOpen(true)), []);

  useEffect(() => {
    if (!open) {
      setRecentOpen(false);
      return;
    }
    api.recentProjects().then(setRecent).catch(() => setRecent([]));
    menu.current?.querySelectorAll<HTMLButtonElement>("button")[firstFocus.current]?.focus();
    const outside = (event: Event) => {
      if (event.target instanceof Node && !container.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("focusin", outside);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("focusin", outside);
    };
  }, [open]);

  const close = () => { setOpen(false); trigger.current?.focus(); };
  const item = (action: (typeof actions)[number]) => (
    <button key={action.feature} role="menuitem" tabIndex={-1} data-feature={`project:${action.feature}`}
      title={t("{action} ({chord})", { action: t(action.tip), chord: action.chord })}
      onClick={() => { close(); action.run(); }}>
      <span>{t(action.label)}</span><span className="menu-chord" aria-hidden="true">{action.chord}</span>
    </button>
  );
  const existing = (recent ?? []).filter((entry) => entry.exists);

  return <div className="project-menu" ref={container}>
    <button ref={trigger} data-feature="shell:project-menu" aria-haspopup="menu" aria-expanded={open}
      aria-controls={open ? id : undefined} title={t("New, open, save and close projects")}
      onClick={() => { firstFocus.current = 0; setOpen(!open); }}
      onKeyDown={event => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault(); event.stopPropagation();
          firstFocus.current = event.key === "ArrowUp" ? actions.length : 0;
          setOpen(true);
        }
      }}>
      {t("Project")} <span aria-hidden="true">▾</span>
    </button>
    {open && <div ref={menu} id={id} className="project-menu-items" role="menu" aria-label={t("Project")}
      onKeyDown={event => {
        // The project shortcuts still reach App's window handler.
        if (event.metaKey || event.ctrlKey) { setOpen(false); return; }
        event.stopPropagation();
        const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button")];
        const current = items.indexOf(document.activeElement as HTMLButtonElement);
        let next: number | null = null;
        if (event.key === "ArrowDown") next = (current + 1) % items.length;
        if (event.key === "ArrowUp") next = (current + items.length - 1) % items.length;
        if (event.key === "Home") next = 0;
        if (event.key === "End") next = items.length - 1;
        if (next !== null) { event.preventDefault(); items[next]?.focus(); }
        else if (event.key === "Escape") { event.preventDefault(); close(); }
        else if (event.key === "Tab") close();
      }}>
      {item(actions[0]!)}
      {item(actions[1]!)}
      <button role="menuitem" tabIndex={-1} data-feature="project:open-recent" aria-haspopup="menu"
        aria-expanded={recentOpen} title={t("Open one of the ten most recent projects")}
        onClick={() => setRecentOpen(!recentOpen)}>
        <span>{t("Open Recent")}</span><span className="menu-chord" aria-hidden="true">{recentOpen ? "▾" : "▸"}</span>
      </button>
      {recentOpen && <div className="project-menu-recent" role="group" aria-label={t("Open Recent")}>
        {existing.length === 0
          ? <p className="muted">{t("No recent projects")}</p>
          : existing.map((entry) => (
            <button key={entry.path} role="menuitem" tabIndex={-1} title={entry.path}
              onClick={() => { close(); onOpenRecent(entry.path); }}>
              {entry.name}
            </button>
          ))}
      </div>}
      {item(actions[2]!)}
      {item(actions[3]!)}
      {item(actions[4]!)}
    </div>}
  </div>;
}
