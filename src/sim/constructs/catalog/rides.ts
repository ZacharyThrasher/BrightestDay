/**
 * Combat vehicles & rides. War machines start beside you and hunt on their own (G hops you in: WASD drives, the
 * brain keeps gunning, clicking takes the guns); pure rides (hoverbike, rocket, coaster cart, chariot) put you
 * in the seat straight away. Counters: chargers (train, monster truck, drill tank, steamroller) turn slowly —
 * side-step; aircraft need line of sight and turn wide; everything big is slow off the line.
 * Frame: +z forward, +y up, wheels on y = 0.
 */
import type { CatalogEntry } from './types.js';
import { P, J, limb, wheel, roll, steer, spin, swing, snap, recoil, aimYaw, aimPitch, aimBoth, gait, newRig, type Rig } from './kit.js';
import { HUNT, FOLLOW, bite, bodyHit } from './beasts.js';

const RIDE = ['rideable', 'carries_player', 'propel_player'];
const DRIVER = ['drives_on_ground', 'wheels_spin', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'pushes_objects'];
const GUNNER = ['shoots', 'projectile_attack', 'aims_at_target'];

/** Wheels on both sides of an axle rolling together (one group), optionally steering (separate groups per side). */
function axle(r: Rig, name: string, z: number, halfTrack: number, y: number, d: number, w: number, steerAmp = 0) {
  if (!steerAmp) {
    for (const x of [-halfTrack, halfTrack]) r.parts.push(...wheel(name, [x, y, z], d, w));
    r.joints.push(J(name, [0, y, z], [roll(d / 2)]));
    return;
  }
  for (const x of [-halfTrack, halfTrack]) {
    const g = `${name}_${x < 0 ? 'l' : 'r'}`;
    r.parts.push(...wheel(g, [x, y, z], d, w));
    r.joints.push(J(g, [x, y, z], [steer(steerAmp), roll(d / 2)]));
  }
}

/** Tank-style tracks: two tread boxes + rolling road-wheel axles. */
function tracks(r: Rig, halfTrack: number, len: number, h: number, zs: number[], d: number) {
  for (const x of [-halfTrack, halfTrack]) r.parts.push(P('box', [0.8, h, len], [x, h / 2, 0], { role: 'tread' }));
  zs.forEach((z, i) => axle(r, `axle${i}`, z, halfTrack + 0.45, d / 2, d, 0.2));
}

// ================================================================== ground war machines

/** Monster truck: giant bouncing wheels, jumps, crushes whatever it lands on or drives through. Hop in with G. */
export const monsterTruck: CatalogEntry = {
  id: 'monster_truck',
  names: ['monster truck', 'monster trucks', 'monstertruck', 'monster truk', 'bigfoot truck', 'crusher truck', 'monster jam', 'grave digger', 'four by four', '4x4', 'lifted truck', 'truck of doom', 'war truck', 'battle truck'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 600,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.4, 0.9, 4.4], [0, 2.3, 0], { role: 'body' }));
    r.parts.push(P('box', [2.2, 0.9, 1.9], [0, 3.2, -0.3], { role: 'cab', solid: false }));
    r.parts.push(P('wedge', [2.1, 0.7, 0.6], [0, 3.1, 0.95], { role: 'windshield', rot: [0, 180, 0], solid: false }));
    r.parts.push(P('box', [2.6, 0.3, 0.3], [0, 2.1, 2.3], { role: 'bumper', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('sphere', [0.3, 0.3, 0.1], [s * 0.8, 2.4, 2.22], { role: 'light', solid: false }));
    r.parts.push(P('box', [2.0, 0.12, 0.2], [0, 3.7, 0.4], { role: 'lightbar', solid: false }));
    axle(r, 'front', 1.5, 1.45, 1.1, 2.2, 0.9, 28);
    axle(r, 'rear', -1.5, 1.45, 1.1, 2.2, 0.9);
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 30, turn: 90, jump: 12 },
    HUNT(50),
    bodyHit(30, 1, 1.4),
    { type: 'rideable', seat: [0, 3.9, -0.4] },
  ],
  functions: [...DRIVER, ...RIDE, 'crush_target'],
  verb: 'crush',
};

/** Battle dozer: armoured tracked pusher — its blade shoves targets (and crates, boulders) out of the way. */
export const dozer: CatalogEntry = {
  id: 'battle_dozer',
  names: ['battle dozer', 'war dozer', 'armored bulldozer', 'armoured bulldozer', 'armored dozer', 'dozer tank', 'siege dozer', 'killdozer', 'kill dozer', 'bulldozer tank', 'combat bulldozer', 'tank dozer', 'plow tank', 'plough tank'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.6, 1.2, 3.6], [0, 1.5, -0.2], { role: 'body' }));
    r.parts.push(P('box', [1.8, 1.3, 1.5], [0, 2.75, -0.8], { role: 'cab', solid: false }));
    r.parts.push(P('cylinder', [0.25, 1.2, 0.25], [0.8, 2.9, 0.6], { role: 'exhaust', solid: false }));
    tracks(r, 1.35, 4.0, 1.0, [-1.3, 0, 1.3], 0.8);
    // blade on push arms: pistons forward on each shove
    r.parts.push(P('box', [3.6, 1.5, 0.35], [0, 1.0, 2.55], { role: 'blade', rot: [-12, 0, 0], group: 'blade' }));
    for (const s of [-1, 1]) r.parts.push(limb('box', [s * 1.1, 1.3, 0.9], [s * 1.1, 0.9, 2.4], 0.25, { role: 'arm', group: 'blade', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 1.0, 2.8], { role: 'muzzle', group: 'blade', solid: false }));
    r.joints.push(J('blade', [0, 1.3, 0.9], [recoil(-0.8, 2), swing(3, 0.3, [1, 0, 0])]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 15, turn: 70, jump: 4 },
    HUNT(50),
    bite('blade', 22, 1.2, 1.8),
    { type: 'rideable', seat: [0, 4.0, -0.8] },
  ],
  functions: [...DRIVER, ...RIDE, 'push_target', 'move_obstacle'],
  verb: 'push',
};

