import {
  BufferAttribute, BufferGeometry, CapsuleGeometry, Color, ConeGeometry, CustomBlending, CylinderGeometry, EdgesGeometry,
  InstancedBufferAttribute, InstancedBufferGeometry, OneFactor, OneMinusSrcAlphaFactor, ShaderMaterial, SphereGeometry,
  TorusGeometry, BoxGeometry, Vector3, ZeroFactor, FrontSide, BackSide, AddEquation, type Side,
} from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { ConstructSpec, Part } from '../sim/constructs/spec.js';
import { partTransform } from '../sim/constructs/geom.js';
import { NOISE_GLSL } from './glsl.js';

// ------------------------------------------------------------------------------------------------
// Geometry: one merged geometry per construct spec. Parts use partTransform() (shared with physics)
// so what you see is what collides. Attribute aPart = (part centre xyz, assembly order 0..1).
// ------------------------------------------------------------------------------------------------

function wedgeGeometry(s: Vector3): BufferGeometry {
  // rises toward +z exactly like the runtime's convex hull
  const x = s.x / 2, y = s.y / 2, z = s.z / 2;
  const v = [
    [-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z], // bottom
    [-x, -y, -z], [x, -y, -z], [x, y, z], [-x, y, z], // slope
    [-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z], // back
    [-x, -y, -z], [-x, -y, z], [-x, y, z], // left
    [x, -y, -z], [x, y, z], [x, -y, z], // right
  ];
  const idx = [0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 8, 9, 10, 8, 10, 11, 12, 13, 14, 15, 16, 17];
  const g = new BufferGeometry();
  g.setAttribute('position', new BufferAttribute(new Float32Array(v.flat()), 3));
  g.setIndex(idx);
  const ng = g.toNonIndexed();
  ng.computeVertexNormals();
  return ng;
}

/** Local geometry for one part (sized/rotated/positioned in the construct frame). */
export function partGeometry(part: Part, scale: number, stretch?: [number, number, number]): { geo: BufferGeometry; center: Vector3 } {
  const t = partTransform(part, scale, stretch);
  const s = t.size;
  let g: BufferGeometry;
  switch (part.shape) {
    case 'box': g = new BoxGeometry(s.x, s.y, s.z); break;
    case 'sphere': g = new SphereGeometry(0.5, 20, 14); g.scale(s.x, s.y, s.z); break;
    case 'cylinder': g = new CylinderGeometry(0.5, 0.5, 1, 20, 1); g.scale(s.x, s.y, s.z); break;
    case 'cone': g = new ConeGeometry(0.5, 1, 20, 1); g.scale(s.x, s.y, s.z); break;
    case 'capsule': {
      const r = Math.max(0.02, (s.x + s.z) / 4);
      g = new CapsuleGeometry(r, Math.max(0.001, s.y - 2 * r), 5, 16);
      g.scale(s.x / (2 * r), 1, s.z / (2 * r));
      break;
    }
    case 'wedge': g = wedgeGeometry(s); break;
    case 'torus': {
      const R0 = Math.max(0.05, s.x / 2 - s.y / 4);
      const tube = Math.max(0.05, s.y / 2);
      g = new TorusGeometry(R0, tube, 10, 32);
      g.rotateX(Math.PI / 2);
      break;
    }
    case 'dome': g = new SphereGeometry(1, 28, 10, 0, Math.PI * 2, 0, Math.PI / 2); g.scale(s.x / 2, s.y, s.z / 2); break;
    default: g = new BoxGeometry(s.x, s.y, s.z);
  }
  g.deleteAttribute('uv');
  if (g.index) g = g.toNonIndexed();
  g.applyQuaternion(t.quat);
  g.translate(t.pos.x, t.pos.y, t.pos.z);
  return { geo: g, center: t.pos.clone() };
}

export interface ConstructGeometry {
  fill: InstancedBufferGeometry;
  edges: InstancedBufferGeometry;
  centers: Vector3[];
  radius: number;
}

/**
 * Build merged fill + edge geometry. `origin` (construct-local) is where assembly starts (the ring
 * hand); parts closer to it form first. `include` selects a subset of parts (one articulated group);
 * the assembly order is still ranked over the whole construct so split groups form exactly like one mesh.
 */
