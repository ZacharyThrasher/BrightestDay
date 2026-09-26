/**
 * Adaptive construct controls: whatever you made gets its own mouse actions.
 *
 * Typing is the creation skill; the mouse is the execution skill. Every construct a player owns can be
 * EQUIPPED (the newest usable one is, automatically; the wheel cycles; naming it — "the hammer" — equips it),
 * and the equipped construct's AFFORDANCE (derived from what it is and does: spec behaviours, machine parts,
 * concept) decides what LMB (primary) and RMB (secondary: tap, or hold to charge/aim) do. Q/E stay tethers.
 *
 * Everything here runs INSIDE the sim from PlayerInput (primary / secondary / cycle + the aim in yaw/pitch),
 * so it is deterministic and per player: `update(player, input, dt)` for each lantern. Actions never hurt their
 * owner or the owner's constructs, cost a little willpower, and emit events for the juice layer:
 *   'action_start'   { construct, player, kind, action, charge }   (a charge: action 'charge' when it begins, then the
 *                                                                   release starts e.g. 'charged_strike' / 'throw')
 *   'action_charge'  { construct, player, kind, level }            (at 25 / 50 / 75 / 100 %)
 *   'action_release' { construct, player, kind, action, level }
 *   'action_hit'     { construct, player, target, targetKind, energy, kind, pos }
 *   'action_fail'    { construct, player, kind, reason }
 *   'equip'          { construct, player, label, kind }
 * Hits also emit the classic 'impact' / 'smash' events, so the renderer's FX and the creativity economy's
 * attribution (inspiration.ts: an unusual tool landing a hit/defeat is a creative solution) pick them up.
 *
 * The affordance table (AFFORDANCES + classify) is the one place to extend: add a kind, give it labels and a
 * branch in `primaryPress` / `secondaryRelease`.
 */
import { Vector3 } from 'three';
import { G, ALL, groups } from '../physics.js';
import type { World, Entity } from '../world.js';
import type { Player, PlayerInput } from '../player.js';
import type { Construct, Inst, ConstructRuntime } from './runtime.js';
import { LOCAL_OWNER, ballistic, quatFromDir } from './runtime.js';
import type { Behavior } from './spec.js';
import type { TargetRef } from './machines.js';
import { COMBAT, interceptPoint } from './combat.js';

export type ActionKind = 'strike' | 'grab' | 'shield' | 'fire' | 'command' | 'ride' | 'launch' | 'detonate' | 'yank' | 'field' | 'hurl';

/** Tuning (see the report in the commit / docs for why). Damages are per hit before the charge multiplier. */
export const ACTIONS = {
  /** hold RMB this long for a full charge (s); a full charge doubles damage/knockback */
  chargeTime: 1.0,
  chargeMax: 1.0,
  /** RMB released sooner than this is a tap */
  tap: 0.2,
  doubleTap: 0.35,
  /** lock-on: cone half-angle (deg) or this close to the aim ray (m) */
  lockCone: 15,
  lockRay: 6,
  strike: { reach: 36, speed: 52, damage: 28, knock: 13, recovery: 0.22, recoveryPerMass: 0.2, slamPitch: -0.5, slamRange: 9, slamRadius: 4.5, slamDamage: 24 },
  grab: { reach: 45, throwSpeed: 32, throwSpeedCharged: 60, throwDamage: 18, slamDamage: 26 },
  shield: { bashReach: 4.5, bashSpeed: 30, bashDamage: 12, bashKnock: 20, shove: 6 },
  hurl: { speed: 36, damage: 22, flight: 4 },
  launch: { speed: 34, place: 40 },
  yank: { speed: 24 },
  /** willpower per action (× cost scale of the construct, 0.6–2) */
  cost: { strike: 2, charge: 3, grab: 2, throw: 2, slam: 2, bash: 2, fire: 1.5 /* per second */, command: 1, launch: 3, detonate: 0, yank: 2, field: 1, hurl: 3, pull: 1 },
};

/** What LMB / RMB do for each kind (HUD prompt). `holding` = labels while a grabber holds something. */
export const AFFORDANCES: Record<ActionKind, { lmb: string; rmb: string; holdingLmb?: string; holdingRmb?: string }> = {
  strike: { lmb: 'Smash', rmb: 'Charge' },
  grab: { lmb: 'Grab', rmb: 'Slap', holdingLmb: 'Throw', holdingRmb: 'Charge throw · tap Slam' },
  shield: { lmb: 'Bash', rmb: 'Raise' },
  fire: { lmb: 'Fire', rmb: 'Precision' },
  command: { lmb: 'Sic ’em', rmb: 'Go there · ×2 Come back' },
  ride: { lmb: 'Fire', rmb: 'Boost' },
  launch: { lmb: 'Launch', rmb: 'Place' },
  detonate: { lmb: 'Detonate', rmb: 'Throw' },
  yank: { lmb: 'Yank', rmb: 'Let go' },
  field: { lmb: 'Push/Pull', rmb: 'Aim' },
  hurl: { lmb: 'Hurl', rmb: 'Pull back' },
};

/** concept heads (spec.concept before '+') that are melee weapons / grabbers / shields */
const MELEE = new Set(['hammer', 'mallet', 'fist', 'boxing glove', 'glove', 'foot', 'boot', 'sword', 'axe', 'bat', 'club', 'spear', 'wrecking ball', 'wrecking_ball', 'pendulum', 'saw', 'drill', 'mace', 'gear', 'spike']);
const MELEE_VERB: Record<string, string> = { sword: 'Slash', axe: 'Chop', fist: 'Punch', 'boxing glove': 'Punch', glove: 'Punch', foot: 'Kick', boot: 'Kick', spear: 'Thrust', saw: 'Slice', drill: 'Drill', bat: 'Swing' };
const GRABBER = new Set(['hand', 'claw', 'lasso', 'whip', 'hook', 'octopus', 'tentacle']);
const SHIELD = new Set(['shield', 'panel', 'wall', 'door', 'dome', 'bubble', 'umbrella', 'armor', 'mirror']);
/** support structures: usable, but never auto-equipped (a ramp you just made shouldn't fly off on your next click) */
const SCENERY = new Set(['platform', 'slab', 'bridge', 'ramp', 'stairs', 'ladder', 'slide', 'pillar', 'tower', 'elevator', 'conveyor', 'carousel', 'tube', 'funnel', 'anchor']);

const UP = new Vector3(0, 1, 0);
const _v = new Vector3(), _v2 = new Vector3();

export interface Lock { ref: TargetRef; pos: Vector3; vel: Vector3; radius: number }

interface Motion {
  cid: string; inst: number; kind: 'swing' | 'bash' | 'slam' | 'shove';
  /** where the lunge has got to (the motion owns the pose; the runtime's hold pull is overridden) */
  cur: Vector3;
  dir: Vector3; speed: number; reach: number; travelled: number;
  damage: number; knock: number; hit: Set<string>;
  /** go = lunging, back = returning to `home` (constructs that are not held stay put) */
  phase: 'go' | 'back'; home: Vector3 | null; charge: number; end: Vector3 | null;
}

interface Flight { cid: string; kind: 'construct' | 'entity'; id: string; inst: number; until: number; damage: number; hit: Set<string>; prev: Vector3; start: number }

interface PState {
  equipped: string | null;
  seen: Set<string>;
  lastFocus: string | null;
  prevP: boolean; prevS: boolean;
  pDownAt: number; sDownAt: number; lastSTap: number;
  charging: { cid: string; kind: 'strike' | 'throw'; start: number; emitted: number } | null;
  busyUntil: number;
  motions: Motion[];
  flights: Flight[];
  raised: string | null;
  aiming: string | null;
  precise: string | null;
  fireCd: number;
  /** reflected shots already redirected toward the crosshair */
  redirected: Set<number>;
}

export interface ActionView {
  equipped: { id: string; label: string; kind: ActionKind; lmb: string; rmb: string } | null;
  list: { id: string; label: string; kind: ActionKind; equipped: boolean }[];
  charge: number;
  precise: boolean;
  lock: Vector3 | null;
  /** aim preview: points of a line / arc, and an optional area radius at the end */
  preview: { kind: 'line' | 'arc'; points: Vector3[]; radius?: number } | null;
}

