/**
 * JUICE — game feel for Brightest Day, driven purely by world events and the interpolated render state.
 *
 * Owns: trauma screen shake, directional camera kicks, FOV punches, kill cam, render-side hitstop
 * (per-object freeze + smooth catch-up; the sim never pauses — safe online), construct snap / crumble /
 * flash-on-hit through existing material uniforms, entity squash on hit, extra impact fx through the
 * existing particle system, procedural sound (src/audio), damage numbers / hit markers / callouts
 * (./ui.ts) and the F4 debug overlay. Every number lives in ./tuning.ts (JUICE).
 *
 * Wiring (see hooks marked "JUICE" in renderer.ts, constructs.ts, entities.ts, main.ts, settings.ts):
 *   renderer.tick()    → juice.tick()            player movement events (jump, land, tethers…)
 *   renderer.render()  → juice.camera(cam, dt)   shake / kicks / kill cam, returns a FOV offset
 *                      → constructs.poseHook(v)  hitstop freeze, snap / crumble / flash uniforms
 *                      → juice.afterViews(dt)    entity freeze + squash, HUD numbers, debug
 *   world.listeners    → juice.onEvent(e)        everything else
 */
import { Color, Matrix4, Quaternion, Vector3, type PerspectiveCamera } from 'three';
import type { World, GameEvent } from '../../sim/world.js';
import type { GameRenderer, RenderJuice } from '../../render/renderer.js';
import type { View } from '../../render/constructs.js';
import type { CommitResult } from '../../lang/types.js';
import type { Settings } from '../settings.js';
import { Sfx } from '../../audio/sfx.js';
import { JuiceUI } from './ui.js';
import { JUICE } from './tuning.js';
import {
  Trauma, Spring, Hitstop, SpeedThresholds, mapEvent, mapPlayerEvent, killCamWeight, noise1, impulseForPeak, clamp, smoothstep,
  type Effect, type JuiceContext, type V3,
} from './core.js';

export interface JuiceHost {
  readonly world: World;
  readonly gr: GameRenderer;
  readonly hud: { readonly root: HTMLElement; readonly consoleInput: HTMLInputElement };
  settings: Settings;
  playerId: string;
  readonly combo: { n: number };
}

const DEG = Math.PI / 180;
const WHITE = new Color(1, 1, 1);
const HURT = new Color('#ff5a4d');
const _v = new Vector3(), _v2 = new Vector3(), _q = new Quaternion(), _m = new Matrix4();
const AX = { x: new Vector3(), y: new Vector3(), z: new Vector3() };

interface Frozen { version: number; count: number; pos: Float32Array[]; quat: Float32Array[] }

export class Juice implements RenderJuice {
  readonly sfx: Sfx;
  readonly ui: JuiceUI;
  readonly trauma = new Trauma();
  readonly hitstop = new Hitstop();
  private kick = { x: this.posSpring(), y: this.posSpring(), z: this.posSpring(), pitch: this.rotSpring(), yaw: this.rotSpring() };
  private fovSpring = new Spring(JUICE.fov.springK, JUICE.fov.springC);
  private speedTh = new SpeedThresholds();
  private killCam: { t: number; key: string; pos: Vector3 | null } | null = null;
  private charge: { id: string; level: number } | null = null;
  private hurtV = 0;
  private gW = 0; // whole-view hitstop weight this frame
  private time = 0;
  private prevVy = 0;
  private prevBoost = false;
  private world: World | null = null;
  private listener: ((e: GameEvent) => void) | null = null;
  private frozenC = new Map<string, Frozen>();
  private frozenE = new Map<string, Vector3>();
  private squash = new Map<string, Spring>();
  private flashes = new Map<string, number>();
  private owners = new Map<string, string>(); // construct → owner, remembered past removal
  private seenKills = new Set<string>();
  private lastFormedAt = -1e9;
  private lastRejectAt = -1e9;
  private keyTimes: number[] = [];
  private debugAcc = 0;
  private frameCpu = 0;
  private color = new Color();
  readonly ctx: JuiceContext;
  /** stats (tests, F4 overlay) */
  readonly counts: Record<string, number> = {};
  readonly stat = { snaps: 0, frozenFrames: 0, frozenEntityFrames: 0, squashes: 0, flashes: 0, killCams: 0, fovPunches: 0, events: 0 };
  readonly cpu: number[] = [];
  lastFov = 0;
  lastShake = 0;

