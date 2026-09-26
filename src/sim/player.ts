import { Vector3 } from 'three';
import { MOVE, type MoveTuning } from './tuning.js';
import { G, groups, PLAYER_MOVE_FILTER, type Rapier, type RBody, type RCollider } from './physics.js';
import type { World } from './world.js';

export interface PlayerInput {
  moveX: number; // strafe, +right
  moveZ: number; // +forward
  yaw: number;
  pitch: number;
  jump: boolean;
  crouch: boolean;
  boost: boolean;
  hookL: boolean;
  hookR: boolean;
  reel: boolean;
  flyToggle: boolean; // edge-triggered
  descend: boolean;
  /** get in / out of the nearest rideable construct (edge-triggered; G) */
  use?: boolean;
}

export const NO_INPUT: PlayerInput = {
  moveX: 0, moveZ: 0, yaw: 0, pitch: 0, jump: false, crouch: false, boost: false,
  hookL: false, hookR: false, reel: false, flyToggle: false, descend: false,
};

/** Modifiers contributed by constructs the player is mounted on / wearing. */
export interface PlayerMods {
  groundSpeedMul: number;
  frictionMul: number;
  airAccelMul: number;
  thrust: Vector3; // world-space acceleration
  thrustAlongLook: number; // acceleration along look direction
  glide: number; // 0 = none; >0 = wing strength
  maxFallSpeed: number; // Infinity = none
  sources: string[];
}

export function emptyMods(): PlayerMods {
  return { groundSpeedMul: 1, frictionMul: 1, airAccelMul: 1, thrust: new Vector3(), thrustAlongLook: 0, glide: 0, maxFallSpeed: Infinity, sources: [] };
}

export interface Tether {
  side: 'L' | 'R';
  state: 'flying' | 'attached' | 'retracting';
  origin: Vector3; // current hook tip position
  target: Vector3; // world point it is flying toward / attached to
  dir: Vector3;
  travel: number;
  dist: number; // distance to hit at fire time (Infinity = miss)
  body: RBody | null; // anchored body (null = static world)
  local: Vector3; // anchor in body-local frame
  length: number;
  attachTime: number;
  constructId: string | null; // non-null when created by a construct (grapple construct)
  autoReel?: boolean; // winch-like constructs reel the player in automatically
}

export type HitHandler = (collider: RCollider, normal: Vector3, player: Player) => 'clip' | 'ignore' | 'handled';

const tmp = new Vector3();
const tmp2 = new Vector3();
const UP = new Vector3(0, 1, 0);

export class Player {
  pos = new Vector3(0, 2, 0);
  vel = new Vector3();
  yaw = 0;
  pitch = 0;
  onGround = false;
  groundNormal = new Vector3(0, 1, 0);
  groundBody: RBody | null = null;
  groundCollider: RCollider | null = null;
  groundVel = new Vector3();
  sliding = false;
  flying = false;
  gas: number = MOVE.gasMax;
  hp = 100;
  hitsTaken = 0;
  lastGroundTime = -1;
  jumpPressedAt = -10;
  prevJump = false;
  prevHookL = false;
  prevHookR = false;
  tethers: { L: Tether | null; R: Tether | null } = { L: null, R: null };
  mods: PlayerMods = emptyMods();
  body!: RBody;
  collider!: RCollider;
  shape: any;
  tune: MoveTuning = { ...MOVE };
  /** diagnostics */
  stats = { maxSpeed: 0, landings: 0, jumps: 0, speedAtLastLanding: 0, lastLaunchSpeed: 0 };
  onHit: HitHandler | null = null;
  /** last intended horizontal movement direction (for climbing, construct helpers) */
  wishDir = new Vector3();
  /** true for one tick after a jump (renderer uses for anims/sfx) */
  events: string[] = [];
  /** Riding a construct: the machine runtime moves us with its seat; WASD/mouse/fire drive it. */
  riding: { id: string; body: RBody | null; camBack: number; camUp: number } | null = null;

