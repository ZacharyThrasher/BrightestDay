/**
 * Combat catalog, part 2: TRAPS and AREA denial — they don't need to aim at a point, they own a patch of ground.
 *
 * Most are stationary machines with a short-range 'guard' brain: when an enemy comes within reach they fire
 * (a bite, a net, a slam, a mine) and an on-fire joint plays the snap. Walk-over parts are visual only (you can step
 * into a trap). Placement: on the ground below the crosshair / the target ('under_target'), or between you and
 * the target for the walls. Damage goes through the machine damage path, so traps never hurt their owner.
 */
import type { CatalogEntry } from './types.js';
import type { Part, Joint } from '../spec.js';
import { P, J, spin, swing, snap, aimYaw, bob } from './kit.js';
import { V, muzzle, guardB, weaponB, ring } from './weapons.js';

const TRAP_FNS = ['has_moving_parts', 'guards_area', 'attacks_enemies', 'damages_target', 'area_attack'];
/** A small jolt on the parent joint, so every child snap/slam plays when the trap fires. */
const jolt = { kind: 'piston' as const, axis: [0, 1, 0] as [number, number, number], amp: 0.06, rate: 4, on: 'fire' as const };

/**
 * An invisible aiming node for floor traps: shots from a floor trap go straight up into whoever stands on it, which
 * the "is it facing the target" check only allows through an aim joint. Its parent carries the on-fire snap.
 */
const seeker = (pivot: [number, number, number], parent: string) => J('seek', pivot, [{ kind: 'aim', yaw: true, pitch: true, rate: 720, minPitch: -89, maxPitch: 89 }], parent);

/** Bear trap: step in and the jaws slam shut — pinned by a net of light for a few seconds, and it bites. */
export const bearTrap: CatalogEntry = {
  id: 'bear_trap',
  names: ['bear trap', 'beartrap', 'bear-trap', 'leg trap', 'jaw trap', 'foothold trap', 'steel trap', 'snare', 'snare trap', 'mantrap', 'bear traps', 'bare trap', 'man trap'],
  cls: 'enclosure', motion: 'static', at: 'under_target', scale: 1.3, lifetime: 60,
  build() {
    const parts: Part[] = [
      V('cylinder', [2.2, 0.12, 2.2], [0, 0.06, 0], 'plate', 'trap'),
      V('cylinder', [0.8, 0.1, 0.8], [0, 0.15, 0], 'pad', 'trap'),
      muzzle([0, 0.6, 0.3], 'seek', 0.3),
    ];
    for (const s of [-1, 1]) {
      const g = s < 0 ? 'jaw_l' : 'jaw_r';
      parts.push(V('box', [0.12, 0.12, 2.0], [s * 1.0, 0.12, 0], 'jaw', g));
      for (const z of [-0.7, -0.35, 0, 0.35, 0.7]) parts.push(V('cone', [0.14, 0.35, 0.14], [s * 1.0, 0.3, z], 'tooth', g));
    }
    const joints: Joint[] = [
      J('trap', [0, 0, 0], [jolt]),
      seeker([0, 0.6, 0], 'trap'),
      J('jaw_l', [-1.0, 0.12, 0], [swing(3, 1.2, [0, 0, -1]), snap(80, 3, [0, 0, -1])], 'trap'),
      J('jaw_r', [1.0, 0.12, 0], [swing(3, 1.2, [0, 0, 1]), snap(80, 3, [0, 0, 1])], 'trap'),
    ];
    return { parts, joints };
  },
  behaviors: [guardB(3), weaponB('net', 'seek', 0.5, 30, 6, 2.6, { size: 0.5 }), weaponB('melee', 'trap', 1, 0, 25, 1.6)],
  functions: ['has_moving_parts', 'guards_area', 'attacks_enemies', 'damages_target', 'restrain_target'],
  verb: 'trap',
};

