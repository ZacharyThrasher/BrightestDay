import { Vector3, Quaternion } from 'three';
import type { World } from './world.js';

export const SCENES = ['duel', 'arena', 'gap', 'tower', 'barrage', 'falling', 'boulder', 'drones', 'playground', 'flat', 'sandbox', 'race', 'horde'] as const;
export type SceneName = (typeof SCENES)[number];

function rng(seed: number) {
  return () => {
    seed |= 0; seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const PALETTE = [0xf2d7b6, 0xe8b99a, 0xc9d6e8, 0xb8c7a8, 0xe6c9d8, 0xd9d2c0, 0xa9c1d9, 0xf0e2c0];

/** Named anchor points each scene exposes for tests and the eval harness. */
export interface SceneInfo {
  spawn: Vector3;
  yaw: number;
  aims: Record<string, Vector3>;
}

function ground(w: World, size = 2000) {
  w.addBox(new Vector3(0, -1, 0), new Vector3(size, 2, size), 0x7fa37a, 'ground');
}

export function buildScene(w: World, name: SceneName): SceneInfo {
  w.scene = name;
  const info: SceneInfo = { spawn: new Vector3(0, 1.0, 0), yaw: 0, aims: { ground_ahead: new Vector3(0, 0, -12), sky: new Vector3(0, 60, -60), wall: new Vector3(0, 3, -40) } };
  switch (name) {
    case 'duel':
      buildDuelArena(w, info);
      break;
    case 'flat':
      ground(w);
      break;
    case 'arena': {
      ground(w);
      w.addBox(new Vector3(0, 5, -40), new Vector3(30, 10, 2), 0xd9d2c0, 'wall');
      w.addBox(new Vector3(-25, 8, -20), new Vector3(10, 16, 10), PALETTE[0]);
      w.addBox(new Vector3(25, 12, -25), new Vector3(10, 24, 10), PALETTE[2]);
      w.addEntity('dummy', new Vector3(6, 1.2, -14), { hostile: false, label: 'training dummy' });
      w.addEntity('crate', new Vector3(-5, 0.8, -10));
      info.aims.wall = new Vector3(0, 4, -39);
      break;
    }
    case 'gap': {
      w.addBox(new Vector3(0, -20, 20), new Vector3(60, 40, 40), 0xc9b08a, 'rock');
      w.addBox(new Vector3(0, -20, -45), new Vector3(60, 40, 40), 0xc9b08a, 'rock');
      w.addBox(new Vector3(0, -41, -12), new Vector3(200, 2, 200), 0x6f8f6a, 'ground');
      w.addGoal(new Vector3(0, 1.5, -40), new Vector3(30, 3, 15), 'far side');
      info.spawn.set(0, 1, 2);
      info.aims.far_ledge = new Vector3(0, 0, -30);
      info.aims.ground_ahead = new Vector3(0, 0, -3);
      info.aims.wall = new Vector3(0, -10, -25);
      break;
    }
    case 'tower': {
      ground(w);
      w.addBox(new Vector3(0, 20, -23), new Vector3(16, 40, 16), PALETTE[2]);
      w.addGoal(new Vector3(0, 42, -23), new Vector3(8, 2.5, 8), 'rooftop');
      info.aims.far_ledge = new Vector3(0, 40, -18);
      info.aims.wall = new Vector3(0, 12, -15);
      break;
    }
    case 'barrage': {
      ground(w);
      w.addEntity('turret', new Vector3(0, 1.2, -30), { label: 'turret' });
      info.aims.turret = new Vector3(0, 1.5, -30);
      break;
    }
    case 'falling': {
      ground(w);
      w.addEntity('civilian', new Vector3(0, 60, -15), { hostile: false, label: 'falling civilian' });
      info.aims.civilian = new Vector3(0, 60, -15);
      break;
    }
    case 'boulder': {
      ground(w);
      w.addBox(new Vector3(-9.5, 5, -12), new Vector3(14, 10, 2), 0xd9d2c0, 'wall');
      w.addBox(new Vector3(9.5, 5, -12), new Vector3(14, 10, 2), 0xd9d2c0, 'wall');
      w.addBox(new Vector3(0, 9, -12), new Vector3(5, 2, 2), 0xd9d2c0, 'wall');
      w.addEntity('boulder', new Vector3(0, 2.2, -12), { hostile: false, label: 'boulder' });
      w.addGoal(new Vector3(0, 1.5, -20), new Vector3(5, 3, 4), 'behind the door');
      info.aims.boulder = new Vector3(0, 2.2, -12);
      info.aims.wall = new Vector3(-7, 4, -11);
      break;
    }
    case 'drones': {
      ground(w);
      w.addEntity('drone', new Vector3(-8, 6, -18));
      w.addEntity('drone', new Vector3(0, 5, -22));
      w.addEntity('drone', new Vector3(7, 8, -28));
      w.addBox(new Vector3(0, 6, -45), new Vector3(40, 12, 2), 0xd9d2c0, 'wall');
      info.aims.drone = new Vector3(0, 5, -22);
      info.aims.wall = new Vector3(0, 5, -44);
      break;
    }
    case 'playground':
      buildPlayground(w, info);
      break;
    case 'sandbox':
      buildSandbox(w, info);
      break;
    case 'horde':
      // the sandbox with enemy waves switched on (a new, capped wave every 22 s)
      buildSandbox(w, info);
      w.content.setWaves(true, { period: 22, cap: 8, firstIn: 10 });
      break;
    case 'race':
      buildRace(w, info);
      break;
  }
  w.player.teleport(info.spawn);
  w.player.yaw = info.yaw;
  w.refreshQueries();
  return info;
}

/**
 * The shareable sandbox: a stylised city for momentum traversal plus challenge stations
 * that each invite many different construct solutions.
 */
function buildPlayground(w: World, info: SceneInfo) {
  const r = rng(1337);
  ground(w, 3000);
  // city grid north of the plaza (negative z)
  const block = 34;
  for (let gx = -6; gx <= 6; gx++) {
    for (let gz = -12; gz <= -2; gz++) {
      if (Math.abs(gx) <= 1 && gz >= -3) continue; // plaza mouth
      if (r() < 0.12) continue; // occasional empty lot
      const cx = gx * block + (r() - 0.5) * 6;
      const cz = gz * block + (r() - 0.5) * 6;
      const wdt = 12 + r() * 12;
      const dep = 12 + r() * 12;
      const tall = r() < 0.18;
      const h = tall ? 70 + r() * 70 : 14 + r() * 40;
      w.addBox(new Vector3(cx, h / 2, cz), new Vector3(wdt, h, dep), PALETTE[Math.floor(r() * PALETTE.length)]);
      if (r() < 0.25) w.addBox(new Vector3(cx, h + 3, cz), new Vector3(wdt * 0.5, 6, dep * 0.5), PALETTE[Math.floor(r() * PALETTE.length)]);
    }
  }
  // plaza props: a couple of skate ramps + pillars for early movement play
  w.addWedge(new Vector3(-18, 1.5, -10), new Vector3(8, 3, 10), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), Math.PI), 0xe4c28f);
  w.addWedge(new Vector3(18, 3, -30), new Vector3(8, 6, 16), new Quaternion(), 0xe4c28f);
  w.addWedge(new Vector3(0, 2, 30), new Vector3(10, 4, 12), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), Math.PI), 0xe4c28f);
  // surf ramp (steep two-sided prism made of two rotated boxes)
  for (const s of [-1, 1]) {
    const q = new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), s * 0.85);
    w.addBox(new Vector3(60 + s * 4, 6, 20), new Vector3(10, 1.5, 90), 0xc9d6e8, 'rock', q);
  }
  for (let i = 0; i < 6; i++) w.addCylinder(new Vector3(-50 + i * 6, 6 + i * 3, 40), 1.4, 12 + i * 6, 0xe8b99a, 'rock');

  // canyon (east): cross it
  w.addBox(new Vector3(200, -30, 60), new Vector3(60, 2, 200), 0x6f8f6a, 'ground');
  w.addGoal(new Vector3(260, 2, 60), new Vector3(20, 4, 60), 'canyon: far side');

  // tower (west)
  w.addBox(new Vector3(-140, 60, 60), new Vector3(20, 120, 20), PALETTE[2]);
  w.addGoal(new Vector3(-140, 122, 60), new Vector3(10, 3, 10), 'tower: rooftop');

  // turret outpost (south-east)
  w.addBox(new Vector3(90, 3, 150), new Vector3(30, 6, 2), 0xd9d2c0, 'wall');
  w.addEntity('turret', new Vector3(90, 7.2, 158), { label: 'turret' });
  // drone field (south-west)
  for (let i = 0; i < 5; i++) w.addEntity('drone', new Vector3(-90 + i * 9, 7 + (i % 3) * 3, 150 + (i % 2) * 8));
  // boulder gate (south)
  w.addBox(new Vector3(-9.5, 5, 110), new Vector3(14, 10, 2), 0xd9d2c0, 'wall');
  w.addBox(new Vector3(9.5, 5, 110), new Vector3(14, 10, 2), 0xd9d2c0, 'wall');
  w.addBox(new Vector3(0, 9, 110), new Vector3(5, 2, 2), 0xd9d2c0, 'wall');
  w.addEntity('boulder', new Vector3(0, 2.2, 110), { hostile: false });
  w.addGoal(new Vector3(0, 2, 122), new Vector3(6, 3, 5), 'gate: through the door');
  // crates to throw around
  for (let i = 0; i < 6; i++) w.addEntity('crate', new Vector3(30 + (i % 3) * 2.5, 0.8 + Math.floor(i / 3) * 1.7, 12));

  // race rings through the city
  const ringPts: [number, number, number][] = [
    [0, 12, -80], [-30, 25, -150], [-10, 45, -230], [40, 30, -300], [80, 60, -250], [60, 20, -170], [100, 15, -90], [40, 8, -30],
  ];
  for (let i = 0; i < ringPts.length; i++) {
    const p = new Vector3(...ringPts[i]);
    const n = new Vector3(...ringPts[(i + 1) % ringPts.length]).sub(new Vector3(...(ringPts[i - 1] ?? [0, 1, 0]))).normalize();
    w.addRing(p, n, 6);
  }
  info.spawn.set(0, 1, 0);
  info.aims.drone = new Vector3(-90, 7, 150);

  // --- added after everything above so existing ids / layout are unchanged ---
  // loose block stacks on the plaza edges (topple them with anything)
  w.content.addStack(new Vector3(14, 0, 16), { shape: 'pyramid', levels: 4 });
  w.content.addStack(new Vector3(-14, 0, 18), { shape: 'tower', levels: 5, size: 1.1, colors: [0xa9c1d9, 0xe6c9d8] });
  // robot patrol yard (south-west): two robots walk a loop and chase anyone who comes close
  w.content.addWall(new Vector3(-45, 0, 72), { bricks: 7, courses: 3 });
  w.content.addRobot(new Vector3(-50, 1.1, 86), { patrol: new Vector3(-45, 0, 86), patrolR: 7, aggro: 20, leash: 36 });
  w.content.addRobot(new Vector3(-40, 1.1, 86), { patrol: new Vector3(-45, 0, 86), patrolR: 7, aggro: 20, leash: 36 });
  info.aims.robot = new Vector3(-50, 1.1, 86);
}

