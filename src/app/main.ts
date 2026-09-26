import { Vector3 } from 'three';
import { World } from '../sim/world.js';
import { buildScene, aimAt, type SceneName } from '../sim/levels.js';
import { Director } from '../lang/director.js';
import type { CommitResult } from '../lang/types.js';
import { defaultSpec, type ConstructSpec } from '../sim/constructs/spec.js';
import { buildForm } from '../sim/constructs/forms.js';
import { NO_INPUT, type PlayerInput } from '../sim/player.js';
import { GameRenderer } from '../render/renderer.js';
import { Input, type Action } from './input.js';
import { HUD, SCENE_KEYS } from './hud.js';
import { loadSettings, saveSettings, type Settings } from './settings.js';
import { IpcProvider } from '../lang/ipc-provider.js';
import { loadSemantic } from '../lang/semantic.js';
import type { Embedder } from '../lang/interpreter.js';

const DT = 1 / 60;
const MAX_STEPS = 6;

/** Public surface for the voice module (and anything else that wants to drive the ring). */
export interface HardlightApp {
  setTranscript(partialText: string, isFinal: boolean): void;
  commit(text: string): Promise<CommitResult | null>;
  preview(text: string): void;
  readonly world: World;
  readonly director: Director;
}

class App implements HardlightApp {
  world!: World;
  director!: Director;
  scene: SceneName = 'playground';
  readonly gr: GameRenderer;
  readonly input: Input;
  readonly hud: HUD;
  settings: Settings;
  private consoleOpen = false;
  private history: string[] = [];
  private histIdx = -1;
  private acc = 0;
  private last = performance.now();
  private simMs = 0;
  private fps = 60;
  private frameMs = 16.7;
  /** Test hook: when true the real-time loop does not advance the sim (scripted stepping only). */
  manual = false;
  private loading = false;
  private frameWaiters: (() => void)[] = [];
  private voiceActive = false;
  private commitsInFlight = 0;
  /** Optional language layers, attached to every new Director (System 1.5 word vectors, System 2 LLM). */
  private embedder: Embedder | null = null;
  private s2: { provider: IpcProvider; mode: 'hybrid' | 'always' } | null = null;
  frameTimes: number[] = [];
  simTimes: number[] = [];
  renderCpu: number[] = [];
  hudTimes: number[] = [];

  constructor(readonly canvas: HTMLCanvasElement, readonly overlay: HTMLElement) {
    this.settings = loadSettings();
    this.gr = new GameRenderer(canvas, { quality: this.settings.graphics, fov: this.settings.fov, constructColor: this.settings.constructColor });
    document.documentElement.style.setProperty('--hl', this.settings.constructColor);
    this.input = new Input(canvas);
    this.input.sensitivity = this.settings.mouseSensitivity;
    this.input.invertY = this.settings.invertY;
    this.hud = new HUD(overlay);
    this.input.onAction = (a, e) => this.onAction(a, e);
    this.setupConsole();
    this.setupMenu();
    window.addEventListener('resize', () => this.gr.resize());
    canvas.addEventListener('click', () => { if (!this.input.locked && !this.consoleOpen) this.input.requestLock(); });
    document.addEventListener('pointerlockchange', () => this.updateMenu());
    // never fail silently: if the OS/browser refuses mouse capture, say so
    document.addEventListener('pointerlockerror', () => this.hud.toast('warn', 'Couldn’t capture the mouse — click again (after Esc, Windows needs a second before re-capturing)'));
    // voice bridge (filled in by the voice integrator; absent in the web build)
    const v = window.hardlight?.voice;
    v?.onPartial?.((t) => this.setTranscript(t, false));
    v?.onFinal?.((t) => this.setTranscript(t, true));
  }

  async loadScene(name: SceneName) {
    if (this.loading) return;
    this.loading = true;
    try {
      const w = await World.create();
      const info = buildScene(w, name);
      const old = this.world;
      this.world = w;
      this.scene = name;
      this.director = new Director(w);
      this.configureDirector();
      this.gr.setWorld(w);
      this.input.yaw = info.yaw;
      this.input.pitch = -0.08;
      w.player.yaw = info.yaw;
      w.player.pitch = this.input.pitch;
      w.listeners.push((e) => this.hud.onEvent(w, e));
      this.hud.reset(w, name);
      this.gr.setPreview([]);
      this.acc = 0;
      // free the previous Rapier world (WASM memory is not garbage collected) unless a commit is in flight
      if (old && !this.commitsInFlight) { try { old.eq.free(); old.pw.free(); } catch { /* already freed */ } }
    } finally {
      this.loading = false;
    }
  }