/** Spike floor: a plate that looks harmless until spikes shoot up through whoever stands on it. */
export const spikeFloor: CatalogEntry = {
  id: 'spike_floor',
  names: ['spike floor', 'spiked floor', 'spike plate', 'impaler', 'spike field', 'floor spikes', 'spikey floor', 'popup spikes', 'spike floors', 'spiked floors', 'spikefloor', 'spike pit', 'pit of spikes', 'punji pit', 'punji sticks', 'pop-up spikes', 'pop up spikes'],
  cls: 'weapon', motion: 'static', at: 'under_target', scale: 1.1, lifetime: 60,
  build() {
    const parts: Part[] = [V('box', [5, 0.12, 5], [0, 0.06, 0], 'plate'), muzzle([0, 0.7, 0], 'spikes', 0.3)];
    for (const x of [-1.8, -0.6, 0.6, 1.8]) for (const z of [-1.8, -0.6, 0.6, 1.8]) parts.push(V('cone', [0.35, 1.3, 0.35], [x, 0.1, z], 'spike', 'spikes'));
    return { parts, joints: [J('spikes', [0, 0, 0], [bob(0.05, 0.8, 0, false), { kind: 'piston', axis: [0, 1, 0], amp: 0.9, rate: 3, on: 'fire' }])] };
  },
  behaviors: [guardB(4.5), weaponB('melee', 'spikes', 1.2, 0, 28, 3)],
  functions: [...TRAP_FNS, 'strike_target'],
  verb: 'trap',
};

/** Minefield: a patch of buried mines — step on it and one pops up and blows. */
export const minefield: CatalogEntry = {
  id: 'minefield',
  names: ['landmine', 'landmines', 'mine field', 'minefeild', 'bouncing betty', 'tripmine', 'claymore mine', 'mines field', 'minefields', 'landmien', 'mine fields', 'land mine', 'land mines', 'trip mine', 'proximity mine', 'proximity mines', 'mine patch'],
  cls: 'weapon', motion: 'static', at: 'under_target', lifetime: 75,
  build() {
    const parts: Part[] = [V('cylinder', [7, 0.04, 7], [0, 0.02, 0], 'field'), muzzle([0, 0.4, 0.3], 'seek', 0.3)];
    for (const [, x, z] of [[0, 0, 0] as [number, number, number], ...ring(6, 2.4)]) {
      parts.push(V('cylinder', [0.8, 0.18, 0.8], [x, 0.09, z], 'mine', 'mines'));
      parts.push(V('sphere', [0.2, 0.2, 0.2], [x, 0.22, z], 'blinker', 'blink'));
    }
    return { parts, joints: [J('mines', [0, 0, 0], [{ kind: 'piston', axis: [0, 1, 0], amp: 0.4, rate: 3, on: 'fire' }]), J('blink', [0, 0, 0], [bob(0.05, 2, 0, false)], 'mines'), seeker([0, 0.4, 0], 'mines')] };
  },
  behaviors: [guardB(5), weaponB('bomb', 'seek', 0.8, 20, 40, 4.5, { size: 0.5 })],
  functions: [...TRAP_FNS, 'push_target'],
  verb: 'trap',
};

/** Tar pit: a bubbling pool of sticky light; anything wading in gets glued down (net) and singed. */
export const tarPit: CatalogEntry = {
  id: 'tar_pit',
  names: ['tar pit', 'tar trap', 'glue trap', 'sticky trap', 'glue pad', 'sticky pad', 'sticky floor', 'slime pit', 'quicksand', 'tarpit', 'tar pits', 'quick sand', 'molasses', 'flypaper', 'fly paper', 'bubblegum trap', 'gum trap', 'tar pool', 'glue puddle', 'sticky puddle'],
  cls: 'enclosure', motion: 'static', at: 'under_target', lifetime: 60,
  build() {
    const parts: Part[] = [V('cylinder', [6, 0.1, 6], [0, 0.05, 0], 'pool', 'pool'), V('torus', [6.2, 0.3, 6.2], [0, 0.1, 0], 'rim'), muzzle([0, 0.5, 0.3], 'seek', 0.3)];
    ring(5, 1.8).forEach(([, x, z], i) => parts.push(V('dome', [0.6 + (i % 2) * 0.3, 0.3, 0.6 + (i % 2) * 0.3], [x, 0.1, z], 'bubble', `bubble${i % 2}`)));
    return {
      parts,
      joints: [
        J('pool', [0, 0, 0], [jolt]),
        seeker([0, 0.5, 0], 'pool'),
        J('bubble0', [0, 0.1, 0], [bob(0.25, 1.2, 0, false)], 'pool'),
        J('bubble1', [0, 0.1, 0], [bob(0.25, 1.2, 0.5, false)], 'pool'),
      ],
    };
  },
  behaviors: [guardB(5), weaponB('net', 'seek', 0.7, 25, 3, 4.5, { size: 0.6 })],
  functions: ['has_moving_parts', 'guards_area', 'attacks_enemies', 'damages_target', 'restrain_target', 'area_attack'],
  verb: 'trap',
};