function has<T extends Behavior['type']>(c: Construct, t: T): Extract<Behavior, { type: T }> | undefined {
  return c.spec.behaviors.find((b) => b.type === t) as any;
}
const clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v);
const head = (c: Construct) => (c.spec.concept.split('+')[0] ?? '').toLowerCase();
const isMirror = (c: Construct) => /\bmirror/.test(c.spec.label) || head(c) === 'mirror';

/** A player's id (World.players[] gives each lantern an `id`; the single local player is LOCAL_OWNER). */
export function playerId(P: Player): string { return ((P as unknown as { id?: string }).id) ?? LOCAL_OWNER; }

/**
 * THE affordance table: what kind of thing is this, as far as the mouse is concerned.
 * Order matters (a rideable tank you're in is ridden; a tank you're not in fires; a dog is ordered).
 */
export function classify(c: Construct, P?: Player | null): ActionKind {
  const h = head(c);
  const types = new Set(c.spec.behaviors.map((b) => b.type));
  if (P?.riding?.id === c.id) return 'ride';
  if (types.has('explode')) return 'detonate';
  if (c.mach?.ranged || types.has('shoot')) return 'fire';
  if (types.has('launch')) return 'launch';
  if (types.has('tether')) return 'yank';
  if (MELEE.has(h) || types.has('strike')) return 'strike';
  if (GRABBER.has(h) || types.has('grab') || types.has('restrain')) return 'grab';
  if (c.mach?.drive || c.mach?.brain) return 'command';
  if (types.has('field')) return 'field';
  if (types.has('reflect') || SHIELD.has(h) || isMirror(c)) return 'shield';
  return 'hurl';
}

/** HUD labels for a construct's current affordance. */
export function labels(c: Construct, kind: ActionKind, holding: boolean): { lmb: string; rmb: string } {
  const a = AFFORDANCES[kind];
  if (kind === 'strike') return { lmb: MELEE_VERB[head(c)] ?? a.lmb, rmb: a.rmb };
  if (kind === 'grab' && holding) return { lmb: a.holdingLmb!, rmb: a.holdingRmb! };
  if (kind === 'shield' && (isMirror(c) || has(c, 'reflect'))) return { lmb: a.lmb, rmb: 'Reflect' };
  return { lmb: a.lmb, rmb: a.rmb };
}

export class ActionSystem {
  private states = new Map<string, PState>();
  /**
   * Constructs mid-lunge pass through creatures and players: the action's own hit test is the one damage path
   * (predictable numbers, a juke really dodges), instead of a 60 m/s kinematic shove crushing whatever it
   * brushes. cid → { until (Infinity while the motion runs), original collision groups per collider }.
   */
  private ghosts = new Map<string, { until: number; groups: Map<number, number> }>();
  private get rt(): ConstructRuntime { return this.w.constructs; }

  constructor(private w: World) {}

  private state(id: string): PState {
    let s = this.states.get(id);
    if (!s) {
      s = {
        equipped: null, seen: new Set(), lastFocus: null, prevP: false, prevS: false, pDownAt: -1, sDownAt: -1, lastSTap: -10,
        charging: null, busyUntil: 0, motions: [], flights: [], raised: null, aiming: null, precise: null, fireCd: 0, redirected: new Set(),
      };
      this.states.set(id, s);
    }
    return s;
  }

  /** What a remote player's HUD needs of their action state (netcode: the host ships it to the client). */
  netState(id: string): { eq: string | null; precise: string | null; charge: { cid: string; kind: 'strike' | 'throw'; start: number } | null } {
    const s = this.state(id);
    return { eq: s.equipped, precise: s.precise, charge: s.charging ? { cid: s.charging.cid, kind: s.charging.kind, start: s.charging.start } : null };
  }

  /** Mirror a host's action state for the HUD (the client never steps actions itself). */
  applyNetState(id: string, v: ReturnType<ActionSystem['netState']>) {
    const s = this.state(id);
    s.equipped = v.eq;
    s.precise = v.precise;
    s.charging = v.charge ? { ...v.charge, emitted: 0 } : null;
  }

  // ------------------------------------------------------------------ equipping

  /** Constructs this player can use, oldest first. */
  usable(owner: string): Construct[] {
    const out: Construct[] = [];
    for (const c of this.rt.list.values()) if (c.owner === owner && c.insts.some((i) => i.alive) && this.w.time < c.expires) out.push(c);
    return out;
  }

  equipped(P: Player): Construct | null {
    const s = this.states.get(playerId(P));
    if (P.riding) return this.rt.get(P.riding.id);
    return s ? this.rt.get(s.equipped) : null;
  }

  /** Equip a construct by id (typed "the hammer" does this through the focus). */
  equip(P: Player, id: string | null): boolean {
    const s = this.state(playerId(P));
    const c = this.rt.get(id);
    if (!c || c.owner !== playerId(P)) return false;
    if (s.equipped === c.id) return true;
    this.unequip(s);
    s.equipped = c.id;
    this.w.emit('equip', { construct: c.id, player: playerId(P), label: c.spec.label, kind: classify(c, P) });
    return true;
  }

  private unequip(s: PState) {
    s.charging = null;
    s.raised = null;
    s.aiming = null;
    s.precise = null;
  }

  private syncEquip(P: Player, s: PState, list: Construct[]) {
    const id = playerId(P);
    // newly formed constructs: the newest usable one is equipped
    let fresh: Construct | null = null;
    for (const c of list) if (!s.seen.has(c.id)) { s.seen.add(c.id); if (!SCENERY.has(head(c)) && !has(c, 'mount')) fresh = c; }
    if (s.seen.size > 256) s.seen = new Set(list.map((c) => c.id));
    if (fresh) this.equip(P, fresh.id);
    // naming a construct in the ring ("the hammer") focuses it: that equips it too
    const f = this.rt.focusId;
    if (f !== s.lastFocus) { s.lastFocus = f; const fc = this.rt.get(f); if (fc && fc.owner === id && !fresh) this.equip(P, fc.id); }
    // equipped one gone: fall back to the next newest
    if (!s.equipped || !list.some((c) => c.id === s.equipped)) {
      const next = [...list].reverse().find((c) => !SCENERY.has(head(c)) && !has(c, 'mount')) ?? list[list.length - 1] ?? null;
      if (s.equipped !== (next?.id ?? null)) { this.unequip(s); s.equipped = next?.id ?? null; if (next) this.w.emit('equip', { construct: next.id, player: id, label: next.spec.label, kind: classify(next, P) }); }
    }
  }

  private cycle(P: Player, s: PState, list: Construct[], n: number) {
    if (!list.length) return;
    const i = Math.max(0, list.findIndex((c) => c.id === s.equipped));
    const k = ((i + Math.sign(n)) % list.length + list.length) % list.length;
    this.equip(P, list[k].id);
  }

  // ------------------------------------------------------------------ per tick

  /**
   * Phase 1, at the start of the tick (before constructs move): read this player's buttons and start actions,
   * so a click acts on the very tick it arrives (a gun fires, a lunge starts, a grab reaches out).
   */
  input(P: Player, inp: PlayerInput, dt: number) {
    P.yaw = inp.yaw; // aim with this tick's view (Player.update sets the same values later this tick)
    P.pitch = inp.pitch;
    const id = playerId(P);
    const s = this.state(id);
    const t = this.w.time;
    const list = this.usable(id);
    this.syncEquip(P, s, list);
    if (inp.cycle && !P.riding) this.cycle(P, s, list, inp.cycle);
    const pDown = !!inp.primary, sDown = !!inp.secondary;
    const pPress = pDown && !s.prevP, sPress = sDown && !s.prevS, sRel = !sDown && s.prevS;
    if (pPress) s.pDownAt = t;
    if (sPress) s.sDownAt = t;
    const c = P.riding ? null : this.rt.get(s.equipped);
    if (c) {
      const kind = classify(c, P);
      if (pPress && t >= s.busyUntil) this.primaryPress(P, s, c, kind);
      if (pDown) this.primaryHeld(P, s, c, kind, dt);
      if (sPress) this.secondaryPress(P, s, c, kind);
      if (sDown) this.secondaryHeld(P, s, c, kind, dt);
      if (sRel) this.secondaryRelease(P, s, c, kind, t - s.sDownAt);
    } else this.unequip(s);
    if (!sDown) { s.raised = null; s.aiming = null; s.precise = null; s.charging = null; }
    s.prevP = pDown;
    s.prevS = sDown;
  }

