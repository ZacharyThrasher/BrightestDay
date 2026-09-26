/**
 * Combat catalog, part 3: SIEGE — slow, heavy, long-range. Lobbed shots (bomb) fly a ballistic arc to where a
 * steady runner will be; a juke during the long flight dodges them, the splash punishes standing still.
 */
import type { CatalogEntry } from './types.js';
import type { Part } from '../spec.js';
import { P, J, wheel, roll, swing, snap, recoil, aimYaw, aimPitch, aimBoth } from './kit.js';
import { V, muzzle, guardB, huntB, weaponB, gunAtOrigin } from './weapons.js';

const SIEGE_FNS = ['has_moving_parts', 'aims_at_target', 'shoots', 'projectile_attack', 'attacks_enemies', 'damages_target', 'area_attack'];

/** Trebuchet: the turntable swings round to face them, the arm whips over and a boulder arcs onto their path. */
export const trebuchet: CatalogEntry = {
  id: 'trebuchet',
  names: ['trebuchay', 'trebuche', 'onager', 'mangonel', 'siege catapult', 'war catapult', 'boulder thrower', 'rock thrower', 'counterweight catapult', 'trebuchets', 'trebucket', 'siege engine'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 90,
  build() {
    const parts: Part[] = [
      P('box', [3.6, 0.4, 5.0], [0, 0.2, 0], { role: 'base' }),
      P('cylinder', [3.0, 0.3, 3.0], [0, 0.55, 0], { role: 'turntable', group: 'turret' }),
      P('box', [0.35, 4.2, 0.5], [-1.1, 2.7, 0], { role: 'frame', group: 'turret' }),
      P('box', [0.35, 4.2, 0.5], [1.1, 2.7, 0], { role: 'frame', group: 'turret' }),
      V('cylinder', [0.3, 2.6, 0.3], [0, 4.7, 0], 'axle', 'turret', [0, 0, 90]),
      V('box', [0.3, 0.3, 7.0], [0, 4.7, -1.8], 'arm', 'arm', [0, 0, 0]),
      V('box', [1.4, 1.4, 1.4], [0, 3.9, 1.3], 'counterweight', 'arm'),
      V('sphere', [0.9, 0.9, 0.9], [0, 4.7, -5.2], 'stone', 'arm'),
      muzzle([0, 6.4, 1.8], 'turret', 0.4),
    ];
    return { parts, joints: [J('turret', [0, 0.55, 0], [aimYaw(90)]), J('arm', [0, 4.7, 0], [snap(150, 1.1, [1, 0, 0])], 'turret')] };
  },
  behaviors: [guardB(60), weaponB('bomb', 'turret', 0.35, 30, 45, 60, { size: 1.0 })],
  functions: [...SIEGE_FNS, 'crush_target'],
  verb: 'shoot',
};

/** Ballista: a giant crossbow on a swivel; heavy bolts fly flat and fast. */
export const ballista: CatalogEntry = {
  id: 'ballista',
  names: ['siege ballista', 'scorpion ballista', 'giant crossbow', 'huge crossbow', 'siege crossbow', 'bolt launcher', 'balista', 'ballistae', 'spear launcher', 'javelin launcher', 'ballistas'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 90,
  build() {
    const h = 1.5;
    const parts: Part[] = [
      P('box', [2.4, 0.3, 2.4], [0, 0.15, 0], { role: 'base' }),
      ...[[-0.8, -0.8], [0.8, -0.8], [0, 0.9]].map(([x, z]) => V('box', [0.25, h, 0.25], [x, h / 2, z], 'leg')),
      P('box', [0.6, 0.5, 1.2], [0, h + 0.3, -1.1], { role: 'stock', group: 'gun' }),
      V('box', [0.5, 0.4, 3.4], [0, h + 0.3, 1.2], 'rail', 'gun'),
      V('box', [2.6, 0.25, 0.35], [-1.4, h + 0.35, 2.4], 'limb', 'limb_l', [0, 25, 0]),
      V('box', [2.6, 0.25, 0.35], [1.4, h + 0.35, 2.4], 'limb', 'limb_r', [0, -25, 0]),
      V('cylinder', [0.18, 3.2, 0.18], [0, h + 0.62, 1.2], 'bolt', 'gun', [90, 0, 0]),
      V('cone', [0.35, 0.6, 0.35], [0, h + 0.62, 3.0], 'bolt_tip', 'gun', [90, 0, 0]),
      muzzle([0, h + 0.3, 0.4], 'gun', 0.3), // at the pivot (see weapons.ts FIRE_FROM_PIVOT)
    ];
    return {
      parts,
      joints: [
        J('gun', [0, h + 0.3, 0], [aimBoth(100, -25, 40), recoil(0.3, 2)]),
        J('limb_l', [0, h + 0.35, 2.4], [snap(30, 3, [0, 1, 0])], 'gun'),
        J('limb_r', [0, h + 0.35, 2.4], [snap(30, 3, [0, -1, 0])], 'gun'),
      ],
    };
  },
  behaviors: [guardB(60), weaponB('gun', 'gun', 0.5, 90, 40, 60, { size: 0.3 })],
  functions: [...SIEGE_FNS.filter((f) => f !== 'area_attack'), 'guards_area'],
  verb: 'shoot',
};

/** Field cannon: a cannon on a wheeled carriage that rolls after the enemy and shells it; climb on to drive it. */
export const fieldCannon: CatalogEntry = {
  id: 'field_cannon',
  names: ['field cannon', 'wheeled cannon', 'cannon cart', 'siege cannon', 'cannon on wheels', 'artillery cart', 'field gun', 'rolling cannon', 'cannon wagon', 'field cannons', 'field guns', 'artillery piece', 'napoleon cannon', 'pirate cannon'],
  cls: 'vehicle', motion: 'kinematic', at: 'ground_front', lifetime: 120,
  build() {
    const parts: Part[] = [
      P('box', [1.6, 0.5, 3.2], [0, 1.0, -0.4], { role: 'carriage' }),
      V('box', [0.3, 0.3, 2.2], [0, 0.6, -2.4], 'trail'),
      ...wheel('axle', [-1.05, 0.9, 0.2], 1.8, 0.25),
      ...wheel('axle', [1.05, 0.9, 0.2], 1.8, 0.25),
      P('box', [1.0, 0.5, 1.0], [0, 1.5, 0.2], { role: 'mount', group: 'turret' }),
      P('cylinder', [0.7, 3.0, 0.7], [0, 1.9, 1.2], { role: 'barrel', rot: [90, 0, 0], group: 'barrel' }),
      V('torus', [0.9, 0.18, 0.9], [0, 1.9, 2.65], 'muzzle_ring', 'barrel', [90, 0, 0]),
      muzzle([0, 1.9, 2.85], 'barrel', 0.3),
    ];
    return {
      parts,
      joints: [
        J('axle', [0, 0.9, 0.2], [roll(0.9)]),
        J('turret', [0, 1.3, 0.2], [aimYaw(90)]),
        J('barrel', [0, 1.9, 0.2], [aimPitch(60, -10, 40), recoil(0.5, 2.5)], 'turret'),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 8, turn: 80 },
    huntB(55),
    weaponB('cannon', 'barrel', 0.6, 70, 35, 60, { size: 0.35 }),
    { type: 'rideable', seat: [0, 2.2, -1.2] },
  ],
  functions: [...SIEGE_FNS, 'moves_autonomously', 'drives_on_ground', 'wheels_spin', 'rideable', 'carries_player', 'pushes_objects'],
  verb: 'shoot',
};

/** Mortar: a stubby tube that lobs shells high over walls onto where they're running. */
export const mortar: CatalogEntry = {
  id: 'mortar',
  names: ['howitzer', 'mortar tube', 'mortar pit', 'siege mortar', 'mortar turret', 'artillery mortar', 'mortar cannon', 'morter', 'mortars', 'howitzers', 'mortar tubes'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 90,
  build() {
    const parts: Part[] = [
      P('cylinder', [2.0, 0.3, 2.0], [0, 0.15, 0], { role: 'baseplate' }),
      V('box', [0.15, 1.2, 0.15], [0.6, 0.6, 0.7], 'bipod', 'turret', [-20, 0, -15]),
      V('box', [0.15, 1.2, 0.15], [-0.6, 0.6, 0.7], 'bipod', 'turret', [-20, 0, 15]),
      P('cylinder', [0.8, 2.4, 0.8], [0, 1.3, 0.2], { role: 'tube', rot: [30, 0, 0], group: 'tube' }),
      V('torus', [1.0, 0.16, 1.0], [0, 2.3, 0.8], 'rim', 'tube', [30, 0, 0]),
      muzzle([0, 2.8, 0.95], 'tube', 0.35),
    ];
    return { parts, joints: [J('turret', [0, 0.3, 0], [aimYaw(120)]), J('tube', [0, 0.4, 0], [recoil(0.5, 2, [0, -1, 0])], 'turret')] };
  },
  behaviors: [guardB(60), weaponB('bomb', 'tube', 0.5, 28, 35, 60, { size: 0.6 })],
  functions: [...SIEGE_FNS, 'guards_area'],
  verb: 'shoot',
};

/** Siege tower: a rolling wooden tower with a crossbow on its roof; ride on top to get up high. */
export const siegeTower: CatalogEntry = {
  id: 'siege_tower',
  names: ['siege tower', 'belfry', 'war tower', 'siege towers', 'rolling tower', 'battle tower', 'seige tower', 'siege towr', 'war towers', 'battle towers', 'rolling towers', 'assault tower', 'mobile tower'],
  cls: 'vehicle', motion: 'kinematic', at: 'ground_front', lifetime: 120,
  build() {
    const parts: Part[] = [
      P('box', [3.2, 6.5, 3.2], [0, 3.85, 0], { role: 'body' }),
      P('box', [3.6, 0.3, 3.6], [0, 7.25, 0], { role: 'deck' }),
      ...[[-1.7, -1.7], [1.7, -1.7], [-1.7, 1.7], [1.7, 1.7]].map(([x, z]) => V('box', [0.3, 1.0, 0.3], [x, 7.9, z], 'crenel')),
      V('box', [3.0, 0.2, 3.0], [0, 6.0, 1.75], 'drawbridge', 'bridge'),
      ...wheel('axle_f', [-1.75, 0.6, 1.1], 1.2, 0.3), ...wheel('axle_f', [1.75, 0.6, 1.1], 1.2, 0.3),
      ...wheel('axle_r', [-1.75, 0.6, -1.1], 1.2, 0.3), ...wheel('axle_r', [1.75, 0.6, -1.1], 1.2, 0.3),
      P('box', [0.4, 0.4, 1.8], [0, 7.8, 0.6], { role: 'crossbow', group: 'gun' }),
      V('box', [1.6, 0.12, 0.2], [0, 7.85, 1.3], 'limb', 'gun'),
      muzzle([0, 7.9, 1.7], 'gun', 0.25),
    ];
    return {
      parts,
      joints: [
        J('axle_f', [0, 0.6, 1.1], [roll(0.6)]),
        J('axle_r', [0, 0.6, -1.1], [roll(0.6)]),
        J('bridge', [0, 6.0, 0.2], [swing(10, 0.5, [1, 0, 0])]),
        J('gun', [0, 7.8, 0.2], [aimBoth(160, -45, 45)]),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 6, turn: 50 },
    huntB(50),
    weaponB('gun', 'gun', 1.2, 80, 16, 50, { size: 0.2 }),
    { type: 'rideable', seat: [0, 8.3, -0.6] },
  ],
  functions: [...SIEGE_FNS.filter((f) => f !== 'area_attack'), 'moves_autonomously', 'drives_on_ground', 'wheels_spin', 'rideable', 'carries_player', 'raise_player', 'support_player', 'pushes_objects'],
  verb: 'shoot',
};

/** Battering ram: a roofed cart with a swinging log; charges the enemy and batters it. Rideable. */
export const batteringRam: CatalogEntry = {
  id: 'battering_ram',
  names: ['battering ram', 'battering-ram', 'ram cart', 'siege ram', 'battering ram cart', 'tortoise ram', 'batterin ram', 'batering ram', 'battering rams', 'siege rams', 'ram carts', 'log ram', 'door breaker'],
  cls: 'vehicle', motion: 'kinematic', at: 'ground_front', lifetime: 90,
  build() {
    const parts: Part[] = [
      P('box', [2.4, 0.4, 4.4], [0, 0.9, 0], { role: 'chassis' }),
      ...[[-1.1, -1.8], [1.1, -1.8], [-1.1, 1.8], [1.1, 1.8]].map(([x, z]) => V('box', [0.2, 2.2, 0.2], [x, 2.2, z], 'post')),
      P('wedge', [2.8, 0.9, 4.8], [0, 3.5, 0], { role: 'roof' }),
      V('box', [0.1, 1.2, 0.1], [0, 2.8, -1.0], 'chain', 'ram'),
      V('box', [0.1, 1.2, 0.1], [0, 2.8, 1.0], 'chain', 'ram'),
      P('cylinder', [0.7, 5.2, 0.7], [0, 2.1, 0.8], { role: 'log', rot: [90, 0, 0], group: 'ram' }),
      V('sphere', [1.0, 1.0, 1.0], [0, 2.1, 3.4], 'ram_head', 'ram'),
      muzzle([0, 2.1, 3.9], 'ram', 0.3),
      ...wheel('axle_f', [-1.35, 0.55, 1.5], 1.1, 0.3), ...wheel('axle_f', [1.35, 0.55, 1.5], 1.1, 0.3),
      ...wheel('axle_r', [-1.35, 0.55, -1.5], 1.1, 0.3), ...wheel('axle_r', [1.35, 0.55, -1.5], 1.1, 0.3),
    ];
    return {
      parts,
      joints: [
        J('axle_f', [0, 0.55, 1.5], [roll(0.55)]),
        J('axle_r', [0, 0.55, -1.5], [roll(0.55)]),
        J('ram', [0, 3.4, 0], [snap(35, 2, [1, 0, 0])]),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 16, turn: 90 },
    huntB(55),
    weaponB('melee', 'ram', 0.8, 0, 45, 2.5),
    { type: 'rideable', seat: [0, 4.4, -0.8] },
  ],
  functions: ['moves_autonomously', 'drives_on_ground', 'wheels_spin', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'strike_target', 'push_target', 'rideable', 'carries_player', 'pushes_objects', 'move_obstacle'],
  verb: 'strike',
};

/** Wrecking crane: the jib swings round toward the enemy while a wrecking ball pendulums along it. */
export const wreckingCrane: CatalogEntry = {
  id: 'wrecking_crane',
  names: ['wrecking crane', 'demolition crane', 'wrecking ball crane', 'crane with a wrecking ball', 'swinging wrecking ball', 'demolition ball crane', 'wrecker crane', 'wrecking tower', 'wrecking cranes', 'demolition cranes', 'wreckin crane'],
  cls: 'heavy', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const parts: Part[] = [
      P('box', [3.0, 0.6, 3.0], [0, 0.3, 0], { role: 'base' }),
      P('box', [1.0, 8.4, 1.0], [0, 4.8, 0], { role: 'mast' }),
      P('box', [0.8, 0.8, 9.0], [0, 9.2, 2.5], { role: 'jib', group: 'turret' }),
      V('box', [1.4, 1.4, 1.4], [0, 9.2, -2.4], 'counterweight', 'turret'),
      V('cylinder', [0.12, 5.4, 0.12], [0, 6.1, 6.2], 'cable', 'pend'),
      P('sphere', [2.4, 2.4, 2.4], [0, 2.4, 6.2], { role: 'ball', group: 'pend' }),
      muzzle([0, 2.4, 6.2], 'pend', 0.4),
    ];
    return { parts, joints: [J('turret', [0, 9.2, 0], [aimYaw(50)]), J('pend', [0, 8.9, 6.2], [swing(50, 0.45, [1, 0, 0])], 'turret')] };
  },
  behaviors: [guardB(12), weaponB('melee', 'pend', 1.5, 0, 45, 2.2)],
  functions: ['has_moving_parts', 'aims_at_target', 'guards_area', 'attacks_enemies', 'damages_target', 'strike_target', 'area_attack', 'sweep'],
  verb: 'strike',
};

/** Cannon tower: a fortified emplacement with a heavy cannon; exploding shells, long range. */
export const cannonTower: CatalogEntry = {
  id: 'cannon_tower',
  names: ['cannon tower', 'cannon emplacement', 'gun emplacement', 'bunker cannon', 'fortress cannon', 'cannon bunker', 'gun tower', 'artillery tower', 'cannon fort', 'bastion', 'cannon towers', 'big bertha', 'coastal gun'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 90,
  build() {
    const h = 3.0;
    const parts: Part[] = [
      P('cylinder', [3.6, h, 3.6], [0, h / 2, 0], { role: 'tower' }),
      ...Array.from({ length: 6 }, (_, i) => { const a = (i / 6) * Math.PI * 2; return V('box', [0.6, 0.6, 0.6], [Math.sin(a) * 1.6, h + 0.3, Math.cos(a) * 1.6], 'merlon'); }),
      P('cylinder', [1.6, 0.6, 1.6], [0, h + 0.3, 0], { role: 'mount', group: 'turret' }),
      V('cylinder', [0.8, 3.4, 0.8], [0, h + 0.8, 1.4], 'barrel', 'gun', [90, 0, 0]),
      V('torus', [1.0, 0.2, 1.0], [0, h + 0.8, 3.05], 'muzzle_ring', 'gun', [90, 0, 0]),
      muzzle([0, h + 0.8, 0.4], 'gun', 0.3), // at the pivot (see weapons.ts FIRE_FROM_PIVOT)
    ];
    return { parts, joints: [J('turret', [0, h, 0], [aimYaw(80)]), J('gun', [0, h + 0.8, 0], [aimPitch(60, -20, 45), recoil(0.6, 2)], 'turret')] };
  },
  behaviors: [guardB(60), weaponB('cannon', 'gun', 0.55, 75, 38, 60, { size: 0.4 })],
  functions: [...SIEGE_FNS, 'guards_area', 'destroys_target'],
  verb: 'shoot',
};

gunAtOrigin(ballista, 1.8);
gunAtOrigin(cannonTower, 3.8);

export const SIEGE: CatalogEntry[] = [trebuchet, ballista, fieldCannon, mortar, siegeTower, batteringRam, wreckingCrane, cannonTower];
