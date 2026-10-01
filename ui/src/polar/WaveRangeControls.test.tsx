// @vitest-environment happy-dom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import WaveRangeControls from "./WaveRangeControls";
import { packSynthetic } from "./benchPacket";
import { unpackScene } from "./scenePacket";
import { NO_WAVE_RANGES, type WaveRanges } from "./waveRanges";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("converts slider edits from feet to metres and preserves physical bounds when units change", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const { samples } = unpackScene(packSynthetic(Float32Array.from([90, 10, 6])));
  samples.hs[0] = 3;
  let ranges: WaveRanges = { ...NO_WAVE_RANGES, hs: { min: 1, max: 2 } };
  const changed = vi.fn((value: WaveRanges) => { ranges = value; });
  const render = (wave_height: "m" | "ft") => act(async () => root.render(<WaveRangeControls samples={samples} ranges={ranges} onChange={changed} count={1} units={{ speed: "kn", wave_height, distance: "nm" }} />));
  try {
    await render("ft");
    const lower = host.querySelector<HTMLInputElement>('[data-feature="view3d:wave-height-min"]')!;
    expect(lower.getAttribute("aria-valuetext")).toBe("3.3 ft");
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(lower, "4");
      lower.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(ranges.hs.min).toBeCloseTo(1.2192, 10);
    expect(ranges.hs.max).toBe(2);
    await render("m");
    expect(lower.getAttribute("aria-valuetext")).toBe("1.2 m");
    await render("ft");
    expect(lower.getAttribute("aria-valuetext")).toBe("4.0 ft");
    expect(changed).toHaveBeenCalledTimes(1);
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});

it("drags both bounds without jumping on press, clamps without shrinking, and stops on release or cancellation", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const { samples } = unpackScene(packSynthetic(Float32Array.from([90, 10, 6])));
  samples.hs[0] = 3;
  let latest: WaveRanges = { ...NO_WAVE_RANGES, hs: { min: 1, max: 2 } };
  const changed = vi.fn();
  function Controls() {
    const [ranges, setRanges] = useState(latest);
    return <WaveRangeControls samples={samples} ranges={ranges} onChange={value => {
      latest = value; changed(value); setRanges(value);
    }} count={1} units={{ speed: "kn", wave_height: "m", distance: "nm" }} />;
  }
  try {
    await act(async () => root.render(<Controls />));
    const middle = host.querySelector<HTMLButtonElement>('[data-feature="view3d:wave-height-move"]')!;
    middle.setPointerCapture = vi.fn();
    middle.hasPointerCapture = vi.fn(() => true);
    middle.releasePointerCapture = vi.fn();
    vi.spyOn(middle.parentElement!, "getBoundingClientRect").mockReturnValue({ width: 300 } as DOMRect);
    const pointer = async (type: string, clientX: number, pointerId = 1) => act(async () => {
      middle.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerId, clientX }));
    });
    await pointer("pointerdown", 100);
    expect(changed).not.toHaveBeenCalled();
    expect(middle.setPointerCapture).toHaveBeenCalledWith(1);
    await pointer("pointermove", 160, 2); // A second finger does not take over.
    expect(changed).not.toHaveBeenCalled();
    await pointer("pointermove", 160); // 60 / 300 of a 3 m scale = 0.6 m.
    expect(latest.hs.min).toBeCloseTo(1.6);
    expect(latest.hs.max).toBeCloseTo(2.6);
    await pointer("pointermove", 1000);
    expect(latest.hs).toEqual({ min: 2, max: null }); // 2–3 m, still 1 m wide.
    await pointer("pointermove", -1000);
    expect(latest.hs).toEqual({ min: null, max: 1 }); // 0–1 m, still 1 m wide.
    await pointer("pointerup", 100);
    expect(latest.hs).toEqual({ min: 1, max: 2 });
    expect(middle.releasePointerCapture).toHaveBeenCalledWith(1);
    await pointer("pointermove", 160);
    expect(latest.hs).toEqual({ min: 1, max: 2 });
    await pointer("pointerdown", 100);
    await pointer("pointercancel", 100);
    await pointer("pointermove", 160);
    expect(latest.hs).toEqual({ min: 1, max: 2 });
    expect(latest.waveAngle).toEqual(NO_WAVE_RANGES.waveAngle);
    expect(latest.wavePeriod).toEqual(NO_WAVE_RANGES.wavePeriod);
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});