export function buildConstructGeometry(spec: ConstructSpec, origin: Vector3 | null, maxInstances: number, include?: (partIndex: number) => boolean): ConstructGeometry {
  const fills: BufferGeometry[] = [];
  const edges: BufferGeometry[] = [];
  const centers: Vector3[] = [];
  const parts = spec.parts.length ? spec.parts : [{ shape: 'box', size: [1, 1, 1], pos: [0, 0, 0] } as Part];
  const allCenters = parts.map((p) => partTransform(p, spec.scale, spec.stretch).pos);
  const o = origin ?? new Vector3(0, 0, -1e3);
  const d = allCenters.map((c) => c.distanceTo(o));
  const order = d.map((_, i) => i).sort((a, b) => d[a] - d[b]);
  const rankAll = new Array(allCenters.length).fill(0);
  order.forEach((pi, r) => { rankAll[pi] = allCenters.length > 1 ? r / (allCenters.length - 1) : 0; });
  const rank: number[] = [];
  for (let i = 0; i < parts.length; i++) {
    if (include && !include(i)) continue;
    const { geo, center } = partGeometry(parts[i], spec.scale, spec.stretch);
    centers.push(center);
    rank.push(rankAll[i]);
    fills.push(geo);
    const eg = new EdgesGeometry(geo, 28);
    edges.push(eg);
  }
  const tag = (g: BufferGeometry, i: number) => {
    const n = g.getAttribute('position').count;
    const a = new Float32Array(n * 4);
    for (let k = 0; k < n; k++) { a[k * 4] = centers[i].x; a[k * 4 + 1] = centers[i].y; a[k * 4 + 2] = centers[i].z; a[k * 4 + 3] = rank[i]; }
    g.setAttribute('aPart', new BufferAttribute(a, 4));
  };
  fills.forEach(tag);
  edges.forEach(tag);
  for (const e of edges) if (!e.getAttribute('normal')) e.setAttribute('normal', new BufferAttribute(new Float32Array(e.getAttribute('position').count * 3), 3));
  const fill = toInstanced(mergeGeometries(fills, false)!, maxInstances);
  const edge = toInstanced(mergeGeometries(edges, false)!, maxInstances);
  fills.forEach((g) => g.dispose());
  edges.forEach((g) => g.dispose());
  fill.computeBoundingSphere();
  return { fill, edges: edge, centers, radius: fill.boundingSphere?.radius ?? 1 };
}

function toInstanced(g: BufferGeometry, n: number): InstancedBufferGeometry {
  const ig = new InstancedBufferGeometry();
  for (const name of Object.keys(g.attributes)) ig.setAttribute(name, g.getAttribute(name));
  if (g.index) ig.setIndex(g.index);
  ig.setAttribute('iPos', new InstancedBufferAttribute(new Float32Array(n * 3), 3));
  ig.setAttribute('iQuat', new InstancedBufferAttribute(new Float32Array(n * 4), 4));
  ig.setAttribute('iDelay', new InstancedBufferAttribute(new Float32Array(n), 1));
  ig.instanceCount = n;
  return ig;
}

/** Share instance attribute buffers between fill and edges so one write updates both. */
export function shareInstanceAttributes(src: InstancedBufferGeometry, dst: InstancedBufferGeometry) {
  for (const k of ['iPos', 'iQuat', 'iDelay']) dst.setAttribute(k, src.getAttribute(k));
}

// ------------------------------------------------------------------------------------------------
// Materials
// ------------------------------------------------------------------------------------------------

const VERT = /* glsl */ `
attribute vec4 aPart;
attribute vec3 iPos;
attribute vec4 iQuat;
attribute float iDelay;
uniform float uForm;
uniform float uDissolve;
uniform float uPulse;
varying vec3 vLocal;
varying vec3 vN;
varying vec3 vView;
varying float vForm;
vec3 qrot(vec4 q, vec3 v) { return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v); }
float easeOutBack(float x) { float c1 = 1.70158; float c3 = c1 + 1.0; return 1.0 + c3 * pow(x - 1.0, 3.0) + c1 * pow(x - 1.0, 2.0); }
void main() {
  float f = clamp((uForm - iDelay - aPart.w * 0.55) / 0.45, 0.0, 1.0);
  float e = easeOutBack(f);
  vec3 p = mix(aPart.xyz, position, e);
  p += (position - aPart.xyz) * (uDissolve * 0.35 + uPulse * 0.06) + normal * uDissolve * 0.2;
  vec3 wp = qrot(iQuat, p) + iPos;
  vLocal = position;
  vN = qrot(iQuat, normal);
  vView = cameraPosition - wp;
  vForm = f;
  gl_Position = projectionMatrix * viewMatrix * vec4(wp, 1.0);
}`;

const FILL_FRAG = /* glsl */ `
uniform vec3 uColor;
uniform float uOpacity;
uniform float uFlash;
uniform float uBack;
uniform float uDissolve;
uniform float uTime;
uniform float uShell;
varying vec3 vLocal;
varying vec3 vN;
varying vec3 vView;
varying float vForm;
${NOISE_GLSL}
void main() {
  vec3 n = normalize(vN);
  vec3 v = normalize(vView);
  float ndv = abs(dot(n, v));
  float fres = pow(1.0 - ndv, 2.4);
  float dn = vnoise3(vLocal * 3.5 + 11.0);
  if (uDissolve > 0.0 && dn < uDissolve * 1.1 - 0.05) discard;
  float burn = uDissolve > 0.0 ? 1.0 - smoothstep(0.0, 0.1, dn - (uDissolve * 1.1 - 0.05)) : 0.0;
  // scrolling energy lines (object space so they travel with the construct)
  float coord = vLocal.y * 1.4 + vLocal.x * 0.25 - uTime * 0.9;
  float band = abs(fract(coord) - 0.5);
  float lines = 1.0 - smoothstep(0.0, fwidth(coord) * 1.2 + 0.035, band);
  float sh = vnoise3(vLocal * 2.2 + vec3(0.0, uTime * 1.3, 0.0));
  float forming = 1.0 - vForm;
  vec3 hot = mix(uColor, vec3(1.0), 0.45);
  vec3 col = uColor * (0.5 + 0.25 * sh + 0.5 * lines) + hot * fres * 2.2 + hot * (forming * 2.5 + uFlash * 1.5 + burn * 3.0);
  float a = 0.16 + 0.1 * sh + 0.62 * fres + 0.16 * lines + forming * 0.5 + uFlash * 0.35 + burn;
  a *= mix(1.0, 0.7, uShell);
  if (uBack > 0.5) { col *= 0.5; a *= 0.55; }
  a = clamp(a, 0.0, 1.0) * uOpacity;
  gl_FragColor = vec4(col * a, a);
}`;

