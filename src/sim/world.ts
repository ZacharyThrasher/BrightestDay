import { Vector3, Quaternion } from 'three';
import { initPhysics, G, ALL, groups, PLAYER_BITS, type Rapier, type RWorld, type RBody, type RCollider } from './physics.js';
import { Player, type PlayerInput, NO_INPUT } from './player.js';
import { ConstructRuntime } from './constructs/runtime.js';
import { WorldContent, type EntityAI } from './content.js';
import { CreativityEconomy } from './inspiration.js';
import { Match, type MatchRules, type MatchSnapshot } from './match.js';
import type { BotController } from './bots.js';
import { ActionSystem } from './constructs/actions.js';

export type Owner =
  | { kind: 'static'; id: string }
  | { kind: 'player'; id: string }
  | { kind: 'construct'; id: string; part: number }
  | { kind: 'entity'; id: string }
  | { kind: 'projectile'; id: number };

export interface StaticGeom {
  id: string;
  shape: 'box' | 'cylinder' | 'wedge';
  pos: Vector3;
  size: Vector3; // full extents (box/wedge), (r, h, r) for cylinder
  rot: Quaternion;
  color: number;
  kind: 'ground' | 'building' | 'rock' | 'goal' | 'wall' | 'ring' | 'prop';
}

export type EntityType = 'drone' | 'civilian' | 'boulder' | 'turret' | 'crate' | 'dummy' | 'robot' | 'block';

/** Optional per-entity setup (all fields optional; defaults keep the classic entities unchanged). */
export interface EntityOpts {
  hostile?: boolean;
  label?: string;
  vel?: Vector3;
  /** 'heavy' drones are bigger, armoured and hit harder. */
  variant?: 'standard' | 'heavy';
  /** blocks: full extents */
  size?: Vector3;
  /** blocks: base colour */
  color?: number;
  /** blocks: the stack / wall they belong to (for topple tallies) */
  group?: string;
  hp?: number;
  /** blocks: density override (kg/m³) */
  density?: number;
}

export interface Entity {
  id: string;
  type: EntityType;
  body: RBody;
  collider: RCollider;
  radius: number;
  hp: number;
  maxHp: number;
  hostile: boolean;
  home: Vector3;
  heldBy: string | null; // construct id
  restrainedBy: string | null;
  stunnedUntil: number;
  disabled: boolean;
  hurt: boolean; // civilians: took a dangerous impact
  safe: boolean; // civilians: came to rest without hurt
  fireCooldown: number;
  lastVel: Vector3;
  label: string;
  variant: 'standard' | 'heavy';
  /** blocks: full extents */
  size: Vector3 | null;
  color: number | null;
  /** blocks: stack / wall id */
  group: string | null;
  /** crates and blocks break into debris at 0 hp */
  breakable: boolean;
  /** construct credited with what happens to this entity next (held it, hit it, threw it, knocked something into it) */
  credit: string | null;
  creditUntil: number;
  disabledAt: number;
  /** behaviour state of robots, bystander civilians and blocks (see content.ts) */
  ai: EntityAI | null;
}

export interface Projectile {
  id: number;
  body: RBody;
  collider: RCollider;
  from: string | null; // entity id
  /** construct responsible (fired it or reflected it) */
  by: string | null;
  reflected: boolean;
  born: number;
  damage: number;
  /** Fired by a construct: never hits its owner (player) or the owner's constructs; damage is attributed to it. */
  friendly?: boolean;
  owner?: string | null; // construct id
  ownerPlayer?: string | null; // player id that owns the firing construct
  splash?: number; // blast radius on impact (cannon shells, bombs)
  kind?: string; // weapon kind (gun, cannon, missile, bomb, net)
}

export interface ProjectileOpts { friendly?: boolean; owner?: string | null; ownerPlayer?: string | null; splash?: number; kind?: string; gravity?: number }

export interface Goal { id: string; min: Vector3; max: Vector3; label: string; reached: boolean }

export interface GameEvent { t: number; type: string; [k: string]: any }

export interface RaceRing { id: string; pos: Vector3; normal: Vector3; radius: number; passed: boolean }

/** Arena spawn point (respawns pick the one farthest from the opponents). */
export interface SpawnPoint { pos: Vector3; yaw: number }

/** Plain, serializable match state (players, constructs, rules) — what the netcode ships and hashes. */
export interface MatchStateSnapshot {
  tick: number;
  time: number;
  match: MatchSnapshot | null;
  players: { id: string; pos: number[]; vel: number[]; yaw: number; pitch: number; hp: number; alive: boolean; respawnAt: number | null; protectedUntil: number; energy: number; gas: number; riding: string | null; heldBy: string | null; inspiration: number }[];
  constructs: { id: string; owner: string; label: string; hp: number; insts: number[][] }[];
  entities: { id: string; pos: number[]; hp: number }[];
  projectiles: number;
}

/** Options for damagePlayer: knockback direction (normalised inside) and strength (1 = a solid hit). */
export interface HitOpts { dir?: Vector3; knock?: number; pos?: Vector3 }

