import { Vector3, Quaternion, Matrix4 } from 'three';
import { G, ALL, groups, type RBody, type RCollider } from '../physics.js';
import type { World, Entity, Projectile } from '../world.js';
import type { Player } from '../player.js';
import { type ConstructSpec, type Behavior, type Ref, type Part, cloneSpec } from './spec.js';
import { partTransform, specExtent } from './geom.js';
import { LIMITS, validateSpec, type ValidationReport } from './validate.js';

const UP = new Vector3(0, 1, 0);
const Z = new Vector3(0, 0, 1);

interface StrikeState { target: Ref; speed: number; v: number; phase: 'go' | 'return' | 'done'; after: 'return' | 'stay' | 'dissolve'; repeat: number; hitIds: Set<string>; start: Vector3 }
interface GrabState { target: Ref; then: 'hold' | 'bring' | 'lift' | 'stay'; v: number; attached: boolean }
interface CargoState { entityId: string; local: Vector3 }

export interface Inst {
  body: RBody;
  colliders: RCollider[];
  pos: Vector3;
  quat: Quaternion;
  prevPos: Vector3;
  vel: Vector3;
  offset: Vector3; // arrangement offset in construct frame
  phase: number;
  strike: StrikeState | null;
  grab: GrabState | null;
  cargo: CargoState | null;
  liftBase: number | null;
  sweepLeft: number;
  shootCd: number;
  spinQ: Quaternion;
  alive: boolean;
}

export interface Construct {
  id: string;
  spec: ConstructSpec;
  insts: Inst[];
  born: number;
  expires: number;
  radius: number;
  extent: ReturnType<typeof specExtent>;
  mass: number;
  cost: number;
  clamps: string[];
  kinematic: boolean;
  frameQuat: Quaternion;
  frameOrigin: Vector3;
  version: number; // bumps on rebuild so the renderer can rebuild meshes
  explodeAt: number;
  name: string | null;
  impacts: number;
}

export interface SpawnResult { construct: Construct | null; report: ValidationReport }

const DRIVING = new Set(['hold', 'follow', 'orbit', 'strike', 'grab', 'sweep', 'lift', 'mount', 'home']);

function has<T extends Behavior['type']>(s: ConstructSpec, t: T): Extract<Behavior, { type: T }> | undefined {
  return s.behaviors.find((b) => b.type === t) as any;
}

export class ConstructRuntime {
  list = new Map<string, Construct>();
  byCollider = new Map<number, string>();
  focusId: string | null = null;
  energy = LIMITS.energyMax;
  private removed: Construct[] = [];
  /** Constructs removed since the renderer last looked (for dissolve effects). */
  drainRemoved() { const r = this.removed; this.removed = []; return r; }

  constructor(private w: World) {}

  get(id: string | null | undefined) { return id ? this.list.get(id) ?? null : null; }

  // ------------------------------------------------------------------ spawning

  /** Validate + place + build. The ONLY way a proposal becomes physics. */
  spawn(proposal: ConstructSpec, opts: { at?: { pos: Vector3; quat: Quaternion }; name?: string } = {}): SpawnResult {
    const report = validateSpec(proposal, this.energy);
    if (report.rejected) {
      this.w.emit('construct_rejected', { label: proposal.label, reason: report.rejected });
      return { construct: null, report };
    }
    const spec = report.spec;
    this.normalizeMotion(spec);
    const id = this.w.uid('c');
    const extent = specExtent(spec);
    const frame = opts.at ?? this.place(spec, extent);
    const c: Construct = {
      id, spec, insts: [], born: this.w.time, expires: this.w.time + spec.lifetime, radius: specExtent(spec, true).radius, extent,
      mass: 0, cost: report.cost, clamps: report.clamps, kinematic: spec.motion === 'kinematic', frameQuat: frame.quat.clone(), frameOrigin: frame.pos.clone(),
      version: 1, explodeAt: Infinity, name: opts.name ?? null, impacts: 0,
    };
    this.build(c, frame.pos, frame.quat);
    this.list.set(id, c);
    this.energy -= report.cost;
    this.focusId = id;
    this.initBehaviors(c);
    this.enforceBudget();
    this.w.emit('construct_formed', { id, label: spec.label, clamps: report.clamps, cost: Math.round(report.cost), source: spec.provenance.source });
    return { construct: c, report };
  }

  /** Re-validate an edited spec and rebuild in place (keeps pose, velocity and cargo). */
  modify(id: string, edited: ConstructSpec): { ok: boolean; report: ValidationReport } {
    const c = this.list.get(id);
    if (!c) return { ok: false, report: { spec: edited, clamps: [], rejected: 'nothing to modify', cost: 0, mass: 0 } };
    const refund = c.cost;
    const report = validateSpec(edited, this.energy + refund);
    if (report.rejected) { this.w.emit('construct_rejected', { label: edited.label, reason: report.rejected }); return { ok: false, report }; }
    const spec = report.spec;
    this.normalizeMotion(spec);
    const poses = c.insts.map((i) => ({ pos: i.pos.clone(), quat: i.quat.clone(), vel: i.vel.clone(), cargo: i.cargo, strike: i.strike, grab: i.grab, phase: i.phase }));
    const oldCount = c.insts.length;
    this.destroyBodies(c, false);
    c.spec = spec;
    c.extent = specExtent(spec);
    c.radius = specExtent(spec, true).radius;
    c.kinematic = spec.motion === 'kinematic';
    c.clamps = report.clamps;
    this.energy += refund - report.cost;
    c.cost = report.cost;
    c.expires = Math.max(c.expires, this.w.time + Math.min(spec.lifetime, 30));
    // rebuild at the first instance's pose; new instances fan out from there
    const base = poses[0] ?? { pos: c.frameOrigin, quat: c.frameQuat };
    this.build(c, base.pos, base.quat, poses.length > 1 && spec.count === oldCount ? poses : undefined);
    for (let i = 0; i < c.insts.length && i < poses.length; i++) {
      const inst = c.insts[i];
      inst.cargo = poses[i].cargo;
      inst.phase = poses[i].phase;
      if (!c.kinematic && inst.body.isDynamic()) inst.body.setLinvel(poses[i].vel, true);
    }
    // cargo beyond the new count is released
    for (let i = c.insts.length; i < poses.length; i++) if (poses[i].cargo) this.releaseEntity(poses[i].cargo!.entityId, new Vector3());
    c.version++;
    this.initBehaviors(c, true);
    this.focusId = id;
    this.w.emit('construct_modified', { id, label: spec.label, clamps: report.clamps });
    return { ok: true, report };
  }

  dismiss(id: string, reason = 'dismissed') {
    const c = this.list.get(id);
    if (!c) return;
    for (const i of c.insts) if (i.cargo) this.releaseEntity(i.cargo.entityId, i.vel.clone());
    this.releaseRestrained(c);
    for (const side of ['L', 'R'] as const) if (this.w.player.tethers[side]?.constructId === id) this.w.player.releaseTether(side);
    this.destroyBodies(c, true);
    this.list.delete(id);
    this.removed.push(c);
    this.energy = Math.min(LIMITS.energyMax, this.energy + c.cost * 0.3);
    if (this.focusId === id) this.focusId = [...this.list.keys()].pop() ?? null;
    this.w.emit('construct_dismissed', { id, label: c.spec.label, reason });
  }

  dismissAll() { for (const id of [...this.list.keys()]) this.dismiss(id); }

  /** Let go of held entities / stop holding (construct stays). */
  release(id: string) {
    const c = this.list.get(id);
    if (!c) return false;
    let any = false;
    for (const i of c.insts) if (i.cargo) { this.releaseEntity(i.cargo.entityId, i.vel.clone()); i.cargo = null; any = true; }
    if (this.releaseRestrained(c)) any = true;
    c.spec.behaviors = c.spec.behaviors.filter((b) => !['grab', 'restrain', 'lift'].includes(b.type));
    for (const i of c.insts) { i.grab = null; i.liftBase = null; }
    this.w.emit('construct_released', { id });
    return any;
  }

