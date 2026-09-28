import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { reportFailure } from "../errors";
import { reportError } from "../hint";
import type { EditOp } from "../generated/EditOp";
import type { EditSurface } from "../generated/EditSurface";
import type { PolarCell } from "../generated/PolarCell";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { msg, useT } from "../i18n";
import { api } from "../ipc";

/** A cell as the 3D view keys it: TWA index | TWS index << 16. */
export function cellCode(i: number, j: number): number {
  return i | (j << 16);
}

const STATISTICS: readonly { id: string; label: string }[] = [
  { id: "p90", label: msg("90th percentile") },
  { id: "p75", label: msg("75th percentile") },
  { id: "median", label: msg("Median") },
  { id: "mean", label: msg("Mean") },
];

/** A typed value, knots; null for an empty box (reset the cell); undefined for something that is not a speed. */
export function parseSpeed(text: string): number | null | undefined {
  const trimmed = text.trim().replace(",", ".");
  if (trimmed === "") return null;
  const value = Number(trimmed);
  return Number.isFinite(value) && value >= 0 && value <= 60 ? value : undefined;
}

/**
 * Editing one source (spec.md 10.4), beside the 3D view: the table of its
 * editable surface (TWA rows × TWS columns, knots), where typing a value is
 * an edit and every edited cell is marked; the tools that act on the
 * selected cells — scale by a percentage, smooth, reset — and Reset all
 * edits; for a track, the statistic of its polar segment (spec.md 12.1).
 * Every action is one undoable change computed in Rust; the table refetches
 * the surface on every revision.
 */
