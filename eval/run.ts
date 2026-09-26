/**
 * Evaluation runner.
 *   npx tsx eval/run.ts eval/dev.jsonl [--verbose] [--only ID] [--out results/name] [--jobs N]
 *                      [--window S] [--phases expected|all|none] [--score observed|claimed] [--quiet [--by-category]]
 *
 * For each item: fresh world in the item's scene, crosshair on the item's aim, context
 * utterances committed (and simulated), then the scored utterance is committed and the world
 * simulated for the scored window (default 8 s, --window). Classic functions (eval/EVAL_SPEC.md)
 * are scored from the interpreter's CLAIMED effects; machine functions (eval/EVAL_SPEC_MACHINES.md)
 * are scored from what is OBSERVED in the world (eval/observe.ts), attributed to the constructs
 * the scored utterance created / modified / ordered / rode. Claims and observations are both
 * reported so over- and under-claims are visible.
 *
 * Scripted phases after the window (only for items that expect the function, or --phases all):
 *   follow — the player dismounts, waits 2 s, then walks ~20 m (for follows_player)
 *   ride   — the harness seats the player in the construct and holds forward, pulling the trigger at the
 *            nearest target for the second half (rideable / carries_player; player-operated weapons)
 *
 * --quiet prints aggregate numbers only (held-out sets); --by-category adds per-category and
 * per-scene pass rates, still without items or failure reasons. --jobs N splits the items over
 * N worker processes.
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { Vector3 } from 'three';
import { World, type Entity } from '../src/sim/world.js';
import type { Construct } from '../src/sim/constructs/runtime.js';
import { MachineObserver, MACHINE_FN_SET, MACHINE_FNS } from './observe.js';
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
  /** grouping keys for the failure-reason table (one per reason) */
  reasonKeys?: string[];
  /** machine functions (EVAL_SPEC_MACHINES.md) observed from world state, with the evidence for each */
  machineObserved?: string[]; evidence?: Record<string, string>;
  /** machine functions claimed by the interpreter but not observed / observed but not claimed */
  overclaims?: string[]; unclaimedObserved?: string[];
  /** functions whose absence failed the item: f → 'claimed' (claimed, not observed) | 'unclaimed' */
  fnMisses?: Record<string, 'claimed' | 'unclaimed'>;
  /** constructs effects were attributed to; scripted phases that ran */
  attributed?: string[]; phases?: string[];
}

/** Harness options (CLI: --window S, --phases expected|all|none). */
export const OPTS = {
  /** scored window after the utterance, seconds (EVAL_SPEC_MACHINES.md default: 8) */
  window: 8,
  /** follow phase: the player waits followWait (unobserved), walks (moveZ = followMoveZ) followWalk, then stands followRest */
  followWait: 2, followWalk: 3.5, followRest: 1.5, followMoveZ: 0.6,
  /** ride phase: the harness seats the player in the construct and holds forward this long */
  ride: 5,
  /** which scripted phases run: 'expected' (only when the item expects follows_player / rideable / carries_player), 'all', 'none' */
  phases: 'expected' as 'expected' | 'all' | 'none',
  /** machine functions scored from what is observed (default) or, for comparison, from what the interpreter claims */
  score: 'observed' as 'observed' | 'claimed',
};

const ENTITY_AIMS = ['civilian', 'drone', 'boulder', 'turret', 'robot', 'crate'];