  /** Phase 2, after the runtime drove its bodies: lunges, wind-ups, raised shields, things in flight. */
  step(P: Player, dt: number) {
    const s = this.state(playerId(P));
    this.windUp(P, s);
    this.stepMotions(P, s, dt);
    this.stepFlights(s);
    this.stepRaised(P, s);
    this.unghost();
  }

  /** Both phases (tests / tools that step a player on its own). */
  update(P: Player, inp: PlayerInput, dt: number) { this.input(P, inp, dt); this.step(P, dt); }

  // ------------------------------------------------------------------ inputs → actions

  private primaryPress(P: Player, s: PState, c: Construct, kind: ActionKind) {
    const A = ACTIONS;
    switch (kind) {
      case 'strike': this.swing(P, s, c, 0, false); break;
      case 'grab': {
        const inst = this.nearestInst(c, P);
        if (!inst) return;
        if (inst.cargo) { this.throwCargo(P, s, c, inst, 0); break; }
        if (inst.grab && !inst.grab.attached) return; // already reaching
        const lock = this.lockOn(P, 'grab', A.grab.reach, c);
        if (lock?.ref.kind === 'entity' && c.kinematic) {
          if (!this.spend(P, c, A.cost.grab, kind)) return;
          inst.grab = { target: { kind: 'entity', id: lock.ref.id }, then: 'hold', v: 0, attached: false };
          s.busyUntil = this.w.time + 0.25;
          this.start(P, c, kind, 'grab');
        } else this.swing(P, s, c, 0, false, 0.6); // nothing to grab: a slap
        break;
      }
      case 'shield': {
        const held = !!has(c, 'hold');
        if (!this.spend(P, c, A.cost.bash, kind)) return;
        const inst = this.nearestInst(c, P);
        if (!inst) return;
        this.ensureKinematic(c);
        const dir = held ? P.look(new Vector3()) : P.forward(new Vector3());
        s.motions = s.motions.filter((m) => m.cid !== c.id);
        this.ghost(c);
        s.motions.push({
          cid: c.id, inst: c.insts.indexOf(inst), kind: held ? 'bash' : 'shove', cur: inst.pos.clone(), dir, speed: A.shield.bashSpeed, reach: held ? A.shield.bashReach : A.shield.shove, travelled: 0,
          damage: A.shield.bashDamage * this.massF(c), knock: A.shield.bashKnock, hit: new Set(), phase: 'go', home: held ? null : null, charge: 0, end: null,
        });
        s.busyUntil = this.w.time + 0.45;
        this.start(P, c, kind, held ? 'bash' : 'shove');
        break;
      }
      case 'fire': this.start(P, c, kind, 'fire'); break;
      case 'command': {
        if (!this.spend(P, c, A.cost.command, kind)) return;
        const lock = this.lockOn(P, 'enemy', 150, c) ?? this.nearestEnemyTo(c, this.aimPoint(P, c), 12);
        if (lock) this.rt.machines.command(c.id, { goal: 'hunt', target: lock.ref.kind === 'entity' ? { kind: 'entity', id: lock.ref.id } : lock.ref.kind === 'player' ? { kind: 'player', id: lock.ref.id } : { kind: 'construct', id: lock.ref.id } });
        else this.rt.machines.command(c.id, { goal: 'goto', target: { kind: 'point', p: this.aimPoint(P, c).toArray() as [number, number, number] } });
        this.start(P, c, kind, lock ? 'hunt' : 'goto');
        break;
      }
      case 'launch': this.launch(P, s, c); break;
      case 'detonate':
        c.explodeAt = this.w.time; // the runtime blows it next tick
        this.start(P, c, kind, 'detonate');
        break;
      case 'yank': this.yank(P, s, c); break;
      case 'field': {
        const f = has(c, 'field');
        if (!f || !this.spend(P, c, A.cost.field, kind)) return;
        if (f.mode === 'attract') f.mode = 'repel';
        else if (f.mode === 'repel') f.mode = 'attract';
        else if (f.mode === 'lift') f.mode = 'attract';
        else f.axis = [-(f.axis?.[0] ?? 0), -(f.axis?.[1] ?? 0), -(f.axis?.[2] ?? 1)];
        this.start(P, c, kind, f.mode === 'wind' ? 'reverse' : f.mode);
        break;
      }
      case 'hurl': this.hurl(P, s, c, 0); break;
      case 'ride': break;
    }
  }

  private primaryHeld(P: Player, s: PState, c: Construct, kind: ActionKind, dt: number) {
    if (kind !== 'fire') return;
    const A = ACTIONS;
    if (!this.spend(P, c, A.cost.fire * dt * this.costScale(c), kind, true)) return;
    const precise = s.precise === c.id;
    const aimAt = this.fireAim(P, c, precise);
    for (const inst of c.insts) {
      if (!inst.alive) continue;
      if (inst.m) inst.m.cmd = { aim: aimAt.clone(), fire: true, until: this.w.time + 0.05, precise };
    }
    const sh = has(c, 'shoot');
    if (sh) {
      s.fireCd -= dt;
      if (s.fireCd > 0) return;
      s.fireCd = 1 / Math.max(0.1, sh.rate);
      for (const inst of c.insts) {
        if (!inst.alive) continue;
        const d = aimAt.clone().sub(inst.pos).normalize();
        const start = inst.pos.clone().addScaledVector(d, c.radius * 0.6 + sh.size + 0.3);
        this.w.spawnProjectile(start, d.multiplyScalar(sh.speed), null, 10, sh.size, { friendly: true, owner: c.id, ownerPlayer: c.owner, kind: 'gun' });
        inst.shootCd = Math.max(inst.shootCd, 1 / Math.max(0.1, sh.rate)); // don't double up with its own auto-fire
        this.w.emit('construct_fire', { construct: c.id, kind: 'gun', pos: start.toArray(), dir: d.toArray() });
      }
    }
  }

  private secondaryPress(P: Player, s: PState, c: Construct, kind: ActionKind) {
    const t = this.w.time;
    switch (kind) {
      case 'grab': {
        const inst = this.nearestInst(c, P);
        if (!inst?.cargo) this.swing(P, s, c, 0, false, 0.6); // empty-handed: a slap
        break;
      }
      case 'shield':
        s.raised = c.id;
        if (isMirror(c) && !has(c, 'reflect')) c.spec.behaviors.push({ type: 'reflect' }); // a mirror you hold up reflects
        this.ensureKinematic(c);
        this.start(P, c, kind, 'raise');
        break;
      case 'fire': s.precise = c.id; this.start(P, c, kind, 'precision'); break;
      case 'field': s.aiming = c.id; this.ensureKinematic(c); break;
      case 'command':
        if (t - s.lastSTap < ACTIONS.doubleTap) {
          this.rt.machines.command(c.id, { goal: 'follow' });
          s.lastSTap = -10;
          this.start(P, c, kind, 'follow');
        } else {
          s.lastSTap = t;
          this.rt.machines.command(c.id, { goal: 'goto', target: { kind: 'point', p: this.aimPoint(P, c).toArray() as [number, number, number] } });
          this.start(P, c, kind, 'goto');
        }
        break;
      case 'launch': this.place(P, c); break;
      case 'detonate': this.hurl(P, s, c, 0); break;
      case 'yank': this.rt.dismiss(c.id, 'released'); this.w.emit('action_release', { construct: c.id, player: playerId(P), kind, action: 'let_go', level: 0 }); break;
      case 'hurl': this.pullBack(P, c); break;
      default: break;
    }
  }

