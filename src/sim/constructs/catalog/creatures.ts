import type { CatalogEntry } from './types.js';
import type { Part, Joint, Vec3 } from '../spec.js';
import { P, J, limb, mirror, gait, swing, wave, snap, recoil, aimYaw, aimPitch, aimBoth, bob } from './kit.js';

/** Four-legged dog: trotting legs, wagging tail, head that watches, snapping jaw. Follows you and bites hostiles. */
export const dog: CatalogEntry = {
  id: 'dog',
  names: ['puppy', 'doggy', 'doggo', 'pup', 'hound', 'guard dog', 'wolf', 'hellhound', 'good boy', 'pooch', 'mutt'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  replaces: ['dog'],
  build() {
    const parts: Part[] = [
      P('capsule', [0.62, 1.35, 0.62], [0, 0.98, -0.05], { role: 'body', rot: [90, 0, 0] }),
      P('sphere', [0.72, 0.72, 0.8], [0, 1.02, 0.45], { role: 'chest' }),
      P('box', [0.5, 0.45, 0.5], [0, 1.42, 0.95], { role: 'head', group: 'head' }),
      P('box', [0.3, 0.2, 0.36], [0, 1.36, 1.3], { role: 'snout', group: 'head' }),
      P('cone', [0.15, 0.3, 0.12], [-0.17, 1.76, 0.88], { role: 'ear', group: 'head', solid: false }),
      P('cone', [0.15, 0.3, 0.12], [0.17, 1.76, 0.88], { role: 'ear', group: 'head', solid: false }),
      P('sphere', [0.08, 0.08, 0.05], [-0.13, 1.5, 1.2], { role: 'eye', group: 'head', solid: false }),
      P('sphere', [0.08, 0.08, 0.05], [0.13, 1.5, 1.2], { role: 'eye', group: 'head', solid: false }),
      P('box', [0.26, 0.08, 0.36], [0, 1.2, 1.28], { role: 'jaw', group: 'jaw', solid: false }),
      P('sphere', [0.12, 0.12, 0.12], [0, 1.24, 1.45], { role: 'muzzle', group: 'jaw', solid: false }),
      P('cylinder', [0.1, 0.7, 0.1], [0, 1.37, -0.94], { role: 'tail', rot: [-40, 0, 0], group: 'tail', solid: false }),
    ];
    const legs: [string, Vec3, number][] = [['leg_fl', [-0.2, 0.82, 0.45], 0], ['leg_fr', [0.2, 0.82, 0.45], 0.5], ['leg_bl', [-0.2, 0.82, -0.5], 0.5], ['leg_br', [0.2, 0.82, -0.5], 0]];
    const joints: Joint[] = [
      J('head', [0, 1.22, 0.75], [aimYaw(220)]),
      J('jaw', [0, 1.24, 1.08], [snap(30, 4)], 'head'),
      J('tail', [0, 1.1, -0.72], [swing(35, 3, [0, 1, 0])]),
    ];
    for (const [g, hip, ph] of legs) {
      parts.push(P('capsule', [0.17, 0.84, 0.17], [hip[0], 0.42, hip[2]], { role: 'leg', group: g, solid: false }));
      parts.push(P('box', [0.2, 0.08, 0.26], [hip[0], 0.04, hip[2] + 0.05], { role: 'paw', group: g, solid: false }));
      joints.push(J(g, hip, [gait(32, 2.4, ph)]));
    }
    return { parts, joints };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 16, turn: 300, jump: 8 },
    { type: 'brain', goal: 'follow', target: { kind: 'self' }, range: 18 },
    { type: 'weapon', kind: 'melee', group: 'jaw', rate: 1.6, speed: 0, damage: 22, range: 1.6 },
  ],
  functions: ['walks', 'drives_on_ground', 'has_moving_parts', 'follows_player', 'attacks_enemies', 'damages_target', 'returns'],
  verb: 'strike',
};