/**
 * Sandbox (EVAL_SPEC_MACHINES): large open ground, scattered crates, a stack of loose blocks, a low
 * wall, two ground robots, two hostile drones, one turret and one civilian standing still.
 * Player at the origin looking down -z.
 */
function buildSandbox(w: World, info: SceneInfo) {
  ground(w, 2000);
  const C = w.content;
  // hostiles + civilian first (low ids, first in entity lists)
  C.addRobot(new Vector3(-10, 1.1, -42));
  C.addRobot(new Vector3(12, 1.1, -40));
  w.addEntity('drone', new Vector3(-16, 8, -36));
  w.addEntity('drone', new Vector3(14, 7, -30));
  w.addEntity('turret', new Vector3(24, 1.2, -56), { label: 'turret' });
  C.addBystander(new Vector3(-14, 0.95, -8));
  // scattered crates
  const crates: [number, number][] = [[6.5, -8], [-6, -11], [-16, -22], [20, -13], [-2, -27], [4, -46]];
  for (const [x, z] of crates) w.addEntity('crate', new Vector3(x, 0.8, z));
  // a stack of loose blocks
  C.addStack(new Vector3(13, 0, -15), { shape: 'pyramid', levels: 4 });
  // a low wall of bricks (breakable)
  C.addWall(new Vector3(-12, 0, -30), { bricks: 8, courses: 3 });
  info.aims.robot = new Vector3(-10, 1.1, -42);
  info.aims.drone = new Vector3(14, 7, -30);
  info.aims.turret = new Vector3(24, 1.5, -56);
  info.aims.civilian = new Vector3(-14, 1, -8);
  info.aims.crate = new Vector3(6.5, 0.8, -8);
  info.aims.blocks = new Vector3(13, 1.5, -15);
  info.aims.wall = new Vector3(-12, 0.8, -30);
}

