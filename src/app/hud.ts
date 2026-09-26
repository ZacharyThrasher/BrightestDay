import type { CommitResult } from '../lang/types.js';
import type { World } from '../sim/world.js';
import type { GameRenderer } from '../render/renderer.js';
import { Vector3 } from 'three';
import { INSPIRED_AT } from '../sim/inspiration.js';
import type { Slot, CastTiming, CastBests } from './typing.js';
import { DuelHud } from './duelHud.js';

export type RingStatus = 'ready' | 'typing' | 'focus' | 'interpreting' | 'formed' | 'clamped' | 'not_understood' | 'rejected';

const STATUS_TEXT: Record<RingStatus, string> = {
  ready: 'ring ready', typing: 'imagining…', focus: 'focus · time slowed', interpreting: 'interpreting…', formed: 'formed', clamped: 'formed · clamped', not_understood: 'not understood', rejected: 'rejected',
};

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = '', parent?: HTMLElement, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  parent?.appendChild(e);
  return e;
}

/** Only touch the DOM when a value changes (keeps HUD cost ~0 per frame). */
function setText(e: HTMLElement, t: string) { if (e.textContent !== t) e.textContent = t; }
function setStyle(e: HTMLElement, k: string, v: string) { if ((e.style as any)[k] !== v) (e.style as any)[k] = v; }

export const SCENE_KEYS = ['duel', 'city', 'playground', 'sandbox', 'flat'] as const; // PvP focus: the duel first; PvE scenes stay in code for tests/eval only

const OBJECTIVES: Record<string, string> = {
  city: 'Big city duel map — skyscrapers to swing from, rooftops to hop, a highway, sky bridges and an open park in the middle. First to 3 knockouts; your rival starts at the far end of the boulevard.',
  duel: '1v1 duel arena — first to 3 knockouts. Type your attacks: “punch him”, “drop a piano on him”, “grab him and throw him”. Practice vs the bot from the menu.',
  playground: 'Free roam — ring run through the city, cross the canyon (east), climb the tower (west), turret outpost & drone field (south).',
  arena: 'Open arena — try anything.',
  gap: 'Cross the 25 m canyon.',
  tower: 'Reach the rooftop.',
  barrage: 'A turret is firing at you. Protect yourself.',
  falling: 'A civilian is falling — save them.',
  boulder: 'A boulder blocks the door. Get through.',
  drones: 'Three hostile drones. Take them down.',
  flat: 'Empty flat ground.',
  sandbox: 'Sandbox — robots, drones and a turret. Smash the crates, topple the blocks, breach the wall, protect the civilian.',
  horde: 'Horde — the sandbox, with a new wave of robots and drones every 22 s.',
  race: 'Race — 200 m to the finish line. The clock starts at the start line.',
};

export interface HudFrame {
  world: World;
  renderer: GameRenderer;
  fps: number;
  frameMs: number;
  simMs: number;
  consoleOpen: boolean;
  locked: boolean;
  /** The local lantern's id (inspiration is per player). */
  playerId: string;
  /** Focus time: meter 0..1, whether time is currently slowed, whether it is enabled at all. */
  focus: { meter: number; active: boolean; enabled: boolean };
  /** Typing combo (0/1 = none) and seconds before it lapses. */
  combo: { n: number; left: number };
}