/** Steamroller: a massive rolling drum that flattens what it catches. Slow to turn — step aside. */
export const steamroller: CatalogEntry = {
  id: 'steamroller',
  names: ['steam roller', 'steamrollers', 'road roller', 'roller truck', 'flattener', 'steamrolla', 'steam rolla', 'steamroler', 'compactor', 'road compactor', 'crusher roller'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 1200,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.0, 1.2, 2.8], [0, 1.6, -0.9], { role: 'body' }));
    r.parts.push(P('box', [1.8, 0.1, 1.8], [0, 3.6, -1.1], { role: 'roof', solid: false }));
    for (const x of [-0.8, 0.8]) for (const z of [-1.9, -0.3]) r.parts.push(P('box', [0.08, 1.4, 0.08], [x, 2.9, z], { role: 'post', solid: false }));
    r.parts.push(P('cylinder', [0.35, 1.6, 0.35], [0, 2.7, 0.3], { role: 'smokestack', solid: false }));
    r.parts.push(P('box', [2.6, 0.3, 1.4], [0, 2.3, 1.4], { role: 'yoke', solid: false }));
    // the drum: one big roller up front
    r.parts.push(P('cylinder', [2.2, 2.6, 2.2], [0, 1.1, 1.4], { role: 'wheel', rot: [0, 0, 90], group: 'drum' }));
    r.parts.push(P('box', [2.7, 1.9, 0.3], [0, 1.1, 1.4], { role: 'hub', group: 'drum', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 0.6, 2.5], { role: 'muzzle', group: 'drum', solid: false }));
    axle(r, 'rear', -1.9, 1.1, 0.8, 1.6, 0.5);
    r.joints.push(J('drum', [0, 1.1, 1.4], [roll(1.1)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 16, turn: 55 },
    HUNT(50),
    bite('drum', 38, 1, 1.4),
    { type: 'rideable', seat: [0, 3.0, -1.1] },
  ],
  functions: [...DRIVER, ...RIDE, 'crush_target'],
  verb: 'crush',
};

