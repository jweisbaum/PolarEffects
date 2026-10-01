import { DAY_BANDS, dayBand } from "../dayBand";
import type { Units } from "../generated/Units";
import type { SourceSummary } from "../generated/SourceSummary";
import { useT } from "../i18n";
import { utc } from "../map/hover";
import { waveUnit } from "../panels/filterUnits";
import { FLAG_EXCLUDED, FLAG_FILTERED, type ScenePacket } from "./scenePacket";
import { SPEED_FACTOR, SPEED_SYMBOL } from "./view3d";

/** Read the displayed packet, so a hover always describes the dot being drawn. */
export default function PolarDotTooltip({ packet, index, sources, units, x, y }: {
  packet: ScenePacket; index: number; sources: SourceSummary[]; units: Units; x: number; y: number;
}) {
  const t = useT();
  const node = index < packet.nodes.count;
  const k = node ? index : index - packet.nodes.count;
  const data = node ? packet.nodes : packet.samples;
  const source = packet.sources[data.source[k]!];
  if (!source || k < 0 || k >= data.count) return null;
  const label = sources.find(s => s.id === source.id)?.label ?? t("Unknown source");
  const wave = waveUnit(units);
  const number = (value: number, suffix: string, factor = 1, digits = 1) => Number.isFinite(value) ? `${(value * factor).toFixed(digits)} ${suffix}` : "–";
  const speed = (value: number) => number(value, SPEED_SYMBOL[units.speed], SPEED_FACTOR[units.speed]);
  const twa = data.points[k * 3]!;
  const flags = data.flags[k]!;
  const rows = [
    ["TWA", number(Math.min(twa, 360 - twa), "°", 1, 0)],
    ["TWS", speed(data.points[k * 3 + 1]!)],
    ["BSP", speed(data.points[k * 3 + 2]!)],
  ];
  if (!node) rows.push(
    [t("Time"), utc(packet.timeOrigin + packet.samples.time[k]!)],
    [t("Time of day"), t(DAY_BANDS[dayBand(flags)]!.label)],
    [t("Wave height"), number(packet.samples.hs[k]!, wave.symbol, wave.factor)],
    [t("Wave angle off the bow"), number(packet.samples.waveAngle[k]!, "°", 1, 0)],
    [t("Wave period"), number(packet.samples.wavePeriod[k]!, "s")],
    [t("Current"), speed(packet.samples.current[k]!)],
  );
  return <div role="tooltip" className="view3d-tooltip" style={{ left: x, top: y }}>
    <strong style={{ color: source.colour }}>{label}</strong>
    <dl>{rows.map(([name, value]) => <div key={name}><dt>{name}</dt><dd>{value}</dd></div>)}</dl>
    {!!(flags & (FLAG_EXCLUDED | FLAG_FILTERED)) && <div className="muted">{flags & FLAG_EXCLUDED ? t("Excluded from the blend") : t("Filtered out")}</div>}
  </div>;
}
