import { useBoatApi } from "../boats/context";
import { useMemo } from "react";
import { useLiveEdit } from "./useLiveEdit";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { TrackFilters } from "../generated/TrackFilters";
import type { Units } from "../generated/Units";
import { useT } from "../i18n";

import { NumberField, SampleFiltersEditor } from "./Tracks";
import { NO_SAMPLE_FILTERS } from "./sampleFilters";

/** Ordered filters select the evidence for each cell in Rust, across tracks. */
export default function PriorityFilters({ project, units, onProject }: {
  project: ProjectSummary;
  units: Units;
  onProject: (project: ProjectSummary) => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const value = useMemo(() => ({ groups: project.blend.priority_groups, minimum: project.blend.priority_min_samples }), [project.blend.priority_groups, project.blend.priority_min_samples]);
  const [{ groups, minimum }, update] = useLiveEdit(value, (next) => api.setPriorityFilters(next.groups, next.minimum).then(onProject));
  const save = (next: TrackFilters[], count = minimum) => update(() => ({ groups: next, minimum: count }));
  const move = (index: number, delta: number) => {
    const next = [...groups];
    const item = next.splice(index, 1)[0];
    if (item) { next.splice(index + delta, 0, item); save(next); }
  };
  return <details className="priority-filters">
    <summary data-feature="view3d:priority-filters">{t("Priority filter groups")}</summary>
    <div className="point-filter-body">
      <p className="muted">{t("Try groups in order for each TWA/TWS cell. Use the first with enough samples across visible tracks.")}</p>
      <NumberField feature="priority:minimum" label={t("Minimum samples per group")} title={t("Minimum samples per group")}
        value={minimum} min={1} max={1000000} step={1} onCommit={(count) => {
          if (count !== null && Number.isInteger(count)) void update((previous) => ({ ...previous, minimum: count }));
        }} />
      {groups.length === 0 && <p className="muted">{t("No priority groups. Every point passing the track and global filters is eligible.")}</p>}
      {groups.map((filters, index) => <details key={index} className="priority-group">
        <summary data-feature="priority:group">{t("Priority {number}", { number: index + 1 })}</summary>
        <div className="settings-buttons">
          <button data-feature="priority:up" disabled={index === 0} onClick={() => move(index, -1)}>{t("Move up")}</button>
          <button data-feature="priority:down" disabled={index === groups.length - 1} onClick={() => move(index, 1)}>{t("Move down")}</button>
          <button data-feature="priority:remove" onClick={() => save(groups.filter((_, i) => i !== index))}>{t("Remove")}</button>
        </div>
        <SampleFiltersEditor filters={filters} units={units} prefix="priority-filters"
          onChange={(next) => update((previous) => ({ ...previous, groups: previous.groups.map((group, i) => i === index ? next : group) }))} />
      </details>)}
      <button data-feature="priority:add" disabled={groups.length >= 16} onClick={() => save([...groups, { ...NO_SAMPLE_FILTERS }])}>{t("Add priority group")}</button>
    </div>
  </details>;
}
