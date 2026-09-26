/**
 * Combat catalog, part 1: WEAPONS — committed strikes and tracking chasers / ranged turrets.
 *
 * Tiers (docs/PVP_DESIGN.md "Speed vs. hitting"):
 *  - STRIKES ride the classic 'strike' behaviour: 45–60 m/s, the intercept point is locked at launch (a juke dodges
 *    them), they come back to your hand and strike again (repeat).
 *  - CHASERS are machines: turrets lead their target with the weapon's shot speed (bullets, beams, homing missiles),
 *    creatures/drones chase at ≤ the chaser speed cap with a limited turn rate.
 * Nothing here can hurt its owner: machine weapons go through MachineSystem.dealDamage (never the owner) and
 * strikes only hit entities / other owners' things.
 *
 * Frame: +z forward (for hand-held weapons: along your look, the handle toward you), +y up, metres, degrees.
 *
 * Names: the semantic layer embeds an entry's id + its first 11 names to approximate unknown words, so those are
 * kept distinctive (typos/plurals pad them); names made of everyday words ("big hammer", "lava pool", "iron
 * gate") come after them, where they still match exactly but don't pull unrelated words ("big", "pool", "iron").
 */
import type { CatalogEntry } from './types.js';
import type { Part, Joint, Behavior, Ref, Vec3 } from '../spec.js';
import { P, J, spin, swing, snap, recoil, aimYaw, aimPitch, aimBoth, roll, bob, limb } from './kit.js';

// ---------------------------------------------------------------- shared helpers (also used by traps.ts / siege.ts)

type Weapon = Extract<Behavior, { type: 'weapon' }>;

/** The attack target the language layer retargets ("…at him" / the aimed thing); defaults to the nearest enemy. */
export const FOE: Ref = { kind: 'nearest_enemy' };

/** Committed strike: locked intercept at launch, then back to the hand and again `repeat` times. */
export const strikeB = (speed: number, after: 'return' | 'stay' | 'dissolve' = 'return', repeat = 0): Behavior =>
  ({ type: 'strike', target: FOE, speed, after, ...(repeat ? { repeat } : {}) });

/** Stationary sentry brain: engages the nearest enemy within `range` of where it stands. */
export const guardB = (range: number): Behavior => ({ type: 'brain', goal: 'guard', range });
/** Mobile hunter brain. */
export const huntB = (range = 50): Behavior => ({ type: 'brain', goal: 'hunt', target: FOE, range });

export const weaponB = (kind: Weapon['kind'], group: string | undefined, rate: number, speed: number, damage: number, range: number, extra: { size?: number; spread?: number } = {}): Behavior => {
  const w: Weapon = { type: 'weapon', kind, rate, speed, damage, range };
  if (group) w.group = group;
  if (extra.size !== undefined) w.size = extra.size;
  if (extra.spread !== undefined) w.spread = extra.spread;
  return w;
};

/** Invisible muzzle marker: where shots / bites come from. */
export const muzzle = (pos: Vec3, group: string, d = 0.22): Part => P('sphere', [d, d, d], pos, { role: 'muzzle', group, solid: false });

/** Visual-only part. */
export const V = (shape: Part['shape'], size: Vec3, pos: Vec3, role: string, group?: string, rot?: Vec3): Part =>
  P(shape, size, pos, { role, solid: false, ...(group ? { group } : {}), ...(rot ? { rot } : {}) });

/** A ring of `n` parts made by `make(angleDeg, x, z)` around the y axis. */
export const around = (n: number, r: number, make: (a: number, x: number, z: number, i: number) => Part): Part[] =>
  Array.from({ length: n }, (_, i) => { const a = (i / n) * 360; const rad = (a * Math.PI) / 180; return make(a, Math.sin(rad) * r, Math.cos(rad) * r, i); });

/** [angleDeg, x, z] for `n` points on a circle of radius r around the y axis. */
export const ring = (n: number, r: number): [number, number, number][] =>
  Array.from({ length: n }, (_, i) => { const a = (i / n) * 360; const rad = (a * Math.PI) / 180; return [a, Math.sin(rad) * r, Math.cos(rad) * r]; });

/**
 * Put the construct origin at the gun's pivot (parts and pivots move down by `dy`; placement still stands it on
 * the ground). Machine weapons lead their target from the construct origin, so a pivot-centred turret leads
 * from (nearly) where its shots start — a base-centred one would trail a fast crosser by a metre.
 */
export function gunAtOrigin(e: CatalogEntry, dy: number): CatalogEntry {
  const build = e.build;
  e.build = () => {
    const b = build();
    for (const p of b.parts) p.pos = [p.pos[0], +(p.pos[1] - dy).toFixed(4), p.pos[2]];
    for (const j of b.joints) j.pivot = [j.pivot[0], +(j.pivot[1] - dy).toFixed(4), j.pivot[2]];
    return b;
  };
  return e;
}

/*
 * FIRE_FROM_PIVOT: fast guns keep their muzzle marker at the aim pivot (the shot still leaves along the barrel and
 * is pushed clear of the gun's own parts). The machine runtime leads a shot as if it started at the construct
 * origin, so a muzzle 3 m out along a 3 m barrel would arrive early and pass in front of a fast crosser.
 */

/** Squat pedestal a turret head sits on (solid base so it can be shot / smashed). Top at y = h. */
export function pedestal(h: number, w = 1.8): Part[] {
  return [
    P('cylinder', [w, 0.35, w], [0, 0.175, 0], { role: 'base' }),
    P('cylinder', [w * 0.45, h - 0.35, w * 0.45], [0, 0.35 + (h - 0.35) / 2, 0], { role: 'column' }),
    ...around(3, w * 0.55, (a, x, z) => V('box', [0.18, 0.2, w * 0.5], [x, 0.1, z], 'foot', undefined, [0, a, 0])),
  ];
}

// ---------------------------------------------------------------- STRIKES (committed, dodgeable by a juke)

const HOLD: Behavior = { type: 'hold', side: 'right' };
const STRIKE_FNS = ['strike_target', 'push_target', 'damages_target', 'attacks_enemies', 'has_moving_parts'];