  constructor(readonly host: JuiceHost, sfx?: Sfx) {
    this.sfx = sfx ?? new Sfx();
    this.ui = new JuiceUI(host.hud.root);
    const gr = host.gr;
    gr.juice = this;
    gr.constructs.poseHook = (v) => this.onConstructPose(v);
    this.ctx = {
      local: host.playerId,
      ownerOf: (id) => this.ownerOf(id),
      distance: (p) => (p ? gr.camPos.distanceTo(_v.set(p[0], p[1], p[2])) : Infinity),
      posOf: (k) => this.posOf(k),
      playerPos: () => { const p = this.world?.player.pos; return p ? [p.x, p.y, p.z] : [0, 0, 0]; },
      keyOf: (id) => this.keyOf(id),
    };
    this.applySettings();
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', () => this.sfx.setMuted(document.hidden));
      const resume = () => this.sfx.resume();
      window.addEventListener('pointerdown', resume, true);
      window.addEventListener('keydown', resume, true);
      window.addEventListener('keydown', (e) => { if (e.code === 'F4') { e.preventDefault(); this.ui.toggleDebug(); } });
      host.hud.consoleInput.addEventListener('input', (e) => this.onKey((e as InputEvent).inputType ?? ''));
    }
  }

  private posSpring() { return new Spring(JUICE.kick.stiffness, JUICE.kick.damping); }
  private rotSpring() { return new Spring(JUICE.kick.rotStiffness, JUICE.kick.rotDamping); }

  /** 0..1 camera shake from settings (0 = off). */
  get shakeSetting() { return clamp(this.host.settings.shake ?? JUICE.defaults.shake, 0, 1); }
  /** Kicks / FOV punches / kill cam scale: off at 0, then 50–100% so they still read at low shake. */
  get motionScale() { const s = this.shakeSetting; return s <= 0 ? 0 : 0.5 + 0.5 * s; }

  applySettings() {
    const s = this.host.settings;
    this.sfx.setVolumes(s.masterVolume ?? JUICE.defaults.master, s.sfxVolume ?? JUICE.defaults.sfx);
    this.ctx.local = this.host.playerId;
  }

  /** Attach to a (new) world. Call after gr.setWorld(w). */
  setWorld(w: World) {
    if (this.world && this.listener) { const i = this.world.listeners.indexOf(this.listener); if (i >= 0) this.world.listeners.splice(i, 1); }
    this.world = w;
    this.listener = (e) => this.onEvent(e);
    w.listeners.push(this.listener);
    this.hitstop.clear();
    this.frozenC.clear(); this.frozenE.clear(); this.squash.clear(); this.flashes.clear(); this.owners.clear(); this.seenKills.clear();
    this.killCam = null; this.charge = null; this.trauma.value = 0; this.hurtV = 0;
    this.prevVy = w.player.vel.y;
    this.sfx.updateCharge(null);
  }

  // ------------------------------------------------------------------------------------------ world queries

  private ownerOf(id: string): string | null {
    if (typeof id !== 'string') return null;
    const c = this.world?.constructs.get(id);
    if (c) { this.owners.set(id, c.owner); return c.owner; }
    return this.owners.get(id) ?? null;
  }

  private players(): Map<string, { pos: Vector3 }> | null {
    const p = (this.world as any)?.players;
    if (p instanceof Map) return p;
    if (Array.isArray(p)) return new Map(p.map((x: any) => [x.id, x]));
    return null;
  }

  private keyOf(id: string): string | null {
    const w = this.world;
    if (!w) return null;
    if (w.entities.has(id)) return `e:${id}`;
    if (w.constructs.get(id)) return `c:${id}`;
    if (id === this.host.playerId || this.players()?.has(id)) return `p:${id}`;
    return null;
  }

  private posOf(key: string): V3 | null {
    const w = this.world;
    if (!w || key.length < 3) return null;
    const id = key.slice(2);
    switch (key[0]) {
      case 'c': {
        const c = w.constructs.get(id);
        const p = this.host.gr.constructs.positionOf(id, _v2);
        if (p && c) return [p.x, p.y, p.z];
        const q = c?.insts[0]?.pos;
        return q ? [q.x, q.y, q.z] : null;
      }
      case 'e': {
        const obj = this.host.gr.entities.objectOf(id);
        if (obj) return [obj.position.x, obj.position.y, obj.position.z];
        const e = w.entities.get(id);
        if (!e) return null;
        const p = w.entityPos(e, _v2);
        return [p.x, p.y, p.z];
      }
      case 'p': {
        const pl = id === this.host.playerId || id === 'p1' && !this.players() ? w.player : this.players()?.get(id);
        const p = pl?.pos;
        return p ? [p.x, p.y, p.z] : null;
      }
    }
    return null;
  }

  // ------------------------------------------------------------------------------------------ events

  onEvent(e: GameEvent) {
    const t0 = performance.now();
    this.stat.events++;
    if (e.type === 'construct_formed' || e.type === 'construct_modified') this.ownerOf(e.id); // remember the owner
    if (e.type === 'construct_formed') this.lastFormedAt = t0;
    if (e.type === 'construct_rejected') this.lastRejectAt = t0;
    if (e.type === 'kill' || e.type === 'player_killed') {
      const k = `${e.target ?? e.victim}@${e.t}`;
      if (this.seenKills.has(k)) return;
      this.seenKills.add(k);
      if (this.seenKills.size > 64) this.seenKills.clear();
    }
    let eff: Effect | null = null;
    try { eff = mapEvent(e, this.ctx); } catch (err) { console.warn('[juice] event mapping failed', e.type, err); }
    if (eff) this.apply(eff);
    this.frameCpu += performance.now() - t0;
  }

  /** Apply one effect (public so tools can drive it). */
  apply(eff: Effect) {
    this.counts[eff.kind] = (this.counts[eff.kind] ?? 0) + 1;
    const ms = this.motionScale;
    if (eff.trauma) this.trauma.add(eff.trauma);
    if (eff.hitstop) for (const k of eff.hitstop.keys) this.hitstop.trigger(k, eff.hitstop.ms);
    if (eff.kick && ms > 0) {
      const K = JUICE.kick;
      const imp = (peak: number) => impulseForPeak(clamp(peak, -K.maxOffset, K.maxOffset), K.stiffness, K.damping) * ms;
      if (eff.kick.dir && eff.kick.mag) { const s = imp(eff.kick.mag); this.kick.x.impulse(eff.kick.dir[0] * s); this.kick.y.impulse(eff.kick.dir[1] * s); this.kick.z.impulse(eff.kick.dir[2] * s); }
      if (eff.kick.lift) this.kick.y.impulse(imp(eff.kick.lift));
      if (eff.kick.pitch) this.kick.pitch.impulse(impulseForPeak(clamp(eff.kick.pitch, -K.maxRot, K.maxRot), K.rotStiffness, K.rotDamping) * ms);
    }
    if (eff.fov && ms > 0) { this.fovSpring.impulse(impulseForPeak(eff.fov, JUICE.fov.springK, JUICE.fov.springC) * ms); this.stat.fovPunches++; }
    if (eff.sound) this.sfx.play(eff.sound.name, { energy: eff.sound.energy, pos: eff.sound.pos ?? null, kind: eff.sound.kind, novelty: eff.sound.novelty });
    if (eff.fx) this.punchFx(eff.fx.pos, eff.fx.energy, eff.fx.tone);
    if (eff.number) this.ui.damage(eff.number.key, eff.number.amount);
    if (eff.marker) this.ui.marker(eff.marker);
    if (eff.callout) this.ui.callout(eff.callout.text, eff.callout.tone, eff.callout.size ?? 1, eff.callout.sub ?? '');
    if (eff.flash) { this.flashes.set(eff.flash.constructId, Math.max(this.flashes.get(eff.flash.constructId) ?? 0, eff.flash.amount)); this.stat.flashes++; }
    if (eff.squash) {
      let s = this.squash.get(eff.squash.entityId);
      if (!s) { s = new Spring(JUICE.fx.squash.k, JUICE.fx.squash.c); this.squash.set(eff.squash.entityId, s); }
      s.impulse(impulseForPeak(JUICE.fx.squash.amount * eff.squash.amount, JUICE.fx.squash.k, JUICE.fx.squash.c));
      this.stat.squashes++;
    }
    if (eff.killCam) {
      const p = eff.killCam.pos ?? this.posOf(eff.killCam.key);
      this.killCam = { t: 0, key: eff.killCam.key, pos: p ? new Vector3(...p) : null };
      this.stat.killCams++;
    }
    if ((eff.kind === 'kill' || eff.kind === 'ko') && eff.hitstop) {
      const k = eff.hitstop.keys.find((x) => x !== '*');
      const p = k ? this.posOf(k) : null;
      if (p) { const v = new Vector3(...p); this.host.gr.fx.ring(v, WHITE, 0.4, 7, 0.4, 3); this.host.gr.fx.glow(v, WHITE, 2.5, 0.14, 4); }
    }
    if (eff.hurt) this.hurtV = Math.max(this.hurtV, eff.hurt);
    if (eff.charge) {
      if (eff.charge.level === null) { if (!this.charge || this.charge.id === eff.charge.constructId) { this.charge = null; this.sfx.updateCharge(null); } }
      else { this.charge = { id: eff.charge.constructId, level: eff.charge.level }; this.sfx.updateCharge(eff.charge.level); }
    }
  }

  /** Bigger, punchier impact read on top of the renderer's own fx (same particle system, same colours). */
  private punchFx(pos: V3, e: number, tone: 'construct' | 'white' | 'hurt') {
    const fx = this.host.gr.fx;
    const p = new Vector3(pos[0], pos[1], pos[2]);
    const col = tone === 'hurt' ? HURT : tone === 'white' ? WHITE : this.color.set(this.host.settings.constructColor);
    const F = JUICE.fx;
    fx.glow(p, WHITE, 0.6 + F.coreFlash * e, 0.09, 4);
    fx.ring(p, WHITE, 0.2, 1.5 + F.shockRing * e, 0.16, 2.5);
    const n = Math.round(F.sparksPerEnergy * e);
    if (n > 0) fx.sparks(p, col, n, 10 + 14 * e, { size: 0.05, life: 0.35 });
  }

  /** The ring console: a subtle click per character, pitch rising with typing speed and combo. */
  private onKey(inputType: string) {
    const now = performance.now();
    this.keyTimes.push(now);
    if (this.keyTimes.length > 8) this.keyTimes.shift();
    const n = this.keyTimes.length;
    const cps = n > 1 ? (n - 1) / Math.max(0.05, (this.keyTimes[n - 1] - this.keyTimes[0]) / 1000) : 0;
    if (n > 1 && now - this.keyTimes[n - 2] > 1500) this.keyTimes = [now];
    const del = inputType.startsWith('delete');
    const pitch = del ? 0.8 : 1 + clamp((cps - 3) / 12, 0, 0.45) + Math.min(9, this.host.combo.n) * 0.025;
    this.sfx.play('keyclick', { pitch, energy: 0.3 });
    this.counts.keyclick = (this.counts.keyclick ?? 0) + 1;
  }

  /** After a cast through the ring: confirm / fail sound and the combo counter. */
  onCast(r: CommitResult | null, ok: boolean) {
    const now = performance.now();
    if (!ok) {
      if (now - this.lastRejectAt > 400) this.sfx.play('castFail', { energy: 0.6 });
      this.counts.cast_fail = (this.counts.cast_fail ?? 0) + 1;
      return;
    }
    const n = this.host.combo.n;
    if (now - this.lastFormedAt > 400) this.sfx.play('confirm', { pitch: 1 + 0.06 * Math.min(9, n) });
    if (n > 1) { this.ui.setCombo(n); this.sfx.play('confirm', { pitch: 1.2 + 0.08 * Math.min(9, n), gain: 0.7 }); }
    this.counts.cast = (this.counts.cast ?? 0) + 1;
    void r;
  }

  // ------------------------------------------------------------------------------------------ RenderJuice

  tick() {
    const w = this.world;
    if (!w) return;
    const t0 = performance.now();
    const P = w.player;
    for (const ev of P.events) {
      let anchor: V3 | null = null;
      if (ev.startsWith('tether_attach_')) { const t = P.tethers[ev.endsWith('L') ? 'L' : 'R']; if (t) anchor = [t.origin.x, t.origin.y, t.origin.z]; }
      const eff = mapPlayerEvent(ev, { fallSpeed: Math.max(0, -this.prevVy), anchor, playerPos: [P.pos.x, P.pos.y, P.pos.z] });
      if (eff) this.apply(eff);
    }
    this.prevVy = P.vel.y;
    this.frameCpu += performance.now() - t0;
  }

  get fxTimeScale() { return 1 - (1 - JUICE.hitstop.fxSlow) * (this.hitstop.holding('*') ? 1 : this.gW); }
  get hurt() { return this.hurtV; }

  camera(cam: PerspectiveCamera, dt: number): number {
    const t0 = performance.now();
    const w = this.world;
    this.time += dt;
    this.hitstop.update(dt * 1000);
    this.gW = this.hitstop.weight('*');
    this.trauma.update(dt);
    this.kick.x.update(dt); this.kick.y.update(dt); this.kick.z.update(dt); this.kick.pitch.update(dt); this.kick.yaw.update(dt);
    this.fovSpring.update(dt);
    this.hurtV = Math.max(0, this.hurtV - dt * 2.2);
    const ms = this.motionScale;
    if (w) {
      const P = w.player;
      const speed = P.vel.length();
      const th = this.speedTh.update(speed);
      if (th >= 0) { this.apply({ kind: 'speed', fov: JUICE.fov.speedKick + th, sound: { name: 'whoosh', energy: 0.4 + 0.3 * th } }); }
      const boosting = !!w.input?.boost && P.gas > 0.5;
      if (boosting && !this.prevBoost) this.apply({ kind: 'boost', fov: JUICE.fov.boostStart });
      this.prevBoost = boosting;
      this.sfx.updateMovement(speed, boosting);
    }
    // camera local axes
    AX.x.set(1, 0, 0).applyQuaternion(cam.quaternion);
    AX.y.set(0, 1, 0).applyQuaternion(cam.quaternion);
    AX.z.set(0, 0, 1).applyQuaternion(cam.quaternion);
    // directional kicks (world space) + trauma shake (camera space)
    if (ms > 0) cam.position.add(_v.set(this.kick.x.x, this.kick.y.x, this.kick.z.x).multiplyScalar(1));
    const sh = this.trauma.shake * this.shakeSetting;
    this.lastShake = sh;
    const T = JUICE.trauma;
    let pitch = ms > 0 ? this.kick.pitch.x * DEG : 0, yaw = 0, roll = 0;
    if (sh > 1e-4) {
      const tt = this.time * T.freq;
      cam.position.addScaledVector(AX.x, noise1(tt, 1) * T.offset * sh).addScaledVector(AX.y, noise1(tt, 2) * T.offset * sh).addScaledVector(AX.z, noise1(tt, 3) * T.offset * 0.5 * sh);
      pitch += noise1(tt, 4) * T.rot * DEG * sh;
      yaw += noise1(tt, 5) * T.rot * DEG * sh;
      roll += noise1(tt, 6) * T.roll * DEG * sh;
    }
    if (pitch || yaw || roll) { cam.rotateX(pitch); cam.rotateY(yaw); cam.rotateZ(roll); }
    // kill cam: lean the view toward the victim + a slow zoom
    let killFov = 0;
    if (this.killCam) {
      const kc = this.killCam;
      kc.t += dt;
      const kw = killCamWeight(kc.t);
      if (kc.t >= JUICE.camera.killCam.dur) this.killCam = null;
      else {
        const p = this.posOf(kc.key);
        if (p) (kc.pos ??= new Vector3()).set(p[0], p[1], p[2]);
        if (kc.pos && ms > 0) {
          _m.lookAt(cam.position, kc.pos, cam.up);
          _q.setFromRotationMatrix(_m);
          cam.quaternion.slerp(_q, kw * JUICE.camera.killCam.look * ms);
        }
        killFov = JUICE.fov.killZoom * kw * smoothstep(kc.t / JUICE.camera.killCam.dur * 1.5);
      }
    }
    // charged action: a slight push-in while winding up
    const chargeFov = this.charge ? -2 * this.charge.level : 0;
    const fov = ms > 0 ? clamp(this.fovSpring.x + killFov + chargeFov, -JUICE.fov.clamp, JUICE.fov.clamp) * ms : 0;
    this.lastFov = fov;
    // listener follows the (shaken) camera
    this.sfx.setListener([cam.position.x, cam.position.y, cam.position.z], [-AX.z.x, -AX.z.y, -AX.z.z], [AX.y.x, AX.y.y, AX.y.z]);
    this.frameCpu += performance.now() - t0;
    return fov;
  }

  /** constructs.ts poseHook: hitstop freeze (instance arrays), snap / crumble (uPulse), flash (uFlash). */
  onConstructPose(v: View) {
    const key = `c:${v.id}`;
    const wgt = Math.max(this.hitstop.weight(key), this.gW);
    if (wgt <= 0) { if (this.frozenC.size) this.frozenC.delete(v.id); }
    else {
      const f = this.frozenC.get(v.id);
      if (!f || f.count !== v.count || f.version !== v.version || f.pos.length !== v.groups.length) {
        this.frozenC.set(v.id, {
          version: v.version, count: v.count,
          pos: v.groups.map((g) => Float32Array.from((g.geo.fill.getAttribute('iPos').array as Float32Array).subarray(0, v.count * 3))),
          quat: v.groups.map((g) => Float32Array.from((g.geo.fill.getAttribute('iQuat').array as Float32Array).subarray(0, v.count * 4))),
        });
      } else {
        this.stat.frozenFrames++;
        const hold = this.hitstop.holding(key) || this.hitstop.holding('*');
        const j = hold ? JUICE.hitstop.jitter * this.shakeSetting : 0;
        const jx = (Math.random() * 2 - 1) * j, jy = (Math.random() * 2 - 1) * j, jz = (Math.random() * 2 - 1) * j;
        v.groups.forEach((g, gi) => {
          const pa = g.geo.fill.getAttribute('iPos').array as Float32Array;
          const qa = g.geo.fill.getAttribute('iQuat').array as Float32Array;
          const fp = f.pos[gi], fq = f.quat[gi];
          for (let k = 0; k < v.count; k++) {
            pa[k * 3] += (fp[k * 3] - pa[k * 3]) * wgt + jx;
            pa[k * 3 + 1] += (fp[k * 3 + 1] - pa[k * 3 + 1]) * wgt + jy;
            pa[k * 3 + 2] += (fp[k * 3 + 2] - pa[k * 3 + 2]) * wgt + jz;
            const o = k * 4;
            const s = qa[o] * fq[o] + qa[o + 1] * fq[o + 1] + qa[o + 2] * fq[o + 2] + qa[o + 3] * fq[o + 3] < 0 ? -1 : 1;
            let x = qa[o] + (fq[o] * s - qa[o]) * wgt, y = qa[o + 1] + (fq[o + 1] * s - qa[o + 1]) * wgt, z = qa[o + 2] + (fq[o + 2] * s - qa[o + 2]) * wgt, ww = qa[o + 3] + (fq[o + 3] * s - qa[o + 3]) * wgt;
            const l = Math.hypot(x, y, z, ww) || 1;
            qa[o] = x / l; qa[o + 1] = y / l; qa[o + 2] = z / l; qa[o + 3] = ww / l;
          }
        });
      }
    }
    // assembly snap (anticipation → overshoot → settle) and dissolve crumble, via the existing uPulse uniform
    const F = JUICE.fx;
    let pulse = 0, flash = this.flashes.get(v.id) ?? 0;
    if (v.dissolveT >= 0) {
      if (v.dissolveT < F.crumble.dur) pulse = F.crumble.implode * Math.sin((v.dissolveT / F.crumble.dur) * Math.PI * 0.5 + Math.PI * 0.5);
    } else {
      const amp = v.formStart === 0 ? 1 : 0.55;
      const after = v.formT - v.formDur;
      if (after < 0) { const fp = v.formT / v.formDur; pulse = F.snap.anticipation * amp * smoothstep((fp - 0.5) / 0.5); }
      else if (after < 0.6) {
        if (after - (this.lastDt ?? 0) < 0) this.stat.snaps++;
        pulse = F.snap.overshoot * amp * Math.exp(-after / F.snap.settle) * Math.cos((after * Math.PI * 2) / 0.2);
        flash += 0.9 * amp * Math.exp(-after / 0.07);
      }
    }
    v.u.uPulse.value = pulse;
    if (flash > 0) v.u.uFlash.value += flash;
  }
  private lastDt = 0;

  afterViews(dt: number) {
    const t0 = performance.now();
    this.lastDt = dt;
    const w = this.world;
    const gr = this.host.gr;
    for (const [id, f] of this.flashes) { const n = f - dt * 3; if (n <= 0) this.flashes.delete(id); else this.flashes.set(id, n); }
    if (w) {
      // entity hitstop freeze
      const act = this.hitstop.active();
      const ids = this.gW > 0 || this.frozenE.size || act.length ? new Set<string>(this.frozenE.keys()) : null;
      if (ids && this.gW > 0) for (const id of w.entities.keys()) ids.add(id);
      if (ids) for (const k of act) if (k.startsWith('e:')) ids.add(k.slice(2));
      if (ids) for (const id of ids) {
        const obj = gr.entities.objectOf(id);
        const wgt = Math.max(this.hitstop.weight(`e:${id}`), this.gW);
        if (!obj || wgt <= 0) { this.frozenE.delete(id); continue; }
        const cap = this.frozenE.get(id);
        if (!cap) { this.frozenE.set(id, obj.position.clone()); continue; }
        obj.position.lerp(cap, wgt);
        if (this.hitstop.holding(`e:${id}`) || this.hitstop.holding('*')) {
          const j = JUICE.hitstop.jitter * this.shakeSetting;
          obj.position.x += (Math.random() * 2 - 1) * j; obj.position.y += (Math.random() * 2 - 1) * j; obj.position.z += (Math.random() * 2 - 1) * j;
        }
        this.stat.frozenEntityFrames++;
      }
      // squash & stretch on hit (transform only)
      for (const [id, s] of this.squash) {
        s.update(dt);
        const obj = gr.entities.objectOf(id);
        if (!obj || s.settled) { this.squash.delete(id); continue; }
        const x = clamp(s.x, -0.45, 0.45);
        obj.scale.set(obj.scale.x * (1 + 0.5 * x), obj.scale.y * (1 - x), obj.scale.z * (1 + 0.5 * x));
      }
    }
    // HUD numbers / markers / callouts
    this.ui.frame(dt, (key) => {
      const p = this.posOf(key);
      if (!p) return null;
      return gr.project(_v.set(p[0], p[1] + 1.1, p[2]));
    });
    this.debugAcc += dt;
    if (this.ui.debugOn && this.debugAcc > 0.1) { this.debugAcc = 0; this.ui.setDebug(this.debugText()); }
    this.frameCpu += performance.now() - t0;
    this.cpu.push(this.frameCpu);
    if (this.cpu.length > 2000) this.cpu.splice(0, 1000);
    this.frameCpu = 0;
  }

  // ------------------------------------------------------------------------------------------ settings / debug

  /** Adds Screen shake / Master / SFX sliders to the menu's settings block (same markup as the others). */
  installMenu(settingsEl: Element | null, save: () => void) {
    if (!settingsEl) return;
    const rows: [keyof Settings, string][] = [['shake', 'Screen shake'], ['masterVolume', 'Master volume'], ['sfxVolume', 'SFX volume']];
    for (const [k, label] of rows) {
      const l = document.createElement('label');
      l.innerHTML = `${label} <input type="range" min="0" max="100" step="5" name="jz-${k}"><output></output>`;
      const inp = l.querySelector('input')!, out = l.querySelector('output')!;
      const sync = () => { const v = Math.round((this.host.settings[k] as number) * 100); inp.value = String(v); out.value = `${v}%`; };
      sync();
      inp.addEventListener('input', () => { (this.host.settings as any)[k] = Number(inp.value) / 100; this.applySettings(); sync(); save(); if (k !== 'shake') this.sfx.play('confirm'); else this.trauma.add(0.35); });
      inp.addEventListener('click', (ev) => ev.stopPropagation());
      settingsEl.appendChild(l);
    }
  }

  stats() {
    const s = [...this.cpu].sort((a, b) => a - b);
    const q = (p: number) => (s.length ? s[Math.min(s.length - 1, Math.floor(s.length * p))] : 0);
    return {
      trauma: this.trauma.value, shake: this.lastShake, fov: this.lastFov, hitstopActive: this.hitstop.active(), hitstop: { triggered: this.hitstop.triggered, refused: this.hitstop.refused, totalHoldMs: this.hitstop.totalHoldMs },
      counts: { ...this.counts }, stat: { ...this.stat }, ui: { ...this.ui.shown, lastCallout: this.ui.lastCallout, liveNumbers: this.ui.liveNumbers },
      sfx: { ...this.sfx.stats(), counts: { ...this.sfx.counts }, silentPlays: this.sfx.silentPlays },
      cpu: { p50: q(0.5), p95: q(0.95), max: s[s.length - 1] ?? 0, n: s.length },
    };
  }

  resetStats() { this.cpu.length = 0; }

  /** Detach / re-attach the render hooks (A/B perf measurement; events and sound stay subscribed). */
  setEnabled(on: boolean) {
    const gr = this.host.gr;
    gr.juice = on ? this : null;
    gr.constructs.poseHook = on ? (v) => this.onConstructPose(v) : null;
  }

  private debugText() {
    const st = this.stats();
    const top = Object.entries(st.counts).sort((a, b) => b[1] - a[1]).slice(0, 6).map(([k, v]) => `${k} ${v}`).join('  ');
    return [
      'JUICE  (F4)',
      `trauma   ${st.trauma.toFixed(2)}  shake ${st.shake.toFixed(3)}  (setting ${Math.round(this.shakeSetting * 100)}%)`,
      `hitstop  ${st.hitstopActive.length ? st.hitstopActive.join(' ') : '—'}  fx×${this.fxTimeScale.toFixed(2)}`,
      `         trig ${st.hitstop.triggered}  refused ${st.hitstop.refused}  held ${st.hitstop.totalHoldMs} ms`,
      `fov      ${st.fov >= 0 ? '+' : ''}${st.fov.toFixed(1)}°  kill cam ${this.killCam ? this.killCam.t.toFixed(2) + ' s' : '—'}`,
      `voices   ${st.sfx.active}/${st.sfx.max}  peak ${st.sfx.peak}  stolen ${st.sfx.stolen}  dropped ${st.sfx.dropped}  thr ${st.sfx.throttled}`,
      `audio    ${st.sfx.state}  loops ${st.sfx.loops}  played ${st.sfx.played}`,
      `effects  ${top || '—'}`,
      `cpu      p50 ${st.cpu.p50.toFixed(3)} ms  p95 ${st.cpu.p95.toFixed(3)} ms`,
    ].join('\n');
  }
}