  private secondaryHeld(P: Player, s: PState, c: Construct, kind: ActionKind, _dt: number) {
    // charging starts as soon as you can swing again (holding RMB through a recovery queues the charge)
    if (!s.charging && this.w.time >= s.busyUntil) {
      if (kind === 'strike') s.charging = { cid: c.id, kind: 'strike', start: this.w.time, emitted: 0 };
      else if (kind === 'grab' && this.nearestInst(c, P)?.cargo) s.charging = { cid: c.id, kind: 'throw', start: this.w.time, emitted: 0 };
      if (s.charging) this.start(P, c, kind, 'charge');
    }
    const ch = s.charging;
    if (ch && ch.cid === c.id) {
      const level = this.chargeLevel(s);
      const step = Math.floor(level * 4 + 1e-6);
      if (step > ch.emitted) { ch.emitted = step; this.w.emit('action_charge', { construct: c.id, player: playerId(P), kind, level: step / 4 }); }
    }
    if (kind === 'fire' && s.precise === c.id) {
      const aimAt = this.fireAim(P, c, true);
      for (const inst of c.insts) if (inst.m && !(inst.m.cmd && inst.m.cmd.fire && inst.m.cmd.until >= this.w.time)) inst.m.cmd = { aim: aimAt, fire: false, until: this.w.time + 0.05, precise: true };
    }
  }

  private secondaryRelease(P: Player, s: PState, c: Construct, kind: ActionKind, held: number) {
    const ch = s.charging;
    s.charging = null;
    if (!ch || ch.cid !== c.id) return;
    const level = clamp(held / ACTIONS.chargeTime, 0, ACTIONS.chargeMax);
    if (ch.kind === 'strike') this.swing(P, s, c, level, this.wantsSlam(P));
    else if (ch.kind === 'throw') {
      const inst = this.nearestInst(c, P);
      if (!inst?.cargo) return;
      if (held < ACTIONS.tap) this.slamCargo(P, s, c, inst);
      else this.throwCargo(P, s, c, inst, level);
    }
  }

  /** While a strike charges, the weapon draws back and up (steady-state offset against the runtime's hold lerp). */
  private windUp(P: Player, s: PState) {
    const ch = s.charging;
    if (!ch || ch.kind !== 'strike') return;
    const c = this.rt.get(ch.cid);
    const inst = c ? this.nearestInst(c, P) : null;
    if (!c || !inst || !inst.body.isKinematic() || s.motions.some((m) => m.cid === c.id)) return;
    const level = this.chargeLevel(s);
    const a = 1 - Math.exp(-18 * this.w.dt);
    inst.pos.add(P.look(new Vector3()).multiplyScalar(-1.1 * level).addScaledVector(UP, 0.9 * level).multiplyScalar(a));
    inst.body.setNextKinematicTranslation(inst.pos);
  }

  /** A charged blow aimed at the ground close by (or steeply down) is a ground slam. */
  wantsSlam(P: Player): boolean {
    if (P.pitch < ACTIONS.strike.slamPitch) return true;
    const a = P.aim(ACTIONS.strike.slamRange + 6);
    return !!a.collider && this.w.ownerOf(a.collider)?.kind === 'static' && a.normal.y > 0.6 && a.point.distanceTo(P.pos) < ACTIONS.strike.slamRange;
  }

  private chargeLevel(s: PState) { return s.charging ? clamp((this.w.time - s.charging.start) / ACTIONS.chargeTime, 0, ACTIONS.chargeMax) : 0; }

  // ------------------------------------------------------------------ strike / bash motions

  /** A committed lunge toward the lock (intercept) or the crosshair; `slam` = down into the ground with a shockwave. */
  private swing(P: Player, s: PState, c: Construct, charge: number, slam: boolean, dmgMul = 1) {
    const A = ACTIONS.strike;
    const t = this.w.time;
    if (t < s.busyUntil) return;
    const inst = this.nearestInst(c, P);
    if (!inst || inst.strike && inst.strike.phase !== 'done') return;
    if (!this.spend(P, c, ACTIONS.cost.strike + ACTIONS.cost.charge * charge, 'strike')) return;
    this.ensureKinematic(c);
    const mf = this.massF(c);
    const speed = clamp(A.speed / Math.sqrt(mf), COMBAT.strike.minSpeed, COMBAT.strike.maxSpeed) * (1 + 0.25 * charge);
    const from = inst.pos.clone();
    let to: Vector3;
    if (slam) {
      const a = P.aim(60);
      to = a.point.distanceTo(P.pos) < 16 ? a.point.clone() : P.pos.clone().addScaledVector(P.forward(new Vector3()), 5).setY(P.pos.y - 1);
    } else {
      const lock = this.lockOn(P, 'enemy', A.reach, c);
      if (lock) to = interceptPoint(from, speed * 0.8, lock.pos, lock.vel, new Vector3());
      else to = this.aimPoint(P, c, A.reach + 10);
    }
    const d = to.clone().sub(from);
    const dist = d.length();
    if (dist < 1e-3) d.copy(P.look(new Vector3())); else d.divideScalar(dist);
    const held = !!has(c, 'hold');
    s.motions = s.motions.filter((m) => m.cid !== c.id);
    this.ghost(c);
    s.motions.push({
      cid: c.id, inst: c.insts.indexOf(inst), kind: slam ? 'slam' : 'swing', cur: from.clone(), dir: d, speed, reach: Math.min(A.reach, dist + 1.2), travelled: 0,
      damage: (slam ? A.slamDamage : A.damage) * (0.55 + 0.45 * mf) * (1 + charge) * dmgMul, knock: Math.min(34, A.knock * mf * (1 + charge)), hit: new Set(),
      phase: 'go', home: held ? null : from, charge, end: slam ? to : null,
    });
    // recovery: heavier weapons recover slower; the clock starts now and the lunge itself adds to it
    s.busyUntil = t + Math.min(A.reach, dist + 1.2) / speed + A.recovery + A.recoveryPerMass * mf;
    this.start(P, c, 'strike', slam ? 'slam' : charge > 0 ? 'charged_strike' : dmgMul < 1 ? 'slap' : 'strike', charge);
    if (charge > 0) this.w.emit('action_release', { construct: c.id, player: playerId(P), kind: 'strike', action: slam ? 'slam' : 'strike', level: charge });
  }

  private stepMotions(P: Player, s: PState, dt: number) {
    for (let k = s.motions.length - 1; k >= 0; k--) {
      const m = s.motions[k];
      const c = this.rt.get(m.cid);
      const inst = c?.insts[m.inst];
      if (!c || !inst?.alive || !inst.body.isKinematic()) { this.endMotion(s, k); continue; }
      if (m.phase === 'back') {
        if (!m.home) { this.endMotion(s, k); continue; }
        const d = m.home.clone().sub(m.cur);
        const dist = d.length();
        const step = Math.min(dist, 30 * dt);
        if (dist > 1e-3) m.cur.addScaledVector(d.divideScalar(dist), step);
        inst.pos.copy(m.cur);
        this.pose(inst);
        if (dist <= step + 0.05) this.endMotion(s, k);
        continue;
      }
      const step = Math.min(m.speed * dt, m.reach - m.travelled);
      const from = m.cur.clone();
      const next = from.clone().addScaledVector(m.dir, step);
      m.travelled += step;
      // what's in the way: world geometry stops it, enemies take the hit
      const R = this.w.R;
      const wall = step > 0.01 ? this.w.pw.castRay(new R.Ray(from, m.dir), step + c.radius * 0.35, true, undefined, groups(G.PLAYER, G.WORLD)) : null;
      let done = false;
      if (m.kind !== 'slam') {
        for (const tr of this.enemies(c)) {
          if (m.hit.has(tr.ref.id)) continue;
          if (segPointDist(from, next, tr.pos) <= c.radius * 0.75 + tr.radius + 0.2) {
            m.hit.add(tr.ref.id);
            this.hit(P, c, tr.ref, m.damage, m.dir, m.knock, m.kind === 'bash' || m.kind === 'shove' ? 'bash' : 'strike', tr.pos);
            if (m.kind === 'swing') done = true; // a committed blow stops on what it hits
          }
        }
      }
      if (wall && !done) {
        next.copy(from).addScaledVector(m.dir, Math.max(0, wall.timeOfImpact - c.radius * 0.35));
        done = true;
        if (m.kind === 'slam' || m.charge > 0.3) this.shockwave(P, c, next.clone(), m);
        else this.w.emit('smash', { construct: c.id, pos: next.toArray(), radius: 1.5 });
      }
      m.cur.copy(next);
      inst.pos.copy(next);
      inst.quat.copy(quatFromDir(m.dir));
      this.pose(inst);
      if (!done && m.travelled >= m.reach - 1e-4) {
        done = true;
        if (m.kind === 'slam') this.shockwave(P, c, inst.pos.clone(), m);
      }
      if (done) {
        if (m.home) m.phase = 'back';
        else this.endMotion(s, k);
      }
    }
  }

