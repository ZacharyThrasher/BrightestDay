/**
 * Monsters & fantasy creatures. Same contract as beasts.ts: each one hunts on its own (the mounts carry you), attacks
 * with articulated parts, never hurts its owner, and has a counter — golems/titans/kaiju are slow (outrun them),
 * dragons and cyclopes need line of sight (break it), zombies shamble, ghosts are weak up close, chargers commit.
 * Frame: +z forward, +y up, feet on y = 0.
 */
import type { CatalogEntry } from './types.js';
import type { Part, Shape, Vec3 } from '../spec.js';
import { P, J, limb, gait, swing, wave, snap, recoil, spin, aimYaw, aimPitch, aimBoth, bob, newRig, walkLeg, legs4, chain, wingPair, placed, type Rig } from './kit.js';
import { HUNT, FOLLOW, bite, bodyHit } from './beasts.js';

const WALKER = ['walks', 'drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'pushes_objects'];
const FLYER = ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'aims_at_target'];
const RIDE = ['rideable', 'carries_player', 'propel_player'];
const RANGED = ['shoots', 'projectile_attack', 'aims_at_target'];

// ------------------------------------------------------------------ two-legged body

interface BipedOpts {
  hipY: number; hipX: number; legT: number;
  torso: Vec3; lean?: number;
  head: Vec3; headShape?: Shape;
  shoulderX: number; armT: number; armLen: number; fist?: number;
  gait: [number, number]; aim?: number; knee?: number; foot?: number;
  /** arms held out in front (zombies, mummies) */
  reach?: boolean;
  /** 'none': the arm has no strike stroke */
  strike?: number;
}

/**
 * Humanoid: solid pelvis, walking legs, a solid torso that turns to the target (with head), two arms parented to it
 * that swing with the gait and strike (a fire stroke) — each hand carries a muzzle for its weapon.
 */
function biped(o: BipedOpts): { r: Rig; shoulderY: number; headC: Vec3; hand: (s: number) => Vec3; top: number } {
  const r = newRig();
  const { hipY, hipX, legT } = o;
  r.parts.push(P('box', [hipX * 2 + legT, legT * 0.9, legT * 1.1], [0, hipY, 0], { role: 'hips' }));
  walkLeg(r, 'leg_l', [-hipX, hipY, 0], legT, [gait(o.gait[0], o.gait[1], 0)], { knee: o.knee, foot: o.foot });
  walkLeg(r, 'leg_r', [hipX, hipY, 0], legT, [gait(o.gait[0], o.gait[1], 0.5)], { knee: o.knee, foot: o.foot });
  const [, ty, tz] = o.torso;
  const tc: Vec3 = [0, hipY + ty / 2 + legT * 0.3, (o.lean ?? 0) * ty * 0.006];
  r.parts.push(P('box', o.torso, tc, { role: 'body', rot: [o.lean ?? 0, 0, 0], group: 'torso' }));
  const top = tc[1] + ty / 2;
  const headC: Vec3 = [0, top + o.head[1] * 0.42, tc[2] + (o.lean ?? 0) * ty * 0.006 + tz * 0.1];
  r.parts.push(P(o.headShape ?? 'box', o.head, headC, { role: 'head', group: 'torso', solid: false }));
  for (const s of [-1, 1]) r.parts.push(P('sphere', [o.head[0] * 0.15, o.head[0] * 0.15, 0.06], [s * o.head[0] * 0.22, headC[1] + o.head[1] * 0.1, headC[2] + o.head[2] / 2], { role: 'eye', group: 'torso', solid: false }));
  const shoulderY = top - o.armT * 0.6;
  const hands: Vec3[] = [];
  for (const s of [-1, 1]) {
    const g = s < 0 ? 'arm_l' : 'arm_r';
    const sh: Vec3 = [s * o.shoulderX, shoulderY, tc[2]];
    const hand: Vec3 = o.reach ? [s * o.shoulderX * 0.8, shoulderY - o.armLen * 0.15, tc[2] + o.armLen] : [s * (o.shoulderX + o.armT * 0.3), shoulderY - o.armLen, tc[2] + o.armLen * 0.15];
    hands.push(hand);
    r.parts.push(P('sphere', [o.armT * 1.3, o.armT * 1.3, o.armT * 1.3], sh, { role: 'shoulder', group: g, solid: false }));
    r.parts.push(limb('capsule', sh, hand, o.armT, { role: 'arm', group: g, solid: false }));
    const f = o.fist ?? o.armT * 1.4;
    r.parts.push(P(o.reach ? 'box' : 'sphere', [f, f, f], hand, { role: 'fist', group: g, solid: false }));
    r.parts.push(P('sphere', [0.14, 0.14, 0.14], [hand[0], hand[1], hand[2] + f * 0.45], { role: 'muzzle', group: g, solid: false }));
    const motion = o.reach ? [swing(8, 0.8, [1, 0, 0], s < 0 ? 0 : 0.5)] : [gait(o.gait[0] * 0.7, o.gait[1], s < 0 ? 0.5 : 0)];
    if (o.strike !== 0) motion.push(snap(o.strike ?? -100, 2));
    r.joints.push(J(g, sh, motion, 'torso'));
  }
  r.joints.push(J('torso', [0, hipY, 0], [aimYaw(o.aim ?? 120), bob(hipY * 0.03, o.gait[1] * 2)]));
  return { r, shoulderY, headC, hand: (s) => hands[s < 0 ? 0 : 1], top };
}

// ================================================================== slow heavies (outrun them)

