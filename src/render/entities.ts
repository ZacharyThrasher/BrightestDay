import {
  Color, CylinderGeometry, Group, IcosahedronGeometry, Mesh, Quaternion, ShaderMaterial, SphereGeometry, TorusGeometry, Vector3,
  CustomBlending, OneFactor, OneMinusSrcAlphaFactor, ZeroFactor, CapsuleGeometry, BoxGeometry, BufferAttribute, Matrix4, Euler,
  type Object3D, type BufferGeometry,
} from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { World, Entity, EntityType } from '../sim/world.js';
import { ENTITY_COLORS } from './config.js';
import { toon, staticsMaterial } from './toon.js';
import { Soup } from './statics.js';
import type { FX } from './fx.js';

export function glowMat(color: string | Color, intensity: number) {
  const m = new ShaderMaterial({
    uniforms: { uColor: { value: new Color(color).multiplyScalar(intensity) }, uOn: { value: 1 } },
    vertexShader: `varying vec3 vN; varying vec3 vV;
      void main(){ vec4 wp = modelMatrix * vec4(position,1.0); vN = normalize(mat3(modelMatrix)*normal); vV = cameraPosition - wp.xyz; gl_Position = projectionMatrix * viewMatrix * wp; }`,
    fragmentShader: `uniform vec3 uColor; uniform float uOn; varying vec3 vN; varying vec3 vV;
      void main(){ float f = abs(dot(normalize(vN), normalize(vV))); vec3 c = mix(vec3(0.08,0.07,0.1), uColor * (0.7 + 0.6*f), uOn); gl_FragColor = vec4(c, 1.0); }`,
  });
  m.blending = CustomBlending;
  m.blendSrc = OneFactor; m.blendDst = OneMinusSrcAlphaFactor; m.blendSrcAlpha = ZeroFactor; m.blendDstAlpha = ZeroFactor;
  return m;
}

function soupMesh(build: (s: Soup) => void): Mesh {
  const s = new Soup();
  build(s);
  const m = new Mesh(s.geometry(), staticsMaterial());
  m.castShadow = true;
  m.receiveShadow = true;
  return m;
}

const Q0 = new Quaternion();

interface MergePart { geo: BufferGeometry; s: [number, number, number]; p: [number, number, number]; r?: [number, number, number]; c: Color }

/** Bake several primitive placements into one vertex-coloured geometry (1 draw call). */
function mergeParts(parts: MergePart[]): BufferGeometry {
  const geos = parts.map((pt) => {
    const g = pt.geo.clone();
    if (g.getAttribute('uv')) g.deleteAttribute('uv');
    const m = new Matrix4().compose(new Vector3(...pt.p), new Quaternion().setFromEuler(new Euler(...(pt.r ?? [0, 0, 0]))), new Vector3(...pt.s));
    g.applyMatrix4(m);
    const n = g.getAttribute('position').count;
    const col = new Float32Array(n * 3);
    for (let i = 0; i < n; i++) { col[i * 3] = pt.c.r; col[i * 3 + 1] = pt.c.g; col[i * 3 + 2] = pt.c.b; }
    g.setAttribute('color', new BufferAttribute(col, 3));
    return g.index ? g.toNonIndexed() : g;
  });
  return mergeGeometries(geos, false)!;
}

function mergedMesh(parts: MergePart[], rim: number): Mesh {
  const m = new Mesh(mergeParts(parts), toon({ vertexColors: true, rim }));
  m.castShadow = true;
  return m;
}

/** Spinning-rotor discs: translucent with rotating blade streaks (no per-rotor transforms needed). */
const rotorMat = (() => {
  const m = new ShaderMaterial({
    uniforms: { uTime: { value: 0 } },
    vertexShader: `varying vec3 vL; varying vec3 vC; attribute vec3 color; void main(){ vL = position; vC = color; gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`,
    fragmentShader: `uniform float uTime; varying vec3 vL; varying vec3 vC;
      void main(){
        float a = atan(vL.z - (vL.z > 0.0 ? 0.813 : -0.813), vL.x - (vL.x > 0.0 ? 0.813 : -0.813));
        float streak = pow(0.5 + 0.5 * cos(2.0 * a - uTime * 40.0), 6.0);
        float alpha = 0.18 + 0.35 * streak;
        gl_FragColor = vec4(vec3(0.85, 0.9, 1.0) * alpha, alpha * 0.5);
      }`,
    transparent: true, depthWrite: false,
  });
  m.blending = CustomBlending; m.blendSrc = OneFactor; m.blendDst = OneMinusSrcAlphaFactor; m.blendSrcAlpha = ZeroFactor; m.blendDstAlpha = OneMinusSrcAlphaFactor;
  return m;
})();

