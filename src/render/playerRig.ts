import {
  BoxGeometry, CapsuleGeometry, Color, Euler, Group, InstancedMesh, Matrix4, Mesh, Object3D, Quaternion, ShaderMaterial,
  SphereGeometry, TorusGeometry, Vector3, CustomBlending, OneFactor, OneMinusSrcAlphaFactor, ZeroFactor, OctahedronGeometry,
  type BufferGeometry, type Scene,
} from 'three';
import { SUIT } from './config.js';
import { toon } from './toon.js';

/** Visual player state sampled per render frame (interpolated). */
export interface RigState {
  pos: Vector3; // capsule centre
  vel: Vector3;
  lookYaw: number;
  lookPitch: number;
  onGround: boolean;
  sliding: boolean;
  flying: boolean;
  boosting: boolean;
  tetherL: Vector3 | null; // anchor/hook tip points
  tetherR: Vector3 | null;
  castTarget: Vector3 | null; // point the ring hand should present toward (construct forming)
}

type Prim = 'capsule' | 'sphere' | 'box';

interface Piece { bone: Object3D; prim: Prim; local: Matrix4; color: Color }

function energyMat(color: Color, intensity: number) {
  const m = new ShaderMaterial({
    uniforms: { uColor: { value: color.clone().multiplyScalar(intensity) } },
    vertexShader: `varying vec3 vN; varying vec3 vV;
      void main(){ vec4 wp = modelMatrix * vec4(position,1.0); vN = normalize(mat3(modelMatrix)*normal); vV = cameraPosition - wp.xyz; gl_Position = projectionMatrix * viewMatrix * wp; }`,
    fragmentShader: `uniform vec3 uColor; varying vec3 vN; varying vec3 vV;
      void main(){ float f = pow(1.0 - abs(dot(normalize(vN), normalize(vV))), 1.5); vec3 c = uColor * (0.8 + f); gl_FragColor = vec4(c, 1.0); }`,
  });
  m.blending = CustomBlending;
  m.blendSrc = OneFactor; m.blendDst = OneMinusSrcAlphaFactor; m.blendSrcAlpha = ZeroFactor; m.blendDstAlpha = ZeroFactor;
  return m;
}

const D = (deg: number) => (deg * Math.PI) / 180;
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
const damp = (a: number, b: number, k: number, dt: number) => lerp(a, b, 1 - Math.exp(-k * dt));
function dampAngle(a: number, b: number, k: number, dt: number) {
  let d = b - a;
  while (d > Math.PI) d -= Math.PI * 2;
  while (d < -Math.PI) d += Math.PI * 2;
  return a + d * (1 - Math.exp(-k * dt));
}

type Pose = Record<string, [number, number, number]>;
const JOINTS = ['pelvis', 'spine', 'chest', 'neck', 'shL', 'shR', 'elL', 'elR', 'hipL', 'hipR', 'knL', 'knR', 'ftL', 'ftR'] as const;

/**
 * Original stylised hero built from primitives: navy suit with pale panels, mint emitters, a glowing
 * ring on the right hand. Rendered with 3 instanced toon meshes + a couple of emissive meshes.
 */
export class PlayerRig {
  readonly root = new Group();
  private bones: Record<string, Object3D> = {};
  private pieces: Piece[] = [];
  private meshes: Record<Prim, InstancedMesh>;
  private ring: Mesh;
  private visor: Mesh;
  private emblem: Mesh;
  private yaw = 0;
  private bodyPitch = 0;
  private bodyRoll = 0;
  private phase = 0;
  private w = { run: 0, air: 0, slide: 0, fly: 0, idle: 1 };
  private dive = 0;
  private cur: Pose = {};
  private lean = new Vector3();
  private prevVel = new Vector3();
  private crouchY = 0;
  private armOverride = { L: 0, R: 0 };
  readonly handL = new Vector3();
  readonly handR = new Vector3();
  readonly back = new Vector3();
  readonly feet = new Vector3();

