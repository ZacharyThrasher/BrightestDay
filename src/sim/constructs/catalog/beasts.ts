/**
 * Combat beasts: animals the ring builds to fight for you. Every one hunts on its own (or carries you), attacks with
 * articulated jaws / claws / horns / stingers, and has a counter:
 *  - chasers (wolves, raptors, big cats, sharks, swarms) run at the chaser cap with a limited turn rate — hard cuts,
 *    walls and boost bursts shake them;
 *  - chargers (rhino, bull, elephant) are fast but turn slowly, so a side-step makes them overshoot;
 *  - flyers (eagle, bats, bees) dive to the target's height to strike — a shield or a quick climb breaks the dive;
 *  - heavies (bear, gorilla, T-rex, crab) hit hard but are slower than a running lantern.
 * Frame: +z forward, +y up, feet on y = 0. See kit.ts for the helpers.
 */
import type { CatalogEntry } from './types.js';
import type { Behavior, Vec3 } from '../spec.js';
import { P, J, limb, gait, swing, wave, snap, recoil, aimYaw, aimBoth, bob, spin, newRig, walkLeg, legs4, chain, wingPair, placed, heading, type Rig } from './kit.js';

// ------------------------------------------------------------------ shared behaviour bits

export const HUNT = (range = 45): Behavior => ({ type: 'brain', goal: 'hunt', target: { kind: 'nearest_enemy' }, range });
export const FOLLOW = (range = 30): Behavior => ({ type: 'brain', goal: 'follow', target: { kind: 'self' }, range });
/** Melee from the muzzle parts of a group (jaws, claws, horns, stingers). */
export const bite = (group: string, damage: number, rate: number, range: number): Behavior => ({ type: 'weapon', kind: 'melee', group, rate, speed: 0, damage, range });
/** Melee with the whole body (swarms, stomps, trampling): hits what it touches. */
export const bodyHit = (damage: number, rate: number, range: number): Behavior => ({ type: 'weapon', kind: 'melee', rate, speed: 0, damage, range });

const WALKER = ['walks', 'drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'pushes_objects'];
const FLYER = ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target'];
const RIDE = ['rideable', 'carries_player', 'propel_player'];

// ------------------------------------------------------------------ body builders

export interface QuadOpts {
  len: number; girth: number; hipY: number; legT: number; legX: number;
  head: Vec3; neck?: number; neckZ?: number; snout?: Vec3;
  ears?: 'cone' | 'round' | 'none';
  gait: [number, number];
  tail?: { n: number; seg: number; t0: number; t1: number; amp: number; rate: number; lift?: number };
  aim?: number; jawAmp?: number;
}

/** Four-legged animal: solid body, yawing head with a biting jaw (muzzle at the jaw tip), trotting legs, waving tail. */
export function quad(o: QuadOpts): { r: Rig; by: number; hc: Vec3; tip: Vec3; neckBase: Vec3 } {
  const r = newRig();
  const G = o.girth, L = o.len;
  const by = o.hipY + G * 0.2;
  r.parts.push(P('capsule', [G, L, G], [0, by, 0], { role: 'body', rot: [90, 0, 0] }));
  r.parts.push(P('sphere', [G * 1.1, G * 1.08, G * 1.15], [0, by + G * 0.06, L * 0.28], { role: 'chest', solid: false }));
  const [hx, hy, hz] = o.head;
  const neckBase: Vec3 = [0, by + G * 0.1, L / 2 - G * 0.15];
  const hc: Vec3 = [0, by + (o.neck ?? G * 0.55), L / 2 + (o.neckZ ?? hz * 0.45)];
  r.parts.push(limb('capsule', neckBase, hc, Math.min(hx, G) * 0.8, { role: 'neck', group: 'head', solid: false }));
  r.parts.push(P('box', [hx, hy, hz], hc, { role: 'head', group: 'head', solid: false }));
  const sn = o.snout ?? [hx * 0.6, hy * 0.45, hz * 0.6];
  const snZ = hc[2] + hz / 2 + sn[2] / 2 - 0.02;
  r.parts.push(P('box', sn, [0, hc[1] - hy * 0.1, snZ], { role: 'snout', group: 'head', solid: false }));
  for (const s of [-1, 1]) r.parts.push(P('sphere', [hx * 0.16, hx * 0.16, 0.06], [s * hx * 0.3, hc[1] + hy * 0.18, hc[2] + hz / 2], { role: 'eye', group: 'head', solid: false }));
  if (o.ears !== 'none') {
    for (const s of [-1, 1]) {
      if (o.ears === 'round') r.parts.push(P('sphere', [hx * 0.3, hx * 0.3, hx * 0.12], [s * hx * 0.36, hc[1] + hy * 0.55, hc[2] - hz * 0.2], { role: 'ear', group: 'head', solid: false }));
      else r.parts.push(P('cone', [hx * 0.3, hy * 0.6, hx * 0.2], [s * hx * 0.32, hc[1] + hy * 0.75, hc[2] - hz * 0.15], { role: 'ear', group: 'head', solid: false }));
    }
  }
  const jawY = hc[1] - hy * 0.1 - sn[1] / 2 - 0.04;
  const jawLen = snZ + sn[2] / 2 - hc[2];
  r.parts.push(P('box', [sn[0] * 0.9, Math.max(0.06, sn[1] * 0.35), jawLen], [0, jawY, hc[2] + jawLen / 2], { role: 'jaw', group: 'jaw', solid: false }));
  const tip: Vec3 = [0, jawY + 0.04, snZ + sn[2] / 2];
  r.parts.push(P('sphere', [0.12, 0.12, 0.12], tip, { role: 'muzzle', group: 'jaw', solid: false }));
  r.joints.push(J('head', neckBase, [aimYaw(o.aim ?? 240)]));
  r.joints.push(J('jaw', [0, jawY, hc[2]], [snap(o.jawAmp ?? 28, 4)], 'head'));
  legs4(r, o.hipY, o.legX, L * 0.3, -L * 0.3, o.legT, o.gait[0], o.gait[1]);
  if (o.tail) {
    const t = o.tail;
    chain(r, 'tail', [0, by + G * 0.2, -L / 2 + G * 0.15], [0, t.lift ?? 0, -t.seg], t.n, t.t0, t.t1, () => [wave(t.amp, t.rate, [0, 1, 0])]);
  }
  return { r, by, hc, tip, neckBase };
}

/** Sprawled bug/crustacean legs (hip → raised knee → foot out to the side), tetrapod gait. */
function bugLegs(r: Rig, hips: number[], hipY: number, hipX: number, reach: number, kneeUp: number, thick: number, rate: number) {
  hips.forEach((hz, i) => {
    for (const side of [-1, 1]) {
      const g = `leg_${side < 0 ? 'l' : 'r'}${i}`;
      const hip: Vec3 = [side * hipX, hipY, hz];
      const foot: Vec3 = [side * (hipX + reach), 0.03, hz + (hz >= 0 ? 0.35 : -0.35) * reach];
      const knee: Vec3 = [side * (hipX + reach * 0.55), hipY + kneeUp, (hz + foot[2]) / 2];
      r.parts.push(limb('capsule', hip, knee, thick, { role: 'leg', group: g, solid: false }));
      r.parts.push(limb('cone', knee, foot, thick, { role: 'leg', group: g, solid: false }));
      const ph = (i + (side < 0 ? 0 : 1)) % 2 ? 0.5 : 0;
      r.joints.push(J(g, hip, [gait(16, rate, ph, [0, 1, 0]), bob(0.12, rate, ph + 0.25)]));
    }
  });
}

// ================================================================== chasers

/** Wolf: lean hunter at the chaser cap; ask for a pack ("five wolves") and they spread out on the target. */
export const wolf: CatalogEntry = {
  id: 'wolf',
  names: ['wolves', 'dire wolf', 'direwolf', 'dire wolves', 'timber wolf', 'grey wolf', 'gray wolf', 'wolf pack', 'pack of wolves', 'alpha wolf', 'wolfs', 'wolvs', 'warg', 'wargs', 'coyote', 'jackal', 'hyena'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const q = quad({ len: 1.5, girth: 0.6, hipY: 0.82, legT: 0.16, legX: 0.2, head: [0.44, 0.42, 0.5], snout: [0.26, 0.2, 0.42], gait: [34, 2.8], tail: { n: 2, seg: 0.42, t0: 0.16, t1: 0.1, amp: 22, rate: 2, lift: 0.14 } });
    q.r.parts.push(P('sphere', [0.6, 0.6, 0.5], [0, q.by + 0.15, 0.55], { role: 'fur', solid: false }));
    return q.r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 34, turn: 260, jump: 9 },
    HUNT(45),
    bite('jaw', 16, 1.8, 1.8),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Raptor: two-legged sprinter with a snapping jaw and sickle-claw kicks. Fastest chaser, fragile. */
export const raptor: CatalogEntry = {
  id: 'raptor',
  names: ['raptors', 'velociraptor', 'velociraptors', 'velocirapter', 'veloceraptor', 'utahraptor', 'deinonychus', 'raptor pack', 'pack of raptors', 'clever girl', 'dromaeosaur', 'raptor dinosaur'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.55, 1.5, 0.6], [0, 1.15, 0.05], { role: 'body', rot: [80, 0, 0] }));
    r.parts.push(limb('capsule', [0, 1.3, 0.6], [0, 1.65, 1.0], 0.28, { role: 'neck', group: 'head', solid: false }));
    r.parts.push(P('box', [0.32, 0.32, 0.62], [0, 1.72, 1.25], { role: 'head', group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.07, 0.07, 0.05], [s * 0.14, 1.8, 1.4], { role: 'eye', group: 'head', solid: false }));
    r.parts.push(P('box', [0.26, 0.08, 0.5], [0, 1.52, 1.3], { role: 'jaw', group: 'jaw', solid: false }));
    r.parts.push(P('sphere', [0.1, 0.1, 0.1], [0, 1.55, 1.55], { role: 'muzzle', group: 'jaw', solid: false }));
    for (const s of [-1, 1]) r.parts.push(limb('capsule', [s * 0.2, 1.2, 0.55], [s * 0.22, 0.95, 0.8], 0.08, { role: 'arm', solid: false }));
    walkLeg(r, 'leg_l', [-0.2, 1.05, -0.05], 0.2, [gait(40, 2.6, 0)], { knee: -0.25, foot: 0.4 });
    walkLeg(r, 'leg_r', [0.2, 1.05, -0.05], 0.2, [gait(40, 2.6, 0.5), snap(55, 3)], { knee: -0.25, foot: 0.4 });
    r.parts.push(P('cone', [0.1, 0.3, 0.1], [0.2, 0.12, 0.28], { role: 'claw', rot: [70, 0, 0], group: 'leg_r', solid: false }));
    r.parts.push(P('sphere', [0.08, 0.08, 0.08], [0.2, 0.15, 0.42], { role: 'muzzle', group: 'leg_r', solid: false }));
    chain(r, 'tail', [0, 1.2, -0.6], [0, 0.05, -0.6], 3, 0.3, 0.1, () => [wave(10, 1.6, [0, 1, 0])]);
    r.joints.push(J('head', [0, 1.3, 0.6], [aimYaw(300), bob(0.05, 2.6)]), J('jaw', [0, 1.55, 1.05], [snap(30, 5)], 'head'));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 36, turn: 300, jump: 11 },
    HUNT(50),
    bite('jaw', 12, 2, 1.8),
    bite('leg_r', 14, 1.4, 1.6),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Big cat: pouncing tiger/lion/panther — sprints at the chaser cap, leaps at raised targets, claws + bite. Rideable. */
