import { expect, it } from "vitest";
import { correspondingDot, createFleetSync } from "./synchronization";
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