/** Pinball flipper: a bat on a hinge that whacks anything near it sideways. */
export const pinballFlipper: CatalogEntry = {
  id: 'pinball_flipper',
  names: ['pinball flipper', 'flipper', 'flipper trap', 'pinball bumper', 'giant flipper', 'flippers', 'bat flipper', 'pinball flippers', 'pinbal flipper', 'flipper bat', 'flipper arm', 'pin ball flipper'],
  cls: 'weapon', motion: 'static', at: 'under_target', scale: 1.2, lifetime: 60,
  build() {
    const g = 'flipper';
    const parts: Part[] = [
      P('cylinder', [1.0, 1.2, 1.0], [-2.0, 0.6, 0], { role: 'post' }),
      P('capsule', [0.8, 3.8, 0.6], [-0.2, 0.6, 0.6], { role: 'flipper', rot: [0, 0, 90], group: g }),
      V('sphere', [0.5, 0.5, 0.5], [-2.0, 1.3, 0], 'cap'),
      muzzle([0.2, 0.6, 1.0], g, 0.3),
    ];
    return { parts, joints: [J(g, [-2.0, 0.6, 0], [swing(4, 0.7, [0, 1, 0]), snap(70, 3, [0, 1, 0])])] };
  },
  behaviors: [guardB(5), weaponB('melee', 'flipper', 1.3, 0, 16, 2.8)],
  functions: [...TRAP_FNS, 'strike_target', 'push_target'],
  verb: 'strike',
};

/** Spring trap: a sprung floor plate that catapults whatever steps on it (you too — use it as a launcher). */
export const springTrap: CatalogEntry = {
  id: 'spring_trap',
  names: ['spring trap', 'launch trap', 'catapult plate', 'ejector plate', 'springboard trap', 'spring floor', 'spring loaded floor', 'spring-loaded floor', 'trapdoor spring', 'launch plate', 'sproing', 'ejector seat'],
  cls: 'launcher', motion: 'static', at: 'under_target', scale: 1.2, lifetime: 60,
  build() {
    const parts: Part[] = [
      V('box', [4.6, 0.12, 4.6], [0, 0.06, 0], 'base'),
      ...[0.2, 0.4, 0.6].map((y) => V('torus', [1.6, 0.12, 1.6], [0, y, 0], 'coil', 'coil')),
      P('box', [3.0, 0.2, 3.0], [0, 0.8, 0], { role: 'pad', group: 'plate' }),
      V('box', [1.2, 0.05, 0.3], [0, 0.92, 0], 'arrow', 'plate'),
    ];
    return { parts, joints: [J('plate', [0, 0.8, 0], [bob(0.12, 1.5, 0, false)]), J('coil', [0, 0.4, 0], [{ kind: 'piston', axis: [0, 1, 0], amp: 0.06, rate: 1.5 }])] };
  },
  behaviors: [{ type: 'launch', speed: 40, dir: 'up' }],
  functions: ['launch_player', 'raise_player', 'lift_target', 'has_moving_parts', 'support_player'],
  verb: 'launch',
};