/** Thor-sized war hammer: wobbles as it winds up, rune rings spin round the head; smashes 4 times. */
export const thunderHammer: CatalogEntry = {
  id: 'thunder_hammer',
  names: ['mjolnir', 'mjölnir', 'thors hammer', "thor's hammer", 'thor hammer', 'thunder hammer', 'banhammer', 'sledge hammer', 'sludge hammer', 'storm hammer', 'giant hammer', 'huge hammer', 'big hammer', 'massive hammer', 'ban hammer', 'hammer of doom'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.6, density: 3000, lifetime: 30,
  build() {
    const g = 'hammer';
    const parts: Part[] = [
      P('cylinder', [0.26, 2.8, 0.26], [0, 0, -1.2], { role: 'handle', rot: [90, 0, 0], group: g }),
      P('sphere', [0.42, 0.42, 0.42], [0, 0, -2.65], { role: 'pommel', group: g }),
      P('box', [2.2, 1.1, 1.2], [0, 0, 0.55], { role: 'head', group: g }),
      V('box', [2.35, 0.3, 1.3], [0, 0, 0.55], 'band', g),
      V('torus', [1.9, 0.12, 1.9], [0, 0, 0.55], 'rune', 'runes', [0, 0, 90]),
      V('torus', [1.5, 0.1, 1.5], [0, 0, 0.55], 'rune', 'runes', [0, 90, 90]),
    ];
    return { parts, joints: [J(g, [0, 0, -2.4], [swing(14, 1.1, [1, 0, 0])]), J('runes', [0, 0, 0.55], [spin(1.2, [1, 0, 0])], g)] };
  },
  behaviors: [strikeB(55, 'return', 3), HOLD],
  functions: [...STRIKE_FNS, 'returns'],
  verb: 'strike',
};

/** Double-bitted war axe that tumbles end over end: thrown, chops, comes back, twice more. */
export const warAxe: CatalogEntry = {
  id: 'war_axe',
  names: ['waraxe', 'war axe', 'greataxe', 'viking axe', 'dwarven axe', 'dwarf axe', 'labrys', 'bearded axe', 'executioner axe', "executioner's axe", 'war axes', 'great axe', 'giant axe', 'double axe', 'throwing axe', 'axe of doom', 'tomahawk of light'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.5, density: 2500, lifetime: 30,
  build() {
    const g = 'axe';
    const parts: Part[] = [
      P('cylinder', [0.2, 2.6, 0.2], [0, 0, -0.9], { role: 'handle', rot: [90, 0, 0], group: g }),
      P('wedge', [0.18, 1.2, 1.1], [0, 0.72, 0.3], { role: 'blade', group: g }),
      P('wedge', [0.18, 1.2, 1.1], [0, -0.72, 0.3], { role: 'blade', rot: [180, 0, 0], group: g }),
      V('cone', [0.2, 0.5, 0.2], [0, 0, 0.6], 'spike', g, [90, 0, 0]),
    ];
    return { parts, joints: [J(g, [0, 0, -0.2], [spin(2.2, [1, 0, 0])])] };
  },
  behaviors: [strikeB(50, 'return', 2), HOLD],
  functions: [...STRIKE_FNS, 'returns', 'spin'],
  verb: 'strike',
};

/** Two-handed greatsword that slashes back and forth; a five-hit combo. */
export const greatsword: CatalogEntry = {
  id: 'greatsword',
  names: ['zweihander', 'zweihänder', 'buster sword', 'broadsword', 'flaming sword', 'energy sword', 'excalibur', 'greatsowrd', 'great sword', 'greatswords', 'great swords', 'master sword', 'giant sword', 'huge sword', 'broad sword', 'two handed sword', 'two-handed sword', 'bastard sword', 'sword of light'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.4, density: 1800, lifetime: 30,
  build() {
    const g = 'sword';
    const parts: Part[] = [
      P('box', [0.16, 0.6, 4.2], [0, 0, 1.0], { role: 'blade', group: g }),
      V('wedge', [0.16, 0.6, 0.6], [0, 0, 3.4], 'tip', g),
      P('box', [1.3, 0.2, 0.22], [0, 0, -1.2], { role: 'guard', group: g }),
      P('cylinder', [0.2, 1.0, 0.2], [0, 0, -1.8], { role: 'handle', rot: [90, 0, 0], group: g }),
      V('sphere', [0.34, 0.34, 0.34], [0, 0, -2.35], 'pommel', g),
    ];
    return { parts, joints: [J(g, [0, 0, -1.8], [swing(40, 1.4, [0, 1, 0])])] };
  },
  behaviors: [strikeB(60, 'return', 4), HOLD],
  functions: [...STRIKE_FNS, 'returns'],
  verb: 'strike',
};

/** Jousting lance: the shaft pumps forward like a piston; charges in, twice. */
export const lance: CatalogEntry = {
  id: 'lance',
  names: ['jousting lance', 'cavalry lance', 'war lance', 'lanse', 'lances', 'jousting lances', 'lance of light', 'lanc', 'lancee', 'war lances', 'cavalry lances', 'pike', 'jousting pole', 'joust'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.4, density: 1500, lifetime: 30,
  build() {
    const g = 'lance';
    const parts: Part[] = [
      P('cone', [0.7, 5.0, 0.7], [0, 0, 1.6], { role: 'shaft', rot: [90, 0, 0], group: g }),
      P('cone', [1.3, 0.9, 1.3], [0, 0, -1.2], { role: 'vamplate', rot: [-90, 0, 0], group: g }),
      V('cylinder', [0.2, 1.2, 0.2], [0, 0, -2.1], 'handle', g, [90, 0, 0]),
      V('torus', [0.7, 0.08, 0.7], [0, 0, 0.3], 'band', g, [90, 0, 0]),
    ];
    return { parts, joints: [J(g, [0, 0, -1.2], [{ kind: 'piston', axis: [0, 0, 1], amp: 0.9, rate: 2.2 }])] };
  },
  behaviors: [strikeB(60, 'return', 2), HOLD],
  functions: [...STRIKE_FNS, 'returns'],
  verb: 'strike',
};

/** Flail: a spiked ball whirling round on its chain; lashes out four times. */
export const flail: CatalogEntry = {
  id: 'flail',
  names: ['morningstar', 'spiked flail', 'war flail', 'flale', 'chain mace', 'meteor hammer', 'flails', 'morningstars', 'flial', 'spiked flails', 'war flails', 'morning star', 'ball and chain', 'ball on a chain', 'mace and chain'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.4, density: 2800, lifetime: 30,
  build() {
    const g = 'whirl';
    const parts: Part[] = [
      P('cylinder', [0.24, 1.8, 0.24], [0, 0, -1.2], { role: 'handle', rot: [90, 0, 0] }),
      V('sphere', [0.36, 0.36, 0.36], [0, 0, -0.25], 'cap'),
      ...[0.35, 0.75, 1.15].map((y, i) => V('torus', [0.34, 0.08, 0.34], [0, y, 0.1], 'link', g, i % 2 ? [0, 0, 90] : [0, 90, 0])),
      P('sphere', [1.2, 1.2, 1.2], [0, 1.9, 0.1], { role: 'head', group: g }),
      ...[[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, 0, 1], [0, 0, -1]].map(([x, y, z]) => V('cone', [0.28, 0.5, 0.28], [x * 0.7, 1.9 + y * 0.7, 0.1 + z * 0.7], 'spike', g, x ? [0, 0, -90 * x] : z ? [90 * z, 0, 0] : undefined)),
    ];
    return { parts, joints: [J(g, [0, 0, 0.1], [spin(1.6, [0, 0, 1])])] };
  },
  behaviors: [strikeB(48, 'return', 3), HOLD],
  functions: [...STRIKE_FNS, 'returns', 'spin'],
  verb: 'strike',
};

/** Boomerang: whirls out, hits, curves home, three throws. */
export const boomerang: CatalogEntry = {
  id: 'boomerang',
  names: ['boomarang', 'boomerrang', 'bumerang', 'giant boomerang', 'battle boomerang', 'boomerangs', 'boomrang', 'bomerang', 'boomerange', 'boomarangs', 'boomerrangs', 'returning blade'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.5, density: 900, lifetime: 30,
  build() {
    const g = 'rang';
    const parts: Part[] = [
      P('box', [1.9, 0.12, 0.45], [-0.7, 0, 0.35], { role: 'wing', rot: [0, 35, 0], group: g }),
      P('box', [1.9, 0.12, 0.45], [0.7, 0, 0.35], { role: 'wing', rot: [0, -35, 0], group: g }),
      V('sphere', [0.5, 0.14, 0.5], [0, 0, 0.9], 'elbow', g),
    ];
    return { parts, joints: [J(g, [0, 0, 0.35], [spin(3, [0, 1, 0])])] };
  },
  behaviors: [strikeB(50, 'return', 2), HOLD],
  functions: [...STRIKE_FNS, 'returns', 'spin', 'projectile_attack'],
  verb: 'strike',
};

/** Giant yo-yo: shoots out and snaps back, seven times in a row. */
export const yoyo: CatalogEntry = {
  id: 'yoyo',
  names: ['yoyos', 'giant yoyo', 'yoyo string', 'yo-yo', 'yo-yos', 'giant yo-yo', 'yoyoo', 'yoyoyo', 'yoyoe', 'yoyo weapon', 'battle yoyo', 'yo yo', 'yoyo of doom', 'killer yoyo'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.4, density: 1500, lifetime: 30,
  build() {
    const g = 'spool';
    const parts: Part[] = [
      P('cylinder', [1.4, 0.35, 1.4], [-0.3, 0, 0], { role: 'disc', rot: [0, 0, 90], group: g }),
      P('cylinder', [1.4, 0.35, 1.4], [0.3, 0, 0], { role: 'disc', rot: [0, 0, 90], group: g }),
      V('cylinder', [0.4, 0.3, 0.4], [0, 0, 0], 'axle', g, [0, 0, 90]),
      V('box', [0.8, 0.1, 0.12], [-0.48, 0, 0], 'stripe', g),
    ];
    return { parts, joints: [J(g, [0, 0, 0], [spin(4, [1, 0, 0])])] };
  },
  behaviors: [strikeB(46, 'return', 6), HOLD],
  functions: [...STRIKE_FNS, 'returns', 'spin'],
  verb: 'strike',
};

/** Chakram: a spinning ring blade thrown flat; four passes. */
export const chakram: CatalogEntry = {
  id: 'chakram',
  names: ['glaive', 'chakrams', 'chakrum', 'war disc', 'tron disc', 'razor disc', 'energy disc', 'chakkram', 'chackram', 'glaives', 'razor discs', 'disc of light', 'ring blade', 'throwing ring', 'razor ring'],
  cls: 'weapon', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.4, density: 1200, lifetime: 30,
  build() {
    const g = 'ring';
    const parts: Part[] = [
      P('torus', [2.0, 0.3, 2.0], [0, 0, 0], { role: 'blade', group: g }),
      V('box', [1.7, 0.08, 0.14], [0, 0, 0], 'spoke', g),
      V('box', [0.14, 0.08, 1.7], [0, 0, 0], 'spoke', g),
      ...around(6, 1.05, (a, x, z) => V('wedge', [0.1, 0.1, 0.4], [x, 0, z], 'tooth', g, [0, a + 90, 0])),
    ];
    return { parts, joints: [J(g, [0, 0, 0], [spin(4, [0, 1, 0])])] };
  },
  behaviors: [strikeB(58, 'return', 3), HOLD],
  functions: [...STRIKE_FNS, 'returns', 'spin', 'projectile_attack'],
  verb: 'strike',
};

/** A piano drops out of the sky onto them (committed: it leads a runner, a sidestep dodges it). */
export const piano: CatalogEntry = {
  id: 'piano',
  names: ['pianoforte', 'pianos', 'pianno', 'piana', 'paino', 'pianoe', 'piannos', 'piyano', 'pianno forte', 'piannoforte', 'pianofortes', 'baby grand', 'upright piano', 'falling piano', 'piano drop', 'grand pianos', 'upright pianos', 'falling pianos', 'piano of doom'],
  cls: 'heavy', motion: 'kinematic', at: 'above_target', scale: 1.3, density: 4000, lifetime: 12,
  build() {
    const parts: Part[] = [
      P('box', [2.6, 1.3, 1.5], [0, 0.9, 0], { role: 'body' }),
      P('box', [2.6, 0.18, 0.7], [0, 1.05, 1.0], { role: 'keys' }),
      V('box', [0.1, 0.12, 0.5], [-0.9, 1.16, 1.05], 'black_key'),
      V('box', [0.1, 0.12, 0.5], [0.2, 1.16, 1.05], 'black_key'),
      V('box', [0.1, 0.12, 0.5], [0.9, 1.16, 1.05], 'black_key'),
      P('box', [2.6, 0.12, 1.5], [0, 1.62, 0], { role: 'lid', group: 'lid' }),
      ...[-1.1, 1.1].map((x) => V('cylinder', [0.22, 0.6, 0.22], [x, 0.0, 0.6], 'leg')),
      V('cylinder', [0.22, 0.6, 0.22], [0, 0.0, -0.6], 'leg'),
    ];
    return { parts, joints: [J('lid', [0, 1.62, -0.75], [swing(35, 2.5, [1, 0, 0])])] };
  },
  behaviors: [strikeB(55, 'dissolve')],
  functions: ['crush_target', 'strike_target', 'damages_target', 'attacks_enemies', 'has_moving_parts'],
  verb: 'crush',
};

/** A giant boot stamps down from above (leads a runner; one committed stomp). */
export const stompBoot: CatalogEntry = {
  id: 'stomp_boot',
  names: ['stomper', 'boot stomp', 'giant boot', 'giant foot', 'stomping boot', 'stompy boot', 'bigfoot stomp', 'giant stomp', 'stompers', 'giant boots', 'giant feet', 'big boot', 'monty python foot'],
  cls: 'heavy', motion: 'kinematic', at: 'above_target', scale: 1.6, density: 3500, lifetime: 12,
  build() {
    const g = 'boot';
    const parts: Part[] = [
      P('cylinder', [1.3, 3.2, 1.3], [0, 2.4, -0.3], { role: 'leg', group: g }),
      P('box', [1.6, 0.9, 2.8], [0, 0.45, 0.3], { role: 'sole', group: g }),
      V('dome', [1.6, 0.8, 1.2], [0, 0.9, 1.1], 'toe', g),
      V('box', [1.7, 0.25, 2.9], [0, 0.1, 0.3], 'tread', g),
    ];
    return { parts, joints: [J(g, [0, 4.0, -0.3], [swing(12, 1.8, [1, 0, 0])])] };
  },
  behaviors: [strikeB(60, 'dissolve')],
  functions: ['crush_target', 'strike_target', 'damages_target', 'attacks_enemies', 'has_moving_parts'],
  verb: 'crush',
};

/** Harpoon: flies after its target (a turn-limited chaser), hooks it and drags it back to you. */
export const harpoon: CatalogEntry = {
  id: 'harpoon',
  names: ['harpoon gun', 'speargun', 'spear gun', 'whaling harpoon', 'harpune', 'harpoons', 'hookshot harpoon', 'harpon', 'harpoon launcher', 'harpoon cannon', 'spearguns'],
  cls: 'manipulator', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.1, density: 800, lifetime: 25,
  build() {
    const parts: Part[] = [
      P('cylinder', [0.16, 3.2, 0.16], [0, 0, -0.6], { role: 'shaft', rot: [90, 0, 0] }),
      P('cone', [0.4, 0.8, 0.4], [0, 0, 1.3], { role: 'tip', rot: [90, 0, 0] }),
      V('cone', [0.14, 0.6, 0.14], [-0.2, 0, 0.8], 'barb', 'barb_l', [90, 0, -30]),
      V('cone', [0.14, 0.6, 0.14], [0.2, 0, 0.8], 'barb', 'barb_r', [90, 0, 30]),
      V('box', [0.7, 0.05, 0.3], [0, 0, -2.0], 'fletch', 'fins'),
      V('box', [0.05, 0.7, 0.3], [0, 0, -2.0], 'fletch', 'fins'),
    ];
    return {
      parts,
      joints: [
        J('barb_l', [-0.1, 0, 1.0], [swing(25, 2, [0, 1, 0])]),
        J('barb_r', [0.1, 0, 1.0], [swing(25, 2, [0, -1, 0])]),
        J('fins', [0, 0, -2.0], [spin(2, [0, 0, 1])]),
      ],
    };
  },
  behaviors: [{ type: 'grab', target: FOE, then: 'bring' }],
  functions: ['grab_target', 'pull_target', 'restrain_target', 'has_moving_parts'],
  verb: 'pull',
};

/** Snatcher claw: chases the target down, clamps shut on it and hoists it high into the air. */
export const snatchClaw: CatalogEntry = {
  id: 'snatch_claw',
  names: ['snatcher claw', 'grappling claw', 'grapple claw', 'yank claw', 'ufo claw', 'claw grabber', 'grabber claw', 'snatch claws', 'grappling claws', 'snatchclaw', 'snatcher claws', 'sky claw', 'skyhook', 'abduction claw'],
  cls: 'manipulator', motion: 'kinematic', at: 'hand', orient: 'along_look', scale: 1.6, density: 600, lifetime: 25,
  build() {
    const parts: Part[] = [
      P('cylinder', [1.0, 0.6, 1.0], [0, 0, -0.3], { role: 'palm', rot: [90, 0, 0] }),
      V('cylinder', [0.3, 1.2, 0.3], [0, 0, -1.2], 'cable', undefined, [90, 0, 0]),
    ];
    const joints: Joint[] = [];
    for (let i = 0; i < 3; i++) {
      const a = (i / 3) * Math.PI * 2;
      const x = Math.sin(a) * 0.45, y = Math.cos(a) * 0.45;
      const g = `finger${i}`;
      parts.push(limb('capsule', [x, y, 0], [x * 2.1, y * 2.1, 0.9], 0.22, { role: 'finger', group: g, solid: false }));
      parts.push(limb('cone', [x * 2.1, y * 2.1, 0.9], [x * 1.2, y * 1.2, 1.6], 0.2, { role: 'talon', group: g, solid: false }));
      joints.push(J(g, [x, y, 0], [swing(22, 1.3, [Math.cos(a), -Math.sin(a), 0])]));
    }
    return { parts, joints };
  },
  behaviors: [{ type: 'grab', target: FOE, then: 'lift' }, { type: 'lift', target: FOE, height: 12 }],
  functions: ['grab_target', 'lift_target', 'restrain_target', 'has_moving_parts'],
  verb: 'grab',
};

// ---------------------------------------------------------------- CHASERS / RANGED TURRETS (lead + track)

const TURRET_FNS = ['has_moving_parts', 'aims_at_target', 'shoots', 'projectile_attack', 'attacks_enemies', 'damages_target', 'guards_area'];

/** Yaw head + pitching gun group on a pedestal: returns the joints; the caller adds parts to 'turret' / 'gun'. */
function turretJoints(h: number, yawRate: number, pitchRate: number, minP = -25, maxP = 55, gunPivotZ = 0) {
  return [J('turret', [0, h, 0], [aimYaw(yawRate)]), J('gun', [0, h + 0.5, gunPivotZ], [aimPitch(pitchRate, minP, maxP)], 'turret')];
}

/** Gatling sentry: six-barrel cluster spins up while the head tracks; bullet hose that leads its target. */
export const gatlingTurret: CatalogEntry = {
  id: 'gatling_turret',
  names: ['gatling turret', 'gatling sentry', 'gatling tower', 'minigun turret', 'minigun sentry', 'chaingun turret', 'machine gun turret', 'machinegun turret', 'gattling turret', 'gatlin turret', 'sentry minigun', 'gatling emplacement', 'machine gun nest', 'minigun nest', 'bullet hose'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 75,
  build() {
    const h = 1.4;
    const parts: Part[] = [
      ...pedestal(h),
      P('box', [1.3, 0.8, 1.2], [0, h + 0.4, -0.1], { role: 'housing', group: 'turret' }),
      V('box', [1.5, 0.08, 1.0], [0, h + 0.85, -0.1], 'shield', 'turret'),
      V('cylinder', [0.6, 0.8, 0.6], [0, h + 0.5, 0.8], 'receiver', 'gun', [90, 0, 0]),
      V('box', [0.3, 0.5, 0.6], [0.45, h + 0.4, 0.5], 'ammo', 'gun'),
      ...around(6, 0.22, (_a, x, z) => V('cylinder', [0.1, 1.9, 0.1], [x, h + 0.5 + z, 1.9], 'barrel', 'barrels', [90, 0, 0])),
      V('torus', [0.6, 0.08, 0.6], [0, h + 0.5, 2.5], 'clamp', 'barrels', [90, 0, 0]),
      muzzle([0, h + 0.5, 0.3], 'barrels', 0.18), // at the pivot: see FIRE_FROM_PIVOT
    ];
    return { parts, joints: [...turretJoints(h, 240, 200), J('barrels', [0, h + 0.5, 1.9], [spin(6, [0, 0, 1])], 'gun')] };
  },
  behaviors: [guardB(50), weaponB('gun', 'gun', 10, 85, 5, 50, { size: 0.13, spread: 2.5 })],
  functions: [...TURRET_FNS, 'destroys_target', 'spin'],
  verb: 'shoot',
};

/** Six-tube rocket pod: salvoes of homing missiles (turn-limited, a hard cut or a wall shakes them). */
export const rocketPod: CatalogEntry = {
  id: 'rocket_pod',
  names: ['rocket pod', 'missile turret', 'rocket turret', 'missile silo', 'katyusha', 'missile barrage', 'swarm missiles', 'homing rocket turret', 'missle turret', 'rocket turrets', 'missile turrets', 'rocket rack', 'rocket pods', 'micro missiles', 'macross missiles'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 75,
  build() {
    const h = 1.3;
    const parts: Part[] = [
      ...pedestal(h),
      P('box', [1.0, 0.5, 1.0], [0, h + 0.25, 0], { role: 'mount', group: 'turret' }),
      P('box', [1.9, 1.3, 1.6], [0, h + 1.0, 0.1], { role: 'pod', group: 'gun' }),
    ];
    for (const [x, y] of [[-0.55, -0.35], [0, -0.35], [0.55, -0.35], [-0.55, 0.35], [0, 0.35], [0.55, 0.35]]) {
      parts.push(V('cylinder', [0.4, 0.2, 0.4], [x, h + 1.0 + y, 0.95], 'tube', 'gun', [90, 0, 0]));
      parts.push(muzzle([x, h + 1.0 + y, 1.1], 'gun', 0.2));
    }
    return { parts, joints: [J('turret', [0, h, 0], [aimYaw(150)]), J('gun', [0, h + 0.9, 0], [aimPitch(120, -10, 60), recoil(0.15, 5)], 'turret')] };
  },
  behaviors: [guardB(60), weaponB('missile', 'gun', 1.3, 32, 22, 60, { size: 0.3 })],
  functions: [...TURRET_FNS, 'area_attack'],
  verb: 'shoot',
};

/** Laser sentry: an instant beam (no travel time) — it only has to keep its lens on you. */
export const laserTurret: CatalogEntry = {
  id: 'laser_turret',
  names: ['laser turret', 'laser sentry', 'laser tower', 'beam turret', 'death ray turret', 'lazer turret', 'laser cannon turret', 'ion cannon', 'particle cannon', 'plasma turret', 'laser emplacement', 'lazer sentry'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 75,
  build() {
    const h = 1.8;
    const parts: Part[] = [
      ...pedestal(h, 1.5),
      P('sphere', [1.2, 1.0, 1.2], [0, h + 0.4, 0], { role: 'head', group: 'turret' }),
      P('cylinder', [0.34, 1.8, 0.34], [0, h + 0.5, 1.0], { role: 'barrel', rot: [90, 0, 0], group: 'gun' }),
      ...[0.6, 1.1, 1.6].map((z) => V('torus', [0.6, 0.1, 0.6], [0, h + 0.5, z], 'coil', 'gun', [90, 0, 0])),
      V('sphere', [0.3, 0.3, 0.3], [0, h + 0.5, 1.95], 'lens', 'gun'),
      muzzle([0, h + 0.5, 2.1], 'gun'),
    ];
    return { parts, joints: turretJoints(h, 300, 260, -40, 70) };
  },
  behaviors: [guardB(55), weaponB('laser', 'gun', 2.5, 0, 10, 55)],
  functions: [...TURRET_FNS],
  verb: 'shoot',
};

/** Tesla coil: stacked rings hum and spin; lightning arcs zap anything within 22 m. */
export const teslaCoil: CatalogEntry = {
  id: 'tesla_coil',
  names: ['tesla coil', 'tesla tower', 'zapper', 'lightning tower', 'shock tower', 'lightning rod', 'zap tower', 'tesla coils', 'telsa coil', 'lightning coil', 'zappers', 'bug zapper', 'arc tower'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const parts: Part[] = [
      P('cylinder', [1.8, 0.5, 1.8], [0, 0.25, 0], { role: 'base' }),
      P('cylinder', [0.5, 4.0, 0.5], [0, 2.5, 0], { role: 'column' }),
      ...[1.4, 2.4, 3.4].map((y, i) => V('torus', [1.6 - i * 0.3, 0.18, 1.6 - i * 0.3], [0, y, 0], 'coil', `ring${i}`)),
      P('sphere', [1.3, 1.3, 1.3], [0, 4.9, 0], { role: 'orb' }),
      V('box', [0.1, 0.1, 0.1], [0, 4.9, 0], 'arc', 'arc'),
      muzzle([0, 4.9, 0.75], 'arc'),
    ];
    return {
      parts,
      joints: [
        J('ring0', [0, 1.4, 0], [spin(1.5, [0, 1, 0]), { kind: 'piston', axis: [0, 1, 0], amp: 0.15, rate: 3 }]),
        J('ring1', [0, 2.4, 0], [spin(-2, [0, 1, 0]), { kind: 'piston', axis: [0, 1, 0], amp: 0.15, rate: 3, phase: 0.33 }]),
        J('ring2', [0, 3.4, 0], [spin(2.5, [0, 1, 0]), { kind: 'piston', axis: [0, 1, 0], amp: 0.15, rate: 3, phase: 0.66 }]),
        J('arc', [0, 4.9, 0], [aimBoth(720, -85, 85)]),
      ],
    };
  },
  behaviors: [guardB(22), weaponB('laser', 'arc', 3, 0, 7, 22)],
  functions: [...TURRET_FNS, 'area_attack'],
  verb: 'shoot',
};

/** Flame sentry: a dragon-mouth nozzle that hoses a cone of fire; short range, hard to escape up close. */
export const flameTurret: CatalogEntry = {
  id: 'flame_turret',
  names: ['flame turret', 'fire turret', 'flamethrower turret', 'flame thrower turret', 'flame sentry', 'flame turrets', 'burner turret', 'flame tower', 'fire tower', 'flamer turret', 'fire spitter', 'inferno tower', 'dragon head turret'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const h = 1.3;
    const parts: Part[] = [
      ...pedestal(h),
      P('cylinder', [1.2, 1.0, 1.2], [0, h + 0.5, -0.4], { role: 'tank', rot: [90, 0, 0], group: 'turret' }),
      P('cone', [0.9, 1.6, 0.9], [0, h + 0.55, 0.9], { role: 'nozzle', rot: [-90, 0, 0], group: 'gun' }),
      V('box', [0.8, 0.14, 0.6], [0, h + 0.2, 1.2], 'jaw', 'jaw'),
      muzzle([0, h + 0.55, 1.8], 'gun'),
    ];
    return { parts, joints: [...turretJoints(h, 220, 200, -30, 45), J('jaw', [0, h + 0.3, 0.9], [snap(25, 6)], 'gun')] };
  },
  behaviors: [guardB(16), weaponB('flame', 'gun', 10, 0, 4, 15)],
  functions: [...TURRET_FNS, 'area_attack'],
  verb: 'shoot',
};

/** Railgun: slow to turn, one very fast, very hard slug every few seconds; punishes predictable running. */
export const railgun: CatalogEntry = {
  id: 'railgun',
  names: ['rail gun', 'gauss cannon', 'gauss gun', 'coilgun', 'coil gun', 'sniper turret', 'sniper cannon', 'railgun turret', 'rail cannon', 'railguns', 'rail guns', 'mass driver'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 75,
  build() {
    const h = 1.6;
    const parts: Part[] = [
      ...pedestal(h),
      P('box', [1.2, 0.7, 1.4], [0, h + 0.35, -0.3], { role: 'housing', group: 'turret' }),
      V('box', [0.22, 0.3, 4.0], [-0.3, h + 0.5, 1.6], 'rail', 'gun'),
      V('box', [0.22, 0.3, 4.0], [0.3, h + 0.5, 1.6], 'rail', 'gun'),
      ...[0.5, 1.3, 2.1, 2.9].map((z) => V('torus', [1.0, 0.12, 1.0], [0, h + 0.5, z], 'coil', 'gun', [90, 0, 0])),
      muzzle([0, h + 0.5, 0.3], 'gun'),
    ];
    return { parts, joints: [J('turret', [0, h, 0], [aimYaw(90)]), J('gun', [0, h + 0.5, -0.3], [aimPitch(70, -20, 50), recoil(0.8, 1.5)], 'turret')] };
  },
  behaviors: [guardB(60), weaponB('gun', 'gun', 0.35, 95, 55, 60, { size: 0.22 })],
  functions: [...TURRET_FNS, 'destroys_target'],
  verb: 'shoot',
};

/** Blunderbuss: four pellets per blast in a wide spread — deadly close, useless far. */
export const blunderbuss: CatalogEntry = {
  id: 'blunderbuss',
  names: ['shotgun turret', 'scattergun', 'boomstick', 'shotgun cannon', 'buckshot cannon', 'shotgun sentry', 'flak cannon', 'flak turret', 'blunderbus', 'blunderbusss', 'blunderbusses', 'shotgun blast', 'double barrel turret'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const h = 1.2;
    const parts: Part[] = [
      ...pedestal(h),
      P('box', [1.2, 0.6, 1.2], [0, h + 0.3, -0.2], { role: 'stock', group: 'turret' }),
      V('cylinder', [0.5, 2.0, 0.5], [-0.28, h + 0.5, 1.0], 'barrel', 'gun', [90, 0, 0]),
      V('cylinder', [0.5, 2.0, 0.5], [0.28, h + 0.5, 1.0], 'barrel', 'gun', [90, 0, 0]),
      V('cone', [1.0, 0.5, 1.0], [0, h + 0.5, 2.1], 'bell', 'gun', [90, 0, 0]),
      muzzle([-0.28, h + 0.5, 0.4], 'gun'), muzzle([0.28, h + 0.5, 0.4], 'gun'),
    ];
    return { parts, joints: [J('turret', [0, h, 0], [aimYaw(260)]), J('gun', [0, h + 0.5, 0], [aimPitch(220, -30, 50), recoil(0.4, 3)], 'turret')] };
  },
  behaviors: [guardB(24), ...[0, 1, 2, 3].map(() => weaponB('gun', 'gun', 1.1, 60, 7, 22, { size: 0.16, spread: 18 }))],
  functions: [...TURRET_FNS, 'area_attack'],
  verb: 'shoot',
};

/** Net sentry: lobs weighted nets that stop whatever they hit dead for a few seconds. */
export const netTurret: CatalogEntry = {
  id: 'net_turret',
  names: ['net turret', 'net sentry', 'net thrower', 'web turret', 'net mortar', 'bola launcher', 'bola turret', 'net cannon turret', 'web sentry', 'net turrets', 'net launcher turret', 'capture turret'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const h = 1.2;
    const parts: Part[] = [
      ...pedestal(h),
      P('box', [1.0, 0.6, 1.0], [0, h + 0.3, 0], { role: 'mount', group: 'turret' }),
      P('cylinder', [0.9, 1.4, 0.9], [0, h + 0.6, 0.6], { role: 'barrel', rot: [70, 0, 0], group: 'gun' }),
      V('torus', [1.0, 0.1, 1.0], [0, h + 0.85, 1.25], 'rim', 'gun', [70, 0, 0]),
      muzzle([0, h + 0.9, 1.4], 'gun', 0.3),
    ];
    return { parts, joints: [J('turret', [0, h, 0], [aimYaw(200)]), J('gun', [0, h + 0.5, 0], [aimPitch(160, -20, 60), recoil(0.3, 2.5)], 'turret')] };
  },
  behaviors: [guardB(35), weaponB('net', 'gun', 0.6, 45, 4, 35, { size: 0.55 })],
  functions: [...TURRET_FNS, 'restrain_target'],
  verb: 'shoot',
};

/** Piranha launcher: a chomping fish-mouth cannon that spits snapping piranhas in a spread. Silly, effective. */
export const piranhaLauncher: CatalogEntry = {
  id: 'piranha_launcher',
  names: ['piranha launcher', 'piranha gun', 'piranha cannon', 'fish cannon', 'fish launcher', 'fish gun', 'shark launcher', 'fish turret', 'piranha turret', 'pirahna launcher', 'piranah launcher'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const h = 1.2;
    const parts: Part[] = [
      ...pedestal(h),
      P('cylinder', [1.0, 0.3, 1.0], [0, h + 0.15, 0], { role: 'mount', group: 'turret' }),
      V('sphere', [1.6, 1.3, 2.0], [0, h + 0.65, 0.2], 'fish', 'gun'),
      V('wedge', [0.1, 0.9, 0.9], [0, h + 1.3, 0], 'fin', 'gun'),
      V('box', [0.1, 1.0, 0.6], [0, h + 0.65, -0.9], 'tail', 'tail'),
      ...[-0.45, 0.45].map((x) => V('sphere', [0.3, 0.3, 0.2], [x, h + 0.95, 0.95], 'eye', 'gun')),
      V('box', [1.1, 0.25, 0.9], [0, h + 0.2, 0.9], 'jaw', 'jaw'),
      ...[-0.3, 0, 0.3].map((x) => V('cone', [0.14, 0.3, 0.14], [x, h + 0.4, 1.3], 'tooth', 'jaw')),
      muzzle([0, h + 0.6, 0.4], 'gun', 0.25),
    ];
    return {
      parts,
      joints: [
        J('turret', [0, h, 0], [aimYaw(220)]),
        J('gun', [0, h + 0.6, 0], [aimPitch(180, -25, 50)], 'turret'),
        J('jaw', [0, h + 0.35, 0.5], [snap(30, 5)], 'gun'),
        J('tail', [0, h + 0.65, -0.7], [swing(30, 3, [0, 1, 0])], 'gun'),
      ],
    };
  },
  behaviors: [guardB(35), weaponB('gun', 'gun', 5, 55, 7, 35, { size: 0.3, spread: 7 })],
  functions: [...TURRET_FNS],
  verb: 'shoot',
};

/** Vending machine that turns to face you and fires soda cans out of its flap. */
export const vendingMachine: CatalogEntry = {
  id: 'vending_machine',
  names: ['vending machine', 'soda machine', 'coke machine', 'drinks machine', 'soda cannon', 'snack machine', 'vendo', 'vending machines', 'vendingmachine', 'soda machines', 'coke machines', 'pop machine'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const parts: Part[] = [
      P('box', [1.8, 0.3, 1.6], [0, 0.15, 0], { role: 'base' }),
      P('box', [1.7, 3.0, 1.3], [0, 1.8, 0], { role: 'body', group: 'turret' }),
      V('box', [1.0, 1.8, 0.1], [-0.25, 2.2, 0.66], 'window', 'turret'),
      ...[0, 1, 2].map((i) => V('cylinder', [0.25, 0.4, 0.25], [-0.55 + i * 0.3, 2.6, 0.5], 'can', 'turret')),
      V('box', [0.3, 0.8, 0.1], [0.6, 2.2, 0.66], 'buttons', 'turret'),
      V('box', [1.0, 0.35, 0.12], [-0.2, 0.75, 0.7], 'flap', 'flap'),
      muzzle([-0.2, 0.9, 0.95], 'flap', 0.25),
    ];
    return { parts, joints: [J('turret', [0, 0.3, 0], [aimYaw(160)]), J('flap', [-0.2, 0.92, 0.7], [snap(70, 5)], 'turret')] };
  },
  behaviors: [guardB(35), weaponB('gun', 'flap', 4, 50, 8, 35, { size: 0.25, spread: 5 })],
  functions: [...TURRET_FNS],
  verb: 'shoot',
};

/** Crossbow sentry: its limbs snap forward with every bolt; accurate, leads well. */
export const crossbowTurret: CatalogEntry = {
  id: 'crossbow_turret',
  names: ['crossbow turret', 'arbalest', 'bolt thrower', 'crossbow sentry', 'arrow turret', 'archer tower', 'arrow tower', 'crossbow tower', 'auto crossbow', 'crossbow turrets', 'arrow turrets', 'repeating crossbow'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 75,
  build() {
    const h = 1.3;
    const parts: Part[] = [
      ...pedestal(h),
      P('box', [0.5, 0.4, 0.8], [0, h + 0.4, -0.3], { role: 'stock', group: 'gun' }),
      V('box', [0.4, 0.3, 1.8], [0, h + 0.4, 1.0], 'rail', 'gun'),
      V('box', [1.4, 0.14, 0.2], [-0.7, h + 0.5, 1.6], 'limb', 'limb_l', [0, 20, 0]),
      V('box', [1.4, 0.14, 0.2], [0.7, h + 0.5, 1.6], 'limb', 'limb_r', [0, -20, 0]),
      V('cylinder', [0.08, 1.6, 0.08], [0, h + 0.62, 1.2], 'bolt', 'gun', [90, 0, 0]),
      muzzle([0, h + 0.4, 0.3], 'gun'),
    ];
    return {
      parts,
      joints: [
        J('gun', [0, h + 0.4, 0], [aimBoth(180, -30, 55)]),
        J('limb_l', [0, h + 0.5, 1.6], [snap(25, 4, [0, 1, 0])], 'gun'),
        J('limb_r', [0, h + 0.5, 1.6], [snap(25, 4, [0, -1, 0])], 'gun'),
      ],
    };
  },
  behaviors: [guardB(55), weaponB('gun', 'gun', 1.2, 80, 18, 55, { size: 0.32 })],
  functions: [...TURRET_FNS],
  verb: 'shoot',
};

// ---------------------------------------------------------------- CHASERS THAT MOVE

/** Five little gun-drones circling a hard-light core; the swarm hunts (≤ chaser speed, turn-limited) and strafes. */
export const droneSwarm: CatalogEntry = {
  id: 'drone_swarm',
  names: ['drone swarm', 'combat drones', 'kamikaze drones', 'swarm of drones', 'quadcopter swarm', 'drone swarms', 'drone army', 'attack drone swarm', 'drone swarmm', 'dronesworm', 'drone horde', 'killer drones', 'hunter drones', 'murder drones', 'hunter killer drones', 'war drones', 'drone squad'],
  cls: 'creature', motion: 'kinematic', at: 'ground_front', lifetime: 60,
  build() {
    const parts: Part[] = [P('sphere', [0.8, 0.8, 0.8], [0, 1.0, 0], { role: 'core' })];
    for (const [a, x, z] of ring(5, 1.9)) {
      parts.push(V('box', [0.7, 0.2, 0.7], [x, 1.0, z], 'drone', 'swarm', [0, a, 0]));
      parts.push(V('box', [1.1, 0.04, 0.12], [x, 1.18, z], 'rotor', 'swarm', [0, a + 45, 0]));
      parts.push(muzzle([x, 0.9, z + 0.4], 'swarm', 0.15));
    }
    return { parts, joints: [J('swarm', [0, 1.0, 0], [spin(0.7, [0, 1, 0]), { kind: 'piston', axis: [0, 1, 0], amp: 0.3, rate: 1.3 }])] };
  },
  behaviors: [{ type: 'drive', mode: 'air', speed: 30, turn: 220, altitude: 5 }, huntB(60), weaponB('gun', 'swarm', 4, 70, 4, 30, { size: 0.12 })],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'shoots', 'projectile_attack', 'attacks_enemies', 'damages_target'],
  verb: 'shoot',
};

/** Bee swarm: a buzzing cloud of hard-light bees that chases its target and stings everything in reach. */
export const beeSwarm: CatalogEntry = {
  id: 'bee_swarm',
  names: ['bee swarm', 'killer bee swarm', 'hornet swarm', 'wasp swarm', 'beehive', 'bee hive', 'swarm of hornets', 'swarm of wasps', 'bee swarms', 'beeswarm', 'hornet swarms', 'hornets nest', "hornet's nest", 'murder hornets', 'angry swarm'],
  cls: 'creature', motion: 'kinematic', at: 'ground_front', lifetime: 45,
  build() {
    const parts: Part[] = [P('sphere', [1.4, 1.4, 1.4], [0, 1.2, 0], { role: 'swarm_core', solid: false })];
    ring(7, 1.3).forEach(([a, x, z], i) => {
      const y = 1.2 + ((i % 3) - 1) * 0.45;
      parts.push(V('capsule', [0.3, 0.55, 0.3], [x, y, z], 'bee', 'bees', [90, a, 0]));
      parts.push(V('box', [0.6, 0.03, 0.22], [x, y + 0.2, z], 'wing', 'bees', [0, a, 0]));
    });
    return { parts, joints: [J('bees', [0, 1.2, 0], [spin(1.4, [0, 1, 0]), { kind: 'piston', axis: [0, 1, 0], amp: 0.35, rate: 4 }])] };
  },
  behaviors: [{ type: 'drive', mode: 'air', speed: 28, turn: 240, altitude: 3 }, huntB(60), weaponB('melee', undefined, 3, 0, 6, 1.6)],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'area_attack'],
  verb: 'strike',
};

/** Giant rubber duck: bounces after its target and body-slams it. Quack. */
export const rubberDuck: CatalogEntry = {
  id: 'rubber_duck',
  names: ['rubber duck', 'rubber ducky', 'rubber duckie', 'giant rubber duck', 'duckie', 'ducky', 'rubber ducks', 'rubberduck', 'rubber duckies', 'rubber duckys', 'ruber duck', 'bath duck'],
  cls: 'creature', motion: 'kinematic', at: 'ground_front', lifetime: 45,
  build() {
    const g = 'duck';
    const parts: Part[] = [
      P('sphere', [2.4, 1.8, 3.0], [0, 1.1, 0], { role: 'body', group: g }),
      P('sphere', [1.5, 1.5, 1.5], [0, 2.5, 0.8], { role: 'head', group: 'head' }),
      V('wedge', [0.8, 0.3, 0.7], [0, 2.35, 1.75], 'beak', 'head'),
      ...[-0.4, 0.4].map((x) => V('sphere', [0.22, 0.26, 0.1], [x, 2.75, 1.45], 'eye', 'head')),
      V('wedge', [1.2, 0.6, 0.8], [0, 1.6, -1.5], 'tail', g, [0, 180, 0]),
    ];
    return {
      parts,
      joints: [
        J(g, [0, 0.2, 0], [{ kind: 'piston', axis: [0, 1, 0], amp: 1.6, rate: 1.4 }]),
        J('head', [0, 2.0, 0.6], [swing(15, 2.8, [1, 0, 0])], g),
      ],
    };
  },
  behaviors: [{ type: 'drive', mode: 'ground', speed: 20, turn: 220, jump: 10 }, huntB(60), weaponB('melee', undefined, 1.5, 0, 20, 1.5)],
  functions: ['moves_autonomously', 'drives_on_ground', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'pushes_objects'],
  verb: 'strike',
};

/** A saw blade the size of a door that rolls along the ground after its target (its spin follows its travel). */
export const rollingSaw: CatalogEntry = {
  id: 'rolling_saw',
  names: ['rolling saw', 'rolling saw blade', 'rolling sawblade', 'saw wheel', 'bouncing buzzsaw', 'bouncing saw', 'rolling blade', 'ripsaw', 'saw drone', 'rolling saws', 'rolling sawblades', 'killer wheel', 'wheel of death', 'runaway saw'],
  cls: 'weapon', motion: 'kinematic', at: 'ground_front', lifetime: 40,
  build() {
    const g = 'blade';
    const r = 1.4;
    const parts: Part[] = [
      P('cylinder', [r * 2, 0.25, r * 2], [0, r, 0], { role: 'disc', rot: [0, 0, 90], group: g }),
      V('cylinder', [0.7, 0.4, 0.7], [0, r, 0], 'hub', g, [0, 0, 90]),
      V('box', [0.3, r * 1.8, 0.3], [0, r, 0], 'spoke', g),
    ];
    for (let i = 0; i < 12; i++) {
      const a = (i / 12) * Math.PI * 2;
      parts.push(V('wedge', [0.12, 0.3, 0.45], [0, r + Math.sin(a) * (r + 0.1), Math.cos(a) * (r + 0.1)], 'tooth', g, [-(a * 180) / Math.PI, 0, 0]));
    }
    return { parts, joints: [J(g, [0, r, 0], [roll(r), bob(0.4, 1.8, 0, false)])] };
  },
  behaviors: [{ type: 'drive', mode: 'ground', speed: 26, turn: 260, jump: 9 }, huntB(60), weaponB('melee', undefined, 3, 0, 14, 1.2)],
  functions: ['moves_autonomously', 'drives_on_ground', 'has_moving_parts', 'wheels_spin', 'attacks_enemies', 'damages_target', 'strike_target', 'spin'],
  verb: 'strike',
};

for (const [e, dy] of [[gatlingTurret, 1.9], [rocketPod, 2.2], [railgun, 2.1], [blunderbuss, 1.7], [netTurret, 1.7], [piranhaLauncher, 1.8], [vendingMachine, 0.9], [crossbowTurret, 1.7]] as const) gunAtOrigin(e, dy);

export const WEAPONS: CatalogEntry[] = [
  thunderHammer, warAxe, greatsword, lance, flail, boomerang, yoyo, chakram, piano, stompBoot, harpoon, snatchClaw,
  gatlingTurret, rocketPod, laserTurret, teslaCoil, flameTurret, railgun, blunderbuss, netTurret, piranhaLauncher, vendingMachine, crossbowTurret,
  droneSwarm, beeSwarm, rubberDuck, rollingSaw,
];

