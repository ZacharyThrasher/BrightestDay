//! CONNECT juice — the never-built presentation half of DESIGN "Weight & power fantasy (pass 2)"
//! (owner: "GETTING hit feels good, now we just have to make connecting … more rewarding / dopamine
//! inducing"). Presentation only: nothing here touches the sim tick. Every number lives in the
//! hot-reloaded `assets/tuning/juice.ron` ([`JuiceTuning`]; a unit test checks it equals the Rust
//! defaults).
//!
//! * **Tiers**: every `Hit` is Light / Medium / Heavy / Finisher ([`tier`]: heavy swings, big
//!   strength, a launch fast enough to KO). Everything below scales by tier.
//! * **Asymmetric hit-stop**: the victim's and the attacker's visuals hold (`HitFreeze`) for the
//!   tier's time; the attacker unfreezes earlier ("cuts through").
//! * **Victim hit-shake + white flash** during the freeze (`HitFreeze::shake_offset` / `flash_of`,
//!   applied by `present::players`).
//! * **Contact resistance** ([`ContactBite`]): the first-person swing curve drags through the body
//!   for a few frames, then snaps through (repays the lost time); a short kick on the hands.
//! * **Attacker camera punch + nudge-zoom** along the hit (juice.rs channels, comfort-gated: the
//!   Comfort preset has shake and FOV kicks at 0 → none).
//! * **Directional sparks**: hard-light streaks flung along the hit vector, count / speed by tier.
//! * **Layered audio**: transient + body (impact.rs) + a sub thump on heavies; heavy connects you're
//!   in duck the world (the `heavy_hit` mix duck).
//! * **Finisher**: a launch that KOs — longest hit-stop, a white flash, and a brief presentation
//!   slow-mo (`Time<Virtual>`: effects, debris, words; never the sim) for everyone near.
//! * **Lunge** (owner: "I don't even feel the lunge"): FOV punch out + speed lines + whoosh + the
//!   viewmodel thrusts forward for the local dash; everyone else sees a hard-light dash streak.

use crate::present::impact::HitFreeze;
use crate::present::sound::Sfx3d;
use crate::present::{bv, corps_rgb, PresentSet};
use crate::debris::{SpawnDebris, VfxRng};
use crate::input::LookAngles;
use crate::juice::{CameraShakeEvent, FovPunch, ScreenFlash};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::PlayerId;
use bd_fx::{FxParticles, PFlags};
use bd_sim::combat::HitKind;
use bd_sim::hook::AssistEvent;
use bd_sim::GameEvent;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Hit tier (index into the `[light, medium, heavy, finisher]` arrays of [`JuiceTuning`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Light = 0,
    Medium = 1,
    Heavy = 2,
    Finisher = 3,
}

/// Every connect-juice number (`assets/tuning/juice.ron`). Arrays are `(light, medium, heavy,
/// finisher)`. Never shown to players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JuiceTuning {
    /// Tiering: impact strength (sim `ImpactFx::strength`) at or above which a hit is Medium…
    pub medium_strength: f32,
    /// …Heavy (a released heavy swing is always Heavy)…
    pub heavy_strength: f32,
    /// …and a knockback (m/s) at or above which it is a Finisher (a launch that KOs).
    pub finisher_kb: f32,
    /// Victim hit-stop (s) per tier.
    pub hitstop: [f32; 4],
    /// Attacker hit-stop = victim's × this (the attacker "cuts through" first).
    pub attacker_frac: f32,
    /// Victim hit-shake amplitude (m) along the hit vector during the freeze, and its frequency (Hz).
    pub victim_shake: [f32; 4],
    pub victim_shake_hz: f32,
    /// Victim white flash (0..1 of the body flash) and how long it fades (s).
    pub victim_flash: [f32; 4],
    pub victim_flash_secs: f32,
    /// Contact resistance: the first-person swing drags at `bite_rate` × speed for this long (s)…
    pub bite_secs: [f32; 4],
    pub bite_rate: f32,
    /// …then plays at `snap_rate` × until the lost time is repaid (the snap through)…
    pub snap_rate: f32,
    /// …and the hands kick back this much (viewmodel spring velocity).
    pub hands_kick: [f32; 4],
    /// Attacker camera punch (shake trauma along the hit) and nudge-zoom (FOV degrees, negative = in).
    pub punch_trauma: [f32; 4],
    pub nudge_zoom: [f32; 4],
    /// Directional sparks: count, speed (m/s), cone half-angle (deg), life (s), size (m), glow.
    pub sparks: [f32; 4],
    pub spark_speed: [f32; 4],
    pub spark_cone_deg: f32,
    pub spark_life: f32,
    pub spark_size: f32,
    pub spark_emissive: f32,
    /// Only within this distance of the camera (m).
    pub spark_max_dist: f32,
    /// Audio: a sub thump layered under Heavy+ hits (gain, pitch), a crisp transient over every hit
    /// you're in (gain, pitch), and the `heavy_hit` world duck on Heavy+ hits you're in.
    pub sub_gain: f32,
    pub sub_pitch: f32,
    pub transient_gain: f32,
    pub transient_pitch: f32,
    pub duck_heavy: bool,
    /// Onomatopoeia size × per tier.
    pub word_scale: [f32; 4],
    /// Finisher: presentation slow-mo (s real time) at this speed for hits within `slowmo_radius` m
    /// of the camera (or involving you); flash intensity / duration.
    pub slowmo_secs: f32,
    pub slowmo_speed: f32,
    pub slowmo_radius: f32,
    pub finisher_flash: f32,
    pub finisher_flash_secs: f32,
    /// Lunge (local dash): FOV punch out (deg), camera trauma, viewmodel thrust (spring velocity).
    pub lunge_fov: f32,
    pub lunge_trauma: f32,
    pub lunge_thrust: f32,
    /// Speed lines while dashing: count, length (m), distance ahead (m), ring radius (m), glow.
    pub lunge_lines: u32,
    pub lunge_line_len: f32,
    pub lunge_line_ahead: f32,
    pub lunge_line_radius: f32,
    pub lunge_line_emissive: f32,
    /// Everyone else's lunge: a hard-light streak from where they started, fading over this long (s).
    pub dash_streak_secs: f32,
    pub dash_streak_thick: f32,
    pub dash_streak_emissive: f32,
}

