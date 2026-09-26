/**
 * Duel HUD (PvP): both lanterns' names + health + the score top-centre, the round clock, a kill feed, round /
 * match banners and the warmup countdown, a respawn countdown, damage numbers and hit markers for the
 * attacker, a lock-on marker on the opponent when a cast would lock, and an arrow to an off-screen opponent.
 * Same visual language as the rest of the HUD (styles.css "duel"); DOM is only touched when values change.
 */
import { Vector3 } from 'three';
import type { World, GameEvent } from '../sim/world.js';
import type { Player } from '../sim/player.js';
import type { GameRenderer } from '../render/renderer.js';

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = '', parent?: HTMLElement, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  parent?.appendChild(e);
  return e;
}
function setText(e: HTMLElement, t: string) { if (e.textContent !== t) e.textContent = t; }
function setStyle(e: HTMLElement, k: string, v: string) { if ((e.style as any)[k] !== v) (e.style as any)[k] = v; }

/** Kill-feed verbs by cause ("p1's piano crushed p2"). */
const VERB: Record<string, string> = {
  strike: 'smashed', smash: 'smashed', crush: 'crushed', explosion: 'blew up', blast: 'blew up', shot: 'shot', gun: 'shot', cannon: 'shelled',
  missile: 'blew up', bomb: 'bombed', laser: 'lasered', flame: 'torched', bite: 'mauled', ram: 'ran over', thrown: 'flattened',
  impact: 'slammed', grab: 'crushed', fall: 'dropped', reflected: 'bounced a shot back at', net: 'netted',
};

interface Floater { el: HTMLElement; pos: Vector3; born: number }

export interface DuelHost {
  showBanner(title: string, sub?: string, ms?: number): void;
  toast(kind: 'good' | 'warn' | 'bad' | 'info' | 'note', text: string, ms?: number): void;
  readonly crosshair: HTMLElement;
}

export class DuelHud {
  private root: HTMLElement;
  private sides: { name: HTMLElement; hp: HTMLElement; hpTrail: HTMLElement; wrap: HTMLElement }[] = [];
  private score: HTMLElement;
  private clock: HTMLElement;
  private feed: HTMLElement;
  private center: HTMLElement;
  private centerBig: HTMLElement;
  private centerSmall: HTMLElement;
  private floaters: Floater[] = [];
  private floatLayer: HTMLElement;
  private oppMarker: HTMLElement;
  private oppLabel: HTMLElement;
  private lockMarker: HTMLElement;
  private hitUntil = 0;
  private killUntil = 0;
  private lastPhase = '';
  private ids: string[] = [];
  private trail = [100, 100];
  /** true when the world has another lantern (the panel is hidden otherwise) */
  active = false;

  constructor(parent: HTMLElement, private host: DuelHost, private local: () => string) {
    this.floatLayer = el('div', 'duel-floats', parent);
    this.oppMarker = el('div', 'marker opponent hidden', parent);
    el('i', '', this.oppMarker);
    this.oppLabel = el('span', '', this.oppMarker);
    this.lockMarker = el('div', 'lockon hidden', parent);
    for (const k of ['tl', 'tr', 'bl', 'br']) el('i', k, this.lockMarker);
    this.root = el('div', 'duel hidden', parent);
    const side = (cls: string) => {
      const wrap = el('div', 'side ' + cls, this.root);
      const name = el('div', 'name', wrap);
      const track = el('div', 'hp', wrap);
      const hpTrail = el('i', 'trail', track);
      const hp = el('i', 'fill', track);
      this.sides.push({ name, hp, hpTrail, wrap });
    };
    side('me');
    const mid = el('div', 'mid', this.root);
    this.score = el('div', 'score', mid, '0 – 0');
    this.clock = el('div', 'clock', mid, '');
    side('rival');
    this.feed = el('div', 'killfeed', parent);
    this.center = el('div', 'duel-center hidden', parent);
    this.centerBig = el('div', 'big', this.center);
    this.centerSmall = el('div', 'small', this.center);
  }

  reset(w: World) {
    this.feed.innerHTML = '';
    for (const f of this.floaters) f.el.remove();
    this.floaters = [];
    this.lastPhase = '';
    this.trail = [100, 100];
    this.active = w.players.length > 1;
    this.root.classList.toggle('hidden', !this.active);
  }

  private nameOf(w: World, id: string | null | undefined) {
    if (!id) return 'the arena';
    if (id === this.local()) return 'you';
    return w.playerById(id)?.name ?? id;
  }