/** DOM heads-up display: readable, stylish, failures always visible. */
export class HUD {
  readonly root: HTMLElement;
  readonly crosshair: HTMLElement;
  /** PvP panel: health + score, kill feed, banners, damage numbers, lock-on, opponent arrow */
  readonly duel: DuelHud;
  private hpFill: HTMLElement;
  private speedNum: HTMLElement;
  private speedBar: HTMLElement;
  private gasFill: HTMLElement;
  private willFill: HTMLElement;
  private inspFill: HTMLElement;
  private inspVal: HTMLElement;
  private statusPill: HTMLElement;
  private statusText: HTMLElement;
  private chip: HTMLElement;
  private chipSummary: HTMLElement;
  private chipMeta: HTMLElement;
  readonly consoleWrap: HTMLElement;
  readonly consoleInput: HTMLInputElement;
  private consoleHint: HTMLElement;
  private ghostTyped: HTMLElement;
  private ghostRest: HTMLElement;
  private focusLine: HTMLElement;
  private castStats: HTMLElement;
  private quickBar: HTMLElement;
  private quickKey = '';
  private toasts: HTMLElement;
  private list: HTMLElement;
  private objTitle: HTMLElement;
  private objText: HTMLElement;
  private objRings: HTMLElement;
  private objGoals: HTMLElement;
  private banner: HTMLElement;
  private help: HTMLElement;
  private perf: HTMLElement;
  readonly menu: HTMLElement;
  private markers: HTMLElement;
  private markerEls = new Map<string, HTMLElement>();
  /** Whose HUD this is (set by the app; inspiration events are per player). */
  localPlayer = 'p1';
  private status: RingStatus = 'ready';
  private statusUntil = 0;
  private interpreting = false;
  private ringStart: number | null = null;
  private ringEnd: number | null = null;
  private listKey = '';
  private perfVisible = false;
  private perfAcc = 0;
  private scene = '';
  private hostilesTotal = 0;

  constructor(parent: HTMLElement) {
    this.root = el('div', 'hud', parent);
    this.markers = el('div', 'markers', this.root);
    // crosshair
    this.crosshair = el('div', 'crosshair', this.root);
    for (const k of ['t', 'b', 'l', 'r']) el('i', 'tick ' + k, this.crosshair);
    el('i', 'dot', this.crosshair);
    el('i', 'range', this.crosshair);
    // speed + bars
    const bc = el('div', 'bottom-center', this.root);
    const sp = el('div', 'speed', bc);
    this.speedNum = el('span', 'num', sp, '0');
    el('span', 'unit', sp, 'm/s');
    const sb = el('div', 'speedbar', bc);
    this.speedBar = el('i', '', sb);
    const bars = el('div', 'bars', bc);
    const hp = el('div', 'bar hp', bars);
    el('label', '', hp, 'HEALTH');
    this.hpFill = el('i', '', el('div', 'track', hp));
    const gas = el('div', 'bar gas', bars);
    el('label', '', gas, 'THRUST');
    this.gasFill = el('i', '', el('div', 'track', gas));
    const will = el('div', 'bar will', bars);
    el('label', '', will, 'WILLPOWER');
    this.willFill = el('i', '', el('div', 'track', will));
    const insp = el('div', 'bar insp', bars);
    const il = el('label', '', insp, 'INSPIRATION ');
    this.inspVal = el('span', 'val', il, '×1.0');
    this.inspFill = el('i', '', el('div', 'track', insp));
    // ring panel (bottom-left)
    const ring = el('div', 'ring-panel', this.root);
    this.statusPill = el('div', 'status ready', ring);
    el('i', 'led', this.statusPill);
    this.statusText = el('span', '', this.statusPill, STATUS_TEXT.ready);
    this.chip = el('div', 'chip hidden', ring);
    el('span', 'lbl', this.chip, 'the ring understood');
    this.chipSummary = el('span', 'sum', this.chip);
    this.chipMeta = el('span', 'meta', this.chip);
    this.castStats = el('div', 'cast-stats hidden', ring);
    this.consoleWrap = el('div', 'console hidden', ring);
    el('span', 'prompt', this.consoleWrap, '›');
    const field = el('div', 'field', this.consoleWrap);
    this.consoleInput = el('input', '', field) as HTMLInputElement;
    this.consoleInput.placeholder = 'imagine something… e.g. “a giant ramp”';
    this.consoleInput.spellcheck = false;
    this.consoleInput.autocomplete = 'off';
    const ghost = el('span', 'ghost', field);
    this.ghostTyped = el('span', 'typed', ghost);
    this.ghostRest = el('span', '', ghost);
    this.focusLine = el('i', 'focus-line', this.consoleWrap);
    this.consoleHint = el('div', 'console-hint hidden', ring, 'Enter form · Shift+Enter chain · Tab complete · ↑ history · empty Enter recast · Esc');
    this.quickBar = el('div', 'key-hint quick hidden', ring);
    const hint = el('div', 'key-hint', ring);
    hint.innerHTML = '<b>Enter</b> imagine · <b>1–9</b> quick-cast · <b>X</b> dismiss · <b>F1</b> help';
    // toasts (top-center)
    this.toasts = el('div', 'toasts', this.root);
    // construct list (top-left)
    this.list = el('div', 'clist', this.root);
    // objective (top-right)
    const obj = el('div', 'objective', this.root);
    this.objTitle = el('div', 'title', obj);
    this.objText = el('div', 'text', obj);
    this.objRings = el('div', 'rings hidden', obj);
    this.objGoals = el('div', 'goals', obj);
    this.banner = el('div', 'banner hidden', this.root);
    this.help = el('div', 'overlay help hidden', this.root);
    this.help.innerHTML = HELP_HTML;
    this.perf = el('pre', 'perf hidden', this.root);
    this.menu = el('div', 'overlay menu hidden', this.root);
    this.duel = new DuelHud(this.root, this, () => this.localPlayer);
    this.menu.remove();
    this.root.appendChild(this.menu); // the menu stays on top of the duel layers
  }