  constructor(scene: Scene, ringColor: Color) {
    this.root.name = 'player';
    const B = (name: string, parent: Object3D | null, x: number, y: number, z: number) => {
      const o = new Object3D();
      o.name = name;
      o.position.set(x, y, z);
      (parent ?? this.root).add(o);
      this.bones[name] = o;
      return o;
    };
    // skeleton (model faces -Z, root at the feet)
    const pelvis = B('pelvis', null, 0, 0.98, 0);
    const spine = B('spine', pelvis, 0, 0.1, 0);
    const chest = B('chest', spine, 0, 0.24, 0);
    const neck = B('neck', chest, 0, 0.26, 0);
    const shL = B('shL', chest, -0.23, 0.17, 0);
    const shR = B('shR', chest, 0.23, 0.17, 0);
    const elL = B('elL', shL, 0, -0.29, 0);
    const elR = B('elR', shR, 0, -0.29, 0);
    B('haL', elL, 0, -0.27, 0);
    B('haR', elR, 0, -0.27, 0);
    const hipL = B('hipL', pelvis, -0.11, -0.06, 0);
    const hipR = B('hipR', pelvis, 0.11, -0.06, 0);
    const knL = B('knL', hipL, 0, -0.43, 0);
    const knR = B('knR', hipR, 0, -0.43, 0);
    B('ftL', knL, 0, -0.43, 0);
    B('ftR', knR, 0, -0.43, 0);
    void neck;

    const base = new Color(SUIT.base), panel = new Color(SUIT.panel), accent = new Color(SUIT.accent), skin = new Color(SUIT.skin), hair = new Color(SUIT.hair);
    const P = (bone: string, prim: Prim, pos: [number, number, number], scale: [number, number, number], color: Color, rot: [number, number, number] = [0, 0, 0]) => {
      const m = new Matrix4().compose(new Vector3(...pos), new Quaternion().setFromEuler(new Euler(...rot)), new Vector3(...scale));
      this.pieces.push({ bone: this.bones[bone], prim, local: m, color });
    };
    // unit prims: capsule r=0.5 total height 2 (so scale y = half length), sphere r=0.5, box 1
    // torso
    P('pelvis', 'box', [0, 0.0, 0], [0.3, 0.2, 0.2], base);
    P('pelvis', 'box', [0, 0.06, 0], [0.32, 0.05, 0.215], panel); // belt
    P('spine', 'capsule', [0, 0.12, 0], [0.27, 0.12, 0.19], base);
    P('chest', 'box', [0, 0.1, 0.0], [0.44, 0.3, 0.24], base, [0, 0, 0]);
    P('chest', 'box', [0, 0.1, -0.03], [0.3, 0.26, 0.2], panel); // chest plate (front, -z)
    P('chest', 'sphere', [-0.23, 0.18, 0], [0.18, 0.16, 0.18], accent); // shoulder pads
    P('chest', 'sphere', [0.23, 0.18, 0], [0.18, 0.16, 0.18], accent);
    // head
    P('neck', 'capsule', [0, 0.02, 0], [0.09, 0.05, 0.09], skin);
    P('neck', 'sphere', [0, 0.15, 0], [0.22, 0.25, 0.23], skin);
    P('neck', 'sphere', [0, 0.2, 0.025], [0.235, 0.2, 0.24], hair);
    P('neck', 'box', [0, 0.235, 0.03], [0.2, 0.07, 0.2], hair, [0.25, 0, 0]);
    // arms
    for (const s of ['L', 'R'] as const) {
      P('sh' + s, 'capsule', [0, -0.145, 0], [0.115, 0.16, 0.115], base);
      P('el' + s, 'capsule', [0, -0.13, 0], [0.1, 0.145, 0.1], base);
      P('el' + s, 'capsule', [0, -0.19, 0], [0.1, 0.035, 0.1], panel); // cuff
      P('ha' + s, 'sphere', [0, -0.04, 0], [0.1, 0.12, 0.08], panel);
      // legs
      P('hip' + s, 'capsule', [0, -0.215, 0], [0.16, 0.23, 0.16], base);
      P('kn' + s, 'capsule', [0, -0.21, 0], [0.125, 0.22, 0.125], base);
      P('kn' + s, 'sphere', [0, 0.0, -0.03], [0.12, 0.12, 0.12], accent); // knee pad
      P('kn' + s, 'capsule', [0, -0.3, 0], [0.12, 0.05, 0.12], panel); // boot cuff
      P('ft' + s, 'box', [0, -0.03, -0.06], [0.12, 0.09, 0.27], panel);
    }
    const geos: Record<Prim, BufferGeometry> = {
      capsule: new CapsuleGeometry(0.5, 1, 4, 10),
      sphere: new SphereGeometry(0.5, 14, 10),
      box: new BoxGeometry(1, 1, 1),
    };
    // capsule: unit length 1 + caps 1 → total 2, scale y = half length accounts for caps approx
    const mat = toon({ vertexColors: false, rim: 0.35, rimColor: '#dff6ff' });
    this.meshes = {} as any;
    for (const prim of ['capsule', 'sphere', 'box'] as Prim[]) {
      const count = this.pieces.filter((p) => p.prim === prim).length;
      const im = new InstancedMesh(geos[prim], mat, count);
      im.castShadow = true;
      im.receiveShadow = false;
      im.frustumCulled = false;
      let k = 0;
      for (const p of this.pieces) if (p.prim === prim) im.setColorAt(k++, p.color);
      if (im.instanceColor) im.instanceColor.needsUpdate = true;
      this.meshes[prim] = im;
      scene.add(im);
    }
    // emissive bits: ring, visor, chest emblem (original rhombus, not a lantern)
    this.ring = new Mesh(new TorusGeometry(0.05, 0.018, 8, 20), energyMat(ringColor, 3.5));
    this.visor = new Mesh(new BoxGeometry(0.19, 0.045, 0.02), energyMat(new Color(SUIT.visor), 1.6));
    this.emblem = new Mesh(new OctahedronGeometry(0.06, 0), energyMat(ringColor, 2.2));
    this.emblem.scale.set(0.9, 1.3, 0.25);
    this.bones.haR.add(this.ring);
    this.ring.position.set(0.0, -0.07, 0);
    this.ring.rotation.set(0, 0, Math.PI / 2);
    this.bones.neck.add(this.visor);
    this.visor.position.set(0, 0.16, -0.115);
    this.bones.chest.add(this.emblem);
    this.emblem.position.set(0, 0.14, -0.135);
    scene.add(this.root);
    for (const j of JOINTS) this.cur[j] = [0, 0, 0];
  }

