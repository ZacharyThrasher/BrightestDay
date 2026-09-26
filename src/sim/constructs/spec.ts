/**
 * Declarative construct description.
 *
 * The semantic layer (rules, lexicon, embeddings, LLM) may only ever PROPOSE one of these.
 * validate.ts clamps it to the resource envelope, and runtime.ts is the only thing that
 * turns it into physics. Nothing here is executable.
 */

export type Shape = 'box' | 'sphere' | 'cylinder' | 'capsule' | 'cone' | 'wedge' | 'torus' | 'dome';

export type Vec3 = [number, number, number];

export interface Part {
  shape: Shape;
  /**
   * box/wedge: full extents [x, y, z] (wedge rises toward -z, i.e. its tall end is at the back)
   * sphere: [d, d, d]; cylinder/capsule/cone: [diameter, height, diameter] along local y
   * torus: [outerDiameter, tubeDiameter, outerDiameter] lying in local XZ plane
   * dome: [diameter, height, diameter] hemisphere shell, open at the bottom
   */
  size: Vec3;
  pos: Vec3;
  rot?: Vec3; // euler XYZ, degrees
  role?: string; // semantic tag: body, finger, thruster, wheel, blade, pad, canopy, head, handle, spike, barrel, wing...
  solid?: boolean; // default true; false = visual only
}

export type Ref =
  | { kind: 'self' }
  | { kind: 'aim' } // whatever is under the crosshair at commit time (entity if any, else point)
  | { kind: 'entity'; id: string }
  | { kind: 'entities'; ids: string[] }
  | { kind: 'construct'; id: string }
  | { kind: 'point'; p: Vec3 }
  | { kind: 'nearest_enemy' }
  | { kind: 'all_enemies' }
  | { kind: 'threat' } // nearest incoming projectile source
  | { kind: 'none' };

export type Behavior =
  | { type: 'hold'; side?: 'left' | 'right' | 'front' | 'back' | 'overhead' | 'feet' }
  | { type: 'follow'; target: Ref; offset?: Vec3 }
  | { type: 'orbit'; around: Ref; radius: number; speed: number; height?: number }
  | { type: 'strike'; target: Ref; speed: number; after: 'return' | 'stay' | 'dissolve'; repeat?: number }
  | { type: 'grab'; target: Ref; then: 'hold' | 'bring' | 'lift' | 'stay' }
  | { type: 'throw'; target: Ref; speed: number }
  | { type: 'thrust'; accel: number; axis: Vec3 }
  | { type: 'spin'; rate: number; axis: Vec3 }
  | { type: 'launch'; speed: number; dir: 'normal' | 'up' | 'forward' | 'arc' }
  | { type: 'cushion' }
  | { type: 'field'; mode: 'attract' | 'repel' | 'wind' | 'lift'; radius: number; strength: number; axis?: Vec3 }
  | { type: 'reflect' }
  | { type: 'conveyor'; speed: number; axis: Vec3 }
  | { type: 'shoot'; target: Ref; rate: number; speed: number; size: number }
  | { type: 'home'; target: Ref; speed: number }
  | { type: 'restrain'; target: Ref }
  | { type: 'crush'; target: Ref }
  | { type: 'mount'; mode: 'wear' | 'ride'; glide?: number; groundSpeedMul?: number; frictionMul?: number; thrustLook?: number; thrustUp?: number; maxFall?: number; bounce?: number }
  | { type: 'tether'; target: Ref; pull: number; toPlayer: boolean }
  | { type: 'lift'; target: Ref; height: number }
  | { type: 'explode'; radius: number; force: number; delay: number; onContact: boolean }
  | { type: 'sweep'; width: number; speed: number }
  | { type: 'pulse'; period: number };

export type BehaviorType = Behavior['type'];

export type PlacementAt =
  | 'aim' // at the crosshair point (on the ground if the ground is hit)
  | 'front' // in front of the player at chest height
  | 'ground_front' // on the ground ahead, oriented along travel (ramps)
  | 'self' // centred on the player
  | 'under_self' // under the player's feet
  | 'behind_self'
  | 'above_self'
  | 'hand' // at the ring hand
  | 'target' // at the target
  | 'under_target' // on the ground below the target / just below it if airborne
  | 'above_target'
  | 'around_target'
  | 'between' // between player and threat/target
  | 'span'; // from player to aim point (bridges, rails)

