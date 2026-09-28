/**
 * The world map's WebGL2 renderer (spec.md 9.1): VectorEffects' basemap —
 * Natural Earth land and coastlines, embedded, no tiles — in the two
 * projections PolarEffects offers.
 *
 * Draw order, bottom to top: sea, land, graticule, coastlines. Later
 * milestones draw tracks above the coastlines.
 *
 * The flat map draws the land triangles directly, once per 360° copy that is
 * in view, so panning across the antimeridian is seamless. The globe cannot:
 * a land triangle that reaches round to the far side would fold back across
 * the near one. So the land is drawn once into an equirectangular mask
 * texture, and the globe is a full-screen pass that inverts the orthographic
 * projection per pixel and looks the mask up (VectorEffects draws its globe
 * base the same way, through cached equirectangular tiles). Coastline and
 * graticule segments are short enough to project per vertex and cut at the
 * horizon per fragment.
 *
 * The projection formulas here are the GLSL twins of `projection.ts`, which
 * is where they are tested.
 */

import type { Basemap } from "./format";
import type { Camera, ProjectionId, Viewport } from "./projection";

/** The colours a frame is drawn in, as 0–1 RGBA. */
export interface MapColours {
  sea: [number, number, number, number];
  land: [number, number, number, number];
  coast: [number, number, number, number];
  graticule: [number, number, number, number];
  void: [number, number, number, number];
}

const GEO_VERT = `#version 300 es
precision highp float;
layout(location = 0) in vec2 aLonLat;
uniform vec3 uCamera;     // centre lon, centre lat, pixels per degree
uniform vec2 uViewport;   // CSS pixels
uniform int uProjection;  // 0 flat, 1 globe, 2 the equirectangular land mask
uniform float uLonOffset; // which 360-degree copy of the flat map
out float vHorizon;
const float DEG = 0.017453292519943295;
void main() {
  vHorizon = 1.0;
  if (uProjection == 2) {
    gl_Position = vec4(aLonLat.x / 180.0, aLonLat.y / 90.0, 0.0, 1.0);
    return;
  }
  vec2 screen;
  if (uProjection == 0) {
    screen = vec2(
      uViewport.x * 0.5 + (aLonLat.x + uLonOffset - uCamera.x) * uCamera.z,
      uViewport.y * 0.5 - (aLonLat.y - uCamera.y) * uCamera.z);
  } else {
    float lam = (aLonLat.x - uCamera.x) * DEG;
    float phi = aLonLat.y * DEG;
    float phi0 = uCamera.y * DEG;
    vHorizon = sin(phi0) * sin(phi) + cos(phi0) * cos(phi) * cos(lam);
    float r = uCamera.z / DEG;
    screen = vec2(
      uViewport.x * 0.5 + r * cos(phi) * sin(lam),
      uViewport.y * 0.5 - r * (cos(phi0) * sin(phi) - sin(phi0) * cos(phi) * cos(lam)));
  }
  gl_Position = vec4(screen.x / uViewport.x * 2.0 - 1.0, 1.0 - screen.y / uViewport.y * 2.0, 0.0, 1.0);
}`;

const GEO_FRAG = `#version 300 es
precision highp float;
uniform vec4 uColor;
in float vHorizon;
out vec4 outColor;
void main() {
  if (vHorizon < 0.0) discard;
  outColor = uColor;
}`;

const GLOBE_VERT = `#version 300 es
precision highp float;
layout(location = 0) in vec2 aClip;
out vec2 vClip;
void main() {
  vClip = aClip;
  gl_Position = vec4(aClip, 0.0, 1.0);
}`;