  // ------------------------------------------------------------------------------------ state changes

  reset(w: World, scene: string) {
    this.scene = scene;
    this.ringStart = this.ringEnd = null;
    setText(this.objTitle, scene.toUpperCase());
    setText(this.objText, OBJECTIVES[scene] ?? '');
    this.objRings.classList.toggle('hidden', w.rings.length === 0);
    this.hostilesTotal = [...w.entities.values()].filter((e) => e.hostile).length;
    for (const m of this.markerEls.values()) m.remove();
    this.markerEls.clear();
    this.listKey = '#reset';
    this.renderGoals(w);
    this.chip.classList.add('hidden');
    this.toasts.innerHTML = '';
    this.duel.reset(w);
  }

  setInterpreting(on: boolean) { this.interpreting = on; }

  flashStatus(s: RingStatus, ms = 3200) { this.status = s; this.statusUntil = performance.now() + ms; }

  showResult(r: CommitResult) {
    const abstain = !!r.abstain || r.ops.some((o) => o.op === 'abstain');
    const status: RingStatus = abstain ? 'not_understood' : !r.applied ? 'rejected' : r.clamps.length ? 'clamped' : 'formed';
    this.flashStatus(status, abstain || !r.applied ? 4500 : 3200);
    this.chip.classList.remove('hidden');
    this.chip.classList.toggle('bad', abstain || !r.applied);
    setText(this.chipSummary, abstain ? `?? ${r.abstain ?? 'not understood'}` : r.summary || '—');
    setText(this.chipMeta, `${r.source} · ${Math.round((r.confidence ?? 0) * 100)}% · ${Math.round(r.latencyMs)} ms${r.escalated ? ' · escalated' : ''}`);
    if (abstain) this.toast('bad', `The ring didn’t understand: ${r.abstain ?? r.text}`);
    else if (!r.applied) this.toast('bad', `Nothing formed${r.messages.length ? '' : ' — the request was refused'}`);
    for (const c of r.clamps) this.toast('warn', `Clamped · ${c}`);
    for (const a of r.approximations) this.toast('info', `Approximated · ${a}`);
    for (const m of r.messages) if (!r.clamps.includes(m) && !r.approximations.includes(m)) this.toast('note', m);
  }

  toast(kind: 'good' | 'warn' | 'bad' | 'info' | 'note', text: string, ms = 5200) {
    const t = el('div', 'toast ' + kind, this.toasts, text);
    while (this.toasts.children.length > 5) this.toasts.firstElementChild?.remove();
    setTimeout(() => t.classList.add('out'), ms);
    setTimeout(() => t.remove(), ms + 400);
  }

  hideBanner() { clearTimeout((this.banner as any)._t); this.banner.classList.add('out'); }

