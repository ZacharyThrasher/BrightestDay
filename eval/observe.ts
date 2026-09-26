/**
 * Observers for the machine functions of eval/EVAL_SPEC_MACHINES.md.
 *
 * Everything here is judged from WORLD STATE OVER TIME (construct instance poses, articulated group poses,
 * entity positions/health, the player's position and seat) and from world EVENTS (construct_fire,
 * machine_beam, entity_damage, machine_damage, entity_disabled, entity_destroyed, inspiration,
 * creative_solution) — never from the interpreter's claimed functions. The only structural facts read from a
 * construct are its part layout (part groups, part transforms, joint pivots) to know where a group sits
 * (legs/wheels reach the bottom quarter; "pointing" groups extend forward of their pivot).
 *
 * Attribution: effects count only for the constructs the scored utterance created, modified, ordered, acted
 * with or rode ("attributed" set, built by the runner from the commit's ops + construct_formed / _modified /
 * machine_order / ride events). Context constructs the scored utterance did not touch are ignored.
 *
 * The runner drives the world through up to three segments:
 *   main   — the scored window (default 8 s), no player input (legacy: the player walks forward in `gap`)
 *   follow — scripted, only when the item expects follows_player: the player dismounts, waits 2 s
 *            (unobserved), walks ~24 m, stands 1.5 s
 *   ride   — scripted, only when the item expects rideable / carries_player: the harness seats the player
 *            ("get in"), holds forward 5 s and, for the second half, points the crosshair at the nearest
 *            target and holds fire (player-operated weapons)
 * The phases are probes: each contributes only the functions listed in SEGMENT_FNS. Event-based functions
 * (damages / destroys / attacks / creative / stale) count from the commit to the end of the main window.
 *
 * Set OBS_DEBUG=1 to print per-group / follow / guard internals to stderr.
 */
import { Vector3, Quaternion } from 'three';
import type { World, GameEvent } from '../src/sim/world.js';
import type { Construct, Inst } from '../src/sim/constructs/runtime.js';
import type { PlayerInput } from '../src/sim/player.js';
import { partTransform } from '../src/sim/constructs/geom.js';
import { G, groups } from '../src/sim/physics.js';
import { INSPIRED_AT } from '../src/sim/inspiration.js';

export const MACHINE_FNS = [
  'moves_autonomously', 'drives_on_ground', 'flies', 'walks', 'wheels_spin', 'has_moving_parts', 'aims_at_target', 'shoots',
  'damages_target', 'destroys_target', 'attacks_enemies', 'follows_player', 'guards_area', 'fetches', 'rideable', 'carries_player',
  'pushes_objects', 'returns', 'multiple_agents', 'creative_bonus', 'stale_penalty',
] as const;
export const MACHINE_FN_SET = new Set<string>(MACHINE_FNS);

/** Thresholds. Values quoted from EVAL_SPEC_MACHINES.md are marked (spec); the rest are harness choices. */
export const TH = {
  travel: 6, // m (spec): moves_autonomously / drives_on_ground / flies
  groundH: 1.5, // m (spec): drives_on_ground — construct bottom within this of the ground…
  groundFrac: 0.9, // …for this fraction of the segment's samples ("staying")
  flyH: 3, // m (spec): flies — bottom at least this high above the ground…
  flyFrac: 0.5, // …for "most of the window"
  partDeg: 5, partPos: 0.1, // has_moving_parts: a group's pose moves ≥ 5° or ≥ 10 cm relative to the body
  legSwingMin: 8, legSwingMax: 110, legReversals: 2, // walks: ≥ 2 lower groups swinging back and forth (not full turns)
  wheelMinTravel: 3, wheelMinTurn: Math.PI, wheelNet: 0.6, wheelMoving: 0.8, wheelRatio: 3, // wheels_spin
  aimErr: 20, aimHold: 0.3, aimGroupMoved: 2, aimBodyYaw: 10, aimFlatElev: 35, aimMaxSwingRate: 1, aimGain: 10, // aims_at_target (20° is spec)
  followDist: 12, followPlayerMove: 15, followFrac: 0.75, followSelfMove: 8, // follows_player (12 m / 15 m spec)
  guardRadius: 8, guardNear: 25, guardHold: 2, // guards_area (8 m spec; 'near' = within a guard's engagement range)
  fetchDist: 4, fetchCarryGrace: 2, // fetches (4 m spec)
  rideMove: 4, // rideable: the construct moves ≥ 4 m under the rider's input
  carry: 8, // carries_player (spec)
  push: 2, // pushes_objects (spec)
  returnNear: 4, returnFar: 8, // returns (4 m spec): surface distance, after having been ≥ 8 m away
  agentMove: 3, agentSep: 1, // multiple_agents: ≥ 2 instances each moving ≥ 3 m, not rigidly linked
  attachedPlayerMove: 3, attachedDev: 1, // "attached to the player": keeps a fixed offset while the player moves
  sampleTicks: 15, // position sampling period (0.25 s)
};