const GLOBE_FRAG = `#version 300 es
precision highp float;
uniform vec3 uCamera;
uniform vec2 uViewport;
uniform sampler2D uMask;
uniform vec4 uSea;
uniform vec4 uLand;
uniform vec4 uVoid;
in vec2 vClip;
out vec4 outColor;
const float DEG = 0.017453292519943295;
void main() {
  float dx = vClip.x * uViewport.x * 0.5;
  float dy = vClip.y * uViewport.y * 0.5;
  float r = uCamera.z / DEG;
  float rho = length(vec2(dx, dy));
  float inside = clamp(r - rho + 0.5, 0.0, 1.0);
  if (inside <= 0.0) { outColor = uVoid; return; }
  float c = asin(min(1.0, rho / r));
  float phi0 = uCamera.y * DEG;
  float lat = asin(clamp(cos(c) * sin(phi0) + (rho > 0.0 ? dy * sin(c) * cos(phi0) / rho : 0.0), -1.0, 1.0));
  float lon = uCamera.x * DEG + atan(dx * sin(c), rho * cos(phi0) * cos(c) - dy * sin(phi0) * sin(c));
  vec2 uv = vec2(fract((lon / DEG + 180.0) / 360.0), (lat / DEG + 90.0) / 180.0);
  float land = texture(uMask, uv).r;
  outColor = mix(uVoid, mix(uSea, uLand, land), inside);
}`;

interface Program {
  program: WebGLProgram;
  uniforms: Record<string, WebGLUniformLocation | null>;
}

interface Geo {
  vao: WebGLVertexArrayObject;
  count: number;
  indexed: boolean;
}

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("could not create a shader");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(`shader failed to compile: ${gl.getShaderInfoLog(shader) ?? ""}`);
  }
  return shader;
}

function link(gl: WebGL2RenderingContext, vert: string, frag: string, names: string[]): Program {
  const program = gl.createProgram();
  if (!program) throw new Error("could not create a program");
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vert));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, frag));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`program failed to link: ${gl.getProgramInfoLog(program) ?? ""}`);
  }
  return { program, uniforms: Object.fromEntries(names.map(name => [name, gl.getUniformLocation(program, name)])) };
}

/** Meridians and parallels every 30°, as line segments of 2° each. */
export function graticule(stepDeg = 30, segmentDeg = 2): Float32Array {
  const out: number[] = [];
  for (let lon = -180; lon < 180; lon += stepDeg) {
    for (let lat = -90; lat < 90; lat += segmentDeg) out.push(lon, lat, lon, Math.min(90, lat + segmentDeg));
  }
  for (let lat = -90 + stepDeg; lat < 90; lat += stepDeg) {
    for (let lon = -180; lon < 180; lon += segmentDeg) out.push(lon, lat, lon + segmentDeg, lat);
  }
  return new Float32Array(out);
}

/** Which 360° copies of the flat map a view needs, as longitude offsets. */
export function copiesInView(camera: Camera, view: Viewport): number[] {
  const half = view.width / 2 / camera.scale;
  const first = Math.floor((camera.lon - half + 180) / 360);
  const last = Math.floor((camera.lon + half + 180) / 360);
  const offsets: number[] = [];
  for (let k = first; k <= last; k += 1) offsets.push(k * 360);
  return offsets;
}

/** The coarse level when zoomed out, where the detailed one is wasted. */
const DETAIL_SCALE = 6;

export class MapRenderer {
  private readonly gl: WebGL2RenderingContext;
  private readonly geo: Program;
  private readonly globe: Program;
  private readonly land = new Map<number, Geo>();
  private readonly coast = new Map<number, Geo>();
  private readonly sea: Geo;
  private readonly grid: Geo;
  private readonly quad: Geo;
  private readonly mask: WebGLTexture;
  private readonly markers: number[];