interface Model {
  group: Group;
  parts: Record<string, Object3D>;
  eye?: ShaderMaterial;
}

// shared geometries
const G = {
  sphere: new SphereGeometry(0.5, 18, 12),
  cyl: new CylinderGeometry(0.5, 0.5, 1, 16),
  capsule: new CapsuleGeometry(0.5, 1, 4, 10),
  torus: new TorusGeometry(0.5, 0.06, 6, 24),
  rock: new IcosahedronGeometry(1, 1),
  box: new BoxGeometry(1, 1, 1),
};

function mesh(geo: any, mat: any, s: [number, number, number], p: [number, number, number] = [0, 0, 0], shadow = true) {
  const m = new Mesh(geo, mat);
  m.scale.set(...s);
  m.position.set(...p);
  m.castShadow = shadow;
  return m;
}

function buildModel(ent: Pick<Entity, 'type' | 'hostile' | 'variant' | 'size' | 'color'>): Model {
  const type: EntityType = ent.type;
  const hostile = ent.hostile;
  const group = new Group();
  const parts: Record<string, Object3D> = {};
  const C = ENTITY_COLORS;
  let eye: ShaderMaterial | undefined;
  switch (type) {
    case 'drone': {
      const body = new Group();
      const trim = new Color(C.droneTrim), dark = new Color(C.droneBody);
      const parts: MergePart[] = [
        { geo: G.sphere, s: [1.25, 0.6, 1.25], p: [0, 0, 0], c: dark },
        { geo: G.sphere, s: [0.8, 0.45, 0.8], p: [0, 0.18, 0], c: trim },
        { geo: G.torus, s: [1.3, 1.3, 1.6], p: [0, 0, 0], r: [Math.PI / 2, 0, 0], c: trim },
      ];
      const discs: MergePart[] = [];
      for (let i = 0; i < 4; i++) {
        const a = Math.PI / 4 + (i * Math.PI) / 2;
        parts.push({ geo: G.box, s: [0.9, 0.1, 0.14], p: [Math.cos(a) * 0.75, 0.05, Math.sin(a) * 0.75], r: [0, -a, 0], c: dark });
        parts.push({ geo: G.cyl, s: [0.14, 0.12, 0.14], p: [Math.cos(a) * 1.15, 0.14, Math.sin(a) * 1.15], c: trim });
        discs.push({ geo: G.cyl, s: [0.85, 0.005, 0.85], p: [Math.cos(a) * 1.15, 0.21, Math.sin(a) * 1.15], c: trim });
      }
      body.add(mergedMesh(parts, 0.3));
      const rotor = new Mesh(mergeParts(discs), rotorMat);
      body.add(rotor);
      eye = glowMat(hostile ? C.hostileGlow : '#7fd3ff', 3.2);
      body.add(mesh(G.sphere, eye, [0.32, 0.22, 0.2], [0, 0.0, -0.58], false));
      if (ent.variant === 'heavy') {
        // armoured heavy drone: same drone, 1.5x, with side plates and a belly gun
        const plate = new Color(C.turretBase);
        body.add(mergedMesh([
          { geo: G.box, s: [0.16, 0.42, 0.9], p: [-0.66, -0.02, 0], r: [0, 0, 0.35], c: plate },
          { geo: G.box, s: [0.16, 0.42, 0.9], p: [0.66, -0.02, 0], r: [0, 0, -0.35], c: plate },
          { geo: G.sphere, s: [0.7, 0.4, 0.7], p: [0, -0.26, 0], c: dark },
          { geo: G.cyl, s: [0.14, 0.6, 0.14], p: [0, -0.34, -0.35], r: [Math.PI / 2, 0, 0], c: trim },
        ], 0.3));
        body.scale.setScalar(1.5);
      }
      group.add(body);
      return { group, parts: { body }, eye };
    }
    case 'robot': {
      // chunky hostile walker, same palette as drones / turrets
      const light = new Color(C.turretHead), dark = new Color(C.droneBody), mid = new Color(C.turretBase), trim = new Color(C.droneTrim);
      const body = new Group();
      body.add(mergedMesh([
        { geo: G.box, s: [0.95, 0.75, 0.6], p: [0, 0.22, 0], c: light },
        { geo: G.box, s: [0.6, 0.34, 0.08], p: [0, 0.3, -0.31], c: dark },
        { geo: G.box, s: [0.7, 0.22, 0.5], p: [0, -0.2, 0], c: mid },
        { geo: G.box, s: [0.55, 0.42, 0.5], p: [0, 0.83, 0], c: dark },
        { geo: G.box, s: [0.5, 0.08, 0.46], p: [0, 1.07, 0], c: light },
        { geo: G.cyl, s: [0.05, 0.32, 0.05], p: [0.17, 1.22, 0.05], c: mid },
        { geo: G.sphere, s: [0.12, 0.12, 0.12], p: [0.17, 1.4, 0.05], c: trim },
        { geo: G.sphere, s: [0.34, 0.34, 0.34], p: [-0.6, 0.5, 0], c: trim },
        { geo: G.sphere, s: [0.34, 0.34, 0.34], p: [0.6, 0.5, 0], c: trim },
      ], 0.3));
      eye = glowMat(hostile ? C.hostileGlow : '#7fd3ff', 3.0);
      body.add(mesh(G.sphere, eye, [0.42, 0.11, 0.1], [0, 0.86, -0.25], false));
      for (const sd of [-1, 1]) {
        const arm = new Group(); arm.position.set(sd * 0.62, 0.5, 0);
        arm.add(mergedMesh([
          { geo: G.capsule, s: [0.2, 0.26, 0.2], p: [0, -0.33, 0], c: dark },
          { geo: G.box, s: [0.3, 0.3, 0.32], p: [0, -0.74, 0], c: mid },
        ], 0.3));
        body.add(arm); parts['arm' + (sd < 0 ? 'L' : 'R')] = arm;
        const leg = new Group(); leg.position.set(sd * 0.22, -0.22, 0);
        leg.add(mergedMesh([
          { geo: G.capsule, s: [0.26, 0.28, 0.26], p: [0, -0.36, 0], c: mid },
          { geo: G.box, s: [0.3, 0.16, 0.46], p: [0, -0.74, -0.06], c: dark },
        ], 0.3));
        group.add(leg); parts['leg' + (sd < 0 ? 'L' : 'R')] = leg;
      }
      parts.body = body;
      group.add(body);
      return { group, parts, eye };
    }
    case 'block': {
      const sz = ent.size ?? new Vector3(1, 1, 1);
      const col = ent.color ?? 0xd9d2c0;
      group.add(soupMesh((s) => { s.begin(new Vector3(), Q0, col, 4); s.box(sz); }));
      break;
    }
    case 'turret': {
      group.add(soupMesh((s) => { s.begin(new Vector3(0, -0.7, 0), Q0, 0x50576e, 4); s.cylinder(1.25, 1.0, 12); s.begin(new Vector3(0, -0.05, 0), Q0, 0x6b7390, 4); s.cylinder(0.95, 0.35, 12); }));
      const head = new Group();
      head.position.set(0, 0.55, 0);
      head.add(mergedMesh([
        { geo: G.sphere, s: [1.6, 1.3, 1.6], p: [0, 0, 0], c: new Color(C.turretHead) },
        { geo: G.cyl, s: [0.32, 1.6, 0.32], p: [0, 0.05, -0.95], r: [Math.PI / 2, 0, 0], c: new Color(C.turretBase) },
        { geo: G.torus, s: [0.55, 0.55, 1.2], p: [0, 0.05, -1.75], c: new Color('#2a2e3d') },
      ], 0.3));
      eye = glowMat(hostile ? C.hostileGlow : '#7fd3ff', 3.0);
      head.add(mesh(G.sphere, eye, [0.5, 0.2, 0.2], [0, 0.35, -0.68], false));
      parts.head = head;
      group.add(head);
      break;
    }
    case 'civilian': {
      const shirt = toon({ color: C.civilianShirt, rim: 0.3 });
      const pants = toon({ color: C.civilianPants, rim: 0.2 });
      const skin = toon({ color: '#e9b894', rim: 0.2 });
      group.add(mergedMesh([
        { geo: G.capsule, s: [0.46, 0.27, 0.32], p: [0, 0.2, 0], c: new Color(C.civilianShirt) },
        { geo: G.sphere, s: [0.3, 0.34, 0.3], p: [0, 0.72, 0], c: new Color('#e9b894') },
        { geo: G.sphere, s: [0.32, 0.22, 0.32], p: [0, 0.82, 0.03], c: new Color('#5a3b2a') },
      ], 0.3));
      void skin;
      for (const s of [-1, 1]) {
        const leg = new Group(); leg.position.set(s * 0.11, -0.2, 0);
        leg.add(mesh(G.capsule, pants, [0.16, 0.33, 0.16], [0, -0.33, 0]));
        group.add(leg); parts['leg' + (s < 0 ? 'L' : 'R')] = leg;
        const arm = new Group(); arm.position.set(s * 0.27, 0.42, 0);
        arm.add(mesh(G.capsule, shirt, [0.12, 0.28, 0.12], [0, -0.28, 0]));
        group.add(arm); parts['arm' + (s < 0 ? 'L' : 'R')] = arm;
      }
      break;
    }
    case 'boulder': {
      const m = mesh(G.rock, toon({ color: C.boulder, rim: 0.15 }), [2.2, 2.05, 2.2]);
      m.receiveShadow = true;
      group.add(m);
      parts.body = m;
      break;
    }
    case 'crate': {
      group.add(soupMesh((s) => {
        s.begin(new Vector3(), Q0, 0xd99a55, 4); s.box(new Vector3(1.6, 1.6, 1.6));
        // framed edges
        for (const y of [-0.74, 0.74]) for (const z of [-0.74, 0.74]) { s.begin(new Vector3(0, y, z), Q0, 0xb87a3e, 4); s.box(new Vector3(1.66, 0.14, 0.14)); }
        for (const x of [-0.74, 0.74]) for (const z of [-0.74, 0.74]) { s.begin(new Vector3(x, 0, z), Q0, 0xb87a3e, 4); s.box(new Vector3(0.14, 1.66, 0.14)); }
      }));
      break;
    }
    case 'dummy': {
      group.add(soupMesh((s) => { s.begin(new Vector3(0, -1.0, 0), Q0, 0x8a7a64, 4); s.cylinder(0.6, 0.2, 10); s.begin(new Vector3(0, -0.5, 0), Q0, 0x8a7a64, 4); s.cylinder(0.08, 1.0, 8); }));
      const body = new Group();
      const X = Math.PI / 2;
      body.add(mergedMesh([
        { geo: G.capsule, s: [0.9, 0.55, 0.7], p: [0, 0.25, 0], c: new Color(C.dummy) },
        { geo: G.sphere, s: [0.55, 0.55, 0.55], p: [0, 1.2, 0], c: new Color(C.dummy) },
        { geo: G.cyl, s: [0.6, 0.04, 0.6], p: [0, 0.35, -0.36], r: [X, 0, 0], c: new Color(C.dummyTarget) },
        { geo: G.cyl, s: [0.38, 0.05, 0.38], p: [0, 0.35, -0.37], r: [X, 0, 0], c: new Color('#ffffff') },
        { geo: G.cyl, s: [0.16, 0.06, 0.16], p: [0, 0.35, -0.38], r: [X, 0, 0], c: new Color(C.dummyTarget) },
      ], 0.2));
      parts.body = body;
      group.add(body);
      break;
    }
  }
  return { group, parts, eye };
}

