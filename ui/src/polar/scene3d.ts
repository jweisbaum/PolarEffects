/**
 * The three.js scene of the 3D polar view (spec.md 10.1, D16): every dot —
 * a track sample or a polar source's grid node — in one draw call, every
 * visible polar source a translucent surface with its grid lines, the blend
 * an opaque one. three.js is bundled by Vite from npm; nothing is fetched
 * (invariant 4).
 *
 * The dots are one `THREE.Points` over a single buffer (200,000 dots are one
 * draw, not 200,000), drawn by a small shader at a fixed pixel size in one of
 * three shapes: a disc, a ring (an excluded sample, drawn hollow) or a cross
 * (an excluded polar node) (spec.md 10.3). Selection rewrites one attribute
 * rather than rebuilding anything.
 *
 * The scene draws what it is given and knows nothing of sources, units or
 * IPC; `view3d.ts` turns the packed scene into its input.
 */
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

import { dotPositions, hatchLines, lassoSelect, project, surfaceMesh, type Layout, type PolarGrid } from "./geometry3d";

/** A disc: a sample or a node in the blend. */
export const SHAPE_DISC = 0;
/** A ring: an excluded sample (hollow). */
export const SHAPE_RING = 1;
/** A cross: an excluded polar node. */
export const SHAPE_CROSS = 2;
/** A square: a polar node whose cell holds an edit (spec.md 10.4). */
export const SHAPE_SQUARE = 3;

/** One surface. */
export interface SurfaceInput {
  grid: PolarGrid;
  /** `#rrggbb`, the source's stored colour. */
  color: string;
  /** The blend is opaque, and so is the source being edited; sources are translucent (spec.md 10.1). */
  opaque?: boolean;
  /** A translucent surface's opacity: 0.18 unless faded behind the source being edited (spec.md 10.4). */
  opacity?: number;
  /** Its grid lines' colour instead of one derived from `color`: the outline of a blend lost on the background. */
  lineColor?: string;
  /**
   * A colour per grid node, (r, g, b) in 0–1, indexed `j * ni + i`: the
   * Compare stage's difference surface (spec.md 11). `color` is then unused
   * for the faces.
   */
  vertexColors?: Float32Array;
  /** Nodes (indexed `j * ni + i`, non-zero marked) whose quads are hatched, in `hatchColor`. */
  hatched?: Uint8Array;
  hatchColor?: string;
}

/** What the scene draws. */
export interface SceneInput {
  /** (TWA, TWS, BSP) triples, one per dot. */
  samples: Float32Array;
  /** (r, g, b) in 0–1, one triple per dot: exactly as long as `samples`. */
  colors: Float32Array;
  /** One `SHAPE_*` per dot; all discs when absent. */
  shapes?: Float32Array;
  surfaces: readonly SurfaceInput[];
  layout: Layout;
}

/** How long the last `setData` took, milliseconds. */
export interface BuildTimes {
  dots: number;
  surfaces: number;
}

/**
 * The part of `THREE.WebGLRenderer` the scene uses, so a test can hand it a
 * stand-in where there is no WebGL.
 */
export interface RendererLike {
  setPixelRatio(ratio: number): void;
  getPixelRatio(): number;
  setSize(width: number, height: number, updateStyle?: boolean): void;
  render(scene: THREE.Object3D, camera: THREE.Camera): void;
  dispose(): void;
}

/** A camera placement. */
export interface View {
  position: readonly [number, number, number];
  target: readonly [number, number, number];
}

const VERTEX = `
attribute vec3 color;
attribute float selected;
attribute float shape;
uniform float size;
varying vec3 vColor;
varying float vShape;
void main() {
  vColor = mix(color, vec3(1.0), selected * 0.85);
  vShape = shape;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  gl_PointSize = size * (1.0 + selected) * (shape > 2.5 ? 1.6 : (shape > 1.5 ? 1.8 : (shape > 0.5 ? 1.4 : 1.0)));
}`;

