import type { CommitResult } from '../lang/types.js';
import type { World } from '../sim/world.js';
import type { GameRenderer } from '../render/renderer.js';
import { Vector3 } from 'three';

export type RingStatus = 'ready' | 'listening' | 'interpreting' | 'formed' | 'clamped' | 'not_understood' | 'rejected';

const STATUS_TEXT: Record<RingStatus, string> = {
  ready: 'ring ready', listening: 'listening…', interpreting: 'interpreting…', formed: 'formed', clamped: 'formed · clamped', not_understood: 'not understood', rejected: 'rejected',
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

export const SCENE_KEYS = ['playground', 'arena', 'gap', 'tower', 'barrage', 'falling', 'boulder', 'drones', 'flat'] as const;

const OBJECTIVES: Record<string, string> = {
  playground: 'Free roam — ring run through the city, cross the canyon (east), climb the tower (west), turret outpost & drone field (south).',
  arena: 'Open arena — try anything.',
  gap: 'Cross the 25 m canyon.',
  tower: 'Reach the rooftop.',
  barrage: 'A turret is firing at you. Protect yourself.',
  falling: 'A civilian is falling — save them.',
  boulder: 'A boulder blocks the door. Get through.',
  drones: 'Three hostile drones. Take them down.',
  flat: 'Empty flat ground.',
};

export interface HudFrame {
  world: World;
  renderer: GameRenderer;
  fps: number;
  frameMs: number;
  simMs: number;
  consoleOpen: boolean;
  locked: boolean;
}

/** DOM heads-up display: readable, stylish, failures always visible. */
export class HUD {
  readonly root: HTMLElement;
  private crosshair: HTMLElement;
  private speedNum: HTMLElement;
  private speedBar: HTMLElement;
  private gasFill: HTMLElement;
  private willFill: HTMLElement;
  private statusPill: HTMLElement;
  private statusText: HTMLElement;
  private chip: HTMLElement;
  private chipSummary: HTMLElement;
  private chipMeta: HTMLElement;
  readonly consoleWrap: HTMLElement;
  readonly consoleInput: HTMLInputElement;
  private consoleHint: HTMLElement;
  private transcript: HTMLElement;
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
  private status: RingStatus = 'ready';
  private statusUntil = 0;
  private listening = false;
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
    const gas = el('div', 'bar gas', bars);
    el('label', '', gas, 'THRUST');
    this.gasFill = el('i', '', el('div', 'track', gas));
    const will = el('div', 'bar will', bars);
    el('label', '', will, 'WILLPOWER');
    this.willFill = el('i', '', el('div', 'track', will));
    // ring panel (bottom-left)
    const ring = el('div', 'ring-panel', this.root);
    this.statusPill = el('div', 'status ready', ring);
    el('i', 'led', this.statusPill);
    this.statusText = el('span', '', this.statusPill, STATUS_TEXT.ready);
    this.chip = el('div', 'chip hidden', ring);
    el('span', 'lbl', this.chip, 'the ring understood');
    this.chipSummary = el('span', 'sum', this.chip);
    this.chipMeta = el('span', 'meta', this.chip);
    this.transcript = el('div', 'transcript hidden', ring);
    this.consoleWrap = el('div', 'console hidden', ring);
    el('span', 'prompt', this.consoleWrap, '›');
    this.consoleInput = el('input', '', this.consoleWrap) as HTMLInputElement;
    this.consoleInput.placeholder = 'imagine something… e.g. “a giant ramp”';
    this.consoleInput.spellcheck = false;
    this.consoleInput.autocomplete = 'off';
    this.consoleHint = el('div', 'console-hint hidden', ring, 'Enter to form · Esc to cancel · ↑ history');
    const hint = el('div', 'key-hint', ring);
    hint.innerHTML = '<b>Enter</b> imagine · <b>V</b> speak · <b>X</b> dismiss · <b>F1</b> help';
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
    this.listKey = '';
    this.renderGoals(w);
  }

  setListening(on: boolean) { this.listening = on; }
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

  showBanner(title: string, sub = '', ms = 2600) {
    this.banner.innerHTML = '';
    el('div', 'big', this.banner, title);
    if (sub) el('div', 'small', this.banner, sub);
    this.banner.classList.remove('hidden', 'out');
    clearTimeout((this.banner as any)._t);
    (this.banner as any)._t = setTimeout(() => this.banner.classList.add('out'), ms);
  }

  setTranscript(text: string, final: boolean) {
    this.transcript.classList.toggle('hidden', !text);
    this.transcript.classList.toggle('final', final);
    setText(this.transcript, text ? `“${text}”` : '');
  }

  setConsole(open: boolean) {
    this.consoleWrap.classList.toggle('hidden', !open);
    this.consoleHint.classList.toggle('hidden', !open);
  }

  toggleHelp(force?: boolean) { this.help.classList.toggle('hidden', force === undefined ? undefined : !force); }
  togglePerf() { this.perfVisible = !this.perfVisible; this.perf.classList.toggle('hidden', !this.perfVisible); }
  get helpOpen() { return !this.help.classList.contains('hidden'); }

  onEvent(w: World, e: { type: string; [k: string]: any }) {
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
      case 'construct_rejected': this.toast('bad', `Rejected · ${e.label}: ${e.reason}`); this.flashStatus('rejected'); break;
      case 'construct_dismissed': if (e.reason === 'budget') this.toast('note', `“${e.label}” dissolved — construct budget`); break;
      case 'civilian_safe': this.toast('good', 'Civilian safe'); this.showBanner('SAVED', 'the civilian is safe'); break;
      case 'civilian_hurt': this.toast('bad', 'The civilian was hurt'); break;
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
    setStyle(this.willFill, 'width', `${Math.max(0, Math.min(100, w.constructs.energy)).toFixed(1)}%`);
    this.gasFill.parentElement!.parentElement!.classList.toggle('low', P.gas < 20);
    // crosshair state
    const aim = P.aim(P.tune.tetherRange * 1.5);
    const own = w.ownerOf(aim.collider);
    const ent = own?.kind === 'entity' ? w.entities.get(own.id) : null;
    const dist = aim.point.distanceTo(P.pos);
    this.crosshair.classList.toggle('hostile', !!ent && ent.hostile && !ent.disabled);
    this.crosshair.classList.toggle('entity', !!ent);
    this.crosshair.classList.toggle('in-range', !!aim.collider && dist <= P.tune.tetherRange);
    // ring status
    let s: RingStatus = 'ready';
    if (this.interpreting) s = 'interpreting';
    else if (this.listening || f.consoleOpen) s = 'listening';
    else if (now < this.statusUntil) s = this.status;
    if (this.statusPill.dataset.s !== s) {
      this.statusPill.dataset.s = s;
      this.statusPill.className = 'status ' + s;
      setText(this.statusText, STATUS_TEXT[s]);
    }
    // construct list
    const cs = [...w.constructs.list.values()];
    const key = cs.map((c) => c.id + c.version + (c.id === w.constructs.focusId ? '*' : '')).join('|');
    if (key !== this.listKey) {
      this.listKey = key;
      this.list.innerHTML = '';
      if (cs.length) el('div', 'hdr', this.list, `CONSTRUCTS · ${cs.length}`);
      for (const c of cs.slice(-8)) {
        const row = el('div', 'row' + (c.id === w.constructs.focusId ? ' focus' : '') + (c.clamps.length ? ' clamped' : ''), this.list);
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
      const off = pr.behind || x < 30 || x > W - 30 || y < 30 || y > H - 30;
      if (pr.behind) { x = W - x; y = H - 40; }
      x = Math.max(30, Math.min(W - 30, x));
      y = Math.max(30, Math.min(H - 30, y));
      e.classList.toggle('edge', off);
      setStyle(e, 'transform', `translate(${x.toFixed(0)}px, ${y.toFixed(0)}px)`);
      const d = m.p.distanceTo(w.player.pos);
      setText(e.lastElementChild as HTMLElement, `${m.label} · ${d.toFixed(0)} m`);
    }
  }
}

const HELP_HTML = `
<h2>HARDLIGHT <small>controls</small></h2>
<div class="cols">
<div>
<h3>Move</h3>
<p><b>WASD</b> move · <b>Space</b> jump / up · <b>Ctrl</b> or <b>C</b> slide / down</p>
<p><b>Shift</b> boost thruster · <b>F</b> toggle flight</p>
<p><b>Q</b>/<b>LMB</b> left tether · <b>E</b>/<b>RMB</b> right tether (hold)</p>
<p><b>R</b>, <b>wheel</b> or <b>MMB</b> reel in</p>
<p class="tip">Bunny-hop: jump the instant you land and strafe (A/D) while turning the mouse to build speed.</p>
</div>
<div>
<h3>Imagine</h3>
<p><b>Enter</b> open the ring console — type an idea, see it form as a ghost, <b>Enter</b> to make it real</p>
<p><b>Esc</b> cancel · <b>↑</b> recall history</p>
<p><b>V</b> (hold) speak · <b>X</b> dismiss focused · <b>Z</b> dismiss all</p>
<p class="tip">Try: “a giant ramp”, “a wall between me and the turret”, “make a boxing glove and punch that drone”.</p>
</div>
<div>
<h3>World</h3>
<p><b>1</b> playground · <b>2</b> arena · <b>3</b> gap · <b>4</b> tower · <b>5</b> barrage · <b>6</b> falling · <b>7</b> boulder · <b>8</b> drones · <b>9</b> flat</p>
<p><b>Backspace</b>/<b>F5</b> reset scene · <b>F1</b> help · <b>F3</b> perf · <b>F11</b> fullscreen</p>
</div>
</div>
<p class="close">press F1 to close</p>`;