export const bigcat: CatalogEntry = {
  id: 'tiger',
  names: ['tigers', 'lion', 'lions', 'lioness', 'panther', 'black panther', 'cheetah', 'leopard', 'jaguar', 'sabertooth', 'saber tooth', 'sabre tooth', 'sabretooth', 'sabertooth tiger', 'saber-toothed tiger', 'smilodon', 'puma', 'cougar', 'mountain lion', 'big cat', 'white tiger', 'war tiger', 'battle cat', 'tigre', 'tyger', 'cheeta', 'lepard', 'leperd', 'snow leopard', 'lynx'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const q = quad({ len: 2.0, girth: 0.8, hipY: 0.95, legT: 0.21, legX: 0.26, head: [0.62, 0.55, 0.58], snout: [0.36, 0.26, 0.28], ears: 'round', gait: [36, 2.6], tail: { n: 3, seg: 0.45, t0: 0.14, t1: 0.1, amp: 24, rate: 1.1, lift: 0.12 } });
    const r = q.r;
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.08, 0.3, 0.08], [s * 0.12, q.tip[1] - 0.12, q.tip[2] - 0.1], { role: 'fang', rot: [180, 0, 0], group: 'head', solid: false }));
    // front paw swipe on top of the trot
    const fl = r.joints.find((j) => j.group === 'leg_fl')!;
    fl.motion.push(snap(-70, 3));
    r.parts.push(P('sphere', [0.1, 0.1, 0.1], [-0.26, 0.15, 0.95], { role: 'muzzle', group: 'leg_fl', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 34, turn: 240, jump: 13 },
    HUNT(45),
    bite('leg_fl', 20, 1.3, 2.0),
    bite('jaw', 16, 1, 1.8),
    { type: 'rideable', seat: [0, 2.05, -0.2] },
  ],
  functions: [...WALKER, ...RIDE],
  verb: 'strike',
};

