/**
 * Interpreter: clauses (parser.ts) + scene context -> proposed Ops -> (commit) validated effects.
 *
 * Layering (cheapest first):
 *   1. deterministic normalisation + ASR repair (fuzzy.ts)
 *   2. rule parser (parser.ts) + concept lexicon (lexicon.ts)       = System 1
 *   3. optional embedding nearest-concept for unknown nouns          = System 1.5 (pluggable)
 *   4. optional LLM escalation producing a bounded spec              = System 2 (pluggable, async)
 * Every proposal passes through the construct runtime's validator; the interpreter never
 * touches physics directly.
 */
import { Vector3 } from 'three';
import type { World, Entity } from '../sim/world.js';
import type { Construct } from '../sim/constructs/runtime.js';
import { type ConstructSpec, type Behavior, type Ref, type Part, type PlacementAt, defaultSpec, cloneSpec, type Vec3 } from '../sim/constructs/spec.js';
import { buildForm, mountPoint, FORMS } from '../sim/constructs/forms.js';
import { specExtent } from '../sim/constructs/geom.js';
import { parse, tokenize, type Clause, type NP, type RefWord } from './parser.js';
import { conceptByName, UNSUPPORTED, CONCEPTS, type Concept, type Addon } from './lexicon.js';
import { SPEED_ADJ, VERBS, lemma, FILLERS, type VerbClass } from './words.js';
import { repair, isGameWord, type Correction } from './fuzzy.js';
import type { Op, Interpretation, CommitResult, EffectReport, SourceKind } from './types.js';

export interface Embedder { nearest(text: string, k: number): { id: string; score: number }[]; nearestVerb?(word: string): { verb: string; cls: VerbClass; score: number } | null }
export interface Escalator { propose(text: string, context: any): Promise<{ ops: Op[]; summary: string; confidence: number } | null> }
import { systemPrompt, userPrompt, extractJson, type S2Provider } from './escalate.js';

interface Ctx {
  aimPoint: Vector3;
  aimEntity: Entity | null;
  aimConstruct: Construct | null;
  aimNormal: Vector3;
  focus: Construct | null;
  corrections: Correction[];
  notes: string[];
  approximations: string[];
  unknownWords: string[];
  confidence: number;
  defaultsUsed: number;
  source: SourceKind;
  lastEntity: Entity | null;
  createdThisUtterance: ConstructSpec[];
  touchedEntities: Set<string>;
  pending: Pending[];
  modTags: string[];
  intents: Set<string>;
  targetKinds: Set<string>;
}

interface Pending { kind: 'throw_after_grab' | 'orbit_after_grab'; construct?: string; ref?: Ref; speed?: number; opIndex: number }

const STRIKE_DEFAULT: Record<string, string> = {
  punch: 'fist', hit: 'fist', sock: 'fist', slug: 'fist', uppercut: 'fist', jab: 'fist', clock: 'fist', deck: 'fist', beat: 'fist', attack: 'fist', 'knock out': 'fist', 'take out': 'fist',
  kick: 'foot', dropkick: 'foot', stomp: 'foot', smash: 'hammer', hammer: 'hammer', pound: 'hammer', slam: 'hammer', bash: 'hammer', clobber: 'hammer', demolish: 'wrecking ball', wreck: 'wrecking ball', destroy: 'hammer',
  whack: 'bat', swat: 'bat', smack: 'bat', bonk: 'mallet', bop: 'mallet', thump: 'mallet', wallop: 'bat', spank: 'bat', boop: 'fist',
  slash: 'sword', slice: 'sword', cut: 'sword', stab: 'sword', chop: 'axe', poke: 'spear', skewer: 'spear', impale: 'spear', ram: 'pusher', headbutt: 'pusher', nail: 'hammer', karate: 'hand',
};

export class Interpreter {
  embedder: Embedder | null = null;
  escalator: Escalator | null = null;
  saved = new Map<string, ConstructSpec>();
  lastEntityId: string | null = null;
  lastCreated: ConstructSpec | null = null;
  createdHistory: ConstructSpec[] = [];
  lastCreatedIds: string[] = [];
  undoStack: ({ kind: 'create'; ids: string[] } | { kind: 'modify'; id: string; prev: ConstructSpec })[] = [];
  private pendings: { p: Pending; ids: string[]; deadline: number }[] = [];
  lastEffect: EffectReport | null = null;
  /** System-2 settings: off | hybrid (only when System 1 is unsure) | always (shadow/eval) */
  s2: S2Provider | null = null;
  s2Mode: 'off' | 'hybrid' | 'always' = 'hybrid';
  s2TimeoutMs = 6000;
  s2Key: string | null = null; // request key override (evaluation record/replay)
  s2Stats = { calls: 0, used: 0, failed: 0, ms: [] as number[] };
  private sysPrompt: string | null = null;

  constructor(private w: World) {
    w.tickHooks.push(() => this.tickPending());
  }

  // =========================================================================== public

  interpret(text: string, opts: { partial?: boolean } = {}): Interpretation {
    const t0 = now();
    const { ops, ctx, normalized } = this.plan(text, !!opts.partial);
    const i = this.toInterpretation(text, normalized, ops, ctx, now() - t0);
    return i;
  }

  async commit(text: string): Promise<CommitResult> {
    const t0 = now();
    const { ops, ctx, normalized } = this.plan(text, false);
    let interp = this.toInterpretation(text, normalized, ops, ctx, now() - t0);
    let escalated = false;
    if (this.s2 && this.s2Mode !== 'off' && (this.s2Mode === 'always' || interp.needsEscalation)) {
      const t1 = now();
      this.s2Stats.calls++;
      try {
        this.sysPrompt ??= systemPrompt();
        const req = { key: this.s2Key ?? `${this.w.tick}:${text}`, system: this.sysPrompt, user: userPrompt(text, this.contextSummary(ctx), { summary: interp.summary, unknown: interp.unknownWords, confidence: interp.confidence }) };
        const out = await this.s2.complete(req, this.s2TimeoutMs);
        const json = out ? extractJson(out) : null;
        if (json && Array.isArray(json.ops)) {
          const ctx2 = this.makeCtx();
          const ops2 = this.compileS2(json.ops, ctx2);
          if (ops2.length) {
            escalated = true;
            this.s2Stats.used++;
            Object.assign(ctx, { intents: ctx2.intents, targetKinds: ctx2.targetKinds, modTags: ctx2.modTags, pending: ctx2.pending, approximations: [...ctx2.approximations], notes: [...ctx.notes, ...ctx2.notes], createdThisUtterance: ctx2.createdThisUtterance });
            const ab = ops2.find((o) => o.op === 'abstain') as any;
            const onlyAb = ops2.every((o) => o.op === 'abstain');
            interp = { ...interp, ops: ops2, summary: '🧠 ' + (typeof json.summary === 'string' ? json.summary.slice(0, 120) : this.summarize(ops2, ctx2)), confidence: Math.max(0, Math.min(1, Number(json.confidence) || 0.6)), source: 'llm', abstain: onlyAb ? ab.reason : null, needsEscalation: false };
          }
        } else if (out !== null) this.s2Stats.failed++;
      } catch (e) {
        this.s2Stats.failed++;
        ctx.notes.push('System-2 unavailable: ' + String(e).slice(0, 80));
      }
      this.s2Stats.ms.push(now() - t1);
    } else if (interp.needsEscalation && this.escalator) {
      try {
        const esc = await this.escalator.propose(text, this.contextSummary(ctx));
        if (esc && esc.ops.length) {
          escalated = true;
          interp = { ...interp, ops: esc.ops, summary: esc.summary, confidence: esc.confidence, source: 'llm', abstain: esc.ops.some((o) => o.op === 'abstain') ? (esc.ops.find((o) => o.op === 'abstain') as any).reason : null, needsEscalation: false };
        }
      } catch (e) {
        ctx.notes.push('System-2 escalation failed: ' + String(e).slice(0, 80));
      }
    }
    const applied = this.apply(interp.ops, ctx);
    const res: CommitResult = {
      ...interp,
      applied: applied.ids.length > 0 || applied.changed,
      messages: [...ctx.notes, ...applied.messages],
      clamps: applied.clamps,
      constructIds: applied.ids,
      latencyMs: now() - t0,
      escalated,
    };
    this.lastEffect = this.effectReport(res, ctx, applied);
    this.w.emit('interpretation', { text, summary: res.summary, applied: res.applied, abstain: res.abstain, confidence: res.confidence, source: res.source });
    return res;
  }

  // =========================================================================== planning

  private makeCtx(): Ctx {
    const P = this.w.player;
    const aim = P.aim(300);
    const own = this.w.ownerOf(aim.collider);
    const aimEntity = own?.kind === 'entity' ? this.w.entities.get(own.id) ?? null : null;
    const aimConstruct = own?.kind === 'construct' ? this.w.constructs.get(own.id) : null;
    return {
      aimPoint: aim.point, aimNormal: aim.normal, aimEntity, aimConstruct, focus: this.w.constructs.get(this.w.constructs.focusId),
      corrections: [], notes: [], approximations: [], unknownWords: [], confidence: 0.92, defaultsUsed: 0, source: 'rule',
      lastEntity: this.lastEntityId ? this.w.entities.get(this.lastEntityId) ?? null : null, createdThisUtterance: [], touchedEntities: new Set(), pending: [],
      modTags: [], intents: new Set(), targetKinds: new Set(),
    };
  }