impl Default for JuiceTuning {
    fn default() -> Self {
        Self {
            medium_strength: 0.45,
            heavy_strength: 0.9,
            finisher_kb: 42.0,
            hitstop: [0.04, 0.07, 0.11, 0.16],
            attacker_frac: 0.7,
            victim_shake: [0.02, 0.03, 0.05, 0.06],
            victim_shake_hz: 38.0,
            victim_flash: [0.35, 0.55, 0.85, 1.0],
            victim_flash_secs: 0.14,
            bite_secs: [0.03, 0.05, 0.08, 0.1],
            bite_rate: 0.12,
            snap_rate: 2.2,
            hands_kick: [0.5, 0.9, 1.5, 2.0],
            punch_trauma: [0.05, 0.1, 0.2, 0.28],
            nudge_zoom: [-0.5, -1.2, -2.8, -4.0],
            sparks: [6.0, 12.0, 22.0, 34.0],
            spark_speed: [14.0, 20.0, 28.0, 34.0],
            spark_cone_deg: 24.0,
            spark_life: 0.22,
            spark_size: 0.07,
            spark_emissive: 9.0,
            spark_max_dist: 70.0,
            sub_gain: 0.85,
            sub_pitch: 0.55,
            transient_gain: 0.45,
            transient_pitch: 1.7,
            duck_heavy: true,
            word_scale: [1.0, 1.0, 1.35, 1.7],
            slowmo_secs: 0.25,
            slowmo_speed: 0.35,
            slowmo_radius: 30.0,
            finisher_flash: 0.35,
            finisher_flash_secs: 0.12,
            lunge_fov: 6.0,
            lunge_trauma: 0.08,
            lunge_thrust: 3.2,
            lunge_lines: 18,
            lunge_line_len: 1.6,
            lunge_line_ahead: 4.0,
            lunge_line_radius: 1.4,
            lunge_line_emissive: 3.0,
            dash_streak_secs: 0.3,
            dash_streak_thick: 0.07,
            dash_streak_emissive: 4.0,
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct JuiceTuningRes(pub JuiceTuning);

/// Tier of a hit (`kind`, sim impact `strength` / `heavy`, final `knockback`, `lethal`).
pub fn tier(t: &JuiceTuning, kind: HitKind, strength: f32, heavy: bool, knockback: f32, lethal: bool) -> Tier {
    if lethal || knockback >= t.finisher_kb {
        Tier::Finisher
    } else if kind == HitKind::HeavyMelee || strength >= t.heavy_strength {
        Tier::Heavy
    } else if heavy || strength >= t.medium_strength {
        Tier::Medium
    } else {
        Tier::Light
    }
}

/// (victim, attacker) hit-stop seconds for a tier.
pub fn hitstop_secs(t: &JuiceTuning, tier: Tier) -> (f32, f32) {
    let v = t.hitstop[tier as usize].clamp(0.0, 0.3);
    (v, v * t.attacker_frac.clamp(0.0, 1.0))
}

/// First-person contact resistance (read by the viewmodel's swing curve): play rate this frame and
/// a pending hand kick.
#[derive(Resource, Debug, Clone)]
pub struct ContactBite {
    /// Swing-curve playback rate for this frame (1 = normal).
    pub rate: f32,
    /// Pending viewmodel kick (spring velocity; the viewmodel takes it).
    pub kick: f32,
    /// Pending forward thrust (lunge), taken by the viewmodel.
    pub thrust: f32,
    bite_left: f32,
    debt: f32,
}

impl Default for ContactBite {
    fn default() -> Self {
        Self { rate: 1.0, kick: 0.0, thrust: 0.0, bite_left: 0.0, debt: 0.0 }
    }
}

impl ContactBite {
    fn start(&mut self, secs: f32, kick: f32) {
        self.bite_left = self.bite_left.max(secs.max(0.0));
        self.kick += kick;
    }

    /// Advance one frame of `dt` real seconds: the drag, then the snap through.
    pub fn step(&mut self, dt: f32, bite_rate: f32, snap_rate: f32) {
        let bite_rate = bite_rate.clamp(0.0, 1.0);
        if self.bite_left > 0.0 {
            self.rate = bite_rate;
            self.debt += (1.0 - bite_rate) * dt;
            self.bite_left = (self.bite_left - dt).max(0.0);
        } else if self.debt > 0.0 {
            let extra = (snap_rate.max(1.0) - 1.0) * dt;
            let pay = extra.min(self.debt);
            self.debt -= pay;
            self.rate = 1.0 + pay / dt.max(1e-4);
        } else {
            self.rate = 1.0;
        }
    }
}

/// Presentation-only slow-mo + dash streak state.
#[derive(Resource, Default)]
struct ConnectState {
    slowmo: f32,
    slowed: bool,
    /// Local dash in progress: seconds left (speed lines).
    dash: f32,
    /// Remote dash streaks: (player, start point, age).
    streaks: Vec<(PlayerId, Vec3, f32, [f32; 3])>,
}

fn tuning_path() -> PathBuf {
    crate::assets_dir().join("tuning").join("juice.ron")
}

fn load_tuning() -> JuiceTuning {
    match std::fs::read_to_string(tuning_path()) {
        Ok(text) => ron::from_str::<JuiceTuning>(&text).unwrap_or_else(|e| {
            eprintln!("juice tuning: failed to parse {}: {e}; using defaults", tuning_path().display());
            JuiceTuning::default()
        }),
        Err(_) => JuiceTuning::default(),
    }
}

fn hot_reload(time: Res<Time<Real>>, mut tuning: ResMut<JuiceTuningRes>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(tuning_path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(tuning_path()) {
                match ron::from_str::<JuiceTuning>(&text) {
                    Ok(t) => {
                        info!("juice tuning reloaded");
                        tuning.0 = t;
                    }
                    Err(e) => warn!("juice tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

pub struct ConnectPlugin;

impl Plugin for ConnectPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(JuiceTuningRes(load_tuning()))
            .init_resource::<ContactBite>()
            .init_resource::<ConnectState>()
            .add_message::<SpawnDebris>()
            .add_message::<Sfx3d>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<ScreenFlash>()
            .add_message::<crate::present::signature::SignatureCue>()
            .add_systems(Update, hot_reload)
            .add_systems(Update, connect_hits.in_set(PresentSet::Events))
            .add_systems(Update, (step_bite, slowmo, dash_fx).in_set(PresentSet::Sync));
        scenario::build(app);
    }
}

#[allow(clippy::too_many_arguments)]
fn connect_hits(
    active: Option<Res<ActiveWorld>>,
    tuning: Res<JuiceTuningRes>,
    comfort: Res<ComfortSettings>,
    views: Res<WorldViews>,
    anchor: Res<crate::camera::ViewAnchor>,
    mut events: MessageReader<GameEvents>,
    mut freeze: ResMut<HitFreeze>,
    mut bite: ResMut<ContactBite>,
    mut st: ResMut<ConnectState>,
    mut rng: ResMut<VfxRng>,
    out: (MessageWriter<SpawnDebris>, MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>, MessageWriter<ScreenFlash>, MessageWriter<crate::present::signature::SignatureCue>),
) {
    let (mut debris, mut sfx, mut shake, mut fov, mut flash, mut sig) = out;
    let Some(active) = active else {
        events.clear();
        return;
    };
    let t = &tuning.0;
    let local = active.local;
    let eye = anchor.eye;
    for ev in events.read() {
        match &ev.event {
            GameEvent::Hit { target, source, construct, kind, knockback, point, lethal, impact: fx, .. } => {
                let tr = tier(t, *kind, fx.strength, fx.heavy, *knockback, *lethal);
                let k = tr as usize;
                let p = bv(*point);
                let dir = bv(fx.dir).normalize_or_zero();
                // Asymmetric hit-stop: victim holds the tier's time, the attacker cuts through first.
                let (vs, as_) = hitstop_secs(t, tr);
                freeze.hold_player(*target, vs);
                freeze.set_shake(*target, dir, t.victim_shake[k], t.victim_shake_hz, vs);
                freeze.set_flash(*target, t.victim_flash[k], vs + t.victim_flash_secs);
                if let Some(s) = source {
                    freeze.hold_player(*s, as_);
                }
                if let Some(c) = construct {
                    freeze.hold_construct(*c, as_);
                }
                let local_attacker = source.is_some() && *source == local;
                let local_victim = local == Some(*target);
                let melee = matches!(kind, HitKind::Melee | HitKind::HeavyMelee);
                if local_attacker {
                    // Contact resistance (first-person swing drags, then snaps through) + hand kick.
                    if melee {
                        bite.start(t.bite_secs[k], t.hands_kick[k]);
                    }
                    // Camera punch along the hit + nudge-zoom toward it (comfort-gated in juice.rs).
                    shake.write(CameraShakeEvent { trauma: t.punch_trauma[k], dir });
                    fov.write(FovPunch { degrees: t.nudge_zoom[k] });
                    // Crisp transient over the hit-confirm tick.
                    sfx.write(Sfx3d::ui("hit_light").pitch(t.transient_pitch).gain(t.transient_gain).exempt());
                }
                if (local_attacker || local_victim) && tr >= Tier::Heavy {
                    sfx.write(Sfx3d::ui("land_heavy").pitch(t.sub_pitch).gain(t.sub_gain).exempt());
                    if t.duck_heavy {
                        sig.write(crate::present::signature::SignatureCue {
                            cue: crate::present::signature::Cue::WorldDuck,
                            pos: Vec3::ZERO,
                            severity: 0.0,
                            mine: true,
                            element: bd_core::mechanics::Element::Light,
                            construct: bd_core::ConstructId(0),
                        });
                    }
                } else if tr >= Tier::Heavy && eye.distance(p) < 40.0 {
                    sfx.write(Sfx3d::at("land_heavy", p).pitch(t.sub_pitch).gain(t.sub_gain * 0.7));
                }
                // Directional sparks: hard-light streaks flung along the hit vector.
                if eye.distance(p) < t.spark_max_dist && dir != Vec3::ZERO {
                    let col = source.and_then(|s| views.player(s)).map_or([1.0; 3], |v| corps_rgb(v.corps));
                    let hot = [0.75 + col[0] * 0.25, 0.75 + col[1] * 0.25, 0.75 + col[2] * 0.25];
                    let r = &mut rng.0;
                    let n = t.sparks[k].clamp(0.0, 80.0) as usize;
                    let cone = t.spark_cone_deg.to_radians();
                    let (a, b) = dir.any_orthonormal_pair();
                    for _ in 0..n {
                        let th = r.f32() * std::f32::consts::TAU;
                        let off = r.f32().sqrt() * cone;
                        let d = (dir * off.cos() + (a * th.cos() + b * th.sin()) * off.sin()).normalize_or(dir);
                        debris.write(SpawnDebris {
                            pos: p + d * 0.15,
                            vel: d * t.spark_speed[k] * r.range(0.6, 1.2),
                            color: hot,
                            size: t.spark_size * r.range(0.6, 1.3),
                            emissive: t.spark_emissive,
                            lifetime: t.spark_life * r.range(0.6, 1.2),
                        });
                    }
                }
                // Finisher: flash + presentation slow-mo for everyone near.
                if tr == Tier::Finisher && (local_attacker || local_victim || eye.distance(p) < t.slowmo_radius) {
                    flash.write(ScreenFlash { color: Color::WHITE, intensity: t.finisher_flash, duration: t.finisher_flash_secs, vignette: false });
                    if !comfort.is_comfort_preset() && comfort.shake > 0.0 {
                        st.slowmo = st.slowmo.max(t.slowmo_secs);
                    }
                    if local_attacker {
                        sfx.write(Sfx3d::ui("final_blow_suck").gain(0.8).exempt());
                    }
                }
            }
            GameEvent::Assist { player, kind: AssistEvent::Lunge, pos, .. } => {
                if Some(*player) == local {
                    // The dash must read as a dash: FOV punch out, a small kick, the arm thrusts.
                    fov.write(FovPunch { degrees: t.lunge_fov });
                    shake.write(CameraShakeEvent { trauma: t.lunge_trauma, dir: Vec3::ZERO });
                    bite.thrust += t.lunge_thrust;
                    st.dash = 0.22;
                } else {
                    let col = views.player(*player).map_or([1.0; 3], |v| corps_rgb(v.corps));
                    st.streaks.retain(|s| s.0 != *player);
                    st.streaks.push((*player, bv(*pos), 0.0, col));
                }
            }
            _ => {}
        }
    }
}

fn step_bite(time: Res<Time<Real>>, tuning: Res<JuiceTuningRes>, mut bite: ResMut<ContactBite>) {
    let t = &tuning.0;
    bite.step(time.delta_secs().min(0.05), t.bite_rate, t.snap_rate);
}

/// Finisher slow-mo: presentation time (`Time<Virtual>`: effects, debris, words) only.
fn slowmo(time: Res<Time<Real>>, tuning: Res<JuiceTuningRes>, mut st: ResMut<ConnectState>, mut virt: ResMut<Time<Virtual>>) {
    let dt = time.delta_secs().min(0.1);
    if st.slowmo > 0.0 {
        st.slowmo -= dt;
        // Ease back in over the last third.
        let tail = (st.slowmo / (tuning.0.slowmo_secs * 0.33).max(1e-3)).clamp(0.0, 1.0);
        let s = tuning.0.slowmo_speed.clamp(0.05, 1.0);
        virt.set_relative_speed(1.0 - (1.0 - s) * tail);
        st.slowed = true;
    } else if st.slowed {
        virt.set_relative_speed(1.0);
        st.slowed = false;
    }
}

/// Speed lines while the local lunge dashes; hard-light dash streaks behind everyone else's.
fn dash_fx(
    time: Res<Time<Real>>,
    tuning: Res<JuiceTuningRes>,
    comfort: Res<ComfortSettings>,
    anchor: Res<crate::camera::ViewAnchor>,
    look: Res<LookAngles>,
    views: Res<WorldViews>,
    mut st: ResMut<ConnectState>,
    mut fx: ResMut<FxParticles>,
) {
    let t = &tuning.0;
    let dt = time.delta_secs().min(0.1);
    if st.dash > 0.0 {
        st.dash -= dt;
        // Comfort: no speed lines (vection).
        if comfort.fov_kick > 0.0 && t.lunge_lines > 0 {
            let fwd = look.forward();
            let rot = look.rotation();
            let k = (st.dash / 0.22).clamp(0.0, 1.0);
            let n = t.lunge_lines.min(64);
            for i in 0..n {
                // Fixed ring around the view axis, streaming past the camera.
                let a = i as f32 / n as f32 * std::f32::consts::TAU + (i as f32 * 2.399);
                let r = t.lunge_line_radius * (0.8 + 0.4 * ((i * 7 % 5) as f32 / 5.0));
                let off = rot * Vec3::new(a.cos() * r, a.sin() * r * 0.7, 0.0);
                let phase = ((1.0 - k) * 2.5 + i as f32 * 0.37).fract();
                let at = anchor.eye + fwd * (t.lunge_line_ahead * (1.0 - phase)) + off;
                fx.immediate_segment(at, at - fwd * t.lunge_line_len * k, 0.012, [1.0, 1.0, 1.0], t.lunge_line_emissive * k, PFlags::PLAIN);
            }
        }
    }
    let secs = t.dash_streak_secs.max(0.01);
    st.streaks.retain_mut(|s| {
        s.2 += dt;
        s.2 < secs
    });
    for (id, from, age, col) in &st.streaks {
        let Some(v) = views.player(*id) else { continue };
        let to = v.pos + Vec3::Y * 1.0 * v.size;
        let k = 1.0 - age / secs;
        fx.immediate_segment(*from, to, t.dash_streak_thick * k, *col, t.dash_streak_emissive * k, PFlags::PLAIN);
        fx.immediate_segment(*from + Vec3::Y * 0.5, to + Vec3::Y * 0.4, t.dash_streak_thick * 0.5 * k, [1.0; 3], t.dash_streak_emissive * k, PFlags::PLAIN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn juice_ron_matches_the_rust_defaults() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/juice.ron");
        let text = std::fs::read_to_string(path).expect("assets/tuning/juice.ron");
        assert_eq!(ron::from_str::<JuiceTuning>(&text).expect("parses"), JuiceTuning::default());
    }

    #[test]
    fn tiers_and_asymmetric_hitstop() {
        let t = JuiceTuning::default();
        assert_eq!(tier(&t, HitKind::Melee, 0.2, false, 8.0, false), Tier::Light);
        assert_eq!(tier(&t, HitKind::Shot, 0.6, false, 12.0, false), Tier::Medium);
        assert_eq!(tier(&t, HitKind::HeavyMelee, 0.5, true, 20.0, false), Tier::Heavy);
        assert_eq!(tier(&t, HitKind::Melee, 0.5, false, 50.0, false), Tier::Finisher);
        let mut prev = 0.0;
        for tr in [Tier::Light, Tier::Medium, Tier::Heavy, Tier::Finisher] {
            let (v, a) = hitstop_secs(&t, tr);
            assert!(v > prev && a < v, "{tr:?}: tiered, attacker shorter");
            prev = v;
        }
        let (v, _) = hitstop_secs(&t, Tier::Light);
        assert!((0.03..=0.05).contains(&v));
    }

    #[test]
    fn bite_drags_then_snaps_through() {
        let t = JuiceTuning::default();
        let mut b = ContactBite::default();
        b.start(0.08, 1.0);
        let dt = 1.0 / 144.0;
        let mut played = 0.0;
        let mut frames = 0;
        let mut min_rate: f32 = 1.0;
        let mut max_rate: f32 = 1.0;
        for _ in 0..144 {
            b.step(dt, t.bite_rate, t.snap_rate);
            played += b.rate * dt;
            frames += 1;
            min_rate = min_rate.min(b.rate);
            max_rate = max_rate.max(b.rate);
        }
        assert!(min_rate < 0.2 && max_rate > 1.5, "drag then snap: {min_rate} {max_rate}");
        assert!((played - frames as f32 * dt).abs() < 1e-3, "the swing ends where it would have: {played}");
        assert_eq!(b.rate, 1.0);
    }
}

// ------------------------------------------------------------------------------------ scenario

/// `--practice --bots 1 --scenario connect [--connect-shot heavy|sword|lunge]`: stages one connect
/// in front of the camera, repeating every 2.5 s, for screenshots / clips of the juice (hit-stop,
/// contact drag, sparks, sub, flash) from the attacker's view.
pub mod scenario {
    use super::*;
    use crate::input::InputOverride;
    use bd_core::input::buttons;
    use bd_core::mechanics::{Element, Form};

    #[derive(Resource, Clone, Debug)]
    pub struct ConnectScenario {
        pub shot: String,
        t: f32,
        cycle_t: f32,
        placed: bool,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "connect") {
            return;
        }
        let shot = args.windows(2).find(|w| w[0] == "--connect-shot").map(|w| w[1].clone()).unwrap_or_else(|| "heavy".into());
        info!("connect scenario: {shot}");
        app.insert_resource(ConnectScenario { shot, t: 0.0, cycle_t: 0.0, placed: false })
            .add_systems(Update, script.run_if(resource_exists::<ActiveWorld>));
    }

    fn ground(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> Option<bd_core::Vec3> {
        let from = bd_core::Vec3::new(x, near_y + 3.0, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 6.0)?;
        Some(bd_core::Vec3::new(x, hit.point.y + 0.05, z))
    }

    fn script(time: Res<Time<Real>>, mut sc: ResMut<ConnectScenario>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>, mut ov: ResMut<InputOverride>) {
        let dt = time.delta_secs();
        sc.t += dt;
        if sc.t < 0.8 {
            return;
        }
        let Some(me) = active.local_player().filter(|p| p.alive).map(|p| (p.id, p.mv.pos)) else { return };
        let (me_id, me_pos) = me;
        let lunge = sc.shot == "lunge";
        let form = if sc.shot == "sword" || lunge { Form::Sword } else { Form::Hammer };
        let dist = if lunge { 5.6 } else { 2.3 };
        // (Re)stage each cycle: the bot stands `dist` m ahead at 60 % (knockback reads, no KO).
        if !sc.placed || sc.cycle_t > 2.5 {
            sc.cycle_t = 0.0;
            active.bots.clear();
            active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
            let yaw = look.yaw;
            let d = bd_core::input::look_dir(yaw, 0.0);
            let w = &active.world;
            let spot = ground(w, me_pos.x + d.x * dist, me_pos.z + d.z * dist, me_pos.y);
            let others: Vec<PlayerId> = w.players.iter().map(|p| p.id).filter(|id| *id != me_id).collect();
            for (k, id) in others.iter().enumerate() {
                if let Some(p) = active.world.player_mut(*id) {
                    if k == 0 {
                        if let Some(s) = spot {
                            p.mv.pos = s;
                        }
                        p.mv.vel = bd_core::Vec3::ZERO;
                        p.mv.tumble = 0.0;
                        p.spawn_shield = 0.0;
                        p.damage_pct = 60.0;
                        p.alive = true;
                    } else {
                        p.alive = false;
                        p.respawn_timer = 1e6;
                    }
                }
            }
            if let Some(p) = active.world.player_mut(me_id) {
                p.mv.vel = bd_core::Vec3::ZERO;
                p.damage_pct = 0.0;
            }
            if !sc.placed || active.world.constructs.held_by(me_id).is_none_or(|c| c.recipe.form != form) {
                let p = active.world.player(me_id).unwrap();
                let (eye, lk) = (p.eye(active.world.params()), p.look());
                let scale = bd_sim::constructs::form_spec(form).default_scale;
                let _ = active.world.cast(me_id, bd_sim::bots::recipe("weapon", form, &[], Element::Light, scale), None, bd_sim::Aim::new(eye, lk));
            }
            sc.placed = true;
        }
        sc.cycle_t += dt;
        // Aim at the bot's chest; swing: heavy = hold 0.55 s then release, sword / lunge = a tap.
        if let Some(b) = active.world.players.iter().find(|p| p.id != me_id && p.alive) {
            let eye = active.world.player(me_id).unwrap().eye(active.world.params());
            let d = (b.hull(active.world.params()).center() - eye).normalize_or_zero();
            look.yaw = (-d.x).atan2(-d.z);
            look.pitch = d.y.clamp(-1.0, 1.0).asin();
        }
        ov.active = true;
        ov.move_x = 0;
        ov.move_z = 0;
        ov.held = 0;
        let c = sc.cycle_t;
        let heavy = sc.shot == "heavy";
        if heavy && (0.6..1.15).contains(&c) || !heavy && (0.6..0.64).contains(&c) {
            ov.held |= buttons::PRIMARY;
        }
    }
}