  /** Fling cargo (or the construct itself) toward a target. */
  throwAt(id: string, target: Ref, speed = 40) {
    const c = this.list.get(id);
    if (!c) return false;
    const tp = this.refPos(target);
    let thrown = false;
    for (const inst of c.insts) {
      const from = inst.cargo ? this.entityPosById(inst.cargo.entityId) ?? inst.pos : inst.pos;
      const aimAt = tp ?? from.clone().add(this.w.player.look(new Vector3()).multiplyScalar(30));
      const v = ballistic(from, aimAt, Math.max(speed, inst.vel.length() * 1.1), this.w.pw.gravity.y);
      if (inst.cargo) {
        this.releaseEntity(inst.cargo.entityId, v);
        inst.cargo = null;
        thrown = true;
      } else {
        // throw the construct itself: it becomes a free physical object
        this.makeDynamic(c, inst, v);
        thrown = true;
      }
    }
    c.spec.behaviors = c.spec.behaviors.filter((b) => !['grab', 'hold', 'orbit', 'follow', 'lift', 'restrain', 'mount'].includes(b.type));
    for (const i of c.insts) i.grab = null;
    this.w.emit('construct_throw', { id, speed: Math.round(speed) });
    return thrown;
  }

  // ------------------------------------------------------------------ placement

  private normalizeMotion(spec: ConstructSpec) {
    const types = new Set(spec.behaviors.map((b) => b.type));
    const drives = [...types].some((t) => DRIVING.has(t));
    if (types.has('crush')) spec.motion = 'dynamic';
    else if (drives && spec.motion !== 'dynamic') spec.motion = 'kinematic';
    else if (types.has('thrust') && spec.motion === 'static') spec.motion = 'dynamic';
    if (types.has('mount') || types.has('hold')) spec.passPlayer = true;
    const orbit = has(spec, 'orbit');
    if (orbit && orbit.around.kind === 'self') spec.passPlayer = true;
    if (spec.placement.at === 'self' || spec.placement.at === 'around_target' && spec.placement.target?.kind === 'self') spec.passPlayer = true;
  }

  refPos(ref: Ref | undefined, out = new Vector3()): Vector3 | null {
    if (!ref) return null;
    const p = this.w.player;
    switch (ref.kind) {
      case 'self': return out.copy(p.pos);
      case 'aim': return out.copy(p.aim().point);
      case 'entity': { const e = this.w.entities.get(ref.id); return e ? this.w.entityPos(e, out) : null; }
      case 'entities': case 'all_enemies': { const e = this.refEntities(ref)[0]; return e ? this.w.entityPos(e, out) : null; }
      case 'construct': { const c = this.list.get(ref.id); return c && c.insts[0] ? out.copy(c.insts[0].pos) : null; }
      case 'point': return out.set(ref.p[0], ref.p[1], ref.p[2]);
      case 'nearest_enemy': { const e = this.nearestEnemy(); return e ? this.w.entityPos(e, out) : null; }
      case 'threat': { const e = this.threat(); return e ? this.w.entityPos(e, out) : null; }
      default: return null;
    }
  }

  refEntities(ref: Ref | undefined): Entity[] {
    if (!ref) return [];
    if (ref.kind === 'entity') { const e = this.w.entities.get(ref.id); return e ? [e] : []; }
    if (ref.kind === 'entities') return ref.ids.map((id) => this.w.entities.get(id)).filter((e): e is Entity => !!e);
    if (ref.kind === 'all_enemies') return [...this.w.entities.values()].filter((e) => e.hostile && !e.disabled).sort((a, b) => this.dist(a) - this.dist(b));
    if (ref.kind === 'nearest_enemy') { const e = this.nearestEnemy(); return e ? [e] : []; }
    if (ref.kind === 'threat') { const e = this.threat(); return e ? [e] : []; }
    return [];
  }

  private dist(e: Entity) { return this.w.entityPos(e).distanceTo(this.w.player.pos); }

  nearestEnemy(): Entity | null {
    let best: Entity | null = null;
    let bd = Infinity;
    for (const e of this.w.entities.values()) {
      if (!e.hostile || e.disabled) continue;
      const d = this.dist(e);
      if (d < bd) { bd = d; best = e; }
    }
    return best;
  }

  threat(): Entity | null {
    // the shooter of the nearest incoming projectile, else nearest hostile
    let best: Entity | null = null;
    let bd = Infinity;
    for (const pr of this.w.projectiles.values()) {
      if (pr.reflected || !pr.from) continue;
      const t = pr.body.translation();
      const d = this.w.player.pos.distanceTo(new Vector3(t.x, t.y, t.z));
      const e = this.w.entities.get(pr.from);
      if (e && d < bd) { bd = d; best = e; }
    }
    return best ?? this.nearestEnemy();
  }

  private groundBelow(p: Vector3, maxDown = 200): number | null {
    const R = this.w.R;
    const hit = this.w.pw.castRay(new R.Ray({ x: p.x, y: p.y + 0.5, z: p.z }, { x: 0, y: -1, z: 0 }), maxDown, true, undefined, groups(G.PLAYER, G.WORLD | G.CONSTRUCT));
    return hit ? p.y + 0.5 - hit.timeOfImpact : null;
  }