interface EView {
  id: string;
  type: EntityType;
  model: Model;
  prevP: Vector3; currP: Vector3;
  prevQ: Quaternion; currQ: Quaternion;
  vel: Vector3;
  hit: number; // hit flash/pop 0..1
  fireFlash: number;
  disabled: boolean;
  held: boolean;
  hurt: boolean;
  safe: boolean;
  yaw: number;
  pitch: number;
  smoke: number;
  mode: string | null; // robots: walk / knocked / getup / down
  punch: number; // robots: seconds into a punch, -1 when idle
  phase: number; // walk cycle
}

interface DebrisView { mesh: Mesh; prevP: Vector3; currP: Vector3; prevQ: Quaternion; currQ: Quaternion; age: number; life: number; seen: number }

const _v = new Vector3(), _q = new Quaternion();

/** Procedural models for drones, turrets, civilians, boulders, crates and dummies. */
export class EntityViews {
  readonly root = new Group();
  private views = new Map<string, EView>();
  private debris = new Map<number, DebrisView>();
  private tickN = 0;

  constructor(private fx: FX) { this.root.name = 'entities'; }

  get count() { return this.views.size; }

  tick(w: World) {
    this.tickDebris(w);
    for (const [id, v] of this.views) {
      if (!w.entities.has(id)) { this.root.remove(v.model.group); this.views.delete(id); }
    }
    for (const e of w.entities.values()) {
      let v = this.views.get(e.id);
      const t = e.body.translation();
      const r = e.body.rotation();
      if (!v) {
        const model = buildModel(e);
        this.root.add(model.group);
        v = {
          id: e.id, type: e.type, model, prevP: new Vector3(t.x, t.y, t.z), currP: new Vector3(t.x, t.y, t.z), prevQ: new Quaternion(r.x, r.y, r.z, r.w), currQ: new Quaternion(r.x, r.y, r.z, r.w),
          vel: new Vector3(), hit: 0, fireFlash: 0, disabled: e.disabled, held: false, hurt: false, safe: false, yaw: 0, pitch: 0, smoke: 0,
          mode: null, punch: -1, phase: 0,
        };
        this.views.set(e.id, v);
      }
      v.prevP.copy(v.currP); v.prevQ.copy(v.currQ);
      v.currP.set(t.x, t.y, t.z); v.currQ.set(r.x, r.y, r.z, r.w);
      const lv = e.body.linvel(); v.vel.set(lv.x, lv.y, lv.z);
      if (e.disabled && !v.disabled) {
        v.disabled = true;
        if (v.model.eye) v.model.eye.uniforms.uOn.value = 0;
      }
      v.held = !!(e.heldBy || e.restrainedBy);
      if (e.ai?.kind === 'robot') { v.mode = e.ai.mode; v.punch = e.ai.punchT; }
      if (e.hurt && !v.hurt) { v.hurt = true; }
      if (e.safe && !v.safe) { v.safe = true; this.fx.ring(v.currP.clone(), new Color('#7dffb0'), 0.5, 4, 0.8, 2); }
    }
  }

