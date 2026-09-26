/**
 * World content that gives constructs something to do: walking robots that punch, destructible
 * stacks / walls / crates that break into debris, bystanders who flee and can be saved, enemy
 * waves and a race timer. Deterministic (fixed step, seeded rng, no wall clock).
 *
 * Every outcome event names the construct responsible in `by` (see World.creditOf).
 */
import { Vector3, Quaternion } from 'three';
import { G, groups, type RBody, type RCollider } from './physics.js';
import type { World, Entity } from './world.js';

const UP = new Vector3(0, 1, 0);
const _a = new Vector3();
const _b = new Vector3();
const _q = new Quaternion();

export interface RobotAI {
  kind: 'robot';
  mode: 'walk' | 'knocked' | 'getup' | 'down';
  modeT: number;
  yaw: number;
  /** seconds into the current punch, -1 when not punching (renderer animates the arm from it) */
  punchT: number;
  punchCd: number;
  punchTarget: string | null;
  punchDone: boolean;
  steerOff: number;
  steerSide: number;
  phase: number; // stagger for periodic queries
  stuckT: number;
  aggro: number;
  /** patrol centre (null = always chase the player) */
  patrol: Vector3 | null;
  patrolR: number;
  patrolIdx: number;
  leash: number;
  speed: number;
  wasHeld: boolean;
  getupFrom: Quaternion | null;
  getupY0: number;
  getupY1: number;
  fromWave: boolean;
}

export interface CivilianAI {
  kind: 'civilian';
  mode: 'idle' | 'flee';
  threat: string | null;
  threatDist: number;
  steerOff: number;
  steerSide: number;
  phase: number;
  shieldT: number;
  shieldBy: string | null;
  carriedBy: string | null;
  carryStart: Vector3 | null;
  releasedAt: number;
  saved: boolean;
}

export interface BlockAI { kind: 'block'; homeQuat: Quaternion; toppled: boolean }

export type EntityAI = RobotAI | CivilianAI | BlockAI;

export interface Debris { id: number; body: RBody; collider: RCollider; size: Vector3; color: number; born: number; life: number }

export interface StackInfo { id: string; kind: 'stack' | 'wall'; total: number; toppled: number; done: boolean }

export interface WaveState {
  enabled: boolean;
  period: number; // seconds between waves
  cap: number; // max simultaneously active wave-spawned + scene hostiles
  timer: number;
  wave: number;
  spawnDist: number;
}

export interface RaceState { startZ: number; finishZ: number; halfWidth: number; startedAt: number | null; finishedAt: number | null; best: number | null; prevZ: number }