/** Race: a 200 m straight road from the start line (z = 0) to a finish line (z = -200); nothing hostile. */
function buildRace(w: World, info: SceneInfo) {
  ground(w, 2000);
  const len = 240, width = 14, z0 = 20;
  // road surface sits 2 cm proud of the ground
  w.addBox(new Vector3(0, -0.48, z0 - len / 2), new Vector3(width, 1, len), 0x8f98aa, 'ground');
  // kerbs along both sides (outside the lane)
  for (const s of [-1, 1]) {
    for (let i = 0; i < 24; i++) {
      w.addBox(new Vector3(s * (width / 2 + 0.5), 0.1, z0 - 5 - i * 10), new Vector3(1, 0.2, 10), i % 2 ? 0xf2f4f8 : 0xe2524f, 'prop');
    }
  }
  // start line + gantry posts
  w.addBox(new Vector3(0, 0.025, 0), new Vector3(width, 0.01, 0.6), 0xf2f4f8, 'prop');
  // finish line: chequered strip and an arch the player passes under
  for (let r = 0; r < 2; r++) {
    for (let i = 0; i < width; i++) {
      w.addBox(new Vector3(-width / 2 + 0.5 + i, 0.025, -200 + (r - 0.5)), new Vector3(1, 0.01, 1), (i + r) % 2 ? 0x2a2e3d : 0xf2f4f8, 'prop');
    }
  }
  for (const zz of [0, -200]) {
    for (const s of [-1, 1]) w.addBox(new Vector3(s * (width / 2 + 2), 4, zz), new Vector3(1, 8, 1), zz === 0 ? 0xa9c1d9 : 0xf0e2c0, 'prop');
    w.addBox(new Vector3(0, 8.6, zz), new Vector3(width + 5, 1.2, 0.8), zz === 0 ? 0xa9c1d9 : 0xffc55c, 'prop');
  }
  w.addGoal(new Vector3(0, 3, -200), new Vector3(width / 2 + 1, 4, 1.5), 'finish line');
  w.content.setupRace(0, -200, width / 2);
  info.spawn.set(0, 1, 8);
  info.aims.finish = new Vector3(0, 1, -200);
  info.aims.far_ledge = new Vector3(0, 0, -200);
  info.aims.wall = new Vector3(0, 8.6, -200);
}

