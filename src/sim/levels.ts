import { Vector3, Quaternion } from 'three';
import type { World } from './world.js';

export const SCENES = ['duel', 'city', 'arena', 'gap', 'tower', 'barrage', 'falling', 'boulder', 'drones', 'playground', 'flat', 'sandbox', 'race', 'horde'] as const;
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
    case 'city':
      buildCity(w, info);
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
  setupDuelSpawns(w, info);
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

/**
 * Generic duel wiring for any scene that exposes a second spawn (`info.aims.spawn_b`): when the scene did not
 * set its own respawn list, the two ends become the spawns (facing each other), and any "practice dummy"
 * standing in for the rival is registered so a real opponent joining removes it.
 */
function setupDuelSpawns(w: World, info: SceneInfo) {
  const b = info.aims.spawn_b;
  if (!w.spawns.length && b) {
    const a = info.spawn;
    w.spawns = [
      { pos: a.clone(), yaw: Math.atan2(-(b.x - a.x), -(b.z - a.z)) },
      { pos: b.clone(), yaw: Math.atan2(-(a.x - b.x), -(a.z - b.z)) },
    ];
  }
  for (const e of w.entities.values()) if (/practice dummy/i.test(e.label) && !w.practiceDummies.includes(e.id)) w.practiceDummies.push(e.id);
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
  // spawns: the two ends, then two far corners (respawns pick the one farthest from the opponent)
  const face = (x: number, z: number) => Math.atan2(x, z);
  w.spawns = [[0, 36], [0, -36], [38, -36], [-38, 36]].map(([x, z]) => ({ pos: new Vector3(x, 1, z), yaw: face(x, z) }));
  // practice target where the rival would stand (removed when a real opponent joins)
  const dummy = w.addEntity('dummy', new Vector3(0, 1.2, -36), { hostile: false, label: 'rival (practice dummy)' });
  w.practiceDummies.push(dummy.id);
}

/**
 * Big city duel map (~500 m square), built for fights: wide streets (≥ 28 m), a huge open central
 * park (~170 m square, only low cover, trees and a monument) ringed by the tallest towers (tether
 * anchors all around the brawl), plazas, a wide central boulevard, an elevated highway, sky bridges,
 * low-rise edges with rooftop gaps, and a tall perimeter. Point-symmetric (every building has a twin
 * at (-x, -z)) so neither spawn is favoured. Spawns at z = ±190 on the boulevard, facing each other.
 */
