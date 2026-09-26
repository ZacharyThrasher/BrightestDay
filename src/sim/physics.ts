import RAPIER from '@dimforge/rapier3d-compat';

export type Rapier = typeof RAPIER;
export type RWorld = InstanceType<Rapier['World']>;
export type RBody = InstanceType<Rapier['RigidBody']>;
export type RCollider = InstanceType<Rapier['Collider']>;

let ready: Promise<Rapier> | null = null;

/** Initialise the Rapier WASM module once (works in Node and in the browser). */
export function initPhysics(): Promise<Rapier> {
  if (!ready) ready = (RAPIER as any).init({}).then(() => RAPIER);
  return ready!;
}

/** Collision membership bits. */
export const G = {
  WORLD: 1,
  PLAYER: 2,
  CONSTRUCT: 4,
  ENTITY: 8,
  PROJECTILE: 16,
  CARGO: 32, // entities currently held by a construct (don't collide with their holder)
  GHOST: 64, // constructs the player can pass through (held items, bubbles around self, mounts)
  DEBRIS: 128, // short-lived broken pieces (collide with world/constructs/entities, never the player or aim)
} as const;

/** Things projectiles / entities / aim can hit. */
export const SOLIDS = G.WORLD | G.CONSTRUCT | G.GHOST | G.ENTITY;

export const ALL = 0xffff;

/** Rapier interaction groups: high 16 bits = membership, low 16 = filter. */
export function groups(member: number, filter: number): number {
  return ((member & 0xffff) << 16) | (filter & 0xffff);
}

/**
 * Per-player membership bits (PvP): every lantern's capsule is a member of G.PLAYER plus its own bit, so a
 * query/projectile can include every player except one (a construct's shots never touch their owner).
 * Bits 256..32768 → up to 8 players.
 */
export const PLAYER_BITS = 0xff00;
export function playerBit(index: number): number { return (256 << Math.max(0, Math.min(7, index))) & PLAYER_BITS; }

/** What the player's movement collides with (other lanterns body-block). */
export const PLAYER_MOVE_FILTER = groups(G.PLAYER, G.WORLD | G.CONSTRUCT | G.ENTITY | G.PLAYER);