export class World {
  R!: Rapier;
  pw!: RWorld;
  eq!: InstanceType<Rapier['EventQueue']>;
  time = 0;
  tick = 0;
  readonly dt = 1 / 60;
  owners = new Map<number, Owner>();
  /** The LOCAL lantern (alias of the player whose id is `localId`; single-player code keeps using it). */
  player!: Player;
  /** Every lantern in this world (index 0 = 'p1'). */
  players: Player[] = [];
  /** Id of the player this machine controls (world.player). */
  localId = 'p1';
  /** Arena spawn points (duel: point-symmetric pairs); empty = respawn where the scene spawned the player. */
  spawns: SpawnPoint[] = [];
  /** Duel rules, when a match is running (null = free play). */
  match: Match | null = null;
  /** Stand-ins for a rival in free practice (removed when a real opponent joins). */
  practiceDummies: string[] = [];
  /** Scripted / AI lanterns: asked for their player's input at the start of every tick. */
  controllers = new Map<string, BotController>();
  statics: StaticGeom[] = [];
  entities = new Map<string, Entity>();
  projectiles = new Map<number, Projectile>();
  goals: Goal[] = [];
  rings: RaceRing[] = [];
  constructs!: ConstructRuntime;
  /** robots, destructibles, bystanders, debris, waves, race timer */
  content!: WorldContent;
  /** Creativity economy (novelty → power), per player: `inspiration.of(playerId)`; see inspiration.ts. */
  inspiration!: CreativityEconomy;
  /** Adaptive construct controls (mouse actions of the equipped construct), per player; see constructs/actions.ts. */
  actions!: ActionSystem;
  log: GameEvent[] = [];
  scene = 'empty';
  input: PlayerInput = NO_INPUT;
  private nextId = 1;
  private lastProjectile: Projectile | null = null;
  stats = { projectilesFired: 0, projectilesBlocked: 0, projectilesReflected: 0, playerHits: 0, entityDamage: 0 };
  /** Called for every event (renderer / audio / HUD subscribe). */
  listeners: ((e: GameEvent) => void)[] = [];
  /** Called at the end of every tick (the interpreter uses it for multi-step actions like grab-then-throw). */
  tickHooks: (() => void)[] = [];

  static async create(): Promise<World> {
    const w = new World();
    w.R = await initPhysics();
    w.pw = new w.R.World({ x: 0, y: -22, z: 0 });
    w.pw.timestep = w.dt;
    w.eq = new w.R.EventQueue(true);
    w.player = w.addPlayer('p1');
    w.constructs = new ConstructRuntime(w);
    w.content = new WorldContent(w);
    w.inspiration = new CreativityEconomy(w);
    w.listeners.push((e) => w.match?.onEvent(e));
    w.actions = new ActionSystem(w);
    return w;
  }

  // ---------------- players (PvP) ----------------

  /** Add a lantern (the first one is the local player 'p1'). Spawns at the scene's next free spawn point. */
  addPlayer(id = `p${this.players.length + 1}`, name?: string): Player {
    const have = this.playerById(id);
    if (have) return have;
    const p = new Player(this, id, this.players.length);
    if (name) p.name = name;
    p.onHit = (c, n, pl) => this.constructs.onPlayerTouch(c, n, pl);
    this.players.push(p);
    if (this.players.length > 1) {
      for (const d of this.practiceDummies.splice(0)) this.removeEntity(d);
      const sp = this.spawns[this.players.length - 1] ?? this.farthestSpawn(p);
      if (sp) { p.teleport(sp.pos); p.yaw = sp.yaw; }
      else p.teleport(this.player.pos.clone().add(new Vector3(0, 0, -10 * (this.players.length - 1)))); // not inside the local lantern
      this.refreshQueries();
    }
    this.emit('player_joined', { id, name: p.name });
    return p;
  }

  playerById(id: string | null | undefined): Player | null {
    if (!id) return null;
    for (const p of this.players) if (p.id === id) return p;
    return null;
  }

  /** The other lanterns (alive or not), nearest first. */
  opponentsOf(id: string): Player[] {
    const me = this.playerById(id);
    const out = this.players.filter((p) => p.id !== id);
    if (me) out.sort((a, b) => a.pos.distanceToSquared(me.pos) - b.pos.distanceToSquared(me.pos));
    return out;
  }

  /** Nearest living opponent of a player (null if none). */
  opponentOf(id: string): Player | null { return this.opponentsOf(id).find((p) => p.alive) ?? null; }

  /** Set a non-local player's input for the next tick(s) (netcode / scripted tests). */
  setInput(id: string, input: PlayerInput) { const p = this.playerById(id); if (p) p.input = input; }

  /** Which player a construct / projectile / player id belongs to (for attribution). */
  attackerOf(by: string | null | undefined): string | null {
    if (!by) return null;
    if (this.playerById(by)) return by;
    const c = this.constructs?.get(by);
    if (c) return c.owner;
    return this.inspiration?.ownerOf(by) ?? null;
  }

  /** Collision bits of every player except `exceptId` (projectile / beam filters that must skip their owner). */
  playerMask(exceptId?: string | null): number {
    let m = 0;
    for (const p of this.players) if (p.id !== exceptId) m |= p.bit;
    return m & PLAYER_BITS;
  }

  /** Start a duel between the given players (default: everyone). Everyone respawns; constructs dissolve. */
  startMatch(rules: Partial<MatchRules> = {}, players = this.players.map((p) => p.id)): Match {
    this.match = null;
    this.resetForRound();
    this.match = new Match(this, players, rules);
    return this.match;
  }

  /** Round (re)start: dissolve every construct, clear projectiles, respawn everyone at their spawn. */
  resetForRound() {
    this.constructs.dismissAll();
    for (const p of [...this.projectiles.values()]) this.removeProjectile(p);
    this.players.forEach((p, i) => {
      const sp = this.spawns.length ? this.spawns[i % this.spawns.length] : null;
      this.respawn(p, sp ?? { pos: p.pos.clone(), yaw: p.yaw }, 0);
    });
    this.refreshQueries();
  }

  /** Can a player be killed (duel / multiplayer)? Single-player scenes keep the old "hp floors at 0" rule. */
  get deathEnabled() { return !!this.match || this.players.length > 1; }