const FRAGMENT = `
varying vec3 vColor;
varying float vShape;
void main() {
  vec2 d = gl_PointCoord - vec2(0.5);
  float r2 = dot(d, d);
  if (vShape > 2.5) {
    // A square with a light rim: an edited node.
    float edge = max(abs(d.x), abs(d.y));
    gl_FragColor = vec4(edge > 0.36 ? vec3(1.0) : vColor, 1.0);
    return;
  }
  if (r2 > 0.25) discard;
  if (vShape > 1.5) {
    if (min(abs(d.x - d.y), abs(d.x + d.y)) > 0.14) discard;
  } else if (vShape > 0.5) {
    if (r2 < 0.1) discard;
  }
  gl_FragColor = vec4(vColor, 1.0);
}`;

function disposeChildren(group: THREE.Group) {
  for (const child of [...group.children]) {
    group.remove(child);
    const drawn = child as THREE.Mesh | THREE.LineSegments;
    drawn.geometry.dispose();
    (drawn.material as THREE.Material).dispose();
  }
}

export class PolarScene {
  readonly renderer: RendererLike;
  readonly scene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(40, 1, 0.1, 2000);
  private controls: OrbitControls | null = null;
  private dots: THREE.Points | null = null;
  private positions = new Float32Array(0);
  private selected = new Float32Array(0);
  private readonly surfaces = new THREE.Group();
  private readonly guides = new THREE.Group();
  private screen = new Float32Array(0);
  private screenFresh = false;
  private readonly mvp = new THREE.Matrix4();
  private width = 1;
  private height = 1;
  private orbitAngle = Math.PI / 4;