  showBanner(title: string, sub = '', ms = 2600) {
    this.banner.innerHTML = '';
    el('div', 'big', this.banner, title);
    if (sub) el('div', 'small', this.banner, sub);
    this.banner.classList.remove('hidden', 'out');
    clearTimeout((this.banner as any)._t);
    (this.banner as any)._t = setTimeout(() => this.banner.classList.add('out'), ms);
  }

  setConsole(open: boolean) {
    this.consoleWrap.classList.toggle('hidden', !open);
    this.consoleHint.classList.toggle('hidden', !open);
    if (!open) this.setGhost('');
  }

  /** Faint inline completion after what's typed (hidden if the text has scrolled inside the field). */
  setGhost(suffix: string) {
    const inp = this.consoleInput;
    const show = !!suffix && inp.scrollWidth <= inp.clientWidth + 1;
    setText(this.ghostTyped, show ? inp.value : '');
    setText(this.ghostRest, show ? suffix : '');
  }

  /** "cast 1.4 s · 9.2 cps · best 0.9 s" after a typed cast (or the quick-cast note). */
  showCast(t: CastTiming | null, bests: CastBests, flags: { bestCast?: boolean; bestCps?: boolean; quick?: string } = {}) {
    const parts: string[] = [];
    if (t) parts.push(`cast ${t.sec.toFixed(1)} s${flags.bestCast ? ' ★' : ''}`, `${t.cps.toFixed(1)} cps${flags.bestCps ? ' ★' : ''}`);
    else if (flags.quick) parts.push(flags.quick);
    if (bests.cast !== null) parts.push(`best ${bests.cast.toFixed(1)} s`);
    if (!parts.length) return;
    this.castStats.classList.remove('hidden');
    this.castStats.classList.toggle('best', !!(flags.bestCast || flags.bestCps));
    this.castStats.dataset.base = parts.join(' · ');
    setText(this.castStats, this.castStats.dataset.base);
  }

  /** The 1–9 quick-cast bar (re-rendered only when the slots change). */
  setQuickSlots(slots: Slot[]) {
    const key = slots.map((s) => s.key + s.text).join('|');
    if (key === this.quickKey) return;
    this.quickKey = key;
    this.quickBar.innerHTML = '';
    this.quickBar.classList.toggle('hidden', !slots.length);
    for (const s of slots) {
      const i = el('span', 'slot' + (s.saved ? ' saved' : ''), this.quickBar);
      el('b', '', i, String(s.key));
      el('span', 'txt', i, s.text);
    }
  }

  toggleHelp(force?: boolean) { this.help.classList.toggle('hidden', force === undefined ? undefined : !force); }
  togglePerf() { this.perfVisible = !this.perfVisible; this.perf.classList.toggle('hidden', !this.perfVisible); }
  get helpOpen() { return !this.help.classList.contains('hidden'); }