  /**
   * THE player damage path (PVP_DESIGN: one attributed `damagePlayer(target, amount, cause, by)`).
   * `by` = construct id, player id or entity id. A player's own constructs never hurt them; the spawn shield,
   * death and a non-live match phase block damage. Hits knock the victim back (scaled by damage), flash them
   * red and show the attacker a hit marker + number (events). Returns the damage dealt.
   */
  damagePlayer(target: Player, amount: number, cause: string, by?: string | null, opts: HitOpts = {}): number {
    if (!target || !(amount > 0) || !target.alive) return 0;
    const attacker = this.attackerOf(by);
    if (attacker === target.id) return 0; // own constructs never hurt their owner
    if (target.isProtected) { this.emit('player_shielded', { target: target.id, by: by ?? null, attacker }); return 0; }
    if (this.match && !this.match.damageEnabled) return 0;
    const dealt = Math.min(amount, target.hp);
    const pos = opts.pos ?? target.pos;
    if (opts.dir && opts.dir.lengthSq() > 1e-8) target.hurtDir.copy(opts.dir).normalize();
    else target.hurtDir.set(0, 0, 0);
    target.hurtAt = this.time;
    target.hp = Math.max(0, target.hp - amount);
    this.stats.playerHits++;
    const weapon = this.weaponOf(by, cause);
    this.emit('player_damage', { target: target.id, amount: Math.round(amount * 10) / 10, cause, by: by ?? null, attacker, weapon, hp: Math.round(target.hp), pos: [pos.x, pos.y, pos.z] });
    // knockback: a velocity impulse scaled by the damage (no hit-stun: you keep full control)
    // (only lanterns' constructs knock you around; PvE hazards keep their own pushes)
    const knock = attacker ? opts.knock ?? 1 : 0;
    if (knock > 0 && target.hurtDir.lengthSq() > 0 && !target.riding && !target.heldBy) {
      const k = Math.min(24, 2.5 + amount * 0.45) * knock;
      const d = target.hurtDir.clone();
      d.y = Math.max(d.y, 0.25);
      d.normalize();
      target.vel.addScaledVector(d, k);
      target.onGround = false;
      target.launchGrace = Math.max(target.launchGrace, 0.35);
      if (k > 12 && by) { target.flungBy = by; target.flungUntil = this.time + 1.2; }
    }
    if (target.hp <= 0 && this.deathEnabled) this.killPlayer(target, by ?? null, cause);
    return dealt;
  }

  /** What to call the thing that did it (kill feed): the construct's label, else the cause. */
  weaponOf(by: string | null | undefined, cause: string): string {
    if (by) {
      const c = this.constructs?.get(by);
      if (c) return c.spec.label;
      const e = this.entities.get(by);
      if (e) return e.label;
      const r = this.inspiration?.labelOf(by);
      if (r) return r;
    }
    return cause;
  }

  killPlayer(target: Player, by: string | null, cause: string) {
    if (!target.alive) return;
    const attacker = this.attackerOf(by);
    const weapon = this.weaponOf(by, cause);
    if (target.riding) this.constructs.machines.dismount(target);
    if (target.heldBy) this.constructs.releasePlayer(target, new Vector3());
    target.alive = false;
    target.hp = 0;
    target.diedAt = this.time;
    target.respawnAt = this.time + (this.match?.rules.respawnDelay ?? 3);
    target.flungBy = null;
    target.releaseTether('L');
    target.releaseTether('R');
    target.setSolid(false);
    // a limp tumble away from the killing blow
    target.vel.addScaledVector(target.hurtDir, 6).add(new Vector3(0, 5, 0));
    target.onGround = false;
    this.emit('player_killed', { target: target.id, by, attacker, cause, weapon, pos: target.pos.toArray() });
    // a fallen lantern's constructs dissolve with them
    for (const c of [...this.constructs.list.values()]) if (c.owner === target.id) this.constructs.dismiss(c.id, 'owner_died');
  }

  /** Spawn point farthest from the player's living opponents (null if the arena has none). */
  farthestSpawn(p: Player): SpawnPoint | null {
    if (!this.spawns.length) return null;
    const foes = this.players.filter((o) => o !== p && o.alive);
    if (!foes.length) return this.spawns[p.index % this.spawns.length];
    let best = this.spawns[0], bd = -1;
    for (const s of this.spawns) {
      let d = Infinity;
      for (const f of foes) d = Math.min(d, f.pos.distanceTo(s.pos));
      if (d > bd + 1e-6) { bd = d; best = s; }
    }
    return best;
  }

  /** Bring a player back: full health and willpower, spawn shield for `protect` seconds. */
  respawn(p: Player, at: SpawnPoint | null = this.farthestSpawn(p), protect = this.match?.rules.spawnProtection ?? 2) {
    if (p.riding) this.constructs.machines.dismount(p);
    if (p.heldBy) this.constructs.releasePlayer(p, new Vector3());
    const sp = at ?? { pos: p.pos.clone(), yaw: p.yaw };
    p.flungBy = null;
    p.flying = false;
    p.teleport(sp.pos);
    p.yaw = sp.yaw;
    p.pitch = -0.05;
    p.hp = p.maxHp;
    p.gas = p.tune.gasMax;
    p.energy = this.constructs?.energyMaxOf(p.id) ?? p.energy;
    p.alive = true;
    p.respawnAt = Infinity;
    p.protectedUntil = this.time + protect;
    p.hurtAt = -Infinity;
    p.setSolid(true);
    this.emit('player_respawn', { id: p.id, pos: sp.pos.toArray(), yaw: sp.yaw, protectedUntil: p.protectedUntil });
  }

  /**
   * Lock-on (PVP_DESIGN "Speed vs. hitting" §1): the opponent a cast from this player would lock onto —
   * within ~15° of the crosshair or within 6 m of the aim ray, in line of sight. Null when nobody qualifies.
   */
  lockTarget(playerId: string, coneDeg = 15, rayDist = 6, maxRange = 160): Player | null {
    const me = this.playerById(playerId);
    if (!me || !me.alive || this.players.length < 2) return null;
    const cam = me.camera();
    const cos = Math.cos((coneDeg * Math.PI) / 180);
    let best: Player | null = null, bs = Infinity;
    const to = new Vector3();
    for (const o of this.players) {
      if (o === me || !o.alive) continue;
      to.copy(o.pos).sub(cam.pos);
      const along = to.dot(cam.dir);
      if (along <= 0) continue;
      const dist = to.length();
      if (dist > maxRange) continue;
      const perp = Math.sqrt(Math.max(0, dist * dist - along * along));
      const inCone = along / Math.max(1e-6, dist) >= cos;
      if (!inCone && perp > rayDist) continue;
      // line of sight from the camera (level geometry only)
      const hit = this.pw.castRay(new this.R.Ray(cam.pos, to.clone().divideScalar(Math.max(1e-6, dist))), Math.max(0, dist - 0.8), true, undefined, groups(G.PLAYER, G.WORLD));
      if (hit) continue;
      const score = perp / Math.max(1, dist) + (inCone ? 0 : 1);
      if (score < bs) { bs = score; best = o; }
    }
    return best;
  }

