/**
 * Solution diversity: many deliberately different requests per challenge, judged ONLY by world state
 * (the interpreter's claims are ignored). A naive autopilot stands in for a human where the solution
 * needs the player to move (walk at the target, jump at edges, look where they were aiming).
 *
 *   npx tsx eval/diversity.ts [--verbose]
 */
import { Vector3 } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene, aimAt, type SceneName } from '../src/sim/levels.js';
import { NO_INPUT, type PlayerInput } from '../src/sim/player.js';
import { Director } from '../src/lang/director.js';
import { loadSemantic } from '../src/lang/semantic.js';

interface Challenge {
  scene: SceneName;
  aim: string;
  goal: string;
  solutions: string[];
  pilot: 'none' | 'walk' | 'walk_jump' | 'glide' | 'jet';
  seconds: number;
  success: (w: World, base: Baseline) => boolean;
}
interface Baseline { hits: number }

const CHALLENGES: Challenge[] = [
  {
    scene: 'gap', aim: 'far_ledge', goal: 'reach the far side of the canyon', pilot: 'walk_jump', seconds: 7,
    solutions: ['bridge across the gap', 'a catapult to fling me over', 'launch me across', 'a zipline to the other side', 'give me wings', 'a long ramp', 'a jetpack', 'a hoverboard', 'get me across'],
    success: (w) => w.goals.some((g) => g.reached),
  },
  {
    scene: 'tower', aim: 'far_ledge', goal: 'get onto the 40 m roof', pilot: 'walk_jump', seconds: 8,
    solutions: ['stairs up the tower', 'launch me up there', 'an elevator', 'a ladder', 'a grappling hook to the roof', 'a jetpack', 'a spring', 'a tornado to lift me', 'get me up there'],
    success: (w) => w.goals.some((g) => g.reached) || w.player.pos.y > 40,
  },
  {
    scene: 'falling', aim: 'civilian', goal: 'the falling civilian lands unhurt', pilot: 'none', seconds: 6,
    solutions: ['catch him', 'a net under him', 'a trampoline under the falling guy', 'a giant hand to grab him', 'a pile of pillows under him', 'a big bucket under him', 'a tornado under him', 'a bouncy castle under that guy', 'save him'],
    success: (w) => [...w.entities.values()].some((e) => e.type === 'civilian' && !e.hurt),
  },
  {
    scene: 'barrage', aim: 'turret', goal: 'take at most 1 hit in 5 s (baseline ≈ 5–6)', pilot: 'none', seconds: 5,
    solutions: ['wall between me and the turret', 'a dome over me', 'a shield that follows me', 'a fan to blow the shots away', 'a mirror that reflects the shots back', 'drop an anvil on the turret', 'three shields orbiting me', 'smash the turret with a giant hammer', 'block the bullets'],
    success: (w, b) => w.stats.playerHits <= 1 && b.hits >= 3,
  },
  {
    scene: 'drones', aim: 'drone', goal: 'the aimed drone is disabled or captured', pilot: 'none', seconds: 5,
    solutions: ['punch that drone', 'grab that drone', 'cage that drone', 'drop an anvil on it', 'shoot it', 'a magnet to pull it in', 'throw a boulder at it', 'a homing missile at the drone', 'swat it with a giant bat'],
    success: (w) => { const d = aimedDrone(w); return !!d && (d.disabled || !!d.heldBy || !!d.restrainedBy); },
  },
  {
    scene: 'boulder', aim: 'boulder', goal: 'boulder moved ≥3 m or player gets behind the door', pilot: 'walk_jump', seconds: 7,
    solutions: ['push the boulder out of the way', 'lift the boulder', 'grab the rock and throw it', 'launch me over the wall', 'a bulldozer to push the boulder', 'stairs over the wall', 'blow up the boulder', 'smash the boulder with a wrecking ball', 'a ramp over the wall'],
    success: (w) => w.goals.some((g) => g.reached) || boulderMoved(w) > 3,
  },
];

let droneId: string | null = null;
let boulderStart: Vector3 | null = null;
function aimedDrone(w: World) { return droneId ? w.entities.get(droneId) ?? null : null; }
function boulderMoved(w: World) { const b = [...w.entities.values()].find((e) => e.type === 'boulder'); return b && boulderStart ? w.entityPos(b).distanceTo(boulderStart) : 0; }