  /** Resolve a placement into a world frame. Deterministic given world state. */
  place(spec: ConstructSpec, ext = specExtent(spec)): { pos: Vector3; quat: Quaternion } {
    const pl = spec.placement;
    const P = this.w.player;
    const fwd = P.forward(new Vector3());
    const look = P.look(new Vector3());
    const yawAway = (dir: Vector3) => new Quaternion().setFromAxisAngle(UP, Math.atan2(dir.x, dir.z));
    let pos = new Vector3();
    let quat = yawAway(fwd);
    const standing = ext.min.y > -0.35 * ext.size.y; // parts built to stand on y=0
    const liftToGround = (p: Vector3) => {
      const g = this.groundBelow(p.clone().add(new Vector3(0, 2, 0)), 300);
      if (g !== null) p.y = g - ext.min.y + 0.02;
      return p;
    };
    const tgtPos = pl.target ? this.refPos(pl.target) : null;
    switch (pl.at) {
      case 'aim': {
        const a = P.aim(220);
        const dist = a.point.distanceTo(P.pos);
        const hitSomething = !!a.collider && dist < 219;
        pos.copy(a.point);
        const own = this.w.ownerOf(a.collider);
        if (hitSomething && own?.kind === 'entity') {
          const e = this.w.entities.get(own.id)!;
          pos.copy(this.w.entityPos(e));
        } else if (hitSomething && a.normal.y > 0.6) {
          pos.y += -ext.min.y + 0.02; // rest on the surface
        } else if (hitSomething) {
          pos.addScaledVector(a.normal, Math.max(ext.size.z, ext.size.x) / 2 + 0.3); // stand off walls
          if (standing) liftToGround(pos);
        } else {
          pos.copy(P.pos).addScaledVector(look, Math.min(40, 12 + ext.radius * 2));
        }
        quat = yawAway(new Vector3().subVectors(pos, P.pos).setY(0).lengthSq() > 1 ? new Vector3().subVectors(pos, P.pos).setY(0).normalize() : fwd);
        break;
      }
      case 'front': {
        const d = pl.distance ?? 2.5 + ext.size.z / 2 + ext.radius * 0.3;
        pos.copy(P.pos).addScaledVector(standing ? fwd : look, d);
        if (standing) { pos.y = P.pos.y; liftToGround(pos); }
        quat = standing ? yawAway(fwd) : quatFromDir(look);
        break;
      }
      case 'ground_front': {
        const hv = new Vector3(P.vel.x, 0, P.vel.z);
        const dir = hv.length() > 4 ? hv.clone().normalize() : fwd;
        const d = pl.distance ?? 2 + Math.min(18, hv.length() * 0.35) + ext.size.z / 2;
        pos.copy(P.pos).addScaledVector(dir, d);
        liftToGround(pos);
        quat = yawAway(dir);
        break;
      }
      case 'self': pos.copy(P.pos).add(new Vector3(0, standing ? -0.9 - ext.min.y : 0, 0)); break;
      case 'under_self': {
        pos.copy(P.pos);
        pos.y = P.pos.y - 0.9 - ext.max.y - 0.05;
        quat = yawAway(fwd);
        break;
      }
      case 'behind_self': pos.copy(P.pos).addScaledVector(fwd, -(2.5 + ext.size.z / 2)); if (standing) liftToGround(pos); break;
      case 'above_self': pos.copy(P.pos).add(new Vector3(0, 2 + ext.size.y / 2 - ext.center.y, 0)); break;
      case 'hand': pos.copy(P.handPos('R')).addScaledVector(look, 1 + ext.size.z / 2); quat = quatFromDir(look); break;
      case 'target': pos.copy(tgtPos ?? P.aim().point); break;
      case 'under_target': {
        const t = tgtPos ?? P.aim().point;
        pos.copy(t);
        const e = pl.target ? this.refEntities(pl.target)[0] : null;
        if (e) {
          // predict where a falling target will come down
          const v = this.w.entityVel(e);
          const g = this.groundBelow(t) ?? 0;
          const h = Math.max(0, t.y - g);
          const vy = v.y;
          const tt = (vy + Math.sqrt(vy * vy + 2 * 22 * h)) / 22;
          pos.x += v.x * tt; pos.z += v.z * tt;
        }
        liftToGround(pos);
        break;
      }
      case 'above_target': pos.copy(tgtPos ?? P.aim().point).add(new Vector3(0, (pl.height ?? 10) + ext.size.y / 2 - ext.center.y, 0)); break;
      case 'around_target': {
        const t = tgtPos ?? P.aim().point;
        pos.copy(t).sub(new Vector3(0, ext.center.y, 0));
        break;
      }
      case 'between': {
        const t = tgtPos ?? (this.threat() ? this.w.entityPos(this.threat()!) : P.aim().point);
        const dir = new Vector3().subVectors(t, P.pos);
        const flat = dir.clone().setY(0);
        if (flat.lengthSq() < 1e-4) flat.copy(fwd);
        flat.normalize();
        pos.copy(P.pos).addScaledVector(flat, pl.distance ?? 3.5);
        if (standing) { pos.y = P.pos.y; liftToGround(pos); } else pos.y = P.pos.y + 0.6;
        quat = yawAway(flat);
        break;
      }
      case 'span': {
        const start = P.pos.clone().setY(P.pos.y - 0.95).addScaledVector(fwd, 0.5);
        const end = tgtPos ?? P.aim(250).point;
        const d = new Vector3().subVectors(end, start);
        const len = d.length();
        const nominal = Math.max(0.5, ext.size.z);
        spec.stretch = [spec.stretch?.[0] ?? 1, spec.stretch?.[1] ?? 1, Math.min(LIMITS.stretch[1], Math.max(0.3, (len / nominal) / spec.scale))];
        pos.copy(start).addScaledVector(d, 0.5);
        quat = quatFromDir(d.normalize());
        // put the walkable top of the deck on the line from the player's feet to the target
        const walk = spec.parts.filter((p) => p.solid !== false && ['deck', 'body', 'belt', 'chute', 'collider', 'mesh', 'pad'].includes(p.role ?? 'body'));
        const deckTop = walk.length ? Math.max(...walk.map((p) => p.pos[1] + p.size[1] / 2)) * spec.scale * (spec.stretch?.[1] ?? 1) : ext.min.y + 0.2 * ext.size.y;
        pos.y -= deckTop - 0.02;
        break;
      }
    }
    if (pl.height && pl.at !== 'above_target') pos.y += pl.height;
    switch (pl.orient) {
      case 'face_player': quat.multiply(new Quaternion().setFromAxisAngle(UP, Math.PI)); break;
      case 'along_look': quat = quatFromDir(look); break;
      case 'toward_target': if (tgtPos) quat = quatFromDir(tgtPos.clone().sub(pos).normalize()); break;
      case 'vertical': quat.multiply(new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), -Math.PI / 2)); break;
      case 'flat': break;
      default: break;
    }
    if (pl.yaw) quat.multiply(new Quaternion().setFromAxisAngle(UP, (pl.yaw * Math.PI) / 180));
    if (pl.tilt) quat.multiply(new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), (-pl.tilt * Math.PI) / 180));
    return { pos, quat };
  }

  private arrangementOffsets(spec: ConstructSpec, ext: ReturnType<typeof specExtent>): Vector3[] {
    const n = spec.count;
    if (n <= 1) return [new Vector3()];
    const w = Math.max(ext.size.x, 0.5) + spec.spacing * 0.5;
    const out: Vector3[] = [];
    for (let i = 0; i < n; i++) {
      const c = i - (n - 1) / 2;
      switch (spec.arrangement) {
        case 'stack': out.push(new Vector3(0, i * ext.size.y * 1.02, 0)); break;
        case 'ring': case 'scatter': {
          const r = Math.max(ext.radius * 1.4, (n * w) / (2 * Math.PI));
          const a = (i / n) * Math.PI * 2 + (spec.arrangement === 'scatter' ? Math.sin(i * 12.9898) * 0.6 : 0);
          out.push(new Vector3(Math.sin(a) * r, 0, Math.cos(a) * r));
          break;
        }
        case 'grid': { const k = Math.ceil(Math.sqrt(n)); out.push(new Vector3(((i % k) - (k - 1) / 2) * w, 0, (Math.floor(i / k) - (k - 1) / 2) * w)); break; }
        case 'arc': { const a = c * 0.35; out.push(new Vector3(Math.sin(a) * 6, 0, Math.cos(a) * 6 - 6)); break; }
        default: out.push(new Vector3(c * w, 0, 0));
      }
    }
    return out;
  }

  // ------------------------------------------------------------------ building bodies

  private build(c: Construct, origin: Vector3, quat: Quaternion, poses?: { pos: Vector3; quat: Quaternion }[]) {
    const R = this.w.R;
    const spec = c.spec;
    const offs = this.arrangementOffsets(spec, c.extent);
    c.insts = [];
    let massTotal = 0;
    for (let k = 0; k < offs.length; k++) {
      const pos = poses?.[k]?.pos.clone() ?? origin.clone().add(offs[k].clone().applyQuaternion(quat));
      const q = poses?.[k]?.quat.clone() ?? quat.clone();
      let desc = spec.motion === 'static' ? R.RigidBodyDesc.fixed() : spec.motion === 'kinematic' ? R.RigidBodyDesc.kinematicPositionBased() : R.RigidBodyDesc.dynamic().setCcdEnabled(true).setLinearDamping(0.05).setAngularDamping(0.3);
      desc = desc.setTranslation(pos.x, pos.y, pos.z).setRotation(q);
      const body = this.w.pw.createRigidBody(desc);
      const colliders: RCollider[] = [];
      for (let pi = 0; pi < spec.parts.length; pi++) {
        const part = spec.parts[pi];
        if (part.solid === false) continue;
        for (const cd of this.partColliders(part, spec)) {
          const col = this.w.pw.createCollider(cd, body);
          colliders.push(col);
          this.w.owners.set(col.handle, { kind: 'construct', id: c.id, part: pi });
          this.byCollider.set(col.handle, c.id);
        }
      }
      if (!colliders.length) {
        // purely visual construct: give it a tiny sensor so it still has a body
        const col = this.w.pw.createCollider(R.ColliderDesc.ball(0.2).setSensor(true).setCollisionGroups(groups(G.GHOST, 0)), body);
        colliders.push(col);
        this.w.owners.set(col.handle, { kind: 'construct', id: c.id, part: 0 });
        this.byCollider.set(col.handle, c.id);
      }
      massTotal += body.mass();
      c.insts.push({
        body, colliders, pos, quat: q, prevPos: pos.clone(), vel: new Vector3(), offset: offs[k], phase: (k / offs.length) * Math.PI * 2,
        strike: null, grab: null, cargo: null, liftBase: null, sweepLeft: 0, shootCd: 0.2 * k, spinQ: new Quaternion(), alive: true,
      });
    }
    // virtual mass for kinematic/static constructs (used for strike impulses)
    c.mass = spec.motion === 'dynamic' ? massTotal / Math.max(1, offs.length) : Math.min(LIMITS.maxVirtualMass, this.volume(spec) * spec.density);
  }

  private volume(spec: ConstructSpec) {
    let v = 0;
    for (const p of spec.parts) if (p.solid !== false) { const t = partTransform(p, spec.scale, spec.stretch); v += t.size.x * t.size.y * t.size.z * 0.6; }
    return v;
  }

  private partColliders(part: Part, spec: ConstructSpec) {
    const R = this.w.R;
    const t = partTransform(part, spec.scale, spec.stretch);
    const s = t.size;
    const member = spec.passPlayer ? G.GHOST : G.CONSTRUCT;
    const filter = ALL & ~G.CARGO;
    const finish = (cd: any, pos = t.pos, rot = t.quat) =>
      cd.setTranslation(pos.x, pos.y, pos.z).setRotation(rot).setCollisionGroups(groups(member, filter))
        .setDensity(spec.density).setRestitution(Math.min(1, spec.restitution)).setFriction(spec.friction)
        .setActiveCollisionTypes(R.ActiveCollisionTypes.DEFAULT | R.ActiveCollisionTypes.KINEMATIC_FIXED | R.ActiveCollisionTypes.DYNAMIC_KINEMATIC)
        .setActiveEvents(R.ActiveEvents.COLLISION_EVENTS);
    const out: any[] = [];
    const hollow = part.role === 'shell';
    switch (part.shape) {
      case 'box': out.push(finish(R.ColliderDesc.cuboid(s.x / 2, s.y / 2, s.z / 2))); break;
      case 'sphere':
        if (hollow) out.push(...this.shellColliders(s.x / 2, t.pos, t.quat, false).map((x) => finish(x.cd, x.pos, x.rot)));
        else out.push(finish(R.ColliderDesc.ball((s.x + s.y + s.z) / 6)));
        break;
      case 'cylinder': out.push(finish(R.ColliderDesc.cylinder(s.y / 2, Math.max(0.02, (s.x + s.z) / 4)))); break;
      case 'capsule': { const r = Math.max(0.02, (s.x + s.z) / 4); out.push(finish(R.ColliderDesc.capsule(Math.max(0.01, s.y / 2 - r), r))); break; }
      case 'cone': out.push(finish(R.ColliderDesc.cone(s.y / 2, Math.max(0.02, (s.x + s.z) / 4)))); break;
      case 'wedge': {
        const hx = s.x / 2, hy = s.y / 2, hz = s.z / 2;
        const cd = R.ColliderDesc.convexHull(new Float32Array([-hx, -hy, -hz, hx, -hy, -hz, -hx, -hy, hz, hx, -hy, hz, -hx, hy, hz, hx, hy, hz]));
        if (cd) out.push(finish(cd));
        break;
      }
      case 'torus': {
        const R0 = s.x / 2 - s.y / 4;
        const tube = Math.max(0.05, s.y / 2);
        const n = 12;
        for (let i = 0; i < n; i++) {
          const a = (i / n) * Math.PI * 2;
          const lp = new Vector3(Math.cos(a) * R0, 0, Math.sin(a) * R0).applyQuaternion(t.quat).add(t.pos);
          const lr = t.quat.clone().multiply(new Quaternion().setFromAxisAngle(UP, -a));
          const segLen = (2 * Math.PI * R0) / n / 2 + tube * 0.3;
          out.push(finish(R.ColliderDesc.cuboid(tube, tube, segLen), lp, lr));
        }
        break;
      }
      case 'dome':
        out.push(...this.shellColliders(s.x / 2, t.pos, t.quat, true, s.y).map((x) => finish(x.cd, x.pos, x.rot)));
        break;
    }
    return out;
  }

  /** Hollow shells from thin panels so things can be INSIDE bubbles/domes/cages. */
  private shellColliders(r: number, center: Vector3, rot: Quaternion, hemi: boolean, height?: number) {
    const R = this.w.R;
    const out: { cd: any; pos: Vector3; rot: Quaternion }[] = [];
    const lats = hemi ? [15, 45, 75] : [-60, -20, 20, 60];
    const yScale = hemi && height ? height / r : 1;
    for (const latDeg of lats) {
      const lat = (latDeg * Math.PI) / 180;
      const n = latDeg === 75 || Math.abs(latDeg) === 60 ? 6 : 10;
      const ringR = Math.cos(lat) * r;
      for (let i = 0; i < n; i++) {
        const a = (i / n) * Math.PI * 2;
        const lp = new Vector3(Math.cos(a) * ringR, Math.sin(lat) * r * yScale, Math.sin(a) * ringR);
        const normal = lp.clone().normalize();
        const q = new Quaternion().setFromUnitVectors(Z, normal);
        const w = (2 * Math.PI * ringR) / n / 2 + 0.2;
        const h = (r * 0.7) / 2;
        out.push({ cd: R.ColliderDesc.cuboid(w, h, 0.12), pos: lp.applyQuaternion(rot).add(center), rot: rot.clone().multiply(q) });
      }
    }
    // cap
    out.push({ cd: R.ColliderDesc.cylinder(0.12, r * 0.35), pos: new Vector3(0, r * yScale * 0.98, 0).applyQuaternion(rot).add(center), rot: rot.clone() });
    if (!hemi) out.push({ cd: R.ColliderDesc.cylinder(0.12, r * 0.35), pos: new Vector3(0, -r * 0.98, 0).applyQuaternion(rot).add(center), rot: rot.clone() });
    return out;
  }

  private destroyBodies(c: Construct, releaseCargo: boolean) {
    for (const i of c.insts) {
      if (releaseCargo && i.cargo) this.releaseEntity(i.cargo.entityId, i.vel.clone());
      for (const col of i.colliders) { this.w.owners.delete(col.handle); this.byCollider.delete(col.handle); }
      if (this.w.bodyAlive(i.body)) this.w.pw.removeRigidBody(i.body);
      i.alive = false;
    }
  }

  private makeDynamic(c: Construct, inst: Inst, vel: Vector3) {
    const R = this.w.R;
    inst.body.setBodyType(R.RigidBodyType.Dynamic, true);
    inst.body.enableCcd(true);
    inst.body.setLinvel(vel, true);
    c.spec.motion = 'dynamic';
    c.kinematic = false;
    // thrown things should collide with the player again
    if (c.spec.passPlayer) {
      c.spec.passPlayer = false;
      for (const col of inst.colliders) col.setCollisionGroups(groups(G.CONSTRUCT, ALL & ~G.CARGO));
    }
    c.expires = Math.min(c.expires, this.w.time + 20);
  }

  // ------------------------------------------------------------------ behaviours

  private initBehaviors(c: Construct, keepState = false) {
    const s = c.spec;
    const strike = has(s, 'strike');
    const grab = has(s, 'grab');
    const restrain = has(s, 'restrain');
    const crush = has(s, 'crush');
    const explode = has(s, 'explode');
    const sweep = has(s, 'sweep');
    const lift = has(s, 'lift');
    const tether = has(s, 'tether');
    const targets = (ref: Ref) => this.refEntities(ref);
    c.insts.forEach((inst, k) => {
      if (strike && (!keepState || !inst.strike || inst.strike.phase === 'done')) {
        const ents = targets(strike.target);
        const tgt: Ref = ents.length > 1 ? { kind: 'entity', id: ents[k % ents.length].id } : strike.target;
        // momentum: the player's velocity toward the target carries into the blow
        const tp = this.refPos(tgt);
        const toward = tp ? Math.max(0, this.w.player.vel.dot(tp.clone().sub(inst.pos).normalize())) : 0;
        const thrust = has(s, 'thrust');
        inst.strike = { target: tgt, speed: Math.min(LIMITS.strikeSpeed, strike.speed + (thrust ? thrust.accel * 0.6 : 0) + toward), v: toward * 0.8, phase: 'go', after: strike.after, repeat: strike.repeat ?? 0, hitIds: new Set(), start: inst.pos.clone() };
      }
      if (grab && !inst.cargo && (!keepState || !inst.grab)) {
        const ents = targets(grab.target);
        const tgt: Ref = ents.length > 1 ? { kind: 'entity', id: ents[k % ents.length].id } : grab.target;
        inst.grab = { target: tgt, then: grab.then, v: Math.max(0, this.w.player.vel.length() * 0.5), attached: false };
      }
      if (restrain && !inst.cargo) {
        const ents = targets(restrain.target);
        const e = ents[k % Math.max(1, ents.length)];
        if (e && !e.heldBy) {
          const ep = this.w.entityPos(e);
          this.attachEntity(c, inst, e, ep);
          e.restrainedBy = c.id;
        }
      }
      if (crush) {
        const tp = this.refPos(crush.target);
        if (tp && inst.body.isDynamic()) {
          const d = tp.clone().sub(inst.pos);
          const t = Math.max(0.2, Math.sqrt(Math.max(0.1, -d.y) * 2 / 40));
          inst.body.setLinvel({ x: d.x / t, y: -30, z: d.z / t }, true);
        }
      }
      if (sweep && !keepState) inst.sweepLeft = sweep.width;
      if (lift && !keepState) inst.liftBase = inst.pos.y;
    });
    if (explode) c.explodeAt = explode.onContact ? Infinity : this.w.time + explode.delay;
    if (tether && tether.toPlayer) {
      const P = this.w.player;
      // an anchor lying on the ground is useless for swinging/ziplining: raise it on a post
      for (const inst of c.insts) {
        const g = this.groundBelow(inst.pos.clone().add(new Vector3(0, 0.5, 0)), 3);
        if (g !== null && inst.pos.y - g < 2 && c.kinematic === false && c.spec.motion === 'static' && c.spec.concept === 'rope') {
          inst.pos.y += 7;
          inst.body.setTranslation(inst.pos, true);
        }
      }
      const side = !P.tethers.R ? 'R' : !P.tethers.L ? 'L' : 'R';
      const tp = this.refPos(tether.target.kind === 'none' ? { kind: 'construct', id: c.id } : tether.target) ?? c.insts[0]?.pos;
      if (tp) {
        const e = this.refEntities(tether.target)[0];
        const body = e ? e.body : tether.target.kind === 'none' || tether.target.kind === 'construct' ? c.insts[0]?.body ?? null : null;
        const tt = P.attachTether(side, tp, body, c.id);
        tt.autoReel = true;
        if (e && e.type === 'drone') e.stunnedUntil = this.w.time + 4;
      }
    }
  }

  preStep(dt: number) {
    this.energy = Math.min(LIMITS.energyMax, this.energy + LIMITS.energyRegen * dt);
    const P = this.w.player;
    for (const c of [...this.list.values()]) {
      if (this.w.time >= c.expires) { this.dismiss(c.id, 'expired'); continue; }
      if (this.w.time >= c.explodeAt) { this.explode(c); continue; }
      const s = c.spec;
      for (const inst of c.insts) {
        if (!inst.alive) continue;
        inst.prevPos.copy(inst.pos);
        if (inst.body.isKinematic()) this.driveKinematic(c, inst, dt);
        else if (inst.body.isDynamic()) this.driveDynamic(c, inst, dt);
        this.applyFields(c, inst, dt);
        this.applyShoot(c, inst, dt);
        if (has(s, 'cushion')) this.applyCushion(c, inst, dt);
        if (has(s, 'conveyor')) this.applyConveyorToEntities(c, inst);
        if (has(s, 'launch')) this.applyLaunchToEntities(c, inst);
      }
      const mount = has(s, 'mount');
      if (mount) this.applyMount(c, mount, P);
    }
  }

  postStep(dt: number) {
    for (const c of this.list.values()) {
      for (const inst of c.insts) {
        if (!inst.alive) continue;
        if (!inst.body.isKinematic()) {
          const t = inst.body.translation();
          const r = inst.body.rotation();
          inst.pos.set(t.x, t.y, t.z);
          inst.quat.set(r.x, r.y, r.z, r.w);
          if (inst.pos.y < -300) { c.expires = 0; }
        }
        inst.vel.copy(inst.pos).sub(inst.prevPos).divideScalar(dt);
        // keep cargo glued (kinematic cargo follows its holder)
        if (inst.cargo) {
          const e = this.w.entities.get(inst.cargo.entityId);
          if (!e) { inst.cargo = null; continue; }
          const target = inst.cargo.local.clone().applyQuaternion(inst.quat).add(inst.pos);
          e.body.setNextKinematicTranslation(target);
        }
      }
    }
  }

  private driveKinematic(c: Construct, inst: Inst, dt: number) {
    const s = c.spec;
    const P = this.w.player;
    let target: Vector3 | null = null;
    let q: Quaternion | null = null;
    const look = P.look(new Vector3());

    if (inst.strike && inst.strike.phase !== 'done') {
      const st = inst.strike;
      const home = this.holdPose(c, inst, 'right');
      const tp = st.phase === 'go' ? this.refPos(st.target) ?? home.pos : home.pos;
      const d = tp.clone().sub(inst.pos);
      const dist = d.length();
      st.v = Math.min(st.speed, st.v + 220 * dt);
      const step = Math.min(dist, st.v * dt);
      if (dist > 1e-3) d.divideScalar(dist);
      target = inst.pos.clone().addScaledVector(d, step);
      q = quatFromDir(st.phase === 'go' ? d : look);
      if (st.phase === 'go') {
        // obstacle in the way? impact there
        const R = this.w.R;
        const wallHit = step > 0.01 ? this.w.pw.castRay(new R.Ray(inst.pos, d), step + c.radius * 0.5, true, undefined, groups(G.PLAYER, G.WORLD)) : null;
        const ents = this.strikeCandidates(st, inst, target, c);
        if (ents.length) {
          for (const e of ents) this.impactEntity(c, inst, e, d, st.v);
          this.finishStrikeLeg(c, inst, st);
        } else if (wallHit) {
          target = inst.pos.clone().addScaledVector(d, Math.max(0, wallHit.timeOfImpact - c.radius * 0.5));
          this.areaImpact(c, target, d, st.v);
          this.finishStrikeLeg(c, inst, st);
        } else if (dist <= step + 0.05) {
          // reached a point target
          this.areaImpact(c, tp, d, st.v);
          this.finishStrikeLeg(c, inst, st);
        }
      } else if (dist <= step + 0.05) {
        if (st.repeat > 0) { st.repeat--; st.phase = 'go'; st.v = 0; st.hitIds.clear(); }
        else st.phase = 'done';
      }
    } else if (inst.grab && !inst.grab.attached) {
      const g = inst.grab;
      const e = this.refEntities(g.target)[0];
      const tp = e ? this.w.entityPos(e) : this.refPos(g.target);
      if (!tp) { inst.grab = null; }
      else {
        const d = tp.clone().sub(inst.pos);
        const dist = d.length();
        g.v = Math.min(45, g.v + 120 * dt);
        const step = Math.min(dist, g.v * dt);
        if (dist > 1e-3) d.divideScalar(dist);
        target = inst.pos.clone().addScaledVector(d, step);
        q = quatFromDir(d);
        const reach = c.radius * 0.8 + (e?.radius ?? 0.5) + 0.3;
        if (dist - step <= reach) {
          if (e && !e.heldBy) this.attachEntity(c, inst, e, tp);
          g.attached = true;
          if (g.then === 'lift') inst.liftBase = target.y;
          this.w.emit('grabbed', { construct: c.id, entity: e?.id ?? null });
        }
      }
    } else {
      const orbit = has(s, 'orbit');
      const follow = has(s, 'follow');
      const hold = has(s, 'hold');
      const mount = has(s, 'mount');
      const home = has(s, 'home');
      const sweep = has(s, 'sweep');
      const lift = has(s, 'lift');
      if (orbit) {
        const center = this.refPos(orbit.around) ?? P.pos;
        inst.phase += orbit.speed * dt;
        const r = orbit.radius + (c.insts.length > 1 ? 0 : 0);
        target = center.clone().add(new Vector3(Math.cos(inst.phase) * r, (orbit.height ?? 0.3), Math.sin(inst.phase) * r));
        const tangent = new Vector3(-Math.sin(inst.phase), 0, Math.cos(inst.phase)).multiplyScalar(Math.sign(orbit.speed) || 1);
        q = quatFromDir(tangent);
      } else if (mount) {
        const side = mountSide(s);
        const hp = this.holdPose(c, inst, side);
        target = hp.pos; q = hp.quat;
      } else if (hold || (inst.grab?.attached && (inst.grab.then === 'hold' || inst.grab.then === 'bring'))) {
        const side = inst.grab?.then === 'bring' ? 'front' : hold?.side ?? (s.placement.at === 'hand' ? 'right' : 'front');
        const hp = this.holdPose(c, inst, side);
        target = inst.pos.clone().lerp(hp.pos, 1 - Math.exp(-18 * dt));
        q = hp.quat;
      } else if (follow) {
        const tp = this.refPos(follow.target) ?? P.pos;
        const off = new Vector3(...(follow.offset ?? [0, 2.5, -2])).applyQuaternion(new Quaternion().setFromAxisAngle(UP, Math.atan2(P.forward().x, P.forward().z)));
        target = inst.pos.clone().lerp(tp.clone().add(off).add(inst.offset), 1 - Math.exp(-6 * dt));
      } else if (home) {
        const tp = this.refPos(home.target);
        if (tp) {
          const d = tp.clone().sub(inst.pos);
          const dist = d.length();
          d.divideScalar(Math.max(1e-6, dist));
          const step = Math.min(dist, home.speed * dt);
          target = inst.pos.clone().addScaledVector(d, step);
          q = quatFromDir(d);
          const e = this.refEntities(home.target)[0];
          if (e && dist < c.radius + e.radius + 0.4) {
            this.impactEntity(c, inst, e, d, home.speed);
            if (has(s, 'explode')) { this.explode(c); return; }
            c.expires = Math.min(c.expires, this.w.time + 0.3);
          }
        }
      } else if (sweep && inst.sweepLeft > 0) {
        const dir = new Vector3(0, 0, 1).applyQuaternion(inst.quat).setY(0).normalize();
        const step = Math.min(inst.sweepLeft, sweep.speed * dt);
        inst.sweepLeft -= step;
        target = inst.pos.clone().addScaledVector(dir, step);
      }
      if (lift && inst.liftBase !== null) {
        const want = inst.liftBase + lift.height;
        const base = target ?? inst.pos.clone();
        base.y += Math.sign(want - base.y) * Math.min(Math.abs(want - base.y), 10 * dt);
        target = base;
      }
    }

    const spin = has(s, 'spin');
    if (spin) {
      const axis = new Vector3(...spin.axis).normalize();
      inst.spinQ.multiply(new Quaternion().setFromAxisAngle(axis, spin.rate * dt));
    }
    const pos = target ?? inst.pos;
    const baseQ = q ?? inst.quat.clone().multiply(inst.spinQ.clone().invert());
    const finalQ = spin ? baseQ.clone().multiply(inst.spinQ) : baseQ;
    inst.pos.copy(pos);
    inst.quat.copy(finalQ);
    inst.body.setNextKinematicTranslation(inst.pos);
    inst.body.setNextKinematicRotation(inst.quat);
  }

  private finishStrikeLeg(c: Construct, inst: Inst, st: StrikeState) {
    if (st.after === 'return') { st.phase = 'return'; st.v = Math.min(st.v, 20); }
    else if (st.after === 'dissolve') { st.phase = 'done'; c.expires = Math.min(c.expires, this.w.time + 0.25); }
    else {
      if (st.repeat > 0) { st.repeat--; st.phase = 'return'; return; }
      st.phase = 'done';
      // stays where it landed
      c.spec.behaviors = c.spec.behaviors.filter((b) => b.type !== 'hold');
    }
  }

  private strikeCandidates(st: StrikeState, inst: Inst, next: Vector3, c: Construct): Entity[] {
    const out: Entity[] = [];
    for (const e of this.w.entities.values()) {
      if (st.hitIds.has(e.id) || e.heldBy === c.id) continue;
      const ep = this.w.entityPos(e);
      if (segPointDist(inst.pos, next, ep) <= c.radius * 0.75 + e.radius + 0.15) { out.push(e); st.hitIds.add(e.id); }
    }
    return out;
  }

  /** Momentum-based hit: heavier and faster constructs hit harder. */
  impactEntity(c: Construct, inst: Inst, e: Entity, dir: Vector3, speed: number) {
    const me = e.body.mass();
    const mc = Math.max(20, c.mass);
    const J = (1.3 * mc * speed) / (1 + mc / me);
    let dv = J / me;
    dv = Math.min(dv, 70);
    const imp = dir.clone().setY(dir.y + 0.15).normalize().multiplyScalar(dv * me);
    if (e.heldBy || e.restrainedBy) return;
    e.body.applyImpulse(imp, true);
    if (e.type === 'drone') e.stunnedUntil = this.w.time + Math.min(3, 0.6 + dv * 0.05);
    this.w.damageEntity(e, Math.max(0, dv - 4) * 7, 'strike');
    c.impacts++;
    this.w.emit('impact', { construct: c.id, entity: e.id, dv: Math.round(dv), pos: this.w.entityPos(e).toArray() });
  }

  private areaImpact(c: Construct, at: Vector3, dir: Vector3, speed: number) {
    const r = Math.max(2.5, c.radius * 1.4);
    for (const e of this.w.entities.values()) {
      const ep = this.w.entityPos(e);
      const d = ep.distanceTo(at);
      if (d > r || e.heldBy) continue;
      const k = 1 - d / r;
      const outward = ep.clone().sub(at).normalize().add(dir.clone().multiplyScalar(0.5)).normalize();
      this.impactEntity(c, { ...(c.insts[0]) } as Inst, e, outward, speed * k);
    }
    this.w.emit('smash', { construct: c.id, pos: at.toArray(), radius: r });
  }

  private holdPose(c: Construct, _inst: Inst, side: string): { pos: Vector3; quat: Quaternion } {
    const P = this.w.player;
    const look = P.look(new Vector3());
    const right = P.right(new Vector3());
    const fwd = P.forward(new Vector3());
    const r = c.radius;
    const pos = P.pos.clone();
    let quat = quatFromDir(look);
    switch (side) {
      case 'left': pos.addScaledVector(right, -(0.9 + r * 0.6)).addScaledVector(look, 0.8 + r * 0.4).add(new Vector3(0, 0.3, 0)); break;
      case 'front': pos.addScaledVector(look, 1.6 + c.extent.size.z / 2 + r * 0.25); break;
      case 'overhead': pos.add(new Vector3(0, 1.2 - c.extent.min.y, 0)); quat = quatFromDir(fwd); break;
      case 'back': pos.addScaledVector(fwd, -0.45).add(new Vector3(0, 0.25, 0)); quat = quatFromDir(fwd); break;
      case 'feet': pos.add(new Vector3(0, -0.95 - c.extent.max.y, 0)); quat = quatFromDir(fwd); break;
      default: pos.addScaledVector(right, 0.9 + r * 0.6).addScaledVector(look, 0.8 + r * 0.4).add(new Vector3(0, 0.3, 0));
    }
    return { pos, quat };
  }

  private driveDynamic(c: Construct, inst: Inst, dt: number) {
    const s = c.spec;
    const thrust = has(s, 'thrust');
    if (thrust) {
      const axis = new Vector3(...thrust.axis).normalize().applyQuaternion(inst.quat);
      const m = inst.body.mass();
      inst.body.resetForces(true); // forces persist in Rapier; re-apply each step
      inst.body.addForce(axis.multiplyScalar(thrust.accel * m), true);
    }
    const home = has(s, 'home');
    if (home) {
      const tp = this.refPos(home.target);
      if (tp) {
        const v = inst.body.linvel();
        const want = tp.clone().sub(inst.pos).normalize().multiplyScalar(home.speed);
        const nv = new Vector3(v.x, v.y, v.z).lerp(want, 1 - Math.exp(-4 * dt));
        inst.body.setLinvel(nv, true);
      }
    }
    const spin = has(s, 'spin');
    if (spin) {
      const axis = new Vector3(...spin.axis).normalize().applyQuaternion(inst.quat).multiplyScalar(spin.rate);
      inst.body.setAngvel(axis, true);
    }
    // crush: swept hit on the target as it passes (guarantees the effect even if the target dodges slightly)
    const crush = has(s, 'crush');
    if (crush) {
      const v = inst.body.linvel();
      const vv = new Vector3(v.x, v.y, v.z);
      const next = inst.pos.clone().addScaledVector(vv, dt);
      for (const e of this.refEntities(crush.target)) {
        if (inst.strike?.hitIds.has(e.id)) continue;
        if (segPointDist(inst.pos, next, this.w.entityPos(e)) <= c.radius * 0.8 + e.radius) {
          this.impactEntity(c, inst, e, new Vector3(0, -1, 0), vv.length());
          inst.strike = { target: { kind: 'none' }, speed: 0, v: 0, phase: 'done', after: 'stay', repeat: 0, hitIds: new Set([e.id]), start: inst.pos.clone() };
        }
      }
    }
    if (has(s, 'explode')?.onContact && c.impacts > 0) this.explode(c);
  }

  private applyFields(c: Construct, inst: Inst, dt: number) {
    const f = has(c.spec, 'field');
    if (!f) return;
    const center = inst.pos;
    const axis = new Vector3(...(f.axis ?? [0, 0, 1])).normalize().applyQuaternion(inst.quat);
    const accelAt = (p: Vector3): Vector3 | null => {
      const d = p.clone().sub(center);
      const dist = d.length();
      if (dist > f.radius || dist < 0.3) return null;
      const k = 1 - dist / f.radius;
      d.divideScalar(dist);
      switch (f.mode) {
        case 'attract': return d.multiplyScalar(-f.strength * k);
        case 'repel': return d.multiplyScalar(f.strength * k);
        case 'wind': {
          // wind is a cone in front of the construct
          if (d.dot(axis) < 0.3) return null;
          return axis.clone().multiplyScalar(f.strength * k);
        }
        case 'lift': return new Vector3(0, f.strength * k + 5, 0);
      }
    };
    for (const e of this.w.entities.values()) {
      if (e.heldBy || e.restrainedBy || e.type === 'turret') continue;
      const a = accelAt(this.w.entityPos(e));
      if (!a) continue;
      e.body.applyImpulse(a.multiplyScalar(e.body.mass() * dt), true);
      if (e.type === 'drone') e.stunnedUntil = Math.max(e.stunnedUntil, this.w.time + 0.15);
    }
    for (const p of this.w.projectiles.values()) {
      const t = p.body.translation();
      const a = accelAt(new Vector3(t.x, t.y, t.z));
      if (a) p.body.applyImpulse(a.multiplyScalar(p.body.mass() * dt * 2.5), true);
    }
    const a = accelAt(this.w.player.pos);
    if (a && !has(c.spec, 'mount')) this.w.player.vel.addScaledVector(a, dt);
  }

  private applyShoot(c: Construct, inst: Inst, dt: number) {
    const sh = has(c.spec, 'shoot');
    if (!sh) return;
    inst.shootCd -= dt;
    if (inst.shootCd > 0) return;
    inst.shootCd = 1 / sh.rate;
    const ents = this.refEntities(sh.target);
    const tp = ents[0] ? this.w.entityPos(ents[0]) : this.refPos(sh.target) ?? this.w.player.aim().point;
    const d = tp.clone().sub(inst.pos).normalize();
    const start = inst.pos.clone().addScaledVector(d, c.radius + sh.size + 0.3);
    const pr = this.w.spawnProjectile(start, d.multiplyScalar(sh.speed), null, 10, sh.size);
    pr.reflected = true; // friendly
    this.w.emit('construct_fire', { construct: c.id });
  }

  private applyCushion(c: Construct, inst: Inst, dt: number) {
    const top = inst.pos.y + c.extent.max.y;
    for (const e of this.w.entities.values()) {
      if (e.heldBy) continue;
      const p = this.w.entityPos(e);
      const v = this.w.entityVel(e);
      const horiz = Math.hypot(p.x - inst.pos.x, p.z - inst.pos.z);
      const h = p.y - top;
      if (horiz > c.radius * 1.3 + 1 || h < -1 || h > 14 || v.y > -2) continue;
      // decelerate to a gentle landing over the remaining height
      const need = (v.y * v.y) / (2 * Math.max(0.6, h)) + 22;
      const a = Math.min(need, 140);
      e.body.applyImpulse({ x: -v.x * 0.05 * e.body.mass(), y: a * e.body.mass() * dt, z: -v.z * 0.05 * e.body.mass() }, true);
    }
  }

  private applyConveyorToEntities(c: Construct, inst: Inst) {
    const cv = has(c.spec, 'conveyor')!;
    for (const col of inst.colliders) {
      this.w.pw.contactPairsWith(col, (other) => {
        const o = this.w.owners.get(other.handle);
        if (o?.kind !== 'entity') return;
        const e = this.w.entities.get(o.id);
        if (!e) return;
        const sv = this.surfaceVelocityOf(c, inst, cv, this.w.entityPos(e));
        const v = this.w.entityVel(e);
        e.body.setLinvel({ x: v.x + (sv.x - v.x) * 0.3, y: v.y, z: v.z + (sv.z - v.z) * 0.3 }, true);
      });
    }
  }

  private surfaceVelocityOf(c: Construct, inst: Inst, cv: { speed: number; axis: [number, number, number] }, at: Vector3): Vector3 {
    if (c.spec.parts.some((p) => p.shape === 'torus')) {
      // loop belt: tangential around the construct centre
      const r = at.clone().sub(inst.pos);
      const n = new Vector3(0, 1, 0).applyQuaternion(inst.quat);
      return new Vector3().crossVectors(n, r).normalize().multiplyScalar(cv.speed);
    }
    return new Vector3(...cv.axis).normalize().applyQuaternion(inst.quat).multiplyScalar(cv.speed);
  }

  surfaceVelocity(col: RCollider): Vector3 | null {
    const id = this.byCollider.get(col.handle);
    if (!id) return null;
    const c = this.list.get(id);
    const cv = c && has(c.spec, 'conveyor');
    if (!c || !cv) return null;
    const inst = c.insts.find((i) => i.colliders.includes(col)) ?? c.insts[0];
    return this.surfaceVelocityOf(c, inst, cv, this.w.player.pos);
  }

  private applyMount(c: Construct, m: Extract<Behavior, { type: 'mount' }>, P: Player) {
    const mods = P.mods;
    mods.sources.push(c.spec.label);
    if (m.glide) mods.glide = Math.max(mods.glide, m.glide);
    if (m.groundSpeedMul) mods.groundSpeedMul *= m.groundSpeedMul;
    if (m.frictionMul !== undefined) mods.frictionMul *= m.frictionMul;
    if (m.maxFall) mods.maxFallSpeed = Math.min(mods.maxFallSpeed, m.maxFall);
    if (m.thrustLook) mods.thrustAlongLook += m.thrustLook;
    const thrust = has(c.spec, 'thrust');
    if (thrust) mods.thrustAlongLook += thrust.accel;
    if (m.thrustUp && this.w.input?.jump && !P.onGround) mods.thrust.y += m.thrustUp;
    if (m.bounce && P.onGround && P.vel.y <= 0.1 && !P.flying) { P.vel.y = m.bounce; P.onGround = false; }
  }

  private explode(c: Construct) {
    const ex = has(c.spec, 'explode');
    const at = c.insts[0]?.pos.clone() ?? new Vector3();
    if (ex) {
      for (const e of this.w.entities.values()) {
        const ep = this.w.entityPos(e);
        const d = ep.distanceTo(at);
        if (d > ex.radius || e.type === 'turret' && d > ex.radius * 0.5) continue;
        const k = 1 - d / ex.radius;
        if (e.heldBy && e.heldBy !== c.id) continue;
        if (e.heldBy === c.id) this.releaseEntity(e.id, new Vector3());
        const dir = ep.clone().sub(at).normalize().add(new Vector3(0, 0.4, 0)).normalize();
        // velocity change falls off gently with mass so blasts still budge boulders
        const dv = ex.force * k * Math.pow(40 / Math.max(40, e.body.mass()), 0.25);
        e.body.applyImpulse(dir.multiplyScalar(dv * e.body.mass()), true);
        this.w.damageEntity(e, ex.force * k * 2.2, 'explosion');
        if (e.type === 'drone') e.stunnedUntil = this.w.time + 2;
      }
      // rocket-jump: blasts push the player too (movement tech, not damage)
      const P = this.w.player;
      const d = P.pos.distanceTo(at);
      if (d < ex.radius) P.vel.addScaledVector(P.pos.clone().sub(at).normalize().add(new Vector3(0, 0.3, 0)).normalize(), ex.force * 0.6 * (1 - d / ex.radius));
      this.w.emit('explosion', { construct: c.id, pos: at.toArray(), radius: ex.radius });
    }
    this.dismiss(c.id, 'exploded');
  }

  // ------------------------------------------------------------------ cargo

  private attachEntity(c: Construct, inst: Inst, e: Entity, at: Vector3) {
    const R = this.w.R;
    if (e.type === 'turret') return;
    e.heldBy = c.id;
    e.body.setBodyType(R.RigidBodyType.KinematicPositionBased, true);
    e.collider.setCollisionGroups(groups(G.CARGO, ALL & ~(G.CONSTRUCT | G.GHOST)));
    // grip point: just in front of the construct, or keep relative position if inside it
    const local = at.clone().sub(inst.pos).applyQuaternion(inst.quat.clone().invert());
    if (local.length() > c.radius * 1.2) local.setLength(c.radius * 0.9 + e.radius);
    inst.cargo = { entityId: e.id, local };
  }

  releaseEntity(id: string, vel: Vector3) {
    const e = this.w.entities.get(id);
    if (!e) return;
    const R = this.w.R;
    e.heldBy = null;
    e.restrainedBy = null;
    e.body.setBodyType(R.RigidBodyType.Dynamic, true);
    e.collider.setCollisionGroups(groups(G.ENTITY, ALL));
    e.body.setLinvel(vel, true);
    if (e.type === 'drone') e.stunnedUntil = this.w.time + 1.5;
    for (const c of this.list.values()) for (const i of c.insts) if (i.cargo?.entityId === id) i.cargo = null;
  }

  private releaseRestrained(c: Construct) {
    let any = false;
    for (const e of this.w.entities.values()) if (e.restrainedBy === c.id) { this.releaseEntity(e.id, new Vector3()); any = true; }
    return any;
  }

  private entityPosById(id: string) { const e = this.w.entities.get(id); return e ? this.w.entityPos(e) : null; }

  cargoOf(id: string): string[] {
    const c = this.list.get(id);
    return c ? c.insts.map((i) => i.cargo?.entityId).filter((x): x is string => !!x) : [];
  }

  // ------------------------------------------------------------------ contacts

  onPlayerTouch(collider: RCollider, n: Vector3, P: Player): 'clip' | 'ignore' | 'handled' {
    const id = this.byCollider.get(collider.handle);
    if (!id) return 'clip';
    const c = this.list.get(id);
    if (!c) return 'clip';
    // climbable constructs (ladders, nets, rungs): pushing into them climbs
    if (c.spec.parts.some((p) => p.role === 'rung' || p.role === 'climb') && n.y < 0.5 && P.wishDir.lengthSq() > 0.1 && P.wishDir.dot(n) < -0.3) {
      P.vel.y = Math.max(P.vel.y, 7);
      P.vel.addScaledVector(n, -1.5);
      return 'clip';
    }
    const launch = has(c.spec, 'launch');
    if (launch) {
      const inst = c.insts.find((i) => i.colliders.includes(collider)) ?? c.insts[0];
      const up = new Vector3(0, 1, 0).applyQuaternion(inst.quat);
      let dir: Vector3;
      switch (launch.dir) {
        case 'up': dir = new Vector3(0, 1, 0); break;
        case 'forward': dir = new Vector3(0, 0, 1).applyQuaternion(inst.quat).setY(0).normalize().add(new Vector3(0, 0.35, 0)).normalize(); break;
        case 'arc': dir = new Vector3(0, 0, 1).applyQuaternion(inst.quat).setY(0).normalize().add(new Vector3(0, 1, 0)).normalize(); break;
        default: dir = n.y > 0.3 ? n.clone() : up;
      }
      if (launch.target) {
        const tp = this.refPos(launch.target);
        if (tp) { this.launchPlayerTo(tp, launch.speed, launch.clearY); this.w.emit('launch', { construct: id, speed: Math.round(P.vel.length()) }); return 'handled'; }
      }
      const into = -P.vel.dot(dir);
      // trampolines give back more when you come in fast (skill: height stacking), pads add a fixed kick
      const out = launch.dir === 'normal' ? Math.max(launch.speed, into * 1.02) : launch.speed + Math.max(0, into) * 0.4;
      if (into > 0) P.vel.addScaledVector(dir, into);
      P.vel.addScaledVector(dir, out);
      P.onGround = false;
      P.stats.lastLaunchSpeed = P.vel.length();
      this.w.emit('launch', { construct: id, speed: Math.round(P.vel.length()) });
      return 'handled';
    }
    return 'clip';
  }

  /** Ballistic launch of the player toward a point (used by launchers aimed at a destination). */
  launchPlayerTo(tpIn: Vector3, speedHint: number, clearY?: number) {
    const P = this.w.player;
    const g = -this.w.pw.gravity.y;
    // aim a few metres past the point so we land ON the ledge/roof instead of clipping its edge
    const flat = tpIn.clone().sub(P.pos).setY(0);
    const tp = tpIn.clone().add(flat.lengthSq() > 1 ? flat.normalize().multiplyScalar(3) : new Vector3()).add(new Vector3(0, 1, 0));
    const d = tp.clone().sub(P.pos);
    const horiz = Math.hypot(d.x, d.z);
    // choose a flight time that clears the height difference with some apex margin
    // clear the tallest static obstacle along the way (walls, buildings) with a few metres to spare
    let obstacle = -Infinity;
    const samples = Math.min(120, Math.max(10, Math.ceil(P.pos.distanceTo(tp) / 0.7)));
    for (let i = 1; i < samples; i++) {
      const q = P.pos.clone().lerp(tp, i / samples);
      const hit = this.w.pw.castRay(new this.w.R.Ray({ x: q.x, y: q.y + 80, z: q.z }, { x: 0, y: -1, z: 0 }), 200, true, undefined, groups(G.PLAYER, G.WORLD));
      if (hit) obstacle = Math.max(obstacle, q.y + 80 - hit.timeOfImpact);
    }
    const apex = Math.max(Math.max(d.y, 0) + 4 + horiz * 0.15, clearY !== undefined ? clearY - P.pos.y : 0, isFinite(obstacle) ? obstacle + 3 - P.pos.y : 0);
    const vy = Math.sqrt(2 * g * apex);
    const tUp = vy / g;
    const tDown = Math.sqrt(Math.max(0, 2 * (apex - d.y) / g));
    const t = tUp + tDown;
    const v = new Vector3(d.x / t, vy, d.z / t);
    const lim = Math.max(speedHint, LIMITS.launchSpeed * 1.4);
    if (v.length() > lim) v.multiplyScalar(lim / v.length());
    P.vel.copy(v);
    P.onGround = false;
    P.flying = false;
    P.stats.lastLaunchSpeed = v.length();
  }

  /** Launchers also fling entities resting on them (spring under a drone/boulder/civilian). */
  private applyLaunchToEntities(c: Construct, inst: Inst) {
    const launch = has(c.spec, 'launch')!;
    const top = inst.pos.y + c.extent.max.y;
    const st = (inst as any).launchCd ?? ((inst as any).launchCd = new Map<string, number>());
    for (const e of this.w.entities.values()) {
      if (e.heldBy || e.restrainedBy || e.type === 'turret') continue;
      const p = this.w.entityPos(e);
      const horiz = Math.hypot(p.x - inst.pos.x, p.z - inst.pos.z);
      const h = p.y - e.radius - top;
      if (horiz > Math.max(c.extent.size.x, c.extent.size.z) / 2 + 0.3 || h < -0.8 || h > 1.2) continue;
      if ((st.get(e.id) ?? -1) > this.w.time) continue;
      st.set(e.id, this.w.time + 0.6);
      const up = new Vector3(0, 1, 0).applyQuaternion(inst.quat);
      const v = up.multiplyScalar(launch.speed);
      if (launch.target) { const tp = this.refPos(launch.target); if (tp) v.copy(ballistic(p, tp, launch.speed, this.w.pw.gravity.y)); }
      e.body.setLinvel(v, true);
      if (e.type === 'drone') e.stunnedUntil = this.w.time + 2;
      this.w.emit('launch_entity', { construct: c.id, entity: e.id });
    }
  }

  onProjectile(id: string, p: Projectile): 'absorb' | 'reflect' {
    const c = this.list.get(id);
    if (!c) return 'absorb';
    if (has(c.spec, 'reflect') || has(c.spec, 'conveyor') && c.spec.parts.some((x) => x.shape === 'torus')) {
      const src = p.from ? this.w.entities.get(p.from) : null;
      const t = p.body.translation();
      const pos = new Vector3(t.x, t.y, t.z);
      const v = p.body.linvel();
      const speed = Math.max(30, Math.hypot(v.x, v.y, v.z) * 1.25);
      const dir = src ? this.w.entityPos(src).add(new Vector3(0, 1.2, 0)).sub(pos).normalize() : new Vector3(-v.x, -v.y, -v.z).normalize();
      p.body.setLinvel(dir.multiplyScalar(speed), true);
      p.born = this.w.time;
      return 'reflect';
    }
    return 'absorb';
  }

  isSoft(id: string) { const c = this.list.get(id); return !!c && (c.spec.soft || !!has(c.spec, 'cushion')); }

  noteImpact(id: string, _entityId: string, _dv: number) { const c = this.list.get(id); if (c) c.impacts++; }

  private enforceBudget() {
    let total = 0;
    for (const c of this.list.values()) total += c.insts.length;
    const order = [...this.list.values()].sort((a, b) => a.born - b.born);
    while (total > 48 && order.length > 1) {
      const c = order.shift()!;
      total -= c.insts.length;
      this.dismiss(c.id, 'budget');
    }
    if (this.list.size > 24) {
      const c = [...this.list.values()].sort((a, b) => a.born - b.born)[0];
      this.dismiss(c.id, 'budget');
    }
  }
}

