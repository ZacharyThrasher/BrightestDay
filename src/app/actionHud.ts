/**
 * HUD for adaptive construct controls (sim/constructs/actions.ts): next to the crosshair, the equipped
 * construct and what its mouse buttons do ("hammer · LMB Smash · RMB Charge"), a compact list of your
 * usable constructs (equipped one highlighted; the wheel cycles), a charge ring around the crosshair, and
 * in-world aim previews (strike line, throw / launch arc, slam radius, lock marker) drawn with the existing
 * glow particles. Reuses the HUD's classes and the construct colour — no new art.
 */
import { Color, Vector3 } from 'three';
import type { World } from '../sim/world.js';
import type { GameRenderer } from '../render/renderer.js';
import type { ActionView } from '../sim/constructs/actions.js';

const SVG = 'http://www.w3.org/2000/svg';
const R = 17; // charge ring radius (px)
const CIRC = 2 * Math.PI * R;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, parent: HTMLElement, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  parent.appendChild(e);
  return e;
}

export class ActionHud {
  private root: HTMLElement;
  private prompt: HTMLElement;
  private list: HTMLElement;
  private ring: SVGCircleElement;
  private ringWrap: SVGSVGElement;
  private promptKey = '';
  private listKey = '';
  private color: Color;
  private dash = '';
  private lockT = 0;
  private ringT = 0;
  private zoomed = false;

  constructor(parent: HTMLElement, color: string) {
    this.color = new Color(color);
    this.root = el('div', 'action-hud', parent);
    this.prompt = el('div', 'key-hint action-prompt hidden', this.root);
    this.list = el('div', 'key-hint quick action-list hidden', this.root);
    this.ringWrap = document.createElementNS(SVG, 'svg');
    this.ringWrap.setAttribute('class', 'action-charge');
    this.ringWrap.setAttribute('width', String(2 * R + 6));
    this.ringWrap.setAttribute('height', String(2 * R + 6));
    this.ring = document.createElementNS(SVG, 'circle');
    this.ring.setAttribute('cx', String(R + 3));
    this.ring.setAttribute('cy', String(R + 3));
    this.ring.setAttribute('r', String(R));
    this.ring.setAttribute('stroke-dasharray', `0 ${CIRC}`);
    this.ringWrap.appendChild(this.ring);
    parent.appendChild(this.ringWrap);
  }

  setColor(hex: string) { this.color.set(hex); }

  /** Per rendered frame. `baseFov` = the player's FOV setting (precision mode zooms in slightly). */
  update(w: World, gr: GameRenderer, dt: number, baseFov: number, active: boolean) {
    const v = w.actions.view(w.player);
    this.renderPrompt(v);
    this.renderList(v);
    // charge ring
    const dash = v.charge > 0 ? `${(v.charge * CIRC).toFixed(1)} ${CIRC.toFixed(1)}` : `0 ${CIRC.toFixed(1)}`;
    if (dash !== this.dash) { this.dash = dash; this.ring.setAttribute('stroke-dasharray', dash); this.ringWrap.classList.toggle('full', v.charge >= 1); }
    // precision: a slight zoom
    if (v.precise !== this.zoomed) { this.zoomed = v.precise; gr.setFov(v.precise ? baseFov * 0.8 : baseFov); }
    if (active) this.drawPreview(v, gr, dt);
  }

  private renderPrompt(v: ActionView) {
    const e = v.equipped;
    const key = e ? `${e.label}|${e.lmb}|${e.rmb}` : '';
    if (key === this.promptKey) return;
    this.promptKey = key;
    this.prompt.classList.toggle('hidden', !e);
    this.prompt.innerHTML = '';
    if (!e) return;
    el('span', 'nm', this.prompt, e.label);
    this.prompt.append(' · ');
    el('b', '', this.prompt, 'LMB');
    this.prompt.append(` ${e.lmb} · `);
    el('b', '', this.prompt, 'RMB');
    this.prompt.append(` ${e.rmb}`);
  }

  private renderList(v: ActionView) {
    const key = v.list.map((x) => x.id + (x.equipped ? '*' : '')).join('|');
    if (key === this.listKey) return;
    this.listKey = key;
    this.list.innerHTML = '';
    this.list.classList.toggle('hidden', v.list.length < 2);
    for (const x of v.list.slice(-6)) {
      const s = el('span', 'slot' + (x.equipped ? ' saved' : ''), this.list);
      el('span', 'txt', s, x.label);
    }
  }

  /** Aim preview in the world: dots along the line / arc, a ring for a slam radius, and a lock marker. */
  private drawPreview(v: ActionView, gr: GameRenderer, dt: number) {
    const p = v.preview;
    if (p && p.points.length >= 2) {
      const pts = p.points;
      const step = p.kind === 'arc' ? 1.4 : 1.8;
      let carry = 0;
      let n = 0;
      for (let i = 1; i < pts.length && n < 40; i++) {
        const a = pts[i - 1], b = pts[i];
        const len = a.distanceTo(b);
        let d = carry;
        while (d < len && n < 40) {
          gr.fx.glow(new Vector3().lerpVectors(a, b, d / Math.max(1e-6, len)), this.color, 0.13 + v.charge * 0.06, 0.05, 1.6);
          d += step;
          n++;
        }
        carry = d - len;
      }
      const end = pts[pts.length - 1];
      if (p.radius) { this.ringT -= dt; if (this.ringT <= 0) { this.ringT = 0.2; gr.fx.ring(end, this.color, p.radius * 0.3, p.radius * 2, 0.22, 1.4); } }
    }
    if (v.lock) {
      this.lockT -= dt;
      if (this.lockT <= 0) { this.lockT = 0.25; gr.fx.ring(v.lock, this.color, 0.6, 2.4, 0.25, 1.8); }
    }
  }
}