export type Segment = 'main' | 'follow' | 'ride';

/** What each segment may contribute. Event-based functions (shoots, damages_target, destroys_target, attacks_enemies,
 * creative_bonus, stale_penalty) are measured from the commit to the end of the main window. */
export const SEGMENT_FNS: Record<Segment, string[]> = {
  main: ['moves_autonomously', 'drives_on_ground', 'flies', 'walks', 'wheels_spin', 'has_moving_parts', 'aims_at_target', 'shoots', 'guards_area', 'fetches', 'returns', 'multiple_agents', 'carries_player', 'rideable', 'pushes_objects'],
  // the scripted phases are probes for their own functions (plus the construct's parts while the harness drives it)
  follow: ['follows_player'],
  // (in the ride phase the harness also pulls the trigger: player-operated guns can show shoots / aims_at_target)
  ride: ['rideable', 'carries_player', 'wheels_spin', 'has_moving_parts', 'shoots', 'aims_at_target'],
};

const LOOSE = new Set(['crate', 'block', 'boulder']);
const D2R = Math.PI / 180;
const Z = new Vector3(0, 0, 1);

function qAngle(a: Quaternion, b: Quaternion) { return 2 * Math.acos(Math.min(1, Math.abs(a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w))); }
function hdist(a: Vector3, b: Vector3) { return Math.hypot(a.x - b.x, a.z - b.z); }
/** angle (deg) between a forward axis and the direction to a target; for low targets the horizontal angle is enough (yaw-only turrets) */
function aimError(fwd: Vector3, to: Vector3): number {
  let err = fwd.angleTo(to) / D2R;
  const elev = Math.asin(Math.min(1, Math.abs(to.y) / to.length())) / D2R;
  if (elev <= TH.aimFlatElev) {
    const a = fwd.clone().setY(0), b = to.clone().setY(0);
    if (a.lengthSq() > 1e-4 && b.lengthSq() > 1e-4) err = Math.min(err, a.angleTo(b) / D2R);
  }
  return err;
}
/** signed rotation angle of q about `ref` (sets ref from q's own axis the first time it is significant) */
function signedAngle(q: Quaternion, ref: { v: Vector3 | null }, minRad = 0.05): number {
  let { x, y, z, w } = q;
  if (w < 0) { x = -x; y = -y; z = -z; w = -w; }
  const len = Math.hypot(x, y, z);
  if (len < 1e-7) return 0;
  const a = 2 * Math.atan2(len, w);
  const ax = new Vector3(x / len, y / len, z / len);
  if (!ref.v) { if (a < minRad) return 0; ref.v = ax; }
  const d = ax.dot(ref.v);
  return Math.abs(d) < 0.3 ? 0 : a * Math.sign(d);
}

/** pointing: the group's parts sit ahead of (or around) its pivot rather than below/behind/beside it (turrets, barrels, heads — not legs, tails, wings) */
interface GroupInfo { centroid: Vector3; lower: boolean; rEst: number; pointing: boolean }
interface GroupTrack {
  info: GroupInfo;
  q0: Quaternion; p0: Vector3; prevQ: Quaternion;
  maxDevAng: number; maxDevPos: number;
  total: number; net: number; movingRot: number; netRef: { v: Vector3 | null };
  /** the same while the construct is on the ground */
  gTotal: number; gNet: number; gMoving: number;
  swingRef: { v: Vector3 | null }; swingExt: number; swingDir: number; reversals: number; allReversals: number; maxSwing: number;
  aimRun: number; aimHeld: boolean; minErr: number;
}
interface Sample { t: number; pos: Vector3; h: number; ridden: boolean; player: Vector3 }
interface InstTrack {
  key: string; cid: string; k: number;
  samples: Sample[];
  groups: Map<string, GroupTrack>;
  lastPos: Vector3 | null; travelH: number; movingTicks: number;
  /** on the ground at the last position sample; travel while grounded (wheels roll on the ground) */
  grounded: boolean; travelGround: number;
  yaw0: number | null; yawDevMax: number; bodyAimRun: number; bodyAimHeld: boolean;
  rideMove: number; carry: number;
  far: boolean; returned: boolean;
}
interface DamageRec { t: number; id: string; by: string | null; kind: 'entity' | 'construct'; hostile: boolean; pos: Vector3 | null; cause: string; final: boolean; inWindow: boolean }

export interface ObserveResult { observed: Set<string>; evidence: Record<string, string> }

