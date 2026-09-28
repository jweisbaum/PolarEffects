/**
 * The three.js scene of the 3D polar view (spec.md 10.1, D16): every sample
 * a dot in its track's colour, every visible polar source a translucent
 * surface with its grid lines. three.js is bundled by Vite from npm; nothing
 * is fetched (invariant 4).
 *
 * The dots are one `THREE.Points` draw call over a single buffer (200,000
 * dots are one draw, not 200,000), drawn round by a small shader at a fixed
 * pixel size. Selection rewrites one attribute rather than rebuilding
 * anything.
 *
 * M3 spike: the structure M7 builds on. Orbit controls, preset cameras,
 * hollow excluded dots and the blend surface come with M7.
 */
import * as THREE from "three";
import { dotPositions, lassoSelect, project, surfaceMesh, type Layout, type PolarGrid } from "./geometry3d";

/** One polar source drawn as a surface. */
export interface SurfaceInput {
  grid: PolarGrid;
  /** `#rrggbb`, the source's stored colour. */
  color: string;
}

/** What the scene draws. */
export interface SceneInput {
  /** (TWA, TWS, BSP) triples, one per dot. */
  samples: Float32Array;
  /** (r, g, b) in 0–1, one per dot. */
  colors: Float32Array;
  surfaces: readonly SurfaceInput[];
  layout: Layout;
}

/** How long the last `setData` took, milliseconds. */
export interface BuildTimes {
  dots: number;
  surfaces: number;
}

const VERTEX = `
attribute vec3 color;
attribute float selected;
uniform float size;
varying vec3 vColor;
void main() {
  vColor = mix(color, vec3(1.0), selected * 0.85);
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  gl_PointSize = size * (1.0 + selected);
}`;

const FRAGMENT = `
varying vec3 vColor;
void main() {
  vec2 d = gl_PointCoord - vec2(0.5);
  if (dot(d, d) > 0.25) discard;
  gl_FragColor = vec4(vColor, 1.0);
}`;

export class PolarScene {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(40, 1, 0.1, 500);
  private dots: THREE.Points | null = null;
  private positions = new Float32Array(0);
  private selected = new Float32Array(0);
  private readonly surfaces = new THREE.Group();
  private screen = new Float32Array(0);
  private readonly mvp = new THREE.Matrix4();
  private width = 1;
  private height = 1;
  private orbitAngle = Math.PI / 4;

  constructor(canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio ?? 1, 2));
    this.camera.up.set(0, 0, 1);
    this.scene.add(this.surfaces);
    this.orbit(this.orbitAngle);
  }

  /** Replaces everything drawn. */
  setData(input: SceneInput): BuildTimes {
    const t0 = performance.now();
    this.positions = dotPositions(input.samples, input.layout);
    const n = input.samples.length / 3;
    this.selected = new Float32Array(n);
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(this.positions, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(input.colors, 3));
    geometry.setAttribute("selected", new THREE.BufferAttribute(this.selected, 1));
    geometry.computeBoundingSphere();
    if (this.dots) {
      this.scene.remove(this.dots);
      this.dots.geometry.dispose();
      (this.dots.material as THREE.Material).dispose();
    }
    this.dots = new THREE.Points(geometry, new THREE.ShaderMaterial({
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: { size: { value: 3 * this.renderer.getPixelRatio() } },
    }));
    this.scene.add(this.dots);
    const t1 = performance.now();

    for (const child of [...this.surfaces.children]) {
      this.surfaces.remove(child);
      const drawn = child as THREE.Mesh | THREE.LineSegments;
      drawn.geometry.dispose();
      (drawn.material as THREE.Material).dispose();
    }
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
      // and the dots inside them stay visible.
      this.surfaces.add(new THREE.Mesh(faces, new THREE.MeshBasicMaterial({
        color, transparent: true, opacity: 0.18, side: THREE.DoubleSide, depthWrite: false,
      })));
      this.surfaces.add(new THREE.LineSegments(lines, new THREE.LineBasicMaterial({
        color, transparent: true, opacity: 0.55, depthWrite: false,
      })));
    }
    const t2 = performance.now();
    return { dots: t1 - t0, surfaces: t2 - t1 };
  }

  /** Sizes the drawing buffer to the canvas's CSS size. */
  resize(width: number, height: number) {
    this.width = Math.max(1, width);
    this.height = Math.max(1, height);
    this.renderer.setSize(this.width, this.height, false);
    this.camera.aspect = this.width / this.height;
    this.camera.updateProjectionMatrix();
  }

  /** Places the camera on a circle round the TWS axis, looking at mid-height. */
  orbit(angle: number) {
    this.orbitAngle = angle;
    const r = 45;
    this.camera.position.set(r * Math.cos(angle), r * Math.sin(angle), 32);
    this.camera.lookAt(0, 0, 14);
    this.camera.updateMatrixWorld();
  }

  render() {
    this.renderer.render(this.scene, this.camera);
  }

  /** Highlights exactly `indices`; everything else is unselected. */
  setSelection(indices: Uint32Array) {
    this.selected.fill(0);
    for (const i of indices) this.selected[i] = 1;
    const attribute = this.dots?.geometry.getAttribute("selected");
    if (attribute) attribute.needsUpdate = true;
  }

  /**
   * The dots inside a screen-space lasso (flat x, y pairs in CSS pixels
   * from the canvas's top left), for the current camera.
   */
  lasso(polygon: ArrayLike<number>): Uint32Array {
    this.camera.updateMatrixWorld();
    this.mvp.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
    this.screen = project(this.positions, this.mvp.elements, this.width, this.height, this.screen);
    return lassoSelect(this.screen.subarray(0, (this.positions.length / 3) * 2), polygon);
  }

  dispose() {
    this.setData({ samples: new Float32Array(0), colors: new Float32Array(0), surfaces: [], layout: "tower" });
    this.renderer.dispose();
  }
}