/** Black hole: a spinning accretion disc that drags everything in and grinds what reaches the core. */
export const blackHole: CatalogEntry = {
  id: 'black_hole',
  names: ['black hole', 'blackhole', 'singularity', 'blackhol', 'wormhole trap', 'gravity bomb', 'black holes', 'blackholes', 'singularities', 'blak hole', 'blackhole trap', 'gravity well', 'gravity wells', 'event horizon', 'mini black hole', 'dark star'],
  cls: 'field', motion: 'static', at: 'aim', lifetime: 12,
  build() {
    return {
      parts: [
        P('sphere', [1.4, 1.4, 1.4], [0, 0, 0], { role: 'core' }),
        V('torus', [4.5, 0.3, 4.5], [0, 0, 0], 'disc', 'disc'),
        V('torus', [3.0, 0.2, 3.0], [0, 0, 0], 'disc', 'disc2', [30, 0, 0]),
        V('torus', [6.0, 0.12, 6.0], [0, 0, 0], 'halo', 'disc3', [-15, 0, 20]),
      ],
      joints: [J('disc', [0, 0, 0], [spin(1.5, [0, 1, 0])]), J('disc2', [0, 0, 0], [spin(-2.2, [0, 1, 0])]), J('disc3', [0, 0, 0], [spin(0.8, [0, 1, 0])])],
    };
  },
  behaviors: [{ type: 'field', mode: 'attract', radius: 18, strength: 55 }, guardB(10), weaponB('melee', undefined, 3, 0, 9, 1.2)],
  functions: ['attract', 'pull_target', 'has_moving_parts', 'attacks_enemies', 'damages_target', 'area_attack', 'spin'],
  verb: 'attract',
};

/** Meteor shower: a dark cloud parks over the target and rains burning rocks, each one led at where they run. */
export const meteorShower: CatalogEntry = {
  id: 'meteor_shower',
  names: ['meteor shower', 'asteroid shower', 'armageddon', 'firestorm', 'meteorshowers', 'meteor swarm', 'meteorite shower', 'meteor showers', 'meteor barrage', 'asteroid barrage', 'meteorshower', 'meteor strike', 'meteor storm', 'asteroid storm', 'meteor rain', 'rain of fire', 'fire rain', 'rock rain', 'hail of rocks', 'rain of meteors'],
  cls: 'weapon', motion: 'static', at: 'above_target', lifetime: 9,
  build() {
    const parts: Part[] = [
      ...ring(6, 2.4).map(([, x, z], i) => V('sphere', [3 + (i % 3) * 0.6, 1.6, 3], [x, (i % 2) * 0.4, z], 'cloud', 'cloud')),
      V('sphere', [3.4, 2.0, 3.4], [0, 0.3, 0], 'cloud', 'cloud'),
      V('sphere', [0.3, 0.3, 0.3], [0, -0.6, 0], 'eye', 'eye'),
      muzzle([0, -1.4, 0.3], 'eye', 0.3),
    ];
    return { parts, joints: [J('cloud', [0, 0, 0], [spin(0.15, [0, 1, 0])]), J('eye', [0, -0.6, 0], [{ kind: 'aim', yaw: true, pitch: true, rate: 500, minPitch: -89, maxPitch: 30 }])] };
  },
  behaviors: [guardB(40), weaponB('bomb', 'eye', 2.2, 30, 18, 45, { size: 0.8 })],
  functions: ['area_attack', 'projectile_attack', 'crush_target', 'shoots', 'attacks_enemies', 'damages_target', 'has_moving_parts', 'aims_at_target'],
  verb: 'crush',
};

/** Shockwave: a ring of light slams into the ground under the target and blasts everything outward. */
export const shockwave: CatalogEntry = {
  id: 'shockwave',
  names: ['shock wave', 'shockwave ring', 'seismic slam', 'thunderclap', 'stomp wave', 'blast wave', 'shockwaves', 'shock waves', 'shockwav', 'shockwave blast', 'seismic wave', 'ground pound', 'ground slam', 'sonic boom', 'thunder clap', 'pulse ring', 'earth shatter'],
  cls: 'field', motion: 'static', at: 'under_target', lifetime: 3,
  build() {
    return {
      parts: [
        V('torus', [3.0, 0.4, 3.0], [0, 0.3, 0], 'ring', 'ring'),
        V('torus', [5.5, 0.3, 5.5], [0, 0.2, 0], 'ring', 'ring2'),
        V('cylinder', [1.2, 0.2, 1.2], [0, 0.1, 0], 'core'),
      ],
      joints: [J('ring', [0, 0, 0], [spin(2, [0, 1, 0]), bob(0.4, 3, 0, false)]), J('ring2', [0, 0, 0], [spin(-1.5, [0, 1, 0]), bob(0.3, 3, 0.5, false)])],
    };
  },
  behaviors: [{ type: 'explode', radius: 12, force: 48, delay: 0.6, onContact: false }],
  functions: ['area_attack', 'push_target', 'repel', 'strike_target', 'damages_target', 'has_moving_parts'],
  verb: 'explode',
};