  private configureDirector() {
    const d = this.director;
    if (!d) return;
    d.interp.embedder = this.embedder;
    if (this.s2) { d.interp.s2 = this.s2.provider; d.interp.s2Mode = this.s2.mode; }
    // System 2 disagreed after System 1 already acted: show it, never silently swap
    d.onCorrection((r) => {
      this.hud.toast('info', `The ring reconsidered → ${r.summary || r.text}`, 6500);
      this.hud.showResult(r);
    });
  }

  /** Load the optional layers in the background; the game is playable before (and without) them. */
  async initLanguageLayers() {
    const b = window.hardlight;
    try {
      const bytes = await b?.semantic?.();
      if (bytes && bytes.byteLength) {
        const idx = await loadSemantic(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer);
        if (idx) { this.embedder = idx; console.log('[hardlight] word vectors loaded'); }
      }
    } catch (err) { console.warn('[hardlight] word vectors unavailable', err); }
    try {
      const info = await b?.s2?.info();
      const mode = b?.s2?.mode?.() ?? 'hybrid';
      if (b?.s2 && info?.enabled && mode !== 'off') {
        this.s2 = { provider: new IpcProvider(b.s2 as any), mode };
        console.log(`[hardlight] System 2 on (${info.name}, ${mode})`);
        this.hud.toast('good', `System 2 interpreter on (${info.name})`, 4000);
      }
    } catch (err) { console.warn('[hardlight] System 2 unavailable', err); }
    this.configureDirectorLayers();
  }

  private configureDirectorLayers() {
    if (!this.director) return;
    this.director.interp.embedder = this.embedder;
    if (this.s2) { this.director.interp.s2 = this.s2.provider; this.director.interp.s2Mode = this.s2.mode; }
  }

  // ------------------------------------------------------------------------------------ loop