/** Shark that swims through the air a metre off the ground: waving tail, huge biting jaw. Chaser. */
export const shark: CatalogEntry = {
  id: 'shark',
  names: ['sharks', 'great white', 'megalodon', 'whale', 'dolphin', 'orca', 'fish', 'barracuda', 'swordfish', 'great white shark', 'hammerhead', 'hammerhead shark', 'tiger shark', 'bull shark', 'sky shark', 'land shark', 'air shark', 'flying shark', 'street shark', 'sharky', 'shork', 'sharc', 'jaws'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  replaces: ['shark'],
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [1.2, 3.2, 1.3], [0, 1.2, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('cone', [1.1, 1.0, 1.2], [0, 1.25, 1.95], { role: 'snout', rot: [90, 0, 0], solid: false }));
    r.parts.push(P('wedge', [0.12, 1.1, 0.9], [0, 2.1, 0.1], { role: 'fin', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(P('wedge', [1.0, 0.08, 0.6], [s * 0.85, 0.95, 0.5], { role: 'fin', rot: [0, 0, s * -25], solid: false }));
      r.parts.push(P('sphere', [0.14, 0.14, 0.1], [s * 0.42, 1.45, 1.55], { role: 'eye', solid: false }));
    }
    // lower jaw with a row of teeth
    r.parts.push(P('box', [0.85, 0.18, 0.9], [0, 0.72, 1.45], { role: 'jaw', group: 'jaw', solid: false }));
    for (const x of [-0.28, 0, 0.28]) r.parts.push(P('cone', [0.1, 0.22, 0.1], [x, 0.9, 1.75], { role: 'tooth', group: 'jaw', solid: false }));
    r.parts.push(P('sphere', [0.14, 0.14, 0.14], [0, 0.85, 1.95], { role: 'muzzle', group: 'jaw', solid: false }));
    const t = chain(r, 'tail', [0, 1.2, -1.5], [0, 0, -0.8], 2, 0.6, 0.35, (i) => [wave(18 + i * 8, 1.4, [0, 1, 0])]);
    r.parts.push(P('box', [0.1, 1.4, 0.55], [0, 1.25, t.tip[2] - 0.15], { role: 'tail', rot: [-15, 0, 0], group: t.last, solid: false }));
    r.joints.push(J('jaw', [0, 0.8, 1.0], [snap(26, 4)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 32, turn: 190, altitude: 1.4 },
    HUNT(45),
    bite('jaw', 30, 1.2, 2.2),
  ],
  functions: ['moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'pushes_objects'],
  verb: 'strike',
};

// ------------------------------------------------------------------ swarms (one construct, several bodies circling)

function swarmMembers(r: Rig, n: number, radius: number, y: number, rate: number, build: (i: number, g: string, yaw: number, pos: Vec3) => void) {
  for (let i = 0; i < n; i++) {
    const a = (i / n) * 360;
    const rad = radius * (0.55 + 0.45 * ((i * 0.618) % 1));
    const yy = y + (((i * 0.37) % 1) - 0.5) * radius * 0.7;
    const pos: Vec3 = [Math.sin((a * Math.PI) / 180) * rad, yy, Math.cos((a * Math.PI) / 180) * rad];
    const g = `m${i}`;
    // negative rate: members travel forward (+z of their own frame) around the circle
    r.joints.push(J(g, [0, yy, 0], [spin(-(rate * (0.8 + 0.4 * ((i * 0.53) % 1))), [0, 1, 0]), swing(10, 0.7 + 0.1 * i, [1, 0, 0], i * 0.2)]));
    build(i, g, a - 90, pos);
  }
}

/** Bee swarm: six buzzing stingers circling as one cloud. Chases at the cap, stings whatever it envelops. */
export const bees: CatalogEntry = {
  id: 'bees',
  names: ['bee', 'bee swarm', 'swarm of bees', 'killer bees', 'killer bee', 'hornets', 'hornet', 'hornet swarm', 'wasps', 'wasp', 'wasp swarm', 'murder hornets', 'murder hornet', 'bumblebees', 'bumblebee', 'bumble bees', 'yellowjackets', 'yellow jackets', 'beez', 'beehive', 'bee hive', 'hive'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'front',
  lifetime: 120,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [0.5, 0.5, 0.5], [0, 1.6, 0], { role: 'body' })); // the queen: the swarm's solid heart
    swarmMembers(r, 6, 2.0, 1.6, 0.9, (i, g, yaw, pos) => {
      const bee = [
        P('capsule', [0.26, 0.5, 0.26], [0, 0, 0], { role: 'body', rot: [90, 0, 0], solid: false }),
        P('torus', [0.28, 0.06, 0.28], [0, 0, -0.05], { role: 'stripe', rot: [90, 0, 0], solid: false }),
        P('cone', [0.08, 0.2, 0.08], [0, 0, -0.33], { role: 'stinger', rot: [-90, 0, 0], solid: false }),
      ];
      r.parts.push(...placed(bee, yaw, pos, g));
      const wg = `w${i}`;
      r.parts.push(...placed([P('box', [0.62, 0.02, 0.18], [0, 0.16, 0.05], { role: 'wing', solid: false })], yaw, pos, wg));
      r.joints.push(J(wg, [pos[0], pos[1] + 0.16, pos[2]], [{ kind: 'piston', axis: [0, 1, 0], amp: 0.06, rate: 6, phase: i * 0.3 }], g));
    });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 30, turn: 360, altitude: 4 },
    HUNT(45),
    bodyHit(4, 5, 4.2),
  ],
  functions: [...FLYER, 'area_attack'],
  verb: 'strike',
};

/** Piranha school: five snapping fish circling a metre above the ground. Chases at the cap, nibbles hard. */
export const piranhas: CatalogEntry = {
  id: 'piranhas',
  names: ['piranha', 'piranha swarm', 'school of piranhas', 'school of piranha', 'flying piranhas', 'flying piranha', 'killer fish', 'piranah', 'pirahna', 'pirana', 'piranna', 'piranhas swarm', 'feeding frenzy'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 120,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [0.6, 0.6, 0.6], [0, 1.0, 0], { role: 'body' }));
    swarmMembers(r, 5, 1.4, 1.0, 1.0, (i, g, yaw, pos) => {
      const fish = [
        P('sphere', [0.3, 0.46, 0.6], [0, 0, 0], { role: 'body', solid: false }),
        P('box', [0.22, 0.1, 0.22], [0, -0.14, 0.24], { role: 'jaw', solid: false }),
        ...[-0.08, 0.08].map((x) => P('cone', [0.05, 0.1, 0.05], [x, -0.07, 0.33], { role: 'tooth', solid: false })),
      ];
      r.parts.push(...placed(fish, yaw, pos, g));
      const tg = `t${i}`;
      r.parts.push(...placed([P('box', [0.04, 0.34, 0.26], [0, 0, -0.4], { role: 'tail', solid: false })], yaw, pos, tg));
      const back = placed([P('box', [0.1, 0.1, 0.1], [0, 0, -0.28])], yaw, pos)[0].pos;
      r.joints.push(J(tg, back, [swing(35, 3 + i * 0.3, [0, 1, 0])], g));
    });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 32, turn: 320, altitude: 1.0 },
    HUNT(40),
    bodyHit(5, 5, 1.0),
  ],
  functions: ['moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'area_attack'],
  verb: 'strike',
};

/** Bat swarm: four leathery bats wheeling in a cloud, diving onto the target. Flyer; a dome or wall stops them cold. */
export const bats: CatalogEntry = {
  id: 'bats',
  names: ['bat swarm', 'swarm of bats', 'vampire bats', 'vampire bat', 'colony of bats', 'cloud of bats', 'fruit bats', 'bat colony', 'flying bats', 'batswarm', 'giant bats', 'battz', 'bats swarm'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'front',
  lifetime: 120,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [0.5, 0.5, 0.5], [0, 2.0, 0], { role: 'body' }));
    swarmMembers(r, 4, 2.4, 2.0, 0.7, (i, g, yaw, pos) => {
      const bat = [
        P('sphere', [0.34, 0.3, 0.5], [0, 0, 0], { role: 'body', solid: false }),
        P('sphere', [0.24, 0.22, 0.22], [0, 0.06, 0.3], { role: 'head', solid: false }),
        ...[-1, 1].map((s) => P('cone', [0.08, 0.18, 0.06], [s * 0.08, 0.22, 0.28], { role: 'ear', solid: false })),
      ];
      r.parts.push(...placed(bat, yaw, pos, g));
      const fwd = heading(yaw);
      for (const s of [-1, 1]) {
        const wg = `w${i}${s < 0 ? 'l' : 'r'}`;
        r.parts.push(...placed([P('wedge', [0.7, 0.04, 0.42], [s * 0.5, 0.02, 0], { role: 'wing', rot: [0, s < 0 ? 0 : 0, 0], solid: false })], yaw, pos, wg));
        const root = placed([P('box', [0.1, 0.1, 0.1], [s * 0.15, 0.02, 0])], yaw, pos)[0].pos;
        const axis: Vec3 = s < 0 ? [-fwd[0], 0, -fwd[2]] : fwd;
        r.joints.push(J(wg, root, [swing(45, 3.2 + i * 0.2, axis, i * 0.13)], g));
      }
    });
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 30, turn: 320, altitude: 4 },
    HUNT(45),
    bodyHit(7, 3, 4.2),
  ],
  functions: [...FLYER, 'area_attack'],
  verb: 'strike',
};