  private endMotion(s: PState, k: number) {
    const m = s.motions[k];
    s.motions.splice(k, 1);
    const g = this.ghosts.get(m.cid);
    if (g && !s.motions.some((x) => x.cid === m.cid)) g.until = this.w.time + 0.35; // let it clear the target first
  }

  private ghost(c: Construct) {
    let g = this.ghosts.get(c.id);
    if (!g) {
      g = { until: Infinity, groups: new Map() };
      for (const i of c.insts) for (const col of i.colliders) {
        const cg = col.collisionGroups();
        g.groups.set(col.handle, cg);
        col.setCollisionGroups(((cg >>> 16) << 16) | ((cg & 0xffff) & ~(G.ENTITY | G.CARGO | G.PLAYER)));
      }
      this.ghosts.set(c.id, g);
    }
    g.until = Infinity;
  }

  private unghost() {
    for (const [cid, g] of this.ghosts) {
      if (this.w.time < g.until) continue;
      this.ghosts.delete(cid);
      const c = this.rt.get(cid);
      if (!c) continue;
      for (const i of c.insts) for (const col of i.colliders) { const cg = g.groups.get(col.handle); if (cg !== undefined && i.alive) col.setCollisionGroups(cg); }
    }
  }

  /** Ground slam: everything hostile nearby is hit and thrown outward. */
  private shockwave(P: Player, c: Construct, at: Vector3, m: Motion) {
    const A = ACTIONS.strike;
    const r = A.slamRadius * (1 + m.charge) * clamp(this.massF(c), 0.8, 1.4);
    for (const tr of this.enemies(c)) {
      if (m.hit.has(tr.ref.id)) continue;
      const d = tr.pos.distanceTo(at) - tr.radius;
      if (d > r) continue;
      const k = 1 - 0.6 * clamp(d / r, 0, 1);
      const out = tr.pos.clone().sub(at).setY(0);
      if (out.lengthSq() < 1e-4) out.copy(m.dir).setY(0);
      out.normalize().add(new Vector3(0, 0.8, 0)).normalize();
      m.hit.add(tr.ref.id);
      this.hit(P, c, tr.ref, m.damage * k, out, m.knock * k, 'slam', tr.pos);
    }
    this.w.emit('smash', { construct: c.id, pos: at.toArray(), radius: r });
  }

  // ------------------------------------------------------------------ grab / throw

  private throwCargo(P: Player, s: PState, c: Construct, inst: Inst, charge: number) {
    const A = ACTIONS.grab;
    const e = inst.cargo ? this.w.entities.get(inst.cargo.entityId) : null;
    if (!e) return;
    if (!this.spend(P, c, ACTIONS.cost.throw * (1 + charge), 'grab')) return;
    const from = this.w.entityPos(e);
    const speed = A.throwSpeed + (A.throwSpeedCharged - A.throwSpeed) * charge;
    const lock = this.lockOn(P, 'enemy', 120, c, e.id);
    // thrown at a spot, it should come down ON it (its centre one radius above the surface)
    const aimPt = lock ? interceptPoint(from, speed, lock.pos, lock.vel, new Vector3()) : this.aimPoint(P, c).add(new Vector3(0, e.radius, 0));
    const v = ballistic(from, aimPt, speed, this.w.pw.gravity.y);
    this.rt.releaseEntity(e.id, v);
    inst.grab = null;
    this.w.credit(e, c.id, 5);
    s.flights.push({ cid: c.id, kind: 'entity', id: e.id, inst: 0, until: this.w.time + 3, damage: A.throwDamage * (1 + charge) * clamp(Math.cbrt(e.body.mass() / 60), 0.6, 2), hit: new Set([e.id]), prev: from.clone(), start: this.w.time });
    s.busyUntil = this.w.time + 0.3;
    this.w.emit('construct_throw', { id: c.id, speed: Math.round(speed) });
    this.start(P, c, 'grab', 'throw', charge);
    this.w.emit('action_release', { construct: c.id, player: playerId(P), kind: 'grab', action: 'throw', level: charge });
  }

  private slamCargo(P: Player, s: PState, c: Construct, inst: Inst) {
    const e = inst.cargo ? this.w.entities.get(inst.cargo.entityId) : null;
    if (!e || !this.spend(P, c, ACTIONS.cost.slam, 'grab')) return;
    const f = P.forward(new Vector3());
    this.rt.releaseEntity(e.id, new Vector3(f.x * 4, -34, f.z * 4));
    inst.grab = null;
    const pos = this.w.entityPos(e);
    const ref: TargetRef = { kind: 'entity', id: e.id };
    // slamming something that fights back hurts it; a crate is just smashed into the floor
    this.w.damageEntity(e, ACTIONS.grab.slamDamage * this.massF(c), 'smash', c.id);
    this.w.emit('smash', { construct: c.id, pos: pos.toArray(), radius: 2 });
    this.w.emit('action_hit', { construct: c.id, player: playerId(P), target: ref.id, targetKind: 'entity', energy: Math.round(ACTIONS.grab.slamDamage * this.massF(c)), kind: 'slam', pos: pos.toArray() });
    s.busyUntil = this.w.time + 0.35;
    this.start(P, c, 'grab', 'slam');
  }

  // ------------------------------------------------------------------ hurl (universal) / pull back

  /** Anything can be thrown: it becomes a physical projectile toward the lock (intercept) or the crosshair. */
  private hurl(P: Player, s: PState, c: Construct, charge: number) {
    const A = ACTIONS.hurl;
    if (this.w.time < s.busyUntil) return;
    if (!this.spend(P, c, ACTIONS.cost.hurl * (1 + charge), 'hurl')) return;
    const inst = this.nearestInst(c, P);
    if (!inst) return;
    const speed = A.speed * (1 + 0.4 * charge);
    // telekinetic lift: something lying on the ground is raised clear of it first (so it flies instead of sliding)
    for (const it of c.insts) {
      if (!it.alive) continue;
      const g = this.groundY(it.pos, it);
      const want = g === null ? -Infinity : g + Math.max(0.6, c.radius * 0.6) + 0.4;
      if (it.pos.y < want) { it.pos.y = want; it.body.setTranslation(it.pos, true); }
    }
    const lock = this.lockOn(P, 'enemy', 150, c);
    const aimPt = lock ? interceptPoint(inst.pos, speed, lock.pos, lock.vel, new Vector3()) : this.aimPoint(P, c);
    // aim at the centre of mass: the target point is where the construct's centre should arrive
    this.rt.throwAt(c.id, { kind: 'point', p: aimPt.toArray() as [number, number, number] }, speed);
    for (let i = 0; i < c.insts.length; i++) {
      const it = c.insts[i];
      if (!it.alive) continue;
      s.flights.push({ cid: c.id, kind: 'construct', id: c.id, inst: i, until: this.w.time + A.flight, damage: A.damage * this.massF(c) * (1 + charge), hit: new Set(), prev: it.pos.clone(), start: this.w.time });
    }
    c.expires = Math.max(c.expires, this.w.time + 8);
    s.busyUntil = this.w.time + 0.5;
    this.start(P, c, 'hurl', 'hurl', charge);
  }

