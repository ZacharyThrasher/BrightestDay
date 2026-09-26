/** Dev probe with a fake opponent lantern (PvP references).  npx tsx eval/probe_pvp.ts "utterance" ... */
import { Vector3 } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene, aimAt } from '../src/sim/levels.js';
import { Director } from '../src/lang/director.js';
import { specSummary } from './probe_machines.js';

async function main() {
  const w = await World.create();
  const info = buildScene(w, 'arena');
  (w as any).opponent = { id: 'p2', pos: new Vector3(0, 1, -25) };
  aimAt(w, info.aims.ground_ahead);
  const d = new Director(w);
  for (const u of process.argv.slice(2)) {
    const r = await d.commit(u);
    console.log(`> ${u}\n  ${r.summary}${r.abstain ? '  (abstain: ' + r.abstain + ')' : ''}`);
    for (const op of r.ops) console.log('  ' + (op.op === 'create' || op.op === 'modify' ? `${op.op}: ${specSummary(op.spec)} ${JSON.stringify(op.spec.behaviors.map((b: any) => b.target ?? b.around).filter(Boolean))}` : JSON.stringify(op)));
    if (r.messages.length) console.log(`  msgs: ${r.messages.join(' | ')}`);
  }
}
main();