export class MachineObserver {
  readonly events: GameEvent[] = [];
  readonly attributed = new Set<string>();
  private cons = new Map<string, Construct>();
  private groupInfo = new Map<string, Map<string, GroupInfo>>();
  private seg: Segment | null = null;
  private segTracks = new Map<string, InstTrack>();
  private segStart = 0;
  private segTick = 0;
  private obs = new Set<string>();
  private ev: Record<string, string> = {};
  private damage: DamageRec[] = [];
  private seenProjectiles = new Set<number>();
  /** constructs that fired, per segment ('main' includes the commit itself) */
  private shotBy = new Map<string, Set<string>>();
  private loose = new Map<string, { p0: Vector3; maxDisp: number; credited: boolean }>();
  private carried = new Map<string, { lastHeld: number; farAtStart: boolean }>();
  private groundRay: any = null;
  private tStart = 0;
  /** event-based functions: open from the commit until the main window ends */
  private eventsOpen = true;
  private mainEnd = Infinity;
  private goneAtMainEnd = new Set<string>();
  private listener = (e: GameEvent) => this.onEvent(e);

  /** targets: ids of the target entity set (aimed entity, or all hostiles when nothing is aimed at) */
  constructor(private w: World, private opts: { targetIds: () => string[]; aimTargetIds: () => string[] }) {
    w.listeners.push(this.listener);
    for (const p of w.projectiles.keys()) this.seenProjectiles.add(p);
  }

  detach() { const i = this.w.listeners.indexOf(this.listener); if (i >= 0) this.w.listeners.splice(i, 1); }

  /** Call right after the scored commit. */
  attribute(ids: Iterable<string>) {
    for (const id of ids) this.attributed.add(id);
    for (const e of this.events) {
      if (['construct_formed', 'construct_modified', 'machine_order', 'ride'].includes(e.type) && typeof e.id === 'string') this.attributed.add(e.id);
    }
    this.tStart = this.w.time;
    for (const e of this.w.entities.values()) if (LOOSE.has(e.type)) this.loose.set(e.id, { p0: this.w.entityPos(e), maxDisp: 0, credited: false });
    const P = this.w.player.pos;
    for (const e of this.w.entities.values()) this.carried.set(e.id, { lastHeld: -Infinity, farAtStart: hdist(this.w.entityPos(e), P) > TH.fetchDist });
    this.refreshConstructs();
  }

  private shot(cid: string) {
    const seg = this.seg ?? (this.eventsOpen ? 'main' : null);
    if (!seg) return;
    const s = this.shotBy.get(seg) ?? new Set<string>();
    s.add(cid);
    this.shotBy.set(seg, s);
  }

  private isA(id: unknown): boolean { return typeof id === 'string' && this.attributed.has(id); }

  private onEvent(e: GameEvent) {
    this.events.push(e);
    switch (e.type) {
      case 'construct_fire': case 'machine_beam':
        // melee swings/bites also report construct_fire: only projectiles and beams/cones count as shooting
        if (this.isA(e.construct) && e.kind !== 'melee') this.shot(e.construct);
        break;
      case 'entity_damage': case 'entity_disabled': case 'entity_destroyed': {
        const ent = this.w.entities.get(e.id);
        this.damage.push({ t: e.t, id: e.id, by: e.by ?? null, kind: 'entity', hostile: !!ent?.hostile, pos: ent ? this.w.entityPos(ent) : Array.isArray(e.pos) ? new Vector3(e.pos[0], e.pos[1], e.pos[2]) : null, cause: e.cause ?? '', final: e.type !== 'entity_damage', inWindow: this.eventsOpen });
        break;
      }
      case 'machine_damage':
        if (e.target === 'construct') {
          const c = this.w.constructs.get(e.id);
          const i = c?.insts[0];
          this.damage.push({ t: e.t, id: e.id, by: e.by ?? null, kind: 'construct', hostile: true, pos: i ? i.pos.clone() : null, cause: e.cause ?? '', final: false, inWindow: this.eventsOpen });
        }
        break;
    }
  }

  private refreshConstructs() {
    for (const id of this.attributed) {
      const c = this.w.constructs.get(id);
      if (c) {
        if (this.cons.get(id) !== c || !this.groupInfo.has(id + '@' + c.version)) this.groupInfo.set(id + '@' + c.version, this.computeGroups(c));
        this.cons.set(id, c);
      }
    }
  }