  /** Bring a (thrown / placed) construct back and hold it in front of you. */
  private pullBack(P: Player, c: Construct) {
    if (has(c, 'hold') && c.kinematic) return;
    if (!this.spend(P, c, ACTIONS.cost.pull, 'hurl')) return;
    const R = this.w.R;
    for (const inst of c.insts) {
      if (!inst.alive) continue;
      inst.body.setBodyType(R.RigidBodyType.KinematicPositionBased, true);
      inst.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
      for (const col of inst.colliders) col.setCollisionGroups(groups(G.GHOST, ALL & ~G.CARGO));
    }
    c.kinematic = true;
    c.spec.motion = 'kinematic';
    c.spec.passPlayer = true;
    c.spec.behaviors = c.spec.behaviors.filter((b) => !['hold', 'follow', 'orbit', 'crush', 'home', 'sweep'].includes(b.type));
    c.spec.behaviors.push({ type: 'hold', side: 'front' });
    this.start(P, c, 'hurl', 'pull');
  }

  private stepFlights(s: PState) {
    const t = this.w.time;
    for (let k = s.flights.length - 1; k >= 0; k--) {
      const f = s.flights[k];
      const c = this.rt.get(f.cid);
      let pos: Vector3 | null = null;
      let radius = 0.5;
      let vel = 0;
      if (f.kind === 'entity') {
        const e = this.w.entities.get(f.id);
        if (e && !e.heldBy) { pos = this.w.entityPos(e); radius = e.radius; vel = this.w.entityVel(e).length(); }
      } else {
        const inst = c?.insts[f.inst];
        if (c && inst?.alive && inst.body.isDynamic()) { pos = inst.pos.clone(); radius = c.radius * 0.8; vel = inst.body.linvel() ? Math.hypot(inst.body.linvel().x, inst.body.linvel().y, inst.body.linvel().z) : 0; }
      }
      if (!pos || !c || t > f.until || (t - f.start > 0.3 && vel < 3)) { s.flights.splice(k, 1); continue; }
      const owner = this.w.constructs.get(f.cid);
      for (const tr of this.enemies(c)) {
        if (f.hit.has(tr.ref.id) || tr.ref.id === f.id) continue;
        if (segPointDist(f.prev, pos, tr.pos) <= radius + tr.radius + 0.3) {
          f.hit.add(tr.ref.id);
          const dir = pos.clone().sub(f.prev);
          if (dir.lengthSq() < 1e-6) dir.set(0, 0, 1);
          dir.normalize();
          const P = this.ownerPlayer(owner ?? c);
          this.hit(P, c, tr.ref, f.damage * clamp(vel / 30, 0.5, 1.6), dir, clamp(vel * 0.4, 4, 20), f.kind === 'entity' ? 'throw' : 'hurl', tr.pos);
        }
      }
      f.prev.copy(pos);
    }
  }

  // ------------------------------------------------------------------ shields / fields held up

  private stepRaised(P: Player, s: PState) {
    const id = s.raised ?? s.aiming;
    const c = this.rt.get(id);
    if (!c) return;
    const inst = this.nearestInst(c, P);
    if (!inst || !inst.body.isKinematic() || s.motions.some((m) => m.cid === c.id)) return;
    const look = P.look(new Vector3());
    if (s.raised === c.id) {
      // raise it in front of you, facing where you aim (blocks; a mirror sends shots to the crosshair)
      const d = 1.4 + c.extent.size.z / 2 + 0.3;
      const want = P.pos.clone().add(new Vector3(0, 0.3, 0)).addScaledVector(look, d);
      inst.pos.lerp(want, 1 - Math.exp(-25 * this.w.dt));
      inst.quat.copy(quatFromDir(look));
      this.pose(inst);
      if (has(c, 'reflect')) this.redirectReflections(P, s, c);
    } else {
      // fields: point it where you aim
      const to = this.aimPoint(P, c).sub(inst.pos);
      if (to.lengthSq() > 1e-4) { inst.quat.copy(quatFromDir(to.normalize())); this.pose(inst); }
    }
  }

  private redirectReflections(P: Player, s: PState, c: Construct) {
    const aimPt = this.lockOn(P, 'enemy', 150, c)?.pos ?? this.aimPoint(P, c);
    for (const p of this.w.projectiles.values()) {
      if (!p.reflected || p.by !== c.id || s.redirected.has(p.id) || this.w.time - p.born > 0.1) continue;
      s.redirected.add(p.id);
      const t = p.body.translation();
      const v = p.body.linvel();
      const speed = Math.hypot(v.x, v.y, v.z);
      const d = aimPt.clone().sub(new Vector3(t.x, t.y, t.z)).normalize().multiplyScalar(speed);
      p.body.setLinvel(d, true);
      this.w.emit('action_release', { construct: c.id, player: playerId(P), kind: 'shield', action: 'reflect', level: 0 });
    }
    if (s.redirected.size > 256) s.redirected.clear();
  }

  // ------------------------------------------------------------------ launchers, tethers

  private onTop(c: Construct, inst: Inst, p: Vector3, r: number, feet: number): boolean {
    const top = inst.pos.y + c.extent.max.y;
    const horiz = Math.hypot(p.x - inst.pos.x, p.z - inst.pos.z);
    const h = p.y - feet - top;
    return horiz <= Math.max(c.extent.size.x, c.extent.size.z) / 2 + 0.6 + r * 0.5 && h > -1.2 && h < 1.8;
  }

  private launch(P: Player, s: PState, c: Construct) {
    const inst = this.nearestInst(c, P);
    const L = has(c, 'launch');
    if (!inst || !L) return;
    const aimPt = this.lockOn(P, 'enemy', 150, c)?.pos ?? this.aimPoint(P, c);
    const speed = Math.max(L.speed, ACTIONS.launch.speed);
    let any = false;
    const meOn = this.onTop(c, inst, P.pos, 0.4, 0.95);
    const riders: Entity[] = [];
    for (const e of this.w.entities.values()) if (!e.heldBy && !e.restrainedBy && e.type !== 'turret' && e.body.isDynamic() && this.onTop(c, inst, this.w.entityPos(e), e.radius, e.radius)) riders.push(e);
    if (!meOn && !riders.length) { this.fail(P, c, 'launch', 'nothing on it'); return; }
    if (!this.spend(P, c, ACTIONS.cost.launch, 'launch')) return;
    if (meOn) {
      if (P === this.w.player) this.rt.launchPlayerTo(aimPt.clone(), speed);
      else { P.vel.copy(ballistic(P.pos, aimPt, speed, this.w.pw.gravity.y)); P.onGround = false; }
      any = true;
    }
    for (const e of riders) {
      const v = ballistic(this.w.entityPos(e), aimPt, speed, this.w.pw.gravity.y);
      e.body.setLinvel(v, true);
      if (e.type === 'drone') e.stunnedUntil = this.w.time + 2;
      this.w.credit(e, c.id, 5);
      s.flights.push({ cid: c.id, kind: 'entity', id: e.id, inst: 0, until: this.w.time + 3, damage: ACTIONS.grab.throwDamage, hit: new Set([e.id]), prev: this.w.entityPos(e), start: this.w.time });
      any = true;
    }
    if (any) {
      s.busyUntil = this.w.time + 0.4;
      this.w.emit('launch', { construct: c.id, speed: Math.round(speed) });
      this.start(P, c, 'launch', meOn ? 'launch_self' : 'launch');
    }
  }

  /** RMB on a launcher: put it down where you aim (the ground under the crosshair, within reach). */
  private place(P: Player, c: Construct) {
    const inst = this.nearestInst(c, P);
    if (!inst) return;
    const a = P.aim(ACTIONS.launch.place);
    if (!a.collider || a.normal.y < 0.6 || this.w.ownerOf(a.collider)?.kind !== 'static') { this.fail(P, c, 'launch', 'aim at the ground'); return; }
    if (!this.spend(P, c, ACTIONS.cost.pull, 'launch')) return;
    inst.pos.copy(a.point).setY(a.point.y - c.extent.min.y + 0.02);
    const f = a.point.clone().sub(P.pos).setY(0);
    if (f.lengthSq() > 1e-4) inst.quat.setFromAxisAngle(UP, Math.atan2(f.x, f.z));
    if (inst.body.isKinematic()) this.pose(inst);
    else { inst.body.setTranslation(inst.pos, true); inst.body.setRotation(inst.quat, true); }
    this.start(P, c, 'launch', 'place');
  }