/** Aim the player at a world point (sets yaw/pitch so the camera crosshair ray passes through it). */
export function aimAt(w: World, target: Vector3) {
  const P = w.player;
  // iterate because the camera sits behind/right of the player
  for (let i = 0; i < 6; i++) {
    const cam = P.camera();
    const d = target.clone().sub(cam.pos);
    const yaw = Math.atan2(-d.x, -d.z);
    const pitch = Math.atan2(d.y, Math.hypot(d.x, d.z));
    P.yaw = yaw;
    P.pitch = pitch;
  }
  return { yaw: P.yaw, pitch: P.pitch };
}

/**
 * 1v1 duel arena (docs/PVP_DESIGN.md). 90 m square, point-symmetric so neither spawn is favoured.
 * Tall perimeter walls (tether anchors, nowhere to run forever), a central tower with two ledges,
 * four mid pillars for swings, low cover near each spawn, ramps for launches. Spawns at z = ±36.
 */
function buildDuelArena(w: World, info: SceneInfo) {
  ground(w, 400);
  const H = 45; // half size
  const wallC = 0xd9d2c0;
  // perimeter walls (28 m tall)
  w.addBox(new Vector3(0, 14, -H - 1), new Vector3(2 * H + 4, 28, 2), wallC, 'wall');
  w.addBox(new Vector3(0, 14, H + 1), new Vector3(2 * H + 4, 28, 2), wallC, 'wall');
  w.addBox(new Vector3(-H - 1, 14, 0), new Vector3(2, 28, 2 * H + 4), wallC, 'wall');
  w.addBox(new Vector3(H + 1, 14, 0), new Vector3(2, 28, 2 * H + 4), wallC, 'wall');
  // central tower + two ledges facing each spawn
  w.addBox(new Vector3(0, 11, 0), new Vector3(8, 22, 8), PALETTE[2]);
  w.addBox(new Vector3(0, 12, -6.5), new Vector3(8, 1, 5), PALETTE[6]);
  w.addBox(new Vector3(0, 12, 6.5), new Vector3(8, 1, 5), PALETTE[6]);
  // mid pillars (point-symmetric pairs) — swing anchors and line-of-sight breakers
  const pillars: [number, number, number][] = [[-20, -14, 18], [20, 14, 18], [20, -16, 12], [-20, 16, 12]];
  pillars.forEach(([x, z, h], i) => w.addBox(new Vector3(x, h / 2, z), new Vector3(4, h, 4), PALETTE[i % PALETTE.length]));
  // side platforms (mirrored) reachable by ramp or tether
  for (const sx of [-1, 1]) {
    w.addBox(new Vector3(sx * 34, 4, sx * 8), new Vector3(10, 8, 16), PALETTE[3]);
    w.addWedge(new Vector3(sx * 34, 2, sx * 8 - sx * 13), new Vector3(8, 4, 10), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), sx > 0 ? 0 : Math.PI));
  }
  // low cover near each spawn (1.6 m: hides a crouching lantern, not a standing one)
  for (const sz of [-1, 1]) {
    w.addBox(new Vector3(-8, 0.8, sz * 26), new Vector3(8, 1.6, 1), PALETTE[1], 'wall');
    w.addBox(new Vector3(9, 0.8, sz * 24), new Vector3(6, 1.6, 1), PALETTE[1], 'wall');
    // launch ramp toward the centre
    w.addWedge(new Vector3(0, 1, sz * 20), new Vector3(6, 2, 7), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), sz > 0 ? Math.PI : 0));
  }
  info.spawn.set(0, 1, 36);
  info.yaw = 0; // facing -z, toward the centre
  info.aims.spawn_b = new Vector3(0, 1, -36);
  info.aims.ground_ahead = new Vector3(0, 0, 24);
  info.aims.wall = new Vector3(0, 11, 4);
  // practice target where the rival would stand (replaced by a real opponent in matches)
  w.addEntity('dummy', new Vector3(0, 1.2, -36), { hostile: false, label: 'rival (practice dummy)' });
}