  onEvent(w: World, e: GameEvent) {
    const me = this.local();
    switch (e.type) {
      case 'player_joined': this.active = w.players.length > 1; this.root.classList.toggle('hidden', !this.active); break;
      case 'player_damage':
        if (e.attacker === me && e.target !== me) {
          this.hitUntil = performance.now() + 160;
          this.floater(e.pos ? new Vector3(e.pos[0], e.pos[1] + 1.1, e.pos[2]) : w.playerById(e.target)?.pos.clone() ?? new Vector3(), String(Math.max(1, Math.round(e.amount))), e.amount >= 25 ? 'big' : '');
        }
        break;
      case 'player_shielded':
        if (e.attacker === me) this.floater(w.playerById(e.target)?.pos.clone().add(new Vector3(0, 1.2, 0)) ?? new Vector3(), 'shielded', 'shield');
        break;
      case 'kill': {
        const killer = this.nameOf(w, e.killer), victim = this.nameOf(w, e.victim);
        const cause = [...w.log].reverse().find((x) => x.type === 'player_killed' && x.target === e.victim)?.cause ?? 'strike';
        const verb = VERB[cause] ?? 'took down';
        const who = e.killer ? `${killer === 'you' ? 'your' : `${killer}’s`} ${e.weapon}` : e.weapon;
        const row = el('div', 'row' + (e.killer === me ? ' mine' : e.victim === me ? ' theirs' : ''), this.feed);
        el('b', '', row, who);
        el('span', '', row, ` ${verb} `);
        el('b', '', row, victim);
        while (this.feed.children.length > 5) this.feed.firstElementChild?.remove();
        setTimeout(() => row.classList.add('out'), 6500);
        setTimeout(() => row.remove(), 7000);
        if (e.killer === me) { this.killUntil = performance.now() + 450; this.host.showBanner('KNOCKOUT', `${e.weapon} → ${victim}`, 1600); }
        break;
      }
      case 'match_state': this.onPhase(w, e); break;
    }
  }

  private onPhase(w: World, e: GameEvent) {
    const me = this.local();
    const key = `${e.phase}:${e.round}:${e.suddenDeath}`;
    if (key === this.lastPhase) return;
    this.lastPhase = key;
    if (e.phase === 'live' && e.suddenDeath) this.host.showBanner('SUDDEN DEATH', 'next knockout wins', 2600);
    else if (e.phase === 'live') this.host.showBanner('FIGHT', `first to ${w.match?.rules.killsToWin ?? 3}`, 1200);
    else if (e.phase === 'round_end') {
      const won = e.roundWinner === me;
      this.host.showBanner(e.winner ? (e.winner === me ? 'VICTORY' : 'DEFEAT') : won ? 'ROUND WON' : 'ROUND LOST', `${this.scoreText(w)}`, 3000);
    } else if (e.phase === 'match_end') this.host.toast(e.winner === me ? 'good' : 'warn', `${e.winner === me ? 'You won' : `${this.nameOf(w, e.winner)} won`} the duel — Backspace for a rematch`, 8000);
  }

  private scoreText(w: World) {
    const m = w.match;
    if (!m) return '';
    const [a, b] = this.order(w).map((p) => m.scores[p.id] ?? 0);
    return `${a} – ${b}`;
  }

  /** Local player first, then the (first) rival. */
  private order(w: World): Player[] {
    const me = w.playerById(this.local()) ?? w.player;
    const rival = w.opponentsOf(me.id)[0];
    return rival ? [me, rival] : [me];
  }

  private floater(pos: Vector3, text: string, cls: string) {
    const e = el('div', 'dmg ' + cls, this.floatLayer, text);
    this.floaters.push({ el: e, pos, born: performance.now() });
    if (this.floaters.length > 24) this.floaters.shift()!.el.remove();
  }