  // ---------------- serializable match state (netcode) ----------------

  /** Everything a duel's outcome depends on, as plain data (rounded for stable hashing). */
  snapshotMatch(): MatchStateSnapshot {
    const r = (v: number) => Math.round(v * 1000) / 1000;
    const v3 = (v: { x: number; y: number; z: number }) => [r(v.x), r(v.y), r(v.z)];
    return {
      tick: this.tick,
      time: r(this.time),
      match: this.match?.snapshot() ?? null,
      players: this.players.map((p) => ({
        id: p.id, pos: v3(p.pos), vel: v3(p.vel), yaw: r(p.yaw), pitch: r(p.pitch), hp: r(p.hp), alive: p.alive,
        respawnAt: isFinite(p.respawnAt) ? r(p.respawnAt) : null, protectedUntil: isFinite(p.protectedUntil) ? r(p.protectedUntil) : -1,
        energy: r(p.energy), gas: r(p.gas), riding: p.riding?.id ?? null, heldBy: p.heldBy, inspiration: r(this.inspiration?.players.get(p.id)?.level ?? 0),
      })),
      constructs: [...this.constructs.list.values()].map((c) => ({ id: c.id, owner: c.owner, label: c.spec.label, hp: r(c.hp), insts: c.insts.map((i) => v3(i.pos)) })),
      entities: [...this.entities.values()].map((e) => ({ id: e.id, pos: v3(this.entityPos(e)), hp: r(e.hp) })),
      projectiles: this.projectiles.size,
    };
  }

  /** 32-bit FNV-1a hash of snapshotMatch() (desync detection: host and client compare these). */
  stateHash(): string { return hashString(JSON.stringify(this.snapshotMatch())); }

  uid(prefix: string) { return `${prefix}${this.nextId++}`; }

  emit(type: string, data: Record<string, any> = {}) {
    const e = { t: this.time, type, ...data };
    this.attributeEvent(e);
    this.log.push(e);
    if (this.log.length > 2000) this.log.splice(0, 500);
    for (const l of this.listeners) l(e);
    return e;
  }

  bodyAlive(b: RBody) {
    // identity check, not just the handle: Rapier reuses handles of removed bodies
    try { return this.pw.bodies.contains(b.handle) && this.pw.getRigidBody(b.handle) === b; } catch { return false; }
  }

  // ---------------- static geometry ----------------

