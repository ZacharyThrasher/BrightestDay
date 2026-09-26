/**
 * JUICE — every game-feel number in one place (presentation only; nothing here touches the sim).
 *
 * Units: seconds unless the key ends in `Ms`; distances in metres; angles in degrees; "energy" is a
 * normalised 0..1(+) impact size computed by `energyOf()` in core.ts from damage / dv / radius.
 *
 * Tuning notes (why these numbers):
 *  - Shake is trauma² (Squirrel Eiserloh, GDC 2016): small hits barely move the view, big hits are
 *    violent, and stacking hits saturates instead of growing without bound. Trauma decays linearly at
 *    1.4/s, so a max-trauma kill settles in ~0.7 s and a 0.3 hit is gone in ~0.2 s — short enough that
 *    aim never feels stolen in a duel. Max offset 0.28 m / 1.6° rot at trauma 1: readable in third
 *    person without clipping the camera into the rig (the boom is 5.2 m).
 *  - Hitstop 40–120 ms: < 40 ms is below perception at 60 fps (2 frames), > 120 ms starts to read as
 *    lag in an online game. Continuous fire can never lock an object: one chain is capped at 160 ms of
 *    freeze and then the target is immune for 250 ms; the whole-view freeze budget is 300 ms per second.
 *  - Catch-up 90 ms with smoothstep: the frozen object blends back onto its live (sim) pose, so the
 *    world is never out of sync for longer than hold + catch-up ≤ 210 ms and never teleports.
 *  - FOV punches are springs (ζ≈0.55) so they overshoot once and settle in ~0.35 s; they add to the
 *    renderer's existing speed FOV (capped +20°), and the sum of juice FOV is clamped to ±12°.
 */
export const JUICE = {
  /** settings defaults (0..1) */
  defaults: { shake: 0.7, master: 0.8, sfx: 0.9 },

  trauma: {
    max: 1,
    decay: 1.4, // trauma units per second (linear)
    exponent: 2, // shake = trauma^exponent
    offset: 0.28, // metres of camera translation at shake 1
    rot: 1.6, // degrees of pitch/yaw at shake 1
    roll: 2.4, // degrees of roll at shake 1
    freq: 18, // Hz of the shake noise
    /** distance falloff for world events (from the camera): full inside `near`, zero beyond `far` */
    near: 8,
    far: 70,
  },

  /** directional kick springs (camera position + rotation); effects specify PEAK metres / degrees */
  kick: {
    stiffness: 260, // spring k (1/s²)
    damping: 18, // c (1/s) → ζ ≈ 0.56
    rotStiffness: 320,
    rotDamping: 22,
    maxOffset: 0.6, // metres (clamped)
    maxRot: 6, // degrees (clamped)
  },

  hitstop: {
    minMs: 40,
    maxMs: 120,
    killMs: 120,
    koMs: 70, // a hostile drone / robot you disabled (PvE "kill")
    catchUpMs: 90,
    chainCapMs: 160, // longest continuous freeze of one target (repeated hits extend up to this)
    immuneMs: 250, // after a chain hits the cap, the target can't be frozen again for this long
    globalBudgetMs: 300, // whole-view freeze allowed per rolling second
    jitter: 0.05, // metres of hit-shake on the frozen object
    fxSlow: 0.15, // particle time scale during a whole-view freeze (sparks hang in the air)
  },

  fov: {
    springK: 170,
    springC: 14,
    clamp: 12, // ± degrees total from juice
    boostStart: 4,
    speedThresholds: [30, 45, 60] as number[], // m/s; crossing upward punches the FOV
    speedKick: 2.5, // + 1 per threshold index
    speedHysteresis: 4, // m/s below a threshold before it can fire again
    tetherYank: 2.2,
    landHardDip: -2.5,
    castPulse: -2.4, // push-in when your construct forms
    launch: 3,
    killZoom: -9, // degrees of slow zoom during the kill cam
  },

  camera: {
    landDip: 0.12, // metres per (fall speed / 20) — soft land
    landHardDip: 0.42,
    jumpLift: 0.04,
    tetherSnap: 0.25, // metres toward the anchor on attach
    recoil: { gun: 0.03, cannon: 0.2, missile: 0.14, bomb: 0.08, laser: 0.05, flame: 0.02, net: 0.06, melee: 0.1, default: 0.08 } as Record<string, number>, // peak metres
    recoilRot: 6, // degrees of pitch kick per metre of recoil (cannon → 1.2°)
    killCam: { dur: 0.75, look: 0.3, easeIn: 0.12 }, // look = max fraction of the rotation toward the victim
    hurtKick: 0.3, // peak metres at a max hit
  },

  /** event → trauma (before distance falloff) = base + perEnergy × energy */
  shake: {
    impact: { base: 0.08, perEnergy: 0.32 },
    smash: { base: 0.22, perEnergy: 0.3 },
    explosion: { base: 0.3, perEnergy: 0.45 },
    hitReceived: { base: 0.22, perEnergy: 0.45 },
    kill: 0.55,
    killed: 0.85,
    ko: 0.28,
    constructDestroyed: 0.3,
    blocked: 0.08,
    landHard: 0.22,
    launch: 0.18,
    recoil: 0.06, // × recoil table value / 0.2 (cannon = 0.06)
    entityDestroyed: 0.12,
  },

  /** energy normalisation: energy = value / ref (clamped to 0..1.5) */
  energyRef: { damage: 40, dv: 30, radius: 6, fall: 30 },

  fx: {
    coreFlash: 1.3, // extra white core glow size per energy (reuses fx.glow)
    shockRing: 5, // extra fast ring end size per energy
    sparksPerEnergy: 16,
    flashOnHit: 0.9, // construct uFlash on being hit
    snap: { anticipation: -2.2, overshoot: 3.2, settle: 0.22 }, // uPulse values (× 6% part scale) around the form snap
    crumble: { implode: -2.5, dur: 0.1 },
    squash: { amount: 0.28, k: 420, c: 16 }, // entity squash-stretch spring on hit
  },

  ui: {
    damageNumber: { life: 0.85, rise: 46, popScale: 1.7, popMs: 110, mergeMs: 160, big: 30, max: 24 },
    hitMarker: { ms: 190, killMs: 420 },
    callout: { ms: 1500, popMs: 180 },
    combo: { ms: 2200 },
  },

  audio: {
    maxVoices: 24,
    /** minimum gap between two sounds of the same kind (ms) — merges machine-gun spam */
    throttleMs: { impact: 28, keyclick: 18, gun: 45, laser: 60, flame: 90, hit: 40, spark: 40, default: 22 } as Record<string, number>,
    /** voice priority (higher steals lower when the pool is full) */
    priority: { ui: 5, kill: 9, sting: 9, hurt: 8, explosion: 7, impact: 6, form: 6, weapon: 4, movement: 3, keyclick: 1, default: 3 } as Record<string, number>,
    pitchVar: 0.06, // ± fraction random pitch per play
    volVar: 0.12, // ± fraction random volume
    refDistance: 5,
    rolloff: 1.1,
    maxDistance: 250,
    boost: { gain: 0.32, cutoffBase: 380, cutoffPerSpeed: 26 },
    wind: { startSpeed: 14, fullSpeed: 60, gain: 0.26 },
    compressor: { threshold: -12, knee: 6, ratio: 12, attack: 0.003, release: 0.18 },
  },
};

export type JuiceTuning = typeof JUICE;