  onEvent(w: World, e: { type: string; [k: string]: any }) {
    this.duel.onEvent(w, e as any);
    switch (e.type) {
      case 'ring_passed': {
        const done = w.rings.filter((r) => r.passed).length;
        if (this.ringStart === null) this.ringStart = w.time;
        if (done === w.rings.length && this.ringEnd === null) {
          this.ringEnd = w.time;
          this.showBanner('RING RUN COMPLETE', `${(this.ringEnd - this.ringStart).toFixed(2)} s`, 4000);
        }
        break;
      }
      case 'goal_reached': this.showBanner('GOAL REACHED', e.label); this.toast('good', `Goal · ${e.label}`); this.renderGoals(w); break;
      case 'construct_rejected': if (!e.owner || e.owner === this.localPlayer) { this.toast('bad', `Rejected · ${e.label}: ${e.reason}`); this.flashStatus('rejected'); } break;
      case 'construct_dismissed':
        if ((!e.owner || e.owner === this.localPlayer) && e.reason === 'budget') this.toast('note', `“${e.label}” dissolved — construct budget`);
        else if (e.owner && e.owner !== this.localPlayer && e.reason === 'destroyed') this.toast('good', `Broke their ${e.label}`, 2600);
        break;
      case 'civilian_safe': this.toast('good', 'Civilian safe'); this.showBanner('SAVED', 'the civilian is safe'); break;
      case 'civilian_hurt': this.toast('bad', 'The civilian was hurt'); break;
      case 'inspiration': {
        if (e.player && e.player !== this.localPlayer) break; // the rival's casts are theirs to read about
        const mul = `×${Number(e.mul).toFixed(1)}`;
        if (e.stale) this.toast('warn', `Stale ${mul} — ${e.reasons?.[0] ?? 'you’ve done this already'}`, 4200);
        else if (e.novelty >= INSPIRED_AT) this.toast('good', `✦ Inspired ${mul}${e.fresh?.length ? ` — new: ${e.fresh.join(', ')}` : ''}`, 4200);
        break;
      }
      case 'creative_solution':
        if (!e.player || e.player === this.localPlayer) this.toast('good', `✦ Creative solution — ${e.reason} · +${Math.round(e.bonus * 100)}% inspiration`, 5200);
        else if (e.against === this.localPlayer) this.toast('warn', `Rival’s creative solution — ${e.reason}`, 4200);
        break;
      case 'entity_disabled': {
        const left = [...w.entities.values()].filter((x) => x.hostile && !x.disabled).length;
        if (e.entityType === 'drone' || e.entityType === 'turret') this.toast('good', `${e.entityType} disabled · ${this.hostilesTotal - left}/${this.hostilesTotal}`);
        if (left === 0 && this.hostilesTotal > 0) this.showBanner('AREA CLEAR', 'all hostiles disabled');
        break;
      }
    }
  }

  private renderGoals(w: World) {
    this.objGoals.innerHTML = '';
    for (const g of w.goals) el('div', 'goal' + (g.reached ? ' done' : ''), this.objGoals, `${g.reached ? '✓' : '○'} ${g.label}`);
  }

  // ------------------------------------------------------------------------------------ per frame