  update(w: World, r: GameRenderer) {
    const now = performance.now();
    this.host.crosshair.classList.toggle('hit', now < this.hitUntil);
    this.host.crosshair.classList.toggle('kill', now < this.killUntil);
    const W = r.canvas.clientWidth, H = r.canvas.clientHeight;
    // damage numbers: rise and fade over 0.9 s
    this.floaters = this.floaters.filter((f) => {
      const age = (now - f.born) / 1000;
      if (age > 0.9) { f.el.remove(); return false; }
      const pr = r.project(f.pos);
      setStyle(f.el, 'transform', `translate(${pr.x.toFixed(0)}px, ${(pr.y - age * 50).toFixed(0)}px) translate(-50%, -50%) scale(${(1 + Math.max(0, 0.25 - age) * 1.6).toFixed(2)})`);
      setStyle(f.el, 'opacity', pr.behind ? '0' : (1 - Math.max(0, age - 0.5) / 0.4).toFixed(2));
      return true;
    });
    const players = this.order(w);
    this.active = players.length > 1;
    this.root.classList.toggle('hidden', !this.active);
    if (!this.active) { this.oppMarker.classList.add('hidden'); this.lockMarker.classList.add('hidden'); this.center.classList.add('hidden'); return; }
    const m = w.match;
    // names, health, score, clock
    players.forEach((p, i) => {
      const s = this.sides[i];
      setText(s.name, (i === 0 ? 'YOU' : p.name.toUpperCase()) + (p.alive ? '' : ' · DOWN'));
      const hp = Math.max(0, Math.min(100, (p.hp / p.maxHp) * 100));
      setStyle(s.hp, 'width', `${hp.toFixed(1)}%`);
      this.trail[i] = Math.max(hp, this.trail[i] - 0.6);
      setStyle(s.hpTrail, 'width', `${this.trail[i].toFixed(1)}%`);
      s.wrap.classList.toggle('low', hp < 30);
      s.wrap.classList.toggle('down', !p.alive);
      s.wrap.classList.toggle('shield', p.alive && p.isProtected);
      s.wrap.classList.toggle('hurt', w.time - p.hurtAt < 0.25);
    });
    setText(this.score, m ? this.scoreText(w) : 'practice');
    let clock = '';
    if (m) {
      if (m.phase === 'warmup') clock = 'get ready';
      else if (m.phase === 'live') clock = m.suddenDeath ? 'SUDDEN DEATH' : fmtTime(m.timeLeft);
      else if (m.phase === 'round_end') clock = 'round over';
      else clock = 'duel over';
    }
    setText(this.clock, clock);
    // centre: warmup countdown / respawn countdown
    const me = players[0];
    let big = '', small = '';
    if (m?.phase === 'warmup') { big = String(Math.max(1, Math.ceil(m.timeLeft))); small = `round ${m.round} · first to ${m.rules.killsToWin}`; }
    else if (!me.alive && isFinite(me.respawnAt)) { big = `RESPAWN ${Math.max(0, me.respawnAt - w.time).toFixed(1)}`; small = 'your constructs dissolved with you'; }
    else if (me.alive && me.isProtected) { small = 'spawn shield'; }
    this.center.classList.toggle('hidden', !big && !small);
    setText(this.centerBig, big);
    setText(this.centerSmall, small);
    this.center.classList.toggle('count', m?.phase === 'warmup');
    // opponent marker (arrow at the screen edge when off-screen) + lock-on brackets
    const opp = players[1];
    const oppPos = (r.playerPos.get(opp.id) ?? opp.pos).clone().add(new Vector3(0, 1.3, 0));
    this.oppMarker.classList.toggle('hidden', !opp.alive);
    if (opp.alive) {
      const pr = r.project(oppPos);
      const M = 40;
      let x = pr.x, y = pr.y;
      const off = pr.behind || x < M || x > W - M || y < M || y > H - M;
      let ang = 0;
      if (off) {
        const cs = r.toCameraSpace(oppPos);
        ang = Math.atan2(-cs.y, cs.x);
        if (!pr.behind && cs.z < 0) ang = Math.atan2(y - H / 2, x - W / 2);
        x = W / 2 + Math.cos(ang) * (W / 2 - M);
        y = H / 2 + Math.sin(ang) * (H / 2 - M);
        y = Math.max(M + 40, Math.min(y, H - 150));
        x = Math.max(M, Math.min(W - M, x));
      }
      this.oppMarker.classList.toggle('edge', off);
      setStyle(this.oppMarker, 'transform', `translate(${x.toFixed(0)}px, ${y.toFixed(0)}px)`);
      setStyle(this.oppMarker.firstElementChild as HTMLElement, 'transform', off ? `rotate(${((ang * 180) / Math.PI + 90).toFixed(0)}deg)` : '');
      setText(this.oppLabel, `${opp.name} · ${opp.pos.distanceTo(me.pos).toFixed(0)} m`);
    }
    // lock-on: brackets on the rival when a cast would lock (cone / near the ray, line of sight)
    const lock = me.alive ? w.lockTarget(me.id) : null;
    this.lockMarker.classList.toggle('hidden', !lock);
    if (lock) {
      const lp = (r.playerPos.get(lock.id) ?? lock.pos).clone().add(new Vector3(0, 0.3, 0));
      const pr = r.project(lp);
      const d = Math.max(4, r.camPos.distanceTo(lp));
      const size = Math.max(26, Math.min(90, 1400 / d));
      setStyle(this.lockMarker, 'transform', `translate(${pr.x.toFixed(0)}px, ${pr.y.toFixed(0)}px)`);
      setStyle(this.lockMarker, 'width', `${size.toFixed(0)}px`);
      setStyle(this.lockMarker, 'height', `${(size * 1.5).toFixed(0)}px`);
    }
  }
}

function fmtTime(s: number) {
  if (!isFinite(s)) return '';
  const t = Math.ceil(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`;
}
