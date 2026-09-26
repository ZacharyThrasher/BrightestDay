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
   * box/wedge: full extents [x, y, z] (wedge rises toward +z: its tall end is at +z)
   * sphere: [d, d, d]; cylinder/capsule/cone: [diameter, height, diameter] along local y
   * torus: [outerDiameter, tubeDiameter, outerDiameter] lying in local XZ plane
   * dome: [diameter, height, diameter] hemisphere shell, open at the bottom
   */
  size: Vec3;
  pos: Vec3;
  rot?: Vec3; // euler XYZ, degrees
  role?: string; // semantic tag: body, finger, thruster, wheel, blade, pad, canopy, head, handle, spike, barrel, muzzle, wing, leg, seat...
  solid?: boolean; // default true; false = visual only
  /** Articulation: parts sharing a group move together with that group's Joint (see ConstructSpec.joints). */
  group?: string;
}

/**
 * How an articulated group moves relative to its parent (the construct body, or another group).
 * All angles in degrees, rates in cycles/second (spin: revolutions/second), distances in metres (unscaled part units).
 * `phase` is a fraction of a cycle (0..1; 0.5 = opposite legs). Axes are in the construct frame (+z forward, +y up, +x right).
 * Several motions on one joint compose in order (e.g. steer then roll; aim then a recoil piston).
 * `on: 'fire'` (swing/piston): instead of cycling continuously, play one cycle each time a weapon mounted in this
 * group (or a child group) fires — recoil, bites, punches.
 */
export type JointMotion =
  | { kind: 'spin'; axis: Vec3; rate: number } // constant rotation: rotors, propellers, drills, saw blades, fans
  | { kind: 'roll'; axis: Vec3; radius: number } // wheels / treads: angle follows the construct's ground speed
  | { kind: 'steer'; axis: Vec3; amp: number } // front wheels / rudders: turn with the construct's turning rate (±amp)
  | { kind: 'aim'; yaw?: boolean; pitch?: boolean; rate: number; minPitch?: number; maxPitch?: number } // turrets, heads, cannons: track the brain's target (else the rider's / player's aim); rate = deg/s
  | { kind: 'swing'; axis: Vec3; amp: number; rate: number; phase?: number; gait?: boolean; on?: 'fire' } // legs, wings, arms, tails, flippers, jaws; gait = scale with speed (legs)
  | { kind: 'piston'; axis: Vec3; amp: number; rate: number; phase?: number; gait?: boolean; on?: 'fire' } // pumps, jackhammers, recoil, bobbing heads
  | { kind: 'wave'; axis: Vec3; amp: number; rate: number; phase?: number }; // like swing, plus an automatic phase lag per chain depth (tails, tentacles, snakes)

export interface Joint {
  group: string; // parts with part.group === group are driven by this joint
  pivot: Vec3; // pivot point in construct space (unscaled)
  parent?: string; // another group (e.g. barrel group parented to turret group)
  motion: JointMotion[];
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
  | { type: 'launch'; speed: number; dir: 'normal' | 'up' | 'forward' | 'arc'; target?: Ref; clearY?: number }
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
  | { type: 'pulse'; period: number }
  // ---- machines & creatures (locomotion + autonomy + weapons + riding) ----
  /**
   * Locomotion capability. ground: follows terrain, wheels/legs; air: flies (altitude hold); hover: floats above ground.
   * speed m/s, turn deg/s, altitude m above ground (air/hover), jump m/s take-off speed, minSpeed m/s (fixed-wing: never slower in the air).
   */
  | { type: 'drive'; mode: 'ground' | 'air' | 'hover'; speed: number; turn: number; altitude?: number; jump?: number; minSpeed?: number }
  /**
   * Autonomy: what the construct decides to do on its own (moves via 'drive', attacks via 'weapon'). range = engage radius (m).
   * target: hunt → entity to attack (default: nearest hostile); follow → who (default: player); guard/patrol/goto → spot;
   * fetch → object to bring (default: nearest loose object); flee → what to run from (default: nearest hostile).
   */
  | { type: 'brain'; goal: 'wander' | 'follow' | 'guard' | 'hunt' | 'fetch' | 'patrol' | 'goto' | 'flee' | 'idle'; target?: Ref; range?: number }
  /**
   * A weapon fired from the 'muzzle' parts in `group` or its child groups (or the construct front) at the brain's target /
   * rider's aim. rate shots/s, speed m/s, damage per hit, range m. laser/flame are instant beams/cones, melee hits what it reaches.
   */
  | { type: 'weapon'; kind: 'gun' | 'cannon' | 'laser' | 'missile' | 'flame' | 'net' | 'melee' | 'bomb'; group?: string; rate: number; speed: number; damage: number; range: number; size?: number; spread?: number }
  /**
   * The player can get in/on it: WASD drives it (needs 'drive'), mouse aims its 'aim' joints, fire buttons fire its weapons.
   * seat = where the rider's body centre goes (construct frame, unscaled). Placed at 'self', the player is put in it on creation.
   */
  | { type: 'rideable'; seat: Vec3; view?: 'third' | 'first' };

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
  /** Articulated groups (optional). Constructs without joints are rigid, as before. */
  joints?: Joint[];
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