/** Blade wall: a wall (blocks shots) with three buzzsaws spinning in it that shred anyone who comes close. */
export const bladeWall: CatalogEntry = {
  id: 'blade_wall',
  names: ['blade wall', 'saw wall', 'spinning blade wall', 'wall of blades', 'wall of saws', 'razor wall', 'meat grinder', 'blender wall', 'buzzsaw wall', 'spinning blades', 'shredder wall', 'woodchipper', 'wood chipper'],
  cls: 'barrier', motion: 'static', at: 'between', lifetime: 60,
  build() {
    const parts: Part[] = [P('box', [7, 3.2, 0.5], [0, 1.6, 0], { role: 'body' })];
    const joints: Joint[] = [];
    [-2.3, 0, 2.3].forEach((x, i) => {
      const g = `saw${i}`;
      parts.push(V('cylinder', [2.0, 0.1, 2.0], [x, 1.6, 0.35], 'saw', g, [90, 0, 0]));
      for (const [a, sx, sz] of ring(8, 1.05)) parts.push(V('cone', [0.2, 0.35, 0.08], [x + sx, 1.6 + sz, 0.35], 'tooth', g, [0, 0, -a]));
      parts.push(muzzle([x, 1.6, 0.9], g, 0.25));
      joints.push(J(g, [x, 1.6, 0.35], [spin(i % 2 ? -4 : 4, [0, 0, 1])]));
    });
    return { parts, joints };
  },
  behaviors: [guardB(6), ...[0, 1, 2].map((i) => weaponB('melee', `saw${i}`, 2.5, 0, 12, 1.8))],
  functions: ['block_projectiles', 'shield_self', 'has_moving_parts', 'guards_area', 'attacks_enemies', 'damages_target', 'spin'],
  verb: 'block',
};

/** Lava floor: the floor is lava — a glowing, bubbling patch that burns anyone standing near it. */
export const lavaFloor: CatalogEntry = {
  id: 'lava_floor',
  names: ['lava floor', 'floor is lava', 'the floor is lava', 'lava pit', 'molten floor', 'magma floor', 'lava trap', 'lava', 'lava floors', 'lavafloor', 'magma pit', 'lava pool', 'lava lake', 'lava moat', 'hot floor', 'magma pool'],
  cls: 'weapon', motion: 'static', at: 'under_target', lifetime: 45,
  build() {
    const parts: Part[] = [V('box', [8, 0.08, 8], [0, 0.04, 0], 'lava'), V('box', [8.4, 0.2, 0.3], [0, 0.1, 4.1], 'crust'), V('box', [8.4, 0.2, 0.3], [0, 0.1, -4.1], 'crust')];
    ring(6, 2.5).forEach(([, x, z], i) => parts.push(V('dome', [0.8, 0.5, 0.8], [x, 0.05, z], 'bubble', `pop${i % 3}`)));
    return {
      parts,
      joints: [0, 1, 2].map((i) => J(`pop${i}`, [0, 0, 0], [bob(0.35, 0.9 + i * 0.3, i / 3, false)])),
    };
  },
  behaviors: [guardB(7), weaponB('melee', undefined, 3, 0, 7, 0.8)],
  functions: [...TRAP_FNS],
  verb: 'trap',
};