/** Mulberry32, same family as levels.ts. */
function rng(seed: number) {
  return () => {
    seed |= 0; seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function yawQuat(yaw: number, out = new Quaternion()) { return out.setFromAxisAngle(UP, yaw); }
function rotY(v: Vector3, a: number, out = new Vector3()) {
  const c = Math.cos(a), s = Math.sin(a);
  return out.set(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
}
function wrapAngle(a: number) { while (a > Math.PI) a -= 2 * Math.PI; while (a < -Math.PI) a += 2 * Math.PI; return a; }

export const ROBOT = { speed: 3.4, accel: 26, reach: 1.9, stopAt: 1.35, punchWindup: 0.32, punchTotal: 0.7, punchCooldown: 0.9, punchDamage: 12, standY: 1.07 };
export const CIVILIAN = { fleeRadius: 11, threatRadius: 16, speed: 5.2, accel: 20, safeDist: 15, carryDist: 6, shieldTime: 1.5 };
const DEBRIS_CAP = 40;

export class WorldContent {
  debris: Debris[] = [];
  stacks = new Map<string, StackInfo>();
  waves: WaveState = { enabled: false, period: 22, cap: 8, timer: 8, wave: 0, spawnDist: 38 };
  race: RaceState | null = null;
  private waveRng = rng(4242);
  private debrisSeq = 1;
  private breakQueue: { id: string; cause: string; by: string | null }[] = [];
  private bystanders = new Set<string>();
  private blocks = new Set<string>();
  private robots = new Set<string>();
  private waveSpawned = new Set<string>();
  private ball: any = null;
  private smallBall: any = null;

  constructor(private w: World) {}

  // ------------------------------------------------------------------ setup

  /** Called by World.addEntity for every new entity. */
  initEntity(e: Entity) {
    if (e.type === 'robot') {
      const ai: RobotAI = {
        kind: 'robot', mode: 'walk', modeT: 0, yaw: 0, punchT: -1, punchCd: 0.5, punchTarget: null, punchDone: false,
        steerOff: 0, steerSide: this.robots.size % 2 ? -1 : 1, phase: this.robots.size % 4, stuckT: 0,
        aggro: 250, patrol: null, patrolR: 8, patrolIdx: (this.robots.size * 2) % 4, leash: 60, speed: ROBOT.speed, wasHeld: false,
        getupFrom: null, getupY0: 0, getupY1: 0, fromWave: false,
      };
      e.ai = ai;
      this.robots.add(e.id);
    } else if (e.type === 'block') {
      const r = e.body.rotation();
      e.ai = { kind: 'block', homeQuat: new Quaternion(r.x, r.y, r.z, r.w), toppled: false };
      this.blocks.add(e.id);
      if (e.group) {
        const st = this.stacks.get(e.group);
        if (st) st.total++;
      }
    }
  }

  /** A hostile ground robot. With `patrol` it walks a loop and only chases a player who comes within `aggro`. */
  addRobot(pos: Vector3, opts: { patrol?: Vector3; patrolR?: number; aggro?: number; leash?: number; label?: string } = {}): Entity {
    const e = this.w.addEntity('robot', pos.clone().setY(pos.y), { label: opts.label ?? 'robot' });
    const ai = e.ai as RobotAI;
    if (opts.patrol) { ai.patrol = opts.patrol.clone(); ai.aggro = opts.aggro ?? 22; ai.leash = opts.leash ?? 40; ai.patrolR = opts.patrolR ?? 8; }
    else if (opts.aggro) ai.aggro = opts.aggro;
    return e;
  }

  /** A civilian standing around who flees from nearby hostiles and can be saved. */
  addBystander(pos: Vector3, label = 'civilian'): Entity {
    const e = this.w.addEntity('civilian', pos, { hostile: false, label });
    e.ai = {
      kind: 'civilian', mode: 'idle', threat: null, threatDist: Infinity, steerOff: 0, steerSide: 1, phase: this.bystanders.size % 6,
      shieldT: 0, shieldBy: null, carriedBy: null, carryStart: null, releasedAt: -1, saved: false,
    };
    this.bystanders.add(e.id);
    return e;
  }

  private newStack(kind: 'stack' | 'wall'): StackInfo {
    const id = this.w.uid(kind === 'wall' ? 'wall' : 'stack');
    const st: StackInfo = { id, kind, total: 0, toppled: 0, done: false };
    this.stacks.set(id, st);
    return st;
  }

  /** Loose blocks: a pyramid (`levels` rows, base `levels` wide) or a straight tower, resting on y = base.y. */
  addStack(base: Vector3, opts: { shape?: 'pyramid' | 'tower'; levels?: number; size?: number; colors?: number[]; yaw?: number } = {}): StackInfo {
    const st = this.newStack('stack');
    const n = opts.levels ?? 4;
    const s = opts.size ?? 1.0;
    const colors = opts.colors ?? [0xe8b99a, 0xc9d6e8, 0xb8c7a8, 0xf0e2c0];
    const q = yawQuat(opts.yaw ?? 0);
    const gap = 0.02;
    for (let row = 0; row < n; row++) {
      const count = opts.shape === 'tower' ? 1 : n - row;
      for (let i = 0; i < count; i++) {
        const x = (i - (count - 1) / 2) * (s + gap);
        const local = new Vector3(x, s / 2 + row * (s + 0.001), 0).applyQuaternion(q);
        const e = this.w.addEntity('block', base.clone().add(local), { size: new Vector3(s, s, s), color: colors[(row + i) % colors.length], group: st.id, label: 'block' });
        e.body.setRotation(q, false);
        (e.ai as BlockAI).homeQuat.copy(q);
      }
    }
    return st;
  }

  /** A low running-bond brick wall centred on `center` (ground level), running along its local x axis. */
  addWall(center: Vector3, opts: { bricks?: number; courses?: number; brick?: Vector3; yaw?: number; color?: number } = {}): StackInfo {
    const st = this.newStack('wall');
    const nb = opts.bricks ?? 8;
    const courses = opts.courses ?? 3;
    const b = opts.brick ?? new Vector3(1.6, 0.5, 0.6);
    const q = yawQuat(opts.yaw ?? 0);
    const base = opts.color ?? 0xd9d2c0;
    const tints = [base, 0xe8dcc6, base, 0xcfc6b2];
    const len = nb * b.x;
    for (let c = 0; c < courses; c++) {
      const odd = c % 2 === 1;
      // running bond: odd courses are offset half a brick with half bricks closing both ends
      const count = odd ? nb + 1 : nb;
      for (let i = 0; i < count; i++) {
        const half = odd && (i === 0 || i === count - 1);
        const bx = half ? b.x / 2 - 0.01 : b.x;
        const x = odd ? (i === 0 ? -len / 2 + b.x / 4 : i === count - 1 ? len / 2 - b.x / 4 : -len / 2 + i * b.x) : -len / 2 + b.x / 2 + i * b.x;
        const local = new Vector3(x, b.y / 2 + c * (b.y + 0.001), 0).applyQuaternion(q);
        const e = this.w.addEntity('block', center.clone().add(local), { size: new Vector3(bx, b.y, b.z), color: tints[(c * 3 + i) % tints.length], group: st.id, hp: 60, density: 180, label: 'brick' });
        e.body.setRotation(q, false);
        (e.ai as BlockAI).homeQuat.copy(q);
      }
    }
    return st;
  }

  setupRace(startZ: number, finishZ: number, halfWidth: number) {
    this.race = { startZ, finishZ, halfWidth, startedAt: null, finishedAt: null, best: null, prevZ: this.w.player.pos.z };
  }

  /** Seconds on the race clock (running or final), or null before the start line. */
  raceTime(): number | null {
    const r = this.race;
    if (!r || r.startedAt === null) return null;
    return (r.finishedAt ?? this.w.time) - r.startedAt;
  }

  /** Turn periodic enemy waves on/off (capped; wrecks are cleared so body counts stay bounded). */
  setWaves(enabled: boolean, opts: Partial<Pick<WaveState, 'period' | 'cap' | 'spawnDist'>> & { firstIn?: number } = {}) {
    Object.assign(this.waves, opts);
    this.waves.enabled = enabled;
    if (opts.firstIn !== undefined) this.waves.timer = opts.firstIn;
  }

  // ------------------------------------------------------------------ per-entity update (before physics)

  updateEntity(e: Entity, p: Vector3, v: Vector3, dt: number) {
    const ai = e.ai!;
    if (ai.kind === 'robot') this.updateRobot(e, ai, p, v, dt);
    else if (ai.kind === 'civilian') this.updateBystander(e, ai, p, v, dt);
  }

  private hostileActive(e: Entity) { return e.hostile && !e.disabled && e.type !== 'civilian'; }

  /** Is the way ahead clear for a body of radius r at p moving along dir? */
  private clear(e: Entity, p: Vector3, dir: Vector3, dist: number, small: boolean, ignore: RCollider | null): boolean {
    const R = this.w.R;
    if (!this.ball) { this.ball = new R.Ball(0.42); this.smallBall = new R.Ball(0.3); }
    const hit = this.w.pw.castShape(
      { x: p.x, y: p.y + (small ? 0.05 : 0.1), z: p.z }, { x: 0, y: 0, z: 0, w: 1 }, { x: dir.x, y: 0, z: dir.z }, small ? this.smallBall : this.ball,
      0, dist, false, undefined, groups(G.ENTITY, G.WORLD | G.ENTITY | G.CONSTRUCT), e.collider, undefined,
      ignore ? (c: RCollider) => c.handle !== ignore.handle : undefined,
    );
    return !hit;
  }

  /** Deterministic local obstacle avoidance: re-evaluated every 4 ticks, sticks to one side while blocked. */
  private steer(e: Entity, ai: { steerOff: number; steerSide: number; phase: number }, p: Vector3, dir: Vector3, look: number, small: boolean, ignore: RCollider | null, out: Vector3): Vector3 {
    if ((this.w.tick + ai.phase) % 4 === 0) {
      const s = ai.steerSide;
      let pick: number | null = null;
      // exhaust the current side before trying the other one (no dithering in front of walls)
      for (const a of [0, 0.55 * s, 1.1 * s, 1.65 * s, -0.55 * s, -1.1 * s, -1.65 * s]) {
        if (this.clear(e, p, rotY(dir, a, _b), look, small, ignore)) { pick = a; break; }
      }
      if (pick === null) pick = 2.3 * s;
      if (pick !== 0) ai.steerSide = pick > 0 ? 1 : -1;
      ai.steerOff = pick;
    }
    return rotY(dir, ai.steerOff, out);
  }

  /** Push the body's horizontal velocity toward `want` with bounded acceleration. */
  private drive(e: Entity, v: Vector3, want: Vector3, accel: number, dt: number) {
    const dx = want.x - v.x, dz = want.z - v.z;
    const l = Math.hypot(dx, dz);
    if (l < 1e-4) return;
    const k = Math.min(1, (accel * dt) / l);
    const m = e.body.mass();
    e.body.applyImpulse({ x: dx * k * m, y: 0, z: dz * k * m }, true);
  }

  private updateRobot(e: Entity, ai: RobotAI, p: Vector3, v: Vector3, dt: number) {
    const w = this.w;
    if (e.heldBy || e.restrainedBy) { ai.wasHeld = true; ai.punchT = -1; return; }
    if (ai.wasHeld) {
      ai.wasHeld = false;
      if (e.disabled) { this.enterDown(e, ai); return; }
      this.enterKnocked(e, ai, _a.copy(v), w.creditOf(e));
      return;
    }
    if (e.disabled) { if (ai.mode !== 'down') this.enterDown(e, ai); return; }
    ai.punchCd = Math.max(0, ai.punchCd - dt);
    if (ai.mode === 'down') return;
    if (ai.mode === 'knocked') {
      ai.modeT += dt;
      const av = e.body.angvel();
      if (ai.modeT > 2.2 && v.lengthSq() < 2.25 && av.x * av.x + av.y * av.y + av.z * av.z < 1) this.startGetup(e, ai, p);
      return;
    }
    if (ai.mode === 'getup') {
      ai.modeT += dt;
      const t = Math.min(1, ai.modeT / 0.7);
      const s = t * t * (3 - 2 * t);
      _q.copy(ai.getupFrom!).slerp(yawQuat(ai.yaw), s);
      e.body.setRotation(_q, true);
      e.body.setTranslation({ x: p.x, y: ai.getupY0 + (ai.getupY1 - ai.getupY0) * s, z: p.z }, true);
      e.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
      e.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
      if (t >= 1) { ai.mode = 'walk'; ai.modeT = 0; }
      return;
    }
    // ---- walk
    // a sudden change of velocity we didn't cause (struck, blasted, rammed) knocks it over
    const jolt = _a.copy(v).sub(e.lastVel);
    if (jolt.length() > 6) { this.enterKnocked(e, ai, jolt, w.creditOf(e)); return; }
    if (w.time < e.stunnedUntil) { ai.punchT = -1; return; }
    const airborne = Math.abs(v.y) > 4;

    // target: the player (if in range), a bystander within arm's reach, else the patrol loop
    const P = w.player.pos;
    let tgt: Vector3 | null = null;
    let tgtId: string | null = null;
    let ignore: RCollider | null = null;
    const dPlayer = Math.hypot(P.x - p.x, P.z - p.z);
    const leashOk = !ai.patrol || Math.hypot(P.x - ai.patrol.x, P.z - ai.patrol.z) < ai.leash;
    if (dPlayer < ai.aggro && leashOk && Math.abs(P.y - p.y) < 14) { tgt = P; tgtId = 'player'; }
    let best = Math.min(dPlayer, 2.2);
    for (const id of this.bystanders) {
      const c = w.entities.get(id);
      if (!c || c.hurt || c.heldBy || c.restrainedBy) continue;
      const cp = c.body.translation();
      const d = Math.hypot(cp.x - p.x, cp.z - p.z);
      if (d < best) { best = d; tgt = _b.set(cp.x, cp.y, cp.z).clone(); tgtId = c.id; ignore = c.collider; }
    }
    let attacking = !!tgt;
    if (!tgt && ai.patrol) {
      const a = ai.patrolIdx * (Math.PI / 2) + 0.4;
      tgt = _b.set(ai.patrol.x + Math.cos(a) * ai.patrolR, p.y, ai.patrol.z + Math.sin(a) * ai.patrolR).clone();
      if (Math.hypot(tgt.x - p.x, tgt.z - p.z) < 1.5) ai.patrolIdx = (ai.patrolIdx + 1) % 4;
      attacking = false;
    }
    const want = _a.set(0, 0, 0);
    let face: Vector3 | null = null;
    if (tgt) {
      const to = new Vector3(tgt.x - p.x, 0, tgt.z - p.z);
      const dist = to.length();
      const stop = attacking ? ROBOT.stopAt : 0.8;
      if (dist > stop && dist > 1e-3) {
        const dir = to.clone().divideScalar(dist);
        const sp = attacking ? ai.speed : ai.speed * 0.55;
        this.steer(e, ai, p, dir, Math.min(3.2, dist), false, ignore, want).multiplyScalar(sp);
      }
      face = attacking && dist < ROBOT.reach + 1 ? to : want.lengthSq() > 0.04 ? want : null;
      // punch
      if (attacking && ai.punchT < 0 && ai.punchCd <= 0 && dist < ROBOT.reach && Math.abs(tgt.y - p.y) < 1.8) {
        ai.punchT = 0; ai.punchTarget = tgtId; ai.punchDone = false;
      }
    }
    if (ai.punchT >= 0) {
      ai.punchT += dt;
      want.multiplyScalar(0.3);
      if (!ai.punchDone && ai.punchT >= ROBOT.punchWindup) { ai.punchDone = true; this.resolvePunch(e, ai, p); }
      if (ai.punchT >= ROBOT.punchTotal) { ai.punchT = -1; ai.punchCd = ROBOT.punchCooldown; }
    }
    // stuck against something: switch sides
    if (want.lengthSq() > 1 && v.x * v.x + v.z * v.z < 0.09) {
      ai.stuckT += dt;
      if (ai.stuckT > 1.2) { ai.stuckT = 0; ai.steerSide = -ai.steerSide; }
    } else ai.stuckT = 0;
    if (face) {
      const wantYaw = Math.atan2(-face.x, -face.z);
      const d = wrapAngle(wantYaw - ai.yaw);
      ai.yaw = wrapAngle(ai.yaw + Math.max(-6 * dt, Math.min(6 * dt, d)));
    }
    e.body.setRotation(yawQuat(ai.yaw, _q), false);
    if (!airborne) this.drive(e, v, want, ROBOT.accel, dt);
  }

  private resolvePunch(e: Entity, ai: RobotAI, p: Vector3) {
    const w = this.w;
    const id = ai.punchTarget;
    let tp: Vector3 | null = null;
    let civ: Entity | undefined;
    if (id === 'player') tp = w.player.pos.clone();
    else if (id) { civ = w.entities.get(id); if (civ && !civ.heldBy) tp = w.entityPos(civ); }
    if (!tp) return;
    const d = tp.clone().sub(p);
    const hd = Math.hypot(d.x, d.z);
    const fist = p.clone().add(new Vector3(-Math.sin(ai.yaw), 0.35, -Math.cos(ai.yaw)).multiplyScalar(0.9));
    if (hd > ROBOT.reach + 0.5 || Math.abs(d.y) > 2) { w.emit('robot_punch', { id: e.id, target: id, hit: false, pos: fist.toArray() }); return; }
    // a construct in the way takes the blow
    const from = p.clone().setY(p.y + 0.35);
    const dir = tp.clone().sub(from);
    const len = dir.length();
    if (len > 1e-3) {
      dir.divideScalar(len);
      const hit = w.pw.castRay(new w.R.Ray(from, dir), len, true, undefined, groups(G.ENTITY, G.CONSTRUCT | G.GHOST), e.collider);
      const o = hit ? w.ownerOf(hit.collider) : null;
      if (o?.kind === 'construct') {
        w.emit('punch_blocked', { id: e.id, construct: o.id, target: id, pos: from.clone().addScaledVector(dir, hit!.timeOfImpact).toArray() });
        return;
      }
    }
    const push = new Vector3(d.x, 0, d.z).normalize();
    if (id === 'player') {
      const P = w.player;
      P.hitsTaken++;
      w.damagePlayer(P, ROBOT.punchDamage, 'punch', e.id, { knock: 0 });
      P.vel.addScaledVector(push, 7);
      P.vel.y = Math.max(P.vel.y, 3.5);
      w.emit('player_hit', { from: e.id, kind: 'punch', damage: ROBOT.punchDamage });
    } else if (civ) {
      civ.body.applyImpulse(push.clone().multiplyScalar(civ.body.mass() * 6).setY(civ.body.mass() * 3), true);
      w.damageEntity(civ, 100, 'robot', null);
    }
    w.emit('robot_punch', { id: e.id, target: id, hit: true, pos: fist.toArray() });
  }

  /** A robot took a hard hit (contact force from World.handleEvents). */
  knock(e: Entity, from: Vector3 | null, dv: number, by: string | null) {
    const ai = e.ai;
    if (ai?.kind !== 'robot' || dv < 5 || e.heldBy || e.restrainedBy) return;
    if (ai.mode !== 'walk' && ai.mode !== 'getup') return;
    const p = this.w.entityPos(e);
    const dir = from ? p.clone().sub(from) : this.w.entityVel(e);
    this.enterKnocked(e, ai, dir, by);
  }

  private enterKnocked(e: Entity, ai: RobotAI, push: Vector3, by: string | null) {
    if (ai.mode === 'down') return;
    ai.mode = 'knocked';
    ai.modeT = 0;
    ai.punchT = -1;
    e.body.lockRotations(false, true);
    const h = new Vector3(push.x, 0, push.z);
    if (h.lengthSq() < 1e-4) h.set(Math.sin(ai.yaw), 0, Math.cos(ai.yaw)); // fall backwards
    h.normalize();
    // tip over in the direction of the hit: torque about UP × h
    const J = e.body.mass() * 0.6;
    e.body.applyTorqueImpulse({ x: h.z * J, y: 0, z: -h.x * J }, true);
    this.w.emit('entity_knocked', { id: e.id, entityType: e.type, by: by ?? this.w.creditOf(e), pos: this.w.entityPos(e).toArray() });
  }

  private enterDown(e: Entity, ai: RobotAI) {
    const wasUp = ai.mode === 'walk' || ai.mode === 'getup';
    ai.mode = 'down';
    ai.punchT = -1;
    e.body.lockRotations(false, true);
    if (wasUp) {
      const J = e.body.mass() * 0.5;
      e.body.applyTorqueImpulse({ x: Math.cos(ai.yaw) * J, y: 0, z: -Math.sin(ai.yaw) * J }, true);
    }
  }

  private startGetup(e: Entity, ai: RobotAI, p: Vector3) {
    const r = e.body.rotation();
    const q = new Quaternion(r.x, r.y, r.z, r.w);
    // still (nearly) upright: just carry on
    const up = UP.clone().applyQuaternion(q);
    ai.getupFrom = q;
    ai.getupY0 = p.y;
    const hit = this.w.pw.castRay(new this.w.R.Ray({ x: p.x, y: p.y + 0.2, z: p.z }, { x: 0, y: -1, z: 0 }), 6, true, undefined, groups(G.ENTITY, G.WORLD | G.CONSTRUCT), e.collider);
    ai.getupY1 = (hit ? p.y + 0.2 - hit.timeOfImpact : p.y - 0.5) + ROBOT.standY;
    if (up.y > 0.95) ai.getupY1 = Math.max(p.y, ai.getupY1);
    // face the way the head was pointing so the get-up reads naturally
    const fwd = new Vector3(0, 0, -1).applyQuaternion(q);
    if (Math.hypot(fwd.x, fwd.z) > 0.2) ai.yaw = Math.atan2(-fwd.x, -fwd.z);
    e.body.lockRotations(true, true);
    e.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
    ai.mode = 'getup';
    ai.modeT = 0;
  }

  private updateBystander(e: Entity, ai: CivilianAI, p: Vector3, v: Vector3, dt: number) {
    const w = this.w;
    // carried by a construct
    if (e.heldBy) {
      if (ai.carriedBy !== e.heldBy) { ai.carriedBy = e.heldBy; ai.carryStart = p.clone(); }
      ai.releasedAt = -1;
    } else if (ai.carriedBy && ai.releasedAt < 0) ai.releasedAt = w.time;
    if (ai.carriedBy && !e.heldBy && w.time - ai.releasedAt > 4) { ai.carriedBy = null; ai.carryStart = null; }

    if ((w.tick + ai.phase) % 6 === 0) this.scanThreat(e, ai, p);
    const threat = ai.threat ? w.entities.get(ai.threat) : undefined;
    if (!threat || !this.hostileActive(threat)) { ai.threat = null; ai.threatDist = Infinity; }

    if (!ai.saved && !e.hurt) this.checkSaved(e, ai, p, dt);
    if (e.heldBy || e.restrainedBy || e.hurt) { if (ai.mode === 'flee') ai.mode = 'idle'; return; }

    const want = _a.set(0, 0, 0);
    if (ai.threat && ai.threatDist < CIVILIAN.fleeRadius) {
      const tp = w.entityPos(threat!, _b);
      if (ai.mode !== 'flee') { ai.mode = 'flee'; w.emit('civilian_flee', { id: e.id, from: threat!.id }); }
      const away = new Vector3(p.x - tp.x, 0, p.z - tp.z);
      if (away.lengthSq() < 1e-4) away.set(1, 0, 0);
      away.normalize();
      this.steer(e, ai, p, away, 2.5, true, null, want).multiplyScalar(CIVILIAN.speed);
    } else if (ai.mode === 'flee' && (!ai.threat || ai.threatDist > CIVILIAN.fleeRadius + 3)) {
      ai.mode = 'idle';
    } else if (ai.mode === 'flee') {
      const tp = w.entityPos(threat!, _b);
      this.steer(e, ai, p, new Vector3(p.x - tp.x, 0, p.z - tp.z).normalize(), 2.5, true, null, want).multiplyScalar(CIVILIAN.speed);
    }
    if (Math.abs(v.y) < 4) this.drive(e, v, want, CIVILIAN.accel, dt);
  }

  private scanThreat(e: Entity, ai: CivilianAI, p: Vector3) {
    let best: string | null = null;
    let bd = CIVILIAN.threatRadius;
    for (const h of this.w.entities.values()) {
      if (!this.hostileActive(h) || h.type === 'turret') continue;
      const d = this.w.entityPos(h, _b).distanceTo(p);
      if (d < bd) { bd = d; best = h.id; }
    }
    ai.threat = best;
    ai.threatDist = best ? bd : Infinity;
    // shielded: the line to the threat passes through a construct
    if (best && !e.heldBy) {
      const tp = this.w.entityPos(this.w.entities.get(best)!, _b);
      const from = p.clone().setY(p.y + 0.4);
      const dir = tp.clone().sub(from);
      const len = dir.length();
      dir.divideScalar(Math.max(1e-3, len));
      const hit = this.w.pw.castRay(new this.w.R.Ray(from, dir), len, true, undefined, groups(G.ENTITY, G.WORLD | G.CONSTRUCT | G.GHOST), e.collider);
      const o = hit ? this.w.ownerOf(hit.collider) : null;
      if (o?.kind === 'construct') { ai.shieldBy = o.id; ai.shieldT += 6 * this.w.dt; }
      else if (e.restrainedBy) { ai.shieldBy = e.restrainedBy; ai.shieldT += 6 * this.w.dt; }
      else { ai.shieldT = 0; ai.shieldBy = null; }
    } else { ai.shieldT = 0; ai.shieldBy = null; }
  }

  private checkSaved(e: Entity, ai: CivilianAI, p: Vector3, _dt: number) {
    if (ai.carriedBy && ai.carryStart) {
      // carried a good distance, or delivered to the player, and no hostile close by
      const moved = Math.hypot(p.x - ai.carryStart.x, p.z - ai.carryStart.z);
      const delivered = p.distanceTo(this.w.player.pos) < 5 && moved > 1.5;
      if ((moved >= CIVILIAN.carryDist || delivered) && this.nearestHostileDist(p) >= CIVILIAN.safeDist) return this.markSaved(e, ai, ai.carriedBy, 'carried');
    }
    if (ai.shieldBy && ai.shieldT >= CIVILIAN.shieldTime) return this.markSaved(e, ai, ai.shieldBy, 'shielded');
  }

  private nearestHostileDist(p: Vector3) {
    let bd = Infinity;
    for (const h of this.w.entities.values()) {
      if (!this.hostileActive(h) || h.type === 'turret') continue;
      bd = Math.min(bd, this.w.entityPos(h, _b).distanceTo(p));
    }
    return bd;
  }

  private markSaved(e: Entity, ai: CivilianAI, by: string | null, how: 'carried' | 'shielded' | 'defended') {
    if (ai.saved || e.hurt) return;
    ai.saved = true;
    e.safe = true;
    this.w.emit('civilian_saved', { id: e.id, by, how, pos: this.w.entityPos(e).toArray() });
  }

  /** Called by World.damageEntity when something reaches 0 hp. */
  onDisabled(e: Entity, by: string | null) {
    if (!e.hostile || !by) return;
    // a construct took out a hostile menacing a bystander: that bystander was saved
    const p = this.w.entityPos(e);
    for (const id of this.bystanders) {
      const c = this.w.entities.get(id);
      const ai = c?.ai;
      if (!c || ai?.kind !== 'civilian' || ai.saved || c.hurt) continue;
      if (this.w.entityPos(c, _b).distanceTo(p) < CIVILIAN.threatRadius) this.markSaved(c, ai, by, 'defended');
    }
  }

  // ------------------------------------------------------------------ breaking + debris

  queueBreak(e: Entity, cause: string, by: string | null) {
    this.breakQueue.push({ id: e.id, cause, by });
  }

  private breakEntity(e: Entity, cause: string, by: string | null) {
    const w = this.w;
    const p = w.entityPos(e);
    const v = w.entityVel(e);
    const r = e.body.rotation();
    const q = new Quaternion(r.x, r.y, r.z, r.w);
    const size = e.type === 'crate' ? new Vector3(1.6, 1.6, 1.6) : (e.size ?? new Vector3(1, 1, 1)).clone();
    const colors = e.type === 'crate' ? [0xd99a55, 0xb87a3e] : [e.color ?? 0xd9d2c0];
    if (e.ai?.kind === 'block' && !e.ai.toppled) this.markToppled(e, e.ai, by ?? w.creditOf(e), p);
    w.removeEntity(e.id);
    this.spawnDebris(p, q, size, colors, v);
    w.emit('entity_destroyed', { id: e.id, entityType: e.type, cause, by, pos: p.toArray(), group: e.group });
  }

  /** Split a box into 4 chunks that fly apart and fade. */
  spawnDebris(center: Vector3, q: Quaternion, size: Vector3, colors: number[], vel: Vector3) {
    const R = this.w.R;
    // split the two longest axes in half
    const ax = [0, 1, 2].sort((a, b) => size.getComponent(b) - size.getComponent(a));
    const piece = size.clone();
    piece.setComponent(ax[0], size.getComponent(ax[0]) / 2);
    piece.setComponent(ax[1], size.getComponent(ax[1]) / 2);
    piece.multiplyScalar(0.88);
    for (let i = 0; i < 4; i++) {
      const off = new Vector3();
      off.setComponent(ax[0], (i & 1 ? 0.5 : -0.5) * size.getComponent(ax[0]) / 2);
      off.setComponent(ax[1], (i & 2 ? 0.5 : -0.5) * size.getComponent(ax[1]) / 2);
      off.applyQuaternion(q);
      const pos = center.clone().add(off);
      const h = ((this.debrisSeq * 2654435761) >>> 0) / 4294967296; // deterministic jitter
      const out = off.clone().normalize().multiplyScalar(2.5 + h * 2).add(vel.clone().multiplyScalar(0.6)).setY(Math.max(vel.y * 0.5, 0) + 2 + h * 2);
      const body = this.w.pw.createRigidBody(R.RigidBodyDesc.dynamic().setTranslation(pos.x, pos.y, pos.z).setRotation(q).setLinvel(out.x, out.y, out.z)
        .setAngvel({ x: (h - 0.5) * 8, y: (0.5 - h) * 6, z: (h * 7) % 4 - 2 }).setAngularDamping(0.5).setLinearDamping(0.1));
      const collider = this.w.pw.createCollider(R.ColliderDesc.cuboid(piece.x / 2, piece.y / 2, piece.z / 2).setDensity(100).setFriction(0.8)
        .setCollisionGroups(groups(G.DEBRIS, G.WORLD | G.CONSTRUCT | G.GHOST | G.ENTITY)), body);
      this.debris.push({ id: this.debrisSeq++, body, collider, size: piece.clone(), color: colors[i % colors.length], born: this.w.time, life: 2.6 + i * 0.25 + h * 0.5 });
    }
    while (this.debris.length > DEBRIS_CAP) this.removeDebris(this.debris[0]);
  }

  private removeDebris(d: Debris) {
    const i = this.debris.indexOf(d);
    if (i >= 0) this.debris.splice(i, 1);
    if (this.w.bodyAlive(d.body)) this.w.pw.removeRigidBody(d.body);
  }

  // ------------------------------------------------------------------ after physics

  postStep(dt: number) {
    const w = this.w;
    // breaks (deferred while held so the construct runtime never loses its cargo mid-tick)
    if (this.breakQueue.length) {
      const keep: typeof this.breakQueue = [];
      for (const b of this.breakQueue) {
        const e = w.entities.get(b.id);
        if (!e) continue;
        if (e.heldBy || e.restrainedBy) { keep.push(b); continue; }
        this.breakEntity(e, b.cause, b.by ?? w.creditOf(e));
      }
      this.breakQueue = keep;
    }
    // topples
    for (const id of this.blocks) {
      const e = w.entities.get(id);
      if (!e) { this.blocks.delete(id); continue; }
      const ai = e.ai as BlockAI;
      if (ai.toppled || e.body.isSleeping()) continue;
      const t = e.body.translation();
      const r = e.body.rotation();
      _q.set(r.x, r.y, r.z, r.w);
      const moved = Math.hypot(t.x - e.home.x, t.z - e.home.z);
      if (moved > 0.8 || e.home.y - t.y > 0.45 || _q.angleTo(ai.homeQuat) > 0.6) this.markToppled(e, ai, w.creditOf(e), _a.set(t.x, t.y, t.z));
    }
    // debris lifetime
    for (let i = this.debris.length - 1; i >= 0; i--) {
      const d = this.debris[i];
      if (w.time - d.born > d.life || d.body.translation().y < -50) this.removeDebris(d);
    }
    if (this.waves.enabled) this.updateWaves(dt);
    if (this.race) this.updateRace();
  }

  private markToppled(e: Entity, ai: BlockAI, by: string | null, p: Vector3) {
    ai.toppled = true;
    this.w.emit('block_toppled', { id: e.id, group: e.group, by, pos: p.toArray() });
    const st = e.group ? this.stacks.get(e.group) : undefined;
    if (!st) return;
    st.toppled++;
    if (!st.done && st.toppled >= Math.max(2, Math.ceil(st.total * 0.4))) {
      st.done = true;
      this.w.emit(st.kind === 'wall' ? 'wall_breached' : 'stack_toppled', { stack: st.id, kind: st.kind, count: st.toppled, total: st.total, by, pos: p.toArray() });
    }
  }

  // ------------------------------------------------------------------ waves

  private updateWaves(dt: number) {
    const w = this.w;
    const W = this.waves;
    // clear wrecks of earlier waves so the body count stays bounded
    for (const id of this.waveSpawned) {
      const e = w.entities.get(id);
      if (!e) { this.waveSpawned.delete(id); continue; }
      if (e.disabled && w.time - e.disabledAt > 12 && !e.heldBy) { w.removeEntity(id); this.waveSpawned.delete(id); this.robots.delete(id); }
    }
    W.timer -= dt;
    if (W.timer > 0) return;
    W.timer = W.period;
    W.wave++;
    let active = 0;
    for (const e of w.entities.values()) if (this.hostileActive(e) && e.type !== 'turret') active++;
    let room = W.cap - active;
    if (room <= 0) { w.emit('wave_skipped', { wave: W.wave, active }); return; }
    const k = W.wave;
    const plan: ('robot' | 'drone' | 'heavy')[] = [];
    for (let i = 0; i < Math.min(4, 1 + Math.floor(k / 2)); i++) plan.push('robot');
    if (k % 2 === 0) plan.push('drone');
    if (k % 3 === 0) plan.push('heavy');
    const ids: string[] = [];
    const positions: number[][] = [];
    const P = w.player.pos;
    const base = this.waveRng() * Math.PI * 2;
    for (let i = 0; i < plan.length && room > 0; i++) {
      const a = base + (i / plan.length) * Math.PI * 1.2;
      const r = W.spawnDist + this.waveRng() * 6;
      const x = P.x + Math.sin(a) * r, z = P.z + Math.cos(a) * r;
      const hit = w.pw.castRay(new w.R.Ray({ x, y: P.y + 60, z }, { x: 0, y: -1, z: 0 }), 200, true, undefined, groups(G.ENTITY, G.WORLD));
      if (!hit) continue;
      const gy = P.y + 60 - hit.timeOfImpact;
      let e: Entity;
      if (plan[i] === 'robot') { e = this.addRobot(new Vector3(x, gy + ROBOT.standY + 0.02, z), { label: 'robot' }); (e.ai as RobotAI).fromWave = true; }
      else e = w.addEntity('drone', new Vector3(x, gy + 7 + (i % 2) * 2, z), plan[i] === 'heavy' ? { variant: 'heavy' } : {});
      this.waveSpawned.add(e.id);
      ids.push(e.id);
      positions.push(w.entityPos(e).toArray());
      room--;
    }
    w.emit('wave_spawned', { wave: k, ids, positions, robots: plan.filter((x) => x === 'robot').length, drones: plan.filter((x) => x !== 'robot').length });
  }

  // ------------------------------------------------------------------ race

  private updateRace() {
    const r = this.race!;
    const w = this.w;
    const P = w.player.pos;
    const inLane = Math.abs(P.x) <= r.halfWidth + 3;
    if (r.finishedAt !== null && P.z > r.startZ + 3) { r.startedAt = null; r.finishedAt = null; w.emit('race_reset', {}); }
    if (r.startedAt === null) {
      if (r.prevZ > r.startZ && P.z <= r.startZ && inLane) { r.startedAt = w.time; w.emit('race_started', {}); }
    } else if (r.finishedAt === null && P.z <= r.finishZ && inLane) {
      r.finishedAt = w.time;
      const time = r.finishedAt - r.startedAt;
      const best = r.best === null || time < r.best;
      if (best) r.best = time;
      w.emit('race_finished', { time: Math.round(time * 100) / 100, best });
    }
    r.prevZ = P.z;
  }

  /** Forget per-entity bookkeeping when an entity is removed. */
  onRemoved(id: string) {
    this.bystanders.delete(id);
    this.blocks.delete(id);
    this.robots.delete(id);
    this.waveSpawned.delete(id);
  }
}