  constructor(private world: World) {
    const R = world.R;
    this.body = world.pw.createRigidBody(R.RigidBodyDesc.kinematicPositionBased().setTranslation(this.pos.x, this.pos.y, this.pos.z));
    this.collider = world.pw.createCollider(
      R.ColliderDesc.capsule(MOVE.playerHalfHeight, MOVE.playerRadius)
        .setCollisionGroups(groups(G.PLAYER, G.PROJECTILE))
        .setActiveCollisionTypes(R.ActiveCollisionTypes.DEFAULT | R.ActiveCollisionTypes.KINEMATIC_FIXED | R.ActiveCollisionTypes.DYNAMIC_KINEMATIC)
        .setActiveEvents(R.ActiveEvents.COLLISION_EVENTS),
      this.body,
    );
    this.shape = new R.Capsule(MOVE.playerHalfHeight, MOVE.playerRadius);
    world.owners.set(this.collider.handle, { kind: 'player' });
  }

  teleport(p: Vector3, vel?: Vector3) {
    this.pos.copy(p);
    this.vel.copy(vel ?? new Vector3());
    this.body.setTranslation(this.pos, true);
    this.tethers.L = this.tethers.R = null;
  }

  /** Get in/on a rideable construct. */
  ride(constructId: string): boolean { return this.world.constructs.machines.ride(constructId); }
  /** Get out of whatever we're riding. */
  dismount(): boolean { return this.world.constructs.machines.dismount(); }

  get feet(): Vector3 { return tmp.copy(this.pos).setY(this.pos.y - MOVE.playerHalfHeight - MOVE.playerRadius); }

  forward(out = new Vector3()): Vector3 { return out.set(-Math.sin(this.yaw), 0, -Math.cos(this.yaw)); }
  right(out = new Vector3()): Vector3 { return out.set(Math.cos(this.yaw), 0, -Math.sin(this.yaw)); }
  look(out = new Vector3()): Vector3 {
    const c = Math.cos(this.pitch);
    return out.set(-Math.sin(this.yaw) * c, Math.sin(this.pitch), -Math.cos(this.yaw) * c);
  }

  /** Third-person camera pose (shared by renderer and aim logic so aim is deterministic in tests). */
  camera(outPos = new Vector3(), outDir = new Vector3()): { pos: Vector3; dir: Vector3 } {
    const R = this.world.R;
    const dir = this.look(outDir);
    const pivot = tmp2.copy(this.pos).add(new Vector3(0, 0.75 + (this.riding?.camUp ?? 0), 0));
    const right = this.right(new Vector3());
    const speed = this.vel.length();
    const back = 5.2 + Math.min(3, speed * 0.03) + (this.riding?.camBack ?? 0);
    const desired = new Vector3().copy(pivot).addScaledVector(dir, -back).addScaledVector(right, 0.9).addScaledVector(UP, 0.35);
    // pull camera in against world geometry
    const off = new Vector3().subVectors(desired, pivot);
    const len = off.length();
    if (len > 1e-3) {
      off.divideScalar(len);
      const hit = this.world.pw.castRay(new R.Ray(pivot, off), len, true, undefined, groups(G.PLAYER, G.WORLD));
      const d = hit ? Math.max(0.3, hit.timeOfImpact - 0.25) : len;
      outPos.copy(pivot).addScaledVector(off, d);
    } else outPos.copy(desired);
    return { pos: outPos, dir };
  }

  /** Aim point: first thing under the crosshair (excluding the player). */
  aim(maxDist = 400): { point: Vector3; collider: RCollider | null; normal: Vector3 } {
    const R = this.world.R;
    const cam = this.camera();
    // start the ray at the player's depth so we never hit things between camera and player
    const startOff = Math.max(0, new Vector3().subVectors(this.pos, cam.pos).dot(cam.dir) - 0.2);
    const origin = cam.pos.clone().addScaledVector(cam.dir, startOff);
    const hit = this.world.pw.castRayAndGetNormal(
      new R.Ray(origin, cam.dir), maxDist, true, undefined,
      groups(G.PLAYER, G.WORLD | G.CONSTRUCT | G.ENTITY | G.CARGO), this.collider, this.riding?.body ?? undefined,
    );
    if (!hit) return { point: origin.addScaledVector(cam.dir, maxDist), collider: null, normal: new Vector3(0, 1, 0) };
    return {
      point: origin.addScaledVector(cam.dir, hit.timeOfImpact),
      collider: hit.collider,
      normal: new Vector3(hit.normal.x, hit.normal.y, hit.normal.z),
    };
  }

