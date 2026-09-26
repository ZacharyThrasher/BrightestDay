/**
 * Evaluation runner.
 *   npx tsx eval/run.ts eval/dev.jsonl [--verbose] [--only ID] [--out results/name]
 *
 * For each item: fresh world in the item's scene, crosshair on the item's aim, context
 * utterances committed (and simulated), then the scored utterance is committed and the world
 * simulated for a few seconds. We score the interpreter's CLAIMED functional effects against
 * the frozen contract (eval/EVAL_SPEC.md) and separately record OBSERVED world effects so claim
 * and effect can be compared.
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { Vector3 } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene, aimAt, type SceneName } from '../src/sim/levels.js';
import { NO_INPUT } from '../src/sim/player.js';
import { Director } from '../src/lang/director.js';
import type { EffectReport } from '../src/lang/types.js';
import { loadSemantic, type SemanticIndex } from '../src/lang/semantic.js';

import { writeFileSync as wfs, existsSync } from 'node:fs';
import { RecordProvider, ReplayProvider, type S2Provider } from '../src/lang/escalate.js';
let SEM: SemanticIndex | null = null;
let S2: S2Provider | null = null;
let S2MODE: 'hybrid' | 'always' = 'hybrid';
export const recorder = new RecordProvider();
export async function enableSemantic(on: boolean) { SEM = on ? await loadSemantic('models/semantic/bd-semantic-nb19-int8.bin') : null; }

export interface Item {
  id: string; category: string; scene: SceneName; aim: string; context: string[]; utterance: string;
  expect: {
    intent: string | null; functions_any: string[]; functions_all: string[]; target: string | null; count_min: number | null; count_max: number | null;
    modifications_any: string[]; approximation_ok: boolean; must_clamp: boolean; must_abstain: boolean;
  };
  note?: string;
}

export interface ItemResult {
  id: string; category: string; scene: string; utterance: string; pass: boolean; reasons: string[];
  report: EffectReport & { intents?: string[]; targetKinds?: string[] }; observed: string[]; summary: string; abstain: string | null; confidence: number;
  latencyMs: number; confidentWrong: boolean; messages: string[]; escalated?: boolean; needsEscalation?: boolean;
}

function stepN(w: World, seconds: number) {
  const n = Math.round(seconds / w.dt);
  for (let i = 0; i < n; i++) w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch });
}

function snapshot(w: World) {
  const ents = new Map<string, { pos: Vector3; hp: number; hurt: boolean; safe: boolean; held: string | null; restrained: string | null }>();
  for (const e of w.entities.values()) ents.set(e.id, { pos: w.entityPos(e), hp: e.hp, hurt: e.hurt, safe: e.safe, held: e.heldBy, restrained: e.restrainedBy });
  return { ents, blocked: w.stats.projectilesBlocked, reflected: w.stats.projectilesReflected, hits: w.stats.playerHits, pos: w.player.pos.clone(), goals: w.goals.filter((g) => g.reached).length, constructs: w.constructs.list.size, speed: w.player.vel.length() };
}

export async function runItem(item: Item): Promise<ItemResult> {
  const w = await World.create();
  const info = buildScene(w, item.scene);
  const d = new Director(w);
  if (SEM) d.interp.embedder = SEM;
  if (S2) { d.interp.s2 = S2; d.interp.s2Mode = S2MODE; }
  const staticAim = info.aims[item.aim] ?? (item.aim === 'none' ? w.player.pos.clone().add(new Vector3(0, 1.5, -30)) : info.aims.ground_ahead);
  // entities move (falling civilians, hovering drones): keep the crosshair on the named thing
  const aimTargetFn = () => {
    const kind = item.aim === 'civilian' ? 'civilian' : item.aim === 'drone' ? 'drone' : item.aim === 'boulder' ? 'boulder' : item.aim === 'turret' ? 'turret' : null;
    if (!kind) return staticAim;
    const es = [...w.entities.values()].filter((e) => e.type === kind && !e.heldBy);
    if (!es.length) return staticAim;
    const e = es.sort((a, b) => w.entityPos(a).distanceTo(staticAim) - w.entityPos(b).distanceTo(staticAim))[0];
    return w.entityPos(e);
  };
  let aimTarget = aimTargetFn();
  aimAt(w, aimTarget);
  stepN(w, 0.2);
  for (const c of item.context) {
    aimTarget = aimTargetFn();
    aimAt(w, aimTarget);
    const keep = d.interp.s2Mode;
    d.interp.s2Mode = 'off';
    await d.commit(c);
    d.interp.s2Mode = keep;
    stepN(w, 1.2);
  }
  aimTarget = aimTargetFn();
  aimAt(w, aimTarget);
  const before = snapshot(w);
  let maxY = w.player.pos.y;
  let maxSpeed = 0;
  d.interp.s2Key = item.id;
  const res = await d.commit(item.utterance);
  const rep = { ...(d.interp.lastEffect as any) };
  const heldDuring = new Set<string>();
  const restrainedDuring = new Set<string>();
  for (let i = 0; i < 180; i++) {
    w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch, moveZ: item.scene === 'gap' ? 1 : 0 });
    maxY = Math.max(maxY, w.player.pos.y);
    maxSpeed = Math.max(maxSpeed, w.player.vel.length());
    for (const e of w.entities.values()) { if (e.heldBy) heldDuring.add(e.id); if (e.restrainedBy) restrainedDuring.add(e.id); }
  }
  const after = snapshot(w);
  // observed effects (independent of what the interpreter claimed)
  const obs = new Set<string>();
  if (after.blocked > before.blocked) obs.add('block_projectiles');
  if (after.reflected > before.reflected) obs.add('reflect');
  if (maxY - before.pos.y > 3) obs.add('raise_player');
  if (maxSpeed > before.speed + 8) obs.add('launch_player');
  if (after.goals > before.goals) obs.add('goal_reached');
  for (const [id, b] of before.ents) {
    const a = after.ents.get(id);
    if (!a) continue;
    if (heldDuring.has(id) && !b.held) obs.add('grab_target');
    if (restrainedDuring.has(id)) obs.add('restrain_target');
    if (a.hp < b.hp - 1) obs.add('damage_target');
    if (a.pos.distanceTo(b.pos) > 3 && !heldDuring.has(id)) obs.add('moved_target');
    if (!a.hurt && (a.safe || heldDuring.has(id)) && item.scene === 'falling') obs.add('catch_target');
    if (a.hurt && !b.hurt) obs.add('target_hurt');
  }
  if (after.constructs > before.constructs) obs.add('construct_created');

  const e = item.expect;
  const reasons: string[] = [];
  const intents: string[] = rep.intents ?? [rep.intent];
  const abstained = !!rep.abstained;
  if (e.must_abstain) {
    if (!abstained) reasons.push('should have abstained');
  } else {
    if (e.intent) {
      const alts = e.intent.split('|');
      if (alts.includes('abstain')) { if (!abstained) reasons.push('expected abstain'); }
      else if (!alts.some((a) => intents.includes(a))) reasons.push(`intent ${intents.join('/') || 'none'} ≠ ${e.intent}`);
    }
    if (abstained && !(e.intent ?? '').includes('abstain')) reasons.push('abstained: ' + (res.abstain ?? ''));
    const fns = new Set<string>(rep.functions ?? []);
    if (e.functions_any?.length && !e.functions_any.some((f) => fns.has(f))) reasons.push(`functions_any miss (${e.functions_any.join(',')}) got [${[...fns].join(',')}]`);
    for (const f of e.functions_all ?? []) if (!fns.has(f)) reasons.push(`missing ${f}`);
    if (e.target) {
      const tk: string[] = rep.targetKinds ?? (rep.target ? [rep.target] : []);
      const ok = tk.includes(e.target) || (e.target === 'nearest_enemy' && tk.includes('aimed_entity')) || (e.target === 'aimed_entity' && tk.includes('nearest_enemy') && item.aim === 'drone') || (e.target === 'previous_construct' && (rep.intent === 'modify' || intents.includes('modify'))) || (e.target === 'none' && tk.length === 0) || (e.target === 'self' && (tk.includes('self') || (rep.functions ?? []).some((f: string) => ['shield_self', 'launch_player', 'raise_player', 'propel_player', 'slow_fall', 'support_player', 'tether_player'].includes(f))));
      if (!ok) reasons.push(`target ${tk.join('/') || 'none'} ≠ ${e.target}`);
    }
    if (e.count_min !== null && e.count_min !== undefined && (rep.count ?? 0) < e.count_min) reasons.push(`count ${rep.count} < ${e.count_min}`);
    if (e.count_max !== null && e.count_max !== undefined && (rep.count ?? 0) > e.count_max) reasons.push(`count ${rep.count} > ${e.count_max}`);
    if (e.modifications_any?.length && !e.modifications_any.some((m) => (rep.modifications ?? []).includes(m))) reasons.push(`mods miss (${e.modifications_any.join(',')}) got [${(rep.modifications ?? []).join(',')}]`);
    if (e.must_clamp && !rep.clamped) reasons.push('should have clamped');
  }
  const pass = reasons.length === 0;
  return {
    id: item.id, category: item.category, scene: item.scene, utterance: item.utterance, pass, reasons, report: rep, observed: [...obs], summary: res.summary,
    abstain: res.abstain, confidence: res.confidence, latencyMs: res.latencyMs, confidentWrong: !pass && !abstained && res.confidence >= 0.7, messages: res.messages,
    escalated: res.escalated, needsEscalation: res.needsEscalation,
  };
}

export function summarize(results: ItemResult[]) {
  const byCat = new Map<string, { n: number; pass: number }>();
  for (const r of results) {
    const c = byCat.get(r.category) ?? { n: 0, pass: 0 };
    c.n++; if (r.pass) c.pass++;
    byCat.set(r.category, c);
  }
  const n = results.length;
  const pass = results.filter((r) => r.pass).length;
  const cw = results.filter((r) => r.confidentWrong).length;
  const abst = results.filter((r) => r.abstain).length;
  const lat = results.map((r) => r.latencyMs).sort((a, b) => a - b);
  // claim vs observed agreement for claims we can observe
  const observable: Record<string, string[]> = { block_projectiles: ['block_projectiles'], grab_target: ['grab_target'], restrain_target: ['restrain_target', 'grab_target'], strike_target: ['damage_target', 'moved_target'], crush_target: ['damage_target', 'moved_target'], catch_target: ['catch_target'], raise_player: ['raise_player', 'goal_reached'], launch_player: ['launch_player', 'raise_player'], move_obstacle: ['moved_target'], bridge_gap: ['goal_reached'] };
  let claimN = 0, claimOk = 0;
  const claimMiss: Record<string, number> = {};
  for (const r of results) {
    for (const f of r.report.functions ?? []) {
      const need = observable[f];
      if (!need) continue;
      // only count claims whose preconditions exist in the scene
      if (f === 'block_projectiles' && r.scene !== 'barrage' && r.scene !== 'drones') continue;
      if (f === 'catch_target' && r.scene !== 'falling') continue;
      if (f === 'move_obstacle' && r.scene !== 'boulder') continue;
      if (f === 'bridge_gap' && r.scene !== 'gap') continue;
      if (['grab_target', 'restrain_target', 'strike_target', 'crush_target'].includes(f) && !['drones', 'barrage', 'boulder', 'falling', 'arena'].includes(r.scene)) continue;
      claimN++;
      if (need.some((o) => r.observed.includes(o))) claimOk++; else claimMiss[f] = (claimMiss[f] ?? 0) + 1;
    }
  }
  return {
    n, pass, rate: pass / n, confidentWrong: cw, confidentWrongRate: cw / n, abstentions: abst,
    byCategory: Object.fromEntries([...byCat].map(([k, v]) => [k, `${v.pass}/${v.n}`])),
    latencyMs: { p50: lat[Math.floor(n * 0.5)], p95: lat[Math.floor(n * 0.95)], max: lat[n - 1] },
    claimVsObserved: { claims: claimN, observed: claimOk, rate: claimN ? claimOk / claimN : null, misses: claimMiss },
  };
}

async function main() {
  const args = process.argv.slice(2);
  const file = args.find((a) => !a.startsWith('--')) ?? 'eval/dev.jsonl';
  const verbose = args.includes('--verbose');
  const only = args.includes('--only') ? args[args.indexOf('--only') + 1] : null;
  const quiet = args.includes('--quiet'); // aggregate only (for sets the implementer must not inspect)
  const out = args.includes('--out') ? args[args.indexOf('--out') + 1] : null;
  await enableSemantic(!args.includes('--no-sem'));
  const s2arg = args.includes('--s2') ? args[args.indexOf('--s2') + 1] : null;
  if (args.includes('--s2mode')) S2MODE = args[args.indexOf('--s2mode') + 1] as any;
  if (s2arg?.startsWith('record:')) { S2 = recorder; S2MODE = 'always'; }
  if (s2arg?.startsWith('replay:')) {
    const f = s2arg.slice(7);
    const m = new Map<string, string>();
    for (const l of readFileSync(f, 'utf8').split('\n').filter((x) => x.trim())) { const j = JSON.parse(l); m.set(j.key, typeof j.response === 'string' ? j.response : JSON.stringify(j.response)); }
    S2 = new ReplayProvider(m);
  }
  const items: Item[] = readFileSync(file, 'utf8').split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l)).filter((i: Item) => !only || i.id === only);
  const results: ItemResult[] = [];
  if (args.includes('--compare') && S2) {
    const keepS2 = S2;
    const rows: { s1: boolean; s2: boolean; agree: boolean; cat: string; s1conf: number; s2conf: number }[] = [];
    for (const it of items) {
      S2 = null; const a = await runItem(it);
      S2 = keepS2; S2MODE = 'always'; const b = await runItem(it);
      const ia = new Set<string>((a.report as any).intents ?? []), ib = new Set<string>((b.report as any).intents ?? []);
      const fa = new Set<string>(a.report.functions ?? []), fb = new Set<string>(b.report.functions ?? []);
      const intentAgree = [...ia].some((x) => ib.has(x));
      const fnAgree = (fa.size === 0 && fb.size === 0) || [...fa].some((x) => fb.has(x));
      rows.push({ s1: a.pass, s2: b.pass, agree: intentAgree && fnAgree && !!a.abstain === !!b.abstain, cat: it.category, s1conf: a.confidence, s2conf: b.confidence });
    }
    const agree = rows.filter((r) => r.agree), dis = rows.filter((r) => !r.agree);
    const pct = (x: number, n: number) => `${x}/${n} (${n ? Math.round((100 * x) / n) : 0}%)`;
    console.log(JSON.stringify({
      n: rows.length,
      agree: agree.length,
      whenAgree_S1pass: pct(agree.filter((r) => r.s1).length, agree.length),
      whenAgree_S2pass: pct(agree.filter((r) => r.s2).length, agree.length),
      whenDisagree_S1pass: pct(dis.filter((r) => r.s1).length, dis.length),
      whenDisagree_S2pass: pct(dis.filter((r) => r.s2).length, dis.length),
      oracle_either: pct(rows.filter((r) => r.s1 || r.s2).length, rows.length),
      policy_S2_overrides_on_disagreement: pct(rows.filter((r) => (r.agree ? r.s1 : r.s2)).length, rows.length),
    }, null, 2));
    return;
  }
  for (const it of items) {
    const r = await runItem(it);
    results.push(r);
    if (!quiet && (verbose || !r.pass)) {
      console.log(`${r.pass ? 'PASS' : 'FAIL'} ${r.id} [${r.category}/${r.scene}] "${r.utterance}"`);
      console.log(`     → ${r.summary}${r.abstain ? '  (abstain: ' + r.abstain + ')' : ''}  conf=${r.confidence.toFixed(2)}`);
      console.log(`     fns=[${(r.report.functions ?? []).join(',')}] intents=${(r.report.intents ?? []).join('/')} targets=${(r.report.targetKinds ?? []).join('/')} count=${r.report.count} mods=[${(r.report.modifications ?? []).join(',')}] obs=[${r.observed.join(',')}]`);
      if (!r.pass) console.log(`     ✗ ${r.reasons.join('; ')}`);
    }
  }
  const s: any = summarize(results);
  if (s2arg?.startsWith('record:')) {
    const f = s2arg.slice(7);
    wfs(f, JSON.stringify({ system: recorder.requests[0]?.system ?? '' }) + '\n' + recorder.requests.map((r) => JSON.stringify({ key: r.key, user: r.user })).join('\n') + '\n');
    console.log(`recorded ${recorder.requests.length} System-2 requests to ${f}`);
  }
  s.escalated = results.filter((r: any) => r.escalated).length;
  s.wouldEscalate = results.filter((r: any) => r.needsEscalation).length;
  console.log(JSON.stringify(s, null, 2));
  if (out) {
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out + '.summary.json', JSON.stringify(s, null, 2));
    if (!quiet) writeFileSync(out + '.items.json', JSON.stringify(results, null, 2));
    else writeFileSync(out + '.failures_by_category.json', JSON.stringify(Object.fromEntries(Object.entries(s.byCategory)), null, 2));
  }
}

if (process.argv[1] && /eval[\\/]run\.ts$/.test(process.argv[1])) main();