/** Domino wall: stands between you and them (blocks shots) and topples forward onto whoever comes close. */
export const dominoWall: CatalogEntry = {
  id: 'domino_wall',
  names: ['domino wall', 'dominoes', 'dominos', 'domino', 'giant domino', 'giant dominoes', 'topple wall', 'timberrr', 'domino walls', 'dominoe', 'dominoe wall', 'toppling wall', 'falling wall', 'timber wall', 'crushing wall'],
  cls: 'barrier', motion: 'static', at: 'between', lifetime: 45,
  build() {
    const g = 'slab';
    const parts: Part[] = [
      ...[-2.2, 0, 2.2].map((x) => P('box', [2.0, 5.0, 0.6], [x, 2.5, 0], { role: 'domino', group: g })),
      ...[-2.2, 0, 2.2].flatMap((x) => [V('sphere', [0.4, 0.4, 0.1], [x, 3.5, 0.32], 'pip', g), V('sphere', [0.4, 0.4, 0.1], [x, 1.5, 0.32], 'pip', g)]),
      muzzle([0, 1.2, 3.0], g, 0.3),
    ];
    return { parts, joints: [J(g, [0, 0, 0.3], [swing(1.5, 0.5, [1, 0, 0]), snap(85, 1.1, [1, 0, 0])])] };
  },
  behaviors: [guardB(7), weaponB('melee', 'slab', 0.8, 0, 35, 3)],
  functions: ['block_projectiles', 'shield_self', 'crush_target', 'has_moving_parts', 'guards_area', 'attacks_enemies', 'damages_target'],
  verb: 'block',
};

/** Portcullis: a spiked iron gate between you and them; it blocks shots and slams on anyone at the gate. */
export const portcullis: CatalogEntry = {
  id: 'portcullis',
  names: ['port cullis', 'portculis', 'spiked gate', 'gate trap', 'portcullises', 'portcullis gate', 'porticullis', 'porcullis', 'portcullace', 'spiked gates', 'gate traps', 'castle gate', 'iron gate', 'drop gate', 'falling gate', 'sally gate'],
  cls: 'barrier', motion: 'static', at: 'between', lifetime: 60,
  build() {
    const g = 'grille';
    const parts: Part[] = [
      P('box', [0.6, 5.5, 0.8], [-3.2, 2.75, 0], { role: 'post' }),
      P('box', [0.6, 5.5, 0.8], [3.2, 2.75, 0], { role: 'post' }),
      P('box', [7.0, 0.7, 0.8], [0, 5.5, 0], { role: 'lintel' }),
      ...[-2.1, -0.7, 0.7, 2.1].map((x) => P('box', [0.25, 4.6, 0.25], [x, 2.5, 0], { role: 'bar', group: g })),
      ...[1.2, 2.6, 4.0].map((y) => P('box', [5.6, 0.22, 0.22], [0, y, 0], { role: 'bar', group: g })),
      ...[-2.1, -0.7, 0.7, 2.1].map((x) => V('cone', [0.3, 0.5, 0.3], [x, 0.0, 0], 'spike', g, [180, 0, 0])),
      muzzle([0, 0.4, 0.4], g, 0.3),
    ];
    return { parts, joints: [J(g, [0, 2.5, 0], [bob(0.05, 0.7, 0, false), { kind: 'piston', axis: [0, 1, 0], amp: 2.2, rate: 1.5, on: 'fire' }])] };
  },
  behaviors: [guardB(4.5), weaponB('melee', 'grille', 0.8, 0, 30, 2.4)],
  functions: ['block_projectiles', 'shield_self', 'has_moving_parts', 'guards_area', 'attacks_enemies', 'damages_target', 'strike_target'],
  verb: 'block',
};