  setRingColor(c: Color) { (this.ring.material as ShaderMaterial).uniforms.uColor.value.copy(c).multiplyScalar(3.5); (this.emblem.material as ShaderMaterial).uniforms.uColor.value.copy(c).multiplyScalar(2.2); }

  setVisible(v: boolean) {
    this.root.visible = v;
    for (const m of Object.values(this.meshes)) m.visible = v;
  }

  update(s: RigState, dt: number, time: number) {
    dt = Math.min(dt, 0.05);
    const hv = new Vector3(s.vel.x, 0, s.vel.z);
    const hs = hv.length();
    const speed = s.vel.length();
    const tethered = !!(s.tetherL || s.tetherR);
    const airborne = !s.onGround && !s.flying;
    // ---- facing: velocity when moving fast, otherwise look
    let targetYaw = s.lookYaw;
    if (s.flying && speed > 3) targetYaw = Math.atan2(-s.vel.x, -s.vel.z);
    else if (hs > 2.5 && !s.castTarget) targetYaw = Math.atan2(-hv.x, -hv.z);
    this.yaw = dampAngle(this.yaw, targetYaw, s.flying ? 6 : 12, dt);
    // ---- pose weights
    const target = { run: 0, air: 0, slide: 0, fly: 0, idle: 0 };
    if (s.flying) target.fly = 1;
    else if (s.sliding && s.onGround) target.slide = 1;
    else if (airborne) target.air = 1;
    else if (hs > 0.6) target.run = 1;
    else target.idle = 1;
    for (const k of Object.keys(this.w) as (keyof typeof this.w)[]) this.w[k] = damp(this.w[k], target[k], 14, dt);
    // ---- run cycle
    const stride = Math.min(1, hs / 11);
    this.phase += dt * (4 + hs * 0.95);
    const ph = this.phase;
    // ---- acceleration lean (in body frame)
    const acc = s.vel.clone().sub(this.prevVel).divideScalar(Math.max(dt, 1e-3));
    this.prevVel.copy(s.vel);
    const fwd = new Vector3(-Math.sin(this.yaw), 0, -Math.cos(this.yaw));
    const right = new Vector3(Math.cos(this.yaw), 0, -Math.sin(this.yaw));
    this.lean.x = damp(this.lean.x, Math.max(-1, Math.min(1, acc.dot(fwd) / 40)), 6, dt);
    this.lean.z = damp(this.lean.z, Math.max(-1, Math.min(1, acc.dot(right) / 40)), 6, dt);

    const poses: [number, Pose][] = [];
    const breathe = Math.sin(time * 2.2) * 0.03;
    poses.push([this.w.idle, {
      pelvis: [0, 0, 0], spine: [breathe, 0, 0], chest: [breathe, 0, 0], neck: [-breathe, 0, 0],
      shL: [0.05, 0, -0.12], shR: [0.05, 0, 0.12], elL: [0.25, 0, 0], elR: [0.25, 0, 0],
      hipL: [0.04, 0, -0.04], hipR: [-0.02, 0, 0.04], knL: [-0.08, 0, 0], knR: [-0.04, 0, 0], ftL: [0.04, 0, 0], ftR: [0.02, 0, 0],
    }]);
    const sw = Math.sin(ph), cw = Math.cos(ph);
    const A = 0.35 + 0.6 * stride;
    poses.push([this.w.run, {
      pelvis: [-0.12 - 0.25 * stride, sw * 0.12, 0], spine: [-0.05, -sw * 0.1, 0], chest: [-0.05, -sw * 0.2, 0], neck: [0.2 + 0.2 * stride, sw * 0.15, 0],
      shL: [sw * A * 1.1, 0, -0.15], shR: [-sw * A * 1.1, 0, 0.15], elL: [1.1 + 0.3 * stride, 0, 0], elR: [1.1 + 0.3 * stride, 0, 0],
      hipL: [-sw * A * 1.3 + 0.2 * stride, 0, 0], hipR: [sw * A * 1.3 + 0.2 * stride, 0, 0],
      knL: [-Math.max(0, cw) * (0.4 + 1.4 * stride) - 0.15, 0, 0], knR: [-Math.max(0, -cw) * (0.4 + 1.4 * stride) - 0.15, 0, 0],
      ftL: [0.2 * cw, 0, 0], ftR: [-0.2 * cw, 0, 0],
    }]);
    const vy = Math.max(-1, Math.min(1, s.vel.y / 15));
    const tuck = Math.max(0, vy);
    poses.push([this.w.air, {
      pelvis: [-0.15 - 0.2 * Math.min(1, hs / 25), 0, 0], spine: [-0.05, 0, 0], chest: [-0.05, 0, 0], neck: [0.15, 0, 0],
      shL: [0.35 - vy * 0.4, 0, -0.9 - vy * 0.3], shR: [0.35 - vy * 0.4, 0, 0.9 + vy * 0.3], elL: [0.7, 0, 0], elR: [0.7, 0, 0],
      hipL: [0.9 + tuck * 0.5, 0, -0.08], hipR: [0.2 - tuck * 0.1, 0, 0.08], knL: [-1.5 - tuck * 0.3, 0, 0], knR: [-0.6, 0, 0], ftL: [0.3, 0, 0], ftR: [0.2, 0, 0],
    }]);
    poses.push([this.w.slide, {
      pelvis: [0.5, 0.3, 0], spine: [0.05, 0, 0], chest: [0.1, -0.2, 0], neck: [-0.1, 0.2, 0],
      shL: [-0.6, 0, -0.6], shR: [0.6, 0, 0.9], elL: [0.4, 0, 0], elR: [1.0, 0, 0],
      hipL: [1.45, 0, -0.1], hipR: [0.25, 0, 0.25], knL: [-0.1, 0, 0], knR: [-2.0, 0, 0], ftL: [0.5, 0, 0], ftR: [0.2, 0, 0],
    }]);
    poses.push([this.w.fly, {
      pelvis: [0, 0, 0], spine: [0.05, 0, 0], chest: [0.05, 0, 0], neck: [-1.1, 0, 0],
      shL: [-0.25, 0, -0.25], shR: [2.9, 0, 0.08], elL: [0.25, 0, 0], elR: [0.05, 0, 0],
      hipL: [-0.08 + Math.sin(time * 3) * 0.04, 0, -0.03], hipR: [0.05 - Math.sin(time * 3) * 0.04, 0, 0.03], knL: [-0.15, 0, 0], knR: [-0.35, 0, 0], ftL: [-0.7, 0, 0], ftR: [-0.6, 0, 0],
    }]);
    // boosting through the air: streamlined dive, arms swept back
    this.dive = damp(this.dive, s.boosting && !s.flying && !s.sliding ? (airborne ? 1 : 0.6) : 0, 8, dt);
    poses.push([this.dive * 3, {
      pelvis: [-0.75, 0, 0], spine: [-0.1, 0, 0], chest: [-0.1, 0, 0], neck: [0.7, 0, 0],
      shL: [-0.7, 0, -0.35], shR: [-0.7, 0, 0.35], elL: [0.3, 0, 0], elR: [0.3, 0, 0],
      hipL: [-0.15, 0, -0.05], hipR: [0.1, 0, 0.05], knL: [-0.5, 0, 0], knR: [-0.9, 0, 0], ftL: [-0.6, 0, 0], ftR: [-0.5, 0, 0],
    }]);
    let wsum = 0;
    for (const [w] of poses) wsum += w;
    for (const j of JOINTS) {
      const out: [number, number, number] = [0, 0, 0];
      for (const [w, pose] of poses) { const r = pose[j]; if (!r) continue; out[0] += r[0] * w; out[1] += r[1] * w; out[2] += r[2] * w; }
      this.cur[j] = [out[0] / wsum, out[1] / wsum, out[2] / wsum];
    }
    for (const j of JOINTS) this.bones[j].rotation.set(...this.cur[j]);

    // ---- body orientation
    const flyPitch = s.flying && speed > 1 ? -Math.PI / 2 + Math.atan2(s.vel.y, Math.max(0.1, hs)) * 0.9 : 0;
    const wantPitch = s.flying ? lerp(-0.25, flyPitch, Math.min(1, speed / 8)) : -this.lean.x * 0.25;
    this.bodyPitch = damp(this.bodyPitch, wantPitch, 8, dt);
    this.bodyRoll = damp(this.bodyRoll, -this.lean.z * (s.flying ? 0.7 : 0.3), 8, dt);
    const bob = this.w.run * Math.abs(Math.sin(ph)) * 0.06 * stride;
    this.crouchY = damp(this.crouchY, this.w.slide * -0.42 + this.w.air * 0.05, 14, dt);
    this.root.position.set(s.pos.x, s.pos.y - 0.9 + bob + this.crouchY, s.pos.z);
    this.root.rotation.set(0, this.yaw, 0);
    // pitch/roll around the body centre (pelvis), so flying rotates the whole figure
    const pel = this.bones.pelvis;
    pel.rotation.x += this.bodyPitch;
    pel.rotation.z += this.bodyRoll;
    if (s.flying) pel.position.y = damp(pel.position.y, 0.98 - 0.0, 8, dt);

    // ---- arm overrides: tethers & ring casting point the arm at the target
    this.root.updateMatrixWorld(true);
    this.armOverride.L = damp(this.armOverride.L, s.tetherL ? 1 : 0, 18, dt);
    this.armOverride.R = damp(this.armOverride.R, s.tetherR || s.castTarget ? 1 : 0, 18, dt);
    if (this.armOverride.L > 0.01) this.aimArm('L', s.tetherL ?? this.handL, this.armOverride.L);
    if (this.armOverride.R > 0.01) this.aimArm('R', s.tetherR ?? s.castTarget ?? this.handR, this.armOverride.R);
    this.root.updateMatrixWorld(true);

    // ---- write instances
    const counters: Record<Prim, number> = { capsule: 0, sphere: 0, box: 0 };
    const m = new Matrix4();
    for (const p of this.pieces) {
      m.multiplyMatrices(p.bone.matrixWorld, p.local);
      this.meshes[p.prim].setMatrixAt(counters[p.prim]++, m);
    }
    for (const im of Object.values(this.meshes)) im.instanceMatrix.needsUpdate = true;
    this.bones.haL.getWorldPosition(this.handL);
    this.bones.haR.getWorldPosition(this.handR);
    this.handL.y -= 0.05; this.handR.y -= 0.05;
    this.bones.chest.localToWorld(this.back.set(0, 0.05, 0.2));
    this.feet.copy(this.root.position);
    this.ring.rotation.y = time * 2;
  }

  /** Rotate shoulder so the arm (-Y) points at a world target; elbow straightens. */
  private aimArm(side: 'L' | 'R', target: Vector3, weight: number) {
    const sh = this.bones['sh' + side];
    const el = this.bones['el' + side];
    const shoulderW = sh.getWorldPosition(new Vector3());
    const dirW = target.clone().sub(shoulderW);
    if (dirW.lengthSq() < 1e-6) return;
    dirW.normalize();
    const parentQ = sh.parent!.getWorldQuaternion(new Quaternion());
    const dirLocal = dirW.applyQuaternion(parentQ.invert());
    const q = new Quaternion().setFromUnitVectors(new Vector3(0, -1, 0), dirLocal);
    sh.quaternion.slerp(q, weight);
    el.rotation.x = lerp(el.rotation.x, 0.08, weight);
  }
}
