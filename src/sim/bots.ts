/**
 * Bot lanterns: scripted / AI players that live inside the deterministic simulation.
 *
 * A BotController is asked, at the start of every world tick, for its player's PlayerInput — exactly what a
 * human's keyboard/mouse (or a remote client) would produce — and may type commands through a CommandSink,
 * which in the app is that player's own Director (the same System-1 path humans use; see
 * src/lang/practice.ts). Everything here is deterministic: no Math.random, no wall clock; randomness comes
 * from a seeded generator keyed by the player id, so two runs of a scripted duel are identical.
 *
 * Attach with `world.controllers.set(bot.playerId, bot)` (or `attachPracticeBot` in src/lang/practice.ts).
 */
import { Vector3 } from 'three';
import { NO_INPUT, type PlayerInput } from './player.js';
import type { World } from './world.js';

/** Where a bot types: the host runs it through that player's Director synchronously (commitSync). */
export interface CommandSink { submit(text: string): unknown }

export interface BotController {
  /** The player this controller drives. */
  readonly playerId: string;
  /** Called at the start of every world tick; returns this tick's input for the player. May submit commands. */
  tick(w: World): PlayerInput;
  /** Type a command into this lantern's ring (same path as a human's Enter). */
  submit?(text: string): unknown;
}

/** Deterministic PRNG (mulberry32) seeded from a string. */
export function seededRng(seed: string): () => number {
  let s = 0x9e3779b9;
  for (let i = 0; i < seed.length; i++) s = Math.imul(s ^ seed.charCodeAt(i), 0x85ebca6b) | 0;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Yaw / pitch that points a player's view at a world point (approximate: from the player's head). */
export function lookAt(from: Vector3, to: Vector3): { yaw: number; pitch: number } {
  const dx = to.x - from.x, dy = to.y - from.y, dz = to.z - from.z;
  return { yaw: Math.atan2(-dx, -dz), pitch: Math.atan2(dy, Math.max(1e-3, Math.hypot(dx, dz))) };
}

export interface PracticeBotOpts {
  /** commands it types, in a seeded-random order */
  commands: string[];
  /** seconds between casts (± 35 %) */
  castEvery: number;
  /** first cast after this many seconds of live play */
  firstCast: number;
  /** aim wobble (radians) */
  aimError: number;
  /** preferred fighting distance (m) */
  range: [number, number];
}

export const PRACTICE_COMMANDS = [
  'punch him', 'a wall', 'drop a piano on him', 'a cannon that shoots him', 'grab him and throw him',
  'throw a boulder at him', 'a trampoline', 'smash him with a hammer', 'a missile that chases him',
];

/**
 * The practice opponent: strafes, jumps, keeps a middle distance, faces you, and every few seconds types
 * one of a few fixed commands. Deliberately simple — a smarter rival is built on the same interface.
 */
export class PracticeBot implements BotController {
  readonly opts: PracticeBotOpts;
  private rng: () => number;
  private strafe = 1;
  private nextFlip = 0;
  private nextJump = 0;
  private nextCast = -1;
  private jumpHeld = 0;
  private order: string[] = [];
  /** what it typed (tests / HUD) */
  readonly typed: { t: number; text: string }[] = [];

  constructor(readonly playerId: string, private sink: CommandSink | null = null, opts: Partial<PracticeBotOpts> = {}) {
    this.opts = { commands: PRACTICE_COMMANDS, castEvery: 5.5, firstCast: 2.5, aimError: 0.05, range: [12, 30], ...opts };
    this.rng = seededRng(`bot:${playerId}`);
  }

  submit(text: string) {
    this.typed.push({ t: -1, text });
    return this.sink?.submit(text);
  }

  tick(w: World): PlayerInput {
    const me = w.playerById(this.playerId);
    if (!me) return NO_INPUT;
    const t = w.time;
    if (!me.alive) { this.nextCast = -1; return { ...NO_INPUT, yaw: me.yaw, pitch: me.pitch }; }
    const opp = w.opponentOf(this.playerId);
    let yaw = me.yaw, pitch = me.pitch;
    let dist = 20;
    if (opp) {
      const head = me.pos.clone().add(new Vector3(0, 0.75, 0));
      const l = lookAt(head, opp.pos);
      yaw = l.yaw + Math.sin(t * 1.3 + 0.7) * this.opts.aimError;
      pitch = l.pitch + Math.sin(t * 0.9) * this.opts.aimError * 0.5;
      dist = me.pos.distanceTo(opp.pos);
    }
    // strafe left/right, flipping at seeded intervals
    if (t >= this.nextFlip) { this.strafe = this.rng() < 0.5 ? -1 : 1; this.nextFlip = t + 0.7 + this.rng() * 1.5; }
    let moveZ = 0;
    if (dist > this.opts.range[1]) moveZ = 1;
    else if (dist < this.opts.range[0]) moveZ = -1;
    // don't hug the arena walls
    if (Math.abs(me.pos.x) > 38 || Math.abs(me.pos.z) > 40) moveZ = 1;
    // hop now and then (and chain a hop or two: bunny-hop)
    let jump = false;
    if (this.jumpHeld > 0) { jump = true; this.jumpHeld--; }
    else if (t >= this.nextJump && me.onGround) { jump = true; this.jumpHeld = 2; this.nextJump = t + 0.9 + this.rng() * 2.2; }
    // casts: only while the round is live (or in free play), the rival is up, and we can act
    const live = !w.match || w.match.phase === 'live';
    if (live && opp && opp.alive) {
      if (this.nextCast < 0) this.nextCast = t + this.opts.firstCast + this.rng() * 1.5;
      if (t >= this.nextCast && !me.heldBy) {
        if (!this.order.length) this.order = shuffle(this.opts.commands.slice(), this.rng);
        const text = this.order.pop()!;
        this.typed.push({ t, text });
        this.sink?.submit(text);
        this.nextCast = t + this.opts.castEvery * (0.65 + this.rng() * 0.7);
      }
    } else if (!live) this.nextCast = -1;
    return { ...NO_INPUT, yaw, pitch, moveX: this.strafe, moveZ, jump };
  }
}

function shuffle<T>(a: T[], rng: () => number): T[] {
  for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(rng() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; }
  return a;
}