  /** Broken pieces: plain toon boxes that shrink away before they despawn. */
  private tickDebris(w: World) {
    this.tickN++;
    for (const d of w.content.debris) {
      let v = this.debris.get(d.id);
      const t = d.body.translation(), r = d.body.rotation();
      if (!v) {
        const mesh = new Mesh(G.box, toon({ color: d.color, rim: 0.15 }));
        mesh.castShadow = true;
        mesh.scale.copy(d.size);
        this.root.add(mesh);
        v = { mesh, prevP: new Vector3(t.x, t.y, t.z), currP: new Vector3(t.x, t.y, t.z), prevQ: new Quaternion(r.x, r.y, r.z, r.w), currQ: new Quaternion(r.x, r.y, r.z, r.w), age: 0, life: d.life, seen: 0 };
        v.mesh.userData.size = d.size.clone();
        this.debris.set(d.id, v);
      }
      v.prevP.copy(v.currP); v.prevQ.copy(v.currQ);
      v.currP.set(t.x, t.y, t.z); v.currQ.set(r.x, r.y, r.z, r.w);
      v.age = w.time - d.born;
      v.seen = this.tickN;
    }
    for (const [id, v] of this.debris) if (v.seen !== this.tickN) { this.root.remove(v.mesh); this.debris.delete(id); }
  }

