import { Vector3, Quaternion } from 'three';
import { initPhysics, G, ALL, groups, type Rapier, type RWorld, type RBody, type RCollider } from './physics.js';
import { Player, type PlayerInput, NO_INPUT } from './player.js';
import { ConstructRuntime } from './constructs/runtime.js';
import { WorldContent, type EntityAI } from './content.js';
import { CreativityEconomy } from './inspiration.js';

export type Owner =
  | { kind: 'static'; id: string }
  | { kind: 'player' }
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
}

export interface Goal { id: string; min: Vector3; max: Vector3; label: string; reached: boolean }

export interface GameEvent { t: number; type: string; [k: string]: any }

export interface RaceRing { id: string; pos: Vector3; normal: Vector3; radius: number; passed: boolean }

export class World {
  R!: Rapier;
  pw!: RWorld;
  eq!: InstanceType<Rapier['EventQueue']>;
  time = 0;
  tick = 0;
  readonly dt = 1 / 60;
  owners = new Map<number, Owner>();
  player!: Player;
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
    w.player = new Player(w);
    w.constructs = new ConstructRuntime(w);
    w.content = new WorldContent(w);
    w.inspiration = new CreativityEconomy(w);
    w.player.onHit = (c, n, p) => w.constructs.onPlayerTouch(c, n, p);
    return w;
  }

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

  spawnProjectile(pos: Vector3, vel: Vector3, from: string | null, damage = 8, radius = 0.28): Projectile {
    const R = this.R;
    const body = this.pw.createRigidBody(R.RigidBodyDesc.dynamic().setTranslation(pos.x, pos.y, pos.z).setLinvel(vel.x, vel.y, vel.z).setGravityScale(0).setCcdEnabled(true));
    const collider = this.pw.createCollider(
      R.ColliderDesc.ball(radius).setDensity(2).setCollisionGroups(groups(G.PROJECTILE, G.WORLD | G.CONSTRUCT | G.GHOST | G.ENTITY | G.PLAYER | G.CARGO))
        .setActiveEvents(R.ActiveEvents.COLLISION_EVENTS)
        .setActiveCollisionTypes(R.ActiveCollisionTypes.DEFAULT | R.ActiveCollisionTypes.DYNAMIC_KINEMATIC),
      body,
    );
    const p: Projectile = { id: collider.handle, body, collider, from, by: null, reflected: false, born: this.time, damage };
    this.projectiles.set(collider.handle, p);
    this.lastProjectile = p;
    this.owners.set(collider.handle, { kind: 'projectile', id: collider.handle });
    this.stats.projectilesFired++;
    return p;
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

  step(input: PlayerInput = NO_INPUT) {
    const dt = this.dt;
    this.input = input;
    this.constructs.preStep(dt);
    this.player.update(input, dt);
    this.updateEntities(dt);
    this.pw.step(this.eq);
    this.handleEvents();
    this.content.postStep(dt);
    this.constructs.postStep(dt);
    this.checkGoals();
    this.time += dt;
    this.tick++;
    for (const h of this.tickHooks) h();
  }

  private updateEntities(dt: number) {
    const ppos = this.player.pos;
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
          if (e.variant === 'heavy') this.entityFire(e, p, ppos, dt, 1.9, 24, 12, 0.36);
          else this.entityFire(e, p, ppos, dt, 2.6, 26);
        }
      } else if (e.type === 'turret') {
        if (!e.disabled && e.hostile) this.entityFire(e, p.clone().add(new Vector3(0, 1.6, 0)), ppos, dt, 0.45, 30);
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
    switch (other.kind) {
      case 'player':
        this.player.hp = Math.max(0, this.player.hp - p.damage);
        this.player.hitsTaken++;
        this.stats.playerHits++;
        this.emit('player_hit', { from: p.from, kind: 'shot', damage: p.damage });
        this.removeProjectile(p);
        break;
      case 'construct': {
        this.stats.projectilesBlocked++;
        const res = this.constructs.onProjectile(other.id, p);
        this.emit('blocked', { construct: other.id, reflected: res === 'reflect' });
        if (res === 'reflect') { this.stats.projectilesReflected++; p.reflected = true; p.by = other.id; }
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
