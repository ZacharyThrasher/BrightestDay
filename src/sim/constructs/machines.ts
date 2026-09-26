/**
 * Machines & creatures: articulation, locomotion, autonomy, weapons and riding for constructs.
 *
 * Every construct with joints or machine behaviours ('drive' | 'brain' | 'weapon' | 'rideable') gets a MachineDef.
 * Movement is a kinematic controller with its own collision queries (ray probes for terrain, a swept footprint
 * box for walls/buildings), so it is deterministic and never tunnels. Loose dynamic objects are pushed by the
 * kinematic body through the normal contact solver. Weapons use the world's projectile system (friendly shots
 * never hit their owner) or instant beams/cones/bites. All machine damage goes through one attributed path
 * (dealDamage). Targets are resolved behind one abstraction (TargetRef: an entity or another owner's construct;
 * a 'player' kind slots in there for PvP).
 */
import { Vector3, Quaternion } from 'three';
import { G, groups, type RBody } from '../physics.js';
import type { World, Entity, Projectile, Owner } from '../world.js';
import type { ConstructRuntime, Construct, Inst } from './runtime.js';
import type { Behavior, Ref } from './spec.js';
import { partTransform, specExtent } from './geom.js';
import { prepareJoints, newJointState, evaluateJoints, isAncestor, type JointRT, type JointState, type Pose } from './joints.js';
import { clampMachineBehavior } from './machine-spec.js';
import { COMBAT, interceptPoint, steerToward } from './combat.js';

type Drive = Extract<Behavior, { type: 'drive' }>;
type Brain = Extract<Behavior, { type: 'brain' }>;
type Weapon = Extract<Behavior, { type: 'weapon' }>;
type Ride = Extract<Behavior, { type: 'rideable' }>;
export type BrainGoal = Brain['goal'];

const D2R = Math.PI / 180;
const GRAV = 22;
/** terrain + obstacles for machine movement queries (world and solid constructs; not ghosts, entities, cargo, player) */
const QF = groups(G.CONSTRUCT, G.WORLD | G.CONSTRUCT);
/** line of sight blockers */
const LOSF = groups(G.CONSTRUCT, G.WORLD);
/** what beams hit */
const BEAMF = groups(G.CONSTRUCT, G.WORLD | G.CONSTRUCT | G.ENTITY | G.CARGO);

const clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v);
const wrap = (a: number) => { a = (a + Math.PI) % (Math.PI * 2); if (a < 0) a += Math.PI * 2; return a - Math.PI; };