  update(f: HudFrame) {
    const w = f.world;
    const P = w.player;
    const now = performance.now();
    // speed
    const sp = P.vel.length();
    setText(this.speedNum, sp.toFixed(0));
    setStyle(this.speedBar, 'width', `${Math.min(100, (sp / 60) * 100).toFixed(1)}%`);
    this.speedNum.parentElement!.classList.toggle('fast', sp > 30);
    setStyle(this.gasFill, 'width', `${((P.gas / P.tune.gasMax) * 100).toFixed(1)}%`);
    setStyle(this.hpFill, 'width', `${Math.max(0, Math.min(100, (P.hp / P.maxHp) * 100)).toFixed(1)}%`);
    this.hpFill.parentElement!.parentElement!.classList.toggle('low', P.hp < 30);
    setStyle(this.willFill, 'width', `${Math.max(0, Math.min(100, (w.constructs.energy / w.constructs.energyMax) * 100)).toFixed(1)}%`);
    const insp = w.inspiration.of(f.playerId);
    const inspBar = this.inspFill.parentElement!.parentElement!;
    setStyle(this.inspFill, 'width', `${(Math.max(0, Math.min(1, insp.level)) * 100).toFixed(1)}%`);
    setText(this.inspVal, `×${insp.mul.toFixed(1)}`);
    inspBar.classList.toggle('high', insp.level >= 0.75);
    inspBar.classList.toggle('low', insp.level < 0.2);
    // console focus meter + combo
    setStyle(this.focusLine, 'width', `${(f.focus.enabled ? f.focus.meter * 100 : 0).toFixed(1)}%`);
    this.focusLine.classList.toggle('active', f.focus.active);
    if (this.castStats.dataset.base) {
      const live = f.combo.n > 1 && f.combo.left > 0;
      setText(this.castStats, live ? `${this.castStats.dataset.base} · combo ×${f.combo.n}` : this.castStats.dataset.base);
      this.castStats.classList.toggle('combo', live);
    }
    this.gasFill.parentElement!.parentElement!.classList.toggle('low', P.gas < 20);
    // crosshair state
    const aim = P.aim(P.tune.tetherRange * 1.5);
    const own = w.ownerOf(aim.collider);
    const ent = own?.kind === 'entity' ? w.entities.get(own.id) : null;
    const rival = own?.kind === 'player' && own.id !== P.id;
    const dist = aim.point.distanceTo(P.pos);
    this.crosshair.classList.toggle('hostile', !!ent && ent.hostile && !ent.disabled || rival);
    this.crosshair.classList.toggle('entity', !!ent);
    this.crosshair.classList.toggle('in-range', !!aim.collider && dist <= P.tune.tetherRange);
    // ring status
    let s: RingStatus = 'ready';
    if (this.interpreting) s = 'interpreting';
    else if (f.consoleOpen) s = f.focus.active ? 'focus' : 'typing';
    else if (now < this.statusUntil) s = this.status;
    if (this.statusPill.dataset.s !== s) {
      this.statusPill.dataset.s = s;
      this.statusPill.className = 'status ' + s;
      setText(this.statusText, STATUS_TEXT[s]);
    }
    // construct list
    const focusId = w.constructs.focusOf(this.localPlayer);
    const cs = [...w.constructs.list.values()].filter((c) => c.owner === this.localPlayer);
    const key = cs.map((c) => c.id + c.version + (c.id === focusId ? '*' : '')).join('|');
    if (key !== this.listKey) {
      this.listKey = key;
      this.list.innerHTML = '';
      if (cs.length) el('div', 'hdr', this.list, `CONSTRUCTS · ${cs.length}`);
      for (const c of cs.slice(-8)) {
        const row = el('div', 'row' + (c.id === focusId ? ' focus' : '') + (c.clamps.length ? ' clamped' : ''), this.list);
        el('span', 'name', row, (c.name ? `${c.name}: ` : '') + c.spec.label + (c.insts.length > 1 ? ` ×${c.insts.length}` : ''));
        const life = el('i', 'life', row);
        life.dataset.id = c.id;
      }
    }
    for (const life of this.list.querySelectorAll<HTMLElement>('i.life')) {
      const c = w.constructs.list.get(life.dataset.id!);
      if (c) setStyle(life, 'width', `${Math.max(0, Math.min(1, (c.expires - w.time) / Math.max(1, c.expires - c.born))) * 100}%`);
    }
    // rings
    if (w.rings.length) {
      const done = w.rings.filter((r) => r.passed).length;
      const t = this.ringStart === null ? 0 : (this.ringEnd ?? w.time) - this.ringStart;
      setText(this.objRings, `RING RUN  ${done}/${w.rings.length}  ·  ${t.toFixed(1)} s`);
    }
    this.updateMarkers(f);
    this.duel.update(w, f.renderer);
    // perf
    if (this.perfVisible) {
      this.perfAcc += 1;
      if (this.perfAcc % 10 === 0) {
        const st = f.renderer.stats;
        const mem = (performance as any).memory?.usedJSHeapSize;
        this.perf.textContent = [
          `fps        ${f.fps.toFixed(0)}`,
          `frame      ${f.frameMs.toFixed(2)} ms`,
          `sim/tick   ${f.simMs.toFixed(2)} ms`,
          `render cpu ${st.frameCpuMs.toFixed(2)} ms (update ${st.updateMs.toFixed(2)})`,
          `draw calls ${st.sceneCalls} scene / ${st.totalCalls} total`,
          `triangles  ${st.sceneTriangles.toLocaleString()}`,
          `constructs ${st.constructs}  entities ${st.entities}`,
          `projectile ${st.projectiles}  particles ${st.particles}  beams ${st.beams}`,
          mem ? `js heap    ${(mem / 1048576).toFixed(0)} MB` : '',
        ].filter(Boolean).join('\n');
      }
    }
  }

