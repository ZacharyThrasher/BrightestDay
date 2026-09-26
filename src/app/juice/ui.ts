/**
 * JUICE HUD layer (DOM): damage numbers that pop / arc / fade with weight, crosshair hit & kill markers,
 * centre callouts ("INSPIRED ×1.6", "CREATIVE KILL"), the combo counter and the F4 debug overlay.
 * Same visual language as the HUD (heavy italic, var(--hl) glow); styles are injected once here so the
 * shared stylesheet stays untouched. Only live elements are written each frame.
 */
import { JUICE } from './tuning.js';
import type { CalloutTone } from './core.js';

const CSS = `
.jz { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
.jz-num { position: absolute; left: 0; top: 0; font-size: 22px; font-weight: 900; font-style: italic; color: #fff; white-space: nowrap; will-change: transform, opacity;
  text-shadow: 0 0 10px var(--hl), 0 2px 2px rgba(0,0,0,0.55); letter-spacing: 0.02em; }
.jz-num.big { color: #fff6d0; text-shadow: 0 0 16px #ffd35c, 0 0 4px #fff, 0 2px 2px rgba(0,0,0,0.5); }
.jz-num.hidden, .jz-mark.hidden, .jz-call.hidden, .jz-combo.hidden { display: none; }
.jz-mark { position: absolute; left: 50%; top: 50%; width: 0; height: 0; will-change: transform, opacity; }
.jz-mark i { position: absolute; width: 3px; height: 11px; left: -1.5px; top: -5.5px; background: #fff; border-radius: 1px; box-shadow: 0 0 6px var(--hl), 0 0 0 1px rgba(10,14,30,0.5); }
.jz-mark.kill i { background: var(--bad, #ff5d73); height: 15px; top: -7.5px; box-shadow: 0 0 10px var(--bad, #ff5d73), 0 0 0 1px rgba(30,6,12,0.6); }
.jz-mark.pulse i { display: none; }
.jz-mark b { display: none; position: absolute; width: 26px; height: 26px; left: -13px; top: -13px; border-radius: 50%; border: 2px solid var(--hl); box-shadow: 0 0 10px var(--hl); }
.jz-mark.pulse b { display: block; }
.jz-call { position: absolute; left: 50%; top: 62%; text-align: center; will-change: transform, opacity; white-space: nowrap; }
.jz-call .t { font-size: 34px; font-weight: 900; font-style: italic; letter-spacing: 0.08em; color: #fff; text-shadow: 0 0 18px var(--hl), 0 2px 3px rgba(0,0,0,0.45); }
.jz-call .s { margin-top: 2px; font-size: 13px; font-weight: 700; letter-spacing: 0.14em; color: #eafff4; text-transform: uppercase; opacity: 0.9; }
.jz-call.gold .t { color: #fff6d0; text-shadow: 0 0 22px #ffd35c, 0 0 6px #ffb04d, 0 2px 3px rgba(0,0,0,0.45); }
.jz-call.dim .t { color: #c9cfdc; text-shadow: 0 1px 2px rgba(0,0,0,0.5); letter-spacing: 0.14em; }
.jz-call.bad .t { color: #fff; text-shadow: 0 0 20px var(--bad, #ff5d73), 0 2px 3px rgba(0,0,0,0.5); }
.jz-combo { position: absolute; left: calc(50% + 46px); top: calc(50% - 30px); font-weight: 900; font-style: italic; color: #fff; will-change: transform, opacity; white-space: nowrap;
  text-shadow: 0 0 12px var(--hl), 0 2px 2px rgba(0,0,0,0.5); transform-origin: 0 50%; }
.jz-combo small { font-size: 0.5em; letter-spacing: 0.2em; margin-right: 4px; opacity: 0.85; }
.jz-debug { position: absolute; right: 12px; top: 190px; margin: 0; padding: 8px 10px; border-radius: 8px; background: rgba(8,12,28,0.72); border: 1px solid rgba(255,255,255,0.14);
  font: 11px/1.45 var(--mono); color: #d8e6ff; white-space: pre; }
`;

const easeOutBack = (x: number) => { const c1 = 1.70158, c3 = c1 + 1; return 1 + c3 * Math.pow(x - 1, 3) + c1 * Math.pow(x - 1, 2); };
const easeOutCubic = (x: number) => 1 - Math.pow(1 - x, 3);

interface Num { el: HTMLElement; key: string; amount: number; t: number; x: number; y: number; drift: number; last: number; live: boolean; size: number }