function buildCity(w: World, info: SceneInfo) {
  const r = rng(20260926);
  ground(w, 1600);
  const N = 4; // lots from -N..N on each axis
  const LOT = 56; // lot pitch: footprint ≤ 28 m leaves ≥ 28 m streets
  const HALF = (N + 0.5) * LOT; // 252
  const col = () => PALETTE[Math.floor(r() * PALETTE.length)];
  const box = (x: number, y: number, z: number, sx: number, sy: number, sz: number, c: number, kind: 'building' | 'wall' | 'rock' = 'building') => {
    w.addBox(new Vector3(x, y, z), new Vector3(sx, sy, sz), c, kind);
    if (x !== 0 || z !== 0) w.addBox(new Vector3(-x, y, -z), new Vector3(sx, sy, sz), c, kind); // point-symmetric twin
  };
  const cyl = (x: number, y: number, z: number, rad: number, h: number, c: number) => {
    w.addCylinder(new Vector3(x, y, z), rad, h, c, 'rock');
    if (x !== 0 || z !== 0) w.addCylinder(new Vector3(-x, y, -z), rad, h, c, 'rock');
  };
  const wedge = (x: number, y: number, z: number, sx: number, sy: number, sz: number, yaw: number, c = 0xe4c28f) => {
    w.addWedge(new Vector3(x, y, z), new Vector3(sx, sy, sz), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), yaw), c);
    w.addWedge(new Vector3(-x, y, -z), new Vector3(sx, sy, sz), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), yaw + Math.PI), c);
  };
  // perimeter (70 m) — the city's edge
  const wallC = 0xd9d2c0;
  box(0, 35, -HALF - 6, 2 * HALF + 16, 70, 4, wallC, 'wall');
  box(-HALF - 6, 35, 0, 4, 70, 2 * HALF + 16, wallC, 'wall');

  const towers: { x: number; z: number; h: number; w: number; d: number }[] = [];
  for (let gx = -N; gx <= N; gx++) {
    for (let gz = -N; gz <= N; gz++) {
      if (gz > 0 || (gz === 0 && gx > 0)) continue; // the twin handles it
      const cx = gx * LOT, cz = gz * LOT;
      const ring = Math.max(Math.abs(gx), Math.abs(gz));
      if (ring <= 1) continue; // central park (built once below)
      if (gx === 0) continue; // central boulevard
      if (Math.abs(gx) === 3 && Math.abs(gz) === 3) { buildPlaza(cx, cz); continue; }
      if (ring === N && r() < 0.15) continue; // an empty lot now and then at the edge
      // tallest towers ring the park; mid-rise beyond; low-rise at the edges
      const base = ring === 2 ? 70 + r() * 90 : ring === 3 ? 26 + r() * 36 : 8 + r() * 16;
      const n = ring === N && r() < 0.6 ? 2 : 1; // low-rise lots split in two (rooftop gaps to hop)
      for (let i = 0; i < n; i++) {
        const bw = n === 2 ? 11 + r() * 2 : 18 + r() * 10;
        const bd = 18 + r() * 10;
        const ox = n === 2 ? (i === 0 ? -7.5 : 7.5) : (r() - 0.5) * 2;
        const h = n === 2 ? base * (0.7 + r() * 0.5) : base;
        const x = cx + ox, z = cz + (r() - 0.5) * 2;
        const c = col();
        box(x, h / 2, z, bw, h, bd, c);
        if (h > 50) {
          let y = h, tw = bw, td = bd;
          for (let t = 0; t < 2 + Math.floor(r() * 2); t++) {
            tw *= 0.72; td *= 0.72;
            const th = 10 + r() * 18;
            box(x, y + th / 2, z, tw, th, td, t % 2 ? c : col());
            y += th;
          }
          if (r() < 0.6) cyl(x, y + 8, z, 0.5, 16, 0xc9d6e8); // antenna
          towers.push({ x, z, h, w: bw, d: bd });
        } else if (r() < 0.35) {
          box(x + bw * 0.2, h + 2, z - bd * 0.2, 4, 4, 4, 0xa9c1d9); // rooftop box (cover)
        }
      }
    }
  }
  // sky bridges between neighbouring towers across a street
  for (let i = 0; i < towers.length; i++) {
    for (let j = i + 1; j < towers.length; j++) {
      const a = towers[i], b = towers[j];
      const dx = b.x - a.x, dz = b.z - a.z;
      const y = Math.min(a.h, b.h) * (0.45 + r() * 0.3);
      if (Math.abs(dz) < 6 && Math.abs(dx) > 40 && Math.abs(dx) < 70 && r() < 0.6) {
        box((a.x + b.x) / 2, y, (a.z + b.z) / 2, Math.abs(dx) - (a.w + b.w) / 2 + 1, 1.2, 5, 0xc9d6e8);
      } else if (Math.abs(dx) < 6 && Math.abs(dz) > 40 && Math.abs(dz) < 70 && r() < 0.6) {
        box((a.x + b.x) / 2, y, (a.z + b.z) / 2, 5, 1.2, Math.abs(dz) - (a.d + b.d) / 2 + 1, 0xc9d6e8);
      }
    }
  }
  // elevated highway along the street between the tower ring and the mid-rise (twin mirrors it), on pillars, with an on-ramp
  const hz = -LOT * 2.5, hy = 12;
  box(-HALF / 2, hy, hz, HALF - 4, 1.4, 14, 0xb8c7a8, 'rock');
  for (let x = -HALF + 20; x < -8; x += 34) cyl(x, hy / 2, hz, 1.6, hy, 0xd9d2c0);
  wedge(-HALF + 34, hy / 2, hz + 14, 12, hy, 24, 0);

  // central park: ~170 m of open ground — low walls, tree clumps, a monument; nothing tall except
  // the monument, so machines and dragons have room and the towers around it are the anchors
  const P = 1.5 * LOT; // park half size (84)
  for (const [x, z] of [[-40, -30], [30, -45], [-55, 10], [50, 5], [-20, -60], [10, -25]] as [number, number][]) {
    cyl(x, 3, z, 0.6, 6, 0xb3a58f);
    box(x, 7.5, z, 6, 3, 6, 0xb8c7a8);
  }
  for (const [x, z, sx, sz] of [[0, -18, 16, 1], [-30, 0, 1, 12], [35, -30, 10, 1], [-60, -50, 1, 14]] as [number, number, number, number][]) box(x, 0.8, z, sx, 1.6, sz, PALETTE[1], 'wall');
  wedge(-25, 1.5, -40, 8, 3, 10, Math.PI / 2);
  w.addCylinder(new Vector3(0, 7, 0), 2.4, 14, PALETTE[6], 'rock'); // monument (tether anchor in the middle)
  void P;
  // plazas: skate ramps + a stepped fountain block
  function buildPlaza(cx: number, cz: number) {
    wedge(cx - 10, 1.5, cz, 8, 3, 10, 0);
    wedge(cx + 10, 2.5, cz + 8, 8, 5, 12, Math.PI / 2);
    box(cx, 1, cz - 10, 12, 2, 12, PALETTE[3]);
    box(cx, 2.5, cz - 10, 6, 1, 6, PALETTE[6]);
  }

  info.spawn.set(0, 1, 190);
  info.yaw = 0; // facing -z down the boulevard, toward the park
  info.aims.spawn_b = new Vector3(0, 1, -190);
  info.aims.ground_ahead = new Vector3(0, 0, 178);
  info.aims.wall = new Vector3(40, 10, 150);
  // respawns (point-symmetric pairs, all on streets/open ground): boulevard ends, park corners, side
  // streets. Respawn picks the one farthest from the opponent; everyone faces the park.
  const face = (x: number, z: number) => Math.atan2(x, z);
  w.spawns = [[0, 190], [0, -190], [84, -84], [-84, 84], [140, 84], [-140, -84], [-196, 28], [196, -28]]
    .map(([x, z]) => ({ pos: new Vector3(x, 1, z), yaw: face(x, z) }));
  w.addEntity('dummy', new Vector3(0, 1.2, -190), { hostile: false, label: 'rival (practice dummy)' });
}
