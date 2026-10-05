// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { DEFAULT_UNITS } from "../panels/filterUnits";
import PolarDotTooltip from "./PolarDotTooltip";
import { FLAG_THROUGH_WATER, unpackScene, type ScenePacket } from "./scenePacket";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The scene the Rust test packed: two polar nodes, then one sample corrected for current. */
function scene(): ScenePacket {
  // By directory, not by URL: under happy-dom the module's URL is not a file one.
  const bytes = readFileSync(join(__dirname, "fixtures", "scene-v5.bin"));
  return unpackScene(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer);
}

async function rows(packet: ScenePacket, index: number): Promise<string[]> {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(<PolarDotTooltip packet={packet} index={index} sources={[]} units={DEFAULT_UNITS} x={0} y={0} />));
  const terms = [...host.querySelectorAll("dt")].map((dt) => dt.textContent ?? "");
  await act(async () => root.unmount());
  host.remove();
  return terms;
}

it("calls a sample's speed SOG, STW where it was corrected for current, and a polar node's BSP (asked 2026-10-04)", async () => {
  const packet = scene();
  expect(await rows(packet, 0)).toContain("BSP");
  const sample = packet.nodes.count;
  expect(packet.samples.flags[0]! & FLAG_THROUGH_WATER).toBeTruthy();
  expect(await rows(packet, sample)).toContain("STW");
  const overGround = { ...packet, samples: { ...packet.samples, flags: Uint32Array.from([packet.samples.flags[0]! & ~FLAG_THROUGH_WATER]) } };
  const shown = await rows(overGround, sample);
  expect(shown).toContain("SOG");
  expect(shown).not.toContain("BSP");
});