/** Dragon: flapping wings, rippling three-segment tail, head that aims its flame. Escorts you; ride it. */
export const dragon: CatalogEntry = {
  id: 'dragon',
  names: ['wyvern', 'drake', 'fire dragon', 'dragon mount', 'phoenix', 'griffin', 'gryphon', 'pegasus', 'pterodactyl', 'eagle ride'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  replaces: ['dragon'],
  build() {
    const parts: Part[] = [
      P('capsule', [1.8, 4.6, 1.9], [0, 2.3, 0], { role: 'body', rot: [90, 0, 0] }),
      P('cylinder', [0.8, 1.9, 0.8], [0, 3.37, 2.57], { role: 'neck', rot: [45, 0, 0], group: 'neck' }),
      P('box', [0.9, 0.75, 1.1], [0, 4.05, 3.5], { role: 'head', group: 'head' }),
      P('box', [0.6, 0.4, 0.9], [0, 3.95, 4.35], { role: 'snout', group: 'head' }),
      P('cone', [0.18, 0.7, 0.18], [-0.3, 4.55, 3.2], { role: 'horn', rot: [-30, 0, 0], group: 'head', solid: false }),
      P('cone', [0.18, 0.7, 0.18], [0.3, 4.55, 3.2], { role: 'horn', rot: [-30, 0, 0], group: 'head', solid: false }),
      P('sphere', [0.22, 0.22, 0.22], [0, 3.9, 4.85], { role: 'muzzle', group: 'head', solid: false }),
      P('box', [0.55, 0.14, 0.9], [0, 3.62, 4.2], { role: 'jaw', group: 'jaw', solid: false }),
      P('cylinder', [0.7, 1.5, 0.7], [0, 2.1, -2.9], { role: 'tail', rot: [90, 0, 0], group: 'tail1' }),
      P('cylinder', [0.5, 1.5, 0.5], [0, 2.0, -4.3], { role: 'tail', rot: [90, 0, 0], group: 'tail2', solid: false }),
      P('cone', [0.4, 1.6, 0.4], [0, 1.95, -5.8], { role: 'tail', rot: [-90, 0, 0], group: 'tail3', solid: false }),
      P('wedge', [0.8, 0.1, 0.9], [0, 1.95, -6.6], { role: 'spade', group: 'tail3', solid: false }),
      P('capsule', [0.45, 1.3, 0.45], [-0.7, 1.0, -1.1], { role: 'leg', rot: [25, 0, 0], solid: false }),
      P('capsule', [0.45, 1.3, 0.45], [0.7, 1.0, -1.1], { role: 'leg', rot: [25, 0, 0], solid: false }),
      P('capsule', [0.35, 1.1, 0.35], [-0.6, 1.2, 1.2], { role: 'leg', rot: [-20, 0, 0], solid: false }),
      P('capsule', [0.35, 1.1, 0.35], [0.6, 1.2, 1.2], { role: 'leg', rot: [-20, 0, 0], solid: false }),
    ];
    const wingL: Part[] = [
      P('box', [4.8, 0.1, 2.4], [-3.1, 3.0, -0.1], { role: 'wing', group: 'wing_l', solid: false }),
      P('cylinder', [0.18, 4.8, 0.18], [-3.1, 3.05, 1.1], { role: 'spar', rot: [0, 0, 90], group: 'wing_l', solid: false }),
      P('cone', [0.2, 0.9, 0.2], [-5.9, 3.05, 1.1], { role: 'claw', rot: [0, 0, 90], group: 'wing_l', solid: false }),
    ];
    parts.push(...wingL, ...wingL.map((p) => mirror(p, () => 'wing_r')));
    return {
      parts,
      joints: [
        J('neck', [0, 2.7, 1.9], [swing(5, 0.5, [1, 0, 0])]),
        J('head', [0, 3.95, 3.15], [aimBoth(110, -55, 30)], 'neck'),
        J('jaw', [0, 3.72, 3.75], [snap(25, 3)], 'head'),
        J('wing_l', [-0.7, 3.0, 0.3], [swing(38, 1.1, [0, 0, -1])]),
        J('wing_r', [0.7, 3.0, 0.3], [swing(38, 1.1, [0, 0, 1])]),
        J('tail1', [0, 2.2, -2.2], [wave(14, 0.7)]),
        J('tail2', [0, 2.05, -3.6], [wave(18, 0.7)], 'tail1'),
        J('tail3', [0, 1.95, -5.0], [wave(22, 0.7)], 'tail2'),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'air', speed: 22, turn: 90, altitude: 9 },
    { type: 'brain', goal: 'follow', target: { kind: 'self' }, range: 35 },
    { type: 'weapon', kind: 'flame', group: 'head', rate: 10, speed: 0, damage: 5, range: 16 },
    { type: 'rideable', seat: [0, 3.75, 0.2] },
  ],
  functions: ['flies', 'moves_autonomously', 'has_moving_parts', 'aims_at_target', 'shoots', 'attacks_enemies', 'damages_target', 'follows_player', 'rideable', 'carries_player', 'propel_player', 'slow_fall'],
};

/** Eight-legged spider: alternating tetrapod gait, snapping fangs, web shots that stun. Hunts. */
export const spider: CatalogEntry = {
  id: 'spider',
  names: ['giant spider', 'tarantula', 'arachnid', 'spider bot', 'spiderbot', 'scorpion', 'crab', 'giant crab', 'black widow'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const parts: Part[] = [
      P('sphere', [1.7, 1.3, 2.0], [0, 1.45, -1.05], { role: 'abdomen' }),
      P('sphere', [1.2, 0.9, 1.3], [0, 1.25, 0.45], { role: 'body' }),
      ...[-0.22, 0.22].flatMap((x) => [1.45, 1.6].map((y) => P('sphere', [0.14, 0.14, 0.1], [x * (y > 1.5 ? 0.6 : 1), y, 1.08], { role: 'eye', solid: false }))),
      P('cone', [0.14, 0.45, 0.14], [-0.16, 0.9, 1.12], { role: 'fang', rot: [180, 0, 0], group: 'fangs', solid: false }),
      P('cone', [0.14, 0.45, 0.14], [0.16, 0.9, 1.12], { role: 'fang', rot: [180, 0, 0], group: 'fangs', solid: false }),
      P('sphere', [0.14, 0.14, 0.14], [0, 0.95, 1.25], { role: 'muzzle', group: 'fangs', solid: false }),
    ];
    const joints: Joint[] = [J('fangs', [0, 1.05, 1.05], [snap(28, 4)])];
    const hipsZ = [0.8, 0.4, 0.0, -0.4];
    const footZ = [2.1, 0.9, -0.5, -1.8];
    hipsZ.forEach((hz, i) => {
      for (const side of [-1, 1]) {
        const g = `leg_${side < 0 ? 'l' : 'r'}${i}`;
        const hip: Vec3 = [side * 0.5, 1.25, hz];
        const knee: Vec3 = [side * 1.35, 2.0, (hz + footZ[i]) / 2];
        const foot: Vec3 = [side * 2.2, 0.02, footZ[i]];
        parts.push(limb('capsule', hip, knee, 0.2, { role: 'leg', group: g, solid: false }));
        parts.push(limb('cone', knee, foot, 0.2, { role: 'leg', group: g, solid: false }));
        const ph = (i + (side < 0 ? 0 : 1)) % 2 ? 0.5 : 0;
        joints.push(J(g, hip, [gait(18, 2.2, ph, [0, 1, 0]), bob(0.18, 2.2, ph + 0.25)]));
      }
    });
    return { parts, joints };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 10, turn: 200, jump: 10 },
    { type: 'brain', goal: 'hunt', target: { kind: 'nearest_enemy' }, range: 40 },
    { type: 'weapon', kind: 'net', group: 'fangs', rate: 0.6, speed: 40, damage: 8, range: 28, size: 0.45 },
    { type: 'weapon', kind: 'melee', group: 'fangs', rate: 2, speed: 0, damage: 18, range: 2.2 },
  ],
  functions: ['walks', 'drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'shoots', 'attacks_enemies', 'damages_target', 'restrain_target'],
  verb: 'strike',
};

/** Little robot buddy: walking legs, upper body that turns to its target, arm blaster with recoil. Follows and defends you. */
export const robot: CatalogEntry = {
  id: 'robot',
  names: ['robot buddy', 'robot friend', 'robot companion', 'robot sidekick', 'droid', 'android', 'bot', 'sidekick', 'helper robot', 'battle droid', 'robot soldier', 'cyborg', 'terminator'],
  cls: 'creature',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const legL: Part[] = [
      P('capsule', [0.22, 0.85, 0.22], [-0.22, 0.46, 0], { role: 'leg', group: 'leg_l', solid: false }),
      P('box', [0.26, 0.1, 0.4], [-0.22, 0.05, 0.06], { role: 'foot', group: 'leg_l', solid: false }),
    ];
    const parts: Part[] = [
      ...legL, ...legL.map((p) => mirror(p, () => 'leg_r')),
      P('box', [0.6, 0.22, 0.36], [0, 0.95, 0], { role: 'pelvis' }),
      P('box', [0.8, 0.7, 0.5], [0, 1.42, 0], { role: 'body', group: 'torso' }),
      P('box', [0.45, 0.38, 0.42], [0, 2.0, 0], { role: 'head', group: 'torso' }),
      P('box', [0.34, 0.08, 0.05], [0, 2.03, 0.22], { role: 'visor', group: 'torso', solid: false }),
      P('cylinder', [0.04, 0.35, 0.04], [0.12, 2.35, 0], { role: 'antenna', group: 'torso', solid: false }),
      P('capsule', [0.18, 0.68, 0.18], [-0.52, 1.36, 0], { role: 'arm', group: 'arm_l', solid: false }),
      P('capsule', [0.18, 0.55, 0.18], [0.52, 1.66, 0.25], { role: 'arm', rot: [90, 0, 0], group: 'arm_r', solid: false }),
      P('box', [0.17, 0.22, 0.52], [0.52, 1.66, 0.68], { role: 'blaster', group: 'arm_r', solid: false }),
      P('sphere', [0.11, 0.11, 0.11], [0.52, 1.66, 0.98], { role: 'muzzle', group: 'arm_r', solid: false }),
    ];
    return {
      parts,
      joints: [
        J('leg_l', [-0.22, 0.88, 0], [gait(28, 1.8, 0)]),
        J('leg_r', [0.22, 0.88, 0], [gait(28, 1.8, 0.5)]),
        J('torso', [0, 1.05, 0], [aimYaw(220), bob(0.04, 3.6)]),
        J('arm_l', [-0.52, 1.68, 0], [gait(25, 1.8, 0.5)], 'torso'),
        J('arm_r', [0.52, 1.66, 0], [aimPitch(160, -40, 65), recoil(0.12, 6)], 'torso'),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 9, turn: 260, jump: 7 },
    { type: 'brain', goal: 'follow', target: { kind: 'self' }, range: 30 },
    { type: 'weapon', kind: 'gun', group: 'torso', rate: 3, speed: 65, damage: 9, range: 35, size: 0.15 },
  ],
  functions: ['walks', 'drives_on_ground', 'has_moving_parts', 'aims_at_target', 'follows_player', 'shoots', 'attacks_enemies', 'damages_target', 'returns'],
  verb: 'shoot',
};

/** Walking mech: two stomping legs, torso that turns to its target, twin recoiling arm cannons. Hunts; climb in. */
export const mech: CatalogEntry = {
  id: 'mech',
  names: ['giant robot', 'battle mech', 'mecha', 'gundam', 'walker', 'robot walker', 'war mech', 'jaeger', 'titan', 'golem'],
  cls: 'vehicle',
  motion: 'kinematic',
  at: 'ground_front',
  lifetime: 150,
  build() {
    const legL: Part[] = [
      P('capsule', [0.55, 1.5, 0.55], [-0.8, 2.0, 0], { role: 'leg', group: 'leg_l', solid: false }),
      P('box', [0.5, 1.4, 0.6], [-0.8, 0.85, 0.1], { role: 'leg', group: 'leg_l', solid: false }),
      P('box', [0.8, 0.25, 1.3], [-0.8, 0.12, 0.2], { role: 'foot', group: 'leg_l', solid: false }),
    ];
    const armL: Part[] = [
      P('sphere', [0.8, 0.8, 0.8], [-1.65, 4.2, 0], { role: 'shoulder', group: 'arm_l' }),
      P('cylinder', [0.42, 2.0, 0.42], [-1.65, 3.95, 0.9], { role: 'cannon', rot: [90, 0, 0], group: 'arm_l' }),
      P('cylinder', [0.56, 0.4, 0.56], [-1.65, 3.95, 2.0], { role: 'muzzle', rot: [90, 0, 0], group: 'arm_l', solid: false }),
    ];
    const parts: Part[] = [
      ...legL, ...legL.map((p) => mirror(p, () => 'leg_r')),
      P('box', [2.0, 0.5, 1.0], [0, 2.7, 0], { role: 'hips' }),
      P('box', [2.6, 1.6, 1.8], [0, 3.8, 0], { role: 'body', group: 'torso' }),
      P('wedge', [1.4, 0.6, 0.6], [0, 3.9, 1.15], { role: 'armor', rot: [0, 180, 0], group: 'torso', solid: false }),
      P('dome', [1.3, 0.7, 1.3], [0, 4.6, 0.1], { role: 'cockpit', group: 'torso', solid: false }),
      ...armL, ...armL.map((p) => mirror(p, () => 'arm_r')),
    ];
    return {
      parts,
      joints: [
        J('leg_l', [-0.8, 2.65, 0], [gait(26, 0.9, 0)]),
        J('leg_r', [0.8, 2.65, 0], [gait(26, 0.9, 0.5)]),
        J('torso', [0, 2.9, 0], [aimYaw(80), bob(0.12, 1.8)]),
        J('arm_l', [-1.65, 4.2, 0], [aimPitch(60, -15, 45), recoil(0.4, 3)], 'torso'),
        J('arm_r', [1.65, 4.2, 0], [aimPitch(60, -15, 45), recoil(0.4, 3)], 'torso'),
      ],
    };
  },
  behaviors: [
    { type: 'drive', mode: 'ground', speed: 7, turn: 70, jump: 9 },
    { type: 'brain', goal: 'hunt', target: { kind: 'nearest_enemy' }, range: 45 },
    { type: 'weapon', kind: 'cannon', group: 'torso', rate: 1.4, speed: 60, damage: 28, range: 45, size: 0.3 },
    { type: 'rideable', seat: [0, 5.25, 0.05] },
  ],
  functions: ['walks', 'drives_on_ground', 'moves_autonomously', 'has_moving_parts', 'aims_at_target', 'shoots', 'attacks_enemies', 'damages_target', 'rideable', 'carries_player', 'pushes_objects'],
  verb: 'shoot',
};