function pilotInput(w: World, mode: Challenge['pilot'], t: number, aimP: Vector3): Partial<PlayerInput> {
  const P = w.player;
  if (mode === 'none') return {};
  // face the objective horizontally, keep the original pitch
  const d = aimP.clone().sub(P.pos);
  const yaw = Math.atan2(-d.x, -d.z);
  const inp: Partial<PlayerInput> = { yaw, pitch: P.pitch, moveZ: 1 };
  // a person riding an elevator/lift stands still until it stops rising
  if (P.onGround && P.groundVel.y > 0.5) return { yaw, pitch: P.pitch };
  // naive: jump when on the ground and blocked or at an edge; hold jump in the air (jetpacks)
  const edge = P.onGround && !groundAhead(w);
  inp.jump = (P.onGround && (edge || Math.floor(t * 2) % 3 === 0)) || !P.onGround;
  // people use the boost thruster to stretch jumps toward where they're going
  if (!P.onGround && P.vel.y < 2 && P.gas > 30) inp.boost = true;
  // if something launched us toward the objective, keep aiming there
  return inp;
}
function groundAhead(w: World) {
  const P = w.player;
  const p = P.pos.clone().addScaledVector(P.forward(), 1.5);
  const hit = w.pw.castRay(new w.R.Ray(p, { x: 0, y: -1, z: 0 }), 3, true);
  return !!hit;
}

async function main() {
  const verbose = process.argv.includes('--verbose');
  const sem = await loadSemantic('models/semantic/bd-semantic-nb19-int8.bin');
  const rows: string[] = [];
  let total = 0, ok = 0;
  for (const ch of CHALLENGES) {
    // baseline: do nothing
    const base = await World.create();
    const bi = buildScene(base, ch.scene);
    for (let i = 0; i < ch.seconds * 60; i++) base.step({ ...NO_INPUT, yaw: base.player.yaw, pitch: base.player.pitch, ...pilotInput(base, ch.pilot, i / 60, bi.aims[ch.aim] ?? bi.aims.ground_ahead) });
    const baseline: Baseline = { hits: base.stats.playerHits };
    const baseOk = ch.success(base, baseline);
    let n = 0;
    const res: string[] = [];
    for (const sol of ch.solutions) {
      const w = await World.create();
      const info = buildScene(w, ch.scene);
      const d = new Director(w);
      if (sem) d.interp.embedder = sem;
      const aimP = info.aims[ch.aim] ?? info.aims.ground_ahead;
      aimAt(w, aimP);
      for (let i = 0; i < 6; i++) w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch });
      droneId = null;
      if (ch.aim === 'drone') { aimAt(w, aimP); const own = w.ownerOf(w.player.aim().collider); droneId = own?.kind === 'entity' ? own.id : [...w.entities.values()].find((e) => e.type === 'drone')?.id ?? null; }
      const b = [...w.entities.values()].find((e) => e.type === 'boulder');
      boulderStart = b ? w.entityPos(b) : null;
      aimAt(w, ch.aim === 'civilian' ? w.entityPos([...w.entities.values()].find((e) => e.type === 'civilian')!) : aimP);
      const r = await d.commit(sol);
      const pitch0 = w.player.pitch;
      for (let i = 0; i < ch.seconds * 60; i++) {
        const inp = pilotInput(w, ch.pilot, i / 60, aimP);
        if (ch.pilot !== 'none') inp.pitch = ch.scene === 'gap' ? -0.1 : pitch0;
        w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch, ...inp });
      }
      const success = ch.success(w, baseline);
      if (success) n++;
      res.push(`${success ? '✔' : '✘'} ${sol}  →  ${r.summary}${r.abstain ? ' (abstain)' : ''}`);
    }
    total += ch.solutions.length;
    ok += n;
    rows.push(`| ${ch.scene} | ${ch.goal} | ${baseOk ? 'yes' : 'no'} | **${n}/${ch.solutions.length}** |`);
    console.log(`\n## ${ch.scene}: ${ch.goal} — ${n}/${ch.solutions.length} (baseline ${baseOk ? 'already succeeds!' : 'fails'})`);
    if (verbose) for (const x of res) console.log('  ' + x);
    else for (const x of res) if (x.startsWith('✘')) console.log('  ' + x);
  }
  console.log(`\n| scene | goal | do-nothing baseline succeeds? | distinct solutions that worked |\n|---|---|---|---|\n${rows.join('\n')}\n\nTOTAL ${ok}/${total}`);
}

main();
