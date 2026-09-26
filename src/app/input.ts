import { MouseFilter, radPerCount, requestRawPointerLock, DEFAULT_SENS } from './mouse.js';
import { NO_INPUT, type PlayerInput } from '../sim/player.js';

export type Action =
  | 'console' | 'dismiss' | 'dismiss_all' | 'help' | 'perf' | 'reset' | 'fullscreen' | `scene_${number}` | `quick_${number}`;

const MOVE_KEYS = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD', 'Space', 'ControlLeft', 'ControlRight', 'KeyC', 'ShiftLeft', 'ShiftRight', 'KeyQ', 'KeyE', 'KeyR', 'KeyV', 'KeyB']);

/**
 * Keyboard + mouse → PlayerInput, sampled once per fixed sim tick. Presses shorter than a tick are
 * latched so they are never lost. Mouse look accumulates at display rate (smooth on 144 Hz).
 */
export class Input {
  yaw = 0;
  pitch = 0;
  sensitivity = DEFAULT_SENS; // Source/Quake units
  invertY = false;
  /** When false (console open / menu), movement keys are ignored. */
  gameplay = true;
  /**
   * Holds carried through a typing session: a tether you were holding stays attached and a slide keeps
   * sliding while you type (momentum is never killed by opening the console). Cleared when it closes.
   */
  private carry = { hookL: false, hookR: false, crouch: false };
  private held = new Set<string>();
  private latched = new Set<string>();
  private flyToggle = false;
  private use = false; // G: get in / out of the nearest rideable construct (edge-triggered)
  private reelUntil = 0;
  private mouse = [false, false, false];
  private mouseLatch = [false, false, false];
  /**
   * Mouse mapping. Default: LMB = construct primary, RMB = secondary, wheel = cycle the equipped construct,
   * MMB = reel; tethers live on Q/E. `mouseTethers` restores the classic mapping (LMB/RMB tethers, wheel reels)
   * and moves construct actions to V (primary) / B (secondary) / Tab (cycle).
   */
  mouseTethers = false;
  private cycleAcc = 0;
  private lastCycle = 0;
  onAction: (a: Action, e?: KeyboardEvent) => void = () => {};
  locked = false;
  /** 'raw' = unadjustedMovement (no OS acceleration), 'plain' = fallback */
  lockMode: 'raw' | 'plain' | 'failed' | 'none' = 'none';
  readonly mouseFilter = new MouseFilter();

  constructor(private target: HTMLElement) {
    window.addEventListener('keydown', (e) => this.keyDown(e));
    window.addEventListener('keyup', (e) => this.keyUp(e));
    window.addEventListener('blur', () => { this.held.clear(); this.mouse = [false, false, false]; });
    document.addEventListener('mousemove', (e) => {
      if (!this.locked) return;
      // raw counts, spike-filtered (see mouse.ts); applied immediately, no smoothing
      const [dx, dy] = this.mouseFilter.filter(e.movementX, e.movementY, e.timeStamp || performance.now());
      const k = radPerCount(this.sensitivity);
      this.yaw -= dx * k;
      this.pitch -= dy * k * (this.invertY ? -1 : 1);
      this.pitch = Math.max(-1.55, Math.min(1.55, this.pitch));
    });
    target.addEventListener('mousedown', (e) => {
      if (!this.locked) return;
      if (e.button < 3) { this.mouse[e.button] = true; this.mouseLatch[e.button] = true; }
      e.preventDefault();
    });
    window.addEventListener('mouseup', (e) => { if (e.button < 3) this.mouse[e.button] = false; });
    target.addEventListener('contextmenu', (e) => e.preventDefault());
    window.addEventListener('wheel', (e) => {
      if (!this.locked || e.deltaY === 0) return;
      if (this.mouseTethers) { if (this.gameplay) this.reelUntil = performance.now() + 180; return; }
      // one step per notch; trackpads fire many small events, so rate-limit them
      const now = performance.now();
      if (now - this.lastCycle < 90) return;
      this.lastCycle = now;
      this.cycleAcc += Math.sign(e.deltaY);
    }, { passive: true });
    document.addEventListener('pointerlockchange', () => {
      this.locked = document.pointerLockElement === this.target;
      if (this.locked) this.mouseFilter.onLock(performance.now());
      else this.mouse = [false, false, false];
    });
  }

  requestLock() {
    // raw, unaccelerated input where supported (Windows/ChromeOS); plain lock otherwise
    void requestRawPointerLock(this.target).then((mode) => { this.lockMode = mode; });
  }