function quatFromDir(dir: Vector3): Quaternion {
  const d = dir.clone();
  if (d.lengthSq() < 1e-8) return new Quaternion();
  d.normalize();
  const m = new Matrix4();
  const up = Math.abs(d.y) > 0.98 ? new Vector3(0, 0, 1) : UP;
  const x = new Vector3().crossVectors(up, d).normalize();
  const y = new Vector3().crossVectors(d, x).normalize();
  m.makeBasis(x, y, d);
  return new Quaternion().setFromRotationMatrix(m);
}

function mountSide(s: ConstructSpec): string {
  const hold = has(s, 'hold');
  if (hold?.side) return hold.side;
  const m = has(s, 'mount')!;
  if (m.groundSpeedMul || m.bounce) return 'feet';
  if (m.maxFall && !m.glide) return 'overhead';
  return 'back';
}

function segPointDist(a: Vector3, b: Vector3, p: Vector3) {
  const ab = b.clone().sub(a);
  const t = ab.lengthSq() > 1e-9 ? Math.min(1, Math.max(0, p.clone().sub(a).dot(ab) / ab.lengthSq())) : 0;
  return a.clone().addScaledVector(ab, t).distanceTo(p);
}

/** Launch velocity that roughly hits `to` from `from` at the given speed (flat-ish arc). */
export function ballistic(from: Vector3, to: Vector3, speed: number, gy: number): Vector3 {
  const d = to.clone().sub(from);
  const dist = d.length();
  const t = dist / Math.max(1, speed);
  const v = d.divideScalar(Math.max(1e-3, t));
  v.y += -0.5 * gy * t; // gy negative
  return v;
}

export { quatFromDir, cloneSpec };