  update(input: PlayerInput, dt: number) {
    const t = this.world.time;
    const T = this.tune;
    this.events.length = 0;
    this.yaw = input.yaw;
    this.pitch = input.pitch;
    if (this.riding) {
      // the machine runtime already put us on the seat this tick; our own movement is off
      this.prevJump = input.jump;
      this.prevHookL = input.hookL;
      this.prevHookR = input.hookR;
      this.tethers.L = this.tethers.R = null;
      this.flying = false;
      this.sliding = false;
      this.onGround = true;
      this.lastGroundTime = t;
      this.body.setTranslation(this.pos, true);
      this.mods = emptyMods();
      return;
    }

    if (input.flyToggle) {
      this.flying = !this.flying;
      this.events.push(this.flying ? 'fly_on' : 'fly_off');
    }
    if (input.jump && !this.prevJump) this.jumpPressedAt = t;
    this.prevJump = input.jump;

    // --- tethers (edge-triggered fire, hold to stay attached)
    this.updateTetherInput('L', input.hookL, this.prevHookL);
    this.updateTetherInput('R', input.hookR, this.prevHookR);
    this.prevHookL = input.hookL;
    this.prevHookR = input.hookR;
    this.advanceTethers(dt);

    const fwd = this.forward(new Vector3());
    const right = this.right(new Vector3());
    const wish = new Vector3().addScaledVector(fwd, input.moveZ).addScaledVector(right, input.moveX);
    const wishLen = Math.min(1, wish.length());
    if (wishLen > 1e-4) wish.normalize();
    this.wishDir.copy(wish).multiplyScalar(wishLen);

    this.categorizeGround();
    const m = this.mods;
    const anyTether = !!(this.tethers.L?.state === 'attached' || this.tethers.R?.state === 'attached');

    if (this.flying) {
      this.flyMove(input, wish, wishLen, dt);
    } else if (this.onGround) {
      const buffered = t - this.jumpPressedAt <= T.jumpBuffer;
      const wantSlide = input.crouch && this.horizSpeed() > T.slideMinSpeed;
      this.sliding = wantSlide;
      if (buffered) {
        // jump: no friction this tick => landing + instant jump preserves speed (bhop)
        this.jumpPressedAt = -10;
        this.vel.y = Math.max(this.vel.y, 0) + T.jumpSpeed;
        this.vel.add(this.groundVel.clone().setY(Math.max(0, this.groundVel.y)));
        this.onGround = false;
        this.stats.jumps++;
        this.events.push('jump');
      } else {
        this.applyFriction(dt, this.sliding ? T.slideFriction : T.friction * m.frictionMul);
        if (!this.sliding) this.accelerate(wish, wishLen * T.runSpeed * m.groundSpeedMul, T.groundAccel, dt);
        else {
          // slope pulls a slide downhill
          const n = this.groundNormal;
          const g = new Vector3(0, -T.gravity * T.slideGravityAlongSlope, 0);
          g.addScaledVector(n, -g.dot(n));
          this.vel.addScaledVector(g, dt);
          this.accelerate(wish, wishLen * T.airWishCap, T.airAccel, dt); // tiny steering
        }
        // keep velocity on the ground plane
        const vn = this.vel.dot(this.groundNormal);
        if (vn < 0) this.vel.addScaledVector(this.groundNormal, -vn);
      }
    } else {
      this.sliding = false;
      // coyote jump
      if (t - this.lastGroundTime < T.coyoteTime && t - this.jumpPressedAt <= T.jumpBuffer && this.vel.y <= 0.5) {
        this.jumpPressedAt = -10;
        this.vel.y = T.jumpSpeed;
        this.stats.jumps++;
        this.events.push('jump');
      }
      this.airAccelerate(wish, wishLen, dt);
      this.vel.y -= T.gravity * dt;
      if (m.glide > 0) this.glide(dt);
      if (this.vel.y < -m.maxFallSpeed) this.vel.y += Math.min(-m.maxFallSpeed - this.vel.y, 60 * dt);
    }

    // construct-provided thrust (rockets, jetpacks, fans, wings...)
    if (m.thrust.lengthSq() > 0) this.vel.addScaledVector(m.thrust, dt);
    if (m.thrustAlongLook) this.vel.addScaledVector(this.look(new Vector3()), m.thrustAlongLook * dt);

    // boost thruster (hold)
    if (input.boost && this.gas > 0) {
      const dir = this.onGround && wishLen > 0 ? wish.clone() : this.look(new Vector3());
      if (!this.onGround && wishLen > 0 && !this.flying) {
        // steer boost toward movement keys, but keep look pitch
        dir.addScaledVector(wish, 0.6).normalize();
      }
      this.vel.addScaledVector(dir, T.boostAccel * dt);
      if (this.onGround && dir.y <= 0.05) this.vel.y = Math.max(this.vel.y, 0);
      this.gas = Math.max(0, this.gas - T.boostDrain * dt);
    } else {
      this.gas = Math.min(T.gasMax, this.gas + (this.onGround ? T.gasRegenGround : T.gasRegenAir) * dt);
    }

    this.applyTetherForces(input, dt, anyTether);

    // speed limit (safety, not gameplay)
    const sp = this.vel.length();
    if (sp > T.maxSpeed) this.vel.multiplyScalar(T.maxSpeed / sp);

    // move with ground platform
    if (this.onGround && this.groundVel.lengthSq() > 0) this.slideMove(this.groundVel.clone(), dt, false);
    const wasGround = this.onGround;
    const vy = this.vel.y;
    this.stepSlideMove(dt, wasGround);
    this.enforceTetherLength();
    this.depenetrate();
    this.categorizeGround();
    if (this.onGround && !wasGround) {
      this.stats.landings++;
      this.stats.speedAtLastLanding = this.horizSpeed();
      if (vy < -18) this.events.push('land_hard'); else this.events.push('land');
    }
    if (this.onGround) this.lastGroundTime = t;
    this.stats.maxSpeed = Math.max(this.stats.maxSpeed, this.vel.length());
    this.body.setTranslation(this.pos, true);
    // reset mods (constructs re-apply each tick)
    this.mods = emptyMods();
  }