export interface Placement {
  at: PlacementAt;
  target?: Ref;
  orient: 'face_away' | 'face_player' | 'upright' | 'along_look' | 'vertical' | 'flat' | 'along_travel' | 'toward_target';
  distance?: number;
  height?: number;
  tilt?: number; // extra pitch in degrees (e.g. "turn it vertical" = 90)
  yaw?: number; // extra yaw in degrees
}

export type Arrangement = 'single' | 'line' | 'ring' | 'grid' | 'stack' | 'arc' | 'scatter' | 'wall';

export interface ConstructSpec {
  concept: string; // lexicon concept id(s) it came from, e.g. "glove+rocket"
  label: string; // human-readable ("giant boxing glove with rockets")
  parts: Part[];
  scale: number;
  stretch?: Vec3; // non-uniform scale from "longer/wider/taller"
  count: number;
  arrangement: Arrangement;
  spacing: number; // metres between instances (before scale)
  motion: 'static' | 'dynamic' | 'kinematic';
  density: number; // kg/m^3 for dynamic
  restitution: number;
  friction: number;
  soft: boolean;
  passPlayer: boolean; // player can move through it (bubbles around self)
  placement: Placement;
  behaviors: Behavior[];
  lifetime: number;
  color?: string; // optional tint name
  provenance: {
    source: 'rule' | 'lexicon' | 'embedding' | 'llm' | 'cache' | 'fallback' | 'edit';
    confidence: number; // 0..1, as claimed by the proposer (not trusted)
    approximatedFrom?: string; // the player's word(s) that were approximated
    notes: string[];
  };
}

export function cloneSpec(s: ConstructSpec): ConstructSpec {
  return JSON.parse(JSON.stringify(s));
}

export function defaultSpec(partial: Partial<ConstructSpec> = {}): ConstructSpec {
  return {
    concept: 'block',
    label: 'block',
    parts: [{ shape: 'box', size: [1, 1, 1], pos: [0, 0, 0] }],
    scale: 1,
    count: 1,
    arrangement: 'single',
    spacing: 1.5,
    motion: 'static',
    density: 300,
    restitution: 0.1,
    friction: 0.6,
    soft: false,
    passPlayer: false,
    placement: { at: 'aim', orient: 'face_away' },
    behaviors: [],
    lifetime: 60,
    provenance: { source: 'rule', confidence: 1, notes: [] },
    ...partial,
  };
}

/** Local bounding box half-extents of a part list (unscaled). */
export function partsBounds(parts: Part[]): { min: Vec3; max: Vec3 } {
  const min: Vec3 = [Infinity, Infinity, Infinity];
  const max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const p of parts) {
    const r = Math.max(p.size[0], p.size[1], p.size[2]) / 2;
    const ext: Vec3 = p.rot && (p.rot[0] || p.rot[1] || p.rot[2]) ? [r, r, r] : [p.size[0] / 2, p.size[1] / 2, p.size[2] / 2];
    for (let i = 0; i < 3; i++) {
      min[i] = Math.min(min[i], p.pos[i] - ext[i]);
      max[i] = Math.max(max[i], p.pos[i] + ext[i]);
    }
  }
  if (!parts.length) return { min: [0, 0, 0], max: [0, 0, 0] };
  return { min, max };
}

export function partVolume(p: Part): number {
  const [x, y, z] = p.size;
  switch (p.shape) {
    case 'sphere': return (Math.PI / 6) * x * y * z;
    case 'cylinder': case 'capsule': return (Math.PI / 4) * x * z * y;
    case 'cone': return (Math.PI / 12) * x * z * y;
    case 'wedge': return 0.5 * x * y * z;
    case 'torus': return Math.PI * Math.PI * (x / 2) * (y / 2) * (y / 2) / 2;
    case 'dome': return 0.15 * x * y * z;
    default: return x * y * z;
  }
}
