import type { CatalogEntry } from './types.js';
import type { Part } from '../spec.js';
import { P, J, wheel, roll, aimYaw, aimPitch, recoil } from './kit.js';

/** Treaded battle tank: rolling road wheels, turret that tracks hostiles, recoiling cannon. Hunts; rideable. */
export const tank: CatalogEntry = {
  id: 'tank',
  names: ['battle tank', 'army tank', 'panzer', 'tank destroyer', 'armored tank', 'armoured tank', 'apc', 'armored car', 'armoured car', 'war machine'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const parts: Part[] = [
      P('box', [3.2, 1.0, 5.0], [0, 1.1, 0], { role: 'body' }),
      P('wedge', [3.2, 1.0, 1.1], [0, 1.1, 3.05], { role: 'armor', rot: [0, 180, 0] }),
      P('box', [0.8, 1.1, 5.6], [-1.95, 0.6, 0], { role: 'tread' }),
      P('box', [0.8, 1.1, 5.6], [1.95, 0.6, 0], { role: 'tread' }),
    ];
    const axles = [-1.9, 0, 1.9];
    axles.forEach((z, i) => {
      for (const x of [-2.42, 2.42]) parts.push(...wheel(`axle${i}`, [x, 0.5, z], 0.9, 0.22));
    });
    // turret (yaws) and barrel (pitches + recoils)
    parts.push(
      P('cylinder', [2.3, 0.8, 2.3], [0, 2.0, -0.3], { role: 'turret', group: 'turret' }),
      P('dome', [0.9, 0.35, 0.9], [0, 2.4, -0.8], { role: 'hatch', group: 'turret', solid: false }),
      P('box', [0.9, 0.6, 0.5], [0, 2.05, 0.95], { role: 'mantlet', group: 'barrel' }),
      P('cylinder', [0.3, 3.0, 0.3], [0, 2.05, 2.5], { role: 'barrel', rot: [90, 0, 0], group: 'barrel' }),
      P('cylinder', [0.46, 0.5, 0.46], [0, 2.05, 4.0], { role: 'muzzle', rot: [90, 0, 0], group: 'barrel', solid: false }),
    );
    return {
      parts,
      joints: [
        ...axles.map((z, i) => J(`axle${i}`, [0, 0.5, z], [roll(0.45)])),
        J('turret', [0, 2.0, -0.3], [aimYaw(90)]),
        J('barrel', [0, 2.05, 0.8], [aimPitch(50, -8, 40), recoil(0.45, 2.5)], 'turret'),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 70 },
    { type: 'brain', goal: 'hunt', target: { kind: 'nearest_enemy' }, range: 45 },
    { type: 'weapon', kind: 'cannon', group: 'barrel', rate: 0.8, speed: 65, damage: 40, range: 50, size: 0.35 },
    { type: 'rideable', seat: [0, 3.1, -0.9] },
  ],
  functions: ['moves_autonomously', 'drives_on_ground', 'wheels_spin', 'has_moving_parts', 'aims_at_target', 'shoots', 'attacks_enemies', 'damages_target', 'rideable', 'carries_player', 'pushes_objects'],
  verb: 'shoot',
};