  horizSpeed() { return Math.hypot(this.vel.x, this.vel.z); }

  private accelerate(wishDir: Vector3, wishSpeed: number, accel: number, dt: number) {
    if (wishSpeed <= 0) return;
    const cur = this.vel.dot(wishDir);
    const add = wishSpeed - cur;
    if (add <= 0) return;
    const a = Math.min(accel * wishSpeed * dt, add);
    this.vel.addScaledVector(wishDir, a);
  }

  /** Source AirAccelerate: projection capped at airWishCap, accel uses full wish speed. */
  private airAccelerate(wishDir: Vector3, wishLen: number, dt: number) {
    if (wishLen <= 0) return;
    const T = this.tune;
    const wishSpeed = wishLen * T.runSpeed;
    const capped = Math.min(wishSpeed, T.airWishCap);
    const cur = this.vel.dot(wishDir);
    const add = capped - cur;
    if (add <= 0) return;
    const a = Math.min(T.airAccel * this.mods.airAccelMul * wishSpeed * dt, add);
    this.vel.addScaledVector(wishDir, a);
  }

  private applyFriction(dt: number, friction: number) {
    const rel = tmp.copy(this.vel).sub(this.groundVel);
    const speed = rel.length();
    if (speed < 1e-3) { this.vel.copy(this.groundVel); return; }
    const control = Math.max(speed, this.tune.stopSpeed);
    const drop = control * friction * dt;
    const ns = Math.max(0, speed - drop) / speed;
    rel.multiplyScalar(ns);
    this.vel.copy(rel).add(this.groundVel);
  }