  /** Structural info about each articulated group (from the construct's parts, in its scaled frame). */
  private computeGroups(c: Construct): Map<string, GroupInfo> {
    const out = new Map<string, GroupInfo>();
    const acc = new Map<string, { sum: Vector3; n: number; r: number; minY: number }>();
    const parentOf = new Map((c.spec.joints ?? []).map((j) => [j.group, j.parent ?? null]));
    for (const p of c.spec.parts) {
      if (!p.group) continue;
      const t = partTransform(p, c.spec.scale, c.spec.stretch);
      const rotated = !!(p.rot && (p.rot[0] || p.rot[1] || p.rot[2]));
      const bottom = t.pos.y - (rotated ? Math.max(t.size.x, t.size.y, t.size.z) / 2 : t.size.y / 2);
      const a = acc.get(p.group) ?? { sum: new Vector3(), n: 0, r: 0, minY: Infinity };
      a.sum.add(t.pos); a.n++; a.r = Math.max(a.r, Math.max(t.size.x, t.size.y, t.size.z) / 2);
      acc.set(p.group, a);
      // a group reaches as low as its lowest part, including the parts of groups hanging off it (thigh → shin → foot)
      let g: string | null = p.group, guard = 0;
      while (g && guard++ < 32) { const x = acc.get(g) ?? { sum: new Vector3(), n: 0, r: 0, minY: Infinity }; x.minY = Math.min(x.minY, bottom); acc.set(g, x); g = parentOf.get(g) ?? null; }
    }
    // "lower" groups (legs, wheels) reach the bottom quarter of the construct
    const lowLine = c.extent.min.y + 0.25 * c.extent.size.y;
    const st = c.spec.stretch ?? [1, 1, 1];
    const pivots = new Map((c.spec.joints ?? []).map((j) => [j.group, new Vector3(j.pivot[0] * st[0], j.pivot[1] * st[1], j.pivot[2] * st[2]).multiplyScalar(c.spec.scale)]));
    for (const [g, a] of acc) {
      const centroid = a.n ? a.sum.divideScalar(a.n) : new Vector3();
      const d = centroid.clone().sub(pivots.get(g) ?? centroid);
      const slack = 0.1 * Math.max(0.5, a.r);
      const pointing = d.z >= -slack && d.z >= Math.max(Math.abs(d.x), -d.y) - slack;
      out.set(g, { centroid, lower: a.minY <= lowLine, rEst: Math.max(0.05, a.r), pointing });
    }
    return out;
  }

  private groundHeight(c: Construct, inst: Inst): number {
    const R = this.w.R;
    if (!this.groundRay) this.groundRay = new R.Ray({ x: 0, y: 0, z: 0 }, { x: 0, y: -1, z: 0 });
    const r = this.groundRay;
    const bottom = inst.pos.y + c.extent.min.y;
    r.origin.x = inst.pos.x; r.origin.y = bottom + 1; r.origin.z = inst.pos.z;
    // static world geometry only (ground, rock, buildings)
    const hit = this.w.pw.castRay(r, 600, true, undefined, groups(0xffff, G.WORLD));
    if (!hit) return Infinity;
    return bottom - (r.origin.y - hit.timeOfImpact);
  }

  // ------------------------------------------------------------------ segments

  begin(seg: Segment) {
    this.seg = seg;
    this.segTracks = new Map();
    this.segStart = this.w.time;
    this.segTick = 0;
    this.refreshConstructs();
  }