  private yank(P: Player, s: PState, c: Construct) {
    const tb = has(c, 'tether');
    if (!tb) return;
    if (!this.spend(P, c, ACTIONS.cost.yank, 'yank')) return;
    const e = this.rt.refEntities(tb.target)[0];
    if (e && e.body.isDynamic() && !e.heldBy) {
      const ep = this.w.entityPos(e);
      const v = P.pos.clone().sub(ep).normalize().multiplyScalar(ACTIONS.yank.speed).add(new Vector3(0, 5, 0));
      e.body.setLinvel(v, true);
      if (e.type === 'drone') e.stunnedUntil = this.w.time + 2;
      this.w.credit(e, c.id, 4);
      s.flights.push({ cid: c.id, kind: 'entity', id: e.id, inst: 0, until: this.w.time + 2, damage: ACTIONS.grab.throwDamage * 0.6, hit: new Set([e.id]), prev: ep, start: this.w.time });
    } else {
      // tethered to a point: a burst toward the anchor
      const inst = this.nearestInst(c, P);
      const tp = inst?.pos ?? null;
      if (tp) P.vel.addScaledVector(tp.clone().sub(P.pos).normalize(), ACTIONS.yank.speed * 0.75);
    }
    s.busyUntil = this.w.time + 0.4;
    this.start(P, c, 'yank', 'yank');
  }

  // ------------------------------------------------------------------ targeting

  /** Things this construct may hit: hostile creatures, practice dummies, and anything another lantern made. */
  private enemies(c: Construct): Lock[] {
    const out: Lock[] = [];
    for (const e of this.w.entities.values()) {
      if (e.disabled || e.heldBy === c.id || (!e.hostile && e.type !== 'dummy')) continue;
      out.push({ ref: { kind: 'entity', id: e.id }, pos: this.w.entityPos(e), vel: this.w.entityVel(e), radius: e.radius });
    }
    // rival lanterns (PvP): alive, not spawn-protected targets still count (the damage path handles the shield)
    for (const pl of this.w.players ?? []) {
      if (playerId(pl) === c.owner || !pl.alive) continue;
      out.push({ ref: { kind: 'player', id: playerId(pl) }, pos: pl.pos.clone(), vel: pl.vel.clone(), radius: 0.55 });
    }
    for (const o of this.rt.list.values()) {
      if (o.owner === c.owner || o.id === c.id) continue;
      const i = o.insts.find((x) => x.alive);
      if (!i) continue;
      out.push({ ref: { kind: 'construct', id: o.id }, pos: o.extent.center.clone().applyQuaternion(i.quat).add(i.pos), vel: i.vel.clone(), radius: Math.max(0.5, o.radius * 0.6) });
    }
    return out;
  }

  /**
   * Lock-on (docs/PVP_DESIGN.md): whatever is under the crosshair, else the candidate closest to the aim ray
   * within a ~15° cone or 6 m of it. 'grab' also takes loose objects (crates, civilians, debris-free entities).
   */
  lockOn(P: Player, mode: 'enemy' | 'grab', range: number, c: Construct, exclude?: string): Lock | null {
    const cam = P.camera();
    const aim = P.aim(range + 20);
    const own = this.w.ownerOf(aim.collider);
    const cands: Lock[] = mode === 'enemy' ? this.enemies(c) : [];
    if (mode === 'grab') {
      for (const e of this.w.entities.values()) {
        if (e.disabled || e.heldBy || e.type === 'turret' || e.id === exclude) continue;
        cands.push({ ref: { kind: 'entity', id: e.id }, pos: this.w.entityPos(e), vel: this.w.entityVel(e), radius: e.radius });
      }
    }
    if (own && (own.kind === 'entity' || own.kind === 'construct' || own.kind === 'player')) {
      const direct = cands.find((x) => x.ref.kind === own.kind && x.ref.id === own.id && x.ref.id !== exclude);
      if (direct && direct.pos.distanceTo(P.pos) <= range + direct.radius) return direct;
    }
    let best: Lock | null = null, bs = Infinity;
    const cone = Math.tan((ACTIONS.lockCone * Math.PI) / 180);
    for (const x of cands) {
      if (x.ref.id === exclude) continue;
      const d = x.pos.distanceTo(P.pos);
      if (d > range + x.radius) continue;
      const to = _v.copy(x.pos).sub(cam.pos);
      const along = to.dot(cam.dir);
      if (along <= 1) continue;
      const perp = _v2.copy(to).addScaledVector(cam.dir, -along).length();
      if (perp / along > cone && perp - x.radius > ACTIONS.lockRay) continue;
      const score = perp / along;
      if (score < bs) { bs = score; best = x; }
    }
    return best;
  }

  private nearestEnemyTo(c: Construct, p: Vector3, r: number): Lock | null {
    let best: Lock | null = null, bd = r;
    for (const x of this.enemies(c)) { const d = x.pos.distanceTo(p) - x.radius; if (d < bd) { bd = d; best = x; } }
    return best;
  }

  private fireAim(P: Player, c: Construct, precise: boolean): Vector3 {
    if (!precise) {
      const lock = this.lockOn(P, 'enemy', 150, c);
      if (lock) {
        const wp = c.mach?.weapons.find((w) => w.kind !== 'melee');
        const sh = has(c, 'shoot');
        const speed = wp ? (wp.kind === 'laser' || wp.kind === 'flame' ? Infinity : Math.max(10, wp.speed || 50)) : sh?.speed ?? 50;
        const from = c.insts[0]?.pos ?? P.pos;
        return isFinite(speed) ? interceptPoint(from, speed, lock.pos, lock.vel, new Vector3()) : lock.pos.clone();
      }
    }
    return this.aimPoint(P, c);
  }

  /** The crosshair point, looking past the construct itself (a tank in front of you doesn't aim at itself). */
  aimPoint(P: Player, c: Construct, max = 250): Vector3 {
    const a = P.aim(max);
    const o = this.w.ownerOf(a.collider);
    if (!(o?.kind === 'construct' && o.id === c.id)) return a.point.clone();
    const R = this.w.R;
    const cam = P.camera();
    const origin = cam.pos.clone().addScaledVector(cam.dir, Math.max(0, _v.copy(P.pos).sub(cam.pos).dot(cam.dir) - 0.2));
    const hit = this.w.pw.castRay(new R.Ray(origin, cam.dir), max, true, undefined, groups(G.PLAYER, G.WORLD | G.CONSTRUCT | G.ENTITY | G.CARGO), P.collider, undefined,
      (col) => this.rt.byCollider.get(col.handle) !== c.id);
    return origin.addScaledVector(cam.dir, hit ? hit.timeOfImpact : max);
  }

  private groundY(p: Vector3, inst: Inst): number | null {
    const R = this.w.R;
    const hit = this.w.pw.castRay(new R.Ray({ x: p.x, y: p.y + 0.5, z: p.z }, { x: 0, y: -1, z: 0 }), 60, true, undefined, groups(G.PLAYER, G.WORLD), undefined, inst.body);
    return hit ? p.y + 0.5 - hit.timeOfImpact : null;
  }

  // ------------------------------------------------------------------ hits (the only damage path of actions)

