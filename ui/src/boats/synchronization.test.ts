import { expect, it } from "vitest";
import { correspondingCell, correspondingDot, createFleetSync } from "./synchronization";
it("links camera state without echoing and gives newly mounted panes the latest view", () => {
  const sync = createFleetSync();
  const received: unknown[] = [];
  const stop = sync.subscribe(event => received.push(event));
  const camera = { kind: "camera", boat: 1, view: { position: [10,20,30], target: [0,0,10] }, layout: "tower" } as const;
  sync.publish(camera);
  expect(received).toEqual([camera]);
  const late: unknown[] = []; sync.subscribe(event => late.push(event));
  expect(late).toEqual([camera]); stop();
  sync.publish({ kind: "hover", boat: 2, point: { twa: 90, tws: 12 } });
  expect(received).toHaveLength(1); expect(late).toHaveLength(2);
});
it("links by nearby wind conditions, preserves asymmetric sides, and reports no match", () => {
  const points = new Float32Array([90,10,7, 270,10,9, 45,16,6, 359,8,2]);
  expect(correspondingDot(points, {twa: 89, tws: 10.2})).toBe(0);
  expect(correspondingDot(points, {twa: 270, tws: 10})).toBe(1);
  expect(correspondingDot(points, {twa: 1, tws: 8})).toBe(3);
  expect(correspondingDot(points, {twa: 90, tws: 16})).toBe(-1);
  expect(correspondingDot(points, {twa: 100, tws: 10})).toBe(-1);
});
it("links a hovered blend cell to the cell at the same wind in another boat's grid", () => {
  const twa = [32, 52, 60, 75, 90, 110, 120, 135, 150, 180], tws = [6, 8, 10, 12, 14, 16, 20];
  // The same grid: the same cell.
  expect(correspondingCell(twa, tws, { twa: 90, tws: 12 })).toEqual({ twa: 4, tws: 3 });
  // Another boat's grid: the cell nearest in wind, within the tolerance the dots use.
  expect(correspondingCell([30, 50, 70, 88, 110], [5, 11.5, 15], { twa: 90, tws: 12 })).toEqual({ twa: 3, tws: 1 });
  // Nothing near enough in angle, or in wind speed, is no cell rather than a far one.
  expect(correspondingCell(twa, tws, { twa: 100, tws: 12 })).toBeNull();
  expect(correspondingCell(twa, tws, { twa: 90, tws: 24 })).toBeNull();
  expect(correspondingCell([], [], { twa: 90, tws: 12 })).toBeNull();
  // An asymmetric grid keeps its sides apart, and 359° is next to 0°.
  expect(correspondingCell([0, 90, 180, 270], tws, { twa: 270, tws: 10 })).toEqual({ twa: 3, tws: 2 });
  expect(correspondingCell([0, 90, 180, 270], tws, { twa: 358, tws: 10 })).toEqual({ twa: 0, tws: 2 });
});
it("carries a blend cell's wind to the other panes, and its end", () => {
  const sync = createFleetSync();
  const received: unknown[] = [];
  sync.subscribe(event => received.push(event));
  sync.publish({ kind: "blend", boat: 1, point: { twa: 90, tws: 12 } });
  sync.publish({ kind: "blend", boat: 1, point: null });
  expect(received).toEqual([{ kind: "blend", boat: 1, point: { twa: 90, tws: 12 } }, { kind: "blend", boat: 1, point: null }]);
});
