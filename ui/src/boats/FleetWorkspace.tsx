import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { AppSettings } from "../generated/AppSettings";
import type { BoatTabs } from "../generated/BoatTabs";
import type { BoatExportResult } from "../generated/BoatExportResult";
import { api, boatApi } from "../ipc";
import { reportFailure } from "../errors";
import { useT } from "../i18n";
import { onReveal } from "../help/highlight";
import { pickExportDirectory } from "../project/dialogs";
import { BoatProvider } from "./context";
import BoatWorkspace from "./BoatWorkspace";
import { FleetSyncProvider } from "./synchronization";
import ConfirmDialog from "../project/ConfirmDialog";

type Layout = 1 | 2 | 4;

export default function FleetWorkspace({ project, settings, onSettings, onProject, changedBoat, onActive, toolbarHost, onReplace }: {
  project: ProjectSummary; settings: AppSettings | null; onSettings: (settings: AppSettings) => void;
  onProject: (project: ProjectSummary) => void; changedBoat: ProjectSummary | null; onActive: (id: number) => void;
  toolbarHost: HTMLElement | null; onReplace: (previousId: number, project: ProjectSummary) => void;
}) {
  const t = useT();
  const [tabs, setTabs] = useState<BoatTabs | null>(null);
  const [summaries, setSummaries] = useState<Record<number, ProjectSummary>>({ [project.id]: project });
  const [active, setActive] = useState(project.id);
  const [layout, setLayout] = useState<Layout>(1);
  useEffect(() => onReveal("boats:tabs", () => setLayout(1)), []);
  const [panes, setPanes] = useState<number[]>([project.id]);
  const [renaming, setRenaming] = useState<{ id: number; name: string } | null>(null);
  const [deleting, setDeleting] = useState<{ id: number; name: string } | null>(null);
  const [changingBoats, setChangingBoats] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [format, setFormat] = useState("expedition");
  const [result, setResult] = useState<BoatExportResult | null>(null);
  const [busy, setBusy] = useState(false);
  const rootId = useRef(project.id);
  rootId.current = project.id;
  const removed = useRef(new Set<number>());
  const tabsRequest = useRef(0);
  const merge = useCallback((next: ProjectSummary) => setSummaries(current =>
    !removed.current.has(next.id) && (!current[next.id] || current[next.id]!.revision <= next.revision) ? { ...current, [next.id]: next } : current), []);
  const refreshTabs = useCallback(() => {
    const request = ++tabsRequest.current;
    return api.boatTabs().then(value => {
      if (request === tabsRequest.current && value?.tabs && value.project_id === rootId.current) setTabs(value);
    }).catch(reportFailure);
  }, []);
  useEffect(() => { merge(project); void refreshTabs(); }, [project, merge, refreshTabs]);
  useEffect(() => { if (changedBoat) { merge(changedBoat); void refreshTabs(); } }, [changedBoat, merge, refreshTabs]);
  const rows = tabs?.tabs ?? [{ id: project.id, name: project.boat_name || project.name, details: {} }];
  const shown = layout === 1 ? [active] : panes.slice(0, layout);
  const load = (id: number) => boatApi(id).projectSummary().then(value => { if (value) merge(value); }).catch(reportFailure);
  const loaded = useRef<number[]>([]);
  loaded.current = Object.keys(summaries).map(Number);
  useEffect(() => {
    // A child write also advances the root revision. Refresh cached child
    // summaries after weather writes and save/undo, without a second event listener.
    for (const id of loaded.current) if (id !== project.id) void boatApi(id).projectSummary().then(value => { if (value) merge(value); }).catch(reportFailure);
  }, [project.revision, project.dirty, project.id, merge]);
  const select = (id: number) => { setActive(id); onActive(id); setRenaming(null); void load(id); };
  const update = (next: ProjectSummary) => {
    merge(next);
    void refreshTabs();
    if (next.id === project.id) onProject(next);
    else void api.projectSummary().then(root => { if (root) onProject(root); }).catch(reportFailure);
  };
  const chooseLayout = (next: Layout) => {
    setLayout(next);
    if (next === 1) return;
    const ids = [active, ...rows.map(row => row.id).filter(id => id !== active)].slice(0, next);
    setPanes(ids);
    for (const id of ids) void load(id);
  };
  const add = async () => {
    setChangingBoats(true);
    try {
      const next = await api.addBoat(t("Boat {number}", { number: rows.length + 1 }));
      update(next); select(next.id);
      if (layout > 1) setPanes(current => current.length < layout ? [...current, next.id] : current);
    } catch (error) { reportFailure(error); }
    finally { setChangingBoats(false); }
  };
  const commitName = () => {
    const edit = renaming; setRenaming(null);
    if (edit?.name.trim()) void boatApi(edit.id).renameBoat(edit.name.trim()).then(update).catch(reportFailure);
  };
  const changedStructure = (next: ProjectSummary) => {
    if (next.id === project.id) update(next);
    else {
      // Promoting/restoring the first boat changes the file's root identity,
      // not the fleet opening. Keep sibling workspaces and comparison state.
      rootId.current = next.id;
      merge(next);
      onReplace(project.id, next);
      void refreshTabs();
    }
  };
  const remove = async () => {
    if (!deleting) return;
    const id = deleting.id;
    setDeleting(null); setChangingBoats(true);
    try {
      const next = await api.deleteBoat(project.id, id);
      removed.current.add(id);
      setSummaries(current => { const remaining = { ...current }; delete remaining[id]; return remaining; });
      const remaining = rows.filter(row => row.id !== id);
      const nextPanes = [...new Set([...panes.filter(value => value !== id), ...remaining.map(row => row.id)])].slice(0, layout);
      setPanes(nextPanes); for (const value of nextPanes) void load(value);
      if (active === id) select(remaining[0]!.id);
      changedStructure(next);
    } catch (error) { reportFailure(error); }
    finally { setChangingBoats(false); }
  };
  const restore = async () => {
    setChangingBoats(true);
    try {
      const next = await api.restoreBoat(project.id);
      removed.current.clear();
      changedStructure(next);
    } catch (error) { reportFailure(error); }
    finally { setChangingBoats(false); }
  };
  const exportAll = async () => {
    setBusy(true);
    try { const directory = await pickExportDirectory(); if (directory) setResult(await api.exportAllPolars(directory, format)); }
    catch (error) { reportFailure(error); }
    finally { setBusy(false); }
  };
  return <div className="fleet-workspace">
    {toolbarHost && createPortal(<button data-feature="boats:add" disabled={changingBoats} onClick={() => void add()}>{t("Add boat")}</button>, toolbarHost)}
    <div className="boat-actions">
      <div role="group" aria-label={t("Boat comparison layout")}>
        <button data-feature="boats:single" aria-pressed={layout === 1} onClick={() => chooseLayout(1)}>{t("Single view")}</button>
        <button data-feature="boats:split" aria-pressed={layout === 2} onClick={() => chooseLayout(2)}>{t("Split view")}</button>
        <button data-feature="boats:four" aria-pressed={layout === 4} onClick={() => chooseLayout(4)}>{t("Four-way view")}</button>
      </div>
      <span className="spacer" />
      {tabs?.can_restore && <button data-feature="boats:restore" disabled={changingBoats} onClick={() => void restore()}>{t("Undo delete boat")}</button>}
      <button data-feature="boats:delete" disabled={changingBoats || rows.length < 2}
        title={rows.length < 2 ? t("A project must contain at least one boat") : t("Delete {boat}", { boat: rows.find(row => row.id === active)?.name ?? "" })}
        onClick={() => { const row = rows.find(row => row.id === active); if (row) setDeleting(row); }}>{t("Delete boat…")}</button>
      {rows.length > 1 && <button data-feature="boats:export-all" onClick={() => { setResult(null); setExporting(true); }}>{t("Export all…")}</button>}
    </div>
    {layout === 1 && <div className="boat-tabs-row">
      <div className="boat-tabs" role="tablist" aria-label={t("Boats")}>
        {rows.map(row => renaming?.id === row.id
          ? <input key={row.id} className="boat-tab-name" value={renaming.name} autoFocus aria-label={t("Boat name")} data-feature="boats:name"
            onFocus={event => event.currentTarget.select()} onChange={event => setRenaming({ id: row.id, name: event.target.value })} onBlur={commitName}
            onKeyDown={event => { event.stopPropagation(); if (event.key === "Enter") event.currentTarget.blur(); if (event.key === "Escape") setRenaming(null); }} />
          : <button key={row.id} role="tab" aria-selected={active === row.id} data-feature="boats:tab" title={t("Double-click to rename the boat")}
            onClick={() => select(row.id)} onDoubleClick={() => { select(row.id); setRenaming({ id: row.id, name: row.name }); }}
            onKeyDown={event => { if (event.key === "F2") { event.preventDefault(); select(row.id); setRenaming({ id: row.id, name: row.name }); } }}>{row.name}</button>)}
      </div>
    </div>}
    <FleetSyncProvider enabled={layout > 1}>
      <div className={`boat-grid boats-${layout}`}>
        {Object.values(summaries).map(summary => {
          const slot = shown.indexOf(summary.id);
          return <section key={summary.id} className={`boat-pane${active === summary.id ? " active" : ""}`} hidden={slot < 0}
            style={{ order: slot }} data-boat-id={summary.id} onPointerDownCapture={() => { if (active !== summary.id) select(summary.id); }}>
            {layout > 1 && slot >= 0 && <select className="boat-pane-picker" data-feature="boats:pane" aria-label={t("Boat in comparison pane")}
              value={summary.id} onChange={event => {
                const id = Number(event.target.value);
                setPanes(current => current.map((old, i) => i === slot ? id : old === id ? summary.id : old));
                select(id);
              }}>{rows.map(row => <option key={row.id} value={row.id}>{row.name}</option>)}</select>}
            <BoatProvider id={summary.id} active={active === summary.id && slot >= 0}><BoatWorkspace project={summary} settings={settings} onSettings={onSettings}
              onProject={update} split={layout > 1} visible={slot >= 0} active={active === summary.id && slot >= 0} /></BoatProvider>
          </section>;
        })}
        {Array.from({ length: Math.max(0, layout - shown.length) }, (_, i) => <div className="boat-pane empty" key={`empty-${i}`} style={{ order: shown.length + i }}>
          <p>{t("Add another boat to compare.")}</p><button data-feature="boats:add-empty" disabled={changingBoats} onClick={() => void add()}>{t("Add boat")}</button>
        </div>)}
      </div>
    </FleetSyncProvider>
    {deleting && <ConfirmDialog title={t("Delete {boat}", { boat: deleting.name })}
      body={t("Remove this boat and its sources from the project? You can undo this deletion while the project remains open.")}
      confirmLabel={t("Delete boat")} onConfirm={() => void remove()} onCancel={() => setDeleting(null)} />}
    {exporting && <div className="modal-backdrop"><section className="modal boat-export-dialog" role="dialog" aria-modal="true" aria-label={t("Export all polars")}>
      <h2>{t("Export all polars")}</h2>
      <label>{t("Format")} <select data-feature="boats:export-format" value={format} onChange={event => setFormat(event.target.value)}>
        <option value="expedition">{t("Expedition")}</option><option value="adrena">{t("Adrena")}</option><option value="csv">{t("CSV")}</option>
      </select></label>
      {result && <><p role="status">{t("Exported {count} boat polars.", { count: result.paths.length })}</p>
        {result.failures.length > 0 && <ul>{result.failures.map(([name, error], i) => <li key={i}>{name}: {error}</li>)}</ul>}</>}
      <div className="modal-actions"><button data-feature="boats:export-close" disabled={busy} onClick={() => setExporting(false)}>{t("Close")}</button>
        <button data-feature="boats:export-confirm" disabled={busy} onClick={() => void exportAll()}>{t("Choose export folder…")}</button></div>
    </section></div>}
  </div>;
}
