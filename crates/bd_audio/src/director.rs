//! Interactive mix states: which snapshot the world is in, how strongly, and the one-off moves
//! (heavy-hit duck, KO negative space, final-blow "suck"). Game-thread only.
//!
//! Base layer (crossfaded): **Menu**, **Lobby**, **Exploration** ↔ **Combat** (combat *heat* from
//! recent damage dealt/taken and gunfire near the listener) ↔ **Space** (vacuum).
//! Overlays (weighted, on top): **Danger** (local damage % — muffled world, heartbeat whose rate
//! and level scale with %), **KO** (dead/respawning), **RingOpen** (typing a construct: the
//! world recedes, focus). Music intensity/tempo ride the same snapshots.

use glam::Vec3;

use crate::dsp::smoothstep;
use crate::engine::{AudioEngine, PlayOpts};
use crate::mix::{MixConfig, MixParams};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scene {
    #[default]
    Menu,
    Lobby,
    InGame,
}

/// What the director needs from the game each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixInputs {
    pub scene: Scene,
    /// Local player's Smash damage %.
    pub damage_pct: f32,
    /// False while KO'd / respawning.
    pub alive: bool,
    /// Ring console open (typing a construct) or voice-casting.
    pub ring_open: bool,
    /// 0..1: how far into vacuum the listener is (Orbit above the atmosphere).
    pub vacuum: f32,
    /// Final stock / match point / sudden death: music tempo lifts.
    pub sudden_death: bool,
    /// In-game pause overlay open.
    pub paused: bool,
}

impl Default for MixInputs {
    fn default() -> Self {
        Self { scene: Scene::Menu, damage_pct: 0.0, alive: true, ring_open: false, vacuum: 0.0, sudden_death: false, paused: false }
    }
}

/// Weights of every snapshot right now (for telemetry / tests).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SnapshotWeights {
    pub menu: f32,
    pub lobby: f32,
    pub combat: f32,
    pub space: f32,
    pub danger: f32,
    pub ko: f32,
    pub ring: f32,
    pub pause: f32,
}

pub struct MixDirector {
    heat: f32,
    w: SnapshotWeights,
    heartbeat_t: f32,
    last_sent: Option<MixParams>,
    send_timer: f32,
    pub current: MixParams,
}

impl Default for MixDirector {
    fn default() -> Self {
        Self::new()
    }
}

impl MixDirector {
    pub fn new() -> Self {
        Self {
            heat: 0.0,
            w: SnapshotWeights { menu: 1.0, ..Default::default() },
            heartbeat_t: 0.0,
            last_sent: None,
            send_timer: 0.0,
            current: MixParams::default(),
        }
    }

    pub fn weights(&self) -> SnapshotWeights {
        self.w
    }
    pub fn heat(&self) -> f32 {
        self.heat
    }

    /// Combat happened near the listener: `amount` ≈ 0.2 a shot nearby, 0.5 a hit, 1 a hit on/by you.
    pub fn combat(&mut self, amount: f32) {
        if amount.is_finite() {
            self.heat = (self.heat + amount.max(0.0) * 0.35).min(1.0);
        }
    }

    /// Combat near a point: scaled by distance to the listener.
    pub fn combat_at(&mut self, amount: f32, pos: Vec3, ear: Vec3) {
        let d = pos.distance(ear);
        self.combat(amount * (1.0 - d / 60.0).clamp(0.0, 1.0));
    }

    /// Heavy hit felt by the local player (dealt or taken): brief world + music duck.
    pub fn heavy_hit(&mut self, engine: &mut AudioEngine) {
        engine.duck_named("heavy_hit");
    }

    /// You KO'd someone: negative space so the streak note rings.
    pub fn ko_confirm(&mut self, engine: &mut AudioEngine) {
        engine.duck_named("ko_confirm");
    }

    /// Match-ending / KO blow: the world is sucked out for a beat (duck + low-pass sweep + inhale).
    pub fn final_blow(&mut self, engine: &mut AudioEngine) {
        engine.duck_named("final_blow");
        engine.play("final_blow_suck", PlayOpts::ui());
    }