/** Guillotine: a frame over the spot; the blade drops on anyone under it. */
export const guillotine: CatalogEntry = {
  id: 'guillotine',
  names: ['guilotine', 'giliotine', 'guillotine trap', 'chopping block', 'guillotines', 'gillotine', 'guillotene', 'guillotine blade', 'guilletine', 'gullotine', 'guillitine', 'falling blade', 'drop blade', 'blade drop'],
  cls: 'weapon', motion: 'static', at: 'under_target', lifetime: 60,
  build() {
    const g = 'blade';
    const parts: Part[] = [
      P('box', [0.35, 5.0, 0.35], [-1.5, 2.5, 0], { role: 'post' }),
      P('box', [0.35, 5.0, 0.35], [1.5, 2.5, 0], { role: 'post' }),
      P('box', [3.5, 0.4, 0.5], [0, 5.1, 0], { role: 'beam' }),
      V('box', [2.8, 0.5, 0.12], [0, 4.4, 0], 'weight', g),
      V('wedge', [2.8, 0.9, 0.12], [0, 3.75, 0], 'blade', g, [90, 0, 0]),
      muzzle([0, 3.0, 0], g, 0.3),
    ];
    return { parts, joints: [J(g, [0, 4.0, 0], [bob(0.06, 0.6, 0, false), { kind: 'piston', axis: [0, -1, 0], amp: 2.9, rate: 2.2, on: 'fire' }])] };
  },
  behaviors: [guardB(4), weaponB('melee', 'blade', 0.7, 0, 45, 2.8)],
  functions: [...TRAP_FNS, 'strike_target', 'crush_target'],
  verb: 'crush',
};

/** Pile driver: a derrick that keeps hammering a huge ram into the ground — don't stand under it. */
export const pileDriver: CatalogEntry = {
  id: 'pile_driver',
  names: ['pile driver', 'piledriver', 'pile-driver', 'pile drivers', 'piledrivers', 'pile drive', 'piledrive', 'pile driver trap', 'pylon driver', 'pile hammer', 'piling hammer', 'stamping press', 'hydraulic press', 'drop hammer', 'steam hammer', 'trip hammer', 'power hammer', 'press trap'],
  cls: 'heavy', motion: 'static', at: 'under_target', lifetime: 45,
  build() {
    const g = 'ram';
    const parts: Part[] = [
      ...[[-1.6, -1.6], [1.6, -1.6], [-1.6, 1.6], [1.6, 1.6]].map(([x, z]) => P('box', [0.3, 6.5, 0.3], [x, 3.25, z], { role: 'post' })),
      P('box', [3.6, 0.6, 3.6], [0, 6.6, 0], { role: 'top' }),
      V('cylinder', [0.4, 1.6, 0.4], [0, 5.5, 0], 'rod', g),
      V('box', [2.4, 1.6, 2.4], [0, 4.0, 0], 'ram', g),
      muzzle([0, 3.0, 0], g, 0.3),
    ];
    return { parts, joints: [J(g, [0, 4.0, 0], [{ kind: 'piston', axis: [0, -1, 0], amp: 2.6, rate: 0.9 }])] };
  },
  behaviors: [guardB(4), weaponB('melee', 'ram', 0.9, 0, 40, 2.8)],
  functions: [...TRAP_FNS, 'crush_target', 'strike_target'],
  verb: 'crush',
};

/** Jack-in-the-box: a box that turns to face intruders, flips its lid and punches with a glove on a spring. */
export const springGlove: CatalogEntry = {
  id: 'spring_glove',
  names: ['spring glove', 'spring loaded glove', 'spring-loaded glove', 'spring punch', 'jack in the box glove', 'punch box', 'punching box', 'boing glove', 'jack-in-the-box', 'spring gloves', 'springglove', 'punching machine', 'boxing machine', 'surprise glove'],
  cls: 'weapon', motion: 'static', at: 'ground_front', lifetime: 60,
  build() {
    const parts: Part[] = [
      P('box', [1.6, 0.2, 1.6], [0, 0.1, 0], { role: 'base' }),
      P('box', [1.6, 1.6, 1.6], [0, 1.0, 0], { role: 'box', group: 'box' }),
      V('box', [1.7, 0.12, 1.7], [0, 1.85, 0], 'lid', 'lid'),
      ...[0.25, 0.55, 0.85].map((z) => V('torus', [0.5, 0.08, 0.5], [0, 1.2, 0.8 + z], 'spring', 'glove', [90, 0, 0])),
      V('sphere', [1.1, 1.0, 1.1], [0, 1.2, 2.0], 'glove', 'glove'),
      V('capsule', [0.3, 0.45, 0.3], [0.55, 1.15, 1.85], 'thumb', 'glove', [90, 0, 20]),
      muzzle([0, 1.2, 2.4], 'glove', 0.3),
    ];
    return {
      parts,
      joints: [
        J('box', [0, 0.2, 0], [aimYaw(240)]),
        J('lid', [0, 1.85, -0.8], [snap(100, 2, [-1, 0, 0])], 'box'),
        J('glove', [0, 1.2, 0.8], [{ kind: 'piston', axis: [0, 0, 1], amp: 2.5, rate: 2.5, on: 'fire' }], 'box'),
      ],
    };
  },
  behaviors: [guardB(6), weaponB('melee', 'glove', 1.0, 0, 22, 3.6)],
  functions: [...TRAP_FNS, 'strike_target', 'push_target', 'aims_at_target'],
  verb: 'strike',
};