const EDGE_FRAG = /* glsl */ `
uniform vec3 uColor;
uniform float uOpacity;
uniform float uFlash;
uniform float uDissolve;
uniform float uTime;
uniform float uGhost;
varying vec3 vLocal;
varying float vForm;
void main() {
  if (uDissolve > 0.0 && fract(dot(vLocal, vec3(1.7, 2.3, 1.9)) * 3.0) < uDissolve) discard;
  vec3 hot = mix(uColor, vec3(1.0), 0.55);
  vec3 col = hot * (2.4 + uFlash * 2.0 + (1.0 - vForm) * 3.0);
  float a = 0.95 * uOpacity;
  if (uGhost > 0.5) {
    float dash = step(0.45, fract(dot(vLocal, vec3(1.3, 1.7, 1.1)) * 1.5 - uTime * 1.2));
    col = hot * 1.4;
    a = uOpacity * (0.35 + 0.55 * dash) * (0.75 + 0.25 * sin(uTime * 7.0));
  }
  gl_FragColor = vec4(col * a, a);
}`;

const GHOST_FRAG = /* glsl */ `
uniform vec3 uColor;
uniform float uOpacity;
uniform float uTime;
varying vec3 vLocal;
varying vec3 vN;
varying vec3 vView;
void main() {
  vec3 n = normalize(vN);
  vec3 v = normalize(vView);
  float fres = pow(1.0 - abs(dot(n, v)), 2.0);
  float scan = 0.5 + 0.5 * sin(gl_FragCoord.y * 1.6 - uTime * 18.0);
  float pulse = 0.75 + 0.25 * sin(uTime * 6.0);
  vec3 hot = mix(uColor, vec3(1.0), 0.35);
  vec3 col = hot * (0.35 + 1.4 * fres) * pulse;
  float a = (0.06 + 0.32 * fres + 0.06 * scan) * uOpacity * pulse;
  gl_FragColor = vec4(col * a, a);
}`;

/**
 * Premultiplied "over" for colour; alpha channel forced to 0 so the post pass knows these pixels are
 * energy (skip fog + ink outline there).
 */
function energyBlend(m: ShaderMaterial) {
  m.blending = CustomBlending;
  m.blendEquation = AddEquation;
  m.blendSrc = OneFactor;
  m.blendDst = OneMinusSrcAlphaFactor;
  m.blendSrcAlpha = ZeroFactor;
  m.blendDstAlpha = ZeroFactor;
  m.transparent = true;
  return m;
}

export interface HardlightUniforms {
  uColor: { value: Color };
  uOpacity: { value: number };
  uFlash: { value: number };
  uDissolve: { value: number };
  uForm: { value: number };
  uPulse: { value: number };
  uTime: { value: number };
  uShell: { value: number };
  uGhost: { value: number };
}

export function hardlightUniforms(color: Color): HardlightUniforms {
  return {
    uColor: { value: color.clone() }, uOpacity: { value: 1 }, uFlash: { value: 0 }, uDissolve: { value: 0 }, uForm: { value: 1 },
    uPulse: { value: 0 }, uTime: { value: 0 }, uShell: { value: 0 }, uGhost: { value: 0 },
  };
}

export function fillMaterial(u: HardlightUniforms, side: Side): ShaderMaterial {
  const m = new ShaderMaterial({
    uniforms: { ...u, uBack: { value: side === BackSide ? 1 : 0 } },
    vertexShader: VERT,
    fragmentShader: FILL_FRAG,
    side,
    depthWrite: side === FrontSide,
  });
  return energyBlend(m);
}

export function edgeMaterial(u: HardlightUniforms): ShaderMaterial {
  return energyBlend(new ShaderMaterial({ uniforms: { ...u }, vertexShader: VERT, fragmentShader: EDGE_FRAG, depthWrite: false }));
}

export function ghostMaterial(u: HardlightUniforms): ShaderMaterial {
  return energyBlend(new ShaderMaterial({ uniforms: { ...u }, vertexShader: VERT, fragmentShader: GHOST_FRAG, depthWrite: false, side: FrontSide }));
}