function rngFrom(seedStr: string, k: number) {
  let seed = k * 7919 + 17;
  for (let i = 0; i < seedStr.length; i++) seed = (Math.imul(seed, 31) + seedStr.charCodeAt(i)) | 0;
  return () => {
    seed |= 0; seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

interface Muzzle { joint: number; local: Vector3; group: string | null }

/** Something a machine can fight. (PvP: add { kind: 'player'; id } here and in tPos/tVel/tRadius/dealDamage.) */
export type TargetRef = { kind: 'entity' | 'construct'; id: string };

export interface MachineDef {
  drive: Drive | null;
  brain: Brain | null;
  weapons: Weapon[];
  ride: Ride | null;
  joints: JointRT[];
  partJoint: number[];
  muzzles: Muzzle[];
  wMuzzles: number[][]; // per weapon: muzzle indices ([] = construct front)
  wAim: number[]; // per weapon: aim joint that points it (-1 none)
  wBody: boolean[]; // per weapon: melee with no group = the whole construct hits what it touches
  ext: ReturnType<typeof specExtent>;
  hx: number; hz: number; stepH: number;
  boxLow: any; boxLowY: number; boxFull: any; boxFullY: number;
  seat: Vector3 | null;
  carLike: boolean; narrow: boolean; fixedWing: boolean;
  maxRange: number; melee: Weapon | null; ranged: boolean;
  front: Vector3;
  /** projectile speed aim joints lead with (first ranged weapon) */
  aimSpeed: number;
}

export interface MachineState {
  js: JointState | null;
  heading: number; speed: number; yawRate: number; vy: number; onGround: boolean;
  hv: Vector3; vel: Vector3;
  travel: number; gaitT: number; gaitAmp: number; steer: number;
  pitch: number; roll: number;
  altY: number | null;
  fireAt: Float64Array; cds: Float64Array; alt: Int32Array;
  goal: BrainGoal | 'none';
  target: TargetRef | null;
  aimPoint: Vector3 | null; aimErr: number; jointErr: Float64Array;
  home: Vector3; wp: Vector3 | null; wait: number; rng: () => number; patrol: Vector3[]; pi: number;
  fetch: { id: string; carrying: boolean } | null;
  stuck: number; detour: number; detourSign: number; progT: number; progWant: number; progPos: Vector3;
  climbBoost: number;
  rider: boolean;
  /** last time the rider pressed fire: while they aim, weapons/aim joints follow the crosshair, otherwise the brain gunner */
  riderAimAt: number;
  /** idle-with-target: keep aim joints on the target but don't fire */
  holdFire: boolean;
  shots: number; hits: number;
  /** owner's mouse command (actions.ts): aim joints/weapons follow `aim` until `until`; fire while `fire`; precise = snap aim */
  cmd?: { aim: Vector3; fire: boolean; until: number; precise: boolean } | null;
}

/** chase: going for an enemy — capped at COMBAT.chaser speed / turn rate so a mobile target can shake it */
interface Nav { point: Vector3 | null; speed: number; arrive: number; face: Vector3 | null; alt: number | null; jump: boolean; chase?: boolean }
interface Control { throttle: number; yawRate: number; heading: number | null; hvel: Vector3; alt: number | null; climb: number; jump: boolean; boost: boolean; land: boolean; chase?: boolean }

/** How fast a weapon's shot travels (instant beams / melee: Infinity). */
function shotSpeed(wp: Weapon): number {
  if (wp.kind === 'laser' || wp.kind === 'flame' || wp.kind === 'melee') return Infinity;
  if (wp.kind === 'missile') return Math.min(Math.max(10, wp.speed || 50), COMBAT.missile.maxSpeed);
  if (wp.kind === 'bomb') return Math.min(30, Math.max(10, wp.speed || 30));
  return Math.max(10, wp.speed || 50);
}

/** Tuning switches (benchmarks). syncColliders: keep colliders of moving solid parts on their parts. */
export const MACHINE_OPTS = { syncColliders: true };

const _v = new Vector3(), _v2 = new Vector3(), _v3 = new Vector3(), _q = new Quaternion(), _q2 = new Quaternion(), _qi = new Quaternion();
const UP = new Vector3(0, 1, 0), X = new Vector3(1, 0, 0), Z = new Vector3(0, 0, 1);
const IDENT = { x: 0, y: 0, z: 0, w: 1 };

export class MachineSystem {
  private prevUse = false;
  private aimTick = -1;
  private aimCache = new Vector3();
  private hostileTick = -1;
  private enemyCache = new Map<string, TargetRef[]>();
  private ray: any = null;
  /** homing shots: projectile id → target */
  missiles = new Map<number, { target: TargetRef; speed: number }>();
  /** ram cooldowns: construct id + target id → time */
  private rams = new Map<string, number>();

  constructor(private w: World, private rt: ConstructRuntime) {}

  // ------------------------------------------------------------------ setup

  /** (Re)build a construct's machine definition + per-instance state. Called after every (re)build. */
  init(c: Construct) {
    const s = c.spec;
    const find = <T extends Behavior['type']>(t: T) => s.behaviors.find((b) => b.type === t) as Extract<Behavior, { type: T }> | undefined;
    const drive = find('drive') ?? null, brain = find('brain') ?? null, ride = find('rideable') ?? null;
    const weapons = s.behaviors.filter((b): b is Weapon => b.type === 'weapon');
    const joints = prepareJoints(s);
    if (!drive && !brain && !weapons.length && !ride && !joints.length) {
      c.mach = null;
      for (const i of c.insts) { i.m = null; i.jointPose = undefined; }
      return;
    }
    const jIdx = new Map(joints.map((r) => [r.j.group, r.index]));
    const partJoint = s.parts.map((p) => (p.group ? jIdx.get(p.group) ?? -1 : -1));
    const muzzles: Muzzle[] = [];
    s.parts.forEach((p, i) => {
      if (p.role !== 'muzzle') return;
      muzzles.push({ joint: partJoint[i], local: partTransform(p, s.scale, s.stretch).pos, group: p.group ?? null });
    });
    if (!muzzles.length) {
      // fall back to the far end of barrels
      s.parts.forEach((p, i) => {
        if (p.role !== 'barrel' && p.role !== 'cannon') return;
        const t = partTransform(p, s.scale, s.stretch);
        muzzles.push({ joint: partJoint[i], local: new Vector3(0, t.size.y / 2, 0).applyQuaternion(t.quat).add(t.pos), group: p.group ?? null });
      });
    }
    const wBody = weapons.map((wp) => wp.kind === 'melee' && (!wp.group || !s.parts.some((p) => p.group === wp.group)));
    const wMuzzles = weapons.map((wp, wi) => {
      if (wBody[wi]) return [];
      const gj = wp.group ? jIdx.get(wp.group) : undefined;
      const list = muzzles.map((_, i) => i).filter((i) => {
        const m = muzzles[i];
        if (!wp.group) return true;
        if (m.group === wp.group) return true;
        return gj !== undefined && m.joint >= 0 && (m.joint === gj || isAncestor(joints, gj, m.joint));
      });
      return list.length ? list : wp.group ? muzzles.map((_, i) => i) : list;
    });
    const wAim = weapons.map((wp, wi) => {
      let j = wMuzzles[wi].length ? muzzles[wMuzzles[wi][0]].joint : wp.group ? jIdx.get(wp.group) ?? -1 : -1;
      let guard = 0;
      while (j >= 0 && guard++ < 32) { if (joints[j].aim) return j; j = joints[j].parent; }
      return -1;
    });
    const ext = specExtent(s);
    const R = this.w.R;
    const hx = Math.max(0.2, (ext.size.x / 2) * 0.85), hz = Math.max(0.2, (ext.size.z / 2) * 0.85);
    const hover = drive?.mode === 'hover';
    const stepH = hover ? (drive!.altitude ?? 1.2) + 0.35 : clamp(ext.size.y * 0.3, 0.35, 1.5);
    const hyLow = Math.max(0.1, (ext.size.y - stepH) / 2);
    const hyFull = Math.max(0.1, (ext.size.y - 0.3) / 2);
    const motions = joints.flatMap((r) => r.j.motion);
    const ranged = weapons.filter((wp) => wp.kind !== 'melee');
    c.mach = {
      drive, brain, weapons, ride, joints, partJoint, muzzles, wMuzzles, wAim, wBody, ext, hx, hz, stepH,
      boxLow: new R.Cuboid(hx, hyLow, hz), boxLowY: ext.min.y + stepH + hyLow,
      boxFull: new R.Cuboid(hx, hyFull, hz), boxFullY: ext.min.y + 0.3 + hyFull,
      seat: ride ? new Vector3(ride.seat[0] * (s.stretch?.[0] ?? 1), ride.seat[1] * (s.stretch?.[1] ?? 1), ride.seat[2] * (s.stretch?.[2] ?? 1)).multiplyScalar(s.scale) : null,
      carLike: motions.some((m) => m.kind === 'roll') && motions.some((m) => m.kind === 'steer'),
      narrow: motions.some((m) => m.kind === 'roll') && ext.size.x < 1.4,
      fixedWing: !!drive && drive.mode === 'air' && (drive.minSpeed ?? 0) > 0,
      maxRange: ranged.reduce((a, wp) => Math.max(a, wp.range), 0),
      melee: weapons.find((wp) => wp.kind === 'melee') ?? null,
      ranged: ranged.length > 0,
      front: new Vector3(ext.center.x, ext.center.y, ext.max.z),
      aimSpeed: ranged.length ? shotSpeed(ranged[0]) : Infinity,
    };
    const P = this.w.player;
    c.insts.forEach((inst, k) => {
      const fwd = _v.set(0, 0, 1).applyQuaternion(inst.quat);
      const js = joints.length ? newJointState(joints) : null;
      const prev = inst.m;
      const m: MachineState = {
        js, heading: prev?.heading ?? Math.atan2(fwd.x, fwd.z), speed: prev?.speed ?? 0, yawRate: 0, vy: 0, onGround: false,
        hv: prev?.hv.clone() ?? new Vector3(), vel: new Vector3(),
        travel: prev?.travel ?? 0, gaitT: prev?.gaitT ?? 0, gaitAmp: 0, steer: 0, pitch: 0, roll: 0, altY: prev?.altY ?? null,
        fireAt: new Float64Array(joints.length).fill(-Infinity), cds: new Float64Array(weapons.length).fill(0.4 + 0.15 * k), alt: new Int32Array(weapons.length),
        goal: brain?.goal ?? 'none', target: null, aimPoint: null, aimErr: 180, jointErr: new Float64Array(joints.length).fill(180),
        home: inst.pos.clone(), wp: null, wait: 0, rng: rngFrom(c.id, k), patrol: [], pi: 0, fetch: null,
        stuck: 0, detour: 0, detourSign: k % 2 ? -1 : 1, progT: 0, progWant: 0, progPos: inst.pos.clone(), climbBoost: 0,
        rider: !!(P.riding && P.riding.id === c.id && k === 0), riderAimAt: -Infinity, holdFire: false, shots: prev?.shots ?? 0, hits: prev?.hits ?? 0,
      };
      inst.m = m;
      inst.jointPose = js?.poses;
      inst.colJoint = (inst.colPart ?? []).map((pi) => (pi >= 0 ? partJoint[pi] ?? -1 : -1));
      this.resetBrain(c, inst, m);
    });
    if (P.riding?.id === c.id) {
      if (!ride || !c.insts[0]) this.dismount();
      else P.riding.body = c.insts[0].body;
    }
  }

  private resetBrain(c: Construct, inst: Inst, m: MachineState) {
    const b = c.mach?.brain;
    m.goal = b?.goal ?? 'none';
    m.target = null;
    m.wp = null;
    m.fetch = null;
    m.home = inst.pos.clone();
    if (!b) return;
    const tp = b.target && b.target.kind !== 'self' && b.target.kind !== 'nearest_enemy' && b.target.kind !== 'all_enemies' && b.target.kind !== 'threat' ? this.rt.refPos(b.target) : null;
    if ((b.goal === 'guard' || b.goal === 'goto') && tp) m.home = tp.clone();
    if (b.goal === 'patrol') {
      const o = inst.pos.clone();
      if (tp) m.patrol = [o, tp.clone()];
      else { const r = 10; m.patrol = [[r, r], [r, -r], [-r, -r], [-r, r]].map(([x, z]) => o.clone().add(new Vector3(x, 0, z))); }
      m.pi = 0;
    }
  }

  // ------------------------------------------------------------------ per tick

  /** Start of the construct pre-step: get in/out, homing shots. */
  preStep(_dt: number) {
    const inp = this.w.input;
    const use = !!inp.use;
    if (use && !this.prevUse) {
      if (this.w.player.riding) this.dismount();
      else { const c = this.nearestRideable(6); if (c) this.ride(c.id); }
    }
    this.prevUse = use;
    for (const [pid, h] of this.missiles) {
      const p = this.w.projectiles.get(pid);
      if (!p) { this.missiles.delete(pid); continue; }
      const tp = this.tPos(h.target, _v);
      if (!tp) continue;
      const t = p.body.translation();
      const v = p.body.linvel();
      const from = _v2.set(t.x, t.y, t.z);
      const aimP = interceptPoint(from, h.speed, tp, this.tVel(h.target, new Vector3()), new Vector3());
      const vel = steerToward(_v3.set(v.x, v.y, v.z), aimP.sub(from), (COMBAT.missile.turnRate * D2R) * _dt, h.speed);
      p.body.setLinvel(vel, true);
    }
  }

  /** Move a machine instance (kinematic body with a 'drive'). */
  drive(c: Construct, inst: Inst, dt: number) {
    const def = c.mach!;
    const m = inst.m!;
    const d = def.drive!;
    const ridden = m.rider && this.w.player.riding?.id === c.id;
    if (ridden) c.expires = Math.max(c.expires, this.w.time + 10);
    let nav: Nav | null = null;
    if (!ridden) nav = this.think(c, inst, m, dt);
    else if (!this.riderAiming(m) && def.brain) this.acquire(c, inst, m);
    const ctl = ridden ? this.riderControl(c, inst, m) : this.navControl(c, inst, m, nav!, dt);
    const p0 = _v3.copy(inst.pos);
    const px = p0.x, py = p0.y, pz = p0.z;
    if (d.mode === 'air') this.stepAir(c, inst, m, ctl, dt, ridden);
    else this.stepGround(c, inst, m, ctl, dt);
    m.vel.set((inst.pos.x - px) / dt, (inst.pos.y - py) / dt, (inst.pos.z - pz) / dt);
    this.ram(c, inst, m);
    inst.body.setNextKinematicTranslation(inst.pos);
    inst.body.setNextKinematicRotation(inst.quat);
    // progress watchdog (brain): commanded to drive but not getting anywhere → back off and detour
    if (!ridden && nav?.point && m.detour <= 0) {
      m.progT += dt;
      m.progWant += (d.mode === 'air' ? ctl.hvel.length() : Math.max(0, ctl.throttle) * d.speed) * dt;
      if (m.progT > 0.8) {
        const moved = Math.hypot(inst.pos.x - m.progPos.x, inst.pos.z - m.progPos.z);
        if (m.progWant > 1.5 && moved < m.progWant * 0.25) { m.stuck++; m.detour = 0.9; m.detourSign = -m.detourSign; m.climbBoost = 2; }
        else m.stuck = 0;
        m.progT = 0;
        m.progWant = 0;
        m.progPos.copy(inst.pos);
      }
    } else if (!nav?.point) { m.progT = 0; m.progWant = 0; m.progPos.copy(inst.pos); }
  }

  /** Joints, colliders, weapons (every construct with a MachineDef, moving or not). */
  animate(c: Construct, inst: Inst, dt: number) {
    const def = c.mach!;
    const m = inst.m;
    if (!m) return;
    const ridden = m.rider && this.w.player.riding?.id === c.id;
    const riderAiming = ridden && this.riderAiming(m);
    const cmd = m.cmd && m.cmd.until >= this.w.time ? m.cmd : null;
    if (!def.drive && !ridden && def.brain) this.think(c, inst, m, dt); // stationary brains still pick targets
    else if (!def.drive && ridden && !riderAiming && def.brain) this.acquire(c, inst, m);
    // what aim joints track: the owner's mouse command, the rider's crosshair while they aim, else the brain's target, else the crosshair
    let aimP: Vector3 | null = null;
    if (cmd) aimP = cmd.aim;
    else if (riderAiming) aimP = this.playerAim();
    else if (m.target) aimP = this.lead(inst, m.target, def.aimSpeed);
    else if (ridden) aimP = this.playerAim();
    if (!aimP && !def.brain && !ridden && def.joints.some((j) => j.aim)) aimP = this.playerAim();
    if (!aimP && def.brain && m.goal === 'follow' && def.joints.some((j) => j.aim)) aimP = this.playerAim();
    m.aimPoint = aimP ? (m.aimPoint ?? new Vector3()).copy(aimP) : null;
    if (m.js) {
      const t = this.w.time - c.born;
      const self = this;
      const time = this.w.time;
      evaluateJoints(def.joints, m.js, {
        t, travel: m.travel, gait: m.gaitT, gaitAmp: m.gaitAmp, steer: m.steer,
        fireAge: (j) => time - m.fireAt[j],
        aim: (rt, pivot, parentQ, cur) => self.aimJoint(rt, pivot, parentQ, cur, m, dt),
      }, c.spec.scale, inst.pos, inst.quat);
      // aim error of leaf aim joints (how well something points at the target)
      let best = 180;
      for (const rt of def.joints) {
        if (!rt.aim) continue;
        let err = 180;
        if (m.aimPoint) {
          const fwd = _v.set(0, 0, 1).applyQuaternion(m.js.worldQ[rt.index]);
          const to = _v2.copy(m.aimPoint).sub(m.js.worldP[rt.index]).normalize();
          if (!rt.aim.pitch) { fwd.y = 0; to.y = 0; fwd.normalize(); to.normalize(); }
          err = Math.acos(clamp(fwd.dot(to), -1, 1)) / D2R;
        }
        m.jointErr[rt.index] = err;
        if (rt.aimLeaf) best = Math.min(best, err);
      }
      m.aimErr = best;
      if (MACHINE_OPTS.syncColliders) this.syncColliders(inst, m.js, def);
    }
    if (def.weapons.length) this.weaponsStep(c, inst, m, dt, ridden);
  }

  /** After all constructs moved: put the rider on the seat. */
  postMove() {
    const P = this.w.player;
    if (!P.riding) return;
    const c = this.rt.get(P.riding.id);
    const inst = c?.insts[0];
    if (!c || !inst || !inst.alive || !c.mach?.seat) { P.riding = null; P.vel.set(0, 0, 0); return; }
    P.pos.copy(c.mach.seat).applyQuaternion(inst.quat).add(inst.pos);
    P.vel.copy(inst.m?.vel ?? _v.set(0, 0, 0));
  }

  private syncColliders(inst: Inst, js: JointState, def: MachineDef) {
    const cj = inst.colJoint;
    if (!cj || !inst.colBase) return;
    for (let k = 0; k < inst.colliders.length; k++) {
      const j = cj[k];
      if (j === undefined || j < 0) continue;
      const base = inst.colBase[k];
      const pose = js.poses.get(def.joints[j].j.group);
      if (!pose || !base) continue;
      const col = inst.colliders[k];
      _v.copy(base.t).applyQuaternion(pose.quat).add(pose.pos);
      _q.copy(pose.quat).multiply(base.r);
      col.setTranslationWrtParent(_v);
      col.setRotationWrtParent(_q);
    }
  }

  // ------------------------------------------------------------------ aiming

  /** Is the rider aiming right now (fired within the last 1.5 s)? */
  private riderAiming(m: MachineState): boolean {
    const inp = this.w.input;
    if (inp.primary || inp.hookL || inp.hookR) m.riderAimAt = this.w.time;
    return this.w.time - m.riderAimAt < 1.5;
  }

  /** Brain target acquisition only (no movement) — the gunner of a ridden machine. */
  private acquire(c: Construct, inst: Inst, m: MachineState) {
    const b = c.mach?.brain;
    if (!b || !c.mach!.weapons.length || b.goal === 'flee' || b.goal === 'fetch') { m.target = null; return; }
    const range = b.range ?? 40;
    m.holdFire = b.goal === 'idle';
    m.target = this.resolveTarget(c, b.target, inst.pos) ?? (b.goal === 'idle' ? null : this.nearestEnemy(c, inst.pos, range));
  }

  private playerAim(): Vector3 {
    if (this.aimTick !== this.w.tick) { this.aimCache.copy(this.w.player.aim(250).point); this.aimTick = this.w.tick; }
    return this.aimCache;
  }

  /** Where to aim at a target: predictive intercept for a shot of the given speed (Infinity = where it is now). */
  private lead(inst: Inst, t: TargetRef, speed = 60, from?: Vector3): Vector3 | null {
    const p = this.tPos(t, new Vector3());
    if (!p || !isFinite(speed)) return p;
    return interceptPoint(from ?? inst.pos, speed, p, this.tVel(t, _v2), p);
  }

  private aimJoint(rt: JointRT, pivot: Vector3, parentQ: Quaternion, cur: { yaw: number; pitch: number }, m: MachineState, dt: number) {
    const a = rt.aim!;
    let yawD = cur.yaw, pitchD = cur.pitch;
    if (m.aimPoint) {
      const d = _v.copy(m.aimPoint).sub(pivot).applyQuaternion(_qi.copy(parentQ).invert());
      if (a.yaw) yawD = Math.atan2(d.x, d.z);
      // pitch measured in the frame after this joint's own yaw
      const flat = a.yaw ? Math.hypot(d.x, d.z) : Math.hypot(d.x, d.z);
      if (a.pitch) pitchD = clamp(Math.atan2(d.y, Math.max(1e-4, flat)), a.min, a.max);
    } else {
      // nothing to look at: drift back to rest
      yawD = 0; pitchD = 0;
    }
    const step = m.cmd?.precise && m.cmd.until >= this.w.time ? Infinity : a.rate * dt;
    if (a.yaw) cur.yaw = wrap(cur.yaw + clamp(wrap(yawD - cur.yaw), -step, step));
    if (a.pitch) cur.pitch = cur.pitch + clamp(pitchD - cur.pitch, -step, step);
  }

  // ------------------------------------------------------------------ brains

  // ------------------------------------------------------------------ targets (one place to resolve them)

  /** Everything hostile to `owner`: hostile creatures/turrets, and constructs belonging to someone else. */
  enemiesOf(owner: string): TargetRef[] {
    if (this.hostileTick !== this.w.tick) { this.enemyCache.clear(); this.hostileTick = this.w.tick; }
    let list = this.enemyCache.get(owner);
    if (!list) {
      list = [];
      for (const e of this.w.entities.values()) if (e.hostile && !e.disabled && !e.heldBy) list.push({ kind: 'entity', id: e.id });
      for (const c of this.rt.list.values()) if (c.owner !== owner && c.insts[0]?.alive) list.push({ kind: 'construct', id: c.id });
      this.enemyCache.set(owner, list);
    }
    return list;
  }

  tPos(t: TargetRef, out = new Vector3()): Vector3 | null {
    if (t.kind === 'entity') { const e = this.w.entities.get(t.id); return e && !e.disabled ? this.w.entityPos(e, out) : null; }
    const c = this.rt.get(t.id);
    const i = c?.insts[0];
    return c && i?.alive ? out.copy(c.extent.center).applyQuaternion(i.quat).add(i.pos) : null;
  }

  private tVel(t: TargetRef, out = new Vector3()): Vector3 {
    if (t.kind === 'entity') { const e = this.w.entities.get(t.id); return e ? this.w.entityVel(e, out) : out.set(0, 0, 0); }
    const i = this.rt.get(t.id)?.insts[0];
    return i ? out.copy(i.vel) : out.set(0, 0, 0);
  }

  private tRadius(t: TargetRef): number {
    if (t.kind === 'entity') return this.w.entities.get(t.id)?.radius ?? 0.5;
    const c = this.rt.get(t.id);
    return c ? Math.max(0.5, c.radius * 0.6) : 0.5;
  }

  private tBody(t: TargetRef): RBody | null {
    if (t.kind === 'entity') return this.w.entities.get(t.id)?.body ?? null;
    return this.rt.get(t.id)?.insts[0]?.body ?? null;
  }

  private nearestEnemy(c: Construct, from: Vector3, range: number, reachableOnly = false): TargetRef | null {
    let best: TargetRef | null = null, bd = range;
    for (const t of this.enemiesOf(c.owner)) {
      const p = this.tPos(t, _v);
      if (!p) continue;
      if (reachableOnly && !this.reachable(c, t, p)) continue;
      const d = p.distanceTo(from);
      if (d < bd) { bd = d; best = t; }
    }
    return best;
  }

  /** Can a melee machine get at it (not hovering out of reach)? */
  private reachable(c: Construct, t: TargetRef, p: Vector3): boolean {
    const d = c.mach?.drive;
    if (!d || d.mode === 'air') return true;
    const g = this.groundY(p.x, p.y + 0.5, p.z, 40, this.tBody(t)) ?? p.y - 1;
    return p.y - this.tRadius(t) - g < 1.2 + (d.jump ? (d.jump * d.jump) / (2 * GRAV) : 0) + c.extent.size.y * 0.5;
  }

  /**
   * THE target resolver: turns an order's Ref into something to fight ("attack that drone", "smash his wall",
   * "get him"). Never resolves to our own owner or the owner's stuff. PvP: 'player' / 'opponent' resolve to
   * lanterns here once World has several players; until then 'opponent' means the nearest hostile.
   */
  resolveTarget(c: Construct, ref: Ref | undefined, from?: Vector3): TargetRef | null {
    if (!ref) return null;
    const at = from ?? c.insts[0]?.pos ?? this.w.player.pos;
    switch (ref.kind) {
      case 'entity': { const e = this.w.entities.get(ref.id); return e && !e.disabled ? { kind: 'entity', id: e.id } : null; }
      case 'entities': { for (const id of ref.ids) { const e = this.w.entities.get(id); if (e && !e.disabled) return { kind: 'entity', id }; } return null; }
      case 'construct': { const o = this.rt.get(ref.id); return o && o.id !== c.id && o.owner !== c.owner ? { kind: 'construct', id: o.id } : null; }
      case 'opponent': case 'nearest_enemy': case 'all_enemies': case 'threat': return this.nearestEnemy(c, at, 1e9);
      case 'player': return null; // single player: the only lantern is our owner
      default: return null;
    }
  }

  /**
   * THE damage path for machines: every hit from a machine's weapons, rams and splash lands here, attributed to
   * the construct (and its owner). Never hurts the owner or the owner's constructs.
   */
  dealDamage(src: Construct, t: TargetRef, amount: number, cause: string, dir?: Vector3, knock = 0) {
    if (amount <= 0) return;
    if (t.kind === 'entity') {
      const e = this.w.entities.get(t.id);
      if (!e || e.heldBy === src.id) return;
      this.w.damageEntity(e, amount, cause, src.id);
      if (knock > 0 && dir && e.body.isDynamic()) {
        const k = Math.min(9, 2 + amount * 0.15) * knock;
        e.body.applyImpulse(_v3.copy(dir).setY(Math.max(0.15, dir.y)).normalize().multiplyScalar(k * e.body.mass()), true);
        if (e.type === 'drone') e.stunnedUntil = Math.max(e.stunnedUntil, this.w.time + 0.4);
      }
    } else {
      const o = this.rt.get(t.id);
      if (!o || o.owner === src.owner) return;
      this.rt.damageConstruct(o.id, amount, cause, src.id);
    }
    for (const i of src.insts) if (i.m) { i.m.hits++; break; }
    this.w.emit('machine_damage', { by: src.id, owner: src.owner, target: t.kind, id: t.id, amount: Math.round(amount), cause });
  }

  /** Decide what to do this tick. Also sets m.target (for weapons/aim). */
  private think(c: Construct, inst: Inst, m: MachineState, dt: number): Nav {
    const def = c.mach!;
    const b = def.brain;
    const P = this.w.player;
    const pos = inst.pos;
    const nav: Nav = { point: null, speed: 1, arrive: 1, face: null, alt: null, jump: false };
    if (m.detour > 0) m.detour -= dt;
    if (m.climbBoost > 0) m.climbBoost -= dt;
    if (!b) {
      m.target = null;
      // a machine with legs/wheels and no orders or rider potters about; vehicles wait for you
      if (def.drive && !def.ride) return this.wander(c, inst, m, nav, dt, m.home, 14);
      return nav;
    }
    const range = b.range ?? (b.goal === 'follow' ? 25 : b.goal === 'guard' ? 22 : 40);
    const armed = def.weapons.length > 0;
    m.holdFire = false;
    switch (m.goal) {
      case 'idle': m.target = b.target ? this.resolveTarget(c, b.target, pos) : null; m.holdFire = true; return nav;
      case 'wander': {
        m.target = armed ? this.nearestEnemy(c, pos, range) : null;
        return this.wander(c, inst, m, nav, dt, m.home, 20);
      }
      case 'follow': {
        const leader = (b.target && b.target.kind !== 'self' ? this.rt.refPos(b.target) : null) ?? P.pos;
        let tgt = armed ? this.nearestEnemy(c, pos, range, !!def.melee && !def.ranged) : null;
        const tgtP = tgt ? this.tPos(tgt, new Vector3()) : null;
        if (tgt && (!tgtP || tgtP.distanceTo(leader) > range + 8)) tgt = null; // leash
        m.target = tgt;
        if (tgt && def.melee && !def.ranged) return this.engage(c, inst, m, tgt, nav);
        // escort slot: behind and to the side of the leader (fans out for several)
        const k = c.insts.indexOf(inst);
        const facing = Math.atan2(-Math.sin(P.yaw), -Math.cos(P.yaw));
        const slotH = facing + Math.PI + (k - (c.insts.length - 1) / 2) * 0.6 + (def.drive?.mode === 'air' ? 0.5 : 0.35);
        const dist = 3.2 + c.radius * 0.8 + (def.drive?.mode === 'air' ? 4 : 0);
        const lv = b.target && b.target.kind !== 'self' ? null : P.vel;
        const slot = _v2.set(Math.sin(slotH) * dist, 0, Math.cos(slotH) * dist).add(leader);
        if (lv) slot.addScaledVector(_v.set(lv.x, 0, lv.z), 0.6); // aim where the leader is going
        const toLeader = Math.hypot(leader.x - pos.x, leader.z - pos.z);
        if (toLeader > dist + 2.5 || Math.hypot(slot.x - pos.x, slot.z - pos.z) > 3) {
          nav.point = slot.clone();
          nav.speed = clamp((toLeader - dist) / 5, 0.35, 1);
          nav.arrive = 0.8;
        }
        if (tgt && tgtP) nav.face = tgtP;
        if (def.drive?.mode === 'air') nav.alt = Math.max(leader.y + 4 + c.extent.size.y * 0.3, tgt && tgtP ? tgtP.y + 3 : -Infinity);
        return nav;
      }
      case 'guard': {
        const spot = m.home;
        const tgt = armed ? this.nearestEnemy(c, spot, range, !!def.melee && !def.ranged) : null;
        m.target = tgt;
        if (tgt && def.melee && !def.ranged) {
          const n = this.engage(c, inst, m, tgt, nav);
          if (n.point && n.point.distanceTo(spot) > range) n.point = spot.clone();
          return n;
        }
        if (Math.hypot(spot.x - pos.x, spot.z - pos.z) > 2.5) { nav.point = spot.clone(); nav.arrive = 1; nav.speed = 0.8; }
        if (tgt) nav.face = this.tPos(tgt, new Vector3());
        return nav;
      }
      case 'hunt': {
        let tgt = this.resolveTarget(c, b.target, pos);
        if (!tgt) tgt = this.nearestEnemy(c, pos, Math.max(range * 3, 150));
        m.target = tgt;
        if (!tgt) return this.wander(c, inst, m, nav, dt, P.pos, 18);
        return this.engage(c, inst, m, tgt, nav);
      }
      case 'fetch': return this.fetchStep(c, inst, m, nav, range);
      case 'patrol': {
        m.target = armed ? this.nearestEnemy(c, pos, range) : null;
        if (!m.patrol.length) return nav;
        const wp = m.patrol[m.pi % m.patrol.length];
        if (Math.hypot(wp.x - pos.x, wp.z - pos.z) < 2) m.pi++;
        nav.point = m.patrol[m.pi % m.patrol.length].clone();
        nav.speed = 0.6;
        nav.arrive = 0.5;
        return nav;
      }
      case 'goto': {
        m.target = null;
        if (Math.hypot(m.home.x - pos.x, m.home.z - pos.z) < 2) { m.goal = 'idle'; return nav; }
        nav.point = m.home.clone();
        nav.arrive = 1;
        if (def.drive?.mode === 'air') nav.alt = m.home.y + 4;
        return nav;
      }
      case 'flee': {
        m.target = null;
        const threat = this.resolveTarget(c, b.target, pos) ?? this.nearestEnemy(c, pos, range);
        const tp = threat ? this.tPos(threat, new Vector3()) : null;
        if (!tp) return nav;
        const away = pos.clone().sub(tp).setY(0);
        if (away.lengthSq() < 1e-4) away.set(1, 0, 0);
        nav.point = pos.clone().addScaledVector(away.normalize(), 20);
        nav.arrive = 0;
        return nav;
      }
    }
    return nav;
  }

  private wander(c: Construct, inst: Inst, m: MachineState, nav: Nav, dt: number, home: Vector3, radius: number): Nav {
    if (m.wait > 0) { m.wait -= dt; return nav; }
    if (!m.wp || Math.hypot(m.wp.x - inst.pos.x, m.wp.z - inst.pos.z) < 2 || m.stuck > 2) {
      if (m.wp) m.wait = 0.4 + m.rng() * 1.2;
      const a = m.rng() * Math.PI * 2;
      const r = radius * (0.4 + 0.6 * m.rng());
      m.wp = new Vector3(home.x + Math.sin(a) * r, home.y, home.z + Math.cos(a) * r);
      m.stuck = 0;
    }
    nav.point = m.wp.clone();
    nav.speed = 0.55;
    nav.arrive = 0.5;
    if (c.mach!.drive?.mode === 'air') nav.alt = null;
    return nav;
  }

  private engage(c: Construct, inst: Inst, m: MachineState, tgt: TargetRef, nav: Nav): Nav {
    const def = c.mach!;
    const tp = this.tPos(tgt, new Vector3());
    if (!tp) return nav;
    const rad = this.tRadius(tgt);
    const pos = inst.pos;
    const dist = Math.hypot(tp.x - pos.x, tp.z - pos.z);
    const air = def.drive?.mode === 'air';
    const ground = this.groundY(tp.x, tp.y + 0.5, tp.z, 40, this.tBody(tgt)) ?? tp.y - 1;
    const reachable = tp.y - rad - ground < 1.2 + (def.drive?.jump ? (def.drive.jump * def.drive.jump) / (2 * GRAV) : 0);
    const flier = tgt.kind === 'entity' && this.w.entities.get(tgt.id)?.type === 'drone';
    nav.face = tp;
    if (def.melee && (air || reachable) && (!def.ranged || dist < def.maxRange * 0.35 || !flier)) {
      // charge: lead the target (pursuit intercept at our chase speed)
      const chaseSpeed = Math.min(def.drive?.speed ?? 10, COMBAT.chaser.maxSpeed);
      nav.point = interceptPoint(pos, Math.max(4, chaseSpeed), tp, this.tVel(tgt, new Vector3()), new Vector3(), 1.5);
      nav.point.y = tp.y;
      nav.chase = true;
      nav.arrive = 0;
      nav.speed = 1;
      nav.jump = !air && dist < 3 + rad && tp.y - rad > pos.y + c.extent.max.y * 0.5;
      if (air) nav.alt = tp.y + 0.5 - c.extent.center.y;
      return nav;
    }
    if (!def.ranged) {
      // can't reach it: stay close underneath and look menacing
      nav.point = tp; nav.arrive = 2.5 + rad; nav.speed = 1;
      return nav;
    }
    const standoff = def.maxRange * (air ? 0.5 : 0.6);
    if (dist > def.maxRange * 0.8) {
      nav.point = tp; nav.arrive = standoff; nav.speed = 1; nav.chase = true;
    } else if (def.drive && def.joints.some((j) => j.aim)) {
      // in range: keep moving — circle the target slowly (turrets keep tracking)
      const a = Math.atan2(pos.x - tp.x, pos.z - tp.z) + m.detourSign * 0.5;
      const r = clamp(dist, standoff * 0.7, def.maxRange * 0.75);
      nav.point = new Vector3(tp.x + Math.sin(a) * r, pos.y, tp.z + Math.cos(a) * r);
      nav.arrive = 0.5;
      nav.speed = 0.35;
    }
    if (air) nav.alt = tp.y + 3;
    return nav;
  }

  private fetchStep(c: Construct, inst: Inst, m: MachineState, nav: Nav, _range: number): Nav {
    const P = this.w.player;
    const b = c.mach!.brain!;
    if (!m.fetch) {
      const r = this.resolveTarget(c, b.target);
      let e = r?.kind === 'entity' ? this.w.entities.get(r.id) ?? null : b.target?.kind === 'entity' ? this.w.entities.get(b.target.id) ?? null : null;
      if (!e) {
        let bd = 80;
        for (const x of this.w.entities.values()) {
          if (x.hostile || x.heldBy || x.type === 'turret' || x.disabled) continue;
          const d = this.w.entityPos(x, _v).distanceTo(inst.pos) + (x.type === 'crate' ? 0 : 20);
          if (d < bd) { bd = d; e = x; }
        }
      }
      if (!e) { m.goal = 'follow'; return nav; }
      m.fetch = { id: e.id, carrying: false };
    }
    const e = this.w.entities.get(m.fetch.id);
    if (!e) { m.fetch = null; m.goal = 'follow'; return nav; }
    m.target = null;
    if (!m.fetch.carrying) {
      if (e.heldBy && e.heldBy !== c.id) { m.fetch = null; m.goal = 'follow'; return nav; }
      const ep = this.w.entityPos(e, new Vector3());
      const front = _v.copy(c.mach!.front).applyQuaternion(inst.quat).add(inst.pos);
      nav.point = ep; nav.arrive = 0; nav.speed = 1;
      if (c.mach!.drive?.mode === 'air') nav.alt = ep.y + 1 - c.extent.min.y;
      if (front.distanceTo(ep) < e.radius + 1.4 || Math.hypot(ep.x - inst.pos.x, ep.z - inst.pos.z) < c.radius * 0.6 + e.radius) {
        this.rt.attachEntity(c, inst, e, front.clone().add(_v2.set(0, 0.2, 0)));
        if (inst.cargo?.entityId === e.id) { m.fetch.carrying = true; this.w.emit('grabbed', { construct: c.id, entity: e.id }); }
      }
      return nav;
    }
    // carrying: bring it to the player and drop it at their feet
    const toP = Math.hypot(P.pos.x - inst.pos.x, P.pos.z - inst.pos.z);
    if (toP < 3.2 + c.radius * 0.5) {
      this.rt.releaseEntity(e.id, new Vector3(0, 1, 0));
      inst.cargo = null;
      m.fetch = null;
      m.goal = 'follow';
      this.w.emit('fetched', { construct: c.id, entity: e.id });
      return nav;
    }
    nav.point = P.pos.clone(); nav.arrive = 2.5; nav.speed = 1;
    if (c.mach!.drive?.mode === 'air') nav.alt = P.pos.y + 3;
    return nav;
  }

  // ------------------------------------------------------------------ control

  private riderControl(c: Construct, inst: Inst, m: MachineState): Control {
    const def = c.mach!;
    const d = def.drive!;
    const inp = this.w.input;
    const P = this.w.player;
    const ctl: Control = { throttle: 0, yawRate: 0, heading: null, hvel: new Vector3(), alt: null, climb: 0, jump: !!inp.jump, boost: !!inp.boost || !!inp.secondary, land: false };
    const turn = d.turn * D2R;
    if (d.mode === 'air') {
      const camYaw = Math.atan2(-Math.sin(P.yaw), -Math.cos(P.yaw)); // heading the camera faces
      const cf = _v.set(Math.sin(camYaw), 0, Math.cos(camYaw));
      const cr = _v2.set(-cf.z, 0, cf.x); // camera right (in world, matching the player's right)
      const max = d.speed * (ctl.boost ? 1.3 : 1);
      if (def.fixedWing) {
        ctl.heading = wrap(camYaw - inp.moveX * 0.6);
        const fwd = _v3.set(Math.sin(m.heading), 0, Math.cos(m.heading));
        const min = d.minSpeed ?? 0;
        ctl.hvel.copy(fwd).multiplyScalar(min + (max - min) * (0.35 + 0.65 * Math.max(0, inp.moveZ)));
        ctl.climb = clamp(Math.sin(P.pitch) * 1.6 + (inp.jump ? 1 : 0) - (inp.descend || inp.crouch ? 1 : 0), -1, 1);
      } else {
        ctl.heading = camYaw;
        ctl.hvel.copy(cf).multiplyScalar(inp.moveZ * max).addScaledVector(_v2.set(Math.cos(P.yaw), 0, -Math.sin(P.yaw)), inp.moveX * max * 0.7);
        ctl.climb = (inp.jump ? 1 : 0) - (inp.descend || inp.crouch ? 1 : 0);
      }
      void cr;
    } else {
      ctl.throttle = inp.moveZ;
      ctl.yawRate = -inp.moveX * turn;
      if (def.carLike && m.speed < -0.5) ctl.yawRate = -ctl.yawRate;
    }
    return ctl;
  }

  private navControl(c: Construct, inst: Inst, m: MachineState, nav: Nav, dt: number): Control {
    const def = c.mach!;
    const d = def.drive!;
    const ctl: Control = { throttle: 0, yawRate: 0, heading: null, hvel: new Vector3(), alt: nav.alt, climb: 0, jump: nav.jump, boost: false, land: !def.brain && !!def.ride, chase: nav.chase };
    const pos = inst.pos;
    const turn = Math.min(d.turn, nav.chase ? COMBAT.chaser.turnRate : Infinity) * D2R;
    let desired: number | null = null;
    let dist = 0;
    if (nav.point) {
      const dx = nav.point.x - pos.x, dz = nav.point.z - pos.z;
      dist = Math.hypot(dx, dz);
      if (dist > 0.3) desired = Math.atan2(dx, dz);
    }
    if (desired === null && nav.face) desired = Math.atan2(nav.face.x - pos.x, nav.face.z - pos.z);
    if (m.detour > 0) desired = m.heading + m.detourSign * 1.4;
    if (d.mode === 'air') {
      if (nav.point && dist > nav.arrive) {
        const sp = Math.min(nav.speed * Math.min(d.speed, nav.chase ? COMBAT.chaser.maxSpeed : Infinity), (dist - nav.arrive) * 1.5 + 1);
        ctl.hvel.set(nav.point.x - pos.x, 0, nav.point.z - pos.z).normalize().multiplyScalar(sp);
      }
      ctl.heading = nav.face && dist < 10 ? Math.atan2(nav.face.x - pos.x, nav.face.z - pos.z) : desired;
      if (m.climbBoost > 0) ctl.alt = (ctl.alt ?? inst.pos.y) + 6;
      return ctl;
    }
    // ground / hover: wall feeler — steer along obstacles instead of into them
    if (desired !== null && nav.point && dist > nav.arrive + 0.5 && m.detour <= 0) {
      const look = clamp(Math.abs(m.speed) * 0.6 + 1.5, 1.5, 7);
      const dir = _v.set(Math.sin(desired), 0, Math.cos(desired));
      const hit = this.cast(inst, def, def.boxLow, def.boxLowY, _v2.copy(dir).multiplyScalar(Math.min(look, dist)), desired);
      if (hit && hit.normal1.y < 0.7) {
        const n = _v3.set(hit.normal1.x, 0, hit.normal1.z).normalize();
        const t1x = n.z, t1z = -n.x;
        const s = t1x * dir.x + t1z * dir.z >= 0 ? 1 : -1;
        desired = Math.atan2(t1x * s + n.x * 0.3, t1z * s + n.z * 0.3);
      }
    }
    if (desired !== null) {
      const err = wrap(desired - m.heading);
      ctl.yawRate = clamp(err * 3, -1, 1) * turn;
      if (nav.point && (dist > nav.arrive || m.detour > 0)) {
        const align = Math.max(0, Math.cos(err));
        let sp = nav.speed * (Math.abs(err) > 1.2 && dist < 8 ? 0.15 : 0.3 + 0.7 * align);
        const brake = Math.max(6, d.speed * 0.9) * 2;
        const remain = Math.max(0, dist - nav.arrive);
        sp = Math.min(sp, Math.sqrt(2 * brake * remain) / Math.max(1, d.speed));
        ctl.throttle = m.detour > 0 ? -0.5 : sp;
      }
    }
    if (m.stuck >= 2 && d.jump) ctl.jump = true;
    if (def.carLike) ctl.yawRate *= clamp(Math.abs(m.speed) / 5, 0.35, 1);
    void dt;
    return ctl;
  }

  // ------------------------------------------------------------------ locomotion

  private rayHit(x: number, y: number, z: number, max: number, body: RBody | null, dir = -1): number | null {
    const R = this.w.R;
    if (!this.ray) this.ray = new R.Ray({ x: 0, y: 0, z: 0 }, { x: 0, y: -1, z: 0 });
    const r = this.ray;
    r.origin.x = x; r.origin.y = y; r.origin.z = z;
    r.dir.x = 0; r.dir.y = dir; r.dir.z = 0;
    const hit = this.w.pw.castRay(r, max, true, undefined, QF, undefined, body ?? undefined);
    if (!hit || hit.timeOfImpact < 1e-3) return null;
    return hit.timeOfImpact;
  }

  /** Height of the ground below (x, y, z), or null. */
  groundY(x: number, y: number, z: number, max: number, body: RBody | null): number | null {
    const t = this.rayHit(x, y, z, max, body);
    return t === null ? null : y - t;
  }

  private cast(inst: Inst, def: MachineDef, shape: any, cy: number, disp: Vector3, heading: number) {
    const len = disp.length();
    if (len < 1e-5) return null;
    _q2.setFromAxisAngle(UP, heading);
    const c = _v3.set(def.ext.center.x, cy, def.ext.center.z).applyQuaternion(_q2).add(inst.pos);
    return this.w.pw.castShape(c, _q2, disp, shape, 0.03, 1, false, undefined, QF, undefined, inst.body);
  }

  /** Swept move of a footprint box; slides along walls. Returns true if something blocked it. */
  private sweep(inst: Inst, def: MachineDef, shape: any, cy: number, disp: Vector3, heading: number, out: Vector3): boolean {
    out.copy(disp);
    const hit = this.cast(inst, def, shape, cy, disp, heading);
    if (!hit) return false;
    const n = hit.normal1;
    if (n.y > 0.7) return false; // walkable slope: the ground probes lift us
    const len = disp.length();
    const toi = Math.max(0, hit.time_of_impact - 0.02 / len);
    out.copy(disp).multiplyScalar(toi);
    const rem = _v.copy(disp).multiplyScalar(1 - toi);
    const nh = _v2.set(n.x, 0, n.z);
    if (nh.lengthSq() > 1e-6) {
      nh.normalize();
      const into = rem.dot(nh);
      if (into < 0) rem.addScaledVector(nh, -into);
      const save = inst.pos.clone();
      inst.pos.add(out);
      const hit2 = this.cast(inst, def, shape, cy, rem, heading);
      inst.pos.copy(save);
      if (!hit2) out.add(rem);
      else if (hit2.normal1.y > 0.7) out.add(rem);
      else out.addScaledVector(rem, Math.max(0, hit2.time_of_impact - 0.02 / Math.max(1e-4, rem.length())));
    }
    return true;
  }

  private stepGround(c: Construct, inst: Inst, m: MachineState, ctl: Control, dt: number) {
    const def = c.mach!;
    const d = def.drive!;
    const hover = d.mode === 'hover';
    const max = Math.min(d.speed * (ctl.boost ? 1.25 : 1), ctl.chase ? COMBAT.chaser.maxSpeed : Infinity);
    const want = clamp(ctl.throttle, -0.5, 1) * max;
    const accel = Math.max(6, d.speed * 0.9);
    const rate = Math.abs(want) < Math.abs(m.speed) || Math.sign(want) !== Math.sign(m.speed) ? accel * 2 : accel;
    m.speed += clamp(want - m.speed, -rate * dt, rate * dt);
    if (!m.onGround && !hover) m.speed *= 1 - 0.2 * dt; // little air control
    m.yawRate += (ctl.yawRate - m.yawRate) * Math.min(1, dt * 10);
    m.heading = wrap(m.heading + m.yawRate * dt);
    const turnMax = d.turn * D2R;
    m.steer = clamp(m.yawRate / turnMax, -1, 1) * (m.speed < -0.3 ? -1 : 1);
    const fwd = _v.set(Math.sin(m.heading), 0, Math.cos(m.heading));
    const disp = new Vector3().copy(fwd).multiplyScalar(m.speed * dt);
    const moved = new Vector3();
    const blocked = this.sweep(inst, def, def.boxLow, def.boxLowY, disp, m.heading, moved);
    if (blocked) m.speed *= 0.6;
    inst.pos.add(moved);
    const along = moved.dot(fwd.set(Math.sin(m.heading), 0, Math.cos(m.heading)));
    m.travel += along;
    const vAct = moved.length() / dt;
    m.gaitAmp += (clamp(vAct / Math.max(0.5, d.speed * 0.15), 0, 1) - m.gaitAmp) * Math.min(1, dt * 6);
    if (vAct > 0.05) m.gaitT += dt * clamp(0.4 + 0.6 * vAct / Math.max(0.5, d.speed), 0.4, 1.3);
    // terrain: probe the footprint
    const ext = def.ext;
    const ch = Math.cos(m.heading), sh = Math.sin(m.heading);
    const fz = def.hz * 0.8, fx = def.hx * 0.8;
    const feet = inst.pos.y + ext.min.y;
    const top = feet + def.stepH + 0.3;
    const reach = def.stepH + 0.3 + 60;
    const probe = (lx: number, lz: number) => {
      const wx = inst.pos.x + lx * ch + lz * sh, wz = inst.pos.z - lx * sh + lz * ch;
      const g = this.groundY(wx, top, wz, reach, inst.body);
      return g;
    };
    const cx = ext.center.x, cz = ext.center.z;
    const gC = probe(cx, cz), gF = probe(cx, cz + fz), gB = probe(cx, cz - fz), gR = probe(cx + fx, cz), gL = probe(cx - fx, cz);
    let g = -Infinity;
    for (const x of [gC, gF !== null && gB !== null ? (gF + gB) / 2 : null, gR !== null && gL !== null ? (gR + gL) / 2 : null, gF, gB]) if (x !== null && x > g) g = x;
    if (gF !== null && gB !== null && gC !== null) g = Math.max(gC, (gF + gB) / 2, gR !== null && gL !== null ? (gR + gL) / 2 : -Infinity);
    const alt = hover ? (d.altitude ?? 1.2) + Math.sin((this.w.time + inst.offset.x) * 2.2) * 0.08 : 0;
    const targetY = isFinite(g) ? g - ext.min.y + alt : -Infinity;
    // vertical: jump / fall / follow the terrain
    if (ctl.jump && m.onGround && d.jump && m.vy <= 0.1) { m.vy = d.jump; m.onGround = false; this.w.emit('machine_jump', { construct: c.id }); }
    if (m.vy > 0.1 || inst.pos.y > targetY + 0.05) {
      m.vy -= (hover ? GRAV * 0.5 : GRAV) * dt;
      inst.pos.y += m.vy * dt;
      if (inst.pos.y <= targetY) {
        if (m.vy < -8) this.w.emit('machine_land', { construct: c.id, speed: Math.round(-m.vy) });
        inst.pos.y = targetY;
        m.vy = 0;
        m.onGround = true;
      } else m.onGround = false;
    } else {
      // at or below the terrain: rise to it smoothly (suspension), step up small ledges
      inst.pos.y += (targetY - inst.pos.y) * Math.min(1, dt * (hover ? 6 : 16));
      m.vy = 0;
      m.onGround = true;
    }
    if (inst.pos.y < -250) c.expires = 0;
    // body tilt from terrain (+ lean for bikes)
    let wantPitch = 0, wantRoll = 0;
    if (gF !== null && gB !== null && Math.abs(gF - gB) < def.stepH * 2 + 0.5) wantPitch = clamp(Math.atan2(gF - gB, 2 * fz), -0.6, 0.6);
    if (gR !== null && gL !== null && Math.abs(gR - gL) < def.stepH * 2 + 0.5) wantRoll = clamp(Math.atan2(gR - gL, 2 * fx), -0.5, 0.5);
    if (def.narrow) wantRoll += clamp(m.yawRate * m.speed * 0.045, -0.6, 0.6);
    if (hover) wantRoll += clamp(m.yawRate * 0.25, -0.35, 0.35);
    wantPitch += clamp(-(want - m.speed) * 0.004, -0.06, 0.06); // nose dips when braking
    const k = Math.min(1, dt * 8);
    m.pitch += (wantPitch - m.pitch) * k;
    m.roll += (wantRoll - m.roll) * k;
    this.orient(inst, m);
  }

  private stepAir(c: Construct, inst: Inst, m: MachineState, ctl: Control, dt: number, ridden: boolean) {
    const def = c.mach!;
    const d = def.drive!;
    const ext = def.ext;
    const max = d.speed * (ctl.boost ? 1.3 : 1);
    const turn = d.turn * D2R;
    // heading
    if (ctl.heading !== null) {
      const err = wrap(ctl.heading - m.heading);
      m.yawRate += (clamp(err * 3, -turn, turn) - m.yawRate) * Math.min(1, dt * 8);
    } else m.yawRate *= 1 - Math.min(1, dt * 6);
    m.heading = wrap(m.heading + m.yawRate * dt);
    const fwd = new Vector3(Math.sin(m.heading), 0, Math.cos(m.heading));
    // ground under / ahead (rooftops count), for altitude hold
    const pos = inst.pos;
    const probeTop = pos.y + 40;
    const gHere = this.groundY(pos.x, probeTop, pos.z, 400, inst.body);
    let gAhead = gHere ?? -Infinity;
    for (const t of [0.8, 1.8]) {
      const gx = pos.x + m.hv.x * t + fwd.x * def.hz * 1.5, gz = pos.z + m.hv.z * t + fwd.z * def.hz * 1.5;
      const ga = this.groundY(gx, probeTop, gz, 400, inst.body);
      if (ga !== null) gAhead = Math.max(gAhead, ga);
    }
    const floorY = (gHere ?? -Infinity) - ext.min.y;
    const clearY = gAhead - ext.min.y + 0.8;
    const airborneNow = gHere === null || pos.y > floorY + 0.6;
    // velocity
    const target = ctl.hvel.clone();
    if (def.fixedWing && (airborneNow || ridden)) {
      const f = target.dot(fwd);
      const min = d.minSpeed ?? 0;
      if (f < min) target.addScaledVector(fwd, min - f);
    }
    if (target.length() > max) target.setLength(max);
    const acc = Math.max(8, d.speed * 0.8) * dt;
    const dv = _v.copy(target).sub(m.hv);
    if (dv.length() > acc) dv.setLength(acc);
    m.hv.add(dv);
    if (def.fixedWing) {
      // planes fly where they point
      const sp = m.hv.length();
      m.hv.copy(fwd).multiplyScalar(sp);
    }
    // altitude target
    const cruise = (gAhead > -Infinity ? gAhead : pos.y) - ext.min.y + (d.altitude ?? 8);
    let alt: number;
    if (ridden) {
      if (m.altY === null) m.altY = Math.max(pos.y, cruise);
      m.altY += ctl.climb * Math.max(6, d.speed * 0.45) * dt;
      m.altY = Math.min(m.altY, (gAhead > -Infinity ? gAhead : pos.y) + 120);
      alt = m.altY;
    } else {
      m.altY = null;
      if (ctl.land) alt = floorY;
      else alt = ctl.alt ?? cruise;
      if (!ctl.land) alt = Math.max(alt, cruise - (d.altitude ?? 8) * 0.6);
    }
    if (!ctl.land || ridden) alt = Math.max(alt, clearY);
    if (ridden && m.altY !== null && m.altY < clearY) m.altY = clearY;
    const climbRate = Math.max(6, d.speed * 0.5);
    const wantVy = clamp((alt - pos.y) * 1.8, -climbRate, climbRate);
    m.vy += clamp(wantVy - m.vy, -25 * dt, 25 * dt);
    // horizontal sweep (whole body)
    const disp = new Vector3().copy(m.hv).multiplyScalar(dt);
    const moved = new Vector3();
    const blocked = this.sweep(inst, def, def.boxFull, def.boxFullY, disp, m.heading, moved);
    if (blocked) { m.hv.multiplyScalar(0.5); m.vy = Math.max(m.vy, climbRate * 0.7); if (m.altY !== null) m.altY += 4 * dt; m.climbBoost = Math.max(m.climbBoost, 1); }
    pos.add(moved);
    // vertical move, never below the ground
    pos.y += m.vy * dt;
    const gNow = this.groundY(pos.x, pos.y + ext.max.y + 1, pos.z, ext.size.y + 30, inst.body);
    const floorNow = gNow === null ? -Infinity : gNow - ext.min.y;
    m.onGround = false;
    if (pos.y <= floorNow) { pos.y = floorNow; m.vy = Math.max(0, m.vy); m.onGround = true; if (!ridden && ctl.land) m.hv.multiplyScalar(1 - Math.min(1, dt * 3)); }
    if (pos.y < -250) c.expires = 0;
    m.speed = m.hv.dot(fwd);
    m.travel += moved.dot(fwd);
    const sp = m.hv.length();
    m.gaitT += dt;
    m.gaitAmp = m.onGround ? clamp(sp / 3, 0, 1) : 0;
    m.steer = clamp(m.yawRate / turn, -1, 1);
    // attitude: bank into turns; helicopters nose down to accelerate, planes pitch with climb
    const lat = m.hv.dot(_v.set(fwd.z, 0, -fwd.x));
    let wantRoll = m.onGround ? 0 : clamp(-m.yawRate * 0.45 * clamp(sp / 8, 0.3, 1.5) - lat * 0.02, -0.7, 0.7);
    let wantPitch: number;
    if (def.fixedWing) wantPitch = m.onGround ? 0 : clamp(Math.atan2(m.vy, Math.max(6, sp)), -0.6, 0.6);
    else wantPitch = m.onGround ? 0 : clamp(-(m.speed / Math.max(1, d.speed)) * 0.22 + m.vy * 0.01, -0.35, 0.3);
    if (ctl.land && m.onGround) { wantRoll = 0; wantPitch = 0; }
    const k = Math.min(1, dt * 5);
    m.pitch += (wantPitch - m.pitch) * k;
    m.roll += (wantRoll - m.roll) * k;
    this.orient(inst, m);
  }

  private orient(inst: Inst, m: MachineState) {
    inst.quat.setFromAxisAngle(UP, m.heading).multiply(_q.setFromAxisAngle(X, -m.pitch)).multiply(_q2.setFromAxisAngle(Z, m.roll));
  }

  // ------------------------------------------------------------------ weapons

  /** Driving into an enemy hurts it (vehicles ram, creatures charge). Damage grows with speed and size. */
  private ram(c: Construct, inst: Inst, m: MachineState) {
    const sp = m.vel.length();
    if (sp < 5) return;
    const def = c.mach!;
    const reach = Math.max(def.hx, def.hz) + 0.6;
    const center = _v.copy(def.ext.center).applyQuaternion(inst.quat).add(inst.pos);
    const dir = _v2.copy(m.vel).divideScalar(sp);
    for (const t of this.enemiesOf(c.owner)) {
      const tp = this.tPos(t, _v3);
      if (!tp) continue;
      const r = this.tRadius(t);
      const d = tp.sub(center);
      const dist = d.length();
      if (dist > reach + r) continue;
      if (d.dot(dir) < 0) continue; // only what's ahead of us
      const key = c.id + '>' + t.id;
      if ((this.rams.get(key) ?? -1) > this.w.time) continue;
      this.rams.set(key, this.w.time + 0.6);
      const mass = Math.min(3, Math.max(0.5, Math.sqrt(c.extent.size.x * c.extent.size.y * c.extent.size.z) / 3));
      const dmg = (sp - 4) * 4 * mass;
      this.dealDamage(c, t, dmg, 'ram', dir.clone(), 1.3);
      this.w.emit('impact', { construct: c.id, entity: t.id, dv: Math.round(sp), pos: this.tPos(t, new Vector3())?.toArray() ?? inst.pos.toArray() });
    }
    if (this.rams.size > 256) for (const [k, until] of this.rams) if (until < this.w.time) this.rams.delete(k);
  }

  private weaponsStep(c: Construct, inst: Inst, m: MachineState, dt: number, ridden: boolean) {
    const def = c.mach!;
    const inp = this.w.input;
    const cmd = m.cmd && m.cmd.until >= this.w.time ? m.cmd : null;
    const wantFire = (ridden && (inp.primary || inp.hookL || inp.hookR)) || !!cmd?.fire;
    const riderAiming = (ridden && this.w.time - m.riderAimAt < 1.5) || !!cmd;
    const tgt = !riderAiming && !m.holdFire && m.target && this.tPos(m.target, _v) ? m.target : null;
    for (let wi = 0; wi < def.weapons.length; wi++) {
      m.cds[wi] -= dt;
      if (m.cds[wi] > 0) continue;
      const wp = def.weapons[wi];
      let aimAt: Vector3 | null = null;
      if (riderAiming) {
        if (!wantFire) continue;
        aimAt = cmd ? cmd.aim.clone() : this.playerAim().clone();
      } else {
        if (!tgt) continue;
        aimAt = wp.kind === 'melee' ? this.tPos(tgt, new Vector3()) : this.lead(inst, tgt, shotSpeed(wp));
        if (!aimAt) continue;
      }
      // muzzle
      const list = def.wMuzzles[wi];
      const mi = list.length ? list[m.alt[wi] % list.length] : -1;
      const muzzle = new Vector3();
      let mj = -1;
      if (mi >= 0) {
        const mz = def.muzzles[mi];
        mj = mz.joint;
        muzzle.copy(mz.local);
        if (mj >= 0 && m.js) { const pose = m.js.poses.get(def.joints[mj].j.group)!; muzzle.applyQuaternion(pose.quat).add(pose.pos); }
        muzzle.applyQuaternion(inst.quat).add(inst.pos);
      } else if (def.wBody[wi]) muzzle.copy(def.ext.center).applyQuaternion(inst.quat).add(inst.pos);
      else muzzle.copy(def.front).applyQuaternion(inst.quat).add(inst.pos);
      const to = aimAt.clone().sub(muzzle);
      const dist = to.length();
      if (!riderAiming) {
        if (wp.kind === 'melee') { if (dist - this.tRadius(tgt!) - (def.wBody[wi] ? c.radius * 0.6 : 0) > wp.range) continue; }
        else if (dist > wp.range) continue;
        // pointing roughly the right way?
        const aj = def.wAim[wi];
        if (aj >= 0) { if (m.jointErr[aj] > (wp.kind === 'flame' ? 25 : 15)) continue; }
        else if (wp.kind !== 'melee') {
          const f = _v.set(Math.sin(m.heading), 0, Math.cos(m.heading));
          const h = _v2.copy(to).setY(0).normalize();
          if (f.dot(h) < Math.cos(35 * D2R)) continue;
        }
        if (wp.kind !== 'melee' && !this.clearShot(muzzle, aimAt, inst.body, tgt!)) continue;
      }
      const dir = dist > 1e-4 ? to.divideScalar(dist) : _v.set(Math.sin(m.heading), 0, Math.cos(m.heading)).clone();
      this.fire(c, inst, m, wp, muzzle, dir, tgt, aimAt);
      m.cds[wi] = 1 / Math.max(0.05, wp.rate);
      m.alt[wi]++;
      m.shots++;
      // recoil / bite: joints on the fired muzzle's chain
      const base = mj >= 0 ? mj : wp.group ? def.joints.findIndex((r) => r.j.group === wp.group) : -1;
      let j = base;
      let guard = 0;
      while (j >= 0 && guard++ < 32) { if (def.joints[j].hasFire) m.fireAt[j] = this.w.time; j = def.joints[j].parent; }
      if (base >= 0) for (const r of def.joints) if (r.hasFire && isAncestor(def.joints, base, r.index)) m.fireAt[r.index] = this.w.time;
    }
  }

  /** Push a shot's spawn point forward until it doesn't overlap our own colliders (shots never hit their maker). */
  private clearOfSelf(inst: Inst, start: Vector3, dir: Vector3, size: number): Vector3 {
    const R = this.w.R;
    const ball = new R.Ball(size);
    for (let k = 0; k < 10; k++) {
      let hit = false;
      this.w.pw.intersectionsWithShape(start, IDENT, ball, () => { hit = true; return false; }, undefined, undefined, undefined, undefined, (col) => col.parent() === inst.body);
      if (!hit) break;
      start.addScaledVector(dir, size + 0.3);
    }
    return start;
  }

  private clearShot(from: Vector3, to: Vector3, body: RBody, tgt: TargetRef): boolean {
    const d = _v.copy(to).sub(from);
    const len = d.length();
    if (len < 1e-3) return true;
    const R = this.w.R;
    const tb = this.tBody(tgt);
    const hit = this.w.pw.castRay(new R.Ray(from, d.divideScalar(len)), Math.max(0, len - this.tRadius(tgt)), true, undefined, LOSF, undefined, body, tb ? (col) => col.parent() !== tb : undefined);
    return !hit;
  }

  private fire(c: Construct, inst: Inst, m: MachineState, wp: Weapon, muzzle: Vector3, dir: Vector3, tgt: TargetRef | null, aimAt: Vector3) {
    const size = wp.size ?? (wp.kind === 'cannon' ? 0.35 : wp.kind === 'bomb' ? 0.45 : wp.kind === 'net' ? 0.45 : 0.18);
    switch (wp.kind) {
      case 'gun': case 'cannon': case 'missile': case 'net': case 'bomb': {
        let spd = Math.max(10, wp.speed || 50);
        const v = dir.clone();
        if (wp.spread) { const a = (m.rng() - 0.5) * wp.spread * D2R, b = (m.rng() - 0.5) * wp.spread * D2R; v.applyAxisAngle(UP, a).applyAxisAngle(X, b); }
        let vel = v.multiplyScalar(spd);
        let gravity = 0;
        if (wp.kind === 'bomb') {
          gravity = 1;
          spd = Math.min(spd, 30);
          const dd = aimAt.clone().sub(muzzle);
          const t = Math.max(0.3, dd.length() / spd);
          vel = dd.divideScalar(t);
          vel.y += 0.5 * GRAV * t;
        }
        const start = this.clearOfSelf(inst, muzzle.clone().addScaledVector(dir, size + 0.45), dir, size);
        const pr = this.w.spawnProjectile(start, vel, null, wp.damage, size, {
          friendly: true, owner: c.id, ownerPlayer: c.owner, kind: wp.kind, gravity,
          splash: wp.kind === 'cannon' ? 2.5 : wp.kind === 'bomb' ? 4.5 : wp.kind === 'missile' ? 3 : undefined,
        });
        if (wp.kind === 'missile' && tgt) this.missiles.set(pr.id, { target: tgt, speed: Math.min(spd, COMBAT.missile.maxSpeed) });
        break;
      }
      case 'laser': {
        const R = this.w.R;
        const hit = this.w.pw.castRay(new R.Ray(muzzle, dir), wp.range, true, undefined, BEAMF, undefined, inst.body);
        const end = muzzle.clone().addScaledVector(dir, hit ? hit.timeOfImpact : wp.range);
        const own = hit ? this.w.owners.get(hit.collider.handle) : null;
        if (own?.kind === 'entity') this.dealDamage(c, { kind: 'entity', id: own.id }, wp.damage, 'laser', dir, 1);
        else if (own?.kind === 'construct') this.dealDamage(c, { kind: 'construct', id: own.id }, wp.damage, 'laser');
        this.w.emit('machine_beam', { construct: c.id, kind: 'laser', from: muzzle.toArray(), to: end.toArray(), dur: Math.min(0.25, 0.6 / wp.rate) });
        break;
      }
      case 'flame': {
        const cone = Math.cos(22 * D2R);
        const cands = this.enemiesOf(c.owner).slice();
        if (tgt && !cands.some((x) => x.kind === tgt.kind && x.id === tgt.id)) cands.push(tgt);
        for (const t of cands) {
          const ep = this.tPos(t, new Vector3());
          if (!ep) continue;
          const r = this.tRadius(t);
          const d = _v2.copy(ep).sub(muzzle);
          const dl = d.length();
          if (dl > wp.range + r || dl < 1e-3) continue;
          if (d.divideScalar(dl).dot(dir) < cone && dl > r + 1) continue;
          if (!this.clearShot(muzzle, ep, inst.body, t)) continue;
          this.dealDamage(c, t, wp.damage, 'flame', dir, 0.2);
        }
        this.w.emit('machine_beam', { construct: c.id, kind: 'flame', from: muzzle.toArray(), to: muzzle.clone().addScaledVector(dir, wp.range * 0.85).toArray(), dur: 0.15 });
        break;
      }
      case 'melee': {
        const hitList: TargetRef[] = tgt ? [tgt] : [];
        if (!tgt) {
          // ridden: strike whatever is right in front (anything that isn't ours)
          for (const e of this.w.entities.values()) if (!e.disabled && !e.heldBy) hitList.push({ kind: 'entity', id: e.id });
          for (const o of this.rt.list.values()) if (o.owner !== c.owner) hitList.push({ kind: 'construct', id: o.id });
        }
        for (const t of hitList) {
          const tp = this.tPos(t, new Vector3());
          if (!tp || (!tgt && tp.distanceTo(muzzle) - this.tRadius(t) - (c.mach!.wBody[c.mach!.weapons.indexOf(wp)] ? c.radius * 0.6 : 0) > wp.range + 0.5)) continue;
          this.dealDamage(c, t, wp.damage, 'bite', dir, 1);
          this.w.emit('impact', { construct: c.id, entity: t.id, dv: Math.round(wp.damage / 2), pos: tp.toArray() });
        }
        break;
      }
    }
    this.w.emit('construct_fire', { construct: c.id, kind: wp.kind, pos: muzzle.toArray(), dir: dir.toArray() });
  }

  /** A friendly projectile (fired by a construct) hit something. */
  projectileHit(p: Projectile, other: Owner) {
    const t = p.body.translation();
    const at = new Vector3(t.x, t.y, t.z);
    const src = p.owner ? this.rt.get(p.owner) : null;
    const ownerPlayer = p.ownerPlayer ?? src?.owner ?? null;
    let direct: TargetRef | null = null;
    if (other.kind === 'construct' && (other.id === p.owner || this.rt.get(other.id)?.owner === ownerPlayer)) { this.w.removeProjectile(p); return; }
    const v = p.body.linvel();
    const dir = new Vector3(v.x, v.y, v.z).normalize();
    if (other.kind === 'entity' || other.kind === 'construct') {
      direct = { kind: other.kind, id: other.id };
      if (src) this.dealDamage(src, direct, p.damage, p.kind ?? 'shot', dir, other.kind === 'entity' ? Math.min(1, 0.3 + p.damage * 0.02) : 0);
      else if (other.kind === 'entity') { const e = this.w.entities.get(other.id); if (e) this.w.damageEntity(e, p.damage, p.kind ?? 'shot', p.owner ?? null); }
      if (p.kind === 'net' && other.kind === 'entity') {
        const e = this.w.entities.get(other.id);
        if (e) { e.stunnedUntil = Math.max(e.stunnedUntil, this.w.time + 4); this.w.emit('netted', { entity: e.id, construct: p.owner }); }
      }
      this.w.emit('machine_hit', { construct: p.owner, entity: other.id, target: other.kind, pos: at.toArray(), kind: p.kind });
      if (other.kind === 'construct') this.w.stats.projectilesBlocked++;
    }
    if (p.splash && src) {
      for (const tr of this.enemiesOf(src.owner)) {
        if (direct && tr.kind === direct.kind && tr.id === direct.id) continue;
        const tp = this.tPos(tr, _v);
        if (!tp) continue;
        const r = this.tRadius(tr);
        const d = tp.distanceTo(at);
        if (d > p.splash + r) continue;
        this.dealDamage(src, tr, p.damage * 0.5 * (1 - Math.max(0, d - r) / p.splash), 'blast');
      }
      this.w.emit('explosion', { construct: p.owner, pos: at.toArray(), radius: p.splash });
    }
    this.missiles.delete(p.id);
    this.w.removeProjectile(p);
  }

  // ------------------------------------------------------------------ riding

  nearestRideable(maxDist = 6): Construct | null {
    const P = this.w.player;
    let best: Construct | null = null, bd = Infinity;
    for (const c of this.rt.list.values()) {
      if (!c.mach?.ride || !c.insts[0]?.alive) continue;
      const i = c.insts[0];
      const d = Math.max(0, P.pos.distanceTo(i.pos) - c.radius * 0.6);
      if (d <= maxDist && d < bd) { bd = d; best = c; }
    }
    return best;
  }

  /** Put the player in/on a rideable construct. */
  ride(id: string): boolean {
    const c = this.rt.get(id);
    const P = this.w.player;
    const inst = c?.insts[0];
    if (!c || !c.mach?.ride || !inst?.alive || !inst.m) return false;
    if (P.riding && P.riding.id !== id) this.dismount();
    P.riding = { id, body: inst.body, camBack: clamp(c.radius * 0.9, 1.2, 12), camUp: clamp(c.extent.size.y * 0.3, 0.3, 4) };
    P.flying = false;
    P.releaseTether('L');
    P.releaseTether('R');
    for (const i of c.insts) if (i.m) i.m.rider = false;
    inst.m.rider = true;
    inst.m.altY = null;
    if (inst.cargo) { this.rt.releaseEntity(inst.cargo.entityId, new Vector3()); inst.cargo = null; }
    this.rt.focusId = id;
    if (c.mach.seat) P.pos.copy(c.mach.seat).applyQuaternion(inst.quat).add(inst.pos);
    this.w.emit('ride', { id, label: c.spec.label });
    return true;
  }

  /** Get out; the player lands beside the machine. */
  dismount(): boolean {
    const P = this.w.player;
    if (!P.riding) return false;
    const id = P.riding.id;
    const c = this.rt.get(id);
    P.riding = null;
    const inst = c?.insts[0];
    if (c && inst) {
      if (inst.m) { inst.m.rider = false; inst.m.altY = null; this.resetBrain(c, inst, inst.m); }
      const side = Math.max(c.extent.max.x, -c.extent.min.x) + 0.9;
      const R = this.w.R;
      let spot: Vector3 | null = null;
      for (const s of [[side, 0], [-side, 0], [0, c.extent.min.z - 1], [0, c.extent.max.z + 1]] as [number, number][]) {
        const cand = new Vector3(s[0], 0, s[1]).applyQuaternion(_q.setFromAxisAngle(UP, inst.m?.heading ?? 0)).add(inst.pos);
        const g = this.groundY(cand.x, inst.pos.y + c.extent.max.y + 1, cand.z, 400, inst.body);
        cand.y = (g ?? inst.pos.y) + 1.0;
        const blocked = this.w.pw.castRay(new R.Ray({ x: inst.pos.x, y: cand.y, z: inst.pos.z }, _v.copy(cand).sub(_v2.set(inst.pos.x, cand.y, inst.pos.z)).normalize()), Math.hypot(s[0], s[1]), true, undefined, LOSF);
        if (!blocked) { spot = cand; break; }
      }
      P.teleport(spot ?? P.pos.clone().add(new Vector3(0, 1.5, 0)), (inst.m?.vel ?? new Vector3()).clone().multiplyScalar(0.5));
    } else P.vel.set(0, 0, 0);
    this.w.emit('dismount', { id });
    return true;
  }

  /** Called when a construct is removed. */
  onDismiss(c: Construct) {
    if (this.w.player.riding?.id === c.id) this.dismount();
  }

  // ------------------------------------------------------------------ orders (language layer API)

  /**
   * Give a construct new orders: { goal: 'hunt', target: {kind:'entity', id} } ("attack that"),
   * { goal: 'follow' } ("follow me" / "come back"), { goal: 'idle' } ("stop"), { goal: 'wander' } ("drive around"),
   * { goal: 'guard', target: {kind:'point', p} }, { goal: 'fetch', target }, { goal: 'goto', target }, { goal: 'patrol' }, { goal: 'flee' }.
   * A construct without a drive gets legs (ground drive) unless it is static. Returns false if it doesn't exist.
   */
  command(id: string, order: { goal: BrainGoal; target?: Ref; range?: number }): boolean {
    const c = this.rt.get(id);
    if (!c) return false;
    const note = (_: string) => {};
    const b = clampMachineBehavior({ type: 'brain', goal: order.goal, target: order.target ?? (order.goal === 'follow' ? { kind: 'self' } : order.goal === 'hunt' ? { kind: 'nearest_enemy' } : undefined), range: order.range } as Brain, note) as Brain | null;
    if (!b) return false;
    const s = c.spec;
    s.behaviors = s.behaviors.filter((x) => x.type !== 'brain' && !['follow', 'orbit', 'strike', 'grab', 'home', 'hold', 'sweep'].includes(x.type));
    s.behaviors.push(b);
    if (!s.behaviors.some((x) => x.type === 'drive') && c.insts[0]?.body.isKinematic()) {
      s.behaviors.push({ type: 'drive', mode: 'ground', speed: 8, turn: 180 });
      s.motion = 'kinematic';
    }
    if (b.goal === 'fetch' && !c.insts.some((i) => i.cargo)) for (const i of c.insts) i.grab = null;
    this.init(c);
    this.w.emit('machine_order', { id, goal: b.goal });
    return true;
  }
}

export type { Pose };