  private updateMarkers(f: HudFrame) {
    const w = f.world;
    const want = new Map<string, { p: Vector3; label: string; cls: string }>();
    const next = w.rings.find((r) => !r.passed);
    if (next) want.set('ring', { p: next.pos, label: `ring ${w.rings.indexOf(next) + 1}`, cls: 'ring' });
    for (const g of w.goals) if (!g.reached) want.set(g.id, { p: g.min.clone().add(g.max).multiplyScalar(0.5), label: g.label, cls: 'goal' });
    for (const [k, m] of this.markerEls) if (!want.has(k)) { m.remove(); this.markerEls.delete(k); }
    const W = f.renderer.canvas.clientWidth, H = f.renderer.canvas.clientHeight;
    for (const [k, m] of want) {
      let e = this.markerEls.get(k);
      if (!e) { e = el('div', 'marker ' + m.cls, this.markers); el('i', '', e); el('span', '', e); this.markerEls.set(k, e); }
      const pr = f.renderer.project(m.p);
      let x = pr.x, y = pr.y;
      const M = 40;
      let off = pr.behind || x < M || x > W - M || y < M || y > H - M;
      if (off) {
        // clamp onto an inset screen ellipse in the direction of the target
        const cs = f.renderer.toCameraSpace(m.p);
        let ang = Math.atan2(-cs.y, cs.x);
        if (!pr.behind && cs.z < 0) ang = Math.atan2(y - H / 2, x - W / 2);
        x = W / 2 + Math.cos(ang) * (W / 2 - M);
        y = H / 2 + Math.sin(ang) * (H / 2 - M);
        y = Math.min(y, H - 150); // keep clear of the bottom HUD
      }
      x = Math.max(M + 70, Math.min(W - M - 70, x));
      e.classList.toggle('edge', off);
      setStyle(e, 'transform', `translate(${x.toFixed(0)}px, ${y.toFixed(0)}px)`);
      const d = m.p.distanceTo(w.player.pos);
      setText(e.lastElementChild as HTMLElement, `${m.label} · ${d.toFixed(0)} m`);
    }
  }
}

const HELP_HTML = `
<h2>BRIGHTEST DAY <small>controls</small></h2>
<div class="cols">
<div>
<h3>Move</h3>
<p><b>WASD</b> move · <b>Space</b> jump / up · <b>Ctrl</b> or <b>C</b> slide / down</p>
<p><b>Shift</b> boost thruster · <b>F</b> toggle flight</p>
<p><b>Q</b> left tether · <b>E</b> right tether (hold) · <b>R</b> or <b>MMB</b> reel in</p>
<p><b>LMB</b> / <b>RMB</b> use what you made — the prompt by the crosshair says how (hold <b>RMB</b> to charge / aim) · <b>wheel</b> switch construct</p>
<p class="tip">Bunny-hop: jump the instant you land and strafe (A/D) while turning the mouse to build speed.</p>
</div>
<div>
<h3>Imagine</h3>
<p><b>Enter</b> open the ring console — you keep your momentum, tethers and aim; only movement keys pause</p>
<p><b>Enter</b> form it · <b>Shift</b>+<b>Enter</b> form and keep typing · empty <b>Enter</b> recast the last one</p>
<p><b>Tab</b> complete a word · <b>↑</b> history · <b>Esc</b> cancel · <b>1</b>–<b>9</b> quick-cast</p>
<p><b>X</b> dismiss focused · <b>Z</b> dismiss all</p>
<p class="tip">New ideas and new combinations inspire the ring (stronger, cheaper); repeating yourself goes stale. Try “a tank with wings”, “drop a piano on that drone”.</p>
</div>
<div>
<h3>World</h3>
<p><b>Alt</b>+<b>1</b> playground · <b>2</b> arena · <b>3</b> gap · <b>4</b> tower · <b>5</b> barrage · <b>6</b> falling · <b>7</b> boulder · <b>8</b> drones · <b>9</b> flat</p>
<p><b>Backspace</b>/<b>F5</b> reset scene · <b>F1</b> help · <b>F3</b> perf · <b>F11</b> fullscreen</p>
</div>
</div>
<p class="close">press F1 to close</p>`;