  /** Call after every world step. */
  tick(input: PlayerInput) {
    if (!this.seg) return;
    const w = this.w;
    const dt = w.dt;
    const P = w.player;
    const sample = this.segTick % TH.sampleTicks === 0;
    this.segTick++;
    if (this.segTick % 30 === 1) this.refreshConstructs();
    // projectiles fired by attributed constructs (any weapon path: machine weapons, old 'shoot', throws)
    for (const [pid, p] of w.projectiles) {
      if (this.seenProjectiles.has(pid)) continue;
      this.seenProjectiles.add(pid);
      const by = p.owner ?? p.by;
      if (this.isA(by) && p.from === null && !p.reflected) this.shot(by!);
    }
    // loose objects moved by attributed constructs (main window)
    for (const [id, l] of this.seg === 'main' ? this.loose : []) {
      const e = w.entities.get(id);
      if (!e) continue;
      const p = w.entityPos(e);
      l.maxDisp = Math.max(l.maxDisp, p.distanceTo(l.p0));
      const who = w.creditOf(e);
      if ((this.isA(who) || this.isA(e.heldBy) || this.isA(e.restrainedBy)) && w.entityVel(e).length() > 0.3) l.credited = true;
    }
    // things carried by attributed constructs, and whether they reached the player
    if (this.seg === 'main') {
      for (const e of w.entities.values()) {
        const cr = this.carried.get(e.id);
        if (!cr) continue;
        if (this.isA(e.heldBy)) cr.lastHeld = w.time;
        if (cr.farAtStart && w.time - cr.lastHeld <= TH.fetchCarryGrace) {
          const p = w.entityPos(e);
          if (hdist(p, P.pos) <= TH.fetchDist && Math.abs(p.y - P.pos.y) <= 3) this.add('fetches', `${e.id} carried by ${e.heldBy ?? 'construct'} to ${hdist(p, P.pos).toFixed(1)} m of the player`);
        }
      }
    }
    // the player's seat: riding an attributed construct, or standing on one
    let onId: string | null = null, onInst: Inst | null = null;
    if (P.riding && this.isA(P.riding.id)) { onId = P.riding.id; onInst = this.cons.get(onId)?.insts[0] ?? null; }
    else if (P.onGround && P.groundCollider) {
      const o = w.ownerOf(P.groundCollider);
      if (o?.kind === 'construct' && this.isA(o.id)) {
        onId = o.id;
        const parent = P.groundCollider.parent();
        onInst = this.cons.get(o.id)?.insts.find((i) => i.body.handle === parent?.handle) ?? null;
      }
    }
    const inputting = !!(input.moveX || input.moveZ || input.jump);
    const aimTargets: Vector3[] = [];
    for (const id of this.opts.aimTargetIds()) { const e = w.entities.get(id); if (e && !e.disabled) aimTargets.push(w.entityPos(e)); }

    for (const [cid, c] of this.cons) {
      if (!w.constructs.list.has(cid)) continue;
      const ginfo = this.groupInfo.get(cid + '@' + c.version) ?? this.computeGroups(c);
      c.insts.forEach((inst, k) => {
        if (!inst.alive) return;
        const key = `${cid}#${k}`;
        let tr = this.segTracks.get(key);
        if (!tr) {
          tr = { key, cid, k, samples: [], groups: new Map(), lastPos: null, travelH: 0, movingTicks: 0, grounded: true, travelGround: 0, yaw0: null, yawDevMax: 0, bodyAimRun: 0, bodyAimHeld: false, rideMove: 0, carry: 0, far: false, returned: false };
          this.segTracks.set(key, tr);
        }
        const ridden = !!P.riding && P.riding.id === cid && k === 0;
        const step = tr.lastPos ? inst.pos.distanceTo(tr.lastPos) : 0;
        const stepH = tr.lastPos ? hdist(inst.pos, tr.lastPos) : 0;
        const moving = stepH / dt > 0.5;
        tr.travelH += stepH;
        if (tr.grounded) tr.travelGround += stepH;
        if (moving) tr.movingTicks++;
        if (onId === cid && (onInst === inst || (!onInst && k === 0))) {
          // transported = horizontal + upward travel (sinking/falling through the floor is not transport)
          const lift = tr.lastPos ? Math.max(0, inst.pos.y - tr.lastPos.y) : 0;
          tr.carry += stepH + lift;
          if (ridden && inputting) tr.rideMove += stepH + lift;
        }
        tr.lastPos = (tr.lastPos ?? new Vector3()).copy(inst.pos);
        // body heading / body aiming
        const fwd = Z.clone().applyQuaternion(inst.quat);
        const yaw = Math.atan2(fwd.x, fwd.z);
        if (tr.yaw0 === null) tr.yaw0 = yaw;
        let dy = Math.abs(yaw - tr.yaw0) % (2 * Math.PI); if (dy > Math.PI) dy = 2 * Math.PI - dy;
        tr.yawDevMax = Math.max(tr.yawDevMax, dy / D2R);
        if (aimTargets.length) {
          let best = 180;
          for (const tp of aimTargets) {
            const to = tp.clone().sub(inst.pos); to.y = 0;
            const f = fwd.clone(); f.y = 0;
            if (to.lengthSq() > 1e-4 && f.lengthSq() > 1e-4) best = Math.min(best, f.angleTo(to) / D2R);
          }
          tr.bodyAimRun = best <= TH.aimErr ? tr.bodyAimRun + dt : 0;
          if (tr.bodyAimRun >= TH.aimHold) tr.bodyAimHeld = true;
        }
        // articulated groups
        if (inst.jointPose) {
          for (const [g, pose] of inst.jointPose) {
            let gt = tr.groups.get(g);
            if (!gt) {
              const info = ginfo.get(g) ?? { centroid: new Vector3(), lower: false, rEst: 0.5, pointing: false };
              gt = { info, q0: pose.quat.clone(), p0: pose.pos.clone(), prevQ: pose.quat.clone(), maxDevAng: 0, maxDevPos: 0, total: 0, net: 0, movingRot: 0, gTotal: 0, gNet: 0, gMoving: 0, netRef: { v: null }, swingRef: { v: null }, swingExt: 0, swingDir: 0, reversals: 0, allReversals: 0, maxSwing: 0, aimRun: 0, aimHeld: false, minErr: 180 };
              tr.groups.set(g, gt);
            }
            const q = pose.quat;
            gt.maxDevAng = Math.max(gt.maxDevAng, qAngle(gt.q0, q) / D2R);
            gt.maxDevPos = Math.max(gt.maxDevPos, pose.pos.distanceTo(gt.p0));
            const d = qAngle(gt.prevQ, q);
            if (d > 1e-6) {
              gt.total += d;
              if (moving) gt.movingRot += d;
              const dq = gt.prevQ.clone().invert().multiply(q);
              const sd = signedAngle(dq, gt.netRef, 0.005);
              gt.net += sd;
              if (tr.grounded) { gt.gTotal += d; gt.gNet += sd; if (moving) gt.gMoving += d; }
            }
            gt.prevQ.copy(q);
            // swing about the rest pose (legs go back and forth; wheels keep turning)
            const s = signedAngle(q, gt.swingRef) / D2R;
            gt.maxSwing = Math.max(gt.maxSwing, Math.abs(s));
            // count direction reversals of the swing angle (4° hysteresis): all of them, and those while travelling (gait)
            let rev = false;
            if (gt.swingDir === 0) { if (Math.abs(s - gt.swingExt) > 4) { gt.swingDir = Math.sign(s - gt.swingExt); gt.swingExt = s; } }
            else if (gt.swingDir > 0) { if (s > gt.swingExt) gt.swingExt = s; else if (s < gt.swingExt - 4) { rev = true; gt.swingDir = -1; gt.swingExt = s; } }
            else { if (s < gt.swingExt) gt.swingExt = s; else if (s > gt.swingExt + 4) { rev = true; gt.swingDir = 1; gt.swingExt = s; } }
            if (rev) { gt.allReversals++; if (moving) gt.reversals++; }
            // aiming: the group's forward (+z) points at a target, and it is the group's own turn (relative to the
            // body) that puts it there — the body's forward alone would miss by ≥ aimGain more (legs of a robot
            // that simply faces its target, or a frozen wheel, do not count)
            if (aimTargets.length) {
              const center = gt.info.centroid.clone().applyQuaternion(q).add(pose.pos).applyQuaternion(inst.quat).add(inst.pos);
              const gf = Z.clone().applyQuaternion(inst.quat.clone().multiply(q));
              const bf = Z.clone().applyQuaternion(inst.quat);
              let ok = false;
              for (const tp of aimTargets) {
                const to = tp.clone().sub(center);
                if (to.lengthSq() < 1e-6) continue;
                const ge = aimError(gf, to), be = aimError(bf, to);
                gt.minErr = Math.min(gt.minErr, ge);
                if (ge <= TH.aimErr && be >= ge + TH.aimGain) ok = true;
              }
              gt.aimRun = ok ? gt.aimRun + dt : 0;
              if (gt.aimRun >= TH.aimHold) gt.aimHeld = true;
            }
          }
        }
        // positions (sampled)
        if (sample) {
          const h = this.groundHeight(c, inst);
          tr.grounded = h <= TH.groundH;
          tr.samples.push({ t: w.time, pos: inst.pos.clone(), h, ridden, player: P.pos.clone() });
        }
        // returns / guard (main segment)
        if (this.seg === 'main') {
          const surf = Math.max(0, hdist(inst.pos, P.pos) - c.radius);
          if (surf >= TH.returnFar) tr.far = true;
          else if (tr.far && surf <= TH.returnNear) tr.returned = true;
        }
      });
    }
  }