  /** Accessible flight: hover + cruise. Faster travel must come from momentum (dive, tethers, boosts, constructs). */
  private flyMove(input: PlayerInput, wish: Vector3, wishLen: number, dt: number) {
    const T = this.tune;
    const look = this.look(new Vector3());
    const dir = new Vector3();
    if (input.moveZ !== 0) dir.addScaledVector(look, input.moveZ);
    if (input.moveX !== 0) dir.addScaledVector(this.right(new Vector3()), input.moveX);
    if (input.jump) dir.y += 1;
    if (input.descend || input.crouch) dir.y -= 1;
    const dl = Math.min(1, dir.length());
    if (dl > 1e-4) dir.normalize();
    const speed = this.vel.length();
    // steer toward wish at cruise speed
    this.accelerate(dir, dl * T.flyCruise, T.flyAccel / Math.max(1, T.flyCruise) * 3, dt);
    if (dl < 1e-4 && speed < T.flyCruise) this.vel.multiplyScalar(Math.max(0, 1 - 2.5 * dt)); // hover brake
    // above cruise: momentum decays slowly instead of being clamped
    if (speed > T.flyCruise) {
      const excess = speed - T.flyCruise;
      this.vel.multiplyScalar((speed - excess * T.flyOverspeedDrag * dt) / speed);
    }
    // diving converts altitude into speed
    if (dl > 0 && dir.y < -0.3) this.vel.addScaledVector(dir, T.gravity * T.flyDiveGravity * -dir.y * dt);
  }

  /** Energy-conserving glide: velocity direction bends toward the nose; gravity keeps acting. */
  private glide(dt: number) {
    const speed = this.vel.length();
    if (speed < 4) return;
    const look = this.look(new Vector3());
    const vdir = this.vel.clone().divideScalar(speed);
    const k = Math.min(1, this.mods.glide * dt * Math.min(1, speed / 20));
    vdir.lerp(look, k).normalize();
    this.vel.copy(vdir.multiplyScalar(speed * (1 - 0.02 * dt)));
    // lift partially cancels gravity at speed
    this.vel.y += Math.min(this.tune.gravity * 0.9, speed * 0.6) * this.mods.glide * 0.25 * dt;
  }

  private categorizeGround() {
    const R = this.world.R;
    this.groundBody = null;
    this.groundCollider = null;
    this.groundVel.set(0, 0, 0);
    const down = new Vector3(0, -1, 0);
    const hit = this.world.pw.castShape(this.pos, { x: 0, y: 0, z: 0, w: 1 }, down, this.shape, 0.02, 0.12, false, undefined, PLAYER_MOVE_FILTER, this.collider);
    let ground = false;
    if (hit) {
      const n = new Vector3(hit.normal1.x, hit.normal1.y, hit.normal1.z);
      if (n.y > 0.7) {
        const body = hit.collider.parent();
        const bv = body ? body.linvel() : { x: 0, y: 0, z: 0 };
        const relVy = this.vel.y - bv.y;
        if (relVy <= this.tune.groundMaxVy && !this.flying) {
          ground = true;
          this.groundNormal.copy(n);
          this.groundCollider = hit.collider;
          if (body && !body.isFixed()) {
            this.groundBody = body;
            this.groundVel.set(bv.x, bv.y, bv.z);
          }
          const sv = this.world.constructs.surfaceVelocity(hit.collider);
          if (sv) this.groundVel.add(sv);
          // snap down
          if (hit.time_of_impact > 0.005) this.pos.y -= hit.time_of_impact;
          const h = this.onHitCollider(hit.collider, n);
          if (h === 'handled') ground = false;
        }
      }
    }
    this.onGround = ground;
  }

  private onHitCollider(collider: RCollider, n: Vector3): 'clip' | 'ignore' | 'handled' {
    return this.onHit ? this.onHit(collider, n, this) : 'clip';
  }