  /** Deal an action's hit, attributed to the construct. Never the owner, never the owner's constructs. */
  private hit(P: Player, c: Construct, t: TargetRef, damage: number, dir: Vector3, knock: number, kind: string, pos: Vector3) {
    if (t.kind === 'player') {
      const pl = this.w.playerById(t.id);
      if (!pl || t.id === c.owner || pl.heldBy === c.id) return;
      this.w.damagePlayer(pl, damage, kind === 'hurl' || kind === 'throw' ? 'construct' : 'strike', c.id, { dir: dir.clone(), knock, pos: pos.clone() });
      this.w.emit('impact', { construct: c.id, player: t.id, dv: Math.round(knock), pos: pos.toArray() });
    } else if (t.kind === 'entity') {
      const e = this.w.entities.get(t.id);
      if (!e || e.heldBy === c.id) return;
      this.w.damageEntity(e, damage, kind === 'hurl' || kind === 'throw' ? 'construct' : 'strike', c.id);
      if (e.body.isDynamic() && knock > 0) {
        const imp = dir.clone().setY(Math.max(dir.y, 0) + 0.25).normalize().multiplyScalar(knock * e.body.mass());
        e.body.applyImpulse(imp, true);
      }
      if (e.type === 'drone') e.stunnedUntil = Math.max(e.stunnedUntil, this.w.time + 0.8);
      this.w.emit('impact', { construct: c.id, entity: e.id, dv: Math.round(knock), pos: pos.toArray() });
    } else {
      const o = this.rt.get(t.id);
      if (!o || o.owner === c.owner) return;
      this.rt.damageConstruct(o.id, damage, kind, c.id);
      this.w.emit('impact', { construct: c.id, target: o.id, dv: Math.round(knock), pos: pos.toArray() });
    }
    c.impacts++;
    this.w.emit('action_hit', { construct: c.id, player: playerId(P), target: t.id, targetKind: t.kind, energy: Math.round(damage), kind, pos: pos.toArray() });
  }

  // ------------------------------------------------------------------ helpers

  private start(P: Player, c: Construct, kind: ActionKind, action: string, charge = 0) {
    this.w.emit('action_start', { construct: c.id, player: playerId(P), kind, action, charge: Math.round(charge * 100) / 100 });
  }

  private fail(P: Player, c: Construct, kind: ActionKind, reason: string) {
    this.w.emit('action_fail', { construct: c.id, player: playerId(P), kind, reason });
  }

  /** Heavier things hit harder (and recover slower): 0.6 (a sword) … 1.8 (a wrecking ball, a tank). */
  massF(c: Construct) { return clamp(Math.cbrt(Math.max(1, c.mass) / 400), 0.6, 1.8); }
  private costScale(c: Construct) { return clamp(0.6 + c.cost / 40, 0.6, 2); }

  /** Willpower from the acting lantern's own pool. */
  private spend(P: Player, c: Construct, amount: number, kind: ActionKind, silent = false): boolean {
    const cost = silent ? amount : amount * this.costScale(c);
    if (P.energy < cost) { if (!silent) this.fail(P, c, kind, 'willpower'); return false; }
    P.energy -= cost;
    return true;
  }

  private nearestInst(c: Construct, P: Player): Inst | null {
    let best: Inst | null = null, bd = Infinity;
    for (const i of c.insts) { if (!i.alive) continue; const d = i.pos.distanceToSquared(P.pos); if (d < bd) { bd = d; best = i; } }
    return best;
  }

  /** Placed (static) things become movable when you pick them up with the mouse. */
  private ensureKinematic(c: Construct) {
    if (c.kinematic && c.insts.every((i) => !i.alive || i.body.isKinematic())) return;
    if (c.insts.some((i) => i.alive && i.body.isDynamic()) && !has(c, 'hold')) return; // free physical things stay physical
    const R = this.w.R;
    for (const i of c.insts) if (i.alive && !i.body.isKinematic()) i.body.setBodyType(R.RigidBodyType.KinematicPositionBased, true);
    c.kinematic = true;
    if (c.spec.motion === 'static') c.spec.motion = 'kinematic';
  }

  private pose(inst: Inst) {
    inst.body.setNextKinematicTranslation(inst.pos);
    inst.body.setNextKinematicRotation(inst.quat);
  }

  private ownerPlayer(_c: Construct): Player {
    const ps = (this.w as unknown as { players?: Player[] }).players;
    return ps?.find((p) => playerId(p) === _c.owner) ?? this.w.player;
  }

  // ------------------------------------------------------------------ HUD view

  view(P: Player): ActionView {
    const id = playerId(P);
    const s = this.state(id);
    const list = this.usable(id);
    const eq = P.riding ? this.rt.get(P.riding.id) : this.rt.get(s.equipped);
    const out: ActionView = { equipped: null, list: list.map((c) => ({ id: c.id, label: c.spec.label, kind: classify(c, P), equipped: c.id === eq?.id })), charge: this.chargeLevel(s), precise: !!s.precise, lock: null, preview: null };
    if (!eq) return out;
    const kind = classify(eq, P);
    const inst = this.nearestInst(eq, P);
    const holding = !!inst?.cargo;
    out.equipped = { id: eq.id, label: eq.spec.label, kind, ...labels(eq, kind, holding) };
    if (!inst) return out;
    const g = this.w.pw.gravity.y;
    const arc = (from: Vector3, to: Vector3, speed: number) => {
      const v = ballistic(from, to, speed, g);
      const T = to.distanceTo(from) / Math.max(1, speed);
      const pts: Vector3[] = [];
      const n = clamp(Math.ceil(T * 12), 4, 40);
      for (let k = 0; k <= n; k++) { const tt = (T * k) / n; pts.push(from.clone().addScaledVector(v, tt).add(new Vector3(0, 0.5 * g * tt * tt, 0))); }
      return pts;
    };
    const lockRange = kind === 'strike' ? ACTIONS.strike.reach : kind === 'grab' && !holding ? ACTIONS.grab.reach : 150;
    const lock = kind === 'ride' || kind === 'field' || kind === 'yank' ? null : this.lockOn(P, kind === 'grab' && !holding ? 'grab' : 'enemy', lockRange, eq, inst.cargo?.entityId);
    out.lock = lock?.pos.clone() ?? null;
    const aimPt = lock?.pos.clone() ?? this.aimPoint(P, eq);
    switch (kind) {
      case 'strike': {
        if (s.charging && this.wantsSlam(P)) {
          const a = P.aim(60).point;
          out.preview = { kind: 'line', points: [inst.pos.clone(), a], radius: ACTIONS.strike.slamRadius * (1 + out.charge) * clamp(this.massF(eq), 0.8, 1.4) };
        } else {
          const to = aimPt.clone();
          const d = to.clone().sub(inst.pos);
          if (d.length() > ACTIONS.strike.reach) to.copy(inst.pos).addScaledVector(d.normalize(), ACTIONS.strike.reach);
          out.preview = { kind: 'line', points: [inst.pos.clone(), to] };
        }
        break;
      }
      case 'grab':
        if (holding) {
          const e = this.w.entities.get(inst.cargo!.entityId);
          if (e) out.preview = { kind: 'arc', points: arc(this.w.entityPos(e), aimPt, ACTIONS.grab.throwSpeed + (ACTIONS.grab.throwSpeedCharged - ACTIONS.grab.throwSpeed) * out.charge) };
        } else if (lock) out.preview = { kind: 'line', points: [inst.pos.clone(), lock.pos.clone()] };
        break;
      case 'hurl': case 'detonate':
        if (kind === 'hurl' || !has(eq, 'hold')) out.preview = { kind: 'arc', points: arc(inst.pos.clone(), aimPt, ACTIONS.hurl.speed) };
        break;
      case 'launch': {
        const top = inst.pos.clone().setY(inst.pos.y + eq.extent.max.y + 1);
        out.preview = { kind: 'arc', points: arc(top, aimPt, ACTIONS.launch.speed) };
        break;
      }
      case 'fire': out.preview = { kind: 'line', points: [inst.pos.clone(), s.precise ? this.aimPoint(P, eq) : aimPt] }; break;
      case 'command': out.preview = { kind: 'line', points: [aimPt.clone(), aimPt.clone()] }; break;
      default: break;
    }
    return out;
  }
}

function segPointDist(a: Vector3, b: Vector3, p: Vector3) {
  const ab = _v.copy(b).sub(a);
  const l2 = ab.lengthSq();
  const t = l2 > 1e-9 ? clamp(_v2.copy(p).sub(a).dot(ab) / l2, 0, 1) : 0;
  return _v2.copy(a).addScaledVector(ab, t).distanceTo(p);
}