  onDamage(id: string) { const v = this.views.get(id); if (v) v.hit = 1; }
  onFire(id: string) { const v = this.views.get(id); if (v) v.fireFlash = 1; }

  muzzle(id: string, out: Vector3): Vector3 | null {
    const v = this.views.get(id);
    if (!v) return null;
    if (v.type === 'turret' && v.model.parts.head) return v.model.parts.head.localToWorld(out.set(0, 0.05, -1.9));
    return out.copy(v.currP);
  }

  frame(alpha: number, dt: number, time: number, playerPos: Vector3) {
    rotorMat.uniforms.uTime.value = time;
    for (const d of this.debris.values()) {
      d.mesh.position.copy(d.prevP).lerp(d.currP, alpha);
      d.mesh.quaternion.copy(d.prevQ).slerp(d.currQ, alpha);
      const k = Math.max(0.01, Math.min(1, (d.life - d.age) / 0.6));
      d.mesh.scale.copy(d.mesh.userData.size).multiplyScalar(k);
    }
    for (const v of this.views.values()) {
      const g = v.model.group;
      g.position.copy(v.prevP).lerp(v.currP, alpha);
      _q.copy(v.prevQ).slerp(v.currQ, alpha);
      v.hit = Math.max(0, v.hit - dt * 5);
      v.fireFlash = Math.max(0, v.fireFlash - dt * 6);
      const pop = 1 + v.hit * 0.18;
      const toP = _v.copy(playerPos).sub(g.position);
      switch (v.type) {
        case 'drone': {
          const body = v.model.parts.body;
          if (v.disabled || v.held) {
            g.quaternion.copy(_q);
          } else {
            const wantYaw = Math.atan2(-toP.x, -toP.z);
            let d = wantYaw - v.yaw; while (d > Math.PI) d -= 2 * Math.PI; while (d < -Math.PI) d += 2 * Math.PI;
            v.yaw += d * (1 - Math.exp(-5 * dt));
            g.quaternion.setFromAxisAngle(new Vector3(0, 1, 0), v.yaw);
            // bank into motion
            body.rotation.set(-v.vel.z * 0.02 * Math.cos(v.yaw), 0, v.vel.x * 0.03);
            body.position.y = Math.sin(time * 3 + v.currP.x) * 0.06;
          }
          if (v.model.eye) v.model.eye.uniforms.uOn.value = v.disabled ? 0 : 0.75 + v.fireFlash * 0.8;
          if (v.disabled) {
            v.smoke -= dt;
            if (v.smoke <= 0) { v.smoke = 0.08; this.fx.glow(g.position.clone().add(new Vector3(0, 0.4, 0)), new Color(0.25, 0.23, 0.28), 0.9, 1.1, 0.5, 2.2); if (Math.random() < 0.3) this.fx.sparks(g.position.clone(), new Color('#ffb04d'), 2, 4, { size: 0.04 }); }
          }
          break;
        }
        case 'turret': {
          g.quaternion.identity();
          const head = v.model.parts.head;
          if (!v.disabled) {
            v.yaw = Math.atan2(-toP.x, -toP.z);
            v.pitch = Math.atan2(toP.y - 0.6, Math.hypot(toP.x, toP.z));
          } else {
            v.pitch += (-0.5 - v.pitch) * Math.min(1, dt * 3);
            v.smoke -= dt;
            if (v.smoke <= 0) { v.smoke = 0.1; this.fx.glow(g.position.clone().add(new Vector3(0, 1.3, 0)), new Color(0.25, 0.23, 0.28), 1.2, 1.4, 0.5, 3); }
          }
          head.rotation.set(0, 0, 0);
          head.rotateY(v.yaw);
          head.rotateX(v.pitch);
          head.position.z = v.fireFlash * 0.12;
          if (v.model.eye) v.model.eye.uniforms.uOn.value = v.disabled ? 0 : 0.8 + v.fireFlash;
          break;
        }
        case 'civilian': {
          const hs = Math.hypot(v.vel.x, v.vel.z);
          const running = hs > 1.2 && !v.held && !v.hurt && Math.abs(v.vel.y) < 3 ? Math.min(1, hs / 4) : 0;
          if (running > 0) {
            // fleeing: face where they run, pump arms and legs
            const wantYaw = Math.atan2(-v.vel.x, -v.vel.z);
            let d = wantYaw - v.yaw; while (d > Math.PI) d -= 2 * Math.PI; while (d < -Math.PI) d += 2 * Math.PI;
            v.yaw += d * (1 - Math.exp(-10 * dt));
          } else v.yaw = Math.atan2(-toP.x, -toP.z);
          g.quaternion.setFromAxisAngle(new Vector3(0, 1, 0), v.yaw);
          v.phase += hs * dt * 3.2;
          const run = Math.sin(v.phase) * 0.9 * running;
          const falling = v.vel.y < -3 && !v.held;
          const f = falling ? 1 : 0;
          const flail = Math.sin(time * 14);
          v.model.parts.armL.rotation.set(-run, 0, -0.2 - f * (2.4 + flail * 0.4));
          v.model.parts.armR.rotation.set(run, 0, 0.2 + f * (2.4 - flail * 0.4));
          v.model.parts.legL.rotation.set(f * (0.4 + flail * 0.5) + run, 0, -0.05);
          v.model.parts.legR.rotation.set(-f * (0.4 + flail * 0.5) - run, 0, 0.05);
          if (v.hurt) g.rotation.z = 1.4;
          if (v.safe && !v.hurt) g.position.y += Math.abs(Math.sin(time * 5)) * 0.15; // happy hop
          break;
        }
        case 'robot': {
          g.quaternion.copy(_q);
          const P = v.model.parts;
          const upright = (v.mode === 'walk' || v.mode === 'getup') && !v.held && !v.disabled;
          const hs = Math.hypot(v.vel.x, v.vel.z);
          const moving = upright ? Math.min(1, hs / 2.5) : 0;
          v.phase += hs * dt * 3.0 * (upright ? 1 : 0);
          const sw = Math.sin(v.phase) * 0.7 * moving;
          const limp = upright ? 0 : v.held ? Math.sin(time * 9) * 0.5 : 0.35;
          P.legL.rotation.set(sw + limp, 0, upright ? 0 : -0.15);
          P.legR.rotation.set(-sw - limp, 0, upright ? 0 : 0.15);
          P.armL.rotation.set(-sw * 0.8, 0, upright ? 0 : -0.9 - limp * 0.5);
          P.armR.rotation.set(sw * 0.8, 0, upright ? 0 : 0.9 + limp * 0.5);
          if (v.punch >= 0 && upright) {
            const t = v.punch;
            const ext = t < 0.32 ? -0.6 * (t / 0.32) : t < 0.45 ? 1.65 : 1.65 * Math.max(0, 1 - (t - 0.45) / 0.25);
            P.armR.rotation.set(ext, 0, 0);
            P.body.rotation.y = t < 0.32 ? 0.25 * (t / 0.32) : t < 0.45 ? -0.3 : -0.3 * Math.max(0, 1 - (t - 0.45) / 0.25);
          } else P.body.rotation.y = 0;
          P.body.position.y = Math.abs(Math.cos(v.phase)) * 0.07 * moving;
          P.body.rotation.z = Math.sin(v.phase) * 0.05 * moving;
          if (v.model.eye) v.model.eye.uniforms.uOn.value = v.disabled ? 0 : 0.8 + (v.punch >= 0 ? 0.6 : 0) + v.hit * 0.5;
          if (v.disabled) {
            v.smoke -= dt;
            if (v.smoke <= 0) { v.smoke = 0.1; this.fx.glow(g.position.clone().add(new Vector3(0, 0.6, 0)), new Color(0.25, 0.23, 0.28), 1.0, 1.2, 0.5, 2.4); }
          }
          break;
        }
        default:
          g.quaternion.copy(_q);
      }
      g.scale.setScalar(pop);
    }
  }

  clear() {
    for (const v of this.views.values()) this.root.remove(v.model.group);
    this.views.clear();
    for (const d of this.debris.values()) this.root.remove(d.mesh);
    this.debris.clear();
  }

  positionOf(id: string): Vector3 | null { return this.views.get(id)?.model.group.position ?? null; }
}

export function entityLabel(e: Entity) { return e.label; }