/** Eagle: soars high, dives with its talons out, climbs away. Flyer; raising a shield or a roof breaks the dive. */
export const eagle: CatalogEntry = {
  id: 'eagle',
  names: ['eagles', 'bald eagle', 'golden eagle', 'war eagle', 'hawk', 'hawks', 'war hawk', 'warhawk', 'falcon', 'falcons', 'peregrine', 'peregrine falcon', 'condor', 'vulture', 'harpy eagle', 'bird of prey', 'giant eagle', 'roc', 'eagel', 'egale'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.6, 1.5, 0.7], [0, 1.5, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('sphere', [0.46, 0.46, 0.5], [0, 1.72, 0.8], { role: 'head', group: 'head', solid: false }));
    r.parts.push(P('cone', [0.18, 0.34, 0.2], [0, 1.64, 1.14], { role: 'beak', rot: [110, 0, 0], group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.08, 0.08, 0.05], [s * 0.16, 1.8, 0.98], { role: 'eye', group: 'head', solid: false }));
    r.parts.push(P('box', [0.9, 0.06, 0.7], [0, 1.52, -1.0], { role: 'tail', group: 'tailfan', solid: false }));
    wingPair(r, 'wing', [0.28, 1.62, 0.1], 2.6, 1.0, 34, 1.6, { tip: false });
    // talons: legs that swing forward to strike
    for (const s of [-1, 1]) {
      r.parts.push(limb('capsule', [s * 0.14, 1.2, 0.05], [s * 0.16, 0.72, 0.3], 0.1, { role: 'leg', group: 'talons', solid: false }));
      r.parts.push(P('cone', [0.1, 0.24, 0.1], [s * 0.16, 0.6, 0.42], { role: 'claw', rot: [120, 0, 0], group: 'talons', solid: false }));
    }
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [0, 0.62, 0.4], { role: 'muzzle', group: 'talons', solid: false }));
    r.joints.push(
      J('head', [0, 1.6, 0.6], [aimYaw(260)]),
      J('tailfan', [0, 1.52, -0.7], [swing(12, 0.8, [0, 0, 1])]),
      J('talons', [0, 1.2, 0.05], [snap(-60, 3)]),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 34, turn: 220, altitude: 7 },
    HUNT(50),
    bite('talons', 18, 1.3, 4.4),
  ],
  functions: [...FLYER],
  verb: 'strike',
};

// ================================================================== chargers (fast, but they commit to a line)

