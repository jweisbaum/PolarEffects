/**
 * The scene's own checks, run in Node with a stand-in renderer (there is no
 * WebGL here): refusing input whose arrays disagree, ignoring selection
 * indices that are not dots, picking by screen position, and freeing
 * everything on dispose.
 */
import * as THREE from "three";
import { describe, expect, it, vi } from "vitest";

import { PolarScene, SHAPE_CROSS, type RendererLike } from "./scene3d";
import { syntheticGrid } from "./geometry3d";

function fakeRenderer(): RendererLike & { dispose: ReturnType<typeof vi.fn> } {
  return { setPixelRatio: () => undefined, getPixelRatio: () => 1, setSize: () => undefined, render: () => undefined, dispose: vi.fn() };
}

function make() {
  const renderer = fakeRenderer();
  const scene = new PolarScene({} as HTMLCanvasElement, renderer);
  scene.resize(200, 200);
  return { scene, renderer };
}

const three = (n: number) => new Float32Array(n * 3);

function points(scene: PolarScene): THREE.Points | undefined {
  return scene.scene.children.find((child): child is THREE.Points => child instanceof THREE.Points);
}

describe("setData", () => {
  it("draws one dot per triple, with its shape", () => {
    const { scene } = make();
    scene.setData({
      samples: Float32Array.from([90, 10, 5, 45, 12, 6]), colors: three(2), shapes: Float32Array.from([0, SHAPE_CROSS]),
      surfaces: [{ grid: syntheticGrid(1), color: "#4e79a7" }, { grid: syntheticGrid(1.1), color: "#ffffff", opaque: true }],
      layout: "tower",
    });
    expect(scene.dotCount).toBe(2);
    expect(points(scene)!.geometry.getAttribute("shape").array[1]).toBe(SHAPE_CROSS);
  });

  it("refuses colours, shapes or coordinates that do not match the dots", () => {
    const { scene } = make();
    const base = { samples: three(3), colors: three(3), surfaces: [], layout: "tower" as const };
    expect(() => scene.setData({ ...base, colors: three(2) })).toThrow(RangeError);
    expect(() => scene.setData({ ...base, colors: three(4) })).toThrow(/4 colours for 3 dots/);
    expect(() => scene.setData({ ...base, shapes: new Float32Array(2) })).toThrow(/2 shapes for 3 dots/);
    expect(() => scene.setData({ ...base, samples: new Float32Array(7), colors: new Float32Array(7) })).toThrow(/whole/);
    // A refusal leaves what was drawn alone.
    expect(scene.dotCount).toBe(0);
    scene.setData(base);
    expect(scene.dotCount).toBe(3);
  });
});

describe("setSelection", () => {
  it("highlights exactly the given dots and ignores anything that is not one", () => {
    const { scene } = make();
    scene.setData({ samples: three(3), colors: three(3), surfaces: [], layout: "tower" });
    expect(scene.setSelection([0, 2, 3, -1, 1.5, 99, Number.NaN])).toBe(5);
    expect([...points(scene)!.geometry.getAttribute("selected").array]).toEqual([1, 0, 1]);
    expect(scene.setSelection([1])).toBe(0);
    expect([...points(scene)!.geometry.getAttribute("selected").array]).toEqual([0, 1, 0]);
  });
});

describe("picking", () => {
  it("finds the dot under the pointer from above, and dots inside a box", () => {
    const { scene } = make();
    // Beam reach at 10 kn BSP sits at x = 10; dead upwind 0 BSP at the origin.
    scene.setData({ samples: Float32Array.from([90, 10, 10, 0, 10, 0]), colors: three(2), surfaces: [], layout: "tower" });
    scene.setView({ position: [0, -0.001, 60], target: [0, 0, 10] });
    const origin = scene.toScreen(0, 0, 10)!;
    expect(origin[0]).toBeCloseTo(100, 3);
    expect(origin[1]).toBeCloseTo(100, 3);
    expect(scene.pick(100, 100, 5)).toBe(1);
    const beam = scene.toScreen(10, 0, 10)!;
    expect(beam[0]).toBeGreaterThan(110);
    expect(scene.pick(beam[0], beam[1], 5)).toBe(0);
    expect(scene.pick(5, 5, 5)).toBe(-1);
    expect([...scene.box(90, 90, 110, 110)]).toEqual([1]);
    expect([...scene.box(0, 0, 200, 200)]).toEqual([0, 1]);
  });
});

describe("dispose", () => {
  it("frees the geometries and the renderer", () => {
    const { scene, renderer } = make();
    scene.setData({ samples: three(2), colors: three(2), surfaces: [{ grid: syntheticGrid(1), color: "#4e79a7" }], layout: "tower" });
    scene.setGuides(Float32Array.from([0, 0, 0, 1, 1, 1]), "#ffffff");
    const geometry = points(scene)!.geometry;
    const disposed = vi.spyOn(geometry, "dispose");
    scene.dispose();
    expect(disposed).toHaveBeenCalled();
    expect(renderer.dispose).toHaveBeenCalledTimes(1);
    expect(points(scene)).toBeUndefined();
    expect(scene.scene.children.every((child) => child.children.length === 0)).toBe(true);
  });
});