/** Runaway train: a locomotive that charges down a straight line at full steam. Barely turns — just step off the tracks. */
export const train: CatalogEntry = {
  id: 'train',
  names: ['trains', 'locomotive', 'steam train', 'steam engine', 'choo choo', 'choo-choo', 'choochoo', 'choo choo train', 'bullet train', 'freight train', 'runaway train', 'express train', 'train engine', 'steam locomotive', 'thomas the tank engine', 'trian', 'tran'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 1000,
  build() {
    const r = newRig();
    r.parts.push(P('cylinder', [1.8, 4.2, 1.8], [0, 2.0, 0.6], { role: 'boiler', rot: [90, 0, 0] }));
    r.parts.push(P('box', [2.3, 2.4, 1.8], [0, 2.4, -2.3], { role: 'cab' }));
    r.parts.push(P('box', [2.5, 0.2, 2.1], [0, 3.7, -2.3], { role: 'roof', solid: false }));
    r.parts.push(P('box', [2.3, 0.4, 6.2], [0, 1.0, -0.2], { role: 'frame' }));
    r.parts.push(P('cylinder', [0.6, 1.3, 0.6], [0, 3.4, 1.9], { role: 'smokestack', solid: false }));
    r.parts.push(P('cone', [0.9, 0.5, 0.9], [0, 4.2, 1.9], { role: 'smokestack', rot: [180, 0, 0], solid: false }));
    r.parts.push(P('dome', [0.8, 0.5, 0.8], [0, 2.9, 0.2], { role: 'dome', solid: false }));
    r.parts.push(P('wedge', [2.3, 1.0, 0.9], [0, 0.6, 3.25], { role: 'cowcatcher', group: 'cowcatcher' }));
    r.parts.push(P('sphere', [0.5, 0.5, 0.3], [0, 2.0, 2.75], { role: 'light', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 1.0, 3.8], { role: 'muzzle', group: 'cowcatcher', solid: false }));
    [1.8, 0.4, -1.0].forEach((z, i) => axle(r, `axle${i}`, z, 1.25, 0.65, 1.3, 0.25));
    // piston rods working with the wheels
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'rod_l' : 'rod_r';
      r.parts.push(P('box', [0.1, 0.12, 2.6], [s * 1.42, 0.65, 0.4], { role: 'rod', group: g, solid: false }));
      r.joints.push(J(g, [s * 1.42, 0.65, 0.4], [{ kind: 'piston', axis: [0, 0, 1], amp: 0.3, rate: 2, phase: s < 0 ? 0 : 0.25, gait: true }]));
    }
    r.joints.push(J('cowcatcher', [0, 0.6, 2.8], [recoil(-0.4, 3)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 34, turn: 28 },
    HUNT(60),
    bite('cowcatcher', 40, 1, 1.5),
    { type: 'rideable', seat: [0, 3.0, -2.3] },
  ],
  functions: [...DRIVER, ...RIDE],
  verb: 'strike',
};

/** Drill tank: tracked charger with a spinning drill nose — bores into targets and walls alike. */
export const drillTank: CatalogEntry = {
  id: 'drill_tank',
  names: ['drill tank', 'drill machine', 'drill vehicle', 'drill car', 'mole machine', 'tunneler', 'tunneller', 'burrowing machine', 'driller', 'mega drill', 'drill mobile', 'drillmobile', 'drill train'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.4, 1.4, 3.2], [0, 1.5, -0.4], { role: 'body' }));
    r.parts.push(P('dome', [1.4, 0.8, 1.4], [0, 2.2, -0.8], { role: 'cockpit', solid: false }));
    tracks(r, 1.3, 3.8, 1.0, [-1.2, 0.2], 0.8);
    r.parts.push(P('cone', [2.0, 2.8, 2.0], [0, 1.5, 2.6], { role: 'drill', rot: [90, 0, 0], group: 'drill' }));
    for (let k = 0; k < 3; k++) r.parts.push(P('torus', [1.8 - k * 0.5, 0.12, 1.8 - k * 0.5], [0, 1.5, 1.6 + k * 0.7], { role: 'thread', rot: [90, 0, k * 30], group: 'drill', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 1.5, 4.0], { role: 'muzzle', group: 'drill', solid: false }));
    r.joints.push(J('drill', [0, 1.5, 1.2], [spin(3, [0, 0, 1]), recoil(-0.5, 3)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 26, turn: 60, jump: 4 },
    HUNT(50),
    bite('drill', 34, 1.6, 1.4),
    { type: 'rideable', seat: [0, 2.9, -0.8] },
  ],
  functions: [...DRIVER, ...RIDE],
  verb: 'strike',
};

/** Wrecking crane: tracked crane whose boom turns to the target and swings a wrecking ball through it. */
export const wreckingCrane: CatalogEntry = {
  id: 'wrecking_crane',
  names: ['wrecking crane', 'demolition crane', 'crane truck', 'mobile crane', 'wrecker crane', 'demolition machine', 'crawler crane', 'wrecking ball crane', 'crane with a wrecking ball', 'demo crane'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 900,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.6, 1.0, 3.4], [0, 1.4, 0], { role: 'body' }));
    tracks(r, 1.35, 3.8, 1.0, [-1.2, 1.2], 0.8);
    r.parts.push(P('box', [2.0, 1.6, 2.0], [0, 2.7, -0.4], { role: 'cab', group: 'boom' }));
    r.parts.push(limb('box', [0, 3.0, 0.4], [0, 7.2, 3.8], 0.4, { role: 'boom', group: 'boom', solid: false }));
    r.parts.push(P('box', [1.6, 0.9, 0.9], [0, 2.9, -1.6], { role: 'counterweight', group: 'boom', solid: false }));
    // cable + ball hang from the boom tip and swing
    r.parts.push(P('cylinder', [0.08, 3.4, 0.08], [0, 5.5, 3.8], { role: 'cable', group: 'ball', solid: false }));
    r.parts.push(P('sphere', [1.6, 1.6, 1.6], [0, 3.4, 3.8], { role: 'ball', group: 'ball', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 3.4, 4.7], { role: 'muzzle', group: 'ball', solid: false }));
    r.joints.push(
      J('boom', [0, 2.0, -0.4], [aimYaw(70)]),
      J('ball', [0, 7.2, 3.8], [swing(22, 0.5, [1, 0, 0]), snap(-70, 1.4)], 'boom'),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 60 },
    HUNT(45),
    bite('ball', 45, 0.6, 3.0),
    { type: 'rideable', seat: [0, 4.0, -0.4] },
  ],
  functions: [...DRIVER, ...RIDE, 'aims_at_target', 'move_obstacle'],
  verb: 'strike',
};

