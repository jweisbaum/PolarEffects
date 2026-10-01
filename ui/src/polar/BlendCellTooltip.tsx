import type { BlendCell } from "../generated/BlendCell";
import type { SourceSummary } from "../generated/SourceSummary";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { useT } from "../i18n";
import { SPEED_FACTOR, SPEED_SYMBOL } from "./view3d";

/**
 * What hovering the blend shows, in the 3D view and the 2D plot (spec.md
 * 9.2, 10.1): the output-grid cell, its value, where the value came from,
 * and each source the blending rule counted there with its own speed and
 * its share of the weight. Rust works all of it out (`blend_cell`); this
 * only words it. Every speed arrives in knots and is shown in the display
 * unit.
 */
export default function BlendCellTooltip({ cell, sources, colour, unit, smoothing, x, y }: {
  cell: BlendCell;
  sources: readonly SourceSummary[];
  /** The Blend entry's colour. */
  colour: string;
  unit: SpeedUnit;
  /** Whether the blend is smoothed (Blend settings): the value is then not exactly the sources' mean. */
  smoothing: boolean;
  x: number;
  y: number;
}) {
  const t = useT();
  const speed = (knots: number, digits: number) => `${(knots * SPEED_FACTOR[unit]).toFixed(digits)} ${SPEED_SYMBOL[unit]}`;
  const headToWind = cell.twa === 0 || cell.twa === 360;
  const note = cell.corrected ? t("Corrected by hand; the sources below gave the value before.")
    : cell.origin === "empty" ? t("No source reaches this cell.")
    : headToWind ? t("Head to wind: 0 by definition.")
    : cell.origin === "filled" ? t("Filled in between neighbouring cells; no source has a value here.")
    : smoothing && cell.contributors.length > 0 ? t("Smoothed after blending these sources.")
    : null;
  return (
    <div role="tooltip" className="view3d-tooltip blend-cell-tooltip" style={{ left: x, top: y }}>
      <strong style={{ color: colour }}>{t("Blend")}</strong>
      <dl>
        <div><dt>TWA</dt><dd>{Math.min(cell.twa, 360 - cell.twa).toFixed(0)}°</dd></div>
        <div><dt>TWS</dt><dd>{speed(cell.tws, 1)}</dd></div>
        <div><dt>BSP</dt><dd>{cell.bsp === null ? "–" : speed(cell.bsp, 2)}</dd></div>
      </dl>
      {note && <div className="muted blend-cell-note">{note}</div>}
      {cell.contributors.length > 0 && (
        <ul className="blend-cell-sources" aria-label={t("Sources behind this cell")}>
          {cell.contributors.map((contributor) => {
            const source = sources.find((candidate) => candidate.id === contributor.source_id);
            return (
              <li key={contributor.source_id}>
                <span className="blend-cell-source">
                  <span className="day-band-swatch" style={{ background: source?.colour ?? "var(--muted)" }} />
                  {source?.label ?? t("Unknown source")}
                </span>
                <span>{speed(contributor.bsp, 2)}</span>
                <span title={t("Its share of the cell's weight")}>{Math.round(contributor.share * 100)}%</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
