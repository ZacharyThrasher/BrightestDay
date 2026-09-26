/**
 * Dev probe for the machine language layer: prints what each utterance proposes.
 *   npx tsx eval/probe_machines.ts [scene] "utterance" ["follow-up" ...]
 */
import { World } from '../src/sim/world.js';
import { buildScene, aimAt, type SceneName } from '../src/sim/levels.js';
import { NO_INPUT } from '../src/sim/player.js';
import { Director } from '../src/lang/director.js';
import type { ConstructSpec } from '../src/sim/constructs/spec.js';

export function specSummary(s: ConstructSpec): string {
  const beh = s.behaviors.map((b: any) => {
    switch (b.type) {
      case 'drive': return `drive(${b.mode},${Math.round(b.speed)}${b.altitude ? ',alt ' + b.altitude : ''})`;
      case 'brain': return `brain(${b.goal}${b.target ? ':' + (b.target.kind === 'entity' ? 'entity' : b.target.kind) : ''})`;
      case 'weapon': return `weapon(${b.kind}${b.group ? '@' + b.group : ''})`;
      case 'rideable': return 'rideable';
      default: return b.type;
    }
  });
  const joints = (s.joints ?? []).map((j) => `${j.group}:${j.motion.map((m) => m.kind).join('+')}`);
  return `[${s.concept}] "${s.label}" ×${s.count} @${s.placement.at} parts=${s.parts.length} beh=[${beh.join(', ')}] joints=[${joints.join(' ')}]`;
}

async function main() {
  const args = process.argv.slice(2);
  const scenes = ['sandbox', 'drones', 'arena', 'race', 'barrage', 'falling', 'gap', 'tower', 'boulder', 'horde'];
  const scene = (scenes.includes(args[0]) ? args.shift() : 'drones') as SceneName;
  const w = await World.create();
  const info = buildScene(w, scene);
  aimAt(w, info.aims.drone ?? info.aims.robot ?? info.aims.ground_ahead);
  const d = new Director(w);
  for (const u of args) {
    const r = await d.commit(u);
    console.log(`> ${u}\n  ${r.summary}${r.abstain ? '  (abstain: ' + r.abstain + ')' : ''}  conf=${r.confidence.toFixed(2)}`);
    for (const op of r.ops) {
      if (op.op === 'create' || op.op === 'modify') console.log(`  ${op.op}: ${specSummary(op.spec)}`);
      else console.log(`  ${JSON.stringify(op)}`);
    }
    const e: any = d.interp.lastEffect;
    console.log(`  fns=[${(e?.functions ?? []).join(',')}] intents=${(e?.intents ?? []).join('/')} targets=${(e?.targetKinds ?? []).join('/')} mods=[${(e?.modifications ?? []).join(',')}]`);
    if (r.messages.length) console.log(`  msgs: ${r.messages.join(' | ')}`);
    for (let i = 0; i < 30; i++) w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch });
  }
}
main();