export type Projector = (key: string) => { x: number; y: number; behind: boolean } | null;

export class JuiceUI {
  readonly root: HTMLElement;
  private nums: Num[] = [];
  private mark: HTMLElement;
  private markT = -1;
  private markDur = 0;
  private markKind: 'hit' | 'kill' | 'pulse' = 'hit';
  private call: HTMLElement;
  private callT = -1;
  private callText: HTMLElement;
  private callSub: HTMLElement;
  private callSize = 1;
  private combo: HTMLElement;
  private comboT = -1;
  private comboN = 0;
  readonly debug: HTMLElement;
  debugOn = false;
  private time = 0;
  /** stats for tests / overlay */
  shown = { numbers: 0, markers: 0, kills: 0, callouts: 0, combos: 0 };
  lastCallout = '';

  constructor(parent: HTMLElement) {
    if (!document.getElementById('jz-style')) {
      const st = document.createElement('style');
      st.id = 'jz-style';
      st.textContent = CSS;
      document.head.appendChild(st);
    }
    this.root = document.createElement('div');
    this.root.className = 'jz';
    parent.appendChild(this.root);
    this.mark = this.div('jz-mark hidden');
    for (const r of [45, 135, 225, 315]) { const i = document.createElement('i'); i.style.transform = `rotate(${r}deg) translateY(-13px)`; this.mark.appendChild(i); }
    this.mark.appendChild(document.createElement('b'));
    this.call = this.div('jz-call hidden');
    this.callText = document.createElement('div'); this.callText.className = 't'; this.call.appendChild(this.callText);
    this.callSub = document.createElement('div'); this.callSub.className = 's'; this.call.appendChild(this.callSub);
    this.combo = this.div('jz-combo hidden');
    this.debug = document.createElement('pre');
    this.debug.className = 'jz-debug hidden';
    this.root.appendChild(this.debug);
  }

  private div(cls: string) { const d = document.createElement('div'); d.className = cls; this.root.appendChild(d); return d; }

  /** A damage number over `key`; rapid hits on the same target merge into one number that re-pops. */
  damage(key: string, amount: number) {
    if (!(amount > 0)) return;
    const D = JUICE.ui.damageNumber;
    const now = this.time;
    const merge = this.nums.find((n) => n.live && n.key === key && (now - n.last) * 1000 < D.mergeMs + n.t * 1000 * 0.5 && n.t < D.life * 0.5);
    this.shown.numbers++;
    if (merge) { merge.amount += amount; merge.t = 0; merge.last = now; merge.size = sizeFor(merge.amount); merge.el.textContent = String(Math.round(merge.amount)); merge.el.classList.toggle('big', merge.amount >= D.big); return; }
    let n = this.nums.find((x) => !x.live);
    if (!n) {
      if (this.nums.length >= D.max) n = this.nums.reduce((a, b) => (a.t > b.t ? a : b));
      else { n = { el: this.div('jz-num hidden'), key, amount: 0, t: 0, x: 0, y: 0, drift: 0, last: 0, live: false, size: 1 }; this.nums.push(n); }
    }
    n.key = key; n.amount = amount; n.t = 0; n.last = now; n.live = true; n.drift = (Math.random() * 2 - 1) * 26; n.size = sizeFor(amount);
    n.el.textContent = String(Math.round(amount));
    n.el.classList.toggle('big', amount >= D.big);
    n.el.classList.remove('hidden');
  }

  marker(kind: 'hit' | 'kill' | 'pulse') {
    if (this.markT >= 0 && this.markKind === 'kill' && kind !== 'kill' && this.markT < this.markDur) return; // a kill marker isn't overwritten by a hit
    this.markKind = kind;
    this.markT = 0;
    this.markDur = (kind === 'kill' ? JUICE.ui.hitMarker.killMs : JUICE.ui.hitMarker.ms) / 1000;
    this.mark.className = `jz-mark ${kind}`;
    if (kind === 'kill') this.shown.kills++; else if (kind === 'hit') this.shown.markers++;
  }

  callout(text: string, tone: CalloutTone, size = 1, sub = '') {
    this.callT = 0;
    this.callSize = size;
    this.callText.textContent = text;
    this.callSub.textContent = sub;
    this.callSub.style.display = sub ? '' : 'none';
    this.call.className = `jz-call ${tone}`;
    this.shown.callouts++;
    this.lastCallout = text;
  }