  private add(fn: string, why: string) {
    if (!this.obs.has(fn)) { this.obs.add(fn); this.ev[fn] = why; }
  }

  /** Evaluate the current segment. */
  end() {
    const seg = this.seg;
    if (!seg) return;
    const allowed = new Set(SEGMENT_FNS[seg]);
    const add = (fn: string, why: string) => { if (allowed.has(fn)) this.add(fn, `[${seg}] ${why}`); };
    const movers: { tr: InstTrack; samples: Sample[] }[] = [];
    const segDur = Math.max(this.w.dt, this.w.time - this.segStart);
    for (const tr of this.segTracks.values()) {
      const S = tr.samples;
      const c = this.cons.get(tr.cid);
      if (!S.length || !c) continue;
      const tag = `${tr.cid}#${tr.k} "${c.spec.label}"`;
      if (process.env.OBS_DEBUG) for (const [k, g] of tr.groups) console.error(seg, tag, k, JSON.stringify({ lower: g.info.lower, rev: g.reversals, sw: +g.maxSwing.toFixed(1), tot: +g.total.toFixed(2), net: +g.net.toFixed(2), dev: +g.maxDevAng.toFixed(1), minErr: +g.minErr.toFixed(1), held: g.aimHeld, pointing: g.info.pointing, r: g.info.rEst }));
      // path lengths between samples (autonomous = neither end ridden)
      let pathH = 0, pathUp = 0, autoH = 0, autoUp = 0;
      for (let i = 1; i < S.length; i++) {
        const h = hdist(S[i].pos, S[i - 1].pos), up = Math.max(0, S[i].pos.y - S[i - 1].pos.y);
        pathH += h; pathUp += up;
        if (!S[i].ridden && !S[i - 1].ridden) { autoH += h; autoUp += up; }
      }
      const anyRidden = S.some((s) => s.ridden);
      // attached to the player (worn / carried along): keeps its offset while the player walks
      const pMove = S[S.length - 1].player.distanceTo(S[0].player);
      const off0 = S[0].pos.clone().sub(S[0].player);
      const offDev = Math.max(...S.map((s) => s.pos.clone().sub(s.player).distanceTo(off0)));
      const attached = !anyRidden && pMove >= TH.attachedPlayerMove && offDev < TH.attachedDev;
      const n = S.length;
      const groundFrac = S.filter((s) => s.h <= TH.groundH).length / n;
      const highFrac = S.filter((s) => s.h >= TH.flyH).length / n;
      const own = !attached;
      if (own && pathH >= TH.travel && groundFrac >= TH.groundFrac) {
        add('drives_on_ground', `${tag} ${pathH.toFixed(1)} m on the ground (${Math.round(groundFrac * 100)}% ≤ ${TH.groundH} m)`);
        const legs = [...tr.groups.entries()].filter(([, g]) => g.info.lower && g.reversals >= TH.legReversals && g.maxSwing >= TH.legSwingMin && g.maxSwing <= TH.legSwingMax);
        if (legs.length >= 2) add('walks', `${tag} legs ${legs.map(([k, g]) => `${k}(${g.reversals} swings, ±${Math.round(g.maxSwing)}°)`).join(' ')}`);
      }
      if (own && pathH + pathUp >= TH.travel && highFrac >= TH.flyFrac) add('flies', `${tag} ${(pathH + pathUp).toFixed(1)} m, ${Math.round(highFrac * 100)}% of the time ≥ ${TH.flyH} m up`);
      if (own && autoH + autoUp >= TH.travel) add('moves_autonomously', `${tag} travelled ${(autoH + autoUp).toFixed(1)} m on its own`);
      if (own && autoH + autoUp >= TH.agentMove) movers.push({ tr, samples: S.filter((s) => !s.ridden) });
      // parts
      for (const [g, gt] of tr.groups) {
        if (gt.maxDevAng >= TH.partDeg || gt.maxDevPos >= TH.partPos) { add('has_moving_parts', `${tag} group ${g} moved ${Math.round(gt.maxDevAng)}° / ${gt.maxDevPos.toFixed(2)} m`); break; }
      }
      // wheels: low groups that keep turning one way while the construct rolls along the ground, ≈ travel / radius
      if (tr.travelGround >= TH.wheelMinTravel) {
        for (const [g, gt] of tr.groups) {
          if (!gt.info.lower || gt.gTotal < TH.wheelMinTurn || Math.abs(gt.gNet) < TH.wheelNet * gt.gTotal || gt.gMoving < TH.wheelMoving * gt.gTotal) continue;
          const ratio = gt.gTotal / tr.travelGround, want = 1 / gt.info.rEst;
          if (ratio >= want / TH.wheelRatio && ratio <= want * TH.wheelRatio) { add('wheels_spin', `${tag} group ${g} turned ${(gt.gTotal / (2 * Math.PI)).toFixed(1)} rev over ${tr.travelGround.toFixed(1)} m on the ground (r≈${gt.info.rEst.toFixed(2)} m)`); break; }
        }
      }
      for (const [g, gt] of tr.groups) {
        // pointing parts only (not legs, tails, wings), not spinners (wheels, props, rotors: a full turn), not fast swingers
        const spinner = gt.total >= 2 * Math.PI, swinger = gt.allReversals / segDur >= TH.aimMaxSwingRate;
        if (gt.aimHeld && gt.info.pointing && gt.maxDevAng >= TH.aimGroupMoved && !spinner && !swinger) { add('aims_at_target', `${tag} group ${g} turned onto a target (≤ ${TH.aimErr}° for ≥ ${TH.aimHold} s)`); break; }
      }
      if (tr.bodyAimHeld && tr.yawDevMax >= TH.aimBodyYaw && this.shotBy.get(seg)?.has(tr.cid)) add('aims_at_target', `${tag} turned its body ${Math.round(tr.yawDevMax)}° onto a target and fired`);
      if (tr.rideMove >= TH.rideMove) add('rideable', `${tag} moved ${tr.rideMove.toFixed(1)} m under the rider's input`);
      if (tr.carry >= TH.carry) add('carries_player', `${tag} carried the player ${tr.carry.toFixed(1)} m`);
      if (seg === 'follow' && !anyRidden && own) {
        // distance from the player to the construct's body (bounding sphere), not its centre
        const gap = (s: Sample) => Math.max(0, s.pos.distanceTo(s.player) - c.radius);
        const near = S.filter((s) => gap(s) <= TH.followDist).length / n;
        const last = S[n - 1];
        const selfMove = last.pos.distanceTo(S[0].pos);
        if (process.env.OBS_DEBUG) console.error('follow', tag, JSON.stringify({ pMove, near, selfMove, gaps: S.map((s) => +gap(s).toFixed(1)) }));
        if (pMove >= TH.followPlayerMove && near >= TH.followFrac && gap(last) <= TH.followDist && selfMove >= TH.followSelfMove) add('follows_player', `${tag} within ${TH.followDist} m for ${Math.round(near * 100)}% while the player moved ${pMove.toFixed(0)} m (it moved ${selfMove.toFixed(0)} m)`);
      }
      if (seg === 'main') {
        if (tr.returned) add('returns', `${tag} came back within ${TH.returnNear} m of the player`);
        // guard: its spot is where it ends up; it holds that spot (within 8 m) from some time on, and hits a hostile near the
        // spot while holding it — either it held the spot all along, or it had been holding it ≥ 2 s (not a charge that ends in a kill)
        const spot = S[n - 1].pos;
        let from = n - 1;
        while (from > 0 && hdist(S[from - 1].pos, spot) <= TH.guardRadius) from--;
        const tStay = S[from].t, all = from === 0;
        const hit = this.damage.find((d) => d.by === tr.cid && d.hostile && d.inWindow && d.pos && hdist(d.pos, spot) <= TH.guardNear && (all || d.t >= tStay + TH.guardHold));
        if (process.env.OBS_DEBUG) console.error('guard', tag, JSON.stringify({ from, n, tStay: tStay - this.segStart, hits: this.damage.filter((d) => d.by === tr.cid).map((d) => ({ id: d.id, t: +(d.t - this.segStart).toFixed(1), dist: d.pos ? +hdist(d.pos, spot).toFixed(1) : null, host: d.hostile })) }));
        if (hit) add('guards_area', `${tag} held its spot${all ? ' all window' : ` from t+${(tStay - this.segStart).toFixed(1)} s`} and hit ${hit.id} ${hdist(hit.pos!, spot).toFixed(0)} m from it`);
      }
    }
    // several independently moving agents
    if (movers.length >= 2) {
      let sep = 0;
      for (let a = 0; a < movers.length && sep < TH.agentSep; a++) {
        for (let b = a + 1; b < movers.length; b++) {
          const A = movers[a].samples, B = movers[b].samples;
          const m = Math.min(A.length, B.length);
          let lo = Infinity, hi = -Infinity;
          for (let i = 0; i < m; i++) { const d = A[A.length - m + i].pos.distanceTo(B[B.length - m + i].pos); lo = Math.min(lo, d); hi = Math.max(hi, d); }
          sep = Math.max(sep, hi - lo);
        }
      }
      if (sep >= TH.agentSep) add('multiple_agents', `${movers.length} instances moved independently (${movers.map((m) => m.tr.key).join(', ')})`);
    }
    // loose objects shoved
    for (const [id, l] of this.loose) if (l.credited && l.maxDisp >= TH.push) add('pushes_objects', `${id} moved ${l.maxDisp.toFixed(1)} m by an attributed construct`);
    if (seg === 'main') {
      this.eventsOpen = false;
      this.mainEnd = this.w.time;
      for (const id of this.opts.targetIds()) { const e = this.w.entities.get(id); if (!e || e.disabled || e.hp <= 0) this.goneAtMainEnd.add(id); }
    }
    this.seg = null;
  }