/** Reaper's scythe: a post with a long arm that sweeps a giant scythe blade round in a circle. */
export const scytheTrap: CatalogEntry = {
  id: 'scythe_trap',
  names: ['scythe', 'giant scythe', 'reaper scythe', "reaper's scythe", 'grim reaper scythe', 'scythe trap', 'spinning scythe', 'sickle', 'scyth', 'sythe', 'war scythe', 'death scythe', 'harvester'],
  cls: 'weapon', motion: 'static', at: 'under_target', lifetime: 45,
  build() {
    const g = 'arm';
    const parts: Part[] = [
      P('cylinder', [0.6, 2.2, 0.6], [0, 1.1, 0], { role: 'post' }),
      V('cylinder', [0.2, 4.4, 0.2], [0, 1.4, 2.2], 'snath', g, [90, 0, 0]),
      V('wedge', [0.1, 0.6, 2.4], [0.9, 1.1, 4.2], 'blade', g, [0, 90, 0]),
      muzzle([0.9, 1.1, 4.2], g, 0.3),
    ];
    return { parts, joints: [J(g, [0, 1.4, 0], [spin(0.55, [0, 1, 0])])] };
  },
  behaviors: [guardB(6.5), weaponB('melee', 'arm', 2, 0, 20, 1.9)],
  functions: [...TRAP_FNS, 'strike_target', 'sweep', 'spin'],
  verb: 'strike',
};

/** Pendulum blade: a gallows frame with a razor pendulum swinging across the path. */
export const bladePendulum: CatalogEntry = {
  id: 'blade_pendulum',
  names: ['pendulum blade', 'pendulum trap', 'swinging blade trap', 'swinging axe trap', 'blade pendulum', 'razor pendulum', 'swinging blades', 'pendulum blades', 'pendulum traps', 'pendelum blade', 'pendulem trap', 'pit and pendulum'],
  cls: 'heavy', motion: 'static', at: 'under_target', lifetime: 45,
  build() {
    const g = 'pend';
    const parts: Part[] = [
      P('box', [0.4, 7.0, 0.4], [-4.2, 3.5, 0], { role: 'post' }),
      P('box', [0.4, 7.0, 0.4], [4.2, 3.5, 0], { role: 'post' }),
      P('box', [8.8, 0.4, 0.4], [0, 7.0, 0], { role: 'beam' }),
      V('cylinder', [0.14, 5.2, 0.14], [0, 4.3, 0], 'rod', g),
      V('wedge', [2.6, 1.0, 0.14], [0, 1.4, 0], 'blade', g, [0, 0, 180]),
      muzzle([0, 1.3, 0], g, 0.3),
    ];
    return { parts, joints: [J(g, [0, 6.9, 0], [swing(55, 0.55, [0, 0, 1])])] };
  },
  behaviors: [guardB(8), weaponB('melee', 'pend', 2, 0, 30, 1.7)],
  functions: [...TRAP_FNS, 'strike_target', 'sweep'],
  verb: 'strike',
};

export const TRAPS: CatalogEntry[] = [
  bearTrap, spikeFloor, minefield, tarPit, pinballFlipper, springTrap, blackHole, meteorShower, shockwave, bladeWall, lavaFloor,
  dominoWall, portcullis, guillotine, pileDriver, springGlove, scytheTrap, bladePendulum,
];