/** Stone golem: slow, enormous, punches like a landslide and shakes the ground under its feet. Ride on its shoulders. */
export const golem: CatalogEntry = {
  id: 'golem',
  names: ['golems', 'stone golem', 'rock golem', 'clay golem', 'iron golem', 'earth golem', 'rock monster', 'rock giant', 'living statue', 'rock man', 'rockman', 'golum', 'gollem', 'golemn', 'boulder man', 'stone man', 'earth elemental', 'rock elemental', 'stone guardian'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 1500,
  build() {
    const b = biped({ hipY: 2.0, hipX: 0.8, legT: 0.95, torso: [3.0, 2.4, 1.8], lean: 10, head: [1.1, 0.9, 1.0], shoulderX: 1.9, armT: 0.9, armLen: 2.6, fist: 1.4, gait: [22, 0.8], aim: 70, knee: 0.1, foot: 1.2 });
    const r = b.r;
    r.parts.push(P('box', [1.2, 0.8, 1.2], [-1.2, b.top - 0.1, 0.1], { role: 'rock', rot: [10, 20, 15], group: 'torso', solid: false }));
    r.parts.push(P('box', [1.0, 0.7, 1.0], [1.3, b.top - 0.15, -0.1], { role: 'rock', rot: [-10, 30, -10], group: 'torso', solid: false }));
    r.parts.push(P('box', [0.8, 0.12, 0.1], [0, b.headC[1] + 0.05, b.headC[2] + 0.52], { role: 'visor', group: 'torso', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 7, turn: 90, jump: 6 },
    HUNT(45),
    bite('arm_r', 45, 0.7, 3.0),
    bite('arm_l', 45, 0.6, 3.0),
    bodyHit(18, 0.5, 1.5),
    { type: 'rideable', seat: [0, 6.8, -0.3] },
  ],
  functions: [...WALKER, ...RIDE, 'push_target', 'area_attack'],
  verb: 'strike',
};

/** Titan: a towering giant with a tree-trunk club. Every swing is a knockout, but it walks — outrun it. */
export const titan: CatalogEntry = {
  id: 'titan',
  names: ['titans', 'colossus', 'stone giant', 'frost giant', 'hill giant', 'fire giant', 'storm giant', 'jotunn', 'jotun', 'behemoth', 'giant titan', 'huge giant', 'big giant', 'giant man', 'tital', 'titian', 'attack titan', 'colossal titan'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const b = biped({ hipY: 4.2, hipX: 0.9, legT: 0.9, torso: [2.8, 3.4, 1.6], head: [1.3, 1.5, 1.3], headShape: 'sphere', shoulderX: 1.8, armT: 0.7, armLen: 3.2, gait: [26, 0.7], aim: 60, foot: 1.3, strike: -120 });
    const r = b.r;
    const h = b.hand(1);
    // club held in the right hand
    r.parts.push(limb('cylinder', [h[0], h[1] - 0.3, h[2] - 0.2], [h[0], h[1] + 0.2, h[2] + 3.6], 0.5, { role: 'club', group: 'arm_r', solid: false }));
    r.parts.push(P('sphere', [1.1, 1.1, 1.1], [h[0], h[1] + 0.25, h[2] + 3.7], { role: 'club', group: 'arm_r', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [h[0], h[1] + 0.25, h[2] + 4.3], { role: 'muzzle', group: 'arm_r', solid: false }));
    r.parts.push(P('box', [3.0, 1.0, 1.8], [0, 4.4, 0], { role: 'belt', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 70, jump: 5 },
    HUNT(50),
    bite('arm_r', 55, 0.5, 4.5),
    bite('arm_l', 30, 0.6, 3.0),
    bodyHit(20, 0.5, 2.0),
  ],
  functions: [...WALKER, 'push_target', 'area_attack'],
  verb: 'strike',
};

/** Cyclops: one great eye that fires a laser (break line of sight!) and a club for anyone who gets close. */
export const cyclops: CatalogEntry = {
  id: 'cyclops',
  names: ['cyclopes', 'cyclopses', 'one eyed giant', 'one-eyed giant', 'laser eye giant', 'cyclop', 'cylops', 'cyclopse', 'polyphemus', 'one eyed monster', 'one-eyed monster', 'eye giant'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 700,
  build() {
    const b = biped({ hipY: 2.4, hipX: 0.6, legT: 0.6, torso: [2.0, 2.4, 1.3], head: [1.2, 1.2, 1.1], headShape: 'sphere', shoulderX: 1.3, armT: 0.5, armLen: 2.2, gait: [26, 1.0], aim: 110 });
    const r = b.r;
    // the eye: aims on its own (pitch + yaw) inside the head
    const e: Vec3 = [0, b.headC[1] + 0.1, b.headC[2] + 0.45];
    r.parts.push(P('sphere', [0.6, 0.6, 0.4], e, { role: 'eye', group: 'eye', solid: false }));
    r.parts.push(P('sphere', [0.22, 0.22, 0.1], [0, e[1], e[2] + 0.2], { role: 'pupil', group: 'eye', solid: false }));
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [0, e[1], e[2] + 0.3], { role: 'muzzle', group: 'eye', solid: false }));
    r.parts.push(P('cone', [0.3, 0.8, 0.3], [0, b.headC[1] + 0.7, b.headC[2]], { role: 'horn', group: 'torso', solid: false }));
    const h = b.hand(-1);
    r.parts.push(limb('cylinder', [h[0], h[1], h[2]], [h[0], h[1] + 0.3, h[2] + 2.2], 0.35, { role: 'club', group: 'arm_l', solid: false }));
    r.parts.push(P('sphere', [0.7, 0.7, 0.7], [h[0], h[1] + 0.3, h[2] + 2.3], { role: 'club', group: 'arm_l', solid: false }));
    r.joints.push(J('eye', e, [aimBoth(200, -70, 50)], 'torso'));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 100, jump: 6 },
    HUNT(55),
    { type: 'weapon', kind: 'laser', group: 'eye', rate: 1, speed: 0, damage: 18, range: 55 },
    bite('arm_l', 34, 0.7, 3.0),
  ],
  functions: [...WALKER, ...RANGED],
  verb: 'shoot',
};

/** Ogre / troll: a hulking brute with a spiked club; slower than you, hits much harder than you'd like. */
export const ogre: CatalogEntry = {
  id: 'ogre',
  names: ['ogres', 'troll', 'trolls', 'cave troll', 'mountain troll', 'orc', 'orcs', 'uruk', 'brute', 'ogar', 'trol', 'shrek', 'hobgoblin', 'bugbear'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 700,
  build() {
    const b = biped({ hipY: 1.5, hipX: 0.5, legT: 0.55, torso: [1.8, 1.8, 1.3], lean: 15, head: [0.8, 0.75, 0.8], shoulderX: 1.15, armT: 0.45, armLen: 1.8, gait: [28, 1.3], aim: 150 });
    const r = b.r;
    r.parts.push(P('sphere', [1.9, 1.2, 1.4], [0, 2.1, 0.25], { role: 'belly', group: 'torso', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.1, 0.25, 0.1], [s * 0.2, b.headC[1] - 0.25, b.headC[2] + 0.4], { role: 'tusk', group: 'torso', solid: false }));
    const h = b.hand(1);
    r.parts.push(limb('cylinder', [h[0], h[1], h[2]], [h[0], h[1] + 0.4, h[2] + 1.8], 0.3, { role: 'club', group: 'arm_r', solid: false }));
    r.parts.push(P('sphere', [0.6, 0.6, 0.6], [h[0], h[1] + 0.45, h[2] + 1.9], { role: 'club', group: 'arm_r', solid: false }));
    for (const a of [0, 90, 180, 270]) r.parts.push(...placed([P('cone', [0.12, 0.3, 0.12], [0.35, 0, 0], { role: 'spike', rot: [0, 0, -90], solid: false })], a, [h[0], h[1] + 0.45, h[2] + 1.9], 'arm_r'));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 12, turn: 140, jump: 7 },
    HUNT(45),
    bite('arm_r', 34, 0.8, 2.6),
    bite('arm_l', 16, 1, 1.8),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Yeti: shaggy climber — hurls snowballs over walls, then closes in with its claws. */
export const yeti: CatalogEntry = {
  id: 'yeti',
  names: ['yetis', 'yetti', 'abominable snowman', 'abominable snow man', 'abominable', 'bigfoot', 'big foot', 'sasquatch', 'sasquach', 'snow monster', 'snow beast', 'wampa', 'skunk ape', 'yeti monster'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 600,
  build() {
    const b = biped({ hipY: 1.4, hipX: 0.45, legT: 0.55, torso: [1.6, 1.9, 1.2], lean: 12, head: [0.8, 0.8, 0.75], headShape: 'sphere', shoulderX: 1.0, armT: 0.45, armLen: 1.9, fist: 0.7, gait: [30, 1.5], aim: 180 });
    const r = b.r;
    r.parts.push(P('sphere', [1.9, 1.3, 1.5], [0, 2.9, 0.1], { role: 'fur', group: 'torso', solid: false }));
    r.parts.push(P('box', [0.5, 0.18, 0.1], [0, b.headC[1] - 0.2, b.headC[2] + 0.38], { role: 'mouth', group: 'torso', solid: false }));
    for (const s of [-1, 1]) for (const dx of [-0.15, 0.15]) r.parts.push(P('cone', [0.08, 0.26, 0.08], [b.hand(s)[0] + dx, b.hand(s)[1] - 0.3, b.hand(s)[2] + 0.2], { role: 'claw', rot: [150, 0, 0], group: s < 0 ? 'arm_l' : 'arm_r', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 17, turn: 170, jump: 10 },
    HUNT(45),
    { type: 'weapon', kind: 'bomb', group: 'arm_l', rate: 0.5, speed: 26, damage: 20, range: 30, size: 0.6 },
    bite('arm_r', 26, 1, 2.2),
  ],
  functions: [...WALKER, 'shoots', 'projectile_attack', 'throw_target'],
  verb: 'strike',
};

/** Minotaur: bull-headed axe fighter that charges you down; like any charger it can't take a corner — dodge sideways. */
export const minotaur: CatalogEntry = {
  id: 'minotaur',
  names: ['minotaurs', 'minitaur', 'minotour', 'minatour', 'minotuar', 'bull man', 'bullman', 'bull headed man', 'labyrinth beast', 'asterion', 'taurus'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 600,
  build() {
    const b = biped({ hipY: 1.5, hipX: 0.42, legT: 0.45, torso: [1.5, 1.8, 1.0], lean: 18, head: [0.75, 0.75, 0.9], shoulderX: 1.0, armT: 0.36, armLen: 1.7, gait: [34, 1.8], aim: 110, knee: -0.3, strike: -120 });
    const r = b.r;
    const hc = b.headC;
    r.parts.push(P('box', [0.5, 0.4, 0.4], [0, hc[1] - 0.2, hc[2] + 0.55], { role: 'snout', group: 'torso', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(limb('cone', [s * 0.35, hc[1] + 0.25, hc[2]], [s * 0.95, hc[1] + 0.7, hc[2] + 0.45], 0.18, { role: 'horn', group: 'torso', solid: false }));
      r.parts.push(P('sphere', [0.12, 0.12, 0.12], [s * 0.95, hc[1] + 0.7, hc[2] + 0.45], { role: 'muzzle', group: 'torso', solid: false }));
    }
    const h = b.hand(1);
    r.parts.push(limb('cylinder', [h[0], h[1] - 0.4, h[2]], [h[0], h[1] + 0.4, h[2] + 1.9], 0.14, { role: 'haft', group: 'arm_r', solid: false }));
    r.parts.push(P('box', [0.1, 1.0, 0.8], [h[0], h[1] + 0.45, h[2] + 1.75], { role: 'axe', group: 'arm_r', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 30, turn: 70, jump: 8 },
    HUNT(50),
    bite('arm_r', 32, 1, 2.4),
    bite('torso', 22, 0.8, 1.2),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Werewolf: hunched two-legged hunter at the chaser cap — claws both sides and a snapping muzzle. */
export const werewolf: CatalogEntry = {
  id: 'werewolf',
  names: ['werewolves', 'werwolf', 'ware wolf', 'were wolf', 'lycan', 'lycans', 'lycanthrope', 'wolfman', 'wolf man', 'wolfmen', 'beast man', 'wolf monster', 'hellhound'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const b = biped({ hipY: 1.25, hipX: 0.34, legT: 0.34, torso: [1.1, 1.5, 0.8], lean: 30, head: [0.55, 0.5, 0.55], shoulderX: 0.75, armT: 0.26, armLen: 1.5, fist: 0.32, gait: [42, 2.4], aim: 280, knee: -0.35 });
    const r = b.r;
    const hc = b.headC;
    r.parts.push(P('box', [0.32, 0.26, 0.5], [0, hc[1] - 0.08, hc[2] + 0.5], { role: 'snout', group: 'torso', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.14, 0.34, 0.1], [s * 0.18, hc[1] + 0.38, hc[2] - 0.05], { role: 'ear', group: 'torso', solid: false }));
    r.parts.push(P('sphere', [1.3, 0.9, 0.9], [0, b.top - 0.2, 0.3], { role: 'mane', group: 'torso', solid: false }));
    chain(r, 'tail', [0, 1.3, -0.4], [0, -0.1, -0.5], 2, 0.2, 0.14, () => [wave(20, 1.5, [0, 1, 0])]);
    for (const s of [-1, 1]) for (const dx of [-0.08, 0.08]) r.parts.push(P('cone', [0.05, 0.22, 0.05], [b.hand(s)[0] + dx, b.hand(s)[1] - 0.2, b.hand(s)[2] + 0.12], { role: 'claw', rot: [150, 0, 0], group: s < 0 ? 'arm_l' : 'arm_r', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 33, turn: 260, jump: 12 },
    HUNT(50),
    bite('arm_r', 17, 1.6, 1.7),
    bite('arm_l', 17, 1.4, 1.7),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

// ================================================================== undead

/** Zombie horde: four shamblers lurching as one mob. Slow, relentless, bites whatever it swarms. */
export const zombies: CatalogEntry = {
  id: 'zombies',
  names: ['zombie', 'zombie horde', 'horde of zombies', 'undead', 'the undead', 'undead horde', 'ghoul', 'ghouls', 'zombi', 'zombies horde', 'zombie mob', 'walking dead', 'zombeh', 'zomby', 'zombys', 'zombiez', 'brain eaters', 'shamblers', 'revenants'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    const spots: [number, number, number][] = [[-0.9, 0.6, 10], [0.9, 0.4, -15], [-0.5, -1.0, 5], [0.7, -1.1, -8]];
    spots.forEach(([x, z, yaw], i) => {
      const g = `z${i}`;
      const body: Part[] = [
        P('box', [0.62, 0.8, 0.4], [0, 1.3, 0.05], { role: 'body', rot: [12, 0, 0] }),
        P('sphere', [0.42, 0.44, 0.42], [0.06, 1.92, 0.2], { role: 'head', solid: false }),
        ...[-1, 1].map((s) => limb('capsule', [s * 0.36, 1.55, 0.1], [s * 0.3, 1.45, 0.8], 0.14, { role: 'arm', solid: false })),
      ];
      r.parts.push(...placed(body, yaw, [x, 0, z], g));
      r.joints.push(J(g, [x, 0.9, z], [swing(9, 0.6 + i * 0.07, [0, 0, 1], i * 0.3), bob(0.06, 1.8, i * 0.25)]));
      for (const s of [-1, 1]) {
        const lg = `${g}${s < 0 ? 'l' : 'r'}`;
        const hip = placed([P('box', [0.1, 0.1, 0.1], [s * 0.16, 0.9, 0])], yaw, [x, 0, z])[0].pos;
        const legPart = placed([limb('capsule', [s * 0.16, 0.9, 0], [s * 0.18, 0.08, 0.05], 0.18, { role: 'leg', solid: false })], yaw, [x, 0, z], lg);
        r.parts.push(...legPart);
        r.joints.push(J(lg, hip, [gait(24, 1.6, (s < 0 ? 0 : 0.5) + i * 0.13)], g));
      }
    });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 5, turn: 120 },
    HUNT(50),
    bodyHit(10, 1.5, 1.0),
  ],
  functions: [...WALKER, 'area_attack'],
  verb: 'strike',
};

/** Skeleton archers: three bony bowmen who keep their distance and loose arrows. Close in, or hide behind a wall. */
export const skeletons: CatalogEntry = {
  id: 'skeletons',
  names: ['skeleton', 'skeleton archer', 'skeleton archers', 'skeleton army', 'bone archers', 'skelly', 'skellies', 'skeletal archers', 'skeliton', 'skelaton', 'skeletton', 'skeleton warriors', 'spooky scary skeletons', 'undead archers', 'archers'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    [-1.6, 0, 1.6].forEach((x, i) => {
      const g = `s${i}`;
      const z = i === 1 ? 0.5 : 0;
      r.parts.push(P('box', [0.5, 0.14, 0.3], [x, 1.0, z], { role: 'hips', group: g }));
      r.parts.push(P('cylinder', [0.12, 0.7, 0.12], [x, 1.4, z], { role: 'spine', group: g, solid: false }));
      for (const y of [1.35, 1.55, 1.75]) r.parts.push(P('torus', [0.55, 0.06, 0.4], [x, y, z], { role: 'rib', group: g, solid: false }));
      r.parts.push(P('sphere', [0.4, 0.42, 0.42], [x, 2.1, z + 0.02], { role: 'skull', group: g, solid: false }));
      for (const s of [-1, 1]) r.parts.push(P('sphere', [0.1, 0.1, 0.05], [x + s * 0.1, 2.12, z + 0.2], { role: 'eye', group: g, solid: false }));
      r.parts.push(limb('capsule', [x - 0.15, 0.95, z], [x - 0.17, 0.05, z], 0.1, { role: 'leg', group: g, solid: false }));
      r.parts.push(limb('capsule', [x + 0.15, 0.95, z], [x + 0.17, 0.05, z], 0.1, { role: 'leg', group: g, solid: false }));
      // bow arm held out in front, pitches to aim
      const bg = `bow${i}`;
      r.parts.push(limb('capsule', [x - 0.3, 1.8, z], [x - 0.2, 1.75, z + 0.7], 0.08, { role: 'arm', group: bg, solid: false }));
      r.parts.push(P('torus', [0.1, 0.05, 1.1], [x - 0.2, 1.75, z + 0.75], { role: 'bow', rot: [0, 0, 90], group: bg, solid: false }));
      r.parts.push(P('sphere', [0.1, 0.1, 0.1], [x - 0.2, 1.75, z + 0.95], { role: 'muzzle', group: bg, solid: false }));
      r.joints.push(J(g, [x, 0.05, z], [aimYaw(200), bob(0.05, 2.4, i * 0.3)]), J(bg, [x - 0.3, 1.8, z], [aimPitch(200, -40, 60), recoil(0.15, 4)], g));
    });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 6, turn: 150, jump: 6 },
    HUNT(50),
    { type: 'weapon', kind: 'gun', group: 'bow1', rate: 0.8, speed: 60, damage: 10, range: 42, size: 0.1, spread: 2 },
    { type: 'weapon', kind: 'gun', group: 'bow0', rate: 0.7, speed: 60, damage: 10, range: 42, size: 0.1, spread: 3 },
    { type: 'weapon', kind: 'gun', group: 'bow2', rate: 0.75, speed: 60, damage: 10, range: 42, size: 0.1, spread: 3 },
  ],
  functions: ['walks', 'drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', ...RANGED, 'multiple_agents'],
  verb: 'shoot',
};

/** Mummy: shambling bandaged pharaoh — flings wrappings that bind you in place, then claws at you. */
export const mummy: CatalogEntry = {
  id: 'mummy',
  names: ['mummies', 'the mummy', 'pharaoh mummy', 'mummy pharaoh', 'wrapped mummy', 'bandage man', 'bandaged mummy', 'mumy', 'mummie', 'mummys', 'egyptian mummy', 'tomb guardian'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const b = biped({ hipY: 1.0, hipX: 0.2, legT: 0.24, torso: [0.7, 0.95, 0.42], head: [0.42, 0.48, 0.45], shoulderX: 0.45, armT: 0.16, armLen: 0.95, fist: 0.18, gait: [20, 1.0], aim: 140, reach: true, strike: -35 });
    const r = b.r;
    for (const y of [1.3, 1.6, 1.9]) r.parts.push(P('torus', [0.82, 0.05, 0.56], [0, y, 0], { role: 'wrap', rot: [0, 0, y * 10 - 15], group: 'torso', solid: false }));
    r.parts.push(P('box', [0.1, 0.3, 0.02], [0.3, 1.3, 0.25], { role: 'wrap', rot: [0, 0, 20], group: 'arm_r', solid: false }));
    r.parts.push(P('cone', [0.55, 0.5, 0.5], [0, b.headC[1] + 0.35, b.headC[2] - 0.05], { role: 'headdress', group: 'torso', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 7, turn: 130 },
    HUNT(45),
    { type: 'weapon', kind: 'net', group: 'arm_r', rate: 0.45, speed: 30, damage: 5, range: 22, size: 0.55 },
    bite('arm_l', 16, 1, 1.6),
  ],
  functions: [...WALKER, 'shoots', 'projectile_attack', 'restrain_target'],
  verb: 'trap',
};

/** Ghost: a drifting wraith with a waving tail and a chilling wail (short cone). Solid only at its lantern-heart. */
export const ghost: CatalogEntry = {
  id: 'ghost',
  names: ['ghosts', 'spirit', 'spectre', 'specter', 'phantom', 'poltergeist', 'wraith', 'banshee', 'spook', 'spooky ghost', 'ghosty', 'ghostie', 'casper', 'gost', 'ghoast', 'goast', 'apparition', 'spectral', 'lich', 'dementor', 'boogeyman', 'bogeyman'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'front',
  lifetime: 120,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [0.5, 0.5, 0.5], [0, 1.6, 0], { role: 'heart' }));
    r.parts.push(P('sphere', [1.2, 1.4, 1.1], [0, 1.8, 0], { role: 'body', group: 'shroud', solid: false }));
    r.parts.push(P('dome', [1.25, 0.8, 1.15], [0, 2.3, 0], { role: 'hood', group: 'shroud', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.2, 0.28, 0.1], [s * 0.22, 2.1, 0.52], { role: 'eye', group: 'shroud', solid: false }));
    r.parts.push(P('sphere', [0.3, 0.4, 0.1], [0, 1.72, 0.55], { role: 'mouth', group: 'shroud', solid: false }));
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [0, 1.72, 0.65], { role: 'muzzle', group: 'shroud', solid: false }));
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'arm_l' : 'arm_r';
      r.parts.push(limb('cone', [s * 0.5, 1.9, 0.1], [s * 0.7, 1.6, 0.9], 0.3, { role: 'arm', group: g, solid: false }));
      r.joints.push(J(g, [s * 0.5, 1.9, 0.1], [swing(18, 0.6, [0, 0, s], s < 0 ? 0 : 0.5)], 'shroud'));
    }
    chain(r, 'wisp', [0, 1.2, -0.1], (i) => [0, -0.25, -0.45 - i * 0.05], 3, 0.8, 0.12, (i) => [wave(24, 0.8, [0, 1, 0]), swing(10, 0.5, [1, 0, 0], i * 0.2)], { role: 'tail', tip: 'cone', parent: 'shroud' });
    r.joints.push(J('shroud', [0, 1.6, 0], [aimYaw(160), bob(0.25, 0.4, 0, false)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 11, turn: 160, altitude: 1.0 },
    HUNT(40),
    { type: 'weapon', kind: 'flame', group: 'shroud', rate: 5, speed: 0, damage: 3, range: 8 },
    bodyHit(12, 1, 1.0),
  ],
  functions: ['moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'shoots', 'area_attack'],
  verb: 'strike',
};

// ================================================================== dragons & big monsters

function dragonBody(r: Rig, o: { len: number; girth: number; y: number; neckTo: Vec3; head: Vec3; wings?: [number, number]; spikes?: 'ice' | 'plates' }) {
  const { len, girth, y } = o;
  r.parts.push(P('capsule', [girth, len, girth * 1.05], [0, y, 0], { role: 'body', rot: [90, 0, 0] }));
  const base: Vec3 = [0, y + girth * 0.2, len / 2 - girth * 0.3];
  r.parts.push(limb('cylinder', base, o.neckTo, girth * 0.42, { role: 'neck', group: 'neck', solid: false }));
  r.parts.push(P('box', o.head, [0, o.neckTo[1] + 0.1, o.neckTo[2] + o.head[2] * 0.35], { role: 'head', group: 'head', solid: false }));
  r.parts.push(P('box', [o.head[0] * 0.7, o.head[1] * 0.25, o.head[2] * 0.8], [0, o.neckTo[1] - o.head[1] * 0.45, o.neckTo[2] + o.head[2] * 0.45], { role: 'jaw', group: 'jaw', solid: false }));
  r.parts.push(P('sphere', [0.24, 0.24, 0.24], [0, o.neckTo[1] - 0.05, o.neckTo[2] + o.head[2] * 0.95], { role: 'muzzle', group: 'head', solid: false }));
  r.parts.push(P('sphere', [0.18, 0.18, 0.18], [0, o.neckTo[1] - o.head[1] * 0.45, o.neckTo[2] + o.head[2] * 0.85], { role: 'muzzle', group: 'jaw', solid: false }));
  for (const s of [-1, 1]) {
    r.parts.push(P('cone', [0.18, 0.8, 0.18], [s * 0.3, o.neckTo[1] + o.head[1] * 0.7, o.neckTo[2]], { role: 'horn', rot: [-35, 0, 0], group: 'head', solid: false }));
    r.parts.push(P('sphere', [0.14, 0.14, 0.1], [s * o.head[0] * 0.35, o.neckTo[1] + o.head[1] * 0.25, o.neckTo[2] + o.head[2] * 0.6], { role: 'eye', group: 'head', solid: false }));
  }
  if (o.spikes) for (let i = 0; i < 4; i++) {
    const z = len * 0.35 - i * len * 0.22;
    r.parts.push(o.spikes === 'ice'
      ? P('cone', [0.35, 1.0 - i * 0.12, 0.35], [0, y + girth * 0.55 + 0.3, z], { role: 'crystal', rot: [-15, 0, 0], solid: false })
      : P('wedge', [0.12, 0.9 - i * 0.1, 0.8], [0, y + girth * 0.55 + 0.25, z], { role: 'plate', solid: false }));
  }
  r.joints.push(
    J('neck', base, [swing(6, 0.5, [1, 0, 0])]),
    J('head', [0, o.neckTo[1], o.neckTo[2]], [aimBoth(140, -70, 35)], 'neck'),
    J('jaw', [0, o.neckTo[1] - o.head[1] * 0.35, o.neckTo[2]], [snap(24, 3)], 'head'),
  );
  if (o.wings) wingPair(r, 'wing', [girth * 0.4, y + girth * 0.35, len * 0.1], o.wings[0], o.wings[1], 38, 1.1);
}

/** Ice dragon: winged, crystal-spined, breathes a freezing beam from the air. Rideable. Break line of sight. */
export const iceDragon: CatalogEntry = {
  id: 'ice_dragon',
  names: ['ice dragon', 'frost dragon', 'frost wyrm', 'frostwyrm', 'snow dragon', 'winter dragon', 'crystal dragon', 'ice wyvern', 'frost drake', 'ice drake', 'blizzard dragon', 'glacier dragon', 'white dragon', 'freezing dragon', 'ice breathing dragon'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    dragonBody(r, { len: 4.4, girth: 1.7, y: 2.2, neckTo: [0, 3.9, 3.2], head: [0.9, 0.7, 1.3], wings: [5.0, 2.4], spikes: 'ice' });
    legs4(r, 1.6, 0.6, 1.2, -1.1, 0.34, 18, 1.2);
    chain(r, 'tail', [0, 2.1, -2.2], [0, -0.1, -1.4], 3, 0.7, 0.3, () => [wave(18, 0.7, [0, 1, 0])], { tip: 'cone' });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 24, turn: 100, altitude: 9 },
    HUNT(50),
    { type: 'weapon', kind: 'laser', group: 'head', rate: 2, speed: 0, damage: 13, range: 40 },
    bite('jaw', 26, 0.8, 2.6),
    { type: 'rideable', seat: [0, 3.7, 0.2] },
  ],
  functions: [...FLYER, ...RANGED, ...RIDE, 'raise_player'],
  verb: 'shoot',
};

/** Storm dragon: a long wingless sky-serpent rippling through the air, spitting rapid lightning. */
export const stormDragon: CatalogEntry = {
  id: 'storm_dragon',
  names: ['storm dragon', 'lightning dragon', 'thunder dragon', 'electric dragon', 'chinese dragon', 'eastern dragon', 'sky serpent', 'lung dragon', 'shenron', 'shenlong', 'sky dragon', 'cloud dragon', 'dragon serpent', 'serpent dragon', 'long dragon', 'thunder serpent'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.9, 2.0, 0.9], [0, 2.0, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('box', [0.8, 0.6, 1.2], [0, 2.3, 1.6], { role: 'head', group: 'head', solid: false }));
    r.parts.push(P('box', [0.6, 0.18, 0.9], [0, 1.95, 1.75], { role: 'jaw', group: 'head', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 2.2, 2.3], { role: 'muzzle', group: 'head', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(limb('cone', [s * 0.25, 2.55, 1.3], [s * 0.6, 3.2, 0.7], 0.12, { role: 'antler', group: 'head', solid: false }));
      r.parts.push(limb('cylinder', [s * 0.3, 2.1, 2.1], [s * 1.2, 1.9, 2.6], 0.04, { role: 'whisker', group: 'head', solid: false }));
    }
    const body = chain(r, 'body', [0, 2.0, -1.0], [0, 0, -1.1], 6, 0.85, 0.3, (i) => [wave(22, 0.6, [0, 1, 0]), swing(8, 0.6, [1, 0, 0], i * 0.15)], { tip: 'cone' });
    for (const [i, s] of [[0, -1], [0, 1], [3, -1], [3, 1]] as [number, number][]) {
      const zc = -1.0 - i * 1.1 - 0.5;
      r.parts.push(limb('capsule', [s * 0.4, 1.8, zc], [s * 0.6, 1.2, zc + 0.2], 0.12, { role: 'leg', group: body.groups[i], solid: false }));
    }
    r.joints.push(J('head', [0, 2.2, 1.0], [aimBoth(200, -70, 40)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 30, turn: 160, altitude: 8 },
    HUNT(50),
    { type: 'weapon', kind: 'laser', group: 'head', rate: 3, speed: 0, damage: 7, range: 35 },
  ],
  functions: [...FLYER, ...RANGED],
  verb: 'shoot',
};

/** Hydra: three heads on long necks — one breathes fire, one bites, one spits a beam. Walks slowly on four legs. */
export const hydra: CatalogEntry = {
  id: 'hydra',
  names: ['hydras', 'hidra', 'hydra monster', 'three headed dragon', 'three-headed dragon', 'three headed monster', 'multi headed dragon', 'many headed monster', 'lernaean hydra', 'ghidorah', 'king ghidorah', 'three headed snake', 'three headed serpent'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [2.4, 3.6, 2.4], [0, 2.3, -0.2], { role: 'body', rot: [90, 0, 0] }));
    legs4(r, 1.7, 0.9, 0.9, -1.1, 0.5, 18, 1.0);
    const heads: [string, number, number][] = [['l', -1, 0], ['m', 0, 0.3], ['r', 1, 0.6]];
    for (const [k, s, ph] of heads) {
      const n = chain(r, `neck_${k}`, [s * 0.6, 2.8, 1.2], (i) => [s * 0.45, 0.9 - i * 0.25, 0.6], 2, 0.55, 0.4, () => [wave(12, 0.5, [0, 1, 0], ph)], { role: 'neck' });
      const hg = `head_${k}`;
      const hc: Vec3 = [n.tip[0], n.tip[1] + 0.1, n.tip[2] + 0.45];
      r.parts.push(P('box', [0.6, 0.5, 0.9], hc, { role: 'head', group: hg, solid: false }));
      r.parts.push(P('box', [0.45, 0.14, 0.7], [hc[0], hc[1] - 0.3, hc[2] + 0.1], { role: 'jaw', group: hg, solid: false }));
      r.parts.push(P('sphere', [0.16, 0.16, 0.16], [hc[0], hc[1] - 0.1, hc[2] + 0.55], { role: 'muzzle', group: hg, solid: false }));
      r.joints.push(J(hg, n.tip, [aimBoth(160, -70, 30), snap(18, 3)], n.last));
    }
    chain(r, 'tail', [0, 2.2, -2.0], [0, -0.2, -1.2], 2, 0.8, 0.35, () => [wave(15, 0.6, [0, 1, 0])], { tip: 'cone' });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 12, turn: 110, jump: 5 },
    HUNT(50),
    { type: 'weapon', kind: 'flame', group: 'head_l', rate: 8, speed: 0, damage: 4, range: 14 },
    bite('head_m', 30, 1, 2.8),
    { type: 'weapon', kind: 'laser', group: 'head_r', rate: 1.2, speed: 0, damage: 12, range: 40 },
  ],
  functions: [...WALKER, ...RANGED, 'area_attack'],
  verb: 'strike',
};

/** Kaiju: a city-sized lizard — atomic breath beam, crushing bite, ground-shaking stomp. Very slow: outrun and flank. */
export const kaiju: CatalogEntry = {
  id: 'kaiju',
  names: ['godzilla', 'gojira', 'giant monster', 'giant lizard', 'kaiju monster', 'city stomper', 'atomic lizard', 'radioactive lizard', 'big lizard', 'monster lizard', 'kaju', 'kaijuu', 'godzila', 'godzillla', 'gozilla', 'king of the monsters', 'kaijus'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [3.4, 5.4, 3.0], [0, 5.6, -0.2], { role: 'body', rot: [20, 0, 0] }));
    r.parts.push(P('sphere', [2.8, 2.6, 2.4], [0, 4.2, 0.4], { role: 'belly', solid: false }));
    r.parts.push(limb('capsule', [0, 7.4, 0.6], [0, 8.4, 1.6], 1.6, { role: 'neck', group: 'head', solid: false }));
    r.parts.push(P('box', [1.6, 1.4, 2.2], [0, 8.8, 2.4], { role: 'head', group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.25, 0.25, 0.15], [s * 0.55, 9.2, 3.3], { role: 'eye', group: 'head', solid: false }));
    r.parts.push(P('box', [1.3, 0.4, 1.9], [0, 8.0, 2.6], { role: 'jaw', group: 'jaw', solid: false }));
    r.parts.push(P('sphere', [0.3, 0.3, 0.3], [0, 8.4, 3.6], { role: 'muzzle', group: 'jaw', solid: false }));
    for (let i = 0; i < 5; i++) r.parts.push(P('wedge', [0.3, 1.6 - i * 0.18, 1.1], [0, 7.4 - i * 0.9, -1.4 - i * 0.4], { role: 'plate', rot: [-20, 0, 0], solid: false }));
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'arm_l' : 'arm_r';
      r.parts.push(limb('capsule', [s * 1.6, 6.4, 1.0], [s * 1.8, 5.2, 2.0], 0.5, { role: 'arm', group: g, solid: false }));
      r.joints.push(J(g, [s * 1.6, 6.4, 1.0], [swing(20, 0.6, [1, 0, 0], s < 0 ? 0 : 0.5)]));
    }
    walkLeg(r, 'leg_l', [-1.3, 3.8, -0.4], 1.3, [gait(22, 0.6, 0)], { knee: 0.6, foot: 2.0 });
    walkLeg(r, 'leg_r', [1.3, 3.8, -0.4], 1.3, [gait(22, 0.6, 0.5)], { knee: 0.6, foot: 2.0 });
    chain(r, 'tail', [0, 3.6, -2.4], (i) => [0, -0.8 + i * 0.2, -2.0], 4, 1.8, 0.4, () => [wave(12, 0.4, [0, 1, 0])], { tip: 'cone' });
    r.joints.push(J('head', [0, 7.4, 0.6], [aimBoth(80, -60, 30)]), J('jaw', [0, 8.2, 1.8], [snap(22, 2)], 'head'));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 60, jump: 5 },
    HUNT(60),
    { type: 'weapon', kind: 'laser', group: 'head', rate: 0.7, speed: 0, damage: 26, range: 60 },
    bite('jaw', 45, 0.7, 4.5),
    bodyHit(22, 0.5, 2.5),
  ],
  functions: [...WALKER, ...RANGED, 'area_attack', 'destroys_target'],
  verb: 'strike',
};

/** Mecha-kaiju: a robot dinosaur with missile pods on its back, a chest beam and a steel bite. Climb into the head. */
export const mechaKaiju: CatalogEntry = {
  id: 'mechagodzilla',
  names: ['mecha godzilla', 'mecha-godzilla', 'mechagodzila', 'robot dinosaur', 'robo rex', 'robot t-rex', 'robot trex', 'mecha rex', 'mecha t-rex', 'mecha trex', 'dinobot', 'zoid', 'robosaurus', 'mecha kaiju', 'robot kaiju', 'mech kaiju', 'mechasaurus', 'robot godzilla', 'dino mech', 'dinosaur mech'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.4, 2.8, 3.4], [0, 4.4, 0], { role: 'body', rot: [15, 0, 0] }));
    r.parts.push(P('box', [1.6, 1.0, 2.0], [0, 6.2, 1.9], { role: 'head', group: 'head', solid: false }));
    r.parts.push(P('dome', [1.0, 0.5, 1.2], [0, 6.75, 1.7], { role: 'cockpit', group: 'head', solid: false }));
    r.parts.push(P('box', [1.3, 0.35, 1.7], [0, 5.55, 2.1], { role: 'jaw', group: 'jaw', solid: false }));
    r.parts.push(P('sphere', [0.25, 0.25, 0.25], [0, 5.8, 3.05], { role: 'muzzle', group: 'jaw', solid: false }));
    // chest beam
    r.parts.push(P('cylinder', [0.7, 0.4, 0.7], [0, 4.6, 1.75], { role: 'emitter', rot: [90, 0, 0], group: 'chest', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 4.6, 2.0], { role: 'muzzle', group: 'chest', solid: false }));
    // missile pods on the back
    for (const s of [-1, 1]) {
      r.parts.push(P('box', [0.8, 0.8, 1.4], [s * 0.9, 6.1, -0.8], { role: 'pod', group: 'pods', solid: false }));
      r.parts.push(P('sphere', [0.2, 0.2, 0.2], [s * 0.9, 6.1, -0.05], { role: 'muzzle', group: 'pods', solid: false }));
    }
    walkLeg(r, 'leg_l', [-1.0, 3.2, -0.3], 1.0, [gait(24, 0.7, 0)], { knee: -0.8, foot: 1.6 });
    walkLeg(r, 'leg_r', [1.0, 3.2, -0.3], 1.0, [gait(24, 0.7, 0.5)], { knee: -0.8, foot: 1.6 });
    chain(r, 'tail', [0, 3.8, -1.8], (i) => [0, -0.4, -1.6], 3, 1.1, 0.4, () => [wave(12, 0.5, [0, 1, 0])], { shape: 'cylinder', tip: 'cone' });
    r.joints.push(
      J('head', [0, 5.6, 1.2], [aimYaw(100)]),
      J('jaw', [0, 5.7, 1.3], [snap(22, 2.5)], 'head'),
      J('chest', [0, 4.6, 1.6], [aimBoth(120, -50, 30)]),
      J('pods', [0, 6.1, -0.8], [aimPitch(60, 10, 70), recoil(0.3, 3)]),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 10, turn: 70, jump: 5 },
    HUNT(60),
    { type: 'weapon', kind: 'missile', group: 'pods', rate: 0.5, speed: 32, damage: 26, range: 60, size: 0.35 },
    { type: 'weapon', kind: 'laser', group: 'chest', rate: 1, speed: 0, damage: 16, range: 50 },
    bite('jaw', 36, 0.8, 3.5),
    { type: 'rideable', seat: [0, 7.2, 1.5] },
  ],
  functions: [...WALKER, ...RANGED, ...RIDE],
  verb: 'shoot',
};

/** Sandworm: a colossal segmented worm that ripples along the ground and swallows what's in front of its ring of teeth. */
export const sandworm: CatalogEntry = {
  id: 'sandworm',
  names: ['sand worm', 'sandworms', 'giant worm', 'death worm', 'mongolian death worm', 'graboid', 'graboids', 'shai hulud', 'shai-hulud', 'purple worm', 'tremor worm', 'worm monster', 'sarlacc', 'giant earthworm', 'megaworm'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('cylinder', [2.0, 1.8, 2.0], [0, 1.3, 0.4], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('torus', [2.0, 0.4, 2.0], [0, 1.4, 1.4], { role: 'mouth', rot: [90, 0, 0], group: 'maw', solid: false }));
    for (let k = 0; k < 6; k++) {
      const a = (k / 6) * Math.PI * 2;
      r.parts.push(P('cone', [0.2, 0.55, 0.2], [Math.cos(a) * 0.65, 1.4 + Math.sin(a) * 0.65, 1.55], { role: 'tooth', rot: [0, 0, (a * 180) / Math.PI + 90], group: 'maw', solid: false }));
    }
    r.parts.push(P('sphere', [0.25, 0.25, 0.25], [0, 1.4, 1.9], { role: 'muzzle', group: 'maw', solid: false }));
    chain(r, 'seg', [0, 1.2, -0.5], (i) => [0, -0.05, -1.3], 5, 1.8, 0.8, () => [wave(18, 0.7, [0, 1, 0])], { shape: 'cylinder', tip: 'cone' });
    r.joints.push(J('maw', [0, 1.4, 1.2], [spin(0.4, [0, 0, 1]), snap(15, 2)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 20, turn: 80 },
    HUNT(50),
    bite('maw', 40, 0.8, 2.2),
  ],
  functions: ['drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'pushes_objects'],
  verb: 'strike',
};

// ================================================================== flying mounts

/** Griffin: eagle-headed, lion-bodied flying mount you're put on. Follows you when you get off; talons strike. */
export const griffin: CatalogEntry = {
  id: 'griffin',
  names: ['gryphon', 'griffon', 'griffen', 'grifin', 'gryphen', 'griffins', 'hippogriff', 'hippogryph', 'buckbeak', 'griffin mount', 'war griffin'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [1.2, 2.4, 1.3], [0, 1.9, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('sphere', [1.1, 1.1, 1.1], [0, 2.3, 1.0], { role: 'chest', solid: false }));
    r.parts.push(P('sphere', [0.7, 0.7, 0.75], [0, 3.0, 1.6], { role: 'head', group: 'head', solid: false }));
    r.parts.push(P('cone', [0.3, 0.55, 0.32], [0, 2.85, 2.1], { role: 'beak', rot: [110, 0, 0], group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.1, 0.1, 0.06], [s * 0.24, 3.12, 1.9], { role: 'eye', group: 'head', solid: false }));
    legs4(r, 1.6, 0.35, 0.7, -0.8, 0.22, 26, 2);
    r.joints.find((j) => j.group === 'leg_fl')!.motion.push(snap(-70, 3));
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [-0.35, 0.2, 1.0], { role: 'muzzle', group: 'leg_fl', solid: false }));
    wingPair(r, 'wing', [0.45, 2.4, 0.4], 3.4, 1.4, 36, 1.3, { tip: false });
    chain(r, 'tail', [0, 2.0, -1.2], [0, -0.1, -0.6], 2, 0.16, 0.12, () => [wave(20, 1, [0, 1, 0])]);
    r.parts.push(P('box', [0.8, 0.16, 0.8], [0, 2.55, -0.1], { role: 'saddle', solid: false }));
    r.joints.push(J('head', [0, 2.6, 1.3], [aimYaw(200)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 30, turn: 130, altitude: 8 },
    FOLLOW(35),
    bite('leg_fl', 18, 1.2, 2.6),
    { type: 'rideable', seat: [0, 3.35, -0.1] },
  ],
  functions: ['flies', 'has_moving_parts', 'follows_player', 'returns', 'attacks_enemies', 'damages_target', ...RIDE, 'raise_player', 'slow_fall'],
};

/** Phoenix: a blazing bird that hunts on its own, breathing fire in passes. Hop on (G) to fly it. */
export const phoenix: CatalogEntry = {
  id: 'phoenix',
  names: ['firebird', 'fire bird', 'pheonix', 'phenix', 'phoneix', 'phoenix bird', 'fawkes', 'flame bird', 'sun bird', 'sunbird', 'thunderbird', 'fire phoenix', 'fire eagle'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.9, 2.0, 1.0], [0, 2.0, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('sphere', [0.6, 0.6, 0.65], [0, 2.35, 1.2], { role: 'head', group: 'head', solid: false }));
    r.parts.push(P('cone', [0.22, 0.45, 0.24], [0, 2.25, 1.6], { role: 'beak', rot: [105, 0, 0], group: 'head', solid: false }));
    for (let i = 0; i < 3; i++) r.parts.push(P('cone', [0.14, 0.6, 0.14], [0, 2.75, 1.05 - i * 0.2], { role: 'crest', rot: [-30 - i * 12, 0, 0], group: 'head', solid: false }));
    r.parts.push(P('sphere', [0.14, 0.14, 0.14], [0, 2.2, 1.8], { role: 'muzzle', group: 'head', solid: false }));
    wingPair(r, 'wing', [0.4, 2.2, 0.2], 3.2, 1.3, 40, 1.4, { tip: false });
    for (const [x, ph] of [[-0.25, 0], [0, 0.3], [0.25, 0.6]] as [number, number][]) {
      chain(r, `plume${ph * 10}_`, [x, 1.95, -0.95], (i) => [x * 0.6, -0.2 - i * 0.1, -0.9], 2, 0.3, 0.12, () => [wave(18, 0.9, [0, 1, 0], ph)], { role: 'tail', tip: 'cone' });
    }
    r.joints.push(J('head', [0, 2.2, 0.9], [aimBoth(200, -75, 30)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 32, turn: 190, altitude: 8 },
    HUNT(50),
    { type: 'weapon', kind: 'flame', group: 'head', rate: 10, speed: 0, damage: 5, range: 14 },
    { type: 'rideable', seat: [0, 2.85, -0.2] },
  ],
  functions: [...FLYER, 'shoots', 'area_attack', ...RIDE, 'raise_player'],
  verb: 'shoot',
};