/** Function families, to group functions_any misses in the failure table. */
const FAMILY: [string, string[]][] = [
  ['combat', ['attacks_enemies', 'damages_target', 'destroys_target', 'shoots', 'aims_at_target', 'projectile_attack', 'strike_target', 'area_attack', 'throw_target', 'crush_target']],
  ['riding', ['rideable', 'carries_player', 'propel_player']],
  ['locomotion', ['moves_autonomously', 'drives_on_ground', 'walks', 'flies', 'wheels_spin', 'has_moving_parts', 'orbit', 'spin']],
  ['companion', ['follows_player', 'guards_area', 'fetches', 'returns', 'shield_other']],
  ['obstacle', ['pushes_objects', 'move_obstacle', 'push_target']],
  ['rescue', ['catch_target', 'grab_target', 'restrain_target']],
  ['traversal', ['raise_player', 'support_player', 'bridge_gap', 'launch_player', 'slow_fall', 'tether_player']],
  ['defence', ['block_projectiles', 'shield_self']],
];
function familyOf(fs: string[]): string {
  const n = new Map<string, number>();
  for (const f of fs) { const fam = FAMILY.find(([, l]) => l.includes(f))?.[0] ?? f; n.set(fam, (n.get(fam) ?? 0) + 1); }
  return [...n].sort((a, b) => b[1] - a[1])[0]?.[0] ?? '?';
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
  const aimEntity = (): Entity | null => {
    if (!ENTITY_AIMS.includes(item.aim)) return null;
    const es = [...w.entities.values()].filter((e) => e.type === item.aim && !e.heldBy);
    if (!es.length) return null;
    return es.sort((a, b) => w.entityPos(a).distanceTo(staticAim) - w.entityPos(b).distanceTo(staticAim))[0];
  };
  const aimTargetFn = () => {
    if (item.aim === 'construct') {
      // the player's most recent construct
      const cs = [...w.constructs.list.values()];
      const c = cs[cs.length - 1];
      return c?.insts[0] ? c.insts[0].pos.clone() : staticAim;
    }
    const e = aimEntity();
    return e ? w.entityPos(e) : staticAim;
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
  // the target of damages_target / destroys_target: the aimed entity, or (nothing aimed at) every hostile
  const aimed = aimEntity();
  const hostileIds = [...w.entities.values()].filter((x) => x.hostile).map((x) => x.id);
  const targetIds = aimed ? [aimed.id] : hostileIds;
  // aims_at_target also accepts another enemy of the aimed kind (a turret picks the nearest drone)
  const aimTargetIds = aimed ? (aimed.hostile ? [...w.entities.values()].filter((x) => x.hostile && x.type === aimed.type).map((x) => x.id) : [aimed.id]) : hostileIds;
  const before = snapshot(w);
  let maxY = w.player.pos.y;
  let maxSpeed = 0;
  d.interp.s2Key = item.id;
  const mo = new MachineObserver(w, { targetIds: () => targetIds, aimTargetIds: () => aimTargetIds });
  const res = await d.commit(item.utterance);
  const rep = { ...(d.interp.lastEffect as any) };
  // attribution: constructs this utterance created, modified, ordered, acted with or rode
  const touched = new Set<string>(res.constructIds ?? []);
  for (const op of res.ops ?? []) {
    if ((op.op === 'modify' || op.op === 'act') && typeof op.target === 'string') touched.add(op.target);
    if (op.op === 'control' && op.target) touched.add(op.target);
    if (op.op === 'ride' && op.target.kind === 'construct') touched.add(op.target.id);
  }
  mo.attribute(touched);
  const heldDuring = new Set<string>();
  const restrainedDuring = new Set<string>();
  const P = w.player;
  // ---- main window: no input (legacy: in `gap` the player keeps walking forward)
  mo.begin('main');
  const nMain = Math.round(OPTS.window / w.dt);
  for (let i = 0; i < nMain; i++) {
    const inp = { ...NO_INPUT, yaw: P.yaw, pitch: P.pitch, moveZ: item.scene === 'gap' ? 1 : 0 };
    w.step(inp);
    mo.tick(inp);
    maxY = Math.max(maxY, P.pos.y);
    maxSpeed = Math.max(maxSpeed, P.vel.length());
    for (const x of w.entities.values()) { if (x.heldBy) heldDuring.add(x.id); if (x.restrainedBy) restrainedDuring.add(x.id); }
  }
  mo.end();
  const after = snapshot(w);
  const e = item.expect;
  const expects = (...fs: string[]) => fs.some((f) => (e.functions_all ?? []).includes(f) || (e.functions_any ?? []).includes(f));
  const phases: string[] = [];
  // ---- follow phase: the player gets off whatever they ride, waits for company ("heel"), then walks ≥ 15 m
  // (toward +z: open ground in every scene). Only the walk (and the rest after it) is observed.
  if (OPTS.phases === 'all' || (OPTS.phases === 'expected' && expects('follows_player'))) {
    phases.push('follow');
    if (P.riding) P.dismount();
    const yaw = Math.PI;
    const still = { ...NO_INPUT, yaw, pitch: 0 };
    for (let i = 0, n = Math.round(OPTS.followWait / w.dt); i < n; i++) { w.step(still); mo.tick(still); }
    mo.begin('follow');
    const nWalk = Math.round(OPTS.followWalk / w.dt), n = nWalk + Math.round(OPTS.followRest / w.dt);
    for (let i = 0; i < n; i++) {
      const inp = { ...NO_INPUT, yaw, pitch: 0, moveZ: i < nWalk ? OPTS.followMoveZ : 0 };
      w.step(inp);
      mo.tick(inp);
    }
    mo.end();
  }
  // ---- ride phase: the harness seats the player in the construct ("get in") and holds forward; for the
  // second half it also points the crosshair at the nearest target and holds fire (player-operated weapons)
  if (OPTS.phases === 'all' || (OPTS.phases === 'expected' && expects('rideable', 'carries_player'))) {
    phases.push('ride');
    const alive = [...mo.attributed].map((id) => w.constructs.get(id)).filter((c): c is Construct => !!c && !!c.insts[0]?.alive);
    const pick = (P.riding && mo.attributed.has(P.riding.id) ? w.constructs.get(P.riding.id) : null) ?? alive.find((c) => !!c.mach?.ride) ?? alive[0] ?? null;
    if (pick) {
      if (P.riding && P.riding.id !== pick.id) P.dismount();
      if (!P.riding) P.ride(pick.id);
    }
    const inst = pick?.insts[0];
    const f = inst ? new Vector3(0, 0, 1).applyQuaternion(inst.quat) : P.forward();
    let yaw = Math.atan2(-f.x, -f.z); // face where the machine faces (air machines fly toward the camera heading)
    let pitch = 0;
    const air = pick?.mach?.drive?.mode === 'air';
    const p0 = inst ? inst.pos.clone() : P.pos.clone();
    mo.begin('ride');
    const n = Math.round(OPTS.ride / w.dt);
    let steer = 0;
    for (let i = 0; i < n; i++) {
      if (i === Math.round(n / 2) && inst && inst.pos.distanceTo(p0) < 1) steer = 1; // stuck against something: turn as well
      const fire = i >= n / 2;
      if (fire) {
        const tps = aimTargetIds.map((id) => w.entities.get(id)).filter((x): x is Entity => !!x && !x.disabled).map((x) => w.entityPos(x));
        tps.sort((a, b) => a.distanceTo(P.pos) - b.distanceTo(P.pos));
        if (tps[0]) ({ yaw, pitch } = aimAt(w, tps[0]));
      }
      const inp = { ...NO_INPUT, yaw, pitch, moveZ: 1, moveX: steer, jump: air && i * w.dt < 1.2, hookL: fire };
      w.step(inp);
      mo.tick(inp);
    }
    mo.end();
  }
  const mres = mo.result();
  mo.detach();
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
  for (const f of mres.observed) obs.add(f);

  const reasons: string[] = [];
  const keys: string[] = [];
  const fail = (text: string, key: string) => { reasons.push(text); keys.push(key); };
  const intents: string[] = rep.intents ?? [rep.intent];
  const abstained = !!rep.abstained;
  const claimed = new Set<string>(rep.functions ?? []);
  // machine functions (EVAL_SPEC_MACHINES.md) are scored from what was OBSERVED; the classic ones from the claim
  // (--score claimed reproduces the previous claim-only scoring, for comparison)
  const exhibited = (f: string) => (MACHINE_FN_SET.has(f) && OPTS.score === 'observed' ? mres.observed.has(f) : claimed.has(f));
  const how = (f: string) => (MACHINE_FN_SET.has(f) ? (claimed.has(f) ? 'claimed, not observed' : 'not claimed, not observed') : 'not claimed');
  const fnMisses: Record<string, 'claimed' | 'unclaimed'> = {};
  if (e.must_abstain) {
    if (!abstained) fail('should have abstained', 'should have abstained');
  } else {
    if (e.intent) {
      const alts = e.intent.split('|');
      if (alts.includes('abstain')) { if (!abstained) fail('expected abstain', 'should have abstained'); }
      else if (!alts.some((a) => intents.includes(a))) fail(`intent ${intents.join('/') || 'none'} ≠ ${e.intent}`, `intent ≠ ${e.intent}`);
    }
    if (abstained && !(e.intent ?? '').includes('abstain')) fail('abstained: ' + (res.abstain ?? ''), 'abstained (should act)');
    if (e.functions_any?.length && !e.functions_any.some(exhibited)) {
      const overclaimed = e.functions_any.filter((f) => MACHINE_FN_SET.has(f) && claimed.has(f));
      const key = overclaimed.length ? `none of [${familyOf(e.functions_any)}] — claimed ${overclaimed.join('/')}, not observed` : `none of [${familyOf(e.functions_any)}] — not claimed`;
      fail(`functions_any miss (${e.functions_any.join(',')}) claimed [${[...claimed].join(',')}] observed [${[...mres.observed].join(',')}]`, key);
      for (const f of e.functions_any) fnMisses[f] = MACHINE_FN_SET.has(f) && claimed.has(f) ? 'claimed' : 'unclaimed';
    }
    for (const f of e.functions_all ?? []) {
      if (exhibited(f)) continue;
      fail(`missing ${f} (${how(f)})`, `missing ${f} (${how(f)})`);
      fnMisses[f] = MACHINE_FN_SET.has(f) && claimed.has(f) ? 'claimed' : 'unclaimed';
    }
    if (e.target) {
      const tk: string[] = rep.targetKinds ?? (rep.target ? [rep.target] : []);
      const ok = tk.includes(e.target) || (e.target === 'nearest_enemy' && tk.includes('aimed_entity')) || (e.target === 'aimed_entity' && tk.includes('nearest_enemy') && item.aim === 'drone') || (e.target === 'previous_construct' && (rep.intent === 'modify' || intents.includes('modify'))) || (e.target === 'none' && tk.length === 0) || (e.target === 'self' && (tk.includes('self') || (rep.functions ?? []).some((f: string) => ['shield_self', 'launch_player', 'raise_player', 'propel_player', 'slow_fall', 'support_player', 'tether_player'].includes(f))));
      if (!ok) fail(`target ${tk.join('/') || 'none'} ≠ ${e.target}`, `target ≠ ${e.target}`);
    }
    if (e.count_min !== null && e.count_min !== undefined && (rep.count ?? 0) < e.count_min) fail(`count ${rep.count} < ${e.count_min}`, 'count too low');
    if (e.count_max !== null && e.count_max !== undefined && (rep.count ?? 0) > e.count_max) fail(`count ${rep.count} > ${e.count_max}`, 'count too high');
    if (e.modifications_any?.length && !e.modifications_any.some((m) => (rep.modifications ?? []).includes(m))) fail(`mods miss (${e.modifications_any.join(',')}) got [${(rep.modifications ?? []).join(',')}]`, `modifications miss (${e.modifications_any.join('|')})`);
    if (e.must_clamp && !rep.clamped) fail('should have clamped', 'should have clamped');
  }
  const pass = reasons.length === 0;
  const claimedM = [...claimed].filter((f) => MACHINE_FN_SET.has(f));
  return {
    id: item.id, category: item.category, scene: item.scene, utterance: item.utterance, pass, reasons, report: rep, observed: [...obs], summary: res.summary,
    abstain: res.abstain, confidence: res.confidence, latencyMs: res.latencyMs, confidentWrong: !pass && !abstained && res.confidence >= 0.7, messages: res.messages,
    escalated: res.escalated, needsEscalation: res.needsEscalation,
    reasonKeys: keys, machineObserved: [...mres.observed], evidence: mres.evidence,
    overclaims: claimedM.filter((f) => !mres.observed.has(f) && (!PHASE_OF[f] || phases.includes(PHASE_OF[f]))), unclaimedObserved: [...mres.observed].filter((f) => !claimed.has(f)),
    fnMisses, attributed: [...mo.attributed], phases,
  };
}

function tally<K extends string>(results: ItemResult[], key: (r: ItemResult) => K) {
  const m = new Map<K, { n: number; pass: number }>();
  for (const r of results) {
    const k = key(r);
    const c = m.get(k) ?? { n: 0, pass: 0 };
    c.n++; if (r.pass) c.pass++;
    m.set(k, c);
  }
  return m;
}

export function summarize(results: ItemResult[]) {
  const byCat = tally(results, (r) => r.category);
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

const PHASE_OF: Record<string, string> = { follows_player: 'follow', rideable: 'ride', carries_player: 'ride' };

/** Developer diagnostics (never printed for held-out files run with --quiet). */
export function diagnose(results: ItemResult[]) {
  const byScene = tally(results, (r) => r.scene);
  const reasons = new Map<string, number>();
  for (const r of results) for (const k of new Set(r.reasonKeys ?? [])) reasons.set(k, (reasons.get(k) ?? 0) + 1);
  // functions whose absence failed items: claimed-but-not-observed (runtime/game) vs never claimed (interpreter)
  const fnMiss = new Map<string, { items: number; claimedNotObserved: number; notClaimed: number }>();
  for (const r of results) {
    if (r.pass) continue;
    for (const [f, why] of Object.entries(r.fnMisses ?? {})) {
      const x = fnMiss.get(f) ?? { items: 0, claimedNotObserved: 0, notClaimed: 0 };
      x.items++;
      if (why === 'claimed') x.claimedNotObserved++; else x.notClaimed++;
      fnMiss.set(f, x);
    }
  }
  // per machine function: how often claimed / observed / overclaimed / observed without a claim
  const mc: Record<string, { claimed: number; observed: number; overclaimed: number; unclaimedObserved: number }> = {};
  for (const f of MACHINE_FNS) mc[f] = { claimed: 0, observed: 0, overclaimed: 0, unclaimedObserved: 0 };
  for (const r of results) {
    const claimed = new Set((r.report.functions ?? []).filter((f) => MACHINE_FN_SET.has(f)));
    const seen = new Set(r.machineObserved ?? []);
    // phase-probed functions are only checked when their scripted phase ran (see --phases)
    const probed = (f: string) => seen.has(f) || !PHASE_OF[f] || (r.phases ?? []).includes(PHASE_OF[f]);
    for (const f of claimed) { if (!probed(f)) continue; mc[f].claimed++; if (!seen.has(f)) mc[f].overclaimed++; }
    for (const f of seen) { mc[f].observed++; if (!claimed.has(f)) mc[f].unclaimedObserved++; }
  }
  return {
    byScene: Object.fromEntries([...byScene].map(([k, v]) => [k, `${v.pass}/${v.n}`])),
    topFailureReasons: [...reasons].sort((a, b) => b[1] - a[1]).map(([k, v]) => ({ reason: k, items: v })),
    functionMisses: Object.fromEntries([...fnMiss].sort((a, b) => b[1].items - a[1].items)),
    machineClaimVsObserved: Object.fromEntries(Object.entries(mc).filter(([, v]) => v.claimed || v.observed)),
  };
}

function table(title: string, m: Map<string, { n: number; pass: number }>) {
  const rows = [...m].sort((a, b) => a[0].localeCompare(b[0]));
  const w = Math.max(8, ...rows.map(([k]) => k.length));
  const lines = [`${title}:`];
  for (const [k, v] of rows) lines.push(`  ${k.padEnd(w)}  ${String(v.pass).padStart(3)}/${String(v.n).padEnd(3)}  ${Math.round((100 * v.pass) / v.n)}%`);
  return lines.join('\n');
}

/** Human-readable report (dev files only). */
function printReport(results: ItemResult[], dx: ReturnType<typeof diagnose>) {
  const pass = results.filter((r) => r.pass).length;
  console.log(`\n==== ${pass}/${results.length} passed (${Math.round((100 * pass) / results.length)}%)`);
  console.log(table('by category', tally(results, (r) => r.category)));
  console.log(table('by scene', tally(results, (r) => r.scene)));
  console.log('top failure reasons (items):');
  for (const { reason, items } of dx.topFailureReasons.slice(0, 15)) console.log(`  ${String(items).padStart(3)}  ${reason}`);
  console.log('functions missing in failed items (claimed-not-observed → runtime/game; not-claimed → interpreter):');
  for (const [f, v] of Object.entries(dx.functionMisses)) console.log(`  ${f.padEnd(20)} ${String(v.items).padStart(3)} items  (claimed, not observed: ${v.claimedNotObserved}; not claimed: ${v.notClaimed})`);
  console.log('machine functions, claim vs observed (all items):');
  for (const [f, v] of Object.entries(dx.machineClaimVsObserved)) console.log(`  ${f.padEnd(20)} claimed ${String(v.claimed).padStart(3)}  observed ${String(v.observed).padStart(3)}  overclaimed ${String(v.overclaimed).padStart(3)}  observed-not-claimed ${String(v.unclaimedObserved).padStart(3)}`);
}

function printItem(r: ItemResult, verbose: boolean) {
  console.log(`${r.pass ? 'PASS' : 'FAIL'} ${r.id} [${r.category}/${r.scene}] "${r.utterance}"`);
  console.log(`     → ${r.summary}${r.abstain ? '  (abstain: ' + r.abstain + ')' : ''}  conf=${r.confidence.toFixed(2)}`);
  console.log(`     fns=[${(r.report.functions ?? []).join(',')}] intents=${(r.report.intents ?? []).join('/')} targets=${(r.report.targetKinds ?? []).join('/')} count=${r.report.count} mods=[${(r.report.modifications ?? []).join(',')}] obs=[${r.observed.join(',')}]`);
  if (r.overclaims?.length || r.unclaimedObserved?.length) console.log(`     claimed-not-observed=[${(r.overclaims ?? []).join(',')}] observed-not-claimed=[${(r.unclaimedObserved ?? []).join(',')}]${r.phases?.length ? '  phases=' + r.phases.join('+') : ''}`);
  if (verbose) for (const [f, why] of Object.entries(r.evidence ?? {})) console.log(`       ${f}: ${why}`);
  if (!r.pass) console.log(`     ✗ ${r.reasons.join('; ')}`);
}

function argVal(args: string[], name: string): string | null { const i = args.indexOf(name); return i >= 0 ? args[i + 1] ?? null : null; }

/** Run items in N worker processes (each loads its own world + models); results come back in item order. */
async function runParallel(items: Item[], jobs: number, args: string[]): Promise<ItemResult[]> {
  const { spawn } = await import('node:child_process');
  const passArgs = args.filter((a, i) => a !== '--jobs' && args[i - 1] !== '--jobs' && a !== '--out' && args[i - 1] !== '--out');
  const byId = new Map<string, ItemResult>();
  await Promise.all(Array.from({ length: jobs }, (_, k) => new Promise<void>((resolve, reject) => {
    const ch = spawn(process.execPath, [...process.execArgv, process.argv[1], ...passArgs, '--shard', `${k}/${jobs}`], { stdio: ['ignore', 'pipe', 'inherit'] });
    let buf = '';
    ch.stdout.on('data', (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i); buf = buf.slice(i + 1);
        if (line.startsWith('@@RESULT ')) { const r = JSON.parse(line.slice(9)) as ItemResult; byId.set(r.id, r); }
      }
    });
    ch.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`worker ${k} exited with ${code}`))));
  })));
  return items.map((it) => byId.get(it.id)).filter((r): r is ItemResult => !!r);
}