/** Rhino: armoured battering charge with a horn that gores what it hits. Turns slowly — side-step it. Rideable. */
export const rhino: CatalogEntry = {
  id: 'rhino',
  names: ['rhinoceros', 'rhinocerous', 'rhinoserous', 'rhinocerus', 'rino', 'rhinos', 'war rhino', 'battle rhino', 'armored rhino', 'armoured rhino', 'charging rhino', 'triceratops', 'tricerotops', 'rhino charge'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const q = quad({ len: 2.6, girth: 1.5, hipY: 0.8, legT: 0.42, legX: 0.5, head: [0.85, 0.75, 1.0], neck: 0.15, snout: [0.62, 0.5, 0.5], gait: [22, 1.8], tail: { n: 1, seg: 0.4, t0: 0.1, t1: 0.1, amp: 20, rate: 2, lift: -0.2 }, aim: 90 });
    const r = q.r;
    r.parts.push(P('cone', [0.34, 0.9, 0.34], [0, q.hc[1] + 0.3, q.tip[2] - 0.15], { role: 'horn', rot: [35, 0, 0], group: 'head', solid: false }));
    r.parts.push(P('cone', [0.22, 0.45, 0.22], [0, q.hc[1] + 0.4, q.tip[2] - 0.6], { role: 'horn', rot: [20, 0, 0], group: 'head', solid: false }));
    r.parts.push(P('sphere', [0.14, 0.14, 0.14], [0, q.hc[1] + 0.62, q.tip[2] + 0.1], { role: 'muzzle', group: 'head', solid: false }));
    r.parts.push(P('box', [1.3, 0.25, 1.6], [0, q.by + 0.78, -0.2], { role: 'armor', solid: false }));
    r.parts.push(P('box', [0.7, 0.2, 0.8], [0, q.by + 0.92, -0.3], { role: 'seat', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 34, turn: 55 },
    HUNT(55),
    bite('head', 34, 1, 1.6),
    { type: 'rideable', seat: [0, 2.65, -0.3] },
  ],
  functions: [...WALKER, ...RIDE],
  verb: 'strike',
};

/** Bull: snorting horned charger. Very fast in a straight line, poor at turning — the matador's side-step beats it. */
export const bull: CatalogEntry = {
  id: 'bull',
  names: ['bulls', 'raging bull', 'angry bull', 'charging bull', 'rodeo bull', 'bison', 'buffalo', 'water buffalo', 'ox', 'oxen', 'yak', 'longhorn', 'texas longhorn', 'bullock', 'mad bull', 'bul'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 700,
  build() {
    const q = quad({ len: 2.2, girth: 1.2, hipY: 0.95, legT: 0.3, legX: 0.4, head: [0.72, 0.72, 0.7], neck: 0.25, snout: [0.5, 0.4, 0.3], gait: [26, 2], tail: { n: 2, seg: 0.45, t0: 0.08, t1: 0.08, amp: 25, rate: 1.4, lift: -0.3 }, aim: 90 });
    const r = q.r;
    r.parts.push(P('sphere', [1.3, 1.1, 1.0], [0, q.by + 0.35, 0.65], { role: 'hump', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(limb('cone', [s * 0.3, q.hc[1] + 0.25, q.hc[2]], [s * 0.85, q.hc[1] + 0.55, q.hc[2] + 0.35], 0.16, { role: 'horn', group: 'head', solid: false }));
      r.parts.push(P('sphere', [0.12, 0.12, 0.12], [s * 0.85, q.hc[1] + 0.55, q.hc[2] + 0.35], { role: 'muzzle', group: 'head', solid: false }));
    }
    r.parts.push(P('torus', [0.2, 0.05, 0.2], [0, q.tip[1] + 0.1, q.tip[2] - 0.05], { role: 'ring', rot: [90, 0, 0], group: 'head', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 32, turn: 60, jump: 5 },
    HUNT(55),
    bite('head', 28, 1.2, 1.6),
    { type: 'rideable', seat: [0, 2.5, -0.3] },
  ],
  functions: [...WALKER, ...RIDE],
  verb: 'strike',
};

/** War elephant: trampling charge, trunk that whips targets away, flapping ears, a howdah to ride in. */
export const elephant: CatalogEntry = {
  id: 'elephant',
  names: ['elephants', 'war elephant', 'mammoth', 'woolly mammoth', 'wooly mammoth', 'oliphant', 'oliphaunt', 'mumakil', 'mumak', 'jumbo', 'dumbo', 'elefant', 'elephent', 'elephunt', 'pachyderm', 'battle elephant'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const q = quad({ len: 3.2, girth: 2.2, hipY: 1.5, legT: 0.62, legX: 0.72, head: [1.4, 1.4, 1.2], neck: 0.55, neckZ: 0.3, snout: [0.5, 0.5, 0.2], ears: 'none', gait: [20, 1.3], tail: { n: 1, seg: 0.7, t0: 0.1, t1: 0.1, amp: 22, rate: 1.5, lift: -0.4 }, aim: 80 });
    const r = q.r;
    const hc = q.hc;
    for (const s of [-1, 1]) {
      const eg = s < 0 ? 'ear_l' : 'ear_r';
      r.parts.push(P('box', [1.2, 1.4, 0.08], [s * 1.2, hc[1] + 0.1, hc[2] - 0.3], { role: 'ear', rot: [0, s * 20, 0], group: eg, solid: false }));
      r.joints.push(J(eg, [s * 0.65, hc[1] + 0.1, hc[2] - 0.3], [swing(22, 0.7, [0, 1, 0], s < 0 ? 0 : 0.5)], 'head'));
      r.parts.push(limb('cone', [s * 0.35, hc[1] - 0.45, hc[2] + 0.5], [s * 0.5, hc[1] - 0.2, hc[2] + 1.6], 0.2, { role: 'tusk', group: 'head', solid: false }));
    }
    // trunk: a waving chain that whips down on the target
    const t = chain(r, 'trunk', [0, hc[1] - 0.2, hc[2] + 0.7], (i) => [0, -0.55, 0.12 + i * 0.05], 4, 0.42, 0.2, (i) => (i === 0 ? [wave(10, 0.8, [1, 0, 0]), snap(-55, 2.5)] : [wave(14, 0.8, [1, 0, 0])]), { parent: 'head', role: 'trunk' });
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], t.tip, { role: 'muzzle', group: t.last, solid: false }));
    // howdah
    r.parts.push(P('box', [1.6, 0.35, 1.6], [0, q.by + 1.2, -0.3], { role: 'seat', solid: false }));
    for (const x of [-0.7, 0.7]) for (const z of [-1.0, 0.4]) r.parts.push(P('box', [0.08, 0.9, 0.08], [x, q.by + 1.8, z], { role: 'post', solid: false }));
    r.parts.push(P('box', [1.7, 0.1, 1.7], [0, q.by + 2.25, -0.3], { role: 'canopy', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 20, turn: 70 },
    HUNT(55),
    bite('trunk3', 30, 0.9, 2.6),
    bodyHit(20, 0.7, 1.2),
    { type: 'rideable', seat: [0, 4.2, -0.3] },
  ],
  functions: [...WALKER, ...RIDE, 'push_target'],
  verb: 'strike',
};

// ================================================================== heavies

/** Grizzly bear: lumbering but quick over short distances, paw swipes that knock you flat, a savage bite. Rideable. */
export const bear: CatalogEntry = {
  id: 'bear',
  names: ['bears', 'grizzly', 'grizzly bear', 'polar bear', 'brown bear', 'black bear', 'kodiak', 'kodiak bear', 'war bear', 'battle bear', 'armored bear', 'armoured bear', 'bear cavalry', 'dire bear', 'cave bear', 'grizly', 'grizzley', 'panda', 'giant panda'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 700,
  build() {
    const q = quad({ len: 2.0, girth: 1.35, hipY: 0.9, legT: 0.38, legX: 0.44, head: [0.8, 0.72, 0.72], neck: 0.3, snout: [0.44, 0.34, 0.36], ears: 'round', gait: [24, 1.6], tail: { n: 1, seg: 0.2, t0: 0.2, t1: 0.2, amp: 10, rate: 1, lift: 0 }, aim: 160 });
    const r = q.r;
    r.parts.push(P('sphere', [1.5, 1.2, 1.2], [0, q.by + 0.4, 0.5], { role: 'hump', solid: false }));
    const fl = r.joints.find((j) => j.group === 'leg_fl')!;
    const fr = r.joints.find((j) => j.group === 'leg_fr')!;
    fl.motion.push(snap(-80, 2.2));
    fr.motion.push(snap(-80, 2.2));
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'leg_fl' : 'leg_fr';
      for (const dx of [-0.12, 0, 0.12]) r.parts.push(P('cone', [0.07, 0.22, 0.07], [s * 0.44 + dx, 0.1, 0.95], { role: 'claw', rot: [90, 0, 0], group: g, solid: false }));
      r.parts.push(P('sphere', [0.12, 0.12, 0.12], [s * 0.44, 0.2, 0.95], { role: 'muzzle', group: g, solid: false }));
    }
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 22, turn: 160, jump: 8 },
    HUNT(45),
    bite('leg_fl', 26, 0.9, 2.2),
    bite('leg_fr', 26, 0.9, 2.2),
    bite('jaw', 18, 0.8, 1.8),
    { type: 'rideable', seat: [0, 2.6, -0.3] },
  ],
  functions: [...WALKER, ...RIDE],
  verb: 'strike',
};

/** Gorilla: knuckle-walks fast, leaps, slams you away with its fists and hurls hard-light boulders. Ride on its back. */
export const gorilla: CatalogEntry = {
  id: 'gorilla',
  names: ['gorillas', 'silverback', 'king kong', 'kong', 'giant ape', 'big ape', 'ape', 'great ape', 'war ape', 'gorrila', 'gorila', 'gorilla warfare', 'harambe', 'mighty joe young', 'orangutan', 'chimp', 'chimpanzee', 'baboon'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 600,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [1.9, 1.7, 1.5], [0, 2.3, 0.2], { role: 'body', rot: [-25, 0, 0] }));
    r.parts.push(P('sphere', [1.4, 1.1, 1.1], [0, 1.6, -0.5], { role: 'belly', solid: false }));
    r.parts.push(P('sphere', [0.8, 0.75, 0.75], [0, 3.1, 0.9], { role: 'head', group: 'head', solid: false }));
    r.parts.push(P('box', [0.8, 0.16, 0.3], [0, 3.35, 1.15], { role: 'brow', group: 'head', solid: false }));
    r.parts.push(P('sphere', [0.55, 0.4, 0.4], [0, 2.9, 1.2], { role: 'snout', group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.1, 0.1, 0.06], [s * 0.18, 3.2, 1.25], { role: 'eye', group: 'head', solid: false }));
    // long arms: knuckle-walk with the gait, slam (right) / throw (left) when they fire
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'arm_l' : 'arm_r';
      r.parts.push(limb('capsule', [s * 1.0, 2.8, 0.5], [s * 1.2, 1.4, 0.9], 0.5, { role: 'arm', group: g, solid: false }));
      r.parts.push(limb('capsule', [s * 1.2, 1.4, 0.9], [s * 1.2, 0.35, 1.1], 0.42, { role: 'arm', group: g, solid: false }));
      r.parts.push(P('sphere', [0.6, 0.5, 0.6], [s * 1.2, 0.28, 1.15], { role: 'fist', group: g, solid: false }));
      r.parts.push(P('sphere', [0.14, 0.14, 0.14], [s * 1.2, 0.4, 1.35], { role: 'muzzle', group: g, solid: false }));
      r.joints.push(J(g, [s * 1.0, 2.8, 0.5], [gait(26, 1.5, s < 0 ? 0 : 0.5), snap(-110, 1.8)]));
    }
    walkLeg(r, 'leg_l', [-0.55, 1.3, -0.4], 0.45, [gait(24, 1.5, 0.5)], { knee: 0.2 });
    walkLeg(r, 'leg_r', [0.55, 1.3, -0.4], 0.45, [gait(24, 1.5, 0)], { knee: 0.2 });
    r.joints.push(J('head', [0, 2.8, 0.7], [aimYaw(200)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 22, turn: 180, jump: 12 },
    HUNT(45),
    bite('arm_r', 34, 1, 2.6),
    { type: 'weapon', kind: 'bomb', group: 'arm_l', rate: 0.4, speed: 26, damage: 26, range: 32, size: 0.7 },
    { type: 'rideable', seat: [0, 3.9, -0.2] },
  ],
  functions: [...WALKER, ...RIDE, 'shoots', 'projectile_attack', 'throw_target', 'push_target'],
  verb: 'strike',
};

/** T-rex: two-legged giant with a bone-crushing bite and a stomp that knocks back anything near it. Rideable. */
export const trex: CatalogEntry = {
  id: 't-rex',
  names: ['trex', 't rex', 'tyrannosaurus', 'tyrannosaurus rex', 'tyranosaurus', 'tyranosaurus rex', 'tyrannasaurus', 'dinosaur', 'dinosaurs', 'dino', 'rex', 'tirex', 't-rexes', 'dinosour', 'dinasaur', 'dinosuar', 'allosaurus', 'giganotosaurus', 'spinosaurus', 'carnotaurus', 'trex dinosaur'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 700,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [1.5, 3.4, 1.8], [0, 2.6, 0], { role: 'body', rot: [75, 0, 0] }));
    r.parts.push(limb('capsule', [0, 3.0, 1.2], [0, 3.35, 2.1], 1.0, { role: 'neck', group: 'neck', solid: false }));
    r.parts.push(P('box', [1.05, 0.9, 1.7], [0, 3.55, 2.8], { role: 'head', group: 'neck', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.14, 0.14, 0.08], [s * 0.4, 3.85, 2.5], { role: 'eye', group: 'neck', solid: false }));
    for (const x of [-0.3, 0, 0.3]) r.parts.push(P('cone', [0.1, 0.24, 0.1], [x, 3.0, 3.4], { role: 'tooth', rot: [180, 0, 0], group: 'neck', solid: false }));
    r.parts.push(P('box', [0.9, 0.3, 1.5], [0, 2.95, 2.85], { role: 'jaw', group: 'jaw', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 2.95, 3.6], { role: 'muzzle', group: 'jaw', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(limb('capsule', [s * 0.55, 2.7, 1.1], [s * 0.6, 2.3, 1.45], 0.14, { role: 'arm', group: 'arms', solid: false }));
    }
    walkLeg(r, 'leg_l', [-0.65, 2.3, -0.3], 0.62, [gait(30, 1.2, 0)], { knee: -0.6, foot: 1.0 });
    walkLeg(r, 'leg_r', [0.65, 2.3, -0.3], 0.62, [gait(30, 1.2, 0.5)], { knee: -0.6, foot: 1.0 });
    chain(r, 'tail', [0, 2.6, -1.5], (i) => [0, 0.05 - i * 0.12, -1.1], 3, 0.9, 0.3, () => [wave(14, 0.7, [0, 1, 0])]);
    r.joints.push(
      J('neck', [0, 3.0, 1.2], [aimYaw(150), swing(4, 0.5, [1, 0, 0])]),
      J('jaw', [0, 3.05, 2.2], [snap(32, 3)], 'neck'),
      J('arms', [0, 2.7, 1.1], [swing(18, 1.5, [1, 0, 0])]),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 22, turn: 110, jump: 7 },
    HUNT(55),
    bite('jaw', 40, 0.9, 3.0),
    bodyHit(16, 0.6, 1.8),
    { type: 'rideable', seat: [0, 4.0, -0.4] },
  ],
  functions: [...WALKER, ...RIDE, 'push_target', 'area_attack'],
  verb: 'strike',
};

/** Crocodile: low armoured lunger with a long snapping jaw and a swishing tail. */
export const croc: CatalogEntry = {
  id: 'crocodile',
  names: ['croc', 'crocs', 'crocodiles', 'alligator', 'alligators', 'gator', 'gators', 'caiman', 'crocodyle', 'crockodile', 'crocodial', 'alligater', 'aligator', 'crocodilian', 'sarcosuchus', 'komodo dragon', 'monitor lizard'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const q = quad({ len: 2.6, girth: 0.75, hipY: 0.45, legT: 0.2, legX: 0.52, head: [0.7, 0.36, 0.8], neck: 0.05, neckZ: 0.4, snout: [0.46, 0.2, 1.1], ears: 'none', gait: [30, 2.2], aim: 200, jawAmp: 35 });
    const r = q.r;
    for (let i = 0; i < 4; i++) r.parts.push(P('cone', [0.2, 0.2, 0.2], [0, q.by + 0.4, 0.9 - i * 0.6], { role: 'spike', solid: false }));
    chain(r, 'tail', [0, q.by, -1.3], [0, 0, -0.75], 4, 0.6, 0.15, () => [wave(22, 1.1, [0, 1, 0])]);
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 24, turn: 150 },
    HUNT(40),
    bite('jaw', 32, 0.9, 1.9),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Boxing kangaroo: hops at speed, jabs with both gloves. */
export const kangaroo: CatalogEntry = {
  id: 'kangaroo',
  names: ['boxing kangaroo', 'kangaroos', 'roo', 'kangeroo', 'kangaroo boxer', 'wallaby', 'boxing roo', 'kanga'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.9, 1.6, 0.8], [0, 1.6, 0], { role: 'body', rot: [-15, 0, 0] }));
    r.parts.push(P('box', [0.42, 0.45, 0.62], [0, 2.6, 0.35], { role: 'head', group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.14, 0.4, 0.1], [s * 0.14, 2.95, 0.2], { role: 'ear', group: 'head', solid: false }));
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'arm_l' : 'arm_r';
      r.parts.push(limb('capsule', [s * 0.42, 2.0, 0.25], [s * 0.35, 1.8, 0.65], 0.14, { role: 'arm', group: g, solid: false }));
      r.parts.push(P('sphere', [0.38, 0.38, 0.4], [s * 0.33, 1.8, 0.8], { role: 'glove', group: g, solid: false }));
      r.parts.push(P('sphere', [0.1, 0.1, 0.1], [s * 0.33, 1.8, 1.0], { role: 'muzzle', group: g, solid: false }));
      r.joints.push(J(g, [s * 0.42, 2.0, 0.25], [recoil(-0.7, 4), swing(10, 1.2, [1, 0, 0], s < 0 ? 0 : 0.5)]));
    }
    // both legs in phase: it hops
    walkLeg(r, 'leg_l', [-0.3, 1.05, -0.2], 0.3, [gait(28, 2.2, 0)], { knee: -0.4, foot: 0.9 });
    walkLeg(r, 'leg_r', [0.3, 1.05, -0.2], 0.3, [gait(28, 2.2, 0)], { knee: -0.4, foot: 0.9 });
    chain(r, 'tail', [0, 0.9, -0.5], [0, -0.35, -0.7], 2, 0.4, 0.2, () => [wave(10, 1.1, [1, 0, 0])]);
    r.joints.push(J('head', [0, 2.3, 0.2], [aimYaw(260)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 28, turn: 220, jump: 14 },
    HUNT(45),
    bite('arm_l', 16, 1.5, 1.6),
    bite('arm_r', 16, 1.5, 1.6),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

// ================================================================== clawed things

/** Scorpion: six legs, pinching claws and a curled tail that stabs over its back. */
export const scorpion: CatalogEntry = {
  id: 'scorpion',
  names: ['scorpions', 'giant scorpion', 'emperor scorpion', 'deathstalker', 'scorpian', 'scorpio', 'scorpoin', 'scorpion king'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [1.5, 0.7, 2.2], [0, 0.85, 0], { role: 'body' }));
    r.parts.push(P('box', [0.9, 0.4, 0.5], [0, 0.85, 1.1], { role: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.1, 0.1, 0.1], [s * 0.2, 1.1, 1.3], { role: 'eye', solid: false }));
    bugLegs(r, [0.5, -0.1, -0.7], 0.8, 0.6, 1.2, 0.5, 0.14, 2.4);
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'claw_l' : 'claw_r';
      r.parts.push(limb('capsule', [s * 0.45, 0.85, 1.2], [s * 0.85, 0.9, 1.9], 0.2, { role: 'arm', group: g, solid: false }));
      r.parts.push(P('box', [0.42, 0.26, 0.6], [s * 0.85, 0.9, 2.2], { role: 'claw', group: g, solid: false }));
      r.parts.push(P('cone', [0.14, 0.4, 0.1], [s * 0.72, 0.9, 2.62], { role: 'pincer', rot: [90, 0, 0], group: g, solid: false }));
      r.parts.push(P('sphere', [0.1, 0.1, 0.1], [s * 0.8, 0.9, 2.7], { role: 'muzzle', group: g, solid: false }));
      r.joints.push(J(g, [s * 0.45, 0.85, 1.2], [swing(8, 0.9, [0, 1, 0], s < 0 ? 0 : 0.5), snap(s * 25, 4, [0, 1, 0])]));
    }
    // tail curls up and over; the base segment snaps forward to sting
    const steps: Vec3[] = [[0, 0.55, -0.45], [0, 0.7, -0.05], [0, 0.45, 0.4], [0, -0.05, 0.5]];
    const t = chain(r, 'tail', [0, 1.0, -1.0], (i) => steps[i], 4, 0.36, 0.22, (i) => (i === 0 ? [swing(6, 0.8, [1, 0, 0]), snap(40, 2.5)] : [swing(5, 0.8, [1, 0, 0])]));
    r.parts.push(P('cone', [0.22, 0.5, 0.22], [0, t.tip[1] - 0.2, t.tip[2] + 0.12], { role: 'stinger', rot: [150, 0, 0], group: t.last, solid: false }));
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [0, t.tip[1] - 0.45, t.tip[2] + 0.2], { role: 'muzzle', group: t.last, solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 18, turn: 220, jump: 6 },
    HUNT(45),
    bite('tail3', 26, 0.8, 3.0),
    bite('claw_r', 14, 1.4, 1.8),
    bite('claw_l', 14, 1.2, 1.8),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Giant crab: armoured shell on six legs, two crushing claws, eyes on stalks. Slow, tough, pinches hard. */
export const crab: CatalogEntry = {
  id: 'crab',
  names: ['crabs', 'giant crab', 'king crab', 'crabby', 'hermit crab', 'lobster', 'giant lobster', 'crayfish', 'crawdad', 'crab tank', 'krab', 'krabby', 'crabbo', 'mecha crab', 'coconut crab'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [2.6, 1.0, 1.9], [0, 1.1, 0], { role: 'shell' }));
    r.parts.push(P('dome', [2.4, 0.5, 1.7], [0, 1.45, 0], { role: 'carapace', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(P('cylinder', [0.08, 0.5, 0.08], [s * 0.3, 1.75, 0.8], { role: 'stalk', group: 'eyes', solid: false }));
      r.parts.push(P('sphere', [0.18, 0.18, 0.18], [s * 0.3, 2.02, 0.8], { role: 'eye', group: 'eyes', solid: false }));
    }
    bugLegs(r, [0.35, -0.2, -0.7], 0.95, 1.05, 1.0, 0.45, 0.16, 2.0);
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'claw_l' : 'claw_r';
      r.parts.push(limb('capsule', [s * 1.0, 1.0, 0.6], [s * 1.35, 1.2, 1.4], 0.3, { role: 'arm', group: g, solid: false }));
      r.parts.push(P('sphere', [0.8, 0.55, 0.95], [s * 1.35, 1.25, 1.85], { role: 'claw', group: g, solid: false }));
      r.parts.push(P('wedge', [0.4, 0.2, 0.6], [s * 1.35, 1.0, 2.35], { role: 'pincer', group: g, solid: false }));
      r.parts.push(P('sphere', [0.12, 0.12, 0.12], [s * 1.35, 1.15, 2.6], { role: 'muzzle', group: g, solid: false }));
      r.joints.push(J(g, [s * 1.0, 1.0, 0.6], [swing(10, 0.6, [1, 0, 0], s < 0 ? 0 : 0.5), snap(-35, 3)]));
    }
    r.joints.push(J('eyes', [0, 1.5, 0.8], [aimYaw(180)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 14, turn: 200, jump: 5 },
    HUNT(40),
    bite('claw_r', 24, 1.1, 2.2),
    bite('claw_l', 24, 0.9, 2.2),
  ],
  functions: [...WALKER],
  verb: 'strike',
};

/** Queen spider: a huge brood mother — webs that pin you, venom fangs, egg sacs lobbed over cover. */
export const spiderQueen: CatalogEntry = {
  id: 'spider_queen',
  names: ['spider queen', 'queen spider', 'broodmother', 'brood mother', 'giant tarantula', 'mother spider', 'shelob', 'aragog', 'spider boss', 'spider mom', 'mega spider', 'huge spider', 'spider queen boss'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [2.6, 2.0, 3.0], [0, 2.1, -1.6], { role: 'abdomen' }));
    r.parts.push(P('sphere', [1.7, 1.3, 1.8], [0, 1.8, 0.6], { role: 'body' }));
    for (const x of [-0.35, -0.15, 0.15, 0.35]) r.parts.push(P('sphere', [0.18, 0.18, 0.12], [x, 2.25, 1.45], { role: 'eye', solid: false }));
    r.parts.push(P('cone', [0.8, 0.9, 0.8], [0, 3.2, -1.6], { role: 'crown', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.2, 0.7, 0.2], [s * 0.25, 1.25, 1.55], { role: 'fang', rot: [180, 0, 0], group: 'fangs', solid: false }));
    r.parts.push(P('sphere', [0.16, 0.16, 0.16], [0, 1.2, 1.75], { role: 'muzzle', group: 'fangs', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 2.1, 1.55], { role: 'muzzle', group: 'spinneret', solid: false }));
    r.parts.push(P('sphere', [0.5, 0.5, 0.5], [0, 2.9, -2.6], { role: 'egg', group: 'sac', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 3.2, -2.6], { role: 'muzzle', group: 'sac', solid: false }));
    const hips = [1.0, 0.6, 0.2, -0.2];
    const foot = [2.9, 1.3, -0.4, -2.0];
    hips.forEach((hz, i) => {
      for (const side of [-1, 1]) {
        const g = `leg_${side < 0 ? 'l' : 'r'}${i}`;
        const hip: Vec3 = [side * 0.7, 1.8, hz];
        const knee: Vec3 = [side * 2.0, 3.0, (hz + foot[i]) / 2];
        const ft: Vec3 = [side * 3.1, 0.03, foot[i]];
        r.parts.push(limb('capsule', hip, knee, 0.3, { role: 'leg', group: g, solid: false }));
        r.parts.push(limb('cone', knee, ft, 0.28, { role: 'leg', group: g, solid: false }));
        const ph = (i + (side < 0 ? 0 : 1)) % 2 ? 0.5 : 0;
        r.joints.push(J(g, hip, [gait(16, 1.8, ph, [0, 1, 0]), bob(0.22, 1.8, ph + 0.25)]));
      }
    });
    r.joints.push(
      J('fangs', [0, 1.6, 1.4], [snap(30, 3)]),
      J('spinneret', [0, 2.1, 1.3], [aimBoth(160, -60, 40)]),
      J('sac', [0, 2.8, -2.2], [bob(0.15, 0.6, 0, false), recoil(0.5, 2, [0, 1, 0])]),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 14, turn: 170, jump: 12 },
    HUNT(50),
    { type: 'weapon', kind: 'net', group: 'spinneret', rate: 0.5, speed: 40, damage: 6, range: 32, size: 0.6 },
    bite('fangs', 30, 1.1, 2.4),
    { type: 'weapon', kind: 'bomb', group: 'sac', rate: 0.3, speed: 22, damage: 18, range: 24, size: 0.5 },
  ],
  functions: [...WALKER, 'shoots', 'projectile_attack', 'restrain_target', 'aims_at_target'],
  verb: 'strike',
};

/** Kraken: floating sea monster — tentacles lash everything around it and ink-snares pin targets from afar. */
export const kraken: CatalogEntry = {
  id: 'kraken',
  names: ['octopus', 'octopuses', 'octopi', 'giant octopus', 'squid', 'giant squid', 'colossal squid', 'cthulhu', 'sea monster', 'octopuss', 'octapus', 'octopous', 'krakken', 'craken', 'release the kraken', 'leviathan'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('sphere', [1.9, 1.7, 1.9], [0, 2.1, 0], { role: 'body' }));
    r.parts.push(P('capsule', [1.5, 2.2, 1.5], [0, 3.2, -0.5], { role: 'mantle', rot: [-30, 0, 0], group: 'mantle', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.34, 0.34, 0.2], [s * 0.5, 2.3, 0.85], { role: 'eye', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 1.9, 1.0], { role: 'muzzle', group: 'siphon', solid: false }));
    r.parts.push(P('cylinder', [0.3, 0.4, 0.3], [0, 1.9, 0.85], { role: 'siphon', rot: [90, 0, 0], group: 'siphon', solid: false }));
    for (let k = 0; k < 6; k++) {
      const a = (k / 6) * Math.PI * 2 + 0.3;
      const dx = Math.sin(a), dz = Math.cos(a);
      chain(r, `ten${k}_`, [dx * 0.7, 1.4, dz * 0.7], (i) => [dx * 0.7, i === 0 ? -0.7 : -0.25, dz * 0.7], 2, 0.34, 0.16, () => [wave(22, 0.9, [dz, 0, -dx], k * 0.17)], { role: 'tentacle' });
    }
    r.joints.push(J('mantle', [0, 2.6, -0.2], [swing(8, 0.5, [1, 0, 0])]), J('siphon', [0, 1.9, 0.7], [aimBoth(140, -60, 30)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 12, turn: 140, altitude: 0.6 },
    HUNT(40),
    bodyHit(18, 1.2, 1.6),
    { type: 'weapon', kind: 'net', group: 'siphon', rate: 0.35, speed: 32, damage: 6, range: 24, size: 0.7 },
  ],
  functions: ['moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'shoots', 'projectile_attack', 'restrain_target', 'grab_target', 'area_attack'],
  verb: 'grab',
};

// ================================================================== mounts

/** Snake: a waving serpent body, a raised hooded head that lunges from 3 m and spits venom. */
export const cobra: CatalogEntry = {
  id: 'cobra',
  names: ['snake', 'serpent', 'python', 'anaconda', 'viper', 'eel', 'centipede', 'king cobra', 'snakes', 'giant snake', 'rattlesnake', 'rattler', 'mamba', 'black mamba', 'boa', 'boa constrictor', 'basilisk', 'snek', 'snaek', 'sanke', 'cobras', 'spitting cobra'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  replaces: ['snake'],
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.6, 1.5, 0.6], [0, 0.32, 0], { role: 'body', rot: [90, 0, 0] }));
    // raised neck + hood + head (yaws to the target), jaw lunges
    r.parts.push(limb('capsule', [0, 0.35, 0.6], [0, 1.5, 1.0], 0.46, { role: 'neck', group: 'head', solid: false }));
    r.parts.push(P('sphere', [1.1, 1.0, 0.18], [0, 1.45, 0.92], { role: 'hood', rot: [-15, 0, 0], group: 'head', solid: false }));
    r.parts.push(P('box', [0.45, 0.3, 0.6], [0, 1.72, 1.3], { role: 'head', group: 'head', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.08, 0.08, 0.06], [s * 0.16, 1.85, 1.52], { role: 'eye', group: 'head', solid: false }));
    r.parts.push(P('box', [0.36, 0.1, 0.5], [0, 1.53, 1.35], { role: 'jaw', group: 'jaw', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.06, 0.2, 0.06], [s * 0.1, 1.5, 1.55], { role: 'fang', rot: [180, 0, 0], group: 'jaw', solid: false }));
    r.parts.push(P('sphere', [0.1, 0.1, 0.1], [0, 1.6, 1.65], { role: 'muzzle', group: 'jaw', solid: false }));
    chain(r, 'body', [0, 0.32, -0.75], [0, 0, -0.8], 5, 0.55, 0.18, () => [wave(26, 1.3, [0, 1, 0])]);
    r.joints.push(J('head', [0, 0.35, 0.6], [aimYaw(240), swing(5, 0.9, [0, 0, 1])]), J('jaw', [0, 1.6, 1.1], [snap(30, 3), recoil(-0.9, 3)], 'head'));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 18, turn: 200 },
    HUNT(40),
    bite('jaw', 24, 1.2, 3.0),
    { type: 'weapon', kind: 'gun', group: 'jaw', rate: 0.7, speed: 45, damage: 7, range: 20, size: 0.12 },
  ],
  functions: ['drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'shoots', 'projectile_attack'],
  verb: 'strike',
};

/** Horse: galloping mount you're put on — fast, jumps well, kicks what's in front. Trots after you when you get off. */
export const horse: CatalogEntry = {
  id: 'horse',
  names: ['pony', 'steed', 'stallion', 'unicorn', 'donkey', 'camel', 'zebra', 'horses', 'mare', 'war horse', 'warhorse', 'battle horse', 'mustang', 'horsey', 'horsie', 'hors', 'hourse', 'charger', 'destrier', 'bronco', 'palomino', 'thoroughbred', 'noble steed', 'trusty steed', 'nightmare horse'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  replaces: ['horse'],
  build() {
    const q = quad({ len: 2.0, girth: 0.95, hipY: 1.25, legT: 0.2, legX: 0.28, head: [0.42, 0.5, 0.85], neck: 1.0, neckZ: 0.25, snout: [0.32, 0.3, 0.3], gait: [38, 2.6], aim: 90 });
    const r = q.r;
    r.parts.push(P('box', [0.12, 0.9, 0.3], [0, q.by + 0.7, 1.1], { role: 'mane', rot: [-30, 0, 0], group: 'head', solid: false }));
    chain(r, 'tail', [0, q.by + 0.3, -1.05], [0, -0.45, -0.3], 2, 0.22, 0.28, () => [wave(18, 1.2, [0, 1, 0])]);
    r.parts.push(P('box', [0.9, 0.18, 0.8], [0, q.by + 0.52, -0.1], { role: 'saddle', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('box', [0.06, 0.5, 0.06], [s * 0.5, q.by + 0.15, -0.1], { role: 'stirrup', solid: false }));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 30, turn: 150, jump: 11 },
    FOLLOW(25),
    bite('jaw', 12, 1.2, 1.8),
    { type: 'rideable', seat: [0, 2.6, -0.1] },
  ],
  functions: ['walks', 'drives_on_ground', 'has_moving_parts', 'follows_player', 'returns', 'attacks_enemies', 'damages_target', ...RIDE],
};