  private keyDown(e: KeyboardEvent) {
    const c = e.code;
    const inField = (e.target as HTMLElement)?.tagName === 'INPUT';
    if (c === 'F1') { e.preventDefault(); this.onAction('help', e); return; }
    if (c === 'F3') { e.preventDefault(); this.onAction('perf', e); return; }
    if (c === 'F5') { e.preventDefault(); this.onAction('reset', e); return; }
    if (c === 'F11') { this.onAction('fullscreen', e); return; }
    if (inField || !this.gameplay) return;
    if (c === 'Enter' || c === 'NumpadEnter') { e.preventDefault(); this.onAction('console', e); return; }
    if (e.altKey && /^Digit[1-9]$/.test(c)) { e.preventDefault(); if (!e.repeat) this.onAction(`scene_${Number(c.slice(5))}`, e); return; }
    if (c === 'Backspace') { e.preventDefault(); this.onAction('reset', e); return; }
    if (c === 'Tab') { e.preventDefault(); if (this.mouseTethers && !e.repeat) this.cycleAcc += 1; }
    if (MOVE_KEYS.has(c)) {
      e.preventDefault();
      this.held.add(c);
      this.latched.add(c);
    }
    if (e.repeat) return;
    if (c === 'KeyF') this.flyToggle = true;
    else if (c === 'KeyG') this.use = true;
    else if (c === 'KeyX') this.onAction('dismiss', e);
    else if (c === 'KeyZ') this.onAction('dismiss_all', e);
    else if (/^(Digit|Numpad)[1-9]$/.test(c)) this.onAction(`quick_${Number(c.slice(-1))}`, e);
  }

  private keyUp(e: KeyboardEvent) {
    this.held.delete(e.code);
  }

  /** Drop all held state. */
  release() { this.held.clear(); this.latched.clear(); this.mouse = [false, false, false]; this.mouseLatch = [false, false, false]; }

  /**
   * Console opened: movement keys stop, but what keeps your momentum carries on — held tethers stay
   * attached and (if `sliding`) the slide continues. Mouse look and mouse-button tethers stay live.
   */
  startTyping(sliding: boolean) {
    this.carry = { hookL: this.down('KeyQ'), hookR: this.down('KeyE'), crouch: sliding && (this.down('ControlLeft') || this.down('ControlRight') || this.down('KeyC')) };
    this.gameplay = false;
    this.latched.clear();
  }

  /** Console closed: keys typed into it never reach movement; physically held keys resume. */
  stopTyping() {
    this.carry = { hookL: false, hookR: false, crouch: false };
    this.gameplay = true;
    this.latched.clear();
  }

  private down(code: string) { return this.held.has(code) || this.latched.has(code); }

  /** One tick's worth of input. */
  sample(): PlayerInput {
    const g = this.gameplay;
    const d = (c: string) => g && this.down(c);
    const k = this.carry;
    const crouch = d('ControlLeft') || d('ControlRight') || d('KeyC') || (!g && k.crouch);
    const mt = this.mouseTethers;
    const m0 = this.mouse[0] || this.mouseLatch[0], m1 = this.mouse[1] || this.mouseLatch[1], m2 = this.mouse[2] || this.mouseLatch[2];
    const inp: PlayerInput = {
      ...NO_INPUT,
      moveX: (d('KeyD') ? 1 : 0) - (d('KeyA') ? 1 : 0),
      moveZ: (d('KeyW') ? 1 : 0) - (d('KeyS') ? 1 : 0),
      yaw: this.yaw,
      pitch: this.pitch,
      jump: d('Space'),
      crouch,
      descend: crouch,
      boost: d('ShiftLeft') || d('ShiftRight'),
      // mouse buttons only register while the pointer is locked, so they stay live while typing
      hookL: d('KeyQ') || (!g && k.hookL) || (mt && m0),
      hookR: d('KeyE') || (!g && k.hookR) || (mt && m2),
      reel: d('KeyR') || (mt && g && performance.now() < this.reelUntil) || m1,
      // construct actions (the equipped construct's LMB / RMB; wheel cycles it) — sim inputs, see sim/constructs/actions.ts
      primary: mt ? d('KeyV') : m0,
      secondary: mt ? d('KeyB') : m2,
      cycle: this.cycleAcc,
      flyToggle: g && this.flyToggle,
      use: g && this.use,
    };
    this.flyToggle = false;
    this.use = false;
    this.cycleAcc = 0;
    this.latched.clear();
    this.mouseLatch = [false, false, false];
    return inp;
  }
}