export default function EditPanel({ project, sourceId, selected, hideOthers, onHideOthers, onSelectCells, onProject, onDone }: {
  project: ProjectSummary;
  sourceId: number;
  /** The selected cells (see `cellCode`). */
  selected: ReadonlySet<number>;
  hideOthers: boolean;
  onHideOthers: (hide: boolean) => void;
  /** Selects cells; `add` keeps the ones already selected. */
  onSelectCells: (codes: number[], add: boolean) => void;
  onProject: (summary: ProjectSummary) => void;
  onDone: () => void;
}) {
  const t = useT();
  const [surface, setSurface] = useState<EditSurface | null>(null);
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  const [percent, setPercent] = useState("5");
  const request = useRef(0);
  const source = project.sources.find((s) => s.id === sourceId) ?? null;

  useEffect(() => {
    const id = ++request.current;
    void api.polarEditSurface(sourceId)
      .then((next) => { if (request.current === id) setSurface(next); })
      .catch((error: unknown) => { if (request.current === id) reportFailure(error); });
  }, [sourceId, project.id, project.revision]);

  const run = (change: Promise<ProjectSummary>) => change.then(onProject).catch(reportFailure);
  const cells: PolarCell[] = useMemo(
    () => [...selected].map((code) => ({ twa_index: code & 0xffff, tws_index: code >>> 16 })),
    [selected],
  );
  const apply = (op: EditOp) => void run(api.editPolar(sourceId, op, cells));
  const scale = Number(percent.replace(",", "."));
  const scaleValid = Number.isFinite(scale) && scale > -100 && scale <= 1000 && scale !== 0;

  const commit = (i: number, j: number) => {
    const code = cellCode(i, j);
    const text = drafts[code];
    if (text === undefined || !surface) return;
    setDrafts((current) => {
      const next = { ...current };
      delete next[code];
      return next;
    });
    const value = parseSpeed(text);
    const held = surface.bsp[i]?.[j] ?? null;
    if (value === undefined) {
      reportError(t("{text} is not a boat speed from 0 to 60 kn", { text }), null);
      return;
    }
    // An empty box resets an edited cell; on a cell without an edit it changes nothing.
    if (value === null && !surface.edited[i]?.[j]) return;
    if (value !== null && held !== null && Math.abs(value - held) < 1e-9) return;
    void run(api.editPolar(sourceId, { type: "type", bsp: value }, [{ twa_index: i, tws_index: j }]));
  };

  const onKey = (event: KeyboardEvent<HTMLInputElement>, i: number, j: number) => {
    if (event.key === "Enter") {
      event.currentTarget.blur();
    } else if (event.key === "Escape") {
      event.stopPropagation();
      setDrafts((current) => {
        const next = { ...current };
        delete next[cellCode(i, j)];
        return next;
      });
      event.currentTarget.blur();
    }
  };

  const isTrack = surface?.kind === "track";
  const cellTip = (i: number, j: number): string => {
    if (!surface) return "";
    const imported = surface.source[i]?.[j];
    const base = imported === null || imported === undefined
      ? (isTrack ? t("No segment value here") : t("No value in the source here"))
      : (isTrack ? t("Segment value {bsp} kn", { bsp: imported.toFixed(2) }) : t("Source value {bsp} kn", { bsp: imported.toFixed(2) }));
    const parts = [base];
    if (isTrack && surface.count) {
      const count = surface.count[i]?.[j] ?? 0;
      const spread = surface.spread?.[i]?.[j];
      parts.push(spread === null || spread === undefined
        ? t("{count} samples", { count })
        : t("{count} samples, spread ±{spread} kn", { count, spread: spread.toFixed(2) }));
    }
    if (surface.edited[i]?.[j]) parts.push(t("Edited"));
    if (surface.excluded[i]?.[j]) parts.push(t("Excluded from the blend"));
    return parts.join(" · ");
  };

  return (
    <section className="view3d-edit" aria-label={t("Edit polar")}>
      <header className="view3d-edit-head">
        <span className="swatch" style={{ background: source?.colour }} />
        <strong>{t("Editing {label}", { label: source?.label ?? "" })}</strong>
        <span className="spacer" />
        <button className="small" data-feature="edit:done" title={t("Leave edit mode (the edits stay)")} onClick={onDone}>
          {t("Done")}
        </button>
      </header>
      <div className="view3d-edit-row">
        <label title={t("Hide the other sources instead of fading them")}>
          <input type="checkbox" data-feature="edit:hide-others" checked={hideOthers}
            onChange={(event) => onHideOthers(event.target.checked)} />
          {t("Hide other sources")}
        </label>
        {isTrack && (
          <label className="view3d-edit-statistic">
            {t("Statistic")}
            <select data-feature="edit:statistic" value={surface?.statistic ?? "p90"}
              title={t("How a cell of the track's polar segment sums up its samples' boat speeds (undoable)")}
              onChange={(event) => void run(api.setSegmentStatistic(sourceId, event.target.value))}>
              {STATISTICS.map((s) => <option key={s.id} value={s.id}>{t(s.label)}</option>)}
            </select>
          </label>
        )}
      </div>
      {isTrack && surface && (
        <p className="muted view3d-edit-note">
          {t("A cell needs {count} samples to have a value.", { count: surface.min_samples })}
        </p>
      )}
      <div className="view3d-edit-row">
        <input className="view3d-edit-percent" data-feature="edit:scale-percent" value={percent} inputMode="decimal"
          aria-label={t("Scale by (%)")} title={t("The percentage Scale applies: 5 is 5 % faster, -5 is 5 % slower")}
          onChange={(event) => setPercent(event.target.value)} />
        <span>%</span>
        <button className="small" data-feature="edit:scale" disabled={cells.length === 0 || !scaleValid}
          title={t("Scale the selected cells by the percentage (undoable)")}
          onClick={() => apply({ type: "scale", percent: scale })}>
          {t("Scale")}
        </button>
        <button className="small" data-feature="edit:smooth" disabled={cells.length === 0}
          title={t("Smooth the selected cells over their neighbours on the grid (undoable)")}
          onClick={() => apply({ type: "smooth" })}>
          {t("Smooth")}
        </button>
        <button className="small" data-feature="edit:reset" disabled={cells.length === 0}
          title={t("Put the selected cells back to the source's values (undoable)")}
          onClick={() => apply({ type: "reset" })}>
          {t("Reset")}
        </button>
        <button className="small" data-feature="edit:reset-all" disabled={(surface?.edit_count ?? 0) === 0}
          title={t("Clear every edit of this source (undoable)")}
          onClick={() => void run(api.editPolar(sourceId, { type: "reset_all" }, []))}>
          {t("Reset all edits")}
        </button>
      </div>
      <p className="muted view3d-edit-note">
        {cells.length === 0
          ? t("Select cells in the table or nodes in the view; drag a node with the Drag tool.")
          : t("{count} cells selected", { count: cells.length })}
        {" · "}
        {t("{count} edits", { count: surface?.edit_count ?? 0 })}
      </p>
      {surface && (
        <div className="view3d-edit-table-wrap" data-feature="edit:table">
          <table className="view3d-edit-table">
            <thead>
              <tr>
                <th title={t("TWA (°) down, TWS (kn) across; boat speeds in knots")}>TWA \ TWS</th>
                {surface.tws.map((tws) => <th key={tws}>{tws}</th>)}
              </tr>
            </thead>
            <tbody>
              {surface.twa.map((twa, i) => (
                <tr key={twa}>
                  <th>{twa}°</th>
                  {surface.tws.map((tws, j) => {
                    const code = cellCode(i, j);
                    const value = surface.bsp[i]?.[j] ?? null;
                    const classes = ["view3d-edit-cell"];
                    if (surface.edited[i]?.[j]) classes.push("edited");
                    if (surface.excluded[i]?.[j]) classes.push("excluded");
                    if (selected.has(code)) classes.push("selected");
                    return (
                      <td key={tws} className={classes.join(" ")}>
                        <input value={drafts[code] ?? (value === null ? "" : value.toFixed(2))} placeholder="–"
                          inputMode="decimal" aria-label={t("BSP at {twa}° and {tws} kn", { twa, tws })} title={cellTip(i, j)}
                          onFocus={(event) => { if (!selected.has(code) || selected.size > 1) onSelectCells([code], false); event.currentTarget.select(); }}
                          onMouseDown={(event) => { if (event.shiftKey) { event.preventDefault(); onSelectCells([code], true); } }}
                          onChange={(event) => setDrafts((current) => ({ ...current, [code]: event.target.value }))}
                          onBlur={() => commit(i, j)}
                          onKeyDown={(event) => onKey(event, i, j)} />
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