  addBox(pos: Vector3, size: Vector3, color = 0x9aa3b5, kind: StaticGeom['kind'] = 'building', rot = new Quaternion()): StaticGeom {
    const id = this.uid('s');
    const body = this.pw.createRigidBody(this.R.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z).setRotation(rot));
    const col = this.pw.createCollider(this.R.ColliderDesc.cuboid(size.x / 2, size.y / 2, size.z / 2).setCollisionGroups(groups(G.WORLD, ALL)).setFriction(0.6), body);
    this.owners.set(col.handle, { kind: 'static', id });
    const g: StaticGeom = { id, shape: 'box', pos: pos.clone(), size: size.clone(), rot: rot.clone(), color, kind };
    this.statics.push(g);
    return g;
  }

  addCylinder(pos: Vector3, radius: number, height: number, color = 0x8f98aa, kind: StaticGeom['kind'] = 'rock'): StaticGeom {
    const id = this.uid('s');
    const body = this.pw.createRigidBody(this.R.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z));
    const col = this.pw.createCollider(this.R.ColliderDesc.cylinder(height / 2, radius).setCollisionGroups(groups(G.WORLD, ALL)), body);
    this.owners.set(col.handle, { kind: 'static', id });
    const g: StaticGeom = { id, shape: 'cylinder', pos: pos.clone(), size: new Vector3(radius, height, radius), rot: new Quaternion(), color, kind };
    this.statics.push(g);
    return g;
  }

  /** Wedge: ramp rising toward +z in local frame (x width, y height, z length). */
  addWedge(pos: Vector3, size: Vector3, rot = new Quaternion(), color = 0xb3a58f): StaticGeom {
    const id = this.uid('s');
    const hx = size.x / 2, hy = size.y / 2, hz = size.z / 2;
    // rises toward +z (tall end away from the local origin's -z side)
    const pts = new Float32Array([
      -hx, -hy, -hz, hx, -hy, -hz, -hx, -hy, hz, hx, -hy, hz, -hx, hy, hz, hx, hy, hz,
    ]);
    const body = this.pw.createRigidBody(this.R.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z).setRotation(rot));
    const col = this.pw.createCollider(this.R.ColliderDesc.convexHull(pts)!.setCollisionGroups(groups(G.WORLD, ALL)), body);
    this.owners.set(col.handle, { kind: 'static', id });
    const g: StaticGeom = { id, shape: 'wedge', pos: pos.clone(), size: size.clone(), rot: rot.clone(), color, kind: 'rock' };
    this.statics.push(g);
    return g;
  }

  addGoal(center: Vector3, half: Vector3, label: string) {
    const g: Goal = { id: this.uid('g'), min: center.clone().sub(half), max: center.clone().add(half), label, reached: false };
    this.goals.push(g);
    return g;
  }

  addRing(pos: Vector3, normal: Vector3, radius = 5) {
    const r: RaceRing = { id: this.uid('ring'), pos: pos.clone(), normal: normal.clone().normalize(), radius, passed: false };
    this.rings.push(r);
    return r;
  }

  // ---------------- entities ----------------

  addEntity(type: EntityType, pos: Vector3, opts: EntityOpts = {}): Entity {
    const R = this.R;
    const id = this.uid(type);
    let desc = R.RigidBodyDesc.dynamic().setTranslation(pos.x, pos.y, pos.z);
    let cdesc: any;
    let radius = 0.5;
    let hp = 100;
    let forceThreshold = 50;
    const heavy = opts.variant === 'heavy';
    switch (type) {
      case 'drone':
        radius = heavy ? 1.05 : 0.7;
        desc = desc.setLinearDamping(0.4).setAngularDamping(2);
        cdesc = R.ColliderDesc.ball(radius).setDensity((heavy ? 110 : 40) / (4 / 3 * Math.PI * radius ** 3));
        hp = heavy ? 260 : 100;
        break;
      case 'robot':
        // 2.1 m tall capsule; rotations locked while it walks (content.ts unlocks them when it's knocked over)
        radius = 0.55;
        desc = desc.lockRotations().setLinearDamping(0.05).setAngularDamping(0.6);
        cdesc = R.ColliderDesc.capsule(0.55, 0.5).setDensity(100).setFriction(0.3);
        hp = 120;
        forceThreshold = 140 * 3 / this.dt;
        break;
      case 'block': {
        const sz = opts.size ?? new Vector3(1, 1, 1);
        radius = Math.max(sz.x, sz.y, sz.z) / 2;
        desc = desc.setAngularDamping(0.15);
        const dens = opts.density ?? 120;
        cdesc = R.ColliderDesc.cuboid(sz.x / 2, sz.y / 2, sz.z / 2).setDensity(dens).setFriction(0.75);
        hp = 120;
        forceThreshold = dens * sz.x * sz.y * sz.z * 1.5 / this.dt;
        break;
      }
      case 'civilian':
        radius = 0.4;
        desc = desc.lockRotations().setLinearDamping(0.02);
        cdesc = R.ColliderDesc.capsule(0.5, 0.4).setDensity(70 / (Math.PI * 0.16 * 1.0 + 4 / 3 * Math.PI * 0.064));
        hp = 100;
        break;
      case 'boulder':
        radius = 2.2;
        desc = desc.setAngularDamping(0.8);
        cdesc = R.ColliderDesc.ball(radius).setDensity(6000 / (4 / 3 * Math.PI * radius ** 3)).setFriction(1.0);
        hp = 1e9;
        break;
      case 'crate':
        radius = 0.8;
        cdesc = R.ColliderDesc.cuboid(0.8, 0.8, 0.8).setDensity(60 / 4.1);
        hp = 60;
        break;
      case 'dummy':
        radius = 0.5;
        desc = desc.lockRotations();
        cdesc = R.ColliderDesc.capsule(0.6, 0.5).setDensity(20);
        hp = 200;
        break;
      case 'turret':
        radius = 1.2;
        desc = R.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z);
        cdesc = R.ColliderDesc.cylinder(1.2, 1.2);
        hp = 300;
        break;
    }
    if (opts.hp !== undefined) hp = opts.hp;
    if (opts.vel) desc = desc.setLinvel(opts.vel.x, opts.vel.y, opts.vel.z);
    const body = this.pw.createRigidBody(desc.setCcdEnabled(type !== 'turret' && type !== 'block'));
    const collider = this.pw.createCollider(
      cdesc.setCollisionGroups(groups(G.ENTITY, ALL)).setActiveEvents(R.ActiveEvents.CONTACT_FORCE_EVENTS | R.ActiveEvents.COLLISION_EVENTS).setContactForceEventThreshold(forceThreshold),
      body,
    );
    this.owners.set(collider.handle, { kind: 'entity', id });
    const e: Entity = {
      id, type, body, collider, radius, hp, maxHp: hp, hostile: opts.hostile ?? (type === 'drone' || type === 'turret' || type === 'robot'),
      home: pos.clone(), heldBy: null, restrainedBy: null, stunnedUntil: 0, disabled: false, hurt: false, safe: false,
      fireCooldown: 1 + ((this.nextId * 0.618) % 1), lastVel: new Vector3(), label: opts.label ?? (heavy ? 'heavy drone' : type),
      variant: heavy ? 'heavy' : 'standard', size: type === 'block' ? (opts.size ?? new Vector3(1, 1, 1)).clone() : null, color: opts.color ?? null,
      group: opts.group ?? null, breakable: type === 'crate' || type === 'block', credit: null, creditUntil: 0, disabledAt: 0, ai: null,
    };
    this.content.initEntity(e);
    this.entities.set(id, e);
    this.emit('entity_spawn', { id, entityType: type, variant: e.variant });
    return e;
  }

  entityPos(e: Entity, out = new Vector3()) { const t = e.body.translation(); return out.set(t.x, t.y, t.z); }
  entityVel(e: Entity, out = new Vector3()) { const t = e.body.linvel(); return out.set(t.x, t.y, t.z); }

  removeEntity(id: string) {
    const e = this.entities.get(id);
    if (!e) return;
    this.owners.delete(e.collider.handle);
    this.pw.removeRigidBody(e.body);
    this.entities.delete(id);
    this.content.onRemoved(id);
  }

  /**
   * Apply damage. `by` = the construct responsible; when omitted it is resolved from the entity's
   * credit (held / hit / thrown / knocked into by a construct recently) or, for construct-style
   * causes, the nearest construct. Events carry `by` so outcomes can be attributed to constructs.
   */
  damageEntity(e: Entity, amount: number, cause: string, by?: string | null) {
    if (e.type === 'boulder' || amount <= 0) return;
    const who = by !== undefined ? by : this.resolveCredit(e, cause);
    if (who) this.credit(e, who);
    if (e.type === 'civilian') {
      if (amount > 30 && !e.hurt) { e.hurt = true; this.emit('civilian_hurt', { id: e.id, cause, amount, by: who }); }
      return;
    }
    if (e.disabled && e.breakable) return; // already breaking
    e.hp = Math.max(0, e.hp - amount);
    this.stats.entityDamage += amount;
    this.emit('entity_damage', { id: e.id, entityType: e.type, amount: Math.round(amount), cause, by: who });
    if (e.hp <= 0 && !e.disabled) {
      e.disabled = true;
      e.disabledAt = this.time;
      if (e.breakable) this.content.queueBreak(e, cause, who);
      else this.emit('entity_disabled', { id: e.id, entityType: e.type, cause, by: who, pos: this.entityPos(e).toArray() });
      this.content.onDisabled(e, who);
    }
  }

  // ---------------- attribution ----------------

  /** Credit a construct with whatever happens to this entity in the next few seconds. */
  credit(e: Entity, constructId: string, seconds = 4) {
    e.credit = constructId;
    e.creditUntil = this.time + seconds;
  }

  /** The construct currently responsible for this entity (holding it, or recently hit/threw/knocked it). */
  creditOf(e: Entity): string | null {
    if (e.heldBy) return e.heldBy;
    if (e.restrainedBy) return e.restrainedBy;
    return e.credit && this.time <= e.creditUntil ? e.credit : null;
  }

  private resolveCredit(e: Entity, cause: string): string | null {
    const c = this.creditOf(e);
    if (c) return c;
    if (cause === 'strike' || cause === 'explosion' || cause === 'crush' || cause === 'construct' || cause === 'smash') return this.nearestConstruct(this.entityPos(e), e.radius + 2.5);
    return null;
  }

  /** Nearest construct instance whose bounding sphere is within `reach` of p. */
  nearestConstruct(p: Vector3, reach: number): string | null {
    const list: Map<string, any> | undefined = (this.constructs as any)?.list;
    if (!list) return null;
    let best: string | null = null;
    let bd = Infinity;
    for (const c of list.values()) {
      const r = typeof c.radius === 'number' ? c.radius : 1;
      for (const i of c.insts ?? []) {
        if (!i?.pos || i.alive === false) continue;
        const d = i.pos.distanceTo(p) - r;
        if (d < reach && d < bd) { bd = d; best = c.id; }
      }
    }
    return best;
  }

  /** Events the construct runtime emits already name construct + entity: use them to credit. */
  private attributeEvent(e: GameEvent) {
    const cid: string | undefined = e.construct;
    if (!cid || typeof cid !== 'string') return;
    if (e.entity) { const ent = this.entities.get(e.entity); if (ent) this.credit(ent, cid); }
    if (e.type === 'construct_fire') {
      const p = this.lastProjectile;
      if (p && p.from === null && p.by === null && p.born === this.time) p.by = cid;
    } else if ((e.type === 'explosion' || e.type === 'smash') && Array.isArray(e.pos)) {
      const at = new Vector3(e.pos[0], e.pos[1], e.pos[2]);
      const r = (e.radius ?? 4) + 1;
      for (const ent of this.entities.values()) if (this.entityPos(ent).distanceTo(at) <= r + ent.radius) this.credit(ent, cid);
    }
  }

  onPlayerRam(collider: RCollider, impulse: number, speed: number) {
    const o = this.owners.get(collider.handle);
    if (o?.kind === 'entity') {
      const e = this.entities.get(o.id);
      if (e) this.damageEntity(e, Math.max(0, speed - 8) * 5, 'ram');
    }
    this.emit('player_ram', { impulse: Math.round(impulse), speed: Math.round(speed) });
  }

  spawnProjectile(pos: Vector3, vel: Vector3, from: string | null, damage = 8, radius = 0.28, opts: ProjectileOpts = {}): Projectile {
    const R = this.R;
    const body = this.pw.createRigidBody(R.RigidBodyDesc.dynamic().setTranslation(pos.x, pos.y, pos.z).setLinvel(vel.x, vel.y, vel.z).setGravityScale(opts.gravity ?? 0).setCcdEnabled(true));
    // players: every lantern's own bit except the shot's owner (a construct's shots never touch its owner)
    const hits = G.WORLD | G.CONSTRUCT | G.GHOST | G.ENTITY | G.CARGO | (opts.friendly || opts.ownerPlayer ? this.playerMask(opts.ownerPlayer ?? null) : G.PLAYER | PLAYER_BITS);
    const collider = this.pw.createCollider(
      R.ColliderDesc.ball(radius).setDensity(2).setCollisionGroups(groups(G.PROJECTILE, hits))
        .setActiveEvents(R.ActiveEvents.COLLISION_EVENTS)
        .setActiveCollisionTypes(R.ActiveCollisionTypes.DEFAULT | R.ActiveCollisionTypes.DYNAMIC_KINEMATIC),
      body,
    );
    const p: Projectile = { id: collider.handle, body, collider, from, by: null, reflected: false, born: this.time, damage };
    if (opts.friendly) { p.friendly = true; p.owner = opts.owner ?? null; p.by = opts.owner ?? null; p.ownerPlayer = opts.ownerPlayer ?? null; p.splash = opts.splash; p.kind = opts.kind; }
    else if (opts.ownerPlayer) { p.ownerPlayer = opts.ownerPlayer; p.owner = opts.owner ?? null; p.by = opts.owner ?? null; p.kind = opts.kind; }
    this.projectiles.set(collider.handle, p);
    this.lastProjectile = p;
    this.owners.set(collider.handle, { kind: 'projectile', id: collider.handle });
    this.stats.projectilesFired++;
    return p;
  }

  /** A reflected / re-owned shot: it now belongs to `ownerPlayer` and never touches them. */
  reownProjectile(p: Projectile, construct: string | null, ownerPlayer: string | null) {
    p.by = construct;
    if (p.friendly) p.owner = construct;
    p.ownerPlayer = ownerPlayer;
    const hits = G.WORLD | G.CONSTRUCT | G.GHOST | G.ENTITY | G.CARGO | this.playerMask(ownerPlayer);
    p.collider.setCollisionGroups(groups(G.PROJECTILE, hits));
  }

  removeProjectile(p: Projectile) {
    if (!this.projectiles.has(p.id)) return;
    this.projectiles.delete(p.id);
    this.owners.delete(p.collider.handle);
    this.pw.removeRigidBody(p.body);
  }

  // ---------------- simulation ----------------

  /** Rapier only updates its query structures during step(); make newly added colliders queryable now. */
  refreshQueries() {
    const dt = this.pw.timestep;
    this.pw.timestep = 0;
    this.pw.step();
    this.pw.timestep = dt;
  }

  /**
   * One 60 Hz tick. `input` drives the LOCAL player; other players use their controller (bots) or the input
   * last given with setInput() (netcode). Edge-triggered keys (fly toggle, use) fire once.
   */
  step(input: PlayerInput = NO_INPUT) {
    const dt = this.dt;
    this.input = input;
    this.player.input = input;
    for (const [id, ctl] of this.controllers) { const p = this.playerById(id); if (p && p !== this.player) p.input = ctl.tick(this); }
    // construct actions read this tick's buttons first (a click acts on the tick it arrives)
    for (const p of this.players) this.actions.input(p, p.input, dt);
    this.constructs.preStep(dt);
    for (const p of this.players) {
      p.update(p.input, dt);
      this.actions.step(p, dt);
      if (p !== this.player && (p.input.flyToggle || p.input.use)) p.input = { ...p.input, flyToggle: false, use: false };
    }
    this.updateEntities(dt);
    this.pw.step(this.eq);
    this.handleEvents();
    this.content.postStep(dt);
    this.constructs.postStep(dt);
    this.updatePlayersLife();
    this.checkGoals();
    this.time += dt;
    this.tick++;
    this.match?.update();
    for (const h of this.tickHooks) h();
  }

  /** Respawns, fall-out-of-the-world deaths. */
  private updatePlayersLife() {
    for (const p of this.players) {
      if (!p.alive) {
        if (this.time >= p.respawnAt) this.respawn(p);
        continue;
      }
      if (p.pos.y < -60 && this.deathEnabled) {
        p.hurtDir.set(0, 1, 0);
        const by = p.flungBy;
        this.damagePlayer(p, p.hp + 1, 'fall', by);
        if (p.alive) this.respawn(p);
      }
    }
  }

  private updateEntities(dt: number) {
    const ppos = this.player.pos;
    const alivePlayers = this.players.length > 1 ? this.players.filter((p) => p.alive) : null;
    for (const e of this.entities.values()) {
      const p = this.entityPos(e);
      const v = this.entityVel(e);
      if (p.y < -200) { this.removeEntity(e.id); continue; }
      if (e.type === 'drone') {
        const free = !e.heldBy && !e.restrainedBy && !e.disabled && this.time >= e.stunnedUntil;
        if (free) {
          // hover PD toward home with a lazy figure-8 wander
          const tgt = e.home.clone().add(new Vector3(Math.sin(this.time * 0.5 + e.home.x) * 3, Math.sin(this.time * 0.9 + e.home.z) * 1.2, Math.cos(this.time * 0.4) * 3));
          const m = e.body.mass();
          const f = tgt.sub(p).multiplyScalar(18).addScaledVector(v, -6).add(new Vector3(0, 22, 0)).multiplyScalar(m);
          const fl = f.length();
          const fmax = m * 60;
          if (fl > fmax) f.multiplyScalar(fmax / fl);
          e.body.resetForces(true);
          e.body.addForce(f, true);
        } else {
          e.body.resetForces(true);
        }
        if (free && e.hostile) {
          const tp = alivePlayers ? nearestPos(alivePlayers, p) ?? ppos : ppos;
          if (e.variant === 'heavy') this.entityFire(e, p, tp, dt, 1.9, 24, 12, 0.36);
          else this.entityFire(e, p, tp, dt, 2.6, 26);
        }
      } else if (e.type === 'turret') {
        const tp = alivePlayers ? nearestPos(alivePlayers, p) ?? ppos : ppos;
        if (!e.disabled && e.hostile) this.entityFire(e, p.clone().add(new Vector3(0, 1.6, 0)), tp, dt, 0.45, 30);
      } else if (e.ai) {
        this.content.updateEntity(e, p, v, dt);
      } else if (e.type === 'civilian') {
        if (!e.hurt && !e.safe && !e.heldBy && v.length() < 0.5 && this.time > 1) {
          e.safe = true;
          this.emit('civilian_safe', { id: e.id });
        }
      }
      if (e.heldBy) this.credit(e, e.heldBy);
      else if (e.restrainedBy) this.credit(e, e.restrainedBy);
      e.lastVel.copy(v);
    }
    // projectiles lifetime
    for (const p of this.projectiles.values()) if (this.time - p.born > 5) this.removeProjectile(p);
  }

  private entityFire(e: Entity, from: Vector3, target: Vector3, dt: number, period: number, speed: number, damage = 8, size = 0.28) {
    e.fireCooldown -= dt;
    if (e.fireCooldown > 0) return;
    const d = target.clone().add(new Vector3(0, 0.3, 0)).sub(from);
    const dist = d.length();
    if (dist > 90) { e.fireCooldown = 0.5; return; }
    e.fireCooldown = period;
    d.normalize();
    const start = from.clone().addScaledVector(d, e.radius + 0.6);
    this.spawnProjectile(start, d.multiplyScalar(speed), e.id, damage, size);
    this.emit('fire', { from: e.id });
  }

  private handleEvents() {
    this.eq.drainCollisionEvents((h1, h2, started) => {
      if (!started) return;
      const o1 = this.owners.get(h1);
      const o2 = this.owners.get(h2);
      if (!o1 || !o2) return;
      if (o1.kind === 'projectile') this.projectileHit(o1.id, o2, h2);
      else if (o2.kind === 'projectile') this.projectileHit(o2.id, o1, h1);
      else if (o1.kind === 'entity' || o2.kind === 'entity') this.touchCredit(o1, o2);
    });
    this.eq.drainContactForceEvents((ev) => {
      const h1 = ev.collider1();
      const h2 = ev.collider2();
      const mag = ev.totalForceMagnitude();
      for (const [h, other] of [[h1, h2], [h2, h1]] as const) {
        const o = this.owners.get(h);
        if (o?.kind !== 'entity') continue;
        const e = this.entities.get(o.id);
        if (!e) continue;
        const oo = this.owners.get(other);
        // soft constructs cushion impacts
        if (oo?.kind === 'construct' && this.constructs.isSoft(oo.id)) continue;
        const dv = (mag * this.dt) / Math.max(1, e.body.mass()); // velocity change this step
        // who is responsible: the construct itself, or whatever construct the other entity is credited to
        const oe = oo?.kind === 'entity' ? this.entities.get(oo.id) : undefined;
        const by = oo?.kind === 'construct' ? oo.id : oe ? this.creditOf(oe) ?? undefined : undefined;
        if (by && dv > 1) this.credit(e, by);
        if (e.type === 'civilian') {
          if (dv > 13) this.damageEntity(e, 100, oo?.kind ?? 'impact', by);
        } else if (e.type === 'block') {
          // blocks mostly topple; only violent hits from constructs / the ground break them
          if (dv > 14 && oo?.kind !== 'entity') this.damageEntity(e, (dv - 14) * 5, oo?.kind === 'construct' ? 'construct' : 'impact', by);
        } else if (dv > 5) {
          const cause = oo?.kind === 'construct' ? 'construct' : oo?.kind === 'static' ? 'impact' : 'collision';
          this.damageEntity(e, (dv - 5) * 7, cause, by);
          if (e.type === 'drone') e.stunnedUntil = this.time + Math.min(2.5, dv * (e.variant === 'heavy' ? 0.04 : 0.08));
          if (oo?.kind === 'construct') this.constructs.noteImpact(oo.id, e.id, dv);
          if (e.type === 'robot') this.content.knock(e, oe ? this.entityPos(oe) : null, dv, by ?? null);
        }
      }
    });
  }

  /** Collision start between an entity and a construct / another entity: pass construct credit along. */
  private touchCredit(o1: Owner, o2: Owner) {
    const e1 = o1.kind === 'entity' ? this.entities.get(o1.id) : undefined;
    const e2 = o2.kind === 'entity' ? this.entities.get(o2.id) : undefined;
    if (e1 && o2.kind === 'construct') { this.credit(e1, o2.id); return; }
    if (e2 && o1.kind === 'construct') { this.credit(e2, o1.id); return; }
    if (e1 && e2) {
      const c1 = this.creditOf(e1), c2 = this.creditOf(e2);
      // only moving things carry credit into resting things (a thrown crate into a stack)
      if (c1 && !c2 && this.entityVel(e1).lengthSq() > 1) this.credit(e2, c1, 3);
      else if (c2 && !c1 && this.entityVel(e2).lengthSq() > 1) this.credit(e1, c2, 3);
    }
  }

  private projectileHit(pid: number, other: Owner, _h: number) {
    const p = this.projectiles.get(pid);
    if (!p) return;
    if (p.friendly) { this.constructs.machines.projectileHit(p, other); return; }
    switch (other.kind) {
      case 'player': {
        const target = this.playerById(other.id);
        if (!target || target.id === p.ownerPlayer) { this.removeProjectile(p); break; }
        // a construct's 'shoot' bullets carry their own damage; a shot sent back by a mirror hits hard
        const shoot = !!p.ownerPlayer && p.kind === 'shoot';
        const dmg = shoot ? p.damage * 0.9 : p.reflected ? 20 : p.damage;
        const v = p.body.linvel();
        target.hitsTaken++;
        this.damagePlayer(target, dmg, shoot ? 'shot' : p.reflected ? 'reflected' : 'shot', p.by ?? p.from, { dir: new Vector3(v.x, v.y, v.z), knock: 0.5 });
        this.emit('player_hit', { from: p.from, kind: 'shot', damage: dmg, target: target.id, by: p.by });
        this.removeProjectile(p);
        break;
      }
      case 'construct': {
        const con = this.constructs.get(other.id);
        this.stats.projectilesBlocked++;
        const res = this.constructs.onProjectile(other.id, p);
        this.emit('blocked', { construct: other.id, reflected: res === 'reflect', from: p.by ?? p.from, owner: con?.owner ?? null });
        if (res === 'reflect') { this.stats.projectilesReflected++; p.reflected = true; this.reownProjectile(p, other.id, con?.owner ?? null); }
        else this.removeProjectile(p);
        break;
      }
      case 'entity': {
        const e = this.entities.get(other.id);
        // scenery (blocks / bricks) stops hostile fire without being worn down by it
        // (and robots don't get shot up by their own side's fire)
        const straySceneryHit = (e?.type === 'block' || (e?.type === 'robot' && !!p.from && !!this.entities.get(p.from)?.hostile)) && !p.reflected && p.from !== null;
        if (e && (p.reflected || p.from !== e.id) && !straySceneryHit) this.damageEntity(e, p.reflected ? 34 : 10, 'projectile', p.by ?? (p.from ? null : undefined));
        this.removeProjectile(p);
        break;
      }
      default:
        this.removeProjectile(p);
    }
  }

  private checkGoals() {
    const p = this.player.pos;
    for (const g of this.goals) {
      if (!g.reached && p.x >= g.min.x && p.x <= g.max.x && p.y >= g.min.y && p.y <= g.max.y && p.z >= g.min.z && p.z <= g.max.z) {
        g.reached = true;
        this.emit('goal_reached', { id: g.id, label: g.label });
      }
    }
    for (const r of this.rings) {
      if (r.passed) continue;
      const d = p.clone().sub(r.pos);
      if (Math.abs(d.dot(r.normal)) < 1.5 && d.length() < r.radius) {
        r.passed = true;
        this.emit('ring_passed', { id: r.id });
      }
    }
  }

  /** Which entity/construct does a collider belong to? */
  ownerOf(c: RCollider | null): Owner | null { return c ? this.owners.get(c.handle) ?? null : null; }
}

function nearestPos(ps: Player[], from: Vector3): Vector3 | null {
  let best: Vector3 | null = null, bd = Infinity;
  for (const p of ps) { const d = p.pos.distanceToSquared(from); if (d < bd) { bd = d; best = p.pos; } }
  return best;
}

/** 32-bit FNV-1a as 8 hex chars. */
export function hashString(s: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193); }
  return (h >>> 0).toString(16).padStart(8, '0');
}
