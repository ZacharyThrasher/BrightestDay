import type { CatalogEntry } from './types.js';
import type { Part } from '../spec.js';
import { P, J, wheel, roll, steer, spin, aimBoth } from './kit.js';

/** Open-top sports car: four rolling wheels, steering front wheels. You're put in the driver's seat. */
export const car: CatalogEntry = {
  id: 'car',
  names: ['race car', 'racecar', 'sports car', 'hot rod', 'go kart', 'go-kart', 'kart', 'buggy', 'dune buggy', 'jeep', 'truck', 'pickup truck', 'monster truck', 'van', 'taxi', 'convertible', 'automobile', 'roadster'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  replaces: ['car'],
  build() {
    const parts: Part[] = [
      P('box', [2.0, 0.55, 4.3], [0, 0.72, 0], { role: 'body' }),
      P('wedge', [1.8, 0.5, 0.6], [0, 1.25, 0.6], { role: 'windshield', rot: [0, 180, 0], solid: false }),
      P('box', [1.8, 0.4, 1.2], [0, 1.2, -1.0], { role: 'deck' }),
      P('box', [1.7, 0.1, 0.1], [0, 1.8, -0.55], { role: 'rollbar', solid: false }),
      P('box', [0.1, 0.6, 0.1], [-0.8, 1.5, -0.55], { role: 'rollbar', solid: false }),
      P('box', [0.1, 0.6, 0.1], [0.8, 1.5, -0.55], { role: 'rollbar', solid: false }),
      P('box', [2.0, 0.08, 0.45], [0, 1.55, -2.0], { role: 'spoiler', solid: false }),
      P('sphere', [0.28, 0.2, 0.1], [-0.65, 0.85, 2.16], { role: 'light', solid: false }),
      P('sphere', [0.28, 0.2, 0.1], [0.65, 0.85, 2.16], { role: 'light', solid: false }),
      ...wheel('wheel_fl', [-1.08, 0.42, 1.35], 0.84, 0.32),
      ...wheel('wheel_fr', [1.08, 0.42, 1.35], 0.84, 0.32),
      ...wheel('axle_r', [-1.08, 0.42, -1.35], 0.84, 0.36),
      ...wheel('axle_r', [1.08, 0.42, -1.35], 0.84, 0.36),
    ];
    return {
      parts,
      joints: [
        J('wheel_fl', [-1.08, 0.42, 1.35], [steer(30), roll(0.42)]),
        J('wheel_fr', [1.08, 0.42, 1.35], [steer(30), roll(0.42)]),
        J('axle_r', [0, 0.42, -1.35], [roll(0.42)]),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 28, turn: 120, jump: 7 },
    { type: 'rideable', seat: [0, 1.45, -0.4] },
  ],
  functions: ['drives_on_ground', 'wheels_spin', 'has_moving_parts', 'rideable', 'carries_player', 'propel_player', 'pushes_objects'],
};

/** Motorbike: steering fork, two rolling wheels, leans into turns. Very fast. */
export const motorcycle: CatalogEntry = {
  id: 'motorcycle',
  names: ['motorbike', 'bike', 'dirt bike', 'scooter', 'moped', 'superbike', 'racing bike', 'speeder bike', 'bicycle', 'harley'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  build() {
    const parts: Part[] = [
      P('capsule', [0.4, 1.9, 0.4], [0, 0.8, -0.05], { role: 'body', rot: [90, 0, 0] }),
      P('sphere', [0.55, 0.42, 0.85], [0, 1.08, 0.35], { role: 'tank' }),
      P('box', [0.36, 0.12, 0.8], [0, 1.02, -0.4], { role: 'seat' }),
      P('cylinder', [0.14, 0.9, 0.14], [0.25, 0.6, -0.7], { role: 'exhaust', rot: [80, 0, 0], solid: false }),
      P('box', [0.9, 0.07, 0.07], [0, 1.38, 0.8], { role: 'handlebar', group: 'fork', solid: false }),
      P('box', [0.07, 0.95, 0.07], [-0.13, 0.9, 0.92], { role: 'fork', rot: [-18, 0, 0], group: 'fork', solid: false }),
      P('box', [0.07, 0.95, 0.07], [0.13, 0.9, 0.92], { role: 'fork', rot: [-18, 0, 0], group: 'fork', solid: false }),
      P('sphere', [0.25, 0.22, 0.12], [0, 1.2, 1.0], { role: 'light', group: 'fork', solid: false }),
      ...wheel('wheel_f', [0, 0.38, 1.08], 0.76, 0.16),
      ...wheel('wheel_r', [0, 0.38, -0.88], 0.76, 0.2),
    ];
    return {
      parts,
      joints: [
        J('fork', [0, 1.0, 0.95], [steer(25)]),
        J('wheel_f', [0, 0.38, 1.08], [roll(0.38)], 'fork'),
        J('wheel_r', [0, 0.38, -0.88], [roll(0.38)]),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 38, turn: 130, jump: 8 },
    { type: 'rideable', seat: [0, 1.6, -0.3] },
  ],
  functions: ['drives_on_ground', 'wheels_spin', 'has_moving_parts', 'rideable', 'carries_player', 'propel_player'],
};

/** Gunship: spinning main + tail rotor, chin gun that tracks targets. Escorts you; get in to fly it. */
export const helicopter: CatalogEntry = {
  id: 'helicopter',
  names: ['chopper', 'gunship', 'heli', 'copter', 'helicopter gunship', 'huey', 'apache', 'quadcopter', 'gyrocopter'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  replaces: ['helicopter'],
  build() {
    const parts: Part[] = [
      P('sphere', [2.4, 2.0, 3.2], [0, 1.9, 0.3], { role: 'cabin' }),
      P('cylinder', [0.4, 3.8, 0.4], [0, 2.2, -3.2], { role: 'tail', rot: [90, 0, 0] }),
      P('box', [0.1, 1.1, 0.8], [0, 2.7, -5.0], { role: 'fin' }),
      P('box', [0.14, 0.12, 3.2], [-0.95, 0.1, 0.3], { role: 'skid' }),
      P('box', [0.14, 0.12, 3.2], [0.95, 0.1, 0.3], { role: 'skid' }),
      ...[-0.95, 0.95].flatMap((x) => [-0.5, 1.1].map((z) => P('box', [0.1, 0.9, 0.1], [x, 0.55, z], { role: 'strut', solid: false }))),
      P('cylinder', [0.22, 0.5, 0.22], [0, 3.05, 0.2], { role: 'mast', solid: false }),
      P('box', [7.6, 0.06, 0.38], [0, 3.35, 0.2], { role: 'rotor', group: 'rotor', solid: false }),
      P('box', [0.38, 0.06, 7.6], [0, 3.35, 0.2], { role: 'rotor', group: 'rotor', solid: false }),
      P('sphere', [0.45, 0.25, 0.45], [0, 3.37, 0.2], { role: 'hub', group: 'rotor', solid: false }),
      P('box', [0.06, 1.6, 0.22], [0.28, 2.4, -5.15], { role: 'rotor', group: 'tail_rotor', solid: false }),
      P('box', [0.06, 0.22, 1.6], [0.28, 2.4, -5.15], { role: 'rotor', group: 'tail_rotor', solid: false }),
      P('sphere', [0.55, 0.55, 0.55], [0, 0.95, 1.65], { role: 'gun', group: 'gun', solid: false }),
      P('cylinder', [0.13, 0.9, 0.13], [0, 0.95, 2.15], { role: 'barrel', rot: [90, 0, 0], group: 'gun', solid: false }),
      P('sphere', [0.18, 0.18, 0.18], [0, 0.95, 2.62], { role: 'muzzle', group: 'gun', solid: false }),
    ];
    return {
      parts,
      joints: [
        J('rotor', [0, 3.35, 0.2], [spin(4.5, [0, 1, 0])]),
        J('tail_rotor', [0.28, 2.4, -5.15], [spin(7, [1, 0, 0])]),
        J('gun', [0, 0.95, 1.65], [aimBoth(160, -70, 20)]),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 22, turn: 90, altitude: 9 },
    { type: 'brain', goal: 'follow', target: { kind: 'self' }, range: 40 },
    { type: 'weapon', kind: 'gun', group: 'gun', rate: 5, speed: 75, damage: 8, range: 45, size: 0.18 },
    { type: 'rideable', seat: [0, 2.0, 0.5] },
  ],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'aims_at_target', 'shoots', 'attacks_enemies', 'damages_target', 'follows_player', 'rideable', 'carries_player', 'propel_player'],
};

/** Propeller plane: spinning prop, wing guns, rolling landing gear. Always moving forward in the air. */
export const airplane: CatalogEntry = {
  id: 'airplane',
  names: ['plane', 'aeroplane', 'jet', 'fighter jet', 'fighter plane', 'warplane', 'biplane', 'propeller plane', 'prop plane', 'spitfire', 'aircraft', 'airliner', 'spaceship', 'space ship', 'starship', 'shuttle', 'space shuttle'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'self',
  lifetime: 150,
  replaces: ['plane'],
  build() {
    const parts: Part[] = [
      P('capsule', [1.1, 6.0, 1.1], [0, 1.4, 0], { role: 'fuselage', rot: [90, 0, 0] }),
      P('box', [9.0, 0.14, 1.6], [0, 1.2, 0.4], { role: 'wing' }),
      P('box', [3.0, 0.1, 0.9], [0, 1.6, -2.6], { role: 'tail' }),
      P('box', [0.1, 1.3, 1.0], [0, 2.2, -2.6], { role: 'fin' }),
      P('dome', [0.8, 0.5, 1.3], [0, 1.85, 0.5], { role: 'canopy', solid: false }),
      P('sphere', [0.14, 0.14, 0.14], [-2.2, 1.2, 1.3], { role: 'muzzle', solid: false }),
      P('sphere', [0.14, 0.14, 0.14], [2.2, 1.2, 1.3], { role: 'muzzle', solid: false }),
      P('box', [0.1, 0.75, 0.1], [-1.0, 0.72, 0.8], { role: 'strut', solid: false }),
      P('box', [0.1, 0.75, 0.1], [1.0, 0.72, 0.8], { role: 'strut', solid: false }),
      P('sphere', [0.3, 0.3, 0.3], [0, 0.95, -2.7], { role: 'tailwheel', solid: false }),
      P('cone', [0.45, 0.5, 0.45], [0, 1.4, 3.2], { role: 'spinner', rot: [90, 0, 0], group: 'prop', solid: false }),
      P('box', [0.16, 2.5, 0.06], [0, 1.4, 3.1], { role: 'propeller', group: 'prop', solid: false }),
      P('box', [2.5, 0.16, 0.06], [0, 1.4, 3.1], { role: 'propeller', group: 'prop', solid: false }),
      ...wheel('gear', [-1.0, 0.33, 0.8], 0.66, 0.2),
      ...wheel('gear', [1.0, 0.33, 0.8], 0.66, 0.2),
    ];
    return {
      parts,
      joints: [
        J('prop', [0, 1.4, 3.1], [spin(8, [0, 0, 1])]),
        J('gear', [0, 0.33, 0.8], [roll(0.33)]),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 40, turn: 70, altitude: 14, minSpeed: 14 },
    { type: 'weapon', kind: 'gun', rate: 6, speed: 90, damage: 8, range: 55, size: 0.15 },
    { type: 'rideable', seat: [0, 2.1, 0.45] },
  ],
  functions: ['flies', 'has_moving_parts', 'rideable', 'carries_player', 'propel_player', 'shoots'],
};