  /** Final result (event-based functions: commit → end of the main window). */
  result(): ObserveResult {
    for (const [seg, ids] of this.shotBy) if (SEGMENT_FNS[seg as Segment].includes('shoots')) this.add('shoots', `[${seg}] fired by ${[...ids].join(', ')}`);
    const targets = new Set(this.opts.targetIds());
    const byA = this.damage.filter((d) => this.isA(d.by) && d.t >= this.tStart && d.inWindow);
    const onTarget = byA.filter((d) => d.kind === 'entity' && targets.has(d.id));
    if (onTarget.length) this.add('damages_target', `${onTarget[0].id} hurt by ${onTarget[0].by} (${onTarget[0].cause})`);
    for (const d of onTarget) {
      if (d.final || this.goneAtMainEnd.has(d.id)) { this.add('destroys_target', `${d.id} ${d.final ? 'disabled/broken' : 'gone'} after damage by ${d.by}`); break; }
    }
    const hostile = byA.find((d) => d.hostile);
    if (hostile) this.add('attacks_enemies', `${hostile.kind} ${hostile.id} hurt by ${hostile.by} (${hostile.cause})`);
    for (const e of this.events) {
      const mine = e.construct === undefined || this.isA(e.construct);
      if (!mine || e.t > this.mainEnd) continue;
      if (e.type === 'inspiration' && e.stale) this.add('stale_penalty', `stale: ${(e.reasons ?? []).join('; ')} (mul ${e.mul}, cost ×${e.costMul})`);
      if (e.type === 'inspiration' && !e.stale && e.mul > 1 && e.novelty >= INSPIRED_AT) this.add('creative_bonus', `novelty ${e.novelty} → power ×${e.mul}`);
      if (e.type === 'creative_solution') this.add('creative_bonus', `creative solution: ${e.reason}`);
    }
    return { observed: new Set(this.obs), evidence: { ...this.ev } };
  }
}