  constructor(gl: WebGL2RenderingContext, basemap: Basemap) {
    this.gl = gl;
    this.geo = link(gl, GEO_VERT, GEO_FRAG, ["uCamera", "uViewport", "uProjection", "uLonOffset", "uColor"]);
    this.globe = link(gl, GLOBE_VERT, GLOBE_FRAG, ["uCamera", "uViewport", "uMask", "uSea", "uLand", "uVoid"]);
    for (const lod of basemap.lods) {
      this.land.set(lod.marker, this.buffers(lod.triVertices, lod.triIndices));
      this.coast.set(lod.marker, this.buffers(lod.lineVertices, lod.lineIndices));
    }
    this.markers = basemap.lods.map(lod => lod.marker);
    this.sea = this.buffers(new Float32Array([-180, -90, 180, -90, -180, 90, -180, 90, 180, -90, 180, 90]));
    this.grid = this.buffers(graticule());
    this.quad = this.buffers(new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]));
    this.mask = this.landMask();
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  private buffers(vertices: Float32Array, indices?: Uint32Array): Geo {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    const vbo = gl.createBuffer();
    if (!vao || !vbo) throw new Error("could not allocate map buffers");
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    if (indices) {
      const ibo = gl.createBuffer();
      if (!ibo) throw new Error("could not allocate map buffers");
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ibo);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
    }
    gl.bindVertexArray(null);
    return { vao, count: indices ? indices.length : vertices.length / 2, indexed: indices !== undefined };
  }

  private draw(geo: Geo, mode: number) {
    const gl = this.gl;
    gl.bindVertexArray(geo.vao);
    if (geo.indexed) gl.drawElements(mode, geo.count, gl.UNSIGNED_INT, 0);
    else gl.drawArrays(mode, 0, geo.count);
  }

  /** The finest land, drawn once into an equirectangular mask for the globe. */
  private landMask(): WebGLTexture {
    const gl = this.gl;
    const width = Math.min(4096, gl.getParameter(gl.MAX_TEXTURE_SIZE) as number);
    const height = width / 2;
    const texture = gl.createTexture();
    const framebuffer = gl.createFramebuffer();
    if (!texture || !framebuffer) throw new Error("could not allocate the land mask");
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    gl.viewport(0, 0, width, height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(this.geo.program);
    gl.uniform1i(this.geo.uniforms.uProjection ?? null, 2);
    gl.uniform4f(this.geo.uniforms.uColor ?? null, 1, 1, 1, 1);
    const finest = this.land.get(this.markers[this.markers.length - 1] ?? 50);
    if (finest) this.draw(finest, gl.TRIANGLES);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.deleteFramebuffer(framebuffer);
    return texture;
  }

  /** Draws one frame. `view` is in CSS pixels; `pixelRatio` maps it to the canvas. */
  render(projection: ProjectionId, camera: Camera, view: Viewport, pixelRatio: number, colours: MapColours): void {
    const gl = this.gl;
    gl.viewport(0, 0, Math.round(view.width * pixelRatio), Math.round(view.height * pixelRatio));
    gl.clearColor(...colours.void);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const lod = camera.scale >= DETAIL_SCALE ? (this.markers[this.markers.length - 1] ?? 50) : (this.markers[0] ?? 110);
    const land = this.land.get(lod);
    const coast = this.coast.get(lod);

    const geo = this.geo.uniforms;
    const useGeo = (mode: number) => {
      gl.useProgram(this.geo.program);
      gl.uniform3f(geo.uCamera ?? null, camera.lon, camera.lat, camera.scale);
      gl.uniform2f(geo.uViewport ?? null, view.width, view.height);
      gl.uniform1i(geo.uProjection ?? null, mode);
      gl.uniform1f(geo.uLonOffset ?? null, 0);
    };
    const colour = (rgba: [number, number, number, number]) => gl.uniform4f(geo.uColor ?? null, ...rgba);

    if (projection === "equirectangular") {
      useGeo(0);
      for (const offset of copiesInView(camera, view)) {
        gl.uniform1f(geo.uLonOffset ?? null, offset);
        colour(colours.sea);
        this.draw(this.sea, gl.TRIANGLES);
        colour(colours.land);
        if (land) this.draw(land, gl.TRIANGLES);
        colour(colours.graticule);
        this.draw(this.grid, gl.LINES);
        colour(colours.coast);
        if (coast) this.draw(coast, gl.LINES);
      }
      return;
    }

    gl.useProgram(this.globe.program);
    const globe = this.globe.uniforms;
    gl.uniform3f(globe.uCamera ?? null, camera.lon, camera.lat, camera.scale);
    gl.uniform2f(globe.uViewport ?? null, view.width, view.height);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.mask);
    gl.uniform1i(globe.uMask ?? null, 0);
    gl.uniform4f(globe.uSea ?? null, ...colours.sea);
    gl.uniform4f(globe.uLand ?? null, ...colours.land);
    gl.uniform4f(globe.uVoid ?? null, ...colours.void);
    this.draw(this.quad, gl.TRIANGLES);

    useGeo(1);
    colour(colours.graticule);
    this.draw(this.grid, gl.LINES);
    colour(colours.coast);
    if (coast) this.draw(coast, gl.LINES);
  }
}