    /// Per frame: fade weights, blend snapshots, send the mix (≤ 30 Hz, only when it moved),
    /// schedule the Danger heartbeat.
    pub fn update(&mut self, dt: f32, inp: &MixInputs, cfg: &MixConfig, engine: &mut AudioEngine) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        // Heat decays over ~4 s after the last exchange.
        self.heat = (self.heat - dt * 0.25).max(0.0);
        let snap = |name: &str| cfg.snapshots.get(name);
        let fade = |cur: f32, target: f32, name: &str| {
            let s = snap(name);
            let secs = if target > cur { s.map_or(0.3, |s| s.fade_in) } else { s.map_or(0.8, |s| s.fade_out) };
            if secs <= 1e-3 {
                target
            } else {
                let step = dt / secs;
                cur + (target - cur).clamp(-step, step)
            }
        };
        let in_game = inp.scene == Scene::InGame;
        let t_menu = if inp.scene == Scene::Menu { 1.0 } else { 0.0 };
        let t_lobby = if inp.scene == Scene::Lobby { 1.0 } else { 0.0 };
        let t_combat = if in_game { smoothstep(0.05, 0.6, self.heat) } else { 0.0 };
        let t_space = if in_game { inp.vacuum.clamp(0.0, 1.0) } else { 0.0 };
        // Danger from Smash %: starts to creep in at 90 %, full by 160 %.
        let t_danger = if in_game && inp.alive { smoothstep(90.0, 160.0, inp.damage_pct) } else { 0.0 };
        let t_ko = if in_game && !inp.alive { 1.0 } else { 0.0 };
        let t_ring = if in_game && inp.ring_open && inp.alive { 1.0 } else { 0.0 };
        let t_pause = if in_game && inp.paused { 1.0 } else { 0.0 };
        let w = &mut self.w;
        w.menu = fade(w.menu, t_menu, "Menu");
        w.lobby = fade(w.lobby, t_lobby, "Lobby");
        w.combat = fade(w.combat, t_combat, "Combat");
        w.space = fade(w.space, t_space, "Space");
        w.danger = fade(w.danger, t_danger, "Danger");
        w.ko = fade(w.ko, t_ko, "KO");
        w.ring = fade(w.ring, t_ring, "RingOpen");
        w.pause = fade(w.pause, t_pause, "Pause");
        let w = *w;

        let mut p = cfg.base_params();
        if let Some(s) = snap("Exploration") {
            p = p.blend(s, 1.0);
        }
        for (name, wt) in [("Combat", w.combat), ("Space", w.space), ("Lobby", w.lobby), ("Menu", w.menu)] {
            if let Some(s) = snap(name) {
                p = p.blend(s, wt);
            }
        }
        for (name, wt) in [("Danger", w.danger), ("KO", w.ko), ("RingOpen", w.ring), ("Pause", w.pause)] {
            if let Some(s) = snap(name) {
                p = p.blend(s, wt);
            }
        }
        if inp.sudden_death {
            p.music_tempo = p.music_tempo.max(1.12);
            p.music_intensity = p.music_intensity.max(0.9);
        }
        self.current = p;

        self.send_timer -= dt;
        let moved = self.last_sent.is_none_or(|l| l.distance(&p) > 0.05);
        if moved && self.send_timer <= 0.0 {
            engine.set_mix(p);
            self.last_sent = Some(p);
            self.send_timer = 1.0 / 30.0;
        }

        // Heartbeat: 70 → 150 bpm, louder with danger.
        if w.danger > 0.05 {
            self.heartbeat_t -= dt;
            if self.heartbeat_t <= 0.0 {
                let bpm = 70.0 + 80.0 * w.danger;
                self.heartbeat_t = 60.0 / bpm;
                let db = -12.0 + 12.0 * w.danger;
                engine.play("heartbeat", PlayOpts::ui().gain_db(db));
            }
        } else {
            self.heartbeat_t = 0.0;
        }
    }
}