  /** Quake PM_StepSlideMove: if a low ledge blocks us on the ground, try stepping over it. */
  private stepSlideMove(dt: number, grounded: boolean) {
    const p0 = this.pos.clone();
    const v0 = this.vel.clone();
    const blocked = this.slideMove(this.vel, dt, true);
    if (!blocked || !grounded || this.flying) return;
    const horizPlain = Math.hypot(this.pos.x - p0.x, this.pos.z - p0.z);
    const pA = this.pos.clone(), vA = this.vel.clone();
    const STEP = 0.55;
    const ident = { x: 0, y: 0, z: 0, w: 1 };
    // up
    const upHit = this.world.pw.castShape(p0, ident, { x: 0, y: 1, z: 0 }, this.shape, 0.01, STEP, false, undefined, PLAYER_MOVE_FILTER, this.collider);
    const up = upHit ? Math.max(0, upHit.time_of_impact - 0.01) : STEP;
    if (up < 0.1) return;
    this.pos.copy(p0).setY(p0.y + up);
    const vB = v0.clone().setY(Math.max(0, v0.y));
    this.slideMove(vB, dt, false);
    // down
    const downHit = this.world.pw.castShape(this.pos, ident, { x: 0, y: -1, z: 0 }, this.shape, 0.01, up + 0.05, false, undefined, PLAYER_MOVE_FILTER, this.collider);
    if (!downHit) { this.pos.copy(pA); this.vel.copy(vA); return; }
    const n = downHit.normal1;
    if (n.y < 0.7) { this.pos.copy(pA); this.vel.copy(vA); return; }
    this.pos.y -= Math.max(0, downHit.time_of_impact);
    const horizStep = Math.hypot(this.pos.x - p0.x, this.pos.z - p0.z);
    if (horizStep > horizPlain + 0.01) { this.vel.copy(vB); this.vel.y = Math.min(this.vel.y, 0); }
    else { this.pos.copy(pA); this.vel.copy(vA); }
  }

  /** Quake-style collide-and-slide using swept capsule casts. Returns true if a wall-like surface blocked us. */
  slideMove(vel: Vector3, dt: number, primary: boolean): boolean {
    let blocked = false;
    const planes: Vector3[] = [];
    let time = dt;
    const ignore = new Set<number>();
    const pred = (c: RCollider) => !ignore.has(c.handle);
    for (let bump = 0; bump < 6 && time > 1e-6; bump++) {
      const speed = vel.length();
      if (speed < 1e-6) break;
      const hit = this.world.pw.castShape(this.pos, { x: 0, y: 0, z: 0, w: 1 }, vel, this.shape, 0.01, time, false, undefined, PLAYER_MOVE_FILTER, this.collider, undefined, ignore.size ? pred : undefined);
      if (!hit) { this.pos.addScaledVector(vel, time); break; }
      const toi = Math.max(0, hit.time_of_impact);
      this.pos.addScaledVector(vel, toi);
      time -= toi;
      const n = new Vector3(hit.normal1.x, hit.normal1.y, hit.normal1.z);
      if (primary) {
        const res = this.onHitCollider(hit.collider, n);
        if (res === 'handled') { planes.length = 0; ignore.add(hit.collider.handle); continue; }
        if (res === 'ignore') { ignore.add(hit.collider.handle); continue; }
        if (this.collideDynamic(hit.collider, n)) { ignore.add(hit.collider.handle); continue; }
      }
      if (n.y < 0.7) blocked = true;
      planes.push(n);
      clipVelocity(vel, n);
      for (let i = 0; i < planes.length - 1; i++) {
        if (vel.dot(planes[i]) < -1e-4) {
          const crease = new Vector3().crossVectors(planes[i], n);
          if (crease.lengthSq() < 1e-8) { vel.set(0, 0, 0); break; }
          crease.normalize();
          vel.copy(crease.multiplyScalar(vel.dot(crease)));
        }
      }
      if (planes.length >= 3) break;
    }
    return blocked;
  }

  /** Momentum exchange with dynamic bodies (entities, free constructs). */
  private collideDynamic(collider: RCollider, n: Vector3): boolean {
    const body = collider.parent();
    if (!body || !body.isDynamic()) return false;
    const bv = body.linvel();
    const rel = new Vector3(this.vel.x - bv.x, this.vel.y - bv.y, this.vel.z - bv.z);
    const s = -rel.dot(n);
    if (s <= 0.5) return false;
    const m1 = this.tune.playerMass;
    const m2 = body.mass();
    const e = 0.2;
    const J = ((1 + e) * s) / (1 / m1 + 1 / m2);
    this.vel.addScaledVector(n, J / m1);
    body.applyImpulse({ x: -n.x * J, y: -n.y * J, z: -n.z * J }, true);
    this.world.onPlayerRam(collider, J, s);
    // if we are still moving into it (it was light), keep going and let physics separate us
    return true;
  }