/** Land battleship: a warship on eight wheels with two turreted cannons and an AA gun. Slow and huge — flank it. */
export const battleship: CatalogEntry = {
  id: 'battleship',
  names: ['battleships', 'battle ship', 'battelship', 'warship', 'war ship', 'dreadnought', 'dreadnaught', 'destroyer', 'land battleship', 'land ship', 'landship', 'battlecruiser', 'battle cruiser', 'ironclad', 'frigate', 'gunboat', 'land boat'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  density: 800,
  build() {
    const r = newRig();
    r.parts.push(P('box', [3.2, 1.6, 8.0], [0, 1.8, 0], { role: 'hull' }));
    r.parts.push(P('wedge', [3.2, 1.6, 1.6], [0, 1.8, 4.8], { role: 'bow', rot: [0, 180, 0], solid: false }));
    r.parts.push(P('box', [2.0, 1.6, 2.2], [0, 3.4, -0.8], { role: 'bridge', solid: false }));
    r.parts.push(P('box', [1.2, 0.8, 1.2], [0, 4.6, -0.8], { role: 'tower', solid: false }));
    r.parts.push(P('cylinder', [0.7, 1.6, 0.7], [0, 4.0, -2.4], { role: 'funnel', solid: false }));
    r.parts.push(P('cylinder', [0.06, 2.0, 0.06], [0, 5.9, -0.8], { role: 'mast', solid: false }));
    [3.0, 1.0, -1.0, -3.0].forEach((z, i) => axle(r, `axle${i}`, z, 1.75, 0.7, 1.4, 0.4));
    for (const [k, z] of [['f', 2.4], ['b', -3.8]] as [string, number][]) {
      const tg = `turret_${k}`, bg = `guns_${k}`;
      const dir = k === 'f' ? 1 : -1;
      r.parts.push(P('cylinder', [1.6, 0.7, 1.6], [0, 2.95, z], { role: 'turret', group: tg, solid: false }));
      for (const x of [-0.3, 0.3]) {
        r.parts.push(P('cylinder', [0.22, 2.2, 0.22], [x, 3.05, z + dir * 1.5], { role: 'barrel', rot: [90, 0, 0], group: bg, solid: false }));
        r.parts.push(P('sphere', [0.16, 0.16, 0.16], [x, 3.05, z + dir * 2.65], { role: 'muzzle', group: bg, solid: false }));
      }
      r.joints.push(J(tg, [0, 2.95, z], [aimYaw(60)]), J(bg, [0, 3.05, z + dir * 0.4], [aimPitch(40, -5, 40), recoil(0.35, 2)], tg));
    }
    r.parts.push(P('box', [0.5, 0.4, 0.8], [1.0, 4.4, -0.2], { role: 'gun', group: 'aa', solid: false }));
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [1.0, 4.4, 0.3], { role: 'muzzle', group: 'aa', solid: false }));
    r.joints.push(J('aa', [1.0, 4.4, -0.2], [aimBoth(200, -30, 80)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 35 },
    HUNT(60),
    { type: 'weapon', kind: 'cannon', group: 'turret_f', rate: 0.45, speed: 60, damage: 30, range: 60, size: 0.4 },
    { type: 'weapon', kind: 'cannon', group: 'turret_b', rate: 0.4, speed: 60, damage: 30, range: 60, size: 0.4 },
    { type: 'weapon', kind: 'gun', group: 'aa', rate: 5, speed: 80, damage: 4, range: 40, size: 0.12, spread: 3 },
    { type: 'rideable', seat: [0, 5.4, -0.8] },
  ],
  functions: [...DRIVER, ...GUNNER, ...RIDE],
  verb: 'shoot',
};

/** Hovercraft: skims a metre up on its skirt, twin rear fans, a turret that tracks. Quick and slidey. */
export const hovercraft: CatalogEntry = {
  id: 'hovercraft',
  names: ['hover craft', 'hovercrafts', 'hoovercraft', 'air cushion vehicle', 'hover tank', 'hovertank', 'grav tank', 'gravtank', 'hover truck', 'hover car', 'hovercar', 'skimmer'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('box', [2.6, 0.8, 4.0], [0, 0.9, 0], { role: 'hull' }));
    r.parts.push(P('torus', [3.4, 0.6, 4.6], [0, 0.4, 0], { role: 'skirt', solid: false }));
    r.parts.push(P('box', [1.8, 0.8, 1.6], [0, 1.7, 0.3], { role: 'cabin', solid: false }));
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'fan_l' : 'fan_r';
      r.parts.push(P('torus', [1.5, 0.18, 1.5], [s * 0.8, 2.0, -1.8], { role: 'duct', rot: [90, 0, 0], solid: false }));
      r.parts.push(P('box', [1.3, 0.1, 0.1], [s * 0.8, 2.0, -1.8], { role: 'propeller', group: g, solid: false }));
      r.parts.push(P('box', [0.1, 1.3, 0.1], [s * 0.8, 2.0, -1.8], { role: 'propeller', group: g, solid: false }));
      r.joints.push(J(g, [s * 0.8, 2.0, -1.8], [spin(6 * s, [0, 0, 1])]));
    }
    r.parts.push(P('cylinder', [0.8, 0.4, 0.8], [0, 2.3, 0.6], { role: 'turret', group: 'turret', solid: false }));
    r.parts.push(P('cylinder', [0.14, 1.2, 0.14], [0, 2.35, 1.3], { role: 'barrel', rot: [90, 0, 0], group: 'turret', solid: false }));
    r.parts.push(P('sphere', [0.12, 0.12, 0.12], [0, 2.35, 1.95], { role: 'muzzle', group: 'turret', solid: false }));
    r.joints.push(J('turret', [0, 2.3, 0.6], [aimBoth(180, -20, 40), recoil(0.15, 6)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 26, turn: 110, altitude: 0.8 },
    HUNT(50),
    { type: 'weapon', kind: 'gun', group: 'turret', rate: 4, speed: 75, damage: 6, range: 42, size: 0.14, spread: 2 },
    { type: 'rideable', seat: [0, 2.6, -0.4] },
  ],
  functions: ['moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', ...GUNNER, ...RIDE, 'pushes_objects'],
  verb: 'shoot',
};

/** Sky submarine: floats a few metres up, periscope watching, fires homing torpedoes. Break its line of sight. */
export const submarine: CatalogEntry = {
  id: 'submarine',
  names: ['submarines', 'sub', 'sky sub', 'air submarine', 'flying submarine', 'sky submarine', 'u-boat', 'uboat', 'u boat', 'yellow submarine', 'submarene', 'submarin', 'sub marine', 'torpedo boat', 'war sub'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [1.9, 7.0, 1.9], [0, 1.4, 0], { role: 'hull', rot: [90, 0, 0] }));
    r.parts.push(P('box', [0.9, 1.3, 1.8], [0, 2.8, 0.4], { role: 'tower', solid: false }));
    r.parts.push(P('box', [2.2, 0.1, 0.6], [0, 2.9, 0.6], { role: 'fin', solid: false }));
    r.parts.push(P('cylinder', [0.12, 1.2, 0.12], [0, 3.9, 0.7], { role: 'periscope', group: 'periscope', solid: false }));
    r.parts.push(P('box', [0.16, 0.16, 0.4], [0, 4.5, 0.85], { role: 'periscope', group: 'periscope', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('box', [0.1, 1.2, 0.8], [s * 0.01, 1.4, -3.3], { role: 'fin', rot: [0, 0, s * 90], solid: false }));
    r.parts.push(P('box', [0.1, 1.3, 0.8], [0, 1.4, -3.3], { role: 'rudder', solid: false }));
    r.parts.push(P('box', [1.3, 0.12, 0.06], [0, 1.4, -3.9], { role: 'propeller', group: 'prop', solid: false }));
    r.parts.push(P('box', [0.12, 1.3, 0.06], [0, 1.4, -3.9], { role: 'propeller', group: 'prop', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(P('cylinder', [0.34, 0.3, 0.34], [s * 0.45, 1.1, 3.35], { role: 'tube', rot: [90, 0, 0], group: 'bow', solid: false }));
      r.parts.push(P('sphere', [0.16, 0.16, 0.16], [s * 0.45, 1.1, 3.6], { role: 'muzzle', group: 'bow', solid: false }));
    }
    for (let i = 0; i < 4; i++) r.parts.push(P('sphere', [0.3, 0.3, 0.1], [0.95, 1.6, 1.8 - i * 1.1], { role: 'porthole', solid: false }));
    r.joints.push(J('periscope', [0, 3.9, 0.7], [aimYaw(120)]), J('prop', [0, 1.4, -3.9], [spin(3, [0, 0, 1])]), J('bow', [0, 1.1, 3.2], [recoil(0.2, 3)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 16, turn: 70, altitude: 2.5 },
    HUNT(55),
    { type: 'weapon', kind: 'missile', group: 'bow', rate: 0.4, speed: 30, damage: 30, range: 55, size: 0.35 },
    { type: 'rideable', seat: [0, 3.8, 0.0] },
  ],
  functions: ['moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', ...GUNNER, ...RIDE],
  verb: 'shoot',
};

// ================================================================== aircraft

/** Fighter jet: fixed-wing strafing runs with nose guns and homing missiles. Wide turns — cut inside them. */
export const jetFighter: CatalogEntry = {
  id: 'fighter_jet',
  names: ['fighter jet', 'jet fighter', 'fighter plane', 'warplane', 'war plane', 'fighterjet', 'f-16', 'f16', 'f-22', 'f22', 'f-35', 'f35', 'strike fighter', 'dogfighter', 'top gun', 'stealth fighter', 'interceptor', 'x-wing', 'xwing', 'starfighter', 'jet fighter plane'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [1.1, 6.4, 1.1], [0, 1.5, 0], { role: 'fuselage', rot: [90, 0, 0] }));
    r.parts.push(P('cone', [1.0, 1.6, 1.0], [0, 1.5, 3.9], { role: 'nose', rot: [90, 0, 0], solid: false }));
    r.parts.push(P('dome', [0.8, 0.5, 1.6], [0, 2.0, 1.6], { role: 'canopy', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(P('wedge', [3.2, 0.14, 2.8], [s * 2.0, 1.4, -0.6], { role: 'wing', solid: false }));
      r.parts.push(P('box', [0.1, 1.4, 1.0], [s * 0.6, 2.3, -2.7], { role: 'fin', rot: [0, 0, s * -15], solid: false }));
      r.parts.push(P('cylinder', [0.14, 1.4, 0.14], [s * 3.2, 1.3, -0.2], { role: 'missile', rot: [90, 0, 0], solid: false }));
      r.parts.push(P('sphere', [0.1, 0.1, 0.1], [s * 0.5, 1.3, 3.3], { role: 'muzzle', group: 'guns', solid: false }));
      r.parts.push(P('sphere', [0.14, 0.14, 0.14], [s * 3.2, 1.3, 0.6], { role: 'muzzle', group: 'rails', solid: false }));
    }
    r.parts.push(P('cylinder', [0.8, 0.6, 0.8], [0, 1.5, -3.2], { role: 'exhaust', rot: [90, 0, 0], group: 'burner', solid: false }));
    r.parts.push(P('cone', [0.7, 1.4, 0.7], [0, 1.5, -4.0], { role: 'flame', rot: [-90, 0, 0], group: 'burner', solid: false }));
    r.joints.push(
      J('burner', [0, 1.5, -3.2], [{ kind: 'piston', axis: [0, 0, 1], amp: 0.12, rate: 6 }]),
      J('guns', [0, 1.3, 3.0], [recoil(0.1, 8)]),
      J('rails', [0, 1.3, 0.3], [recoil(0.2, 3)]),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 48, turn: 90, altitude: 12, minSpeed: 26 },
    HUNT(60),
    { type: 'weapon', kind: 'gun', group: 'guns', rate: 8, speed: 90, damage: 5, range: 50, size: 0.14, spread: 2 },
    { type: 'weapon', kind: 'missile', group: 'rails', rate: 0.3, speed: 32, damage: 28, range: 60, size: 0.3 },
    { type: 'rideable', seat: [0, 2.3, 1.5] },
  ],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'shoots', 'projectile_attack', ...RIDE, 'raise_player'],
  verb: 'shoot',
};

/** Bomber: four spinning props and a bomb bay — flies over the target and carpets the ground. Keep moving sideways. */
export const bomber: CatalogEntry = {
  id: 'bomber',
  names: ['bombers', 'bomber plane', 'bomber jet', 'b-52', 'b52', 'b-2', 'b2', 'stealth bomber', 'carpet bomber', 'bomb plane', 'heavy bomber', 'dive bomber', 'lancaster', 'flying fortress', 'bommer'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [1.6, 8.0, 1.6], [0, 2.0, 0], { role: 'fuselage', rot: [90, 0, 0] }));
    r.parts.push(P('box', [13.0, 0.2, 2.0], [0, 2.0, 0.4], { role: 'wing', solid: false }));
    r.parts.push(P('box', [4.0, 0.14, 1.2], [0, 2.4, -3.6], { role: 'tail', solid: false }));
    r.parts.push(P('box', [0.14, 1.8, 1.3], [0, 3.2, -3.6], { role: 'fin', solid: false }));
    r.parts.push(P('dome', [1.0, 0.6, 1.4], [0, 2.7, 2.6], { role: 'canopy', solid: false }));
    [-4.4, -2.2, 2.2, 4.4].forEach((x, i) => {
      const g = `prop${i}`;
      r.parts.push(P('cylinder', [0.7, 1.4, 0.7], [x, 1.85, 1.2], { role: 'engine', rot: [90, 0, 0], solid: false }));
      r.parts.push(P('box', [0.14, 1.8, 0.06], [x, 1.85, 1.95], { role: 'propeller', group: g, solid: false }));
      r.parts.push(P('box', [1.8, 0.14, 0.06], [x, 1.85, 1.95], { role: 'propeller', group: g, solid: false }));
      r.joints.push(J(g, [x, 1.85, 1.95], [spin(7, [0, 0, 1])]));
    });
    r.parts.push(P('box', [0.9, 0.3, 1.8], [0, 1.15, 0.2], { role: 'bay', group: 'bay', solid: false }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 0.85, 0.2], { role: 'muzzle', group: 'bay', solid: false }));
    r.joints.push(J('bay', [0, 1.15, 0.2], [recoil(0.25, 2, [0, -1, 0])]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 26, turn: 60, altitude: 14, minSpeed: 14 },
    HUNT(60),
    { type: 'weapon', kind: 'bomb', group: 'bay', rate: 1, speed: 20, damage: 30, range: 30, size: 0.6 },
    { type: 'rideable', seat: [0, 3.2, 2.2] },
  ],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'shoots', 'projectile_attack', 'area_attack', ...RIDE, 'raise_player'],
  verb: 'shoot',
};

/** UFO: flying saucer with a spinning rim; circles overhead, a tractor beam pins its target while the death ray burns. */
export const ufo: CatalogEntry = {
  id: 'ufo',
  names: ['flying saucer', 'u.f.o.', 'u.f.o', 'alien ship', 'alien spaceship', 'alien spacecraft', 'mothership', 'mother ship', 'flying disc', 'saucer', 'alien saucer', 'abductor', 'ufos', 'spaceship ufo', 'tractor beam ship', 'alien ufo'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('cylinder', [5.0, 0.6, 5.0], [0, 1.6, 0], { role: 'disc' }));
    r.parts.push(P('dome', [2.4, 1.2, 2.4], [0, 1.9, 0], { role: 'dome', solid: false }));
    r.parts.push(P('dome', [3.0, 0.6, 3.0], [0, 1.3, 0], { role: 'belly', rot: [180, 0, 0], solid: false }));
    r.parts.push(P('torus', [5.2, 0.3, 5.2], [0, 1.6, 0], { role: 'rim', group: 'rim', solid: false }));
    for (let k = 0; k < 6; k++) {
      const a = (k / 6) * Math.PI * 2;
      r.parts.push(P('sphere', [0.34, 0.34, 0.34], [Math.sin(a) * 2.6, 1.6, Math.cos(a) * 2.6], { role: 'light', group: 'rim', solid: false }));
    }
    r.parts.push(P('cone', [1.6, 1.4, 1.6], [0, 0.4, 0], { role: 'beam', solid: false }));
    r.parts.push(P('sphere', [0.5, 0.5, 0.5], [0, 0.9, 0], { role: 'emitter', group: 'ray', solid: false }));
    r.parts.push(P('sphere', [0.16, 0.16, 0.16], [0, 0.9, 0.35], { role: 'muzzle', group: 'ray', solid: false }));
    r.joints.push(J('rim', [0, 1.6, 0], [spin(0.6, [0, 1, 0])]), J('ray', [0, 0.9, 0], [aimBoth(240, -89, 20)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 26, turn: 200, altitude: 9 },
    HUNT(55),
    { type: 'weapon', kind: 'laser', group: 'ray', rate: 1.5, speed: 0, damage: 11, range: 35 },
    { type: 'weapon', kind: 'net', group: 'ray', rate: 0.35, speed: 30, damage: 4, range: 30, size: 0.8 },
    { type: 'rideable', seat: [0, 3.0, 0] },
  ],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', ...GUNNER, 'restrain_target', ...RIDE, 'raise_player'],
  verb: 'shoot',
};

// ================================================================== rides (you're put in the seat)

/** Hoverbike / jetski: skims a metre above anything, very fast, jumps, nose blaster auto-fires at enemies. */
export const hoverbike: CatalogEntry = {
  id: 'hoverbike',
  names: ['hover bike', 'hoverbikes', 'speeder', 'speeder bike', 'jet ski', 'jetski', 'jet-ski', 'jet skis', 'sea doo', 'seadoo', 'waverunner', 'wave runner', 'sky bike', 'skybike', 'grav bike', 'gravbike', 'hover cycle', 'hovercycle', 'swoop bike', 'hover motorcycle'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  build() {
    const r = newRig();
    r.parts.push(P('capsule', [0.8, 2.8, 0.7], [0, 0.9, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('wedge', [0.9, 0.5, 1.0], [0, 1.25, 0.9], { role: 'cowl', rot: [0, 180, 0], solid: false }));
    r.parts.push(P('box', [0.5, 0.16, 1.0], [0, 1.3, -0.4], { role: 'seat', solid: false }));
    r.parts.push(P('box', [1.0, 0.06, 0.06], [0, 1.55, 0.6], { role: 'handlebar', group: 'bars', solid: false }));
    for (const s of [-1, 1]) {
      r.parts.push(P('box', [0.5, 0.12, 1.6], [s * 0.55, 0.55, 0.3], { role: 'vane', rot: [0, 0, s * 20], solid: false }));
      r.parts.push(P('torus', [0.5, 0.12, 0.5], [s * 0.45, 0.6, -1.3], { role: 'thruster', rot: [90, 0, 0], solid: false }));
    }
    r.parts.push(P('cone', [0.4, 0.8, 0.4], [0, 0.9, -1.8], { role: 'flame', rot: [-90, 0, 0], group: 'burner', solid: false }));
    r.parts.push(P('cylinder', [0.12, 0.8, 0.12], [0, 0.75, 1.6], { role: 'barrel', rot: [90, 0, 0], group: 'gun', solid: false }));
    r.parts.push(P('sphere', [0.1, 0.1, 0.1], [0, 0.75, 2.05], { role: 'muzzle', group: 'gun', solid: false }));
    r.joints.push(
      J('bars', [0, 1.3, 0.6], [{ kind: 'steer', axis: [0, 1, 0], amp: 20 }]),
      J('burner', [0, 0.9, -1.4], [{ kind: 'piston', axis: [0, 0, 1], amp: 0.1, rate: 6 }]),
      J('gun', [0, 0.75, 1.2], [aimBoth(240, -30, 30), recoil(0.1, 6)]),
    );
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'hover', speed: 36, turn: 150, altitude: 0.9, jump: 10 },
    FOLLOW(30),
    { type: 'weapon', kind: 'gun', group: 'gun', rate: 5, speed: 80, damage: 5, range: 45, size: 0.12, spread: 2 },
    { type: 'rideable', seat: [0, 1.95, -0.4] },
  ],
  functions: ['has_moving_parts', 'follows_player', 'returns', 'attacks_enemies', 'damages_target', ...GUNNER, ...RIDE],
};

/** Rideable rocket: straddle a missile and fly it into them — the nose hits like a truck. Always moving forward. */
export const rocketRide: CatalogEntry = {
  id: 'rocket_ride',
  names: ['rocket ride', 'rideable rocket', 'ride a rocket', 'rocket mount', 'missile ride', 'ride the missile', 'rocket sled', 'nuke ride', 'dr strangelove', 'rocket steed', 'ridable rocket', 'rocket horse'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 120,
  build() {
    const r = newRig();
    r.parts.push(P('cylinder', [1.0, 5.0, 1.0], [0, 1.2, 0], { role: 'body', rot: [90, 0, 0] }));
    r.parts.push(P('cone', [1.0, 1.4, 1.0], [0, 1.2, 3.2], { role: 'nose', rot: [90, 0, 0], group: 'nose' }));
    r.parts.push(P('sphere', [0.2, 0.2, 0.2], [0, 1.2, 3.95], { role: 'muzzle', group: 'nose', solid: false }));
    for (let k = 0; k < 4; k++) r.parts.push(...(k % 2 ? [P('box', [0.08, 1.0, 0.9], [0, 1.2 + (k === 1 ? 0.6 : -0.6), -2.2], { role: 'fin', solid: false })] : [P('box', [1.0, 0.08, 0.9], [k === 0 ? 0.6 : -0.6, 1.2, -2.2], { role: 'fin', solid: false })]));
    r.parts.push(P('box', [0.5, 0.14, 0.8], [0, 1.78, -0.3], { role: 'saddle', solid: false }));
    r.parts.push(P('cylinder', [0.8, 0.5, 0.8], [0, 1.2, -2.7], { role: 'nozzle', rot: [90, 0, 0], solid: false }));
    r.parts.push(P('cone', [0.8, 1.8, 0.8], [0, 1.2, -3.8], { role: 'flame', rot: [-90, 0, 0], group: 'burner', solid: false }));
    r.joints.push(J('burner', [0, 1.2, -2.9], [{ kind: 'piston', axis: [0, 0, 1], amp: 0.2, rate: 6 }]), J('nose', [0, 1.2, 2.5], [recoil(0.3, 3)]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 45, turn: 120, altitude: 3, minSpeed: 18 },
    HUNT(50),
    bite('nose', 45, 0.6, 1.8),
    { type: 'rideable', seat: [0, 2.3, -0.3] },
  ],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', ...RIDE, 'raise_player'],
  verb: 'strike',
};

/** Roller-coaster cart: a two-seat cart on its own stretch of track — whips along, launches off bumps, rams. */
export const coaster: CatalogEntry = {
  id: 'roller_coaster',
  names: ['roller coaster', 'rollercoaster', 'roller-coaster', 'coaster', 'coaster cart', 'coaster car', 'minecart', 'mine cart', 'mine car', 'mine kart', 'roller coster', 'rollercoster', 'rollarcoaster', 'thrill ride', 'coaster ride', 'wild mouse'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  build() {
    const r = newRig();
    // the cart
    r.parts.push(P('box', [1.8, 1.0, 2.6], [0, 1.1, 0], { role: 'body' }));
    r.parts.push(P('box', [1.6, 0.8, 0.3], [0, 1.9, -1.1], { role: 'seatback', solid: false }));
    r.parts.push(P('box', [1.6, 0.8, 0.3], [0, 1.9, 0.2], { role: 'seatback', solid: false }));
    r.parts.push(P('wedge', [1.8, 0.8, 0.8], [0, 1.2, 1.6], { role: 'nose', rot: [0, 180, 0], solid: false }));
    r.parts.push(P('torus', [1.2, 0.1, 0.6], [0, 2.2, -0.35], { role: 'lap bar', rot: [0, 0, 0], group: 'bar', solid: false }));
    axle(r, 'front', 0.9, 0.7, 0.35, 0.6, 0.25);
    axle(r, 'rear', -0.9, 0.7, 0.35, 0.6, 0.25);
    // a stretch of track under it, laid down as it goes
    for (const x of [-0.7, 0.7]) r.parts.push(P('box', [0.14, 0.1, 5.5], [x, 0.05, 0], { role: 'rail', solid: false }));
    for (const z of [-2.4, -1.2, 0, 1.2, 2.4]) r.parts.push(P('box', [1.8, 0.06, 0.25], [0, 0.02, z], { role: 'tie', solid: false }));
    r.joints.push(J('bar', [0, 2.2, -0.35], [swing(6, 1.5, [1, 0, 0])]));
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 34, turn: 100, jump: 14 },
    FOLLOW(30),
    bodyHit(20, 1, 1.2),
    { type: 'rideable', seat: [0, 2.1, -0.5] },
  ],
  functions: ['drives_on_ground', 'wheels_spin', 'has_moving_parts', 'follows_player', 'returns', 'attacks_enemies', 'damages_target', 'strike_target', ...RIDE],
};

/** War chariot: two galloping horses pulling a cart with scythed wheels. You drive; the blades cut what you pass. */
export const chariot: CatalogEntry = {
  id: 'chariot',
  names: ['chariots', 'war chariot', 'battle chariot', 'scythed chariot', 'roman chariot', 'horse chariot', 'horse and cart', 'horse cart', 'carriage', 'stagecoach', 'charriot', 'chariott'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  build() {
    const r = newRig();
    // cart
    r.parts.push(P('box', [1.8, 0.2, 1.4], [0, 1.0, -1.6], { role: 'floor' }));
    r.parts.push(P('box', [1.8, 1.0, 0.1], [0, 1.55, -0.95], { role: 'front', solid: false }));
    for (const s of [-1, 1]) r.parts.push(P('box', [0.1, 0.8, 1.4], [s * 0.9, 1.45, -1.6], { role: 'side', solid: false }));
    r.parts.push(...wheel('wheels', [-1.15, 0.8, -1.6], 1.6, 0.14), ...wheel('wheels', [1.15, 0.8, -1.6], 1.6, 0.14));
    for (const s of [-1, 1]) r.parts.push(P('cone', [0.14, 1.2, 0.14], [s * 1.8, 0.8, -1.6], { role: 'scythe', rot: [0, 0, s * -90], group: 'wheels', solid: false }));
    r.parts.push(P('box', [0.12, 0.12, 2.4], [0, 1.1, 0.1], { role: 'pole', solid: false }));
    r.joints.push(J('wheels', [0, 0.8, -1.6], [roll(0.8)]));
    // two horses, legs galloping
    for (const s of [-1, 1]) {
      const x = s * 0.65;
      r.parts.push(P('capsule', [0.8, 1.7, 0.8], [x, 1.55, 1.3], { role: 'horse', rot: [90, 0, 0], solid: false }));
      r.parts.push(limb('capsule', [x, 1.7, 2.0], [x, 2.4, 2.4], 0.34, { role: 'neck', solid: false }));
      r.parts.push(P('box', [0.34, 0.4, 0.7], [x, 2.45, 2.65], { role: 'head', solid: false }));
      for (const [lz, ph] of [[1.85, s < 0 ? 0 : 0.5], [0.75, s < 0 ? 0.5 : 0]] as [number, number][]) {
        const g = `hleg${s < 0 ? 'l' : 'r'}${lz > 1 ? 'f' : 'b'}`;
        r.parts.push(limb('capsule', [x, 1.3, lz], [x, 0.05, lz + 0.1], 0.16, { role: 'leg', group: g, solid: false }));
        r.joints.push(J(g, [x, 1.3, lz], [gait(40, 2.6, ph)]));
      }
    }
    return r;
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 30, turn: 120, jump: 6 },
    FOLLOW(30),
    bodyHit(18, 2, 1.5),
    { type: 'rideable', seat: [0, 2.2, -1.7] },
  ],
  functions: ['walks', 'drives_on_ground', 'wheels_spin', 'has_moving_parts', 'follows_player', 'returns', 'attacks_enemies', 'damages_target', 'strike_target', ...RIDE],
};