it("moves the range with keyboard steps in display units, preserving its width at either limit", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const { samples } = unpackScene(packSynthetic(Float32Array.from([90, 10, 6])));
  samples.hs[0] = 3; // Display ceiling: 9.9 ft.
  let latest: WaveRanges = { ...NO_WAVE_RANGES, hs: { min: 1.2192, max: 1.8288 } }; // 4–6 ft.
  function Controls() {
    const [ranges, setRanges] = useState(latest);
    return <WaveRangeControls samples={samples} ranges={ranges} onChange={value => {
      latest = value; setRanges(value);
    }} count={1} units={{ speed: "kn", wave_height: "ft", distance: "nm" }} />;
  }
  try {
    await act(async () => root.render(<Controls />));
    const middle = host.querySelector<HTMLButtonElement>('[data-feature="view3d:wave-height-move"]')!;
    const key = async (key: string) => act(async () => {
      middle.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key }));
    });
    await key("ArrowRight"); // Both limits move 0.1 ft (0.03048 m).
    expect(latest.hs.min).toBeCloseTo(1.24968);
    expect(latest.hs.max).toBeCloseTo(1.85928);
    await key("Home");
    expect(latest.hs.min).toBeNull();
    expect(latest.hs.max).toBeCloseTo(0.6096);
    await key("ArrowLeft");
    expect(latest.hs.max).toBeCloseTo(0.6096);
    await key("End");
    expect(latest.hs.min).toBeCloseTo(2.40792); // 7.9–9.9 ft.
    expect(latest.hs.max).toBeNull();
    await key("ArrowLeft");
    expect(latest.hs.min).toBeCloseTo(2.37744);
    expect(latest.hs.max).toBeCloseTo(2.98704);
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});

it("keeps a saved range's scale stable while dragging above the remaining samples", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const { samples } = unpackScene(packSynthetic(Float32Array.from([90, 10, 6])));
  samples.hs[0] = 3;
  let latest: WaveRanges = { ...NO_WAVE_RANGES, hs: { min: 2, max: 4 } };
  function Controls() {
    const [ranges, setRanges] = useState(latest);
    return <WaveRangeControls samples={samples} ranges={ranges} onChange={value => {
      latest = value; setRanges(value);
    }} count={1} units={{ speed: "kn", wave_height: "m", distance: "nm" }} />;
  }
  try {
    await act(async () => root.render(<Controls />));
    const middle = host.querySelector<HTMLButtonElement>('[data-feature="view3d:wave-height-move"]')!;
    middle.setPointerCapture = vi.fn();
    vi.spyOn(middle.parentElement!, "getBoundingClientRect").mockReturnValue({ width: 400 } as DOMRect);
    const pointer = async (type: string, clientX: number) => act(async () => {
      middle.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerId: 1, clientX }));
    });
    await pointer("pointerdown", 200);
    await pointer("pointermove", 100); // 1 m left: 1–3 m.
    expect(latest.hs).toEqual({ min: 1, max: 3 });
    expect(host.querySelector<HTMLInputElement>('input[type="range"]')!.max).toBe("4");
    await pointer("pointermove", 150); // Reverse half a metre: 1.5–3.5 m.
    expect(latest.hs).toEqual({ min: 1.5, max: 3.5 });
    await pointer("pointerup", 150);
    expect(latest.hs).toEqual({ min: 1.5, max: 3.5 });
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});