  setCombo(n: number) {
    if (n < 2) return;
    this.comboN = n;
    this.comboT = 0;
    this.combo.innerHTML = `<small>COMBO</small>×${n}`;
    this.combo.classList.remove('hidden');
    this.shown.combos++;
  }

  /** Per frame: animate everything live. `project(key)` → screen px of that target (or null if gone). */
  frame(dt: number, project: Projector) {
    this.time += dt;
    const D = JUICE.ui.damageNumber;
    for (const n of this.nums) {
      if (!n.live) continue;
      n.t += dt;
      if (n.t >= D.life) { n.live = false; n.el.classList.add('hidden'); continue; }
      const p = project(n.key);
      if (p) { n.x = p.x; n.y = p.y; }
      if (p?.behind) { n.el.style.opacity = '0'; continue; }
      const k = n.t / D.life;
      const pop = n.t * 1000 < D.popMs ? D.popScale + (1 - D.popScale) * easeOutBack(n.t * 1000 / D.popMs) : 1;
      const rise = D.rise * easeOutCubic(Math.min(1, k * 1.6)) - 30 * k * k; // up fast, then a little gravity
      const x = n.x + n.drift * easeOutCubic(k);
      const y = n.y - 20 - rise;
      n.el.style.transform = `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px) translate(-50%, -50%) scale(${(pop * n.size).toFixed(3)})`;
      n.el.style.opacity = (k < 0.65 ? 1 : 1 - (k - 0.65) / 0.35).toFixed(3);
    }
    if (this.markT >= 0) {
      this.markT += dt;
      const k = this.markT / this.markDur;
      if (k >= 1) { this.markT = -1; this.mark.classList.add('hidden'); }
      else {
        const s = this.markKind === 'pulse' ? 0.6 + 0.9 * easeOutCubic(k) : this.markKind === 'kill' ? 1.5 - 0.5 * easeOutBack(Math.min(1, k * 3)) : 1.35 - 0.35 * easeOutBack(Math.min(1, k * 3));
        const rot = this.markKind === 'kill' ? 12 * (1 - Math.min(1, k * 4)) : 0;
        this.mark.style.transform = `scale(${s.toFixed(3)}) rotate(${rot.toFixed(1)}deg)`;
        this.mark.style.opacity = (k < 0.5 ? 1 : 1 - (k - 0.5) / 0.5).toFixed(3);
      }
    }
    if (this.callT >= 0) {
      const C = JUICE.ui.callout;
      this.callT += dt;
      const ms = this.callT * 1000;
      if (ms >= C.ms) { this.callT = -1; this.call.classList.add('hidden'); }
      else {
        const pop = ms < C.popMs ? 1.8 - 0.8 * easeOutBack(ms / C.popMs) : 1;
        const out = ms > C.ms - 300 ? (ms - (C.ms - 300)) / 300 : 0;
        const s = pop * this.callSize * (1 - 0.1 * out);
        this.call.style.transform = `translate(-50%, ${(-6 * out).toFixed(1)}px) scale(${s.toFixed(3)})`;
        this.call.style.opacity = (1 - out).toFixed(3);
      }
    }
    if (this.comboT >= 0) {
      this.comboT += dt;
      const ms = this.comboT * 1000;
      const dur = JUICE.ui.combo.ms;
      if (ms >= dur) { this.comboT = -1; this.combo.classList.add('hidden'); }
      else {
        const base = 18 + Math.min(9, this.comboN) * 2.5; // escalates with the combo
        const pop = ms < 160 ? 1.9 - 0.9 * easeOutBack(ms / 160) : 1;
        const wob = this.comboN >= 5 && ms < 400 ? Math.sin(ms / 18) * (400 - ms) / 400 * 3 : 0;
        this.combo.style.fontSize = `${base}px`;
        this.combo.style.transform = `translate(0, -50%) scale(${pop.toFixed(3)}) rotate(${(wob - 4).toFixed(2)}deg)`;
        this.combo.style.opacity = (ms > dur - 400 ? (dur - ms) / 400 : 1).toFixed(3);
      }
    }
  }

  toggleDebug(on?: boolean) { this.debugOn = on ?? !this.debugOn; this.debug.classList.toggle('hidden', !this.debugOn); }
  setDebug(text: string) { if (this.debugOn && this.debug.textContent !== text) this.debug.textContent = text; }

  get liveNumbers() { return this.nums.filter((n) => n.live).length; }
}

function sizeFor(amount: number) { return 0.9 + Math.min(1.3, amount / 40); }
