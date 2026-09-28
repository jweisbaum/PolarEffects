import { describe, expect, it } from "vitest";

import { copiesInView, graticule } from "./renderer";

describe("the flat map's copies", () => {
  const view = { width: 800, height: 400 };

  it("needs one copy when the world fits around the centre", () => {
    expect(copiesInView({ lon: 0, lat: 0, scale: 3 }, view)).toEqual([0]);
  });

  it("adds the copy across the antimeridian when the view reaches it", () => {
    expect(copiesInView({ lon: 170, lat: 0, scale: 10 }, view)).toEqual([0, 360]);
    expect(copiesInView({ lon: -170, lat: 0, scale: 10 }, view)).toEqual([-360, 0]);
  });

  it("covers a view wider than the world", () => {
    expect(copiesInView({ lon: 0, lat: 0, scale: 1 }, { width: 1000, height: 400 })).toEqual([-360, 0, 360]);
  });
});

describe("the graticule", () => {
  it("is line segments on whole degrees, every 30°", () => {
    const lines = graticule();
    expect(lines.length % 4).toBe(0);
    const lons = new Set<number>();
    for (let i = 0; i < lines.length; i += 4) if (lines[i] === lines[i + 2]) lons.add(lines[i]!);
    expect([...lons].sort((a, b) => a - b)).toEqual([-180, -150, -120, -90, -60, -30, 0, 30, 60, 90, 120, 150]);
  });
});

describe("disposing the renderer", () => {
  /** A WebGL2 context that only counts what is made and what is freed. */
  function fakeGl() {
    const made = { buffer: 0, vao: 0, texture: 0, program: 0 };
    const freed = { buffer: 0, vao: 0, texture: 0, program: 0 };
    let lost = false;
    const gl = new Proxy({}, {
      get: (_target, name: string) => {
        if (name === "createBuffer") return () => (made.buffer++, {});
        if (name === "createVertexArray") return () => (made.vao++, {});
        if (name === "createTexture") return () => (made.texture++, {});
        if (name === "createProgram") return () => (made.program++, {});
        if (name === "deleteBuffer") return () => freed.buffer++;
        if (name === "deleteVertexArray") return () => freed.vao++;
        if (name === "deleteTexture") return () => freed.texture++;
        if (name === "deleteProgram") return () => freed.program++;
        if (name === "getExtension") return () => ({ loseContext: () => { lost = true; } });
        if (name === "getParameter") return () => 4096;
        if (/^(get(Shader|Program)Parameter)$/.test(name)) return () => true;
        if (/^(create|get)/.test(name)) return () => ({});
        if (/^[A-Z_0-9]+$/.test(name)) return 0;
        return () => undefined;
      },
    }) as WebGL2RenderingContext;
    return { gl, made, freed, lost: () => lost };
  }

  it("frees every buffer, array, texture and program it made, and gives the context back", async () => {
    const { MapRenderer } = await import("./renderer");
    const lod = (marker: number) => ({
      marker, triVertices: new Float32Array([0, 0, 1, 0, 0, 1]), triIndices: new Uint32Array([0, 1, 2]),
      lineVertices: new Float32Array([0, 0, 1, 0]), lineIndices: new Uint32Array([0, 1]),
    });
    const fake = fakeGl();
    const renderer = new MapRenderer(fake.gl, { version: 1, lods: [lod(110), lod(50)] });
    expect(fake.made.buffer).toBeGreaterThan(0);
    renderer.dispose();
    expect(fake.freed).toEqual(fake.made);
    expect(fake.lost()).toBe(true);
  });
});