  private depenetrate() {
    const R = this.world.R;
    this.body.setTranslation(this.pos, true);
    const hits: RCollider[] = [];
    this.world.pw.intersectionsWithShape(this.pos, { x: 0, y: 0, z: 0, w: 1 }, this.shape, (c) => { hits.push(c); return true; }, undefined, PLAYER_MOVE_FILTER, this.collider);
    for (const c of hits) {
      const contact = c.contactShape(this.shape, this.pos, { x: 0, y: 0, z: 0, w: 1 }, 0.0);
      if (!contact || contact.distance >= 0) continue;
      // normal1 is on collider c, pointing outward toward us
      const n = new Vector3(contact.normal1.x, contact.normal1.y, contact.normal1.z);
      this.pos.addScaledVector(n, -contact.distance + 0.005);
      const body = c.parent();
      if (body && !body.isFixed()) {
        const bv = body.linvel();
        const push = bv.x * n.x + bv.y * n.y + bv.z * n.z;
        const mine = this.vel.dot(n);
        if (push > mine) this.vel.addScaledVector(n, push - mine); // being shoved by a moving construct
      } else {
        clipVelocity(this.vel, n);
      }
    }
  }

  // ---------------- tethers ----------------

  private updateTetherInput(side: 'L' | 'R', held: boolean, prev: boolean) {
    if (held && !prev) this.fireTether(side);
    if (!held && prev) this.releaseTether(side);
  }

  fireTether(side: 'L' | 'R', constructId: string | null = null, targetPoint?: Vector3) {
    const R = this.world.R;
    const aim = targetPoint ? { point: targetPoint } : this.aim(this.tune.tetherRange * 1.5);
    const origin = this.handPos(side);
    const dir = new Vector3().subVectors(aim.point, origin);
    const len = dir.length();
    dir.divideScalar(Math.max(1e-6, len));
    const hit = this.world.pw.castRay(new R.Ray(origin, dir), this.tune.tetherRange, true, undefined, groups(G.PLAYER, G.WORLD | G.CONSTRUCT | G.ENTITY | G.CARGO), this.collider);
    const tether: Tether = {
      side, state: 'flying', origin: origin.clone(), target: new Vector3(), dir, travel: 0,
      dist: hit ? hit.timeOfImpact : Infinity, body: null, local: new Vector3(), length: 0, attachTime: 0, constructId,
    };
    if (hit) {
      tether.target.copy(origin).addScaledVector(dir, hit.timeOfImpact);
      const body = hit.collider.parent();
      if (body && !body.isFixed()) {
        tether.body = body;
        const bt = body.translation();
        const br = body.rotation();
        tether.local.copy(tether.target).sub(new Vector3(bt.x, bt.y, bt.z)).applyQuaternion(quatInv(br));
      }
    } else {
      tether.target.copy(origin).addScaledVector(dir, this.tune.tetherRange);
    }
    this.tethers[side] = tether;
    this.events.push('tether_fire_' + side);
    return tether;
  }

  /** Attach a rope directly (used by constructs that ARE the anchor, e.g. a grappling hook construct). */
  attachTether(side: 'L' | 'R', point: Vector3, body: RBody | null, constructId: string | null): Tether {
    const t: Tether = {
      side, state: 'attached', origin: point.clone(), target: point.clone(), dir: new Vector3().subVectors(point, this.pos).normalize(), travel: 0, dist: 0,
      body, local: new Vector3(), length: point.distanceTo(this.pos), attachTime: this.world.time, constructId,
    };
    if (body) {
      const bt = body.translation();
      t.local.copy(point).sub(new Vector3(bt.x, bt.y, bt.z)).applyQuaternion(quatInv(body.rotation()));
    }
    this.tethers[side] = t;
    this.events.push('tether_attach_' + side);
    return t;
  }

  releaseTether(side: 'L' | 'R') {
    if (this.tethers[side]) this.events.push('tether_release_' + side);
    this.tethers[side] = null;
  }

  handPos(side: 'L' | 'R', out = new Vector3()): Vector3 {
    const r = this.right(new Vector3());
    return out.copy(this.pos).addScaledVector(r, side === 'L' ? -0.45 : 0.45).add(new Vector3(0, 0.1, 0));
  }

  anchorPoint(t: Tether, out = new Vector3()): Vector3 {
    if (t.body) {
      if (!this.world.bodyAlive(t.body)) { t.body = null; return out.copy(t.target); }
      const bt = t.body.translation();
      const br = t.body.rotation();
      out.copy(t.local).applyQuaternion(br as any).add(new Vector3(bt.x, bt.y, bt.z));
      t.target.copy(out);
      return out;
    }
    return out.copy(t.target);
  }