  start() {
    const frame = (now: number) => {
      const dt = Math.max(0, (now - this.last) / 1000);
      this.last = now;
      this.frameMs = this.frameMs * 0.9 + dt * 1000 * 0.1;
      this.fps = 1000 / Math.max(1, this.frameMs);
      this.frameTimes.push(dt * 1000);
      if (this.frameTimes.length > 4000) this.frameTimes.splice(0, 2000);
      if (this.world && !this.loading) {
        if (!this.manual) {
          this.acc += Math.min(dt, 0.25);
          let steps = 0;
          while (this.acc >= DT && steps < MAX_STEPS) { this.stepSim(this.input.sample()); this.acc -= DT; steps++; }
          if (steps === MAX_STEPS) this.acc = 0; // too slow: drop time instead of spiralling
        }
        const alpha = this.manual ? 1 : this.acc / DT;
        this.gr.render(alpha, dt, { yaw: this.input.yaw, pitch: this.input.pitch });
        this.renderCpu.push(this.gr.stats.frameCpuMs);
        if (this.renderCpu.length > 4000) this.renderCpu.splice(0, 2000);
        const th = performance.now();
        this.hud.update({ world: this.world, renderer: this.gr, fps: this.fps, frameMs: this.frameMs, simMs: this.simMs, consoleOpen: this.consoleOpen, locked: this.input.locked });
        this.hudTimes.push(performance.now() - th);
        if (this.hudTimes.length > 4000) this.hudTimes.splice(0, 2000);
      }
      const waiters = this.frameWaiters;
      this.frameWaiters = [];
      waiters.forEach((f) => f());
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }

  stepSim(inp: PlayerInput) {
    const t0 = performance.now();
    this.world.step(inp);
    const ms = performance.now() - t0;
    this.simMs = this.simMs * 0.95 + ms * 0.05;
    this.simTimes.push(ms);
    if (this.simTimes.length > 4000) this.simTimes.splice(0, 2000);
    this.gr.tick();
  }

  // ------------------------------------------------------------------------------------ actions

  private onAction(a: Action, _e?: KeyboardEvent) {
    const w = this.world;
    if (!w) return;
    if (a === 'console') this.openConsole();
    else if (a === 'dismiss') { const id = w.constructs.focusId; if (id) w.constructs.dismiss(id); }
    else if (a === 'dismiss_all') w.constructs.dismissAll();
    else if (a === 'help') this.hud.toggleHelp();
    else if (a === 'perf') this.hud.togglePerf();
    else if (a === 'reset') void this.loadScene(this.scene);
    else if (a === 'fullscreen') { if (window.hardlight?.toggleFullscreen) window.hardlight.toggleFullscreen(); }
    else if (a === 'voice_down') {
      const v = window.hardlight?.voice;
      this.voiceActive = true;
      this.hud.setListening(true);
      if (v?.start) v.start(); else this.hud.toast('note', 'Voice is not connected in this build — press Enter to type instead.', 3500);
    } else if (a === 'voice_up') {
      if (!this.voiceActive) return;
      this.voiceActive = false;
      this.hud.setListening(false);
      window.hardlight?.voice?.stop?.();
    } else if (a.startsWith('scene_')) {
      const i = Number(a.slice(6)) - 1;
      const name = SCENE_KEYS[i];
      if (name) void this.loadScene(name as SceneName);
    }
  }

  // ------------------------------------------------------------------------------------ console / ring

  private setupConsole() {
    const inp = this.hud.consoleInput;
    inp.addEventListener('input', () => this.preview(inp.value));
    inp.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.code === 'Enter' || e.code === 'NumpadEnter') {
        e.preventDefault();
        const text = inp.value.trim();
        this.closeConsole();
        if (text) { this.history.push(text); void this.commit(text); }
      } else if (e.code === 'Escape') {
        e.preventDefault();
        this.closeConsole();
      } else if (e.code === 'ArrowUp' || e.code === 'ArrowDown') {
        e.preventDefault();
        if (!this.history.length) return;
        if (this.histIdx < 0) this.histIdx = this.history.length;
        this.histIdx = Math.max(0, Math.min(this.history.length, this.histIdx + (e.code === 'ArrowUp' ? -1 : 1)));
        inp.value = this.history[this.histIdx] ?? '';
        this.preview(inp.value);
      } else if (e.code === 'F1' || e.code === 'F3') {
        e.preventDefault();
        if (e.code === 'F1') this.hud.toggleHelp(); else this.hud.togglePerf();
      }
    });
  }

  openConsole() {
    if (this.consoleOpen) return;
    this.consoleOpen = true;
    this.histIdx = -1;
    this.input.gameplay = false;
    this.input.release();
    this.hud.setConsole(true);
    this.hud.consoleInput.value = '';
    this.hud.consoleInput.focus();
  }

  closeConsole() {
    this.consoleOpen = false;
    this.input.gameplay = true;
    this.hud.setConsole(false);
    this.hud.consoleInput.blur();
    this.gr.setPreview([]);
  }

  preview(text: string) {
    if (!this.director) return;
    try {
      const t = text.trim();
      this.gr.setPreview(t ? this.director.preview(t).specs : []);
    } catch (err) {
      console.warn('preview failed', err);
      this.gr.setPreview([]);
    }
  }

  async commit(text: string): Promise<CommitResult | null> {
    if (!this.director) return null;
    this.hud.setInterpreting(true);
    this.gr.setPreview([]);
    this.commitsInFlight++;
    try {
      const r = await this.director.commit(text);
      this.hud.showResult(r);
      return r;
    } catch (err: any) {
      console.error('commit failed', err);
      this.hud.flashStatus('not_understood');
      this.hud.toast('bad', `Ring error: ${err?.message ?? err}`);
      return null;
    } finally {
      this.commitsInFlight--;
      this.hud.setInterpreting(this.commitsInFlight > 0);
    }
  }

  /** Voice module entry point: partials drive the ghost preview, finals commit. */
  setTranscript(partialText: string, isFinal: boolean) {
    const t = (partialText ?? '').trim();
    this.hud.setTranscript(t, isFinal);
    if (isFinal) {
      this.hud.setListening(false);
      this.gr.setPreview([]);
      if (t) { this.history.push(t); void this.commit(t); }
      setTimeout(() => this.hud.setTranscript('', false), 2500);
    } else {
      this.hud.setListening(true);
      this.preview(t);
    }
  }

  // ------------------------------------------------------------------------------------ menu

  private setupMenu() {
    const m = this.hud.menu;
    m.innerHTML = `
      <div class="menu-card">
        <div class="logo">HARD<span>LIGHT</span></div>
        <div class="tag">imagine it · the ring makes it real</div>
        <button class="play">Click to play</button>
        <div class="scenes"></div>
        <div class="settings">
          <label>Mouse sensitivity <input type="range" min="0.2" max="4" step="0.05" name="sens"><output></output></label>
          <label>Field of view <input type="range" min="60" max="110" step="1" name="fov"><output></output></label>
          <label>Invert Y <input type="checkbox" name="inv"></label>
          <label>Construct colour <input type="color" name="color"></label>
          <label>Graphics <select name="gfx"><option value="high">high</option><option value="low">low</option></select></label>
        </div>
        <div class="foot">F1 controls · settings.json next to the app overrides defaults</div>
      </div>`;
    const sc = m.querySelector('.scenes')!;
    SCENE_KEYS.forEach((s, i) => {
      const b = document.createElement('button');
      b.textContent = `${i + 1} · ${s}`;
      b.onclick = (e) => { e.stopPropagation(); void this.loadScene(s as SceneName).then(() => this.input.requestLock()); };
      sc.appendChild(b);
    });
    const q = <T extends Element>(s: string) => m.querySelector(s) as unknown as T;
    const sens = q<HTMLInputElement>('input[name=sens]'), fov = q<HTMLInputElement>('input[name=fov]');
    const inv = q<HTMLInputElement>('input[name=inv]'), color = q<HTMLInputElement>('input[name=color]'), gfx = q<HTMLSelectElement>('select[name=gfx]');
    const sync = () => {
      sens.value = String(this.settings.mouseSensitivity); (sens.nextElementSibling as HTMLOutputElement).value = this.settings.mouseSensitivity.toFixed(2);
      fov.value = String(this.settings.fov); (fov.nextElementSibling as HTMLOutputElement).value = String(this.settings.fov);
      inv.checked = this.settings.invertY; color.value = this.settings.constructColor; gfx.value = this.settings.graphics;
    };
    sync();
    const apply = () => {
      this.settings.mouseSensitivity = Number(sens.value);
      this.settings.fov = Number(fov.value);
      this.settings.invertY = inv.checked;
      this.settings.constructColor = color.value;
      const gfxChanged = gfx.value !== this.settings.graphics;
      this.settings.graphics = gfx.value as any;
      this.input.sensitivity = this.settings.mouseSensitivity;
      this.input.invertY = this.settings.invertY;
      this.gr.setFov(this.settings.fov);
      this.gr.setConstructColor(this.settings.constructColor);
      document.documentElement.style.setProperty('--hl', this.settings.constructColor);
      saveSettings(this.settings);
      sync();
      if (gfxChanged) this.hud.toast('note', 'Graphics quality applies after restart (or reload).');
    };
    for (const e of [sens, fov, inv, color, gfx]) { e.addEventListener('input', apply); e.addEventListener('click', (ev) => ev.stopPropagation()); }
    m.querySelector('.settings')!.addEventListener('click', (ev) => ev.stopPropagation());
    m.addEventListener('click', () => this.input.requestLock());
    this.updateMenu();
  }

  private updateMenu() {
    const show = !this.input.locked && !this.consoleOpen && !this.manual;
    this.hud.menu.classList.toggle('hidden', !show);
  }

  setManual(on: boolean) { this.manual = on; this.updateMenu(); }

  nextFrame(): Promise<void> { return new Promise((r) => this.frameWaiters.push(r)); }
}