  plan(text: string, partial: boolean): { ops: Op[]; ctx: Ctx; normalized: string } {
    const ctx = this.makeCtx();
    let raw = text.toLowerCase();
    for (const u of UNSUPPORTED) {
      const m = u.pattern.exec(raw);
      if (!m) continue;
      // strip just the impossible part if the rest still asks for a real construct ("a bridge that's invisible but neon pink")
      const rest = (raw.slice(0, m.index) + ' ' + raw.slice(m.index + m[0].length)).replace(/\b(that is|that's|thats|but|and|also|which is)\s*$/g, ' ');
      const restHasConstruct = tokenize(rest).some((w) => conceptByName.has(w) || conceptByName.has(w.replace(/s$/, '')));
      if (restHasConstruct && m[0].split(' ').length <= 2) { raw = rest; ctx.notes.push(`✖ ${u.reason} (ignored that part)`); ctx.confidence -= 0.1; continue; }
      return { ops: [{ op: 'abstain', reason: u.reason }], ctx: { ...ctx, confidence: 0.9 }, normalized: raw };
    }
    const contra = detectContradiction(raw);
    if (contra) return { ops: [{ op: 'abstain', reason: contra }], ctx: { ...ctx, confidence: 0.85 }, normalized: raw };
    // ASR repair on unknown words, then parse
    const punctTokens = (t: string) => t.replace(/[^a-z0-9' ,.!?;-]/g, ' ').replace(/([,.!?;])/g, ' $1 ').split(/\s+/).filter(Boolean);
    const rep = repair(punctTokens(raw));
    ctx.corrections.push(...rep.corrections);
    if (this.embedder?.nearestVerb) {
      for (let i = 0; i < rep.tokens.length; i++) {
        const w = rep.tokens[i];
        const prev = rep.tokens[i - 1];
        const clauseStart = i === 0 || [',', '.', 'and', 'then', 'now', 'just', 'please', '!'].includes(prev);
        if (!clauseStart || w.length < 3 || VERBS[lemma(w)] || conceptByName.has(w) || isFunctionish(w)) continue;
        const next = rep.tokens[i + 1] ?? '';
        if (!next || ![...'the that this those these it him her them em me my a an all both his their'.split(' ')].includes(next)) continue;
        const nv = this.embedder.nearestVerb(w);
        if (nv && nv.score >= 0.55) { ctx.approximations.push(`“${w}” ≈ ${nv.verb}`); ctx.confidence -= 0.15; ctx.source = 'embedding'; rep.tokens[i] = nv.verb; }
      }
    }
    let p = parse(rep.tokens.join(' '));
    let ops = this.clausesToOps(p.clauses, ctx, partial);
    // if nothing made sense, try an aggressive phonetic re-read (homophones like "punts" -> "punch")
    if (!partial && (!ops.length || ops.every((o) => o.op === 'abstain')) && ctx.unknownWords.length) {
      const rep2 = repair(punctTokens(raw), 'aggressive');
      if (rep2.corrections.length) {
        const ctx2 = this.makeCtx();
        ctx2.corrections.push(...rep2.corrections);
        const p2 = parse(rep2.tokens.join(' '));
        const ops2 = this.clausesToOps(p2.clauses, ctx2, partial);
        if (ops2.length && !ops2.every((o) => o.op === 'abstain')) { p = p2; ops = ops2; Object.assign(ctx, ctx2); }
      }
    }
    if (p.corrections) ctx.notes.push('used your correction');
    if (p.analogies.length) ctx.confidence -= 0.05;
    if (ctx.corrections.length) ctx.confidence -= 0.08 * ctx.corrections.length;
    return { ops, ctx, normalized: p.normalized };
  }

  private clausesToOps(clauses: Clause[], ctx: Ctx, partial: boolean): Op[] {
    const ops: Op[] = [];
    for (const c of clauses) {
      if (c.negated) { ctx.notes.push('ignored a negated instruction'); continue; }
      try {
        ops.push(...this.clauseToOps(c, ctx, ops, partial));
      } catch (e) {
        ops.push({ op: 'abstain', reason: 'internal interpretation error' });
        ctx.notes.push(String(e).slice(0, 120));
      }
    }
    // collapse "abstain" when something else was understood
    const real = ops.filter((o) => o.op !== 'abstain');
    if (real.length) return real;
    return ops.length ? [ops[0]] : [{ op: 'abstain', reason: 'I didn’t catch a construct or action in that' }];
  }

  // --------------------------------------------------------------------------- clause dispatch

  private clauseToOps(c: Clause, ctx: Ctx, prior: Op[], partial: boolean): Op[] {
    this.bindDefinites(c, ctx);
    const special = this.reinterpret(c, ctx);
    if (special) return special;
    const v = c.vclass;
    // save / recall
    if (v === 'save') return this.opSave(c, ctx);
    const savedHit = this.savedNameIn(c);
    if (savedHit && (v === null || v === 'recall' || v === 'create' || v === 'duplicate')) return this.opRecall(savedHit, c, ctx);
    if (v === 'recall') return [{ op: 'abstain', reason: 'no saved design with that name' }];
    if (v === 'undo') { ctx.intents.add('control'); ctx.modTags.push('undo'); return [{ op: 'control', action: 'undo' }]; }
    if (v === 'dismiss') return this.opDismiss(c, ctx);
    if (v === 'release' || v === 'drop' && !c.objects.some((o) => o.concept)) return this.opRelease(c, ctx);
    if (v === 'duplicate') return this.opDuplicate(c, ctx);

    const obj = c.objects[0] ?? null;
    const objIsRef = !!obj?.ref;
    const objConcept = !!(obj && (obj.concept || obj.unknownHead) && !objIsRef);
    // contradictory sizes in one noun phrase ("a huge tiny ball")
    if (obj && obj.sizes.some((x) => x >= 1.4) && obj.sizes.some((x) => x <= 0.7)) return [{ op: 'abstain', reason: 'big or small? that contradicts itself' }];
    // "a jetpack on the back of those", "spikes onto the wall": attach to an existing construct
    if ((v === null || v === 'create' || v === 'add') && objConcept && c.preps.some((p) => ['on', 'onto', 'to'].includes(p.prep) && (p.np.ref?.kind === 'construct' || p.np.ref?.kind === 'constructs'))) return this.opAdd(c, ctx);
    // pure modification: "bigger", "make it faster", "turn it vertical", "make them spin" handled below
    if (v === null && !objConcept && !objIsRef && c.bareAdjs.length) return this.opModify(c, ctx, null);
    if (v === 'modify') return this.opModify(c, ctx, obj);
    if (v === 'create' && objIsRef && obj!.ref!.kind !== 'self' && (c.bareAdjs.length || obj!.count !== null || obj!.sizes.length || obj!.dims.length || obj!.materials.length || c.preps.some((p) => p.prep === 'into'))) return this.opModify(c, ctx, obj);
    // "get him" / "get that drone" = grab it
    if (c.verb === 'get' && objIsRef && ['entity', 'person'].includes(obj!.ref!.kind)) return this.opAction({ ...c, vclass: 'grab' }, ctx, null);
    if (v === 'create' && !obj && c.bareAdjs.length) return this.opModify(c, ctx, null);
    if (v === 'add') return this.opAdd(c, ctx);
    // "put it over there" / "move it here"
    if ((v === 'create' && c.verb === 'put' || v === 'push' && c.verb === 'move') && objIsRef && (obj!.ref!.kind === 'construct' || obj!.ref!.kind === 'constructs') && c.preps.length) return this.opMove(c, ctx);
    // "give it wheels", "put rockets on it"
    if (v === 'create' && (c.verb === 'give' || c.verb === 'put') && (obj?.ref?.kind === 'construct' || c.preps.some((p) => ['on', 'onto', 'to'].includes(p.prep) && p.np.ref?.kind === 'construct'))) return this.opAdd(c, ctx);
    if (v === 'create' && c.objects.length === 0 && c.objects.every((o) => !o.addons.length) && !c.purpose && !c.bareAdjs.length) {
      return [{ op: 'abstain', reason: `make what? I heard “${c.raw.join(' ')}”` }];
    }
    // plain creation (possibly with placement / behaviour phrases), incl. bare noun phrases
    if (v === null || v === 'create' || v === 'point' && objConcept) {
      if (!obj) return [{ op: 'abstain', reason: `I didn’t catch what to make in “${c.raw.join(' ')}”` }];
      if (objIsRef && !objConcept) {
        if (obj.ref!.kind === 'aim' || obj.ref!.kind === 'place') return [{ op: 'abstain', reason: 'make what, there?' }];
        return [{ op: 'abstain', reason: 'which construct or action do you mean?' }];
      }
      if (!obj.concept && !obj.unknownHead && obj.addons.length && ctx.focus) return this.opAdd(c, ctx);
      const purposeVerb = c.purpose?.vclass ?? null;
      if (purposeVerb && SELF_GOALS.has(purposeVerb)) {
        const specs = this.objectsToSpecs(c.objects.filter((o) => o.concept || o.unknownHead), c, ctx, purposeVerb);
        ctx.intents.add('create');
        const ops: Op[] = specs.map((spec) => ({ op: 'create', spec }));
        return [...ops, ...this.ensureGoal(purposeVerb, specs, ctx, c.purpose!)];
      }
      if (purposeVerb && !['create', 'modify'].includes(purposeVerb)) {
        // "a wall to block the bullets", "something to get me across" -> the purpose verb decides
        return this.opAction({ ...c.purpose!, objects: c.purpose!.objects.length ? c.purpose!.objects : [], preps: [...c.purpose!.preps, ...c.preps], purpose: null } as Clause, ctx, obj);
      }
      const specs = this.objectsToSpecs(c.objects.filter((o) => o.concept || o.unknownHead), c, ctx);
      if (!specs.length) return this.abstainUnknown(obj, ctx);
      ctx.intents.add('create');
      return specs.map((spec) => ({ op: 'create', spec }));
    }
    // action verbs
    return this.opAction(c, ctx, null);
  }

  private abstainUnknown(np: NP, ctx: Ctx): Op[] {
    const w = np.unknownHead ?? np.tokens.join(' ');
    ctx.unknownWords.push(w);
    ctx.confidence = 0.2;
    return [{ op: 'abstain', reason: `I don’t know what a “${w}” is — describe its shape or what it should do` }];
  }

  // --------------------------------------------------------------------------- building specs from noun phrases

  private resolveConcept(np: NP, ctx: Ctx): { concept: Concept | null; approx: string | null } {
    if (np.concept) {
      // a concept word used with an unknown head word after it ("wall thing", "hammer robot") keeps the concept
      if (np.analogy) ctx.approximations.push(`like a ${np.conceptWords} → ${np.concept.id}`);
      if (['creature'].includes(np.concept.cls)) ctx.approximations.push(`${np.conceptWords} → approximated as a hard-light ${np.concept.form}`);
      return { concept: np.concept, approx: null };
    }
    const head = np.unknownHead;
    if (!head) return { concept: null, approx: null };
    // compound unknowns: try every unknown word from the right ("airport baggage carousel" already matched; "moving sidewalk"?)
    for (let i = np.unknownWords.length - 1; i >= 0; i--) {
      const w = np.unknownWords[i];
      const c = conceptByName.get(w) ?? conceptByName.get(w.replace(/s$/, ''));
      if (c) return { concept: c, approx: null };
    }
    if (this.embedder) {
      const phrase = np.unknownWords.join(' ');
      const res = this.embedder.nearest(phrase, 3);
      if (res.length && res[0].score >= 0.5) {
        const c = CONCEPTS.find((x) => x.id === res[0].id) ?? null;
        if (c) {
          ctx.approximations.push(`“${phrase}” ≈ ${c.id} (${Math.round(res[0].score * 100)}%)`);
          ctx.confidence -= 0.25;
          ctx.source = 'embedding';
          return { concept: c, approx: phrase };
        }
      }
    }
    return { concept: null, approx: head };
  }

  /** Build validated-shape (not yet validated-budget) specs for each object noun phrase. */
  private objectsToSpecs(nps: NP[], c: Clause, ctx: Ctx, verbHint: VerbClass | null = null): ConstructSpec[] {
    const out: ConstructSpec[] = [];
    for (const np of nps) {
      const { concept, approx } = this.resolveConcept(np, ctx);
      if (!concept) {
        if (np.unknownHead) {
          ctx.unknownWords.push(np.unknownHead);
          // if the verb gives it a clear job we can still act with a generic form, flagged as an approximation
          if (verbHint && verbHint !== 'create') {
            const generic = this.genericForVerb(verbHint);
            const spec = this.specFromConcept(generic, np, c, ctx);
            spec.provenance = { source: 'fallback', confidence: 0.35, approximatedFrom: np.unknownHead, notes: [`unknown “${np.unknownHead}” → generic ${generic.id}`] };
            ctx.approximations.push(`“${np.unknownHead}” unknown → used a ${generic.id}`);
            ctx.confidence = Math.min(ctx.confidence, 0.4);
            out.push(spec);
          }
        }
        continue;
      }
      const spec = this.specFromConcept(concept, np, c, ctx);
      if (approx) spec.provenance = { source: 'embedding', confidence: 0.5, approximatedFrom: approx, notes: [] };
      out.push(spec);
    }
    return out;
  }

  private genericForVerb(v: VerbClass): Concept {
    const map: Partial<Record<VerbClass, string>> = { strike: 'fist', grab: 'hand', throw: 'ball', trap: 'cage', block: 'wall', protect: 'dome', crush: 'weight', launch: 'spring', carry: 'platform', catch: 'net', rescue: 'net', pull: 'hand', push: 'hand', lift: 'hand', shoot: 'blaster', cross: 'bridge', climb: 'launch pad', ride: 'hoverboard', sweep: 'broom', attract: 'magnet', repel: 'fan', orbit: 'ball', spin: 'saw' };
    return conceptByName.get(map[v] ?? 'block')!;
  }

  specFromConcept(concept: Concept, np: NP | null, c: Clause | null, ctx: Ctx): ConstructSpec {
    const parts = buildForm(concept.form);
    let scale = concept.scale ?? 1;
    for (const s of np?.sizes ?? []) scale *= s;
    const stretch: Vec3 = [1, 1, 1];
    for (const d of np?.dims ?? []) { stretch[0] *= d[0]; stretch[1] *= d[1]; stretch[2] *= d[2]; }
    const spec = defaultSpec({
      concept: concept.id,
      label: this.labelFor(concept, np),
      parts,
      scale,
      stretch: stretch.some((x) => x !== 1) ? stretch : undefined,
      count: Math.max(1, np?.count ?? 1),
      arrangement: (np?.arrangement as any) ?? (np && (np.count ?? 1) > 1 ? 'line' : 'single'),
      motion: concept.motion,
      density: concept.density ?? (concept.cls === 'heavy' ? 4000 : 400),
      restitution: concept.restitution ?? 0.1,
      friction: concept.friction ?? 0.6,
      soft: !!concept.soft,
      placement: { at: concept.at, orient: concept.orient ?? 'face_away' },
      behaviors: (concept.beh ?? []).map((b) => this.fillTarget(b, null, ctx)),
      lifetime: concept.lifetime ?? (concept.motion === 'static' ? 90 : 60),
      provenance: { source: 'lexicon', confidence: ctx.confidence, notes: [] },
    });
    if (concept.at === 'hand' && !spec.behaviors.some((b) => DRIVES.has(b.type))) spec.behaviors.push({ type: 'hold', side: 'right' });
    if (np?.absExtent) {
      const ext = specExtent({ ...spec, stretch: undefined });
      const cur = [ext.size.x, ext.size.y, ext.size.z][np.absExtent.axis] || 1;
      const k = np.absExtent.meters / cur;
      const st: Vec3 = [...(spec.stretch ?? [1, 1, 1])] as Vec3;
      st[np.absExtent.axis] *= k;
      spec.stretch = st;
      if (k > 14) spec.scale *= Math.min(k / 14, 1e6); // absurd lengths also push overall size so the clamp is visible
    }
    if (np) {
      this.applyMaterials(spec, np.materials, ctx);
      for (const sp of np.speeds) this.scaleSpeeds(spec, sp);
      for (const a of np.addons) this.addAddon(spec, a.addon, a.where, ctx);
      if (np.colors.length) spec.color = np.colors[0];
      if ((np.count ?? 1) > 1 && !np.arrangement && concept.cls === 'barrier' && spec.placement.at === 'self') spec.arrangement = 'ring';
    }
    // clause-level preps shape placement / targets
    if (c) this.applyPreps(spec, c, ctx);
    // relative clauses: "that orbits me", "that shoots at the drones"
    for (const rel of np?.rel ?? []) this.applyRelClause(spec, rel, ctx);
    if (c && (c as any).relationWhere && np?.addons.length === 0) void 0;
    ctx.createdThisUtterance.push(spec);
    return spec;
  }

  private labelFor(concept: Concept, np: NP | null): string {
    const bits: string[] = [];
    const size = (np?.sizes ?? []).reduce((a, b) => a * b, 1);
    if (size >= 2.5) bits.push('giant'); else if (size >= 1.4) bits.push('big'); else if (size < 0.5) bits.push('tiny'); else if (size < 0.9) bits.push('small');
    for (const m of np?.materials ?? []) if (!['vertical', 'sideways', 'flat', 'upsidedown', 'reverse', 'permanent', 'temporary'].includes(m)) bits.push(m);
    bits.push(np?.conceptWords && conceptByName.get(np.conceptWords) === concept ? np.conceptWords : concept.id);
    let label = bits.join(' ');
    if ((np?.count ?? 1) > 1) label = `${np!.count} × ${label}`;
    if (np?.addons.length) label += ' with ' + np.addons.map((a) => a.addon.id + (a.count > 1 || a.addon.names[0] !== a.addon.id ? 's' : '')).join(' & ');
    return label;
  }

  private applyMaterials(spec: ConstructSpec, mats: string[], ctx: Ctx) {
    for (const m of mats) {
      switch (m) {
        case 'heavy': spec.density *= 4; if (spec.motion === 'kinematic' && !spec.behaviors.some((b) => b.type === 'mount')) void 0; break;
        case 'light': spec.density *= 0.3; break;
        case 'sturdy': spec.lifetime = Math.max(spec.lifetime, 120); break;
        case 'bouncy':
          spec.restitution = 0.95;
          if (!spec.behaviors.some((b) => b.type === 'launch')) spec.behaviors.push({ type: 'launch', speed: 18, dir: 'normal' });
          break;
        case 'sticky': if (!spec.behaviors.some((b) => b.type === 'grab')) spec.behaviors.push({ type: 'field', mode: 'attract', radius: 4, strength: 25 }); ctx.approximations.push('sticky → short-range pull'); break;
        case 'soft': spec.soft = true; if (!spec.behaviors.some((b) => b.type === 'cushion')) spec.behaviors.push({ type: 'cushion' }); break;
        case 'slippery': spec.friction = 0; break;
        case 'spiky': this.addAddon(spec, ADDON('spike'), 'front', ctx); spec.density *= 1.3; break;
        case 'spinning': if (!spec.behaviors.some((b) => b.type === 'spin')) spec.behaviors.push({ type: 'spin', rate: 10, axis: [0, 1, 0] }); break;
        case 'explosive': spec.behaviors.push({ type: 'explode', radius: 6, force: 32, delay: 4, onContact: true }); break;
        case 'homing': spec.behaviors.push({ type: 'home', target: { kind: 'nearest_enemy' }, speed: 30 }); break;
        case 'floating': if (spec.motion === 'dynamic') spec.motion = 'kinematic'; break;
        case 'rolling': spec.motion = 'dynamic'; break;
        case 'magnetic': spec.behaviors.push({ type: 'field', mode: 'attract', radius: 12, strength: 30 }); break;
        case 'reflective': spec.behaviors.push({ type: 'reflect' }); break;
        case 'vertical': spec.placement.tilt = 90; break;
        case 'sideways': spec.placement.yaw = 90; break;
        case 'upsidedown': spec.placement.tilt = 180; break;
        case 'reverse': spec.placement.yaw = 180; break;
        case 'permanent': spec.lifetime = 150; break;
        case 'temporary': spec.lifetime = 15; break;
        case 'flat': spec.stretch = [1.3, 0.4, 1.3]; break;
      }
    }
  }

  private scaleSpeeds(spec: ConstructSpec, k: number): boolean {
    let any = false;
    spec.behaviors = spec.behaviors.map((b) => {
      switch (b.type) {
        case 'strike': any = true; return { ...b, speed: b.speed * k };
        case 'throw': any = true; return { ...b, speed: b.speed * k };
        case 'home': any = true; return { ...b, speed: b.speed * k };
        case 'orbit': any = true; return { ...b, speed: b.speed * k };
        case 'spin': any = true; return { ...b, rate: b.rate * k };
        case 'conveyor': any = true; return { ...b, speed: b.speed * k };
        case 'shoot': any = true; return { ...b, rate: b.rate * k, speed: b.speed * Math.sqrt(k) };
        case 'thrust': any = true; return { ...b, accel: b.accel * k };
        case 'launch': any = true; return { ...b, speed: b.speed * Math.sqrt(k) };
        case 'sweep': any = true; return { ...b, speed: b.speed * k };
        case 'mount': any = true; return { ...b, groundSpeedMul: b.groundSpeedMul ? b.groundSpeedMul * Math.sqrt(k) : b.groundSpeedMul, thrustLook: b.thrustLook ? b.thrustLook * k : b.thrustLook, glide: b.glide ? b.glide * Math.sqrt(k) : b.glide };
        case 'field': any = true; return { ...b, strength: b.strength * k };
        default: return b;
      }
    });
    return any;
  }

  addAddon(spec: ConstructSpec, a: Addon, where: string | null, ctx: Ctx) {
    const hostExt = specExtent({ ...spec, scale: 1, stretch: undefined });
    const hostSize = Math.max(hostExt.size.x, hostExt.size.y, hostExt.size.z);
    const addParts = buildForm(a.form);
    const addExt = specExtent({ ...defaultSpec({ parts: addParts }) });
    const addSize = Math.max(addExt.size.x, addExt.size.y, addExt.size.z) || 1;
    const k = (hostSize * a.scale * 0.5) / addSize;
    const w = (where ?? a.where) as any;
    const mounts = mountPoint(spec.parts, w);
    for (const m of mounts) {
      for (const p of addParts) {
        const rot = rotForMount(w, a);
        const pos = rotate(p.pos, rot).map((x, i) => x * k + m.pos[i]) as Vec3;
        spec.parts.push({ ...p, size: p.size.map((x) => x * k) as Vec3, pos, rot: combineRot(rot, p.rot), role: p.role === 'body' ? a.id : p.role ?? a.id });
      }
    }
    if (spec.parts.length > 48) spec.parts = spec.parts.slice(0, 48);
    for (const b of a.beh ?? []) spec.behaviors.push(this.fillTarget(b, null, ctx));
    if (a.mountMods) {
      const mount = spec.behaviors.find((b) => b.type === 'mount') as any;
      if (mount) for (const [key, val] of Object.entries(a.mountMods)) mount[key] = key === 'maxFall' ? Math.min(mount[key] ?? Infinity, val) : Math.max(mount[key] ?? 0, val) * (key === 'groundSpeedMul' && mount[key] ? 1.2 : 1);
      else if (a.id === 'wheel') { if (spec.motion === 'static') spec.motion = 'dynamic'; }
      else if (a.id === 'wing') ctx.approximations.push('wings on a non-vehicle are decorative');
    }
    if (a.density) spec.density *= a.density;
    if (a.restitution) spec.restitution = a.restitution;
    spec.label = spec.label.includes(' with ') ? spec.label + ` & ${a.id}s` : spec.label;
  }

  private fillTarget(b: any, target: Ref | null, ctx: Ctx): Behavior {
    const copy = JSON.parse(JSON.stringify(b));
    for (const key of ['target', 'around']) {
      if (copy[key]?.kind === 'T') copy[key] = target ?? this.defaultTarget(ctx);
    }
    return copy;
  }

  private defaultTarget(ctx: Ctx): Ref {
    if (ctx.aimEntity) return { kind: 'entity', id: ctx.aimEntity.id };
    const ne = this.w.constructs.nearestEnemy();
    if (ne) return { kind: 'entity', id: ne.id };
    return { kind: 'point', p: ctx.aimPoint.toArray() as Vec3 };
  }

  // --------------------------------------------------------------------------- references

  /** Resolve a reference word to entities (for action verbs) — never guesses silently: notes are recorded. */
  resolveEntities(r: RefWord | null, ctx: Ctx, forVerb: VerbClass | null): Entity[] {
    const all = [...this.w.entities.values()];
    const P = this.w.player.pos;
    const dist = (e: Entity) => this.w.entityPos(e).distanceTo(P);
    const alive = (e: Entity) => !e.disabled || e.type === 'civilian' || e.type === 'boulder' || e.type === 'crate';
    if (!r) {
      if (ctx.aimEntity) return [ctx.aimEntity];
      return [];
    }
    if (r.kind === 'entity') {
      let pool = all.filter((e) => e.type === r.entityType || r.entityType === 'drone' && e.type === 'drone');
      if (!pool.length && r.entityType === 'drone') pool = all.filter((e) => e.hostile);
      if (!pool.length && r.entityType === 'turret') pool = all.filter((e) => e.type === 'turret');
      if (!pool.length && r.entityType === 'civilian') pool = all.filter((e) => e.type === 'civilian' || e.type === 'dummy');
      if (!pool.length) { ctx.notes.push(`no ${r.noun ?? r.entityType} in sight`); return []; }
      pool = pool.filter(alive).length ? pool.filter(alive) : pool;
      if (r.plural || r.which === 'all') return pool.sort((a, b) => dist(a) - dist(b));
      if (r.which === 'nearest') return [pool.sort((a, b) => dist(a) - dist(b))[0]];
      if (ctx.aimEntity && pool.includes(ctx.aimEntity) && r.which !== 'other') return [ctx.aimEntity];
      if (ctx.lastEntity && pool.includes(ctx.lastEntity) && r.which !== 'other') return [ctx.lastEntity];
      // nothing under the crosshair: nearest in view cone, else nearest
      const look = this.w.player.look(new Vector3());
      const inView = pool.filter((e) => this.w.entityPos(e).sub(P).normalize().dot(look) > 0.8).sort((a, b) => dist(a) - dist(b));
      const pick = (r.which === 'other' ? pool.filter((e) => e !== ctx.aimEntity && e !== ctx.lastEntity) : inView.length ? inView : pool).sort((a, b) => dist(a) - dist(b))[0] ?? pool[0];
      return [pick];
    }
    if (r.kind === 'person' || r.kind === 'construct' && forVerb && ENTITY_VERBS.has(forVerb) || r.kind === 'constructs' && forVerb && ENTITY_VERBS.has(forVerb)) {
      if (r.plural || r.kind === 'constructs') {
        if (ctx.aimEntity) return all.filter((e) => e.type === ctx.aimEntity!.type && alive(e)).sort((a, b) => dist(a) - dist(b));
        const hostiles = all.filter((e) => e.hostile && alive(e)).sort((a, b) => dist(a) - dist(b));
        if (hostiles.length) return hostiles;
      }
      // held cargo counts as "him" too
      const held = ctx.focus ? this.w.constructs.cargoOf(ctx.focus.id).map((id) => this.w.entities.get(id)!).filter(Boolean) : [];
      if (ctx.aimEntity) return [ctx.aimEntity];
      if (held.length) return held;
      if (ctx.lastEntity) return [ctx.lastEntity];
      if (r.kind === 'construct') return []; // "it" with nothing under the crosshair is a construct, not an entity
      const ne = all.filter((e) => e.hostile && alive(e)).sort((a, b) => dist(a) - dist(b))[0] ?? all.sort((a, b) => dist(a) - dist(b))[0];
      if (ne) { ctx.notes.push(`“${r.kind === 'person' ? 'him' : 'it'}” → nearest ${ne.type}`); ctx.confidence -= 0.1; }
      return ne ? [ne] : [];
    }
    return [];
  }

  resolveConstructs(r: RefWord | null, ctx: Ctx, np?: NP | null): Construct[] {
    const list = [...this.w.constructs.list.values()];
    if (r?.id) { const c = this.w.constructs.get(r.id); if (c) return [c]; }
    if (np?.concept && !np.ref && np.det && ['the', 'that', 'this', 'those', 'these', 'my'].includes(np.det)) {
      const same = list.filter((c) => c.spec.concept === np.concept!.id || c.spec.label.includes(np.conceptWords ?? '###'));
      if (same.length) return np.plural ? same : [same[same.length - 1]];
    }
    if (!r) return ctx.focus ? [ctx.focus] : [];
    if (r.kind === 'construct') {
      if (ctx.aimConstruct && r.which === 'last' && !ctx.focus) return [ctx.aimConstruct];
      if (r.which === 'first') return list.slice(0, 1);
      if (r.which === 'other') return list.filter((c) => c !== ctx.focus).slice(-1);
      return ctx.focus ? [ctx.focus] : list.slice(-1);
    }
    if (r.kind === 'constructs') {
      const lastIds = this.lastCreatedIds.map((id) => this.w.constructs.get(id)).filter((c): c is Construct => !!c);
      if (lastIds.length > 1) return lastIds;
      return ctx.focus ? [ctx.focus] : list.slice(-2);
    }
    return [];
  }

  /** Turn a noun phrase into a runtime Ref (entity / self / point / construct). */
  refFor(np: NP | null, ctx: Ctx, forVerb: VerbClass | null): { ref: Ref | null; entities: Entity[]; kind: string } {
    if (!np) return { ref: null, entities: [], kind: 'none' };
    const r = np.ref;
    if (r?.kind === 'self') return { ref: { kind: 'self' }, entities: [], kind: 'self' };
    if (r?.kind === 'aim') {
      if (ctx.aimEntity) return { ref: { kind: 'entity', id: ctx.aimEntity.id }, entities: [ctx.aimEntity], kind: 'aimed_entity' };
      return { ref: { kind: 'point', p: ctx.aimPoint.toArray() as Vec3 }, entities: [], kind: 'location' };
    }
    if (r?.kind === 'place') {
      const p = ctx.aimPoint.clone();
      if (r.place === 'sky' || r.place === 'air') p.copy(this.w.player.pos).add(new Vector3(0, 30, 0)).addScaledVector(this.w.player.forward(), 10);
      if (r.place === 'ground' || r.place === 'floor') p.y = Math.min(p.y, this.w.player.pos.y - 0.9);
      return { ref: { kind: 'point', p: p.toArray() as Vec3 }, entities: [], kind: 'location' };
    }
    if (r && (r.kind === 'entity' || r.kind === 'person' || (r.kind === 'construct' || r.kind === 'constructs') && forVerb && ENTITY_VERBS.has(forVerb))) {
      const es = this.resolveEntities(r, ctx, forVerb);
      if (es.length) {
        const kind = es.length === 1 && ctx.aimEntity === es[0] ? 'aimed_entity' : es.length === 1 && es[0] === this.w.constructs.nearestEnemy() ? 'nearest_enemy' : es.length > 1 ? 'aimed_entity' : 'other';
        for (const e of es) ctx.touchedEntities.add(e.id);
        this.lastEntityId = es[0].id;
        return { ref: es.length > 1 ? { kind: 'entities', ids: es.map((e) => e.id) } : { kind: 'entity', id: es[0].id }, entities: es, kind };
      }
      if (r.kind === 'entity' || r.kind === 'person') return { ref: null, entities: [], kind: 'missing' };
    }
    if (r?.kind === 'construct' || r?.kind === 'constructs' || (np.concept && np.det && ['the', 'that', 'this', 'those', 'these', 'my'].includes(np.det))) {
      const cs = this.resolveConstructs(r, ctx, np);
      if (cs.length) return { ref: { kind: 'construct', id: cs[0].id }, entities: [], kind: 'previous_construct' };
    }
    return { ref: null, entities: [], kind: 'none' };
  }

  // --------------------------------------------------------------------------- placement from prepositions

  private applyPreps(spec: ConstructSpec, c: Clause, ctx: Ctx) {
    for (const p of c.preps) {
      const np = p.np;
      const isSelf = np.ref?.kind === 'self';
      const tgt = this.refFor(np, ctx, 'create');
      const ref = tgt.ref;
      if (ref) ctx.targetKinds.add(tgt.kind);
      switch (p.prep) {
        case 'under': case 'underneath': case 'beneath': case 'below':
          spec.placement = { ...spec.placement, at: isSelf ? 'under_self' : 'under_target', target: isSelf ? undefined : ref ?? undefined };
          break;
        case 'over': case 'above': case 'on top of': case 'upon':
          if (np.ref?.kind === 'place' && ['gap', 'canyon', 'chasm', 'pit', 'hole', 'ravine', 'river'].includes(np.ref.place!)) { spec.placement = { ...spec.placement, at: 'span' }; break; }
          if (['enclosure'].includes(conceptByName.get(spec.concept)?.cls ?? '') || ['dome', 'bubble', 'umbrella', 'shield'].includes(spec.concept)) { spec.placement = { ...spec.placement, at: isSelf ? 'self' : 'around_target', target: isSelf ? undefined : ref ?? undefined }; if (isSelf) spec.passPlayer = true; break; }
          spec.placement = { ...spec.placement, at: isSelf ? 'above_self' : 'above_target', target: ref ?? undefined, height: spec.placement.height ?? 6 };
          break;
        case 'around': case 'round':
          if (spec.behaviors.some((b) => b.type === 'orbit')) break;
          spec.placement = { ...spec.placement, at: isSelf ? 'self' : 'around_target', target: isSelf ? undefined : ref ?? undefined };
          if (spec.count > 1) spec.arrangement = 'ring';
          else if (['barrier'].includes(conceptByName.get(spec.concept)?.cls ?? '') && !['dome', 'bubble'].includes(spec.concept)) { spec.count = Math.max(spec.count, 4); spec.arrangement = 'ring'; }
          if (isSelf) spec.passPlayer = true;
          break;
        case 'in front of':
          spec.placement = { ...spec.placement, at: isSelf ? 'front' : 'between', target: isSelf ? undefined : ref ?? undefined };
          break;
        case 'behind':
          spec.placement = { ...spec.placement, at: isSelf ? 'behind_self' : 'target', target: ref ?? undefined };
          break;
        case 'between': {
          const other = c.preps.find((q) => q !== p && q.prep === 'and') ?? null;
          void other;
          // "between me and the turret": parser gives np tokens "me and the turret" sometimes
          const toks = np.tokens;
          const andAt = toks.indexOf('and');
          let target: Ref | undefined = ref ?? undefined;
          if (andAt >= 0) {
            const second = toks.slice(andAt + 1);
            const fake = { ...np, tokens: second } as NP;
            const e = this.refFor(parseLite(second), ctx, 'block');
            target = e.ref ?? target;
            void fake;
          }
          if (!target || target.kind === 'self') target = undefined;
          spec.placement = { ...spec.placement, at: 'between', target };
          break;
        }
        case 'across': case 'through':
          spec.placement = { ...spec.placement, at: spec.placement.at === 'span' || ['surface', 'ramp'].includes(conceptByName.get(spec.concept)?.cls ?? '') ? 'span' : spec.placement.at };
          break;
        case 'to': case 'toward': case 'towards': case 'up to':
          if (['bridge', 'slab', 'rope', 'conveyor', 'slide', 'stairs', 'ladder', 'tube'].includes(spec.concept)) spec.placement = { ...spec.placement, at: 'span', target: ref ?? undefined };
          else if (ref && ref.kind !== 'self') this.targetBehaviors(spec, ref);
          break;
        case 'on': case 'onto': case 'at': case 'into': case 'against': case 'in':
          if (isSelf) {
            if (!spec.behaviors.some((b) => b.type === 'mount')) spec.placement = { ...spec.placement, at: 'self' };
            break;
          }
          if (ref?.kind === 'entity' || ref?.kind === 'entities') {
            const cls = conceptByName.get(spec.concept)?.cls;
            if (cls === 'heavy') spec.placement = { ...spec.placement, at: 'above_target', target: ref };
            else if (cls === 'enclosure') spec.placement = { ...spec.placement, at: 'around_target', target: ref };
            else if (cls === 'launcher' || cls === 'surface' || spec.soft) spec.placement = { ...spec.placement, at: 'under_target', target: ref };
            this.targetBehaviors(spec, ref);
          } else if (ref?.kind === 'point') {
            spec.placement = { ...spec.placement, at: 'aim' };
            this.targetBehaviors(spec, ref);
          }
          break;
        case 'next to': case 'near': case 'by': case 'beside':
          spec.placement = { ...spec.placement, at: isSelf ? 'front' : 'target', target: ref ?? undefined, height: 0 };
          break;
        case 'for':
          if (!isSelf && ref) { spec.placement = { ...spec.placement, at: 'around_target', target: ref }; }
          break;
        case 'with':
          for (const a of np.addons) this.addAddon(spec, a.addon, a.where, ctx);
          if (np.concept && !np.addons.length) {
            // "a car with a cannon" where cannon is a concept that also exists as an addon
            const asAddon = ADDON_BY_CONCEPT(np.concept.id);
            if (asAddon) this.addAddon(spec, asAddon, null, ctx);
          }
          break;
      }
    }
  }

  private targetBehaviors(spec: ConstructSpec, ref: Ref) {
    spec.behaviors = spec.behaviors.map((b: any) => (b.target && (b.target.kind === 'nearest_enemy' || b.target.kind === 'point' || b.target.kind === 'none') && b.type !== 'tether' ? { ...b, target: ref } : b));
  }

  private applyRelClause(spec: ConstructSpec, rel: Clause, ctx: Ctx) {
    const obj = rel.objects[0] ?? rel.preps.find((p) => ['around', 'at', 'to', 'toward', 'towards', 'into'].includes(p.prep))?.np ?? null;
    const t = this.refFor(obj, ctx, rel.vclass);
    const tref: Ref = t.ref ?? (rel.vclass === 'orbit' || rel.vclass === 'follow' || rel.vclass === 'protect' ? { kind: 'self' } : this.defaultTarget(ctx));
    if (t.ref) ctx.targetKinds.add(t.kind);
    const speedK = rel.bareAdjs.map((a) => SPEED_ADJ[a]).filter(Boolean).reduce((a, b) => a * b, 1);
    switch (rel.vclass) {
      case 'orbit': spec.behaviors.push({ type: 'orbit', around: tref, radius: 3.5 + specExtent(spec).radius, speed: 2.5 * speedK }); if (tref.kind === 'self') spec.passPlayer = true; break;
      case 'spin': case 'swing':
        if (rel.preps.some((p) => p.prep === 'around')) spec.behaviors.push({ type: 'orbit', around: tref, radius: 3.5 + specExtent(spec).radius, speed: 2.5 * speedK });
        else spec.behaviors.push({ type: 'spin', rate: 8 * speedK, axis: [0, 1, 0] });
        break;
      case 'follow': spec.behaviors.push(tref.kind === 'self' ? { type: 'follow', target: tref, offset: [0, 2.5, -2] } : { type: 'home', target: tref, speed: 20 * speedK }); break;
      case 'shoot': spec.behaviors.push({ type: 'shoot', target: t.ref ?? { kind: 'nearest_enemy' }, rate: 2 * speedK, speed: 50, size: 0.3 }); break;
      case 'strike': case 'crush': spec.behaviors.push({ type: 'strike', target: tref, speed: 40 * speedK, after: 'return', repeat: 3 }); break;
      case 'grab': case 'catch': case 'pull': case 'hold': spec.behaviors.push({ type: 'grab', target: tref, then: rel.vclass === 'pull' ? 'bring' : 'hold' }); break;
      case 'explode': spec.behaviors.push({ type: 'explode', radius: 6, force: 32, delay: 3, onContact: true }); break;
      case 'launch': spec.behaviors.push({ type: 'launch', speed: 28, dir: 'up' }); break;
      case 'attract': spec.behaviors.push({ type: 'field', mode: 'attract', radius: 12, strength: 35 }); break;
      case 'repel': case 'push': spec.behaviors.push({ type: 'field', mode: rel.verb === 'push' ? 'repel' : 'wind', radius: 12, strength: 40 }); break;
      case 'block': case 'protect': case 'reflect':
        spec.placement = { ...spec.placement, at: 'between', target: t.ref && t.ref.kind !== 'self' ? t.ref : undefined };
        if (rel.vclass === 'reflect' || rel.particle === 'back') spec.behaviors.push({ type: 'reflect' });
        break;
      case 'ride': case 'carry': case 'cross': case 'climb':
        if (!spec.behaviors.some((b) => b.type === 'mount')) spec.behaviors.push({ type: 'mount', mode: 'ride', groundSpeedMul: 1.8, frictionMul: 0.4, glide: rel.verb === 'fly' || rel.verb === 'glide' ? 1.4 : undefined });
        spec.placement = { ...spec.placement, at: 'self' };
        break;
      case 'trap': spec.behaviors.push({ type: 'restrain', target: tref }); spec.placement = { ...spec.placement, at: 'around_target', target: tref }; break;
      case 'lift': spec.behaviors.push({ type: 'lift', target: tref, height: 10 }); break;
      case 'sweep': case 'clear': spec.behaviors.push({ type: 'sweep', width: 16, speed: 14 }); break;
      default:
        if (rel.verb === 'fly' || rel.verb === 'glide') { spec.behaviors.push({ type: 'thrust', accel: 20, axis: [0, 0, 1] }); }
        else if (rel.bareAdjs.length) this.applyMaterials(spec, rel.bareAdjs.map((a) => a), ctx);
        else ctx.notes.push(`ignored “${rel.raw.join(' ')}”`);
    }
  }

  // --------------------------------------------------------------------------- actions

  private opAction(c: Clause, ctx: Ctx, instrumentNP: NP | null): Op[] {
    const v = c.vclass!;
    const verb = c.verb!;
    const ops: Op[] = [];
    // incoming projectiles as the object ("catch the rockets", "stop the bullets", "fling them back")
    const PROJ = /\b(rockets?|missiles?|bullets?|shots?|lasers?|projectiles?|fire|fireballs?|bolts?|incoming|barrage|volley)\b/;
    const objText = (c.objects[0]?.tokens ?? []).join(' ');
    const threatScene = this.w.projectiles.size > 0 || [...this.w.entities.values()].some((e) => e.type === 'turret' || e.hostile && e.type === 'drone');
    const flingBack = (v === 'throw' || v === 'reflect' || v === 'push' || v === 'launch') && (c.particle === 'back' || /\bback\b/.test(c.raw.join(' '))) && !(ctx.focus && this.w.constructs.cargoOf(ctx.focus.id).length);
    if (threatScene && (PROJ.test(objText) && ['catch', 'grab', 'stop', 'block', 'trap', 'hold', 'reflect', 'throw', 'swing', 'strike', 'crush', 'protect'].includes(v) || flingBack && (ctx.focus || PROJ.test(objText) || c.objects[0]?.ref))) {
      const withP = c.preps.find((p) => p.prep === 'with');
      const wantReflect = flingBack || v === 'reflect' || /\b(back|return|reflect|fling|bounce)\b/.test(c.raw.join(' '));
      const holder = withP?.np.ref ? this.resolveConstructs(withP.np.ref, ctx, withP.np)[0] : flingBack ? ctx.focus : null;
      if (holder) {
        const s = cloneSpec(holder.spec);
        s.placement = { ...s.placement, at: 'between', target: undefined };
        (s as any)._replace = true;
        s.behaviors = s.behaviors.filter((b) => !['hold', 'orbit', 'strike', 'grab'].includes(b.type));
        if (wantReflect && !s.behaviors.some((b) => b.type === 'reflect')) s.behaviors.push({ type: 'reflect' });
        if (s.motion === 'dynamic') s.motion = 'static';
        ctx.intents.add('modify'); ctx.intents.add('act');
        ctx.modTags.push('add_behavior');
        return [{ op: 'modify', target: holder.id, spec: s, mods: ['add_behavior', 'reorient'] }];
      }
      const inst = withP?.np.concept ? withP.np : null;
      const cid = inst?.concept?.id ?? (wantReflect ? 'shield' : 'wall');
      const spec = this.specFromConcept(conceptByName.get(cid)!, inst, null, ctx);
      spec.behaviors = spec.behaviors.filter((b) => b.type !== 'hold');
      spec.motion = spec.motion === 'dynamic' ? 'static' : spec.behaviors.some((b) => DRIVES.has(b.type)) ? spec.motion : 'static';
      spec.placement = { ...spec.placement, at: 'between', orient: 'face_away' };
      if (wantReflect) spec.behaviors.push({ type: 'reflect' });
      ctx.intents.add('create');
      return [{ op: 'create', spec }];
    }
    // patient/target selection
    const objNP = c.objects[0] ?? null;
    let patient = objNP && (objNP.ref || !objNP.concept && !objNP.unknownHead) ? objNP : null;
    let instrument: NP | null = instrumentNP ?? (objNP && !patient ? objNP : null);
    // "with a hammer" / "with it"
    const withP = c.preps.find((p) => p.prep === 'with');
    if (withP && (withP.np.concept || withP.np.unknownHead || withP.np.ref)) instrument = withP.np;
    // destination / target preps
    const destP = c.preps.find((p) => ['at', 'into', 'onto', 'on', 'to', 'toward', 'towards', 'against', 'through', 'off', 'over', 'across', 'up to', 'under', 'around'].includes(p.prep) && p !== withP);
    // causative subject is the instrument ("make the fingers grab him", "make them orbit me")
    let subjectConstructs: Construct[] = [];
    if (c.causative && c.subject) {
      subjectConstructs = this.resolveConstructs(c.subject.ref, ctx, c.subject);
      if (!subjectConstructs.length && c.subject.concept && ctx.focus) {
        // "the fingers" of the current construct: reshape the focused construct into that form
        subjectConstructs = [ctx.focus];
        (c as any).reshapeTo = c.subject.concept;
      }
    }
    // entity / point target
    const targetNP = patient ?? destP?.np ?? null;
    let tgt = this.refFor(targetNP, ctx, v);
    if (!tgt.ref && patient == null && destP) tgt = this.refFor(destP.np, ctx, v);
    if (tgt.ref) ctx.targetKinds.add(tgt.kind);
    if (tgt.kind === 'missing') { ctx.confidence -= 0.3; }
    const selfTarget = tgt.ref?.kind === 'self';
    const speedK = [...c.bareAdjs.map((a) => SPEED_ADJ[a] ?? 1), ...(instrument?.speeds ?? [])].reduce((a, b) => a * b, 1);

    // helpers
    const focus = subjectConstructs[0] ?? (instrument?.ref?.kind === 'construct' ? this.resolveConstructs(instrument.ref, ctx)[0] : null) ?? null;
    const useFocus = (want: (c: Construct) => boolean) => {
      if (focus) return focus;
      if (instrument && !instrument.ref) return null;
      if (ctx.focus && want(ctx.focus)) return ctx.focus;
      return null;
    };
    const create = (conceptId: string, np: NP | null, beh: Behavior[], placement?: Partial<ConstructSpec['placement']>, extra?: (s: ConstructSpec) => void): ConstructSpec => {
      let concept = conceptByName.get(conceptId)!;
      let spec: ConstructSpec;
      if (np && (np.concept || np.unknownHead)) {
        const specs = this.objectsToSpecs([np], { ...c, preps: [] } as Clause, ctx, v);
        spec = specs[0] ?? this.specFromConcept(concept, np, null, ctx);
        concept = conceptByName.get(spec.concept) ?? concept;
      } else {
        spec = this.specFromConcept(concept, np, null, ctx);
        ctx.defaultsUsed++;
      }
      // strip default target-y behaviours that the action replaces
      spec.behaviors = spec.behaviors.filter((b) => !(['strike', 'grab', 'restrain'].includes(b.type) && beh.some((x) => x.type === b.type)));
      spec.behaviors.push(...beh);
      if (placement) spec.placement = { ...spec.placement, ...placement } as any;
      extra?.(spec);
      return spec;
    };
    const modifyWith = (con: Construct, change: (s: ConstructSpec) => void, tags: string[]): Op => {
      const s = cloneSpec(con.spec);
      const reshape: Concept | undefined = (c as any).reshapeTo;
      if (reshape && reshape.id !== s.concept) {
        const old = specExtent(s).radius;
        s.parts = buildForm(reshape.form);
        s.concept = reshape.id;
        s.label = s.label.replace(/[^ ]+$/, reshape.id);
        const nr = specExtent({ ...s, scale: 1 }).radius;
        s.scale = old / Math.max(0.1, nr);
        tags.push('reshape');
      }
      change(s);
      ctx.modTags.push(...tags);
      return { op: 'modify', target: con.id, spec: s, mods: tags };
    };
    const needTarget = (): boolean => {
      if (tgt.ref) return true;
      const d = this.defaultTarget(ctx);
      tgt = { ref: d, entities: d.kind === 'entity' ? [this.w.entities.get(d.id)!] : [], kind: d.kind === 'entity' ? (ctx.aimEntity ? 'aimed_entity' : 'nearest_enemy') : 'location' };
      ctx.targetKinds.add(tgt.kind);
      if (d.kind === 'entity') this.lastEntityId = d.id;
      return true;
    };
    const intent = (x: 'create' | 'act' | 'modify') => ctx.intents.add(x);

    switch (v) {
      case 'strike': case 'swing': {
        const held = focus ?? ctx.focus;
        if (v === 'swing' && held && this.w.constructs.cargoOf(held.id).length) {
          intent('act');
          return [modifyWith(held, (s) => s.behaviors.push({ type: 'orbit', around: { kind: 'self' }, radius: 4.5 + specExtent(s).radius * 0.5, speed: 6 * speedK }), ['add_behavior'])];
        }
        needTarget();
        const heavy = (con: { spec: ConstructSpec }) => conceptByName.get(con.spec.concept)?.cls === 'heavy';
        const wield = useFocus((f) => f.spec.motion !== 'static' && !f.spec.behaviors.some((b) => b.type === 'mount') && f.radius < 12);
        const strikeB: Behavior = { type: 'strike', target: tgt.ref!, speed: 42 * speedK, after: 'return', repeat: c.bareAdjs.includes('again') ? 1 : 0 };
        if (wield) {
          intent('act');
          return [modifyWith(wield, (s) => { s.behaviors = s.behaviors.filter((b) => b.type !== 'strike'); s.behaviors.push({ ...strikeB, after: heavy(wield) ? 'stay' : 'return' } as Behavior); }, ['add_behavior'])];
        }
        const cid = instrument?.concept?.id ?? STRIKE_DEFAULT[verb] ?? 'fist';
        const spec = create(cid, instrument, [], { at: 'hand', orient: 'along_look' });
        const cls = conceptByName.get(spec.concept)?.cls;
        if (cls === 'heavy' && !spec.behaviors.some((b) => b.type === 'strike')) {
          spec.motion = 'dynamic'; spec.placement = { at: 'above_target', target: tgt.ref!, orient: 'face_away', height: 8 }; spec.behaviors.push({ type: 'crush', target: tgt.ref! });
        } else {
          const count = tgt.entities.length > 1 ? Math.min(tgt.entities.length, 6) : spec.count;
          spec.count = Math.max(spec.count, count);
          if (spec.count > 1) spec.arrangement = 'arc';
          spec.behaviors = spec.behaviors.filter((b) => b.type !== 'home' || cls === 'projectile');
          if (cls === 'projectile' && spec.behaviors.some((b) => b.type === 'home')) spec.behaviors = spec.behaviors.map((b) => (b.type === 'home' ? { ...b, target: tgt.ref! } : b));
          else spec.behaviors.push(strikeB);
          if (spec.motion === 'static' || spec.motion === 'dynamic') spec.motion = 'kinematic';
        }
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'grab': case 'hold': case 'catch': case 'rescue': case 'pull': case 'lift': case 'reel': {
        // catching / saving something that is falling: cushion underneath (or a hand if asked)
        needTarget();
        const ents = tgt.entities;
        const falling = ents.some((e) => this.w.entityVel(e).y < -3 || this.w.entityPos(e).y > 6 && e.type === 'civilian');
        if ((v === 'catch' || v === 'rescue') && tgt.ref?.kind === 'point' && /bullet|shot|projectile|laser|rocket|missile/.test(c.raw.join(' '))) {
          intent('create');
          return [{ op: 'create', spec: create('wall', null, [], { at: 'between', orient: 'face_away' }) }];
        }
        if ((v === 'catch' || v === 'rescue') && falling && !(instrument?.concept && ['manipulator'].includes(instrument.concept.cls))) {
          const cid = instrument?.concept?.id ?? 'net';
          const spec = create(cid, instrument, [{ type: 'cushion' }], { at: 'under_target', target: tgt.ref!, orient: 'face_away' }, (s) => { s.soft = true; s.motion = 'static'; s.scale *= 1.3; });
          intent('create'); intent('act');
          return [{ op: 'create', spec }];
        }
        if (v === 'rescue' && !falling) {
          const spec = create('dome', null, [], { at: 'around_target', target: tgt.ref!, orient: 'face_away' });
          intent('create');
          return [{ op: 'create', spec }];
        }
        if (selfTarget && (v === 'pull' || v === 'reel' || v === 'lift')) {
          // "pull me up there" / "lift me up": an elevator or a tether toward the aim point
          if (v === 'lift') { intent('create'); return [{ op: 'create', spec: create('elevator', null, [], { at: 'under_self', orient: 'face_away' }, (s) => { s.behaviors = [{ type: 'lift', target: { kind: 'self' }, height: Math.max(8, Math.min(40, ctx.aimPoint.y - this.w.player.pos.y + 3)) }]; }) }]; }
          intent('create');
          return [{ op: 'create', spec: create('grappling hook', null, [], { at: 'aim', orient: 'face_away' }) }];
        }
        const then: 'hold' | 'bring' | 'lift' = v === 'pull' || v === 'reel' || c.particle === 'over' || c.particle === 'in' || c.preps.some((p) => p.prep === 'to' && p.np.ref?.kind === 'self') ? 'bring' : v === 'lift' || c.particle === 'up' && v !== 'grab' ? 'lift' : 'hold';
        const extra: Behavior[] = then === 'lift' ? [{ type: 'lift', target: tgt.ref!, height: 10 }] : [];
        const wield = useFocus((f) => ['manipulator', 'rope'].includes(conceptByName.get(f.spec.concept)?.cls ?? '') || !!(c as any).reshapeTo);
        if (wield) {
          intent('act');
          return [modifyWith(wield, (s) => { s.behaviors = s.behaviors.filter((b) => !['grab', 'strike', 'orbit'].includes(b.type)); s.behaviors.push({ type: 'grab', target: tgt.ref!, then }, ...extra); }, ['add_behavior'])];
        }
        const cid = instrument?.concept?.id ?? (verb === 'lasso' || verb === 'hook' ? 'lasso' : 'hand');
        const count = tgt.entities.length > 1 ? Math.min(tgt.entities.length, 6) : 1;
        const spec = create(cid, instrument, [{ type: 'grab', target: tgt.ref!, then }, ...extra], { at: 'hand', orient: 'along_look' }, (s) => {
          if (count > 1) { s.count = count; s.arrangement = 'arc'; }
          if (s.motion !== 'kinematic') s.motion = 'kinematic';
          const heavyTarget = ents.some((e) => e.type === 'boulder');
          if (heavyTarget) s.scale = Math.max(s.scale, 3.2);
        });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'throw': {
        // destination: explicit "at/into X", else crosshair
        const dest = destP ? this.refFor(destP.np, ctx, 'throw') : { ref: null as Ref | null, entities: [] as Entity[], kind: 'none' };
        const destRef: Ref = dest.ref && dest.ref.kind !== 'self' ? dest.ref : { kind: 'point', p: ctx.aimPoint.toArray() as Vec3 };
        if (dest.ref) ctx.targetKinds.add(dest.kind);
        const speed = 42 * speedK;
        // holding something? throw that
        const holder = focus ?? [...this.w.constructs.list.values()].reverse().find((x) => this.w.constructs.cargoOf(x.id).length);
        const patientEnts = patient ? this.refFor(patient, ctx, 'throw').entities : [];
        if (holder && (this.w.constructs.cargoOf(holder.id).length || !patientEnts.length && (!patient || patient.ref?.kind === 'construct'))) {
          intent('act');
          return [{ op: 'act', target: holder.id, action: 'throw', ref: destRef, speed }];
        }
        if (patientEnts.length) {
          // not holding it yet: grab, then throw when the grip closes
          const spec = create('hand', null, [{ type: 'grab', target: { kind: 'entity', id: patientEnts[0].id }, then: 'hold' }], { at: 'hand', orient: 'along_look' });
          ctx.pending.push({ kind: 'throw_after_grab', ref: destRef, speed, opIndex: 0 });
          intent('create'); intent('act');
          return [{ op: 'create', spec }];
        }
        // throw a new object ("throw a boulder at him")
        const cid = instrument?.concept?.id ?? 'ball';
        const spec = create(cid, instrument, [], { at: 'hand', orient: 'along_look' }, (s) => { s.motion = 'kinematic'; s.behaviors = s.behaviors.filter((b) => b.type !== 'hold'); });
        ctx.pending.push({ kind: 'throw_after_grab', ref: tgt.ref && tgt.ref.kind !== 'self' ? tgt.ref : destRef, speed, opIndex: 0 });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'spin': case 'orbit': {
        const around = c.preps.find((p) => ['around', 'round'].includes(p.prep));
        const aroundRef = around ? this.refFor(around.np, ctx, 'orbit').ref : null;
        // "spin him around": whoever is held gets swung around the player
        const holder = focus ?? ctx.focus;
        const holding = holder ? this.w.constructs.cargoOf(holder.id).length > 0 : false;
        if ((v === 'spin' && holding) || (v === 'orbit' && holder && !objNP?.concept)) {
          const center: Ref = aroundRef ?? (v === 'orbit' && objNP?.ref?.kind === 'self' ? { kind: 'self' } : { kind: 'self' });
          intent(holding ? 'act' : 'modify');
          return [modifyWith(holder!, (s) => { s.behaviors = s.behaviors.filter((b) => b.type !== 'orbit' && b.type !== 'hold'); s.behaviors.push({ type: 'orbit', around: center, radius: 4 + specExtent(s).radius * 0.6, speed: (v === 'spin' ? 6 : 2.5) * speedK }); }, ['add_behavior'])];
        }
        if (v === 'spin' && patient && tgt.entities.length && !holding) {
          // grab it first, then swing it
          const spec = create('hand', null, [{ type: 'grab', target: tgt.ref!, then: 'hold' }], { at: 'hand', orient: 'along_look' });
          ctx.pending.push({ kind: 'orbit_after_grab', opIndex: 0 });
          intent('create'); intent('act');
          return [{ op: 'create', spec }];
        }
        if (instrument && (instrument.concept || instrument.unknownHead)) {
          const center: Ref = aroundRef ?? (tgt.ref && tgt.ref.kind !== 'point' ? tgt.ref : { kind: 'self' });
          const spec = create(instrument.concept?.id ?? 'ball', instrument, [], {}, (s) => {
            if (v === 'orbit' || aroundRef) { s.behaviors.push({ type: 'orbit', around: center, radius: 4 + specExtent(s).radius, speed: 2.5 * speedK }); s.placement = { at: center.kind === 'self' ? 'self' : 'around_target', target: center.kind === 'self' ? undefined : center, orient: 'face_away' }; if (center.kind === 'self') s.passPlayer = true; }
            else s.behaviors.push({ type: 'spin', rate: 8 * speedK, axis: [0, 1, 0] });
          });
          intent('create');
          return [{ op: 'create', spec }];
        }
        if (holder) {
          intent('modify');
          return [modifyWith(holder, (s) => { if (aroundRef) s.behaviors.push({ type: 'orbit', around: aroundRef, radius: 4 + specExtent(s).radius, speed: 2.5 * speedK }); else s.behaviors.push({ type: 'spin', rate: 8 * speedK, axis: [0, 1, 0] }); }, ['add_behavior'])];
        }
        return [{ op: 'abstain', reason: 'spin what? make something first' }];
      }
      case 'crush': case 'drop': {
        needTarget();
        const cid = instrument?.concept && conceptByName.get(instrument.concept.id)?.cls !== 'manipulator' ? instrument.concept.id : verb === 'squeeze' ? 'hand' : 'anvil';
        if (verb === 'squeeze' || (instrument?.concept && instrument.concept.cls === 'manipulator')) {
          const holder = useFocus((f) => conceptByName.get(f.spec.concept)?.cls === 'manipulator');
          if (holder) { intent('act'); return [modifyWith(holder, (s) => { s.behaviors = s.behaviors.filter((b) => b.type !== 'grab'); s.behaviors.push({ type: 'grab', target: tgt.ref!, then: 'hold' }, { type: 'strike', target: tgt.ref!, speed: 20, after: 'stay' }); }, ['add_behavior'])]; }
        }
        const spec = create(cid, instrument, [{ type: 'crush', target: tgt.ref! }], { at: 'above_target', target: tgt.ref!, orient: 'face_away', height: 8 }, (s) => { s.motion = 'dynamic'; s.density = Math.max(s.density, 3000); });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'trap': case 'stop': {
        if (v === 'stop' && !patient && !tgt.ref) {
          const f = focus ?? ctx.focus;
          if (!f) return [{ op: 'abstain', reason: 'stop what?' }];
          intent('modify');
          return [modifyWith(f, (s) => { s.behaviors = s.behaviors.filter((b) => !['orbit', 'spin', 'strike', 'sweep', 'home', 'thrust', 'shoot', 'follow'].includes(b.type)); }, ['remove_behavior'])];
        }
        if (v === 'stop' && /bullet|shot|projectile|laser|rocket|missile|fire/.test(c.raw.join(' '))) {
          intent('create');
          return [{ op: 'create', spec: create('wall', null, [], { at: 'between', orient: 'face_away' }) }];
        }
        needTarget();
        if (v === 'stop' && tgt.ref?.kind === 'construct') {
          const con = this.w.constructs.get((tgt.ref as any).id)!;
          intent('modify');
          return [modifyWith(con, (s) => { s.behaviors = s.behaviors.filter((b) => !['orbit', 'spin', 'strike', 'sweep', 'home', 'thrust'].includes(b.type)); }, ['remove_behavior'])];
        }
        const cid = instrument?.concept?.id ?? (verb === 'net' ? 'net' : verb === 'bubble' ? 'bubble' : verb === 'freeze' ? 'box trap' : 'cage');
        const count = tgt.entities.length > 1 ? Math.min(tgt.entities.length, 6) : 1;
        if (count > 1) {
          const ops: Op[] = tgt.entities.slice(0, 6).map((e) => ({ op: 'create' as const, spec: create(cid, instrument, [{ type: 'restrain', target: { kind: 'entity', id: e.id } }], { at: 'around_target', target: { kind: 'entity', id: e.id }, orient: 'face_away' }, (s) => { s.motion = 'kinematic'; s.behaviors = s.behaviors.filter((b) => b.type !== 'cushion'); }) }));
          intent('create'); intent('act');
          return ops;
        }
        const spec = create(cid, instrument, [{ type: 'restrain', target: tgt.ref! }], { at: 'around_target', target: tgt.ref!, orient: 'face_away' }, (s) => { s.motion = 'kinematic'; s.behaviors = s.behaviors.filter((b) => b.type !== 'cushion'); });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'block': case 'protect': case 'reflect': {
        const reflect = v === 'reflect' || c.particle === 'back' || /\b(back|reflect|return|deflect|bounce|fling)\b/.test(c.raw.join(' ')) && /\b(back|at (them|him|it|the))\b/.test(c.raw.join(' '));
        const protectee = patient && patient.ref && patient.ref.kind !== 'self' && v === 'protect' ? this.refFor(patient, ctx, 'protect') : null;
        if (protectee?.ref && protectee.ref.kind === 'entity' && !this.w.entities.get((protectee.ref as any).id)?.hostile) {
          const cid = instrument?.concept?.id ?? 'dome';
          intent('create');
          return [{ op: 'create', spec: create(cid, instrument, reflect ? [{ type: 'reflect' }] : [], { at: 'around_target', target: protectee.ref, orient: 'face_away' }) }];
        }
        const threat = tgt.ref && tgt.ref.kind === 'entity' && this.w.entities.get(tgt.ref.id)?.hostile ? tgt.ref : undefined;
        const cid = instrument?.concept?.id ?? (reflect ? 'shield' : v === 'protect' && selfTarget && !this.w.constructs.threat() ? 'dome' : 'wall');
        const spec = create(cid, instrument, reflect ? [{ type: 'reflect' }] : [], cid === 'dome' || cid === 'bubble' ? { at: 'self', orient: 'face_away' } : { at: 'between', target: threat, orient: 'face_away' }, (s) => {
          if (s.behaviors.some((b) => b.type === 'hold')) { s.behaviors = s.behaviors.filter((b) => b.type !== 'hold'); s.motion = 'static'; }
        });
        intent('create');
        return [{ op: 'create', spec }];
      }
      case 'shoot': {
        needTarget();
        // shooting an object: a net/cage traps, a rocket homes, a ball/boulder strikes
        if (instrument && (instrument.concept || instrument.unknownHead) && !['blaster', 'cannon', 'bow', 'turret'].includes(instrument.concept?.id ?? '')) {
          const cls = instrument.concept?.cls;
          if (cls === 'enclosure') { intent('create'); intent('act'); return [{ op: 'create', spec: create(instrument.concept!.id, instrument, [{ type: 'restrain', target: tgt.ref! }], { at: 'around_target', target: tgt.ref!, orient: 'face_away' }, (s) => { s.motion = 'kinematic'; s.behaviors = s.behaviors.filter((b) => b.type !== 'cushion'); }) }]; }
          const spec = create(instrument.concept?.id ?? 'ball', instrument, [], { at: 'hand', orient: 'along_look' }, (s) => {
            s.motion = 'kinematic';
            if (!s.behaviors.some((b) => b.type === 'home')) s.behaviors.push({ type: 'strike', target: tgt.ref!, speed: 55 * speedK, after: 'dissolve' });
            else s.behaviors = s.behaviors.map((b) => (b.type === 'home' ? { ...b, target: tgt.ref! } : b));
            const n = Math.max(s.count, tgt.entities.length > 1 ? Math.min(6, tgt.entities.length) : 1);
            s.count = n; if (n > 1) s.arrangement = 'arc';
          });
          intent('create'); intent('act');
          return [{ op: 'create', spec }];
        }
        const holder = useFocus((f) => f.spec.behaviors.some((b) => b.type === 'shoot'));
        if (holder) { intent('act'); return [modifyWith(holder, (s) => { s.behaviors = s.behaviors.map((b) => (b.type === 'shoot' ? { ...b, target: tgt.ref! } : b)); }, ['retarget'])]; }
        const cid = instrument?.concept?.id ?? 'blaster';
        const spec = create(cid, instrument, [], cid === 'turret' ? {} : { at: 'hand', orient: 'along_look' });
        spec.behaviors = spec.behaviors.map((b) => (b.type === 'shoot' ? { ...b, target: tgt.ref!, rate: b.rate * speedK } : b));
        if (!spec.behaviors.some((b) => b.type === 'shoot')) spec.behaviors.push({ type: 'shoot', target: tgt.ref!, rate: 3, speed: 55, size: 0.3 });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'launch': case 'climb': case 'cross': case 'carry': case 'ride': case 'descend': {
        const aimFar = ctx.aimPoint.distanceTo(this.w.player.pos);
        const toPlace = destP ? this.refFor(destP.np, ctx, v).ref : null;
        const destPoint: Vec3 = toPlace && toPlace.kind === 'point' ? toPlace.p : ctx.aimPoint.toArray() as Vec3;
        // vehicles / wearables named explicitly: "ride a dragon", "fly me with wings"
        const vehicle = instrument?.concept && ['vehicle', 'wearable', 'creature'].includes(instrument.concept.cls) ? instrument : objNP?.concept && ['vehicle', 'wearable', 'creature'].includes(objNP.concept.cls) ? objNP : null;
        if (vehicle) { intent('create'); return [{ op: 'create', spec: create(vehicle.concept!.id, vehicle, [], { at: 'self', orient: 'face_away' }) }]; }
        if (v === 'descend') { intent('create'); return [{ op: 'create', spec: create('parachute', null, [], { at: 'self', orient: 'face_away' }) }]; }
        if (v === 'launch' && patient && tgt.entities.length) {
          // launch someone else: spring under them
          intent('create'); intent('act');
          return [{ op: 'create', spec: create('spring', null, [], { at: 'under_target', target: tgt.ref!, orient: 'face_away' }, (s) => { s.behaviors = [{ type: 'launch', speed: 30, dir: 'up' }]; }) }];
        }
        if (v === 'cross' && !instrument) {
          intent('create');
          if (aimFar > 70) return [{ op: 'create', spec: create('catapult', null, [], { at: 'under_self', orient: 'face_away' }, (s) => { s.behaviors = [{ type: 'launch', speed: 45, dir: 'arc', target: { kind: 'point', p: destPoint } }]; }) }];
          return [{ op: 'create', spec: create('bridge', null, [], { at: 'span', orient: 'face_away', target: toPlace ?? undefined }) }];
        }
        if (instrument && (instrument.concept || instrument.unknownHead) && !['launcher'].includes(instrument.concept?.cls ?? '')) {
          intent('create');
          const spec = create(instrument.concept?.id ?? 'platform', instrument, [], {});
          return [{ op: 'create', spec }];
        }
        // self launch toward destination (up there / the roof / across)
        const cid = instrument?.concept?.id ?? (v === 'launch' && !toPlace && c.particle !== 'over' ? 'spring' : 'launch pad');
        const target: Ref | undefined = v === 'launch' && !toPlace && !c.particle ? undefined : { kind: 'point', p: destPoint };
        const spec = create(cid, instrument, [], { at: 'under_self', orient: 'face_away' }, (s) => {
          s.behaviors = s.behaviors.filter((b) => b.type !== 'launch');
          s.behaviors.push({ type: 'launch', speed: v === 'launch' ? 34 * speedK : 40, dir: target ? 'arc' : 'up', target });
        });
        (spec as any)._launchNow = true;
        intent('create');
        return [{ op: 'create', spec }];
      }
      case 'push': case 'repel': case 'sweep': case 'clear': {
        if ((v === 'repel' || verb === 'blow') && !patient?.ref) {
          intent('create');
          return [{ op: 'create', spec: create(instrument?.concept?.id ?? 'fan', instrument, [], { at: 'front', orient: 'face_away' }) }];
        }
        needTarget();
        if (v === 'clear' || v === 'sweep') {
          intent('create');
          return [{ op: 'create', spec: create(instrument?.concept?.id ?? 'pusher', instrument, [], { at: 'front', orient: 'face_away' }, (s) => { s.behaviors = s.behaviors.filter((b) => b.type !== 'sweep'); s.behaviors.push({ type: 'sweep', width: 20, speed: 12 }); s.scale = Math.max(s.scale, 2.4); }) }];
        }
        const heavyTarget = tgt.entities.some((e) => e.type === 'boulder');
        const wield = useFocus((f) => f.spec.motion !== 'static' && !f.spec.behaviors.some((b) => b.type === 'mount'));
        if (wield) { intent('act'); return [modifyWith(wield, (s) => { s.behaviors = s.behaviors.filter((b) => b.type !== 'strike'); s.behaviors.push({ type: 'strike', target: tgt.ref!, speed: 30 * speedK, after: 'return' }); }, ['add_behavior'])]; }
        const spec = create(instrument?.concept?.id ?? 'hand', instrument, [{ type: 'strike', target: tgt.ref!, speed: (heavyTarget ? 30 : 28) * speedK, after: 'dissolve' }], { at: 'hand', orient: 'along_look' }, (s) => { s.motion = 'kinematic'; if (heavyTarget) s.scale = Math.max(s.scale, 3.4); });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'attract': {
        needTarget();
        const spec = create(instrument?.concept?.id ?? 'magnet', instrument, [], { at: 'front', orient: 'face_away' }, (s) => { if (!s.behaviors.some((b) => b.type === 'field')) s.behaviors.push({ type: 'field', mode: 'attract', radius: 14, strength: 40 }); });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'follow': {
        needTarget();
        const f = focus ?? ctx.focus;
        if (f && !instrument?.concept) { intent('modify'); return [modifyWith(f, (s) => { s.behaviors = s.behaviors.filter((b) => !['follow', 'home', 'orbit', 'hold'].includes(b.type)); s.behaviors.push(tgt.ref!.kind === 'self' ? { type: 'follow', target: tgt.ref!, offset: [0, 2.5, -2] } : { type: 'home', target: tgt.ref!, speed: 18 * speedK }); }, ['add_behavior'])]; }
        const spec = create(instrument?.concept?.id ?? 'bird', instrument, [tgt.ref!.kind === 'self' ? { type: 'follow', target: tgt.ref!, offset: [0, 2.5, -2] } : { type: 'home', target: tgt.ref!, speed: 18 * speedK }], {});
        intent('create');
        return [{ op: 'create', spec }];
      }
      case 'explode': {
        const f = focus ?? (patient?.ref?.kind === 'construct' ? ctx.focus : null);
        if (f && (!tgt.entities.length)) { intent('act'); return [modifyWith(f, (s) => { s.behaviors = s.behaviors.filter((b) => b.type !== 'explode'); s.behaviors.push({ type: 'explode', radius: 7, force: 35, delay: 0.3, onContact: false }); }, ['add_behavior'])]; }
        needTarget();
        const spec = create(instrument?.concept?.id ?? 'bomb', instrument, [{ type: 'home', target: tgt.ref!, speed: 32 }, { type: 'explode', radius: 7, force: 35, delay: 6, onContact: true }], { at: 'hand', orient: 'along_look' }, (s) => { s.motion = 'kinematic'; s.behaviors = s.behaviors.filter((b, i, arr) => b.type !== 'explode' || arr.findIndex((x) => x.type === 'explode') === i); });
        intent('create'); intent('act');
        return [{ op: 'create', spec }];
      }
      case 'point': {
        const f = focus ?? ctx.focus;
        needTarget();
        if (!f) return [{ op: 'abstain', reason: 'point what?' }];
        intent('modify');
        return [modifyWith(f, (s) => { s.behaviors = s.behaviors.map((b: any) => (b.target ? { ...b, target: tgt.ref } : b)); s.placement = { ...s.placement, orient: 'toward_target', target: tgt.ref! }; }, ['retarget', 'reorient'])];
      }
      case 'dismiss': return this.opDismiss(c, ctx);
      case 'release': return this.opRelease(c, ctx);
      default:
        return [{ op: 'abstain', reason: `not sure how to “${verb}” with a construct` }];
    }
  }



  // --------------------------------------------------------------------------- System-2 compilation

  private refFromS2(r: any, ctx: Ctx): Ref | null {
    if (r && typeof r === 'object' && r.kind) return r as Ref;
    if (typeof r !== 'string') return null;
    const note = (k: string) => ctx.targetKinds.add(k);
    if (r === 'self' || r === 'me' || r === 'player') { note('self'); return { kind: 'self' }; }
    if (r === 'aimed' || r === 'aim') {
      if (ctx.aimEntity) { note('aimed_entity'); this.lastEntityId = ctx.aimEntity.id; return { kind: 'entity', id: ctx.aimEntity.id }; }
      note('location'); return { kind: 'point', p: ctx.aimPoint.toArray() as Vec3 };
    }
    if (r === 'aim_point' || r === 'there') { note('location'); return { kind: 'point', p: ctx.aimPoint.toArray() as Vec3 }; }
    if (r === 'nearest_enemy') { const e = this.w.constructs.nearestEnemy(); if (e) { note(ctx.aimEntity === e ? 'aimed_entity' : 'nearest_enemy'); return { kind: 'entity', id: e.id }; } return null; }
    if (r === 'all_hostile' || r === 'all_enemies') { note(ctx.aimEntity?.hostile ? 'aimed_entity' : 'nearest_enemy'); return { kind: 'all_enemies' }; }
    if (r === 'threat') { note('nearest_enemy'); return { kind: 'threat' }; }
    if (r === 'focus') { if (ctx.focus) { note('previous_construct'); return { kind: 'construct', id: ctx.focus.id }; } return null; }
    const m = /^(entity|construct):(.+)$/.exec(r);
    if (m && m[1] === 'entity' && this.w.entities.has(m[2])) {
      const e = this.w.entities.get(m[2])!;
      note(ctx.aimEntity === e ? 'aimed_entity' : e === this.w.constructs.nearestEnemy() ? 'nearest_enemy' : 'other');
      this.lastEntityId = e.id;
      return { kind: 'entity', id: e.id };
    }
    if (m && m[1] === 'construct' && this.w.constructs.get(m[2])) { note('previous_construct'); return { kind: 'construct', id: m[2] }; }
    return null;
  }

  private behaviorFromS2(b: any, ctx: Ctx): Behavior | null {
    if (!b || typeof b !== 'object' || typeof b.type !== 'string') return null;
    const num = (x: any, d: number) => (typeof x === 'number' && isFinite(x) ? x : d);
    const tgt = (x: any, d: Ref | null = null) => this.refFromS2(x, ctx) ?? d;
    const dflt = this.defaultTarget(ctx);
    switch (b.type) {
      case 'strike': return { type: 'strike', target: tgt(b.target, dflt)!, speed: num(b.speed, 42), after: ['return', 'stay', 'dissolve'].includes(b.after) ? b.after : 'return', repeat: b.repeat ? num(b.repeat, 0) : undefined };
      case 'grab': return { type: 'grab', target: tgt(b.target, dflt)!, then: ['hold', 'bring', 'lift', 'stay'].includes(b.then) ? b.then : 'hold' };
      case 'restrain': return { type: 'restrain', target: tgt(b.target, dflt)! };
      case 'crush': return { type: 'crush', target: tgt(b.target, dflt)! };
      case 'orbit': return { type: 'orbit', around: tgt(b.around ?? b.target, { kind: 'self' })!, radius: num(b.radius, 4), speed: num(b.speed, 2.5) };
      case 'spin': return { type: 'spin', rate: num(b.rate, 8), axis: [0, 1, 0] };
      case 'follow': return { type: 'follow', target: tgt(b.target, { kind: 'self' })!, offset: [0, 2.5, -2] };
      case 'home': return { type: 'home', target: tgt(b.target, dflt)!, speed: num(b.speed, 30) };
      case 'shoot': return { type: 'shoot', target: tgt(b.target, { kind: 'nearest_enemy' })!, rate: num(b.rate, 2), speed: num(b.speed, 50), size: num(b.size, 0.3) };
      case 'launch': return { type: 'launch', speed: num(b.speed, 28), dir: ['up', 'arc', 'normal', 'forward'].includes(b.dir) ? b.dir : 'up', ...(b.target ? { target: tgt(b.target) ?? undefined } : {}) };
      case 'cushion': return { type: 'cushion' };
      case 'reflect': return { type: 'reflect' };
      case 'field': return { type: 'field', mode: ['attract', 'repel', 'wind', 'lift'].includes(b.mode) ? b.mode : 'repel', radius: num(b.radius, 12), strength: num(b.strength, 35) };
      case 'conveyor': return { type: 'conveyor', speed: num(b.speed, 12), axis: [0, 0, 1] };
      case 'thrust': return { type: 'thrust', accel: num(b.accel, 25), axis: [0, 0, 1] };
      case 'explode': return { type: 'explode', radius: num(b.radius, 6), force: num(b.force, 32), delay: num(b.delay, 2), onContact: !!b.onContact };
      case 'lift': return { type: 'lift', target: tgt(b.target, { kind: 'self' })!, height: num(b.height, 12) };
      case 'sweep': return { type: 'sweep', width: num(b.width, 16), speed: num(b.speed, 14) };
      case 'mount': return { type: 'mount', mode: b.mode === 'wear' ? 'wear' : 'ride', glide: b.glide !== undefined ? num(b.glide, 1) : undefined, thrustLook: b.thrustLook !== undefined ? num(b.thrustLook, 15) : undefined, thrustUp: b.thrustUp !== undefined ? num(b.thrustUp, 28) : undefined, maxFall: b.maxFall !== undefined ? num(b.maxFall, 5) : undefined, groundSpeedMul: b.groundSpeedMul !== undefined ? num(b.groundSpeedMul, 2) : undefined, bounce: b.bounce !== undefined ? num(b.bounce, 14) : undefined, frictionMul: b.groundSpeedMul ? 0.4 : undefined };
      case 'tether': return { type: 'tether', target: { kind: 'none' }, pull: 16, toPlayer: true };
      case 'hold': return { type: 'hold', side: ['right', 'left', 'front', 'back', 'overhead'].includes(b.side) ? b.side : 'right' };
      default: ctx.notes.push(`ignored unknown behaviour “${String(b.type).slice(0, 20)}”`); return null;
    }
  }

  /** Compile model output into ordinary ops. Anything malformed is dropped (and noted), never guessed at. */
  compileS2(raw: any[], ctx: Ctx): Op[] {
    const out: Op[] = [];
    const PLACES = new Set(['aim', 'front', 'ground_front', 'self', 'under_self', 'behind_self', 'above_self', 'hand', 'target', 'under_target', 'above_target', 'around_target', 'between', 'span']);
    const ENTITYACTS = new Set(['strike', 'grab', 'restrain', 'crush', 'home', 'shoot']);
    const findConcept = (id: any, label?: any): Concept | null => {
      if (typeof id === 'string') { const c = conceptByName.get(id.toLowerCase()) ?? CONCEPTS.find((x) => x.id === id.toLowerCase()); if (c) return c; }
      if (this.embedder && (typeof id === 'string' || typeof label === 'string')) {
        const r = this.embedder.nearest(String(id ?? label), 1);
        if (r[0] && r[0].score >= 0.5) return CONCEPTS.find((x) => x.id === r[0].id) ?? null;
      }
      return null;
    };
    const applyChanges = (sp: ConstructSpec, ch: any, tags: string[]) => {
      if (!ch || typeof ch !== 'object') return;
      if (typeof ch.scale_mul === 'number' && isFinite(ch.scale_mul) && ch.scale_mul > 0) { sp.scale *= ch.scale_mul; tags.push(ch.scale_mul >= 1 ? 'scale_up' : 'scale_down'); }
      if (Array.isArray(ch.stretch_mul) && ch.stretch_mul.length === 3 && ch.stretch_mul.every((x: any) => typeof x === 'number' && x > 0)) { const st = sp.stretch ?? [1, 1, 1]; sp.stretch = [st[0] * ch.stretch_mul[0], st[1] * ch.stretch_mul[1], st[2] * ch.stretch_mul[2]]; tags.push(ch.stretch_mul.reduce((a: number, b: number) => a * b, 1) >= 1 ? 'scale_up' : 'scale_down'); }
      if (typeof ch.speed_mul === 'number' && isFinite(ch.speed_mul) && ch.speed_mul > 0) { this.scaleSpeeds(sp, ch.speed_mul); tags.push(ch.speed_mul >= 1 ? 'faster' : 'slower'); }
      if (typeof ch.count === 'number' && ch.count >= 1) { sp.count = Math.floor(ch.count); if (sp.arrangement === 'single') sp.arrangement = 'line'; tags.push('count_change'); }
      if (typeof ch.density_mul === 'number' && ch.density_mul > 0) { sp.density *= ch.density_mul; tags.push(ch.density_mul >= 1 ? 'heavier' : 'lighter'); }
      if (ch.concept) {
        const c = findConcept(ch.concept);
        if (c) { const r0 = specExtent(sp).radius; sp.parts = buildForm(c.form); sp.concept = c.id; sp.label = c.id; sp.scale = r0 / Math.max(0.2, specExtent({ ...sp, scale: 1, stretch: undefined }).radius); tags.push('add_part'); }
      }
      if (Array.isArray(ch.remove_behaviors)) { sp.behaviors = sp.behaviors.filter((b) => !ch.remove_behaviors.includes(b.type)); tags.push('remove_behavior'); }
      if (Array.isArray(ch.add_behaviors)) for (const b of ch.add_behaviors) { const bb = this.behaviorFromS2(b, ctx); if (bb) { sp.behaviors = sp.behaviors.filter((x) => x.type !== bb.type); sp.behaviors.push(bb); tags.push('add_behavior'); } }
      if (Array.isArray(ch.addons)) for (const a of ch.addons) { const ad = ADDONS_ALL.find((x) => x.id === a?.id || x.names.includes(a?.id)); if (ad) { this.addAddon(sp, ad, a.where ?? null, ctx); tags.push('add_part'); } }
      if (typeof ch.tilt === 'number') { sp.placement = { ...sp.placement, tilt: ch.tilt }; tags.push('reorient'); }
      if (typeof ch.yaw === 'number') { sp.placement = { ...sp.placement, yaw: ch.yaw }; tags.push('reorient'); }
      if (ch.placement && PLACES.has(ch.placement.at)) { sp.placement = { ...sp.placement, at: ch.placement.at, target: this.refFromS2(ch.placement.target, ctx) ?? undefined }; (sp as any)._replace = true; tags.push('reorient'); }
    };
    for (const o of raw.slice(0, 6)) {
      if (!o || typeof o !== 'object') continue;
      switch (o.op) {
        case 'create': {
          const c = findConcept(o.concept, o.label);
          if (!c) { ctx.notes.push(`model proposed unknown concept “${String(o.concept).slice(0, 20)}”`); continue; }
          const sp = this.specFromConcept(c, null, null, ctx);
          if (typeof o.label === 'string') sp.label = o.label.slice(0, 60);
          if (typeof o.scale === 'number' && isFinite(o.scale) && o.scale > 0) sp.scale *= o.scale;
          if (typeof o.count === 'number' && o.count >= 1) sp.count = Math.floor(Math.min(o.count, 1e9));
          if (['single', 'line', 'ring', 'arc', 'stack', 'grid', 'scatter'].includes(o.arrangement)) sp.arrangement = o.arrangement;
          if (sp.count > 1 && sp.arrangement === 'single') sp.arrangement = 'line';
          if (['static', 'kinematic', 'dynamic'].includes(o.motion)) sp.motion = o.motion;
          if (o.soft) { sp.soft = true; if (!sp.behaviors.some((b) => b.type === 'cushion')) sp.behaviors.push({ type: 'cushion' }); }
          if (o.placement && PLACES.has(o.placement.at)) sp.placement = { ...sp.placement, at: o.placement.at, target: this.refFromS2(o.placement.target, ctx) ?? undefined };
          if (Array.isArray(o.addons)) for (const a of o.addons) { const ad = ADDONS_ALL.find((x) => x.id === a?.id || x.names.includes(a?.id)); if (ad) this.addAddon(sp, ad, a.where ?? null, ctx); }
          if (typeof o.tilt === 'number') sp.placement.tilt = o.tilt;
          if (Array.isArray(o.behaviors)) {
            const bs = o.behaviors.map((b: any) => this.behaviorFromS2(b, ctx)).filter(Boolean) as Behavior[];
            const types = new Set(bs.map((b) => b.type));
            sp.behaviors = [...sp.behaviors.filter((b) => !types.has(b.type) && !(b.type === 'hold' && (types.has('strike') || types.has('grab')))), ...bs];
          }
          sp.provenance = { source: 'llm', confidence: 0.6, notes: [] };
          ctx.intents.add('create');
          if (sp.behaviors.some((b: any) => ENTITYACTS.has(b.type))) ctx.intents.add('act');
          out.push({ op: 'create', spec: sp });
          break;
        }
        case 'modify': {
          const tr = this.refFromS2(o.target === undefined ? 'focus' : String(o.target).startsWith('construct:') || o.target === 'focus' ? o.target : `construct:${o.target}`, ctx);
          const con = tr?.kind === 'construct' ? this.w.constructs.get(tr.id) : ctx.focus;
          if (!con) { ctx.notes.push('model referred to a construct that does not exist'); continue; }
          const sp = cloneSpec(con.spec);
          const tags: string[] = [];
          applyChanges(sp, o.changes, tags);
          const mods = (Array.isArray(o.mods) ? o.mods.filter((m: any) => typeof m === 'string') : []).concat(tags);
          if (!mods.length) continue;
          ctx.modTags.push(...mods);
          ctx.intents.add('modify');
          if ((o.changes?.add_behaviors ?? []).some((b: any) => ENTITYACTS.has(b?.type))) ctx.intents.add('act');
          sp.provenance = { ...sp.provenance, source: 'llm' };
          out.push({ op: 'modify', target: con.id, spec: sp, mods: [...new Set(mods)] as string[] });
          break;
        }
        case 'act': {
          const tr = this.refFromS2(o.target === undefined || o.target === 'focus' ? 'focus' : String(o.target).startsWith('construct:') ? o.target : `construct:${o.target}`, ctx);
          const id = tr?.kind === 'construct' ? tr.id : ctx.focus?.id;
          if (!id || !['throw', 'release', 'dismiss'].includes(o.action)) continue;
          const at = o.at ? this.refFromS2(o.at, ctx) ?? undefined : undefined;
          ctx.intents.add(o.action === 'throw' ? 'act' : 'control');
          if (o.action === 'dismiss') ctx.modTags.push('dismiss');
          if (o.action === 'release') ctx.modTags.push('release');
          out.push({ op: 'act', target: id, action: o.action, ref: at, speed: 42 });
          break;
        }
        case 'control': {
          if (!['dismiss_all', 'undo', 'save', 'release_all'].includes(o.action)) continue;
          ctx.intents.add('control');
          ctx.modTags.push(o.action === 'dismiss_all' ? 'dismiss' : o.action === 'release_all' ? 'release' : o.action);
          if (o.action === 'release_all') { for (const c of this.w.constructs.list.values()) if (this.w.constructs.cargoOf(c.id).length) out.push({ op: 'act', target: c.id, action: 'release' }); continue; }
          out.push({ op: 'control', action: o.action, name: typeof o.name === 'string' ? o.name.slice(0, 30) : undefined, target: ctx.focus?.id });
          break;
        }
        case 'abstain':
          out.push({ op: 'abstain', reason: typeof o.reason === 'string' ? o.reason.slice(0, 140) : 'the model was unsure' });
          break;
      }
    }
    return out;
  }

  // --------------------------------------------------------------------------- revision-1 abstractions

  /** "the bridge" / "that hammer" refer to an existing construct when one exists; "the wall" may be scenery. */
  private bindDefinites(c: Clause, ctx: Ctx) {
    const nps: NP[] = [...c.objects, ...c.preps.map((p) => p.np), ...(c.subject ? [c.subject] : [])];
    const fromBefore = c.preps.some((p) => p.prep === 'from' && p.np.tokens.some((w) => ['before', 'earlier', 'last', 'previously', 'yesterday', 'ago'].includes(w)));
    for (const np of nps) {
      if (np.ref || !np.concept || !np.det) continue;
      if (!['the', 'that', 'this', 'those', 'these', 'my', 'its', 'your'].includes(np.det)) continue;
      if (fromBefore) { (np as any).fromBefore = true; continue; }
      const list = [...this.w.constructs.list.values()].filter((x) => x.spec.concept === np.concept!.id || np.conceptWords && x.spec.label.includes(np.conceptWords));
      if (list.length) { np.ref = { kind: np.plural ? 'constructs' : 'construct', id: list[list.length - 1].id }; continue; }
      if (PLACE_WORDS.has(np.conceptWords ?? '')) np.ref = { kind: 'place', place: np.conceptWords! };
    }
  }

  /** Verb meaning depends on what it is applied to (affordance-driven reinterpretation). */
  private reinterpret(c: Clause, ctx: Ctx): Op[] | null {
    const v = c.vclass;
    const verb = c.verb ?? '';
    const obj = c.objects[0] ?? null;
    const toks = c.raw.join(' ');
    const cls = obj?.concept?.cls;
    const destP = c.preps.find((p) => ['into', 'at', 'to', 'toward', 'towards', 'onto', 'on', 'under', 'underneath', 'below', 'beneath', 'over', 'above', 'around', 'behind', 'next to', 'near', 'through', 'across', 'against'].includes(p.prep));
    const deictic = !!destP && destP.np.ref?.kind === 'construct' && destP.np.tokens.length === 1 && ['that', 'this', 'there', 'it'].includes(destP.np.tokens[0]);
    const destNP: NP | null = destP ? (deictic ? { ...destP.np, ref: { kind: 'aim' } } : destP.np) : null;
    const entityDest = destNP ? this.refFor(destNP, ctx, 'strike') : null;
    // 1) "throw/drop/toss a bridge across", "throw up a wall": quick creation, not hurling
    const STRUCT = ['surface', 'ramp', 'barrier', 'launcher', 'enclosure', 'container', 'field'];
    if (['throw', 'drop'].includes(v ?? '') && obj && !obj.ref && obj.concept && STRUCT.includes(cls!) && !(cls === 'enclosure' && entityDest?.entities.length && obj.concept.id !== 'dome')) {
      const specs = this.objectsToSpecs([obj], c, ctx);
      ctx.intents.add('create');
      return specs.map((spec) => ({ op: 'create', spec }));
    }
    // 2) "drive it into that", "send those at the drones", "fly it under the civilian": move/ram an existing construct
    const MOVERS = ['drive', 'fly', 'send', 'move', 'steer', 'ram', 'roll', 'slide', 'shove', 'push', 'launch', 'fling', 'toss', 'hurl', 'throw', 'chuck', 'aim', 'point', 'bring', 'take', 'put', 'carry'];
    const constructObj = obj?.ref && (obj.ref.kind === 'construct' || obj.ref.kind === 'constructs') ? obj : c.subject?.ref && (c.subject.ref.kind === 'construct' || c.subject.ref.kind === 'constructs') ? c.subject : null;
    if (constructObj && MOVERS.includes(verb) && destP) {
      const cons = this.resolveConstructs(constructObj.ref, ctx, constructObj);
      const carrying = cons.some((x) => this.w.constructs.cargoOf(x.id).length);
      if (cons.length && !(carrying && ['throw', 'fling', 'toss', 'hurl', 'chuck', 'send'].includes(verb))) {
        const placeish = ['under', 'underneath', 'below', 'beneath', 'over', 'above', 'around', 'behind', 'next to', 'near', 'across'].includes(destP.prep);
        const ramPoint = !placeish && entityDest?.ref?.kind === 'point' && ['drive', 'ram', 'send', 'fling', 'hurl', 'throw', 'launch', 'shove', 'push', 'roll', 'slide', 'chuck', 'toss'].includes(verb) && ['into', 'at', 'against', 'through', 'toward', 'towards'].includes(destP.prep);
        if (ramPoint) {
          ctx.targetKinds.add(entityDest!.kind); ctx.intents.add('act'); ctx.modTags.push('retarget');
          return cons.map((con) => {
            const s2 = cloneSpec(con.spec);
            s2.behaviors = s2.behaviors.filter((b) => !['strike', 'hold', 'orbit', 'follow', 'mount'].includes(b.type));
            s2.behaviors.push({ type: 'strike', target: entityDest!.ref!, speed: 40, after: 'stay' });
            return { op: 'modify' as const, target: con.id, spec: s2, mods: ['retarget'] };
          });
        }
        if (entityDest?.ref && entityDest.entities.length && !placeish) {
          ctx.targetKinds.add(entityDest.kind);
          ctx.intents.add('act');
          ctx.modTags.push('retarget');
          const ents = entityDest.entities;
          return cons.map((con, k) => {
            const s2 = cloneSpec(con.spec);
            s2.behaviors = s2.behaviors.filter((b) => !['strike', 'hold', 'orbit', 'follow', 'mount'].includes(b.type));
            const t: Ref = ents.length > 1 ? { kind: 'entities', ids: ents.map((e) => e.id) } : { kind: 'entity', id: ents[k % ents.length].id };
            s2.behaviors.push({ type: 'strike', target: t, speed: 40, after: conceptByName.get(s2.concept)?.cls === 'heavy' ? 'stay' : 'return' });
            return { op: 'modify' as const, target: con.id, spec: s2, mods: ['retarget'] };
          });
        }
        // otherwise it's a relocation ("fly it under the civilian")
        ctx.intents.add('modify');
        const c2 = deictic ? { ...c, preps: c.preps.map((p) => (p === destP ? { ...p, np: destNP! } : p)) } : c;
        return cons.map((con) => {
          const s2 = cloneSpec(con.spec);
          s2.behaviors = s2.behaviors.filter((b) => !['hold', 'orbit', 'follow'].includes(b.type));
          this.applyPreps(s2, c2, ctx);
          (s2 as any)._replace = true;
          ctx.modTags.push('reorient');
          return { op: 'modify' as const, target: con.id, spec: s2, mods: ['reorient'] };
        });
      }
    }
    // 3) "make it move with me", "have it come along": follow the player
    if (['move', 'come', 'go', 'stay', 'walk', 'fly', 'float', 'hover', 'keep'].includes(verb) && /\b(with me|along|next to me|beside me|by my side|near me|close to me|on me)\b/.test(toks)) {
      const cons = constructObj ? this.resolveConstructs(constructObj.ref, ctx, constructObj) : ctx.focus ? [ctx.focus] : [];
      if (cons.length) {
        ctx.intents.add('modify'); ctx.modTags.push('add_behavior'); ctx.targetKinds.add('previous_construct');
        return cons.map((con) => {
          const s2 = cloneSpec(con.spec);
          s2.behaviors = s2.behaviors.filter((b) => !['follow', 'orbit', 'hold', 'strike', 'sweep'].includes(b.type));
          const between = s2.placement.at === 'between';
          s2.behaviors.push({ type: 'follow', target: { kind: 'self' }, offset: between ? [0, 0.5, 3] : [0, 2.5, -2] });
          if (s2.motion === 'static') s2.motion = 'kinematic';
          return { op: 'modify' as const, target: con.id, spec: s2, mods: ['add_behavior'] };
        });
      }
    }
    // 4) "take it up", "lift it all the way up", "raise the platform"
    if (['take', 'lift', 'raise', 'hoist', 'elevate', 'bring', 'move'].includes(verb) && constructObj && (c.particle === 'up' || /\bup\b/.test(toks) || verb === 'raise' || verb === 'lift' || verb === 'hoist' || verb === 'elevate')) {
      const cons = this.resolveConstructs(constructObj.ref, ctx, constructObj);
      if (cons.length) {
        const h = /\b(all the way|fully)\b/.test(toks) ? 40 : /\b(higher|more|lot|way)\b/.test(toks) ? 25 : 15;
        ctx.intents.add('act'); ctx.modTags.push('add_behavior'); ctx.targetKinds.add('previous_construct');
        return cons.map((con) => {
          const s2 = cloneSpec(con.spec);
          const onIt = this.w.player.groundCollider && con.insts.some((i) => i.colliders.includes(this.w.player.groundCollider!));
          const under = s2.placement.at === 'under_self' || onIt;
          s2.behaviors = s2.behaviors.filter((b) => b.type !== 'lift');
          s2.behaviors.push({ type: 'lift', target: under ? { kind: 'self' } : { kind: 'construct', id: con.id }, height: h });
          if (under) ctx.targetKinds.add('self');
          if (s2.motion === 'static') s2.motion = 'kinematic';
          return { op: 'modify' as const, target: con.id, spec: s2, mods: ['add_behavior'] };
        });
      }
    }
    // 5) "use that to catch her", "use the hammer on him"
    if (verb === 'use' && obj && !this.savedNameIn(c)) {
      if (c.purpose) return this.opAction({ ...c.purpose, causative: false, subject: obj.ref ? obj : null } as Clause, ctx, obj);
      const onP = c.preps.find((p) => ['on', 'against', 'at'].includes(p.prep));
      if (onP) return this.opAction({ ...c, vclass: 'strike', verb: 'hit', objects: [onP.np], preps: [{ prep: 'with', np: obj }] } as Clause, ctx, null);
    }
    // 6) "bring back that flying hammer from before", "the ramp from earlier again"
    const recallWords = /\b(from before|from earlier|from last time|from the start|like before|as before|the old one)\b/.test(toks) || (verb === 'bring' && c.particle === 'back');
    if (recallWords && obj?.concept) {
      const prev = [...this.createdHistory].reverse().find((sp) => sp.concept === obj.concept!.id);
      if (prev) {
        const s2 = cloneSpec(prev);
        s2.provenance = { source: 'cache', confidence: 0.9, notes: ['recalled from earlier'] };
        ctx.modTags.push('recall'); ctx.intents.add('control'); ctx.intents.add('create');
        return [{ op: 'create', spec: s2 }];
      }
    }
    // 7) underspecified: "help!", "deal with that", "up", "across"
    const bare = c.raw.filter((w) => !['please', 'quick', 'quickly', 'now', 'hurry', 'go', 'ring', 'me', 'us', 'somebody', 'someone', 'uh', 'um', 'oh', 'god', 'no', 'just'].includes(w));
    const dirOnly = bare.length > 0 && bare.length <= 3 && bare.every((w) => ['up', 'upward', 'upwards', 'higher', 'down', 'downward', 'across', 'over', 'there', 'get', 'take', 'lift', 'fly', 'send', 'jump', 'boost', 'all', 'the', 'way'].includes(w)) && bare.some((w) => ['up', 'upward', 'upwards', 'higher', 'down', 'downward', 'across', 'over'].includes(w));
    if (v === 'handle' || dirOnly) return this.situational(c, ctx, dirOnly ? bare : null);
    return null;
  }

  /** Pick the most useful function the scene calls for; visible as an assumption, lower confidence. */
  private situational(c: Clause, ctx: Ctx, dirWords: string[] | null): Op[] {
    const P = this.w.player;
    const note = (m: string) => { ctx.approximations.push(`assumed: ${m}`); ctx.confidence = Math.min(ctx.confidence, 0.6); };
    const empty: Clause = { ...c, objects: [], preps: [], purpose: null, causative: false, subject: null, particle: null, bareAdjs: [] };
    if (dirWords) {
      if (dirWords.some((w) => w.startsWith('down'))) { note('get you down safely'); return this.opAction({ ...empty, vclass: 'descend', verb: 'descend' }, ctx, null); }
      if (dirWords.some((w) => w === 'across' || w === 'over')) { note('get you across'); return this.opAction({ ...empty, vclass: 'cross', verb: 'cross' }, ctx, null); }
      note('get you up there'); ctx.targetKinds.add('self');
      return this.opAction({ ...empty, vclass: 'climb', verb: 'climb' }, ctx, null);
    }
    const ents = [...this.w.entities.values()];
    const falling = ents.find((e) => e.type === 'civilian' && !e.heldBy && !e.safe && (this.w.entityVel(e).y < -2 || this.w.entityPos(e).y > 5));
    if (falling) {
      note(`catch the falling ${falling.label}`);
      const np = { ...this.blankNP(), ref: { kind: 'entity', entityType: 'civilian', which: 'that' } } as NP;
      return this.opAction({ ...empty, vclass: 'catch', verb: 'catch', objects: [np] }, ctx, null);
    }
    const aimed = ctx.aimEntity && ctx.aimEntity.hostile && !ctx.aimEntity.disabled ? ctx.aimEntity : null;
    const objRef = c.objects[0]?.ref;
    if (aimed && (objRef || /\b(that|this|it|him|them)\b/.test(c.raw.join(' ')))) {
      note(`attack the ${aimed.type}`);
      return this.opAction({ ...empty, vclass: 'strike', verb: 'hit', objects: [{ ...this.blankNP(), ref: { kind: 'entity', entityType: aimed.type, which: 'that' } } as NP] }, ctx, null);
    }
    const threat = this.w.projectiles.size > 0 || ents.some((e) => e.type === 'turret' && !e.disabled);
    if (threat) { note('block the incoming fire'); return this.opAction({ ...empty, vclass: 'block', verb: 'block' }, ctx, null); }
    if (aimed) { note(`attack the ${aimed.type}`); return this.opAction({ ...empty, vclass: 'strike', verb: 'hit' }, ctx, null); }
    return [{ op: 'abstain', reason: 'help how? tell me what to make or do' }];
  }

  private blankNP(): NP {
    return { tokens: [], det: null, count: null, plural: false, sizes: [], dims: [], speeds: [], materials: [], colors: [], concept: null, conceptWords: null, unknownHead: null, unknownWords: [], ignoredWords: [], addons: [], rel: [], ref: null, analogy: false, arrangement: null };
  }

  /** Function-first composition: make sure what we built actually serves the stated goal. */
  private ensureGoal(goal: VerbClass, specs: ConstructSpec[], ctx: Ctx, purpose: Clause): Op[] {
    const have = new Set(specs.flatMap((sp) => functionsOf(sp, this.w)));
    const need: Record<string, string[]> = { climb: ['raise_player'], launch: ['launch_player', 'raise_player'], ride: ['propel_player'], carry: ['raise_player', 'propel_player'], cross: ['bridge_gap'], descend: ['slow_fall'] };
    const req = need[goal];
    if (!req || req.some((f) => have.has(f))) return [];
    const aim = ctx.aimPoint;
    const P = this.w.player;
    const empty: Clause = { ...purpose, objects: [], preps: purpose.preps, purpose: null, causative: false, subject: null, bareAdjs: [] };
    ctx.approximations.push(`added a helper so you can ${purpose.verb ?? goal}`);
    if (goal === 'climb' && aim.y - P.pos.y > 4 && aim.distanceTo(P.pos) < 70) {
      const hook = this.specFromConcept(conceptByName.get('grappling hook')!, null, null, ctx);
      hook.placement = { at: 'aim', orient: 'face_away' };
      return [{ op: 'create', spec: hook }];
    }
    return this.opAction({ ...empty, vclass: goal === 'launch' || goal === 'carry' ? 'climb' : goal, verb: goal }, ctx, null);
  }

  // --------------------------------------------------------------------------- modify / add / control

  private opModify(c: Clause, ctx: Ctx, obj: NP | null): Op[] {
    const targets = obj?.ref || obj?.concept ? this.resolveConstructs(obj.ref, ctx, obj) : ctx.focus ? [ctx.focus] : [];
    const intoP = c.preps.find((p) => p.prep === 'into' || p.prep === 'to' && (p.np.concept || p.np.unknownHead));
    if (!targets.length) {
      if (intoP?.np.concept) return [{ op: 'create', spec: this.specFromConcept(intoP.np.concept, intoP.np, null, ctx) }];
      return [{ op: 'abstain', reason: 'there’s nothing to change yet — make something first' }];
    }
    const ops: Op[] = [];
    for (const con of targets) {
      const s = cloneSpec(con.spec);
      const tags: string[] = [];
      const adjs = [...c.bareAdjs, ...(obj?.tokens ?? [])];
      const verb = c.verb ?? '';
      let size = 1;
      const toks = c.raw.join(' ');
      for (const a of c.bareAdjs) if (a in SIZE) size *= SIZE[a];
      if (/\b(bigger|larger|grow|enlarge|expand|inflate|embiggen|supersize|upsize)\b/.test(toks) && size === 1) size = 1.6;
      if (/\b(way|much|lot|really|super|even) (bigger|larger)\b/.test(toks)) size = Math.max(size, 2.5);
      if (/\b(smaller|shrink|downsize|tinier|littler)\b/.test(toks) && size === 1) size = 0.6;
      if (/\bdouble (the )?size\b|twice as (big|large)\b/.test(toks)) size = 2;
      if (/\bhalf (the )?size\b/.test(toks)) size = 0.5;
      const timesM = toks.match(/(\d+) times (bigger|larger|as big)/);
      if (timesM) size = parseInt(timesM[1], 10);
      if (size !== 1) { s.scale *= size; tags.push(size > 1 ? 'scale_up' : 'scale_down'); }
      // dimensions
      const st: Vec3 = [...(s.stretch ?? [1, 1, 1])] as Vec3;
      let dimChanged = false;
      for (const a of adjs) { const d = DIM[a]; if (d) { st[0] *= d[0]; st[1] *= d[1]; st[2] *= d[2]; dimChanged = true; } }
      if (/\b(stretch|lengthen|extend)\b/.test(verb)) { st[2] *= 1.6; dimChanged = true; }
      if (/\bwiden\b/.test(verb)) { st[0] *= 1.6; dimChanged = true; }
      if (/\bthicken\b/.test(verb)) { st[0] *= 1.4; st[1] *= 1.4; dimChanged = true; }
      if (dimChanged) { s.stretch = st; tags.push(st[0] * st[1] * st[2] >= 1 ? 'scale_up' : 'scale_down'); }
      // speed
      let sp = 1;
      for (const a of adjs) if (SPEED_ADJ[a]) sp *= SPEED_ADJ[a];
      if (/\bspeed (it|them) up\b/.test(toks)) sp *= 1.5;
      if (sp !== 1) {
        const had = this.scaleSpeeds(s, sp);
        if (!had) {
          // nothing moves yet: "faster" on a static thing makes it spin; on a vehicle it's mount speed
          ctx.notes.push('it wasn’t moving — gave it motion');
          s.behaviors.push({ type: 'spin', rate: 6 * sp, axis: [0, 1, 0] });
        }
        tags.push(sp > 1 ? 'faster' : 'slower');
      }
      // materials
      const mats = adjs.map((a) => MATERIAL[a]).filter(Boolean);
      if (mats.length) {
        this.applyMaterials(s, mats, ctx);
        for (const m of mats) tags.push(m === 'heavy' ? 'heavier' : m === 'light' ? 'lighter' : ['vertical', 'sideways', 'upsidedown', 'reverse', 'flat'].includes(m) ? 'reorient' : 'add_behavior');
      }
      // reorient verbs
      if (/\b(flip|reverse|invert|turn it around|turn (it|them) around|backwards)\b/.test(toks)) {
        s.placement = { ...s.placement, yaw: ((s.placement.yaw ?? 0) + 180) % 360 };
        s.behaviors = s.behaviors.map((b) => (b.type === 'conveyor' ? { ...b, speed: -b.speed } : b.type === 'orbit' ? { ...b, speed: -b.speed } : b.type === 'spin' ? { ...b, rate: -b.rate } : b));
        tags.push('reorient');
      }
      if (/\b(tilt|lean|angle)\b/.test(toks)) { s.placement = { ...s.placement, tilt: (s.placement.tilt ?? 0) + 30 }; tags.push('reorient'); }
      if (/\bupside down\b/.test(toks)) { s.placement = { ...s.placement, tilt: 180 }; tags.push('reorient'); }
      if (/\b(vertical|upright|stand (it|them) up)\b/.test(toks) && !tags.includes('reorient')) { s.placement = { ...s.placement, tilt: 90 }; tags.push('reorient'); }
      if (/\b(horizontal|lay (it|them) (down|flat))\b/.test(toks)) { s.placement = { ...s.placement, tilt: 0 }; tags.push('reorient'); }
      // "turn it into a X"
      if (intoP && (intoP.np.concept || intoP.np.unknownHead)) {
        const { concept } = this.resolveConcept(intoP.np, ctx);
        if (concept) {
          const r0 = specExtent(s).radius;
          s.parts = buildForm(concept.form);
          s.concept = concept.id;
          s.label = this.labelFor(concept, intoP.np);
          const newBeh = (concept.beh ?? []).map((b) => this.fillTarget(b, null, ctx));
          s.behaviors = [...s.behaviors.filter((b) => !newBeh.some((n) => n.type === b.type)), ...newBeh];
          if (concept.motion !== 'static' && s.motion === 'static') s.motion = concept.motion;
          if (concept.soft) { s.soft = true; }
          s.scale = Math.max(0.3, r0 / Math.max(0.2, specExtent({ ...s, scale: 1, stretch: undefined }).radius)) * Math.max(0.5, (intoP.np.sizes[0] ?? 1));
          for (const a of intoP.np.addons) this.addAddon(s, a.addon, a.where, ctx);
          tags.push('add_part');
        }
      }
      // count ("make five of them", "three more")
      const countM = toks.match(/\b(\d+|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|dozen|twenty|hundred|thousand)\b(?! times)/);
      if (countM && /\b(of (them|those|it|these)|copies|more|of these|times as many)\b/.test(toks)) {
        const n = /^\d+$/.test(countM[1]) ? parseInt(countM[1], 10) : ({ two: 2, three: 3, four: 4, five: 5, six: 6, seven: 7, eight: 8, nine: 9, ten: 10, eleven: 11, twelve: 12, dozen: 12, twenty: 20, hundred: 100, thousand: 1000 } as any)[countM[1]];
        s.count = /\bmore\b/.test(toks) ? s.count + n : n;
        if (s.arrangement === 'single') s.arrangement = s.behaviors.some((b) => b.type === 'orbit') ? 'ring' : 'line';
        tags.push('count_change');
      }
      if (!tags.length) {
        if (c.verb === 'turn' || c.verb === 'change' || c.verb === 'transform') { ops.push({ op: 'abstain', reason: 'turn it into what?' }); continue; }
        ops.push({ op: 'abstain', reason: `not sure how to change it (“${c.raw.join(' ')}”)` });
        continue;
      }
      s.provenance = { ...s.provenance, source: 'edit' };
      ctx.modTags.push(...tags);
      ctx.intents.add('modify');
      ops.push({ op: 'modify', target: con.id, spec: s, mods: tags });
    }
    return ops;
  }

  private opAdd(c: Clause, ctx: Ctx): Op[] {
    // target construct: "on/to X" or "give it ..." or focus
    const onP = c.preps.find((p) => ['on', 'onto', 'to', 'in'].includes(p.prep));
    let targets: Construct[] = [];
    if (onP) targets = this.resolveConstructs(onP.np.ref, ctx, onP.np);
    const giveObj = c.objects.find((o) => o.ref?.kind === 'construct' || o.ref?.kind === 'constructs');
    if (!targets.length && giveObj) targets = this.resolveConstructs(giveObj.ref, ctx, giveObj);
    if (!targets.length && ctx.focus) targets = [ctx.focus];
    if (!targets.length) {
      // "put a trampoline on the ground" etc: that's a create
      const specs = this.objectsToSpecs(c.objects.filter((o) => o.concept || o.unknownHead), c, ctx);
      if (specs.length) { ctx.intents.add('create'); return specs.map((spec) => ({ op: 'create', spec })); }
      return [{ op: 'abstain', reason: 'add it to what? make something first' }];
    }
    const target = targets[0];
    const addNPs = c.objects.filter((o) => o !== giveObj);
    // attach existing constructs ("attach those to the shield")
    const attachRefs = addNPs.filter((o) => o.ref?.kind === 'constructs' || o.ref?.kind === 'construct');
    const s = cloneSpec(target.spec);
    const tags: string[] = [];
    const extraOps: Op[] = [];
    for (const r of attachRefs) {
      const srcs = this.resolveConstructs(r.ref, ctx, r).filter((x) => x.id !== target.id);
      for (const src of srcs) {
        const k = src.spec.scale / s.scale;
        const mp = mountPoint(s.parts, 'front')[0];
        for (const p of src.spec.parts) s.parts.push({ ...p, size: p.size.map((x) => x * k) as Vec3, pos: p.pos.map((x, i) => x * k + mp.pos[i]) as Vec3 });
        s.behaviors.push(...src.spec.behaviors.filter((b) => !['hold', 'orbit', 'follow', 'mount'].includes(b.type)));
        extraOps.push({ op: 'act', target: src.id, action: 'dismiss' });
        tags.push('attach');
      }
    }
    for (const np of addNPs) {
      if (np.ref) continue;
      const where = (c as any).relationWhere ?? null;
      if (np.addons.length) for (const a of np.addons) { for (let i = 0; i < 1; i++) this.addAddon(s, a.addon, a.where ?? where, ctx); tags.push('add_part'); }
      else if (np.concept) {
        const asAddon = ADDON_BY_CONCEPT(np.concept.id);
        if (asAddon) { this.addAddon(s, asAddon, where, ctx); tags.push('add_part'); }
        else {
          // generic: bolt the concept's geometry onto the front
          const parts = buildForm(np.concept.form);
          const mp = mountPoint(s.parts, (where as any) ?? 'front')[0];
          const k = 0.5 * specExtent({ ...s, scale: 1, stretch: undefined }).radius / Math.max(0.3, specExtent(defaultSpec({ parts })).radius);
          for (const p of parts) s.parts.push({ ...p, size: p.size.map((x) => x * k) as Vec3, pos: p.pos.map((x, i) => x * k + mp.pos[i]) as Vec3 });
          for (const b of np.concept.beh ?? []) if (!['hold', 'mount', 'restrain'].includes(b.type)) s.behaviors.push(this.fillTarget(b, null, ctx));
          tags.push('add_part');
        }
      } else if (np.unknownHead) {
        // unknown part: approximate as generic hard-light bars at the requested spot, and say so
        const where = ((c as any).relationWhere ?? (np.tokens.includes('sides') || np.tokens.includes('side') ? 'sides' : 'front')) as any;
        const mp = mountPoint(s.parts, where);
        const e = specExtent({ ...s, scale: 1, stretch: undefined });
        for (const m of mp) s.parts.push({ shape: 'box', size: [0.15, Math.max(0.4, e.size.y * 0.35), Math.max(0.6, e.size.z * 0.9)], pos: [m.pos[0], m.pos[1] + e.size.y * 0.2, m.pos[2]], role: np.unknownHead });
        ctx.approximations.push(`“${np.unknownHead}” → generic hard-light bars`);
        ctx.unknownWords.push(np.unknownHead);
        tags.push('add_part');
      }
    }
    // "add a turret to it that shoots the drones" etc
    for (const np of addNPs) for (const rel of np.rel) this.applyRelClause(s, rel, ctx);
    if (!tags.length) return [{ op: 'abstain', reason: `couldn’t work out what to add (“${c.raw.join(' ')}”)` }];
    s.label = target.spec.label.includes('with') ? target.spec.label : target.spec.label;
    s.provenance = { ...s.provenance, source: 'edit' };
    ctx.modTags.push(...tags);
    ctx.intents.add('modify');
    return [{ op: 'modify', target: target.id, spec: s, mods: tags }, ...extraOps];
  }

  private opMove(c: Clause, ctx: Ctx): Op[] {
    const obj = c.objects[0];
    const targets = this.resolveConstructs(obj.ref, ctx, obj);
    if (!targets.length) return [{ op: 'abstain', reason: 'move what?' }];
    const ops: Op[] = [];
    for (const con of targets) {
      const s = cloneSpec(con.spec);
      this.applyPreps(s, c, ctx);
      if (!c.preps.length || c.preps.every((p) => p.np.ref?.kind === 'aim')) s.placement = { ...s.placement, at: 'aim' };
      (s as any)._replace = true;
      ctx.modTags.push('reorient');
      ctx.intents.add('modify');
      ops.push({ op: 'modify', target: con.id, spec: s, mods: ['reorient'] });
    }
    return ops;
  }

  private opDismiss(c: Clause, ctx: Ctx): Op[] {
    const obj = c.objects[0] ?? null;
    ctx.intents.add('control');
    const toks = c.raw.join(' ');
    ctx.modTags.push('dismiss');
    if (/\b(everything|all|them all|all of them|every construct|the lot|clear all|everything)\b/.test(toks) && !obj?.ref?.entityType) return [{ op: 'control', action: 'dismiss_all' }];
    // "get rid of the drones" means attack them
    if (obj?.ref?.kind === 'entity') return this.opAction({ ...c, vclass: 'strike', verb: 'destroy' }, ctx, null);
    const targets = obj ? this.resolveConstructs(obj.ref, ctx, obj) : ctx.focus ? [ctx.focus] : [];
    if (!targets.length) return [{ op: 'abstain', reason: 'nothing to dismiss' }];
    ctx.targetKinds.add('previous_construct');
    return targets.map((t) => ({ op: 'act', target: t.id, action: 'dismiss' }));
  }

  private opRelease(c: Clause, ctx: Ctx): Op[] {
    ctx.intents.add('control');
    ctx.modTags.push('release');
    if (/\b(everything|all)\b/.test(c.raw.join(' '))) ctx.modTags.push('dismiss');
    const obj = c.objects[0] ?? null;
    const holders = [...this.w.constructs.list.values()].filter((x) => this.w.constructs.cargoOf(x.id).length || x.spec.behaviors.some((b) => b.type === 'restrain' || b.type === 'grab'));
    const t = obj?.ref?.kind === 'construct' ? this.resolveConstructs(obj.ref, ctx, obj) : [];
    const list = t.length ? t : holders.length ? holders : ctx.focus ? [ctx.focus] : [];
    const ops: Op[] = list.map((x) => ({ op: 'act', target: x.id, action: 'release' }));
    if (this.w.player.tethers.L || this.w.player.tethers.R) ops.push({ op: 'control', action: 'tether_release' });
    return ops.length ? ops : [{ op: 'abstain', reason: 'not holding anything' }];
  }

  private opDuplicate(c: Clause, ctx: Ctx): Op[] {
    const toks = c.raw.join(' ');
    const src = ctx.focus?.spec ?? this.lastCreated;
    if (!src) return [{ op: 'abstain', reason: 'nothing to repeat yet' }];
    if (/\bdouble\b/.test(toks) && ctx.focus) {
      const s = cloneSpec(ctx.focus.spec);
      if (/\bsize\b/.test(toks)) { s.scale *= 2; ctx.modTags.push('scale_up'); }
      else { s.count = Math.max(2, s.count * 2); if (s.arrangement === 'single') s.arrangement = 'line'; ctx.modTags.push('count_change', 'duplicate'); }
      ctx.intents.add('modify');
      return [{ op: 'modify', target: ctx.focus.id, spec: s, mods: ['duplicate', 'count_change'] }];
    }
    const s = cloneSpec(src);
    s.provenance = { ...s.provenance, source: 'cache' };
    ctx.modTags.push('duplicate');
    ctx.intents.add('create');
    return [{ op: 'create', spec: s }];
  }

  private opSave(c: Clause, ctx: Ctx): Op[] {
    ctx.intents.add('control');
    if (!ctx.focus) return [{ op: 'abstain', reason: 'nothing to save yet' }];
    const name = (c.savedName ?? 'design').replace(/[^a-z0-9 ]/g, '').trim() || 'design';
    ctx.modTags.push('save');
    return [{ op: 'control', action: 'save', name, target: ctx.focus.id }];
  }

  private savedNameIn(c: Clause): string | null {
    const t = c.raw.join(' ');
    for (const name of this.saved.keys()) if (new RegExp(`\\b${name}\\b`).test(t)) return name;
    return null;
  }

  private opRecall(name: string, c: Clause, ctx: Ctx): Op[] {
    const s = cloneSpec(this.saved.get(name)!);
    s.provenance = { source: 'cache', confidence: 0.95, notes: [`recalled “${name}”`] };
    this.applyPreps(s, c, ctx);
    ctx.modTags.push('recall');
    ctx.intents.add('control');
    ctx.intents.add('create');
    return [{ op: 'create', spec: s, name }];
  }

  // =========================================================================== applying

  private apply(ops: Op[], ctx: Ctx): { ids: string[]; changed: boolean; clamps: string[]; messages: string[] } {
    const ids: string[] = [];
    const clamps: string[] = [];
    const messages: string[] = [];
    let changed = false;
    const createdIds: string[] = [];
    ops.forEach((op, idx) => {
      switch (op.op) {
        case 'create': {
          const r = this.w.constructs.spawn(op.spec, { name: op.name });
          clamps.push(...r.report.clamps);
          if (r.construct) {
            ids.push(r.construct.id);
            createdIds.push(r.construct.id);
            this.lastCreated = cloneSpec(op.spec);
            this.createdHistory.push(cloneSpec(r.construct.spec));
            if (this.createdHistory.length > 40) this.createdHistory.shift();
            if ((op.spec as any)._launchNow) {
              const launch = r.construct.spec.behaviors.find((b) => b.type === 'launch') as any;
              if (launch?.target) this.w.constructs.launchPlayerTo(this.w.constructs.refPos(launch.target)!, launch.speed);
              else { this.w.player.vel.y = Math.max(this.w.player.vel.y, launch?.speed ?? 30); this.w.player.onGround = false; }
              this.w.emit('launch', { construct: r.construct.id, speed: Math.round(this.w.player.vel.length()) });
            }
            for (const p of ctx.pending.filter((q) => q.opIndex === idx || q.opIndex === 0)) this.pendings.push({ p: { ...p, construct: r.construct.id }, ids: [r.construct.id], deadline: this.w.time + 4 });
          } else messages.push(`✖ ${r.report.rejected}`);
          break;
        }
        case 'modify': {
          const con = this.w.constructs.get(op.target);
          if (!con) { messages.push('✖ that construct is gone'); break; }
          const prev = cloneSpec(con.spec);
          if ((op.spec as any)._replace) {
            // re-place: rebuild at a new location
            const frame = this.w.constructs.place(op.spec);
            for (const inst of con.insts) { inst.pos.copy(frame.pos); inst.quat.copy(frame.quat); }
          }
          const r = this.w.constructs.modify(op.target, op.spec);
          clamps.push(...r.report.clamps);
          if (r.ok) { changed = true; ids.push(op.target); this.undoStack.push({ kind: 'modify', id: op.target, prev }); }
          else messages.push(`✖ ${r.report.rejected}`);
          break;
        }
        case 'act': {
          if (op.action === 'dismiss') { this.w.constructs.dismiss(op.target); changed = true; }
          else if (op.action === 'release') { this.w.constructs.release(op.target); changed = true; }
          else if (op.action === 'throw') { changed = this.w.constructs.throwAt(op.target, op.ref ?? { kind: 'aim' }, op.speed ?? 40) || changed; }
          break;
        }
        case 'control': {
          changed = true;
          if (op.action === 'dismiss_all') this.w.constructs.dismissAll();
          else if (op.action === 'undo') this.undo(messages);
          else if (op.action === 'tether_release') { this.w.player.releaseTether('L'); this.w.player.releaseTether('R'); }
          else if (op.action === 'save' && op.target) {
            const con = this.w.constructs.get(op.target);
            if (con) { const s = cloneSpec(con.spec); this.saved.set(op.name!, s); con.name = op.name!; messages.push(`saved as “${op.name}”`); }
          }
          break;
        }
        case 'abstain':
          break;
      }
    });
    if (createdIds.length) { this.lastCreatedIds = createdIds; this.undoStack.push({ kind: 'create', ids: createdIds }); }
    if (this.undoStack.length > 50) this.undoStack.shift();
    return { ids, changed, clamps, messages };
  }

  private undo(messages: string[]) {
    const last = this.undoStack.pop();
    if (!last) { messages.push('nothing to undo'); return; }
    if (last.kind === 'create') for (const id of last.ids) this.w.constructs.dismiss(id, 'undo');
    else this.w.constructs.modify(last.id, last.prev);
    messages.push('undone');
  }

  private tickPending() {
    if (!this.pendings.length) return;
    this.pendings = this.pendings.filter((q) => {
      if (this.w.time > q.deadline) return false;
      const id = q.p.construct!;
      const con = this.w.constructs.get(id);
      if (!con) return false;
      const holding = this.w.constructs.cargoOf(id).length > 0;
      const grabbing = con.spec.behaviors.some((b) => b.type === 'grab');
      if (q.p.kind === 'throw_after_grab') {
        if (holding || !grabbing && this.w.time - con.born > 0.25) { this.w.constructs.throwAt(id, q.p.ref ?? { kind: 'aim' }, q.p.speed ?? 40); return false; }
      } else if (q.p.kind === 'orbit_after_grab' && holding) {
        const s = cloneSpec(con.spec);
        s.behaviors.push({ type: 'orbit', around: { kind: 'self' }, radius: 4.5, speed: 6 });
        this.w.constructs.modify(id, s);
        return false;
      }
      return true;
    });
  }

  // =========================================================================== reporting

  private toInterpretation(text: string, normalized: string, ops: Op[], ctx: Ctx, ms: number): Interpretation {
    const abst = ops.find((o) => o.op === 'abstain') as Extract<Op, { op: 'abstain' }> | undefined;
    const onlyAbstain = ops.length > 0 && ops.every((o) => o.op === 'abstain');
    // coverage: English words the rule layer had no role for are a sign it may have missed the point
    const uncovered = [...new Set(tokenize(normalized).filter((w) => w.length > 2 && !isGameWord(w) && !ctx.unknownWords.includes(w)))];
    let conf = Math.max(0, Math.min(1, ctx.confidence - ctx.defaultsUsed * 0.03 - ctx.unknownWords.length * 0.2 - Math.min(0.4, uncovered.length * 0.1)));
    if (onlyAbstain) conf = Math.min(conf, 0.3);
    const source: SourceKind = ops.some((o) => o.op === 'create' && o.spec.provenance.source === 'cache') ? 'cache' : ctx.source === 'embedding' ? 'embedding' : ops.some((o) => o.op === 'create' && o.spec.provenance.source === 'fallback') ? 'rule' : onlyAbstain ? 'none' : ctx.approximations.length ? 'lexicon' : 'rule';
    return {
      text: normalized,
      ops,
      summary: this.summarize(ops, ctx),
      confidence: conf,
      source,
      approximations: [...ctx.approximations, ...ctx.corrections.map((c) => `heard “${c.from}” as “${c.to}”`)],
      unknownWords: [...ctx.unknownWords, ...uncovered],
      abstain: onlyAbstain ? abst!.reason : null,
      needsEscalation: onlyAbstain && (ctx.unknownWords.length > 0 || uncovered.length > 0) || conf < 0.6 || uncovered.length >= 2 || ops.some((o) => o.op === 'create' && o.spec.provenance.source === 'fallback'),
      timings: { parseMs: ms },
    };
  }

  private summarize(ops: Op[], _ctx: Ctx): string {
    const parts: string[] = [];
    for (const op of ops) {
      if (op.op === 'create') parts.push(`${op.spec.label}${this.behSummary(op.spec)}`);
      else if (op.op === 'modify') parts.push(`${this.w.constructs.get(op.target)?.spec.label ?? 'it'} → ${op.mods.join(', ')}${this.behSummary(op.spec, this.w.constructs.get(op.target)?.spec)}`);
      else if (op.op === 'act') parts.push(`${op.action} ${this.w.constructs.get(op.target)?.spec.label ?? ''}`);
      else if (op.op === 'control') parts.push(op.action + (op.name ? ` “${op.name}”` : ''));
      else parts.push(`? ${op.reason}`);
    }
    return parts.join(' + ');
  }

  private behSummary(s: ConstructSpec, prev?: ConstructSpec): string {
    const b = s.behaviors.filter((x) => !prev || !prev.behaviors.some((p) => JSON.stringify(p) === JSON.stringify(x)));
    const words = b.map((x) => {
      const t = (x as any).target ?? (x as any).around;
      const tn = t ? this.refName(t) : '';
      switch (x.type) {
        case 'strike': return `strike ${tn}`; case 'grab': return `grab ${tn}`; case 'orbit': return `orbit ${tn}`; case 'crush': return `drop on ${tn}`; case 'restrain': return `trap ${tn}`;
        case 'shoot': return `shoot ${tn}`; case 'home': return `chase ${tn}`; case 'launch': return 'launch'; case 'thrust': return 'thrust'; case 'reflect': return 'reflect'; case 'cushion': return 'cushion';
        case 'mount': return 'ride/wear'; case 'field': return x.mode; case 'spin': return 'spin'; case 'lift': return 'lift'; case 'explode': return 'explode'; case 'tether': return 'tether';
        default: return '';
      }
    }).filter(Boolean);
    const place = s.placement.at !== 'aim' ? ` @${s.placement.at.replace('_', ' ')}` : '';
    return (words.length ? ' → ' + [...new Set(words)].join(', ') : '') + (prev ? '' : place);
  }

  private refName(r: Ref): string {
    switch (r.kind) {
      case 'self': return 'you';
      case 'entity': return this.w.entities.get(r.id)?.type ?? 'target';
      case 'entities': return `${r.ids.length} targets`;
      case 'construct': return this.w.constructs.get(r.id)?.spec.label ?? 'construct';
      case 'point': return 'there';
      case 'nearest_enemy': return 'nearest enemy';
      case 'all_enemies': return 'all enemies';
      default: return '';
    }
  }

  private contextSummary(ctx: Ctx) {
    const P = this.w.player;
    const surf = ctx.aimEntity ? null : ctx.aimPoint.distanceTo(P.pos) > 200 ? 'sky/far' : ctx.aimNormal.y > 0.7 ? (ctx.aimPoint.y > P.pos.y + 3 ? 'high ledge/roof' : 'ground') : 'wall/side of something';
    return {
      aim: ctx.aimEntity ? { entity: `entity:${ctx.aimEntity.id}`, type: ctx.aimEntity.type } : { point: ctx.aimPoint.toArray().map((x) => Math.round(x)), surface: surf, distance: Math.round(ctx.aimPoint.distanceTo(P.pos)) },
      player: { onGround: P.onGround, height: Math.round(P.pos.y), speed: Math.round(P.vel.length()) },
      focus: ctx.focus ? `construct:${ctx.focus.id}` : null,
      constructs: [...this.w.constructs.list.values()].slice(-6).map((c) => ({ id: `construct:${c.id}`, label: c.spec.label, concept: c.spec.concept, count: c.insts.length, holding: this.w.constructs.cargoOf(c.id).map((e) => `entity:${e}`) })),
      entities: [...this.w.entities.values()].slice(0, 10).map((e) => ({ id: `entity:${e.id}`, type: e.type, hostile: e.hostile, dist: Math.round(this.w.entityPos(e).distanceTo(P.pos)), height: Math.round(this.w.entityPos(e).y), falling: this.w.entityVel(e).y < -3, held: !!e.heldBy })),
      incomingFire: this.w.projectiles.size > 0,
    };
  }

  /** Functional effect report (see eval/EVAL_SPEC.md) derived from what was actually applied. */
  effectReport(res: CommitResult, ctx: Ctx, applied: { ids: string[]; changed: boolean; clamps: string[] }): EffectReport {
    const fns = new Set<string>();
    let count: number | null = null;
    const specs: { spec: ConstructSpec; created: boolean }[] = [];
    for (const op of res.ops) {
      if (op.op === 'create') specs.push({ spec: op.spec, created: true });
      if (op.op === 'modify') specs.push({ spec: op.spec, created: false });
      if (op.op === 'act' && op.action === 'throw') fns.add('throw_target');
    }
    for (const id of applied.ids) { const c = this.w.constructs.get(id); if (c) count = (count ?? 0) + c.insts.length; }
    for (const { spec } of specs) for (const f of functionsOf(spec, this.w)) fns.add(f);
    if (ctx.pending.some((p) => p.kind === 'throw_after_grab')) fns.add('throw_target');
    if (ctx.pending.some((p) => p.kind === 'orbit_after_grab')) { fns.add('orbit'); fns.add('spin'); }
    // swinging something you're holding around you is also "spinning" it
    for (const { spec } of specs) if (spec.behaviors.some((b) => b.type === 'orbit') && spec.behaviors.some((b) => b.type === 'grab')) fns.add('spin');
    const intents = [...ctx.intents];
    const abstained = !!res.abstain;
    let intent: EffectReport['intent'] = abstained ? 'abstain' : intents.includes('act') ? 'act' : (intents[0] as any) ?? 'none';
    (res as any).intents = abstained ? ['abstain'] : intents;
    const tk = [...ctx.targetKinds].filter((k) => k !== 'none' && k !== 'missing');
    const target = tk.includes('aimed_entity') ? 'aimed_entity' : tk[0] ?? null;
    return {
      intent, functions: [...fns], target, count, modifications: [...new Set(ctx.modTags)],
      approximated: res.approximations.length > 0 || specs.some((s) => s.spec.provenance.approximatedFrom),
      clamped: applied.clamps.length > 0, abstained, confidence: res.confidence,
      ...( { intents: abstained ? ['abstain'] : intents, targetKinds: tk } as any),
    };
  }
}

// ----------------------------------------------------------------------------- helpers

const DRIVES = new Set(['hold', 'follow', 'orbit', 'strike', 'grab', 'sweep', 'lift', 'mount', 'home']);
const SELF_GOALS = new Set<VerbClass>(['climb', 'cross', 'descend', 'launch', 'ride', 'carry']);
const PLACE_WORDS = new Set(['wall', 'tower', 'building', 'platform', 'door', 'ledge', 'roof', 'bridge', 'pillar', 'ground', 'floor']);

/** Self-contradicting requests should be questioned, not silently resolved. */
export function detectContradiction(raw: string): string | null {
  const t = ' ' + raw.toLowerCase().replace(/[^a-z' ]/g, ' ').replace(/\s+/g, ' ') + ' ';
  const m = t.match(/\bbut (don'?t|do not|dont|not|no|never|without)\b(.*)$/);
  if (m) {
    const head = tokenize(t.slice(0, m.index));
    const tail = tokenize(m[2]);
    const sing = (w: string) => w.replace(/s$/, '');
    const headConcepts = head.filter((w) => conceptByName.has(w) || conceptByName.has(sing(w))).map(sing);
    const sharedNoun = tail.some((w) => headConcepts.includes(sing(w)));
    const tailVerb = tail.find((w) => ['touch', 'hurt', 'hit', 'use', 'make', 'build', 'move', 'grab', 'catch', 'harm', 'break', 'damage', 'create', 'let', 'do', 'go', 'fall', 'drop', 'punch', 'save'].includes(w));
    if (sharedNoun || tailVerb) return 'that contradicts itself — do you want it or not?';
  }
  if (/\b(catch|save|grab|hold|rescue|protect)\b.*\bbut\b.*\blet (him|her|it|them|the \w+) (fall|go|drop|die|hit)\b/.test(t)) return 'catch or let fall? that contradicts itself';
  if (/\b(stop|freeze)\b.*\bbut\b.*\b(keep|let) (it|him|her|them) (moving|going|move)\b/.test(t)) return 'stop it or keep it moving? that contradicts itself';
  return null;
}
const ENTITY_VERBS = new Set<VerbClass>(['strike', 'grab', 'throw', 'pull', 'push', 'lift', 'crush', 'trap', 'catch', 'rescue', 'shoot', 'follow', 'attract', 'hold', 'swing', 'spin', 'launch', 'protect', 'explode', 'stop', 'drop', 'reel']);
import { SIZE_ADJ as SIZE, DIM_ADJ as DIM, MATERIAL_ADJ as MATERIAL } from './words.js';
import { ADDONS } from './lexicon.js';
const ADDONS_ALL = ADDONS;
const ADDON = (id: string) => ADDONS.find((a) => a.id === id)!;
const ADDON_BY_CONCEPT = (conceptId: string): Addon | null => {
  const map: Record<string, string> = { rocket: 'rocket', wheel: 'wheel', wings: 'wing', spike: 'spike', sword: 'blade', saw: 'blade', magnet: 'magnet', hook: 'hook', cannon: 'cannon', blaster: 'cannon', turret: 'cannon', fan: 'fan', spring: 'spring', chain: 'chain', rope: 'chain', hand: 'finger', fist: 'finger', shield: 'shield', parachute: 'parachute', net: 'net', bucket: 'bucket', drill: 'drill', hammer: 'hammerhead', 'jetpack': 'rocket', 'grappling hook': 'hook', claw: 'hook', umbrella: 'parachute', 'boxing glove': 'finger' };
  return map[conceptId] ? ADDON(map[conceptId]) : null;
};

function parseLite(toks: string[]): NP {
  // minimal NP for the second half of "between me and X"
  return parseNPLite(toks);
}
import { parseNP } from './parser.js';
function parseNPLite(toks: string[]): NP { return parseNP(toks, []); }

function rotForMount(where: string, a: Addon): Vec3 {
  if (a.id === 'rocket' || a.id === 'fan') return [0, 0, 0]; // thrust along +z, flame toward -z
  switch (where) {
    case 'top': return a.id === 'propeller' ? [0, 0, 0] : [0, 0, 0];
    case 'bottom': return a.id === 'wheel' ? [0, 0, 0] : [180, 0, 0];
    case 'front': case 'tip': return a.id === 'spike' ? [90, 0, 0] : [0, 0, 0];
    default: return [0, 0, 0];
  }
}
function rotate(p: Vec3, r: Vec3): Vec3 {
  if (!r[0] && !r[1] && !r[2]) return p;
  const v = new Vector3(...p).applyEuler(new (require_three_euler())(r[0] * Math.PI / 180, r[1] * Math.PI / 180, r[2] * Math.PI / 180));
  return [v.x, v.y, v.z];
}
import { Euler, Quaternion } from 'three';
function require_three_euler() { return Euler; }
function combineRot(a: Vec3, b?: Vec3): Vec3 | undefined {
  if (!b) return a[0] || a[1] || a[2] ? a : undefined;
  if (!a[0] && !a[1] && !a[2]) return b;
  const q = new Quaternion().setFromEuler(new Euler(a[0] * Math.PI / 180, a[1] * Math.PI / 180, a[2] * Math.PI / 180)).multiply(new Quaternion().setFromEuler(new Euler(b[0] * Math.PI / 180, b[1] * Math.PI / 180, b[2] * Math.PI / 180)));
  const e = new Euler().setFromQuaternion(q);
  return [e.x * 180 / Math.PI, e.y * 180 / Math.PI, e.z * 180 / Math.PI];
}

/** Map a spec to functional capabilities (claimed effects). */
export function functionsOf(spec: ConstructSpec, w: World): string[] {
  const f = new Set<string>();
  const concept = conceptByName.get(spec.concept);
  const cls = concept?.cls;
  const at = spec.placement.at;
  const types = new Set(spec.behaviors.map((b) => b.type));
  const tgt = spec.placement.target;
  const targetIsSelf = !tgt || tgt.kind === 'self';
  const solid = spec.parts.some((p) => p.solid !== false);
  if (solid && (cls === 'barrier' || cls === 'enclosure' && !types.has('restrain')) && !types.has('mount')) {
    if (at === 'between' || at === 'front' || at === 'self' || at === 'behind_self' || types.has('hold')) { f.add('block_projectiles'); f.add('shield_self'); }
    if ((at === 'around_target' || at === 'target') && tgt && tgt.kind !== 'self') { f.add('shield_other'); f.add('block_projectiles'); }
    if (at === 'aim') f.add('block_projectiles');
  }
  if (types.has('reflect')) f.add('block_projectiles');
  if (types.has('field') && spec.behaviors.some((b: any) => b.type === 'field' && (b.mode === 'wind' || b.mode === 'repel'))) { f.add('repel'); f.add('push_target'); f.add('block_projectiles'); }
  if (types.has('field') && spec.behaviors.some((b: any) => b.type === 'field' && b.mode === 'attract')) { f.add('attract'); f.add('pull_target'); }
  if (types.has('field') && spec.behaviors.some((b: any) => b.type === 'field' && b.mode === 'lift')) { f.add('lift_target'); f.add('raise_player'); }
  if (spec.motion === 'static' && solid && ['surface', 'ramp', 'launcher', 'barrier'].includes(cls ?? '') && !types.has('mount')) {
    f.add('support_player');
    if (at === 'span' || spec.concept === 'bridge') f.add('bridge_gap');
    if (['ramp', 'stairs', 'ladder', 'tower', 'pillar', 'slide'].includes(spec.concept) || cls === 'ramp') f.add('raise_player');
    if (at === 'under_self') f.add('support_player');
  }
  if (spec.concept === 'platform' || spec.concept === 'slab' || spec.concept === 'elevator') { f.add('support_player'); if (at === 'span') f.add('bridge_gap'); }
  if (types.has('lift')) { const l = spec.behaviors.find((b) => b.type === 'lift') as any; if (l.target?.kind === 'self' || at === 'under_self') { f.add('raise_player'); f.add('support_player'); } else f.add('lift_target'); }
  if (types.has('launch')) {
    const l = spec.behaviors.find((b) => b.type === 'launch') as any;
    if (at === 'under_target' && tgt && tgt.kind !== 'self' && !spec.soft) f.add('lift_target');
    else { f.add('launch_player'); if (l.dir === 'up' || l.dir === 'arc' || l.target) f.add('raise_player'); if (l.target || l.dir === 'arc') f.add('bridge_gap'); }
  }
  if (types.has('mount')) {
    const m = spec.behaviors.find((b) => b.type === 'mount') as any;
    if (m.glide || m.thrustLook || m.thrustUp || m.groundSpeedMul || types.has('thrust')) f.add('propel_player');
    if (m.glide || m.maxFall) f.add('slow_fall');
    if (m.thrustUp) f.add('raise_player');
    if (m.bounce) { f.add('launch_player'); f.add('raise_player'); }
  }
  if (types.has('tether')) { f.add('tether_player'); f.add('raise_player'); }
  if (types.has('conveyor')) { f.add('support_player'); f.add('propel_player'); }
  if (types.has('grab')) {
    const g = spec.behaviors.find((b) => b.type === 'grab') as any;
    f.add('grab_target');
    if (g.then === 'bring') f.add('pull_target');
    if (g.then === 'lift') f.add('lift_target');
    const e = g.target?.kind === 'entity' ? w.entities.get(g.target.id) : null;
    if (e && (e.type === 'civilian' || w.entityVel(e).y < -3)) f.add('catch_target');
    if (e?.type === 'boulder') f.add('move_obstacle');
    f.add('restrain_target');
  }
  if (types.has('restrain')) { f.add('restrain_target'); }
  if (types.has('cushion') || spec.soft) {
    if (at === 'under_target' || at === 'under_self' || at === 'target' || at === 'aim') f.add('catch_target');
    if (at === 'under_self') f.add('slow_fall');
  }
  if (types.has('strike')) {
    const s = spec.behaviors.find((b) => b.type === 'strike') as any;
    f.add('strike_target');
    f.add('push_target');
    const ids = s.target?.kind === 'entity' ? [s.target.id] : s.target?.kind === 'entities' ? s.target.ids : [];
    if (ids.some((id: string) => w.entities.get(id)?.type === 'boulder')) f.add('move_obstacle');
    if (ids.length > 1 || spec.count > 1) f.add('area_attack');
    if (spec.count > 1 && (spec.arrangement === 'arc' || spec.arrangement === 'line')) f.add('projectile_attack');
  }
  if (types.has('crush')) { f.add('crush_target'); f.add('strike_target'); }
  if (types.has('shoot') || types.has('home')) f.add('projectile_attack');
  if (types.has('home') && cls === 'projectile') f.add('strike_target');
  if (types.has('explode')) { f.add('area_attack'); f.add('strike_target'); f.add('push_target'); }
  if (types.has('orbit')) f.add('orbit');
  if (types.has('spin')) f.add('spin');
  if (types.has('sweep')) { f.add('sweep'); f.add('push_target'); f.add('move_obstacle'); f.add('area_attack'); }
  if (types.has('thrust') && !types.has('mount') && !types.has('strike') && spec.motion === 'dynamic') f.add('strike_target');
  if (types.has('follow') && targetIsSelf && (cls === 'barrier' || cls === 'enclosure')) { f.add('shield_self'); f.add('block_projectiles'); }
  if (types.has('orbit') && (cls === 'barrier' || cls === 'enclosure' || spec.concept === 'shield')) { f.add('block_projectiles'); f.add('shield_self'); f.add('area_attack'); }
  if (types.has('orbit') && (cls === 'weapon' || cls === 'heavy')) f.add('area_attack');
  if (cls === 'heavy' && spec.motion === 'dynamic' && at === 'above_target') { f.add('crush_target'); }
  if (types.has('hold') && cls === 'barrier') { f.add('shield_self'); f.add('block_projectiles'); }
  return [...f];
}

function isFunctionish(w: string) { return FILLERS.has(w) || ['what', 'where', 'when', 'how', 'why', 'who', 'is', 'are', 'was', 'can', 'could', 'would', 'should', 'do', 'does', 'did', 'yes', 'no', 'not'].includes(w); }

function now() { return typeof performance !== 'undefined' ? performance.now() : Date.now(); }
export type { PlacementAt, Part };