  private advanceTethers(dt: number) {
    for (const side of ['L', 'R'] as const) {
      const t = this.tethers[side];
      if (!t) continue;
      if (t.state === 'flying') {
        t.travel += this.tune.tetherShotSpeed * dt;
        const hand = this.handPos(side);
        if (t.travel >= t.dist) {
          t.state = 'attached';
          t.attachTime = this.world.time;
          const a = this.anchorPoint(t);
          t.length = a.distanceTo(this.pos);
          t.origin.copy(a);
          this.events.push('tether_attach_' + side);
        } else if (t.travel >= this.tune.tetherRange) {
          this.tethers[side] = null; // missed
          this.events.push('tether_miss_' + side);
        } else {
          t.origin.copy(hand).addScaledVector(t.dir, t.travel);
        }
      } else {
        this.anchorPoint(t, t.origin);
      }
    }
  }

  private applyTetherForces(input: PlayerInput, dt: number, _any: boolean) {
    const T = this.tune;
    for (const side of ['L', 'R'] as const) {
      const t = this.tethers[side];
      if (!t || t.state !== 'attached') continue;
      const a = this.anchorPoint(t);
      const d = new Vector3().subVectors(a, this.pos);
      const dist = d.length();
      if (dist < 1.2) continue;
      d.divideScalar(dist);
      let pull = T.tetherPull;
      if (t.autoReel && dist < 3) {
        // arrived: let go with a little hop so momentum carries us over the ledge
        this.vel.y = Math.max(this.vel.y, 7);
        this.vel.addScaledVector(d.clone().setY(0).normalize(), 5);
        this.tethers[side] = null;
        this.events.push('tether_release_' + side);
        continue;
      }
      if (input.reel || t.autoReel) {
        pull += T.tetherReelPull;
        t.length = Math.max(1.5, Math.min(t.length, dist) - T.tetherReelRate * dt);
      }
      // ratchet: rope never lengthens by itself
      t.length = Math.min(t.length, Math.max(dist, 1.5));
      let dv = pull * dt;
      // winches reel at a controlled speed instead of slingshotting you into orbit
      if (t.autoReel) { const toward = this.vel.dot(d); if (toward > 16) dv = 0; this.vel.y = Math.max(this.vel.y, -2); }
      this.vel.addScaledVector(d, dv);
      if (this.onGround && d.y > 0.2) this.onGround = false;
      // reaction on dynamic anchors (yank drones / pull objects)
      if (t.body && this.world.bodyAlive(t.body) && t.body.isDynamic()) {
        const m2 = t.body.mass();
        const J = dv * T.playerMass * 0.8;
        t.body.applyImpulse({ x: -d.x * J, y: -d.y * J, z: -d.z * J }, true);
        void m2;
      }
    }
  }

  /** Inextensible rope: remove outward radial velocity and correct position (keeps tangential speed => swings/orbits). */
  private enforceTetherLength() {
    for (const side of ['L', 'R'] as const) {
      const t = this.tethers[side];
      if (!t || t.state !== 'attached') continue;
      const a = this.anchorPoint(t);
      const d = new Vector3().subVectors(this.pos, a);
      const dist = d.length();
      if (dist <= t.length || dist < 1e-4) continue;
      d.divideScalar(dist);
      const radial = this.vel.dot(d);
      if (radial > 0) this.vel.addScaledVector(d, -radial);
      const corr = d.multiplyScalar(-(dist - t.length));
      this.slideMove(corr.divideScalar(1 / 60), 1 / 60, false); // move back along rope, colliding
    }
  }
}

function clipVelocity(v: Vector3, n: Vector3) {
  const back = v.dot(n);
  if (back < 0) v.addScaledVector(n, -back * 1.001);
}

function quatInv(q: { x: number; y: number; z: number; w: number }) {
  // returns a THREE-compatible quaternion-like with conjugate
  return { x: -q.x, y: -q.y, z: -q.z, w: q.w, isQuaternion: true } as any;
}

export const _unused = UP;