// ------------------------------------------------------------------------------------------------
// Boot
// ------------------------------------------------------------------------------------------------

async function boot() {
  const canvas = document.getElementById('view') as HTMLCanvasElement;
  const overlay = document.getElementById('overlay') as HTMLElement;
  const app = new App(canvas, overlay);
  const params = new URLSearchParams(location.search);
  const test = params.has('test');
  if (test) app.setManual(true);
  const scene = (params.get('scene') as SceneName) || 'playground';
  await app.loadScene(scene);
  app.start();
  void app.initLanguageLayers();
  (window as any).hardlightApp = app as HardlightApp;
  installTestHooks(app);
  document.body.classList.add('ready');
  window.hardlight?.ready?.();
  console.log(`[hardlight] renderer ready (graphics=${app.settings.graphics}, scene=${scene}, fov=${app.settings.fov})`);
}

/** Deterministic scripting surface for tools/screenshot.mjs and tools/perf.mjs. */
function installTestHooks(app: App) {
  const hooks: any = {
    app,
    get world() { return app.world; },
    manual(on: boolean) { app.setManual(on); },
    /** Step the sim n ticks with the given (partial) input; look defaults to the current view. */
    step(n: number, inp: Partial<PlayerInput> = {}) {
      for (let i = 0; i < n; i++) {
        const full: PlayerInput = { ...NO_INPUT, yaw: app.input.yaw, pitch: app.input.pitch, ...inp };
        app.input.yaw = full.yaw; app.input.pitch = full.pitch;
        app.stepSim(full);
        if (inp.flyToggle) inp = { ...inp, flyToggle: false };
      }
    },
    look(yaw: number, pitch: number) { app.input.yaw = yaw; app.input.pitch = pitch; app.world.player.yaw = yaw; app.world.player.pitch = pitch; },
    aimAt(p: [number, number, number]) { const r = aimAt(app.world, new Vector3(...p)); app.input.yaw = r.yaw; app.input.pitch = r.pitch; return r; },
    teleport(p: [number, number, number], v: [number, number, number] = [0, 0, 0]) { app.world.player.teleport(new Vector3(...p), new Vector3(...v)); app.gr.tick(); app.gr.tick(); },
    async scene(name: SceneName) { await app.loadScene(name); },
    preview(text: string) { app.openConsole(); app.hud.consoleInput.value = text; app.preview(text); },
    closeConsole() { app.closeConsole(); },
    async commit(text: string) { const r = await app.commit(text); return r ? { applied: r.applied, summary: r.summary, clamps: r.clamps, abstain: r.abstain, source: r.source } : null; },
    spawnForm(form: string, p: Partial<ConstructSpec> = {}) {
      const r = app.world.constructs.spawn(defaultSpec({ concept: form, label: form, parts: buildForm(form), ...p }));
      return r.construct?.id ?? r.report.rejected;
    },
    fire(n: number, speed = 20) {
      const P = app.world.player.pos;
      for (let i = 0; i < n; i++) {
        const a = (i / n) * Math.PI * 2;
        const from = new Vector3(P.x + Math.cos(a) * 60, P.y + 5 + (i % 5) * 3, P.z + Math.sin(a) * 60);
        const v = P.clone().sub(from).normalize().multiplyScalar(speed);
        app.world.spawnProjectile(from, v, null);
      }
    },
    frames(n = 1) { let p = Promise.resolve(); for (let i = 0; i < n; i++) p = p.then(() => app.nextFrame()); return p; },
    stats() { return { ...app.gr.stats, simMsAvg: app.simTimes.slice(-120).reduce((a, b) => a + b, 0) / Math.max(1, Math.min(120, app.simTimes.length)) }; },
    resetTimings() { app.frameTimes.length = 0; app.simTimes.length = 0; app.renderCpu.length = 0; app.hudTimes.length = 0; },
    timings() { return { frame: app.frameTimes.slice(), sim: app.simTimes.slice(), render: app.renderCpu.slice(), hud: app.hudTimes.slice() }; },
    /**
     * Benchmark loop inside the page: each iteration steps `ticksPerFrame` sim ticks with input from
     * `inputFn(i)` and waits for one rendered frame. Returns per-frame samples.
     */
    async bench(n: number, ticksPerFrame: number, inputSrc: string) {
      const inputFn = new Function('i', 'w', inputSrc) as (i: number, w: World) => Partial<PlayerInput>;
      const out: { sim: number; render: number; update: number; hud: number; frame: number; calls: number; tris: number; total: number }[] = [];
      let last = performance.now();
      for (let i = 0; i < n; i++) {
        let sim = 0;
        for (let k = 0; k < ticksPerFrame; k++) {
          const inp = inputFn(i, app.world) ?? {};
          const t0 = performance.now();
          hooks.step(1, inp);
          sim += performance.now() - t0;
        }
        await app.nextFrame();
        const now = performance.now();
        const st = app.gr.stats;
        out.push({ sim, render: st.frameCpuMs, update: st.updateMs, hud: app.hudTimes[app.hudTimes.length - 1] ?? 0, frame: now - last, calls: st.sceneCalls, tris: st.sceneTriangles, total: st.totalCalls });
        last = now;
      }
      return out;
    },
    hud: app.hud,
    toggleHelp(on?: boolean) { app.hud.toggleHelp(on); },
    togglePerf() { app.hud.togglePerf(); },
  };
  (window as any).__hl = hooks;
}

boot().catch((err) => {
  console.error('[hardlight] boot failed', err);
  const d = document.createElement('pre');
  d.className = 'fatal';
  d.textContent = `HARDLIGHT failed to start:\n${err?.stack ?? err}`;
  document.body.appendChild(d);
  window.hardlight?.fatal?.(String(err?.message ?? err));
});