async function main() {
  const args = process.argv.slice(2);
  const file = args.find((a, i) => !a.startsWith('--') && !['--only', '--out', '--s2', '--s2mode', '--window', '--phases', '--score', '--jobs', '--shard'].includes(args[i - 1])) ?? 'eval/dev.jsonl';
  const verbose = args.includes('--verbose');
  const only = argVal(args, '--only');
  const quiet = args.includes('--quiet'); // aggregate only (for sets the implementer must not inspect)
  const byCategory = args.includes('--by-category'); // with --quiet: per-category / per-scene pass rates (still no items)
  const out = argVal(args, '--out');
  if (argVal(args, '--window')) OPTS.window = Number(argVal(args, '--window'));
  if (argVal(args, '--phases')) OPTS.phases = argVal(args, '--phases') as typeof OPTS.phases;
  if (argVal(args, '--score')) OPTS.score = argVal(args, '--score') as typeof OPTS.score;
  const shard = argVal(args, '--shard');
  const jobs = Math.max(1, Number(argVal(args, '--jobs') ?? 1));
  const s2arg = argVal(args, '--s2');
  if (args.includes('--s2mode')) S2MODE = argVal(args, '--s2mode') as any;
  let items: Item[] = readFileSync(file, 'utf8').split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l)).filter((i: Item) => !only || i.id === only);
  if (shard) { const [k, nk] = shard.split('/').map(Number); items = items.filter((_, i) => i % nk === k); }
  const parallel = jobs > 1 && !shard && !s2arg && !args.includes('--compare') && items.length > 1;
  if (!parallel) await enableSemantic(!args.includes('--no-sem'));
  if (s2arg?.startsWith('record:')) { S2 = recorder; S2MODE = 'always'; }
  if (s2arg?.startsWith('replay:')) {
    const f = s2arg.slice(7);
    const m = new Map<string, string>();
    for (const l of readFileSync(f, 'utf8').split('\n').filter((x) => x.trim())) { const j = JSON.parse(l); m.set(j.key, typeof j.response === 'string' ? j.response : JSON.stringify(j.response)); }
    S2 = new ReplayProvider(m);
  }
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
  let results: ItemResult[] = [];
  if (parallel) results = await runParallel(items, Math.min(jobs, items.length), args);
  else {
    for (const it of items) {
      const r = await runItem(it);
      if (shard) { process.stdout.write('@@RESULT ' + JSON.stringify(r) + '\n'); continue; }
      results.push(r);
      if (!quiet && (verbose || !r.pass)) printItem(r, verbose);
    }
    if (shard) return;
  }
  if (parallel && !quiet) for (const r of results) if (verbose || !r.pass) printItem(r, verbose);
  const s: any = summarize(results);
  if (s2arg?.startsWith('record:')) {
    const f = s2arg.slice(7);
    wfs(f, JSON.stringify({ system: recorder.requests[0]?.system ?? '' }) + '\n' + recorder.requests.map((r) => JSON.stringify({ key: r.key, user: r.user })).join('\n') + '\n');
    console.log(`recorded ${recorder.requests.length} System-2 requests to ${f}`);
  }
  s.escalated = results.filter((r: any) => r.escalated).length;
  s.wouldEscalate = results.filter((r: any) => r.needsEscalation).length;
  const dx = diagnose(results);
  if (quiet) {
    // held-out sets: aggregate numbers only — no items, no failure reasons
    if (byCategory) {
      console.log(table('by category', tally(results, (r) => r.category)));
      console.log(table('by scene', tally(results, (r) => r.scene)));
      s.byScene = dx.byScene;
    }
  } else {
    printReport(results, dx);
    Object.assign(s, dx);
  }
  console.log(JSON.stringify(s, null, 2));
  if (out) {
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out + '.summary.json', JSON.stringify(s, null, 2));
    if (!quiet) writeFileSync(out + '.items.json', JSON.stringify(results, null, 2));
    else writeFileSync(out + '.failures_by_category.json', JSON.stringify(Object.fromEntries(Object.entries(s.byCategory)), null, 2));
  }
}

if (process.argv[1] && /eval[\\/]run\.ts$/.test(process.argv[1])) main();
