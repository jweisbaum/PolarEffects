import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import { reportFailure } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import { onReveal } from "../help/highlight";
import { msg, useT } from "../i18n";
import { api } from "../ipc";
import { editSource } from "../polar/editFocus";
import BlendSettingsDialog from "./BlendSettingsDialog";
import ExportDialog from "./ExportDialog";
import PALETTE from "./palette.json";

/** A glyph and a name per kind of source. */
const KINDS: Readonly<Record<string, { glyph: string; name: string }>> = {
  orc: { glyph: "◆", name: msg("ORC polar") },
  polar_file: { glyph: "▦", name: msg("Polar file") },
  track: { glyph: "〰", name: msg("Track") },
};

let gestures = 0;

/**
 * The source list (spec.md 8): the Blend entry at the top — its colour, a
 * show or hide switch, its coverage (cells with direct evidence and cells
 * filled), Blend settings and Export — then every source
 * with its colour (a palette of sixteen plus a custom picker), a show or hide
 * switch, its label (renamed in place), kind, count and blend weight, and
 * Edit, Compare and Remove. Rows reorder by dragging the handle, or with the
 * arrow keys on it. Every change is one undoable command in Rust; the list
 * shows whatever summary comes back.
 */
export default function SourceList({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const t = useT();
  const [picking, setPicking] = useState<number | null>(null);
  const [pickingBlend, setPickingBlend] = useState(false);
  const [dialog, setDialog] = useState<"settings" | "export" | null>(null);
  const [renaming, setRenaming] = useState<{ id: number; text: string } | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [weights, setWeights] = useState<Record<number, number>>({});
  const gesture = useRef<string | null>(null);
  const sources = project.sources;
  const blend = project.blend;
  const first = useRef<number | null>(null);
  first.current = sources[0]?.id ?? null;

  // The search's reveal step for the colour picker (spec.md 3.6): open it on
  // the first source.
  useEffect(() => onReveal("sources:colours", () => setPicking(first.current)), []);

  const run = (change: Promise<ProjectSummary>) => change.then(onProject).catch(reportFailure);

  const commitRename = () => {
    if (!renaming) return;
    const source = sources.find((s) => s.id === renaming.id);
    const text = renaming.text.trim();
    setRenaming(null);
    if (source && text.length > 0 && text !== source.label) void run(api.setSourceLabel(source.id, text));
  };

  const setWeight = (source: SourceSummary, weight: number) => {
    setWeights((current) => ({ ...current, [source.id]: weight }));
    void run(api.setSourceWeight(source.id, weight, gesture.current));
  };
  const endWeight = (source: SourceSummary) => {
    gesture.current = null;
    setWeights((current) => {
      const next = { ...current };
      delete next[source.id];
      return next;
    });
  };

  const move = (source: SourceSummary, to: number) => {
    if (to < 0 || to >= sources.length) return;
    void run(api.moveSource(source.id, to));
  };
  const onHandleKey = (event: KeyboardEvent, source: SourceSummary, index: number) => {
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      move(source, index + (event.key === "ArrowUp" ? -1 : 1));
    }
  };

  return (
    <ul className="source-list">
      {/* The blend (spec.md 8, 12). */}
      <li className={["source-row", "blend-row", blend.visible ? "" : "hidden-source"].join(" ").trim()}>
        <div className="source-line">
          <button className="swatch-button" data-feature="sources:blend-colour"
            style={{ backgroundColor: blend.colour }} aria-expanded={pickingBlend}
            title={t("Change the blend's colour")} aria-label={t("Blend colour")}
            onClick={() => setPickingBlend(!pickingBlend)} />
          <input type="checkbox" checked={blend.visible} data-feature="sources:blend-visible"
            aria-label={t("Show the blend")}
            title={t("Show or hide the blend in every plot. Export is not affected.")}
            onChange={(event) => void run(api.setBlendVisible(event.target.checked))} />
          <span className="source-label blend-label">{t("Blend")}</span>
          <span className="muted source-count"
            title={t("{direct} cells have direct evidence from a source, {filled} are filled between them (and the 0° row), {empty} stay empty.", { direct: blend.direct, filled: blend.filled, empty: blend.empty })}>
            {t("{direct} direct, {filled} filled", { direct: blend.direct, filled: blend.filled })}
          </span>
        </div>
        <div className="source-line">
          <button className="small" data-feature="sources:blend-settings"
            title={t("The output grid and how sources are blended")}
            onClick={() => setDialog("settings")}>
            {t("Blend settings")}
          </button>
          <button className="small" data-feature="sources:export"
            title={t("Write the blend as an Expedition, Adrena or CSV polar")}
            onClick={() => setDialog("export")}>
            {t("Export…")}
          </button>
        </div>
        {pickingBlend && (
          <ColourPicker colour={blend.colour} onClose={() => setPickingBlend(false)}
            onChoose={(colour) => {
              if (colour !== blend.colour) void run(api.setBlendColour(colour));
            }} />
        )}
        {dialog === "settings" && (
          <BlendSettingsDialog project={project} onProject={onProject} onClose={() => setDialog(null)} />
        )}
        {dialog === "export" && <ExportDialog project={project} onClose={() => setDialog(null)} />}
      </li>
      {sources.length === 0 && <li className="muted placeholder">{t("No sources yet.")}</li>}
      {sources.map((source, index) => {
        const kind = KINDS[source.kind];
        const weight = weights[source.id] ?? source.weight;
        return (
          <li key={source.id}
            className={["source-row", source.visible ? "" : "hidden-source", dragging === source.id ? "dragging" : ""].join(" ").trim()}
            onDragOver={(event) => { if (dragging !== null) event.preventDefault(); }}
            onDrop={(event) => {
              event.preventDefault();
              const moved = sources.find((s) => s.id === dragging);
              setDragging(null);
              if (moved && moved.id !== source.id) move(moved, index);
            }}>
            <div className="source-line">
              <span className="drag-handle" draggable tabIndex={0} data-feature="sources:reorder"
                title={t("Drag to reorder the list, or press the up and down arrow keys")}
                aria-label={t("Reorder")}
                onDragStart={(event) => {
                  event.dataTransfer?.setData("text/plain", String(source.id));
                  setDragging(source.id);
                }}
                onDragEnd={() => setDragging(null)}
                onKeyDown={(event) => onHandleKey(event, source, index)}>
                ⠿
              </span>
              <button className="swatch-button" data-feature="sources:colour"
                style={{ backgroundColor: source.colour }} aria-expanded={picking === source.id}
                title={t("Change the colour")} aria-label={t("Colour")}
                onClick={() => setPicking(picking === source.id ? null : source.id)} />
              <input type="checkbox" checked={source.visible} data-feature="sources:visible"
                title={t("Show or hide this source. A hidden source is left out of the blend and every plot.")}
                aria-label={t("Visible")}
                onChange={(event) => void run(api.setSourceVisible(source.id, event.target.checked))} />
              <span className="kind-icon" title={kind ? t(kind.name) : source.kind}
                aria-label={kind ? t(kind.name) : source.kind}>{kind?.glyph ?? "?"}</span>
              {renaming?.id === source.id
                ? <input className="source-rename" autoFocus value={renaming.text} data-feature="sources:rename"
                  aria-label={t("Source name")} title={t("Type a new name; Enter to keep it, Esc to cancel")}
                  onChange={(event) => setRenaming({ id: source.id, text: event.target.value })}
                  onBlur={commitRename}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") commitRename();
                    else if (event.key === "Escape") setRenaming(null);
                  }} />
                : <button className="source-label" data-feature="sources:rename"
                  title={t("Click to rename")}
                  onClick={() => setRenaming({ id: source.id, text: source.label })}>
                  {source.label}
                </button>}
              <span className="muted source-count">
                {source.used === null
                  ? t("{count} cells", { count: source.count })
                  : t("{used}/{count} samples", { used: source.used, count: source.count })}
              </span>
            </div>
            <div className="source-line">
              <input type="range" min={0} max={2} step={0.05} value={weight} data-feature="sources:weight"
                aria-label={t("Weight")} title={t("Blend weight, 0 to 2. 1 is the default.")}
                onPointerDown={() => { gestures += 1; gesture.current = `drag-${gestures}`; }}
                onPointerUp={() => endWeight(source)}
                onBlur={() => endWeight(source)}
                onChange={(event) => setWeight(source, Number(event.target.value))} />
              <span className="source-weight">{weight.toFixed(2)}</span>
              <button className="small" data-feature="sources:edit"
                title={t("Edit this source's polar in the 3D view, with its table")}
                onClick={() => editSource(source.id)}>
                {t("Edit")}{source.edits > 0 && <span className="muted" title={t("{count} edits", { count: source.edits })}> ✎</span>}
              </button>
              <button className="small" disabled data-feature="sources:compare"
                title={t("Compare this source with another or with the blend. Arrives in a later version.")}>{t("Compare")}</button>
              <button className="small" data-feature="sources:remove"
                title={t("Remove this source from the project (undoable)")}
                onClick={() => void run(api.removeSource(source.id))}>{t("Remove")}</button>
            </div>
            {picking === source.id && (
              <ColourPicker colour={source.colour} onClose={() => setPicking(null)}
                onChoose={(colour) => {
                  if (colour !== source.colour) void run(api.setSourceColour(source.id, colour));
                }} />
            )}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * The colour popover: the sixteen palette colours new sources take, and a
 * custom picker. A palette colour applies and closes; the custom picker
 * applies when the system picker is closed, so its drag is one change.
 */
function ColourPicker({ colour, onChoose, onClose }: {
  colour: string;
  onChoose: (colour: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const custom = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const choose = useRef(onChoose);
  choose.current = onChoose;
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const input = custom.current;
    // React's onChange is the DOM's `input`, which fires on every movement
    // of the system picker; `change` fires once, when it is closed.
    const commit = () => { if (input) choose.current(input.value); };
    input?.addEventListener("change", commit);
    const onKey = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") close.current(); };
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (box.current && target && !box.current.contains(target)
        && !(target instanceof HTMLElement && target.closest(".swatch-button"))) close.current();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      input?.removeEventListener("change", commit);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, []);

  return (
    <div className="colour-popover" ref={box} role="dialog" aria-label={t("Source colour")}>
      <div className="palette" data-feature="sources:palette">
        {PALETTE.map((option) => (
          <button key={option} className={option === colour ? "palette-swatch selected" : "palette-swatch"}
            style={{ backgroundColor: option }} aria-pressed={option === colour} title={option} aria-label={option}
            onClick={() => { onChoose(option); onClose(); }} />
        ))}
      </div>
      <label className="custom-colour">
        {t("Custom colour")}
        <input ref={custom} type="color" defaultValue={colour} data-feature="sources:custom-colour"
          title={t("Pick any colour")} />
      </label>
    </div>
  );
}