  /** `renderer` is for tests; the app lets the scene make a WebGL one, which throws where WebGL is missing. */
  constructor(canvas: HTMLCanvasElement, renderer?: RendererLike) {
    this.renderer = renderer ?? new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio ?? 1, 2));
    this.camera.up.set(0, 0, 1);
    this.scene.add(this.surfaces);
    this.scene.add(this.guides);
    this.orbit(this.orbitAngle);
  }

  /** How many dots are drawn. */
  get dotCount(): number {
    return this.positions.length / 3;
  }

  /**
   * Replaces everything drawn. Refuses (with a `RangeError`, drawing
   * nothing new) input whose arrays disagree on the number of dots: a
   * colour array one triple short would otherwise colour every later dot
   * from its neighbour.
   */
  setData(input: SceneInput): BuildTimes {
    if (input.samples.length % 3 !== 0) {
      throw new RangeError(`${input.samples.length} dot coordinates are not whole (TWA, TWS, BSP) triples`);
    }
    const n = input.samples.length / 3;
    if (input.colors.length !== input.samples.length) {
      throw new RangeError(`${input.colors.length / 3} colours for ${n} dots`);
    }
    if (input.shapes && input.shapes.length !== n) {
      throw new RangeError(`${input.shapes.length} shapes for ${n} dots`);
    }
    const t0 = performance.now();
    this.positions = dotPositions(input.samples, input.layout);
    this.screenFresh = false;
    this.selected = new Float32Array(n);
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(this.positions, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(input.colors, 3));
    geometry.setAttribute("selected", new THREE.BufferAttribute(this.selected, 1));
    geometry.setAttribute("shape", new THREE.BufferAttribute(input.shapes ?? new Float32Array(n), 1));
    geometry.computeBoundingSphere();
    if (this.dots) {
      this.scene.remove(this.dots);
      this.dots.geometry.dispose();
      (this.dots.material as THREE.Material).dispose();
    }
    this.dots = new THREE.Points(geometry, new THREE.ShaderMaterial({
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: { size: { value: 4 * this.renderer.getPixelRatio() } },
    }));
    this.scene.add(this.dots);
    const t1 = performance.now();

    disposeChildren(this.surfaces);
    for (const surface of input.surfaces) {
      const mesh = surfaceMesh(surface.grid, input.layout);
      const shared = new THREE.BufferAttribute(mesh.positions, 3);
      const faces = new THREE.BufferGeometry();
      faces.setAttribute("position", shared);
      faces.setIndex(new THREE.BufferAttribute(mesh.triangles, 1));
      const lines = new THREE.BufferGeometry();
      lines.setAttribute("position", shared);
      lines.setIndex(new THREE.BufferAttribute(mesh.lines, 1));
      const color = new THREE.Color(surface.color);
      // Translucent and not writing depth, so surfaces behind show through
      // and the dots inside them stay visible. The blend is opaque.
      const opaque = surface.opaque === true;
      const opacity = surface.opacity ?? 0.18;
      const painted = surface.vertexColors !== undefined && surface.vertexColors.length === mesh.positions.length;
      if (painted) faces.setAttribute("color", new THREE.BufferAttribute(surface.vertexColors!, 3));
      this.surfaces.add(new THREE.Mesh(faces, new THREE.MeshBasicMaterial({
        color: painted ? new THREE.Color(0xffffff) : color, vertexColors: painted,
        transparent: !opaque, opacity: opaque ? 1 : opacity, side: THREE.DoubleSide, depthWrite: opaque,
      })));
      if (surface.hatched) {
        const hatch = hatchLines(surface.grid, surface.hatched);
        if (hatch.length > 0) {
          const crossing = new THREE.BufferGeometry();
          crossing.setAttribute("position", shared);
          crossing.setIndex(new THREE.BufferAttribute(hatch, 1));
          this.surfaces.add(new THREE.LineSegments(crossing, new THREE.LineBasicMaterial({
            color: new THREE.Color(surface.hatchColor ?? "#888888"), transparent: true, opacity: 0.9, depthWrite: false,
          })));
        }
      }
      this.surfaces.add(new THREE.LineSegments(lines, new THREE.LineBasicMaterial({
        // On an opaque surface the grid lines are lightened, or they vanish into it.
        color: surface.lineColor !== undefined ? new THREE.Color(surface.lineColor)
          : opaque ? color.clone().lerp(new THREE.Color(0xffffff), 0.6) : color, transparent: true,
        opacity: opaque ? 0.9 : Math.min(0.55, opacity * 3), depthWrite: false,
      })));
    }
    const t2 = performance.now();
    return { dots: t1 - t0, surfaces: t2 - t1 };
  }

  /** Axis guides: line segments as (x, y, z) pairs in model space. */
  setGuides(segments: Float32Array, color: string) {
    disposeChildren(this.guides);
    if (segments.length === 0) return;
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(segments, 3));
    this.guides.add(new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({
      color: new THREE.Color(color), transparent: true, opacity: 0.45, depthWrite: false,
    })));
  }

  /** The colour behind everything, `#rrggbb` (follows the theme). */
  setBackground(color: string) {
    this.scene.background = new THREE.Color(color);
  }

  /** Sizes the drawing buffer to the canvas's CSS size. */
  resize(width: number, height: number) {
    this.width = Math.max(1, width);
    this.height = Math.max(1, height);
    this.renderer.setSize(this.width, this.height, false);
    this.camera.aspect = this.width / this.height;
    this.camera.updateProjectionMatrix();
    this.screenFresh = false;
  }

  /**
   * Orbit, pan and zoom with the pointer (spec.md 10.1): left drag turns,
   * right drag pans, the wheel zooms. `onChange` runs after every move, so
   * the owner renders on demand rather than every frame.
   */
  enableControls(element: HTMLElement, onChange: () => void) {
    this.controls?.dispose();
    this.controls = new OrbitControls(this.camera, element);
    this.controls.enableDamping = false;
    this.controls.addEventListener("change", () => {
      this.screenFresh = false;
      onChange();
    });
  }

  /** Whether dragging turns the view; off while a lasso or box is drawn. */
  setRotateEnabled(enabled: boolean) {
    if (this.controls) this.controls.enableRotate = enabled;
  }

  /** Places the camera. */
  setView(view: View) {
    this.camera.position.set(...view.position);
    const target = new THREE.Vector3(...view.target);
    this.controls?.target.copy(target);
    this.camera.lookAt(target);
    this.camera.updateMatrixWorld();
    this.controls?.update();
    this.screenFresh = false;
  }

  /** Places the camera on a circle round the TWS axis, looking at mid-height (the benchmark's orbit). */
  orbit(angle: number) {
    this.orbitAngle = angle;
    const r = 45;
    this.setView({ position: [r * Math.cos(angle), r * Math.sin(angle), 32], target: [0, 0, 14] });
  }

  render() {
    this.renderer.render(this.scene, this.camera);
  }

  /**
   * Highlights exactly `indices`; everything else is unselected. An index
   * that is not a dot — past the end, negative, fractional — is ignored
   * rather than written past the buffer. Returns how many were ignored.
   */
  setSelection(indices: ArrayLike<number>): number {
    const n = this.selected.length;
    this.selected.fill(0);
    let ignored = 0;
    for (let k = 0; k < indices.length; k++) {
      const i = indices[k]!;
      if (Number.isInteger(i) && i >= 0 && i < n) this.selected[i] = 1;
      else ignored++;
    }
    const attribute = this.dots?.geometry.getAttribute("selected");
    if (attribute) attribute.needsUpdate = true;
    return ignored;
  }

  /** Every dot's screen position for the current camera, cached until it moves. */
  private projected(): Float32Array {
    if (!this.screenFresh) {
      this.camera.updateMatrixWorld();
      this.mvp.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
      this.screen = project(this.positions, this.mvp.elements, this.width, this.height, this.screen);
      this.screenFresh = true;
    }
    return this.screen.subarray(0, this.dotCount * 2);
  }

  /**
   * The dots inside a screen-space lasso (flat x, y pairs in CSS pixels
   * from the canvas's top left), for the current camera.
   */
  lasso(polygon: ArrayLike<number>): Uint32Array {
    return lassoSelect(this.projected(), polygon);
  }

  /** The dots inside a screen-space box between two corners. */
  box(x0: number, y0: number, x1: number, y1: number): Uint32Array {
    return this.lasso([x0, y0, x1, y0, x1, y1, x0, y1]);
  }

  /** The dot nearest a screen point within `radius` pixels, or -1. */
  pick(x: number, y: number, radius: number): number {
    const screen = this.projected();
    let best = -1;
    let bestDistance = radius * radius;
    for (let i = 0; i < screen.length / 2; i++) {
      const dx = screen[i * 2]! - x, dy = screen[i * 2 + 1]! - y;
      const d = dx * dx + dy * dy;
      // NaN (behind the camera) fails the comparison.
      if (d <= bestDistance) { best = i; bestDistance = d; }
    }
    return best;
  }

  /** A model-space point's screen position, or null when it is behind the camera. */
  toScreen(x: number, y: number, z: number): [number, number] | null {
    this.camera.updateMatrixWorld();
    this.mvp.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
    const out = project(new Float32Array([x, y, z]), this.mvp.elements, this.width, this.height);
    return Number.isNaN(out[0]) ? null : [out[0]!, out[1]!];
  }

  /** Frees every GPU resource: geometries, materials, controls and the renderer. */
  dispose() {
    this.controls?.dispose();
    this.controls = null;
    if (this.dots) {
      this.scene.remove(this.dots);
      this.dots.geometry.dispose();
      (this.dots.material as THREE.Material).dispose();
      this.dots = null;
    }
    disposeChildren(this.surfaces);
    disposeChildren(this.guides);
    this.positions = new Float32Array(0);
    this.selected = new Float32Array(0);
    this.renderer.dispose();
  }
}
