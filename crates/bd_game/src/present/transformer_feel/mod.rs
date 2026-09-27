//! TRANSFORMER FEEL (client presentation + audio; owner: "If I'm playing ANY transformer, it should
//! FEEL awesome"). Everything is keyed by DATA — the archetype the host fights with (`combat.ron` →
//! `transformer.archetypes`: its `fx` flavour and dash kind), the body's size, the visual mode and
//! the sim's events — never by a character's name. No new network traffic: it reads the replicated
//! world, the interpolated views and `GameEvents`.
//!
//! | module | what |
//! |---|---|
//! | [`transform`] | THE TRANSFORM: a size-classed servo bed (zippy + high for suits, weighty at 6 m, deep + sub at 10 m+) under a click / clunk cascade synced to the panels actually locking, the final lock on the stand-up; combat transforms "snap"; local-pilot camera kick (FOV punch, shake, a roll twitch — comfort-gated); size-scaled spark showers; mid-air whoosh + streaks; autonomous transforms (`Transformed`) now actually play the sequence |
//! | [`weight`] | robot-mode footfalls from the biped robot rig (metal step + knee servo per step, ground shake by size and distance, dust), beast-alt footfalls, landing thuds |
//! | [`vehicle`] | engine voice per archetype (diesel, V8, bike, jet turbine, tank treads, rotor), pitched by speed; drift smoke + screech from the replicated velocity vs facing; drift-boost burst; boost trails |
//! | [`combat`] | per-archetype swing + metal hit layers, robot arm clips on string hits, the finisher crunch, transform-attack speed lines, perfect-dodge sting; BEAST / DINO: roars, chomps, tail whoosh, pounce, breath |
//! | [`scenario`] | `--scenario tffeel --tf-shot …` dev frames (followers transforming, walking robots, beasts) |
//!
//! Sounds: `tools/sfxgen/sounds_transformer_feel.py` (original synthesis; the transform family keeps
//! the tuned-click / servo / lock / hiss identity of `sounds_transform.py`).

pub mod combat;
pub mod scenario;
pub mod transform;
pub mod vehicle;
pub mod weight;

use super::constructs::SimConstruct;
use super::{bv, PresentSet};
use crate::sim_bridge::{ActiveWorld, WorldViews};
use bd_core::ConstructId;
use bd_core::mechanics::Mechanic;
use bd_fx::TransformFx;
use bd_sim::constructs::brawl::DashKind;
use bevy::prelude::*;

pub struct TransformerFeelPlugin;

impl Plugin for TransformerFeelPlugin {
    fn build(&self, app: &mut App) {
        // Messages we write (idempotent; startup-safe whatever the plugin order).
        app.add_message::<super::sound::Sfx3d>()
            .add_message::<bd_fx::BurstFx>()
            .add_message::<crate::juice::CameraShakeEvent>()
            .add_message::<crate::juice::FovPunch>()
            .init_resource::<transform::CamTwitch>()
            .add_systems(Update, track.after(PresentSet::Sync).before(TfSet))
            .configure_sets(Update, TfSet.after(PresentSet::Sync))
            .add_systems(Update, (transform::events, transform::cascade, weight::steps, vehicle::voice, vehicle::drift, combat::events, combat::layers).chain().in_set(TfSet))
            .add_systems(
                PostUpdate,
                transform::twitch_camera
                    .after(crate::camera::CameraUpdateSet)
                    .after(super::camera3p::place_camera)
                    .before(bevy::transform::TransformSystems::Propagate),
            );
        scenario::build(app);
        if std::env::var_os("BD_TF_LOG").is_some() {
            app.add_systems(PostUpdate, log_sounds);
        }
    }
}

/// `BD_TF_LOG=1`: log every world sound this pass plays (and the transform family) — headless checks.
fn log_sounds(time: Res<Time<Real>>, mut sfx: MessageReader<super::sound::Sfx3d>) {
    const OURS: [&str; 12] = ["tf_", "robot_", "engine_", "tyre_", "drift_boost", "beast_", "pounce_", "transform_", "kaiju_roar", "kaiju_step", "rotor_loop", "land_heavy"];
    for s in sfx.read() {
        if OURS.iter().any(|p| s.name.starts_with(p)) {
            info!("tf sound t={:.2} {} gain {:.2} pitch {:.2}", time.elapsed_secs(), s.name, s.gain, s.pitch);
        }
    }
}

/// Transformer-feel systems (after the construct sync, so poses / loops written there are final).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TfSet;

/// Engine voice family (from the archetype's `fx` flavour + name; new flavours fall back to `Car`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// Big diesel (trucks): the base `vehicle_engine_loop`.
    Diesel,
    Car,
    Bike,
    Jet,
    Rotor,
    Tread,
    /// Beast / dino alt: no engine — breath, growls, footfalls.
    Beast,
    Boat,
    Train,
}

impl Voice {
    pub fn of(name: &str, fx: &str) -> Voice {
        match fx {
            "afterburner" => Voice::Jet,
            "rotor_wash" => Voice::Rotor,
            "tread_dust" => Voice::Tread,
            "exhaust" => Voice::Diesel,
            "dino" | "beast" => Voice::Beast,
            "wake" => Voice::Boat,
            "freight" => Voice::Train,
            _ if name == "bike" => Voice::Bike,
            _ => Voice::Car,
        }
    }
    /// Rolls on wheels (tyre smoke + screech when it slides).
    pub fn wheels(self) -> bool {
        matches!(self, Voice::Car | Voice::Bike | Voice::Diesel)
    }
}

/// Per-transformer presentation state (on the construct entity, next to bd_fx's `TransformFx`).
#[derive(Component)]
pub struct TfFeel {
    pub id: ConstructId,
    pub voice: Voice,
    /// Heavy class (the archetype's dash is a shoulder step / planted shuffle): heavy metal punches.
    pub heavy: bool,
    /// Robot height in the world (m) — the size everything scales by.
    pub height: f32,
    /// Smoothed world velocity of the visual (m/s).
    pub vel: Vec3,
    /// Visual mode: robot (`TransformFx::to_robot`) and whether the sequence has settled.
    pub robot: bool,
    pub settled: bool,
    /// Sequence progress 0..1 (`TransformFx::t`).
    pub t: f32,
    /// The local player pilots it / wears it.
    pub local: bool,
    /// On the ground (the sim's flag, replicated).
    pub grounded: bool,
    /// Seconds since the entity appeared (first-frame guards).
    pub age: f32,
    prev_pos: Option<Vec3>,
    // --- transform
    pub(crate) last_t: f32,
    pub(crate) last_to_robot: bool,
    pub(crate) click_gap: f32,
    pub(crate) clicks_this_seq: u16,
    pub(crate) seq_speed: f32,
    /// Seconds since a `HeldFx::Transform` restarted the sequence (a `Transformed` right after it is the same transform).
    pub(crate) since_held: f32,
    /// Seconds the visual mode has disagreed with the replicated one.
    pub(crate) mismatch: f32,
    // --- weight
    pub(crate) prev_dist: Option<f32>,
    pub(crate) dist: f32,
    pub(crate) air_t: f32,
    pub(crate) fall: f32,
    pub(crate) step_gap: f32,
    // --- vehicle
    pub(crate) engine: Option<Entity>,
    pub(crate) screech: Option<Entity>,
    pub(crate) slip: f32,
    pub(crate) smoke_credit: f32,
    pub(crate) trail_credit: f32,
    // --- combat
    pub(crate) clip: Option<(bd_shapes::Attack, f32)>,
    pub(crate) burst_t: f32,
    pub(crate) swing_side: bool,
    /// Seconds until the beast-mode roar (a beast alt arriving).
    pub(crate) roar_in: Option<f32>,
    pub(crate) since_roar: f32,
}

impl TfFeel {
    fn new(id: ConstructId, voice: Voice, heavy: bool) -> Self {
        TfFeel {
            id,
            voice,
            heavy,
            height: 3.0,
            vel: Vec3::ZERO,
            robot: false,
            settled: true,
            t: 1.0,
            local: false,
            grounded: true,
            age: 0.0,
            prev_pos: None,
            last_t: 1.0,
            last_to_robot: false,
            click_gap: 0.0,
            clicks_this_seq: 0,
            seq_speed: 1.0,
            since_held: 99.0,
            mismatch: 0.0,
            prev_dist: None,
            dist: 0.0,
            air_t: 0.0,
            fall: 0.0,
            step_gap: 0.0,
            engine: None,
            screech: None,
            slip: 0.0,
            smoke_credit: 0.0,
            trail_credit: 0.0,
            clip: None,
            burst_t: 0.0,
            swing_side: false,
            roar_in: None,
            since_roar: 99.0,
        }
    }

    /// Size factor for pitch: small robots high and zippy, giants low (6 m = 1).
    pub fn pitch(&self) -> f32 {
        size_pitch(self.height)
    }

    /// 0 = suit-sized … 1 = 6 m … 2+ = kaiju (log-ish weight for gains / shake / particle counts).
    pub fn weight(&self) -> f32 {
        size_weight(self.height)
    }
}

/// Pitch multiplier for a body `h` m tall (2.6 m ≈ 1.4, 6 m = 1, 10 m ≈ 0.82, 20 m ≈ 0.62).
pub fn size_pitch(h: f32) -> f32 {
    let h = if h.is_finite() { h.max(0.3) } else { 6.0 };
    (6.0 / h).powf(0.4).clamp(0.55, 1.5)
}

/// Weight factor for a body `h` m tall (2.6 m ≈ 0.43, 6 m = 1, 10 m ≈ 1.4, 20 m ≈ 2.2, capped 3).
pub fn size_weight(h: f32) -> f32 {
    let h = if h.is_finite() { h.max(0.3) } else { 6.0 };
    (h / 6.0).powf(0.66).clamp(0.2, 3.0)
}

/// Size class of a body `h` m tall: 0 small (≤ 2.6 m suits), 1 mid, 2 large (≥ 9.5 m).
pub fn size_class(h: f32) -> usize {
    if h <= 2.6 {
        0
    } else if h < 9.5 {
        1
    } else {
        2
    }
}

/// Deterministic cheap jitter for particles (no shared RNG).
pub(crate) struct Jit(pub u32);

impl Jit {
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn dir(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }
}

/// Attach [`TfFeel`] to transformer constructs and keep the shared kinematics fresh.
#[allow(clippy::type_complexity)]
fn track(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    new: Query<(Entity, &SimConstruct), (With<TransformFx>, Without<TfFeel>)>,
    mut q: Query<(&SimConstruct, &TransformFx, &Transform, &mut TfFeel)>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    for (e, sc) in &new {
        let Some(c) = active.world.constructs.get(sc.id).filter(|c| c.recipe.has(Mechanic::Transforms)) else { continue };
        let a = active.world.cfg.tuning.transformer.archetype_of(&c.recipe);
        let heavy = matches!(a.dash, DashKind::Heavy | DashKind::Planted);
        commands.entity(e).try_insert(TfFeel::new(sc.id, Voice::of(&a.name, &a.fx), heavy));
    }
    let local = active.local;
    let seat = local.and_then(|id| views.player(id)).and_then(|p| p.seat).map(|(id, _)| id);
    for (sc, tfx, tf, mut f) in &mut q {
        f.age += dt;
        f.since_held += dt;
        f.height = (tfx.rig.height * tf.scale.y.abs()).clamp(0.3, 400.0);
        f.robot = tfx.to_robot;
        f.t = tfx.t;
        f.settled = tfx.t >= 1.0;
        let cv = views.construct(sc.id);
        let suit_local = cv.is_some_and(|v| v.attached()) && local == Some(sc.owner);
        f.local = seat == Some(sc.id) || suit_local;
        f.grounded = active.world.constructs.get(sc.id).is_none_or(|c| c.grounded || c.body.supported);
        let p = tf.translation;
        if let Some(pp) = f.prev_pos
            && dt > 1e-4
        {
            let v = (p - pp) / dt;
            if v.is_finite() && v.length() < 150.0 {
                let k = (dt * 14.0).min(1.0);
                f.vel = f.vel.lerp(v, k);
            }
        }
        f.prev_pos = Some(p);
    }
}

/// World position of the construct's centre (the sim's hull centre), else the visual's root.
pub(crate) fn centre_of(active: &ActiveWorld, id: ConstructId, fallback: Vec3) -> Vec3 {
    active.world.constructs.get(id).map_or(fallback, |c| bv(c.center()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sound this module plays exists in the generated manifest (missing cues are silent).
    #[test]
    fn transformer_feel_sounds_exist() {
        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/sfx/manifest.ron")).expect("manifest");
        let ids = [
            "tf_seq_robot_s", "tf_seq_robot_m", "tf_seq_robot_l", "tf_seq_alt_s", "tf_seq_alt_m", "tf_seq_alt_l", "tf_snap_robot", "tf_snap_alt",
            "tf_lock_s", "tf_lock_m", "tf_lock_l", "tf_panel_clunk", "tf_air_whoosh", "tf_eye_zing", "transform_panel_click", "robot_step_light",
            "robot_step_mid", "robot_footstep_heavy", "kaiju_step", "robot_servo_step", "robot_land", "land_heavy", "engine_car_loop",
            "engine_bike_loop", "engine_jet_loop", "engine_tank_loop", "rotor_loop", "tyre_screech_loop", "drift_boost_burst",
            "robot_swing_heavy", "robot_swing_fast", "robot_punch_heavy", "robot_punch_fast", "robot_finisher_crunch", "tf_focus_sting",
            "kaiju_roar", "beast_chomp", "beast_tail_whoosh", "pounce_whoosh",
        ];
        for id in ids {
            assert!(manifest.contains(&format!("\"{id}\": (")), "sound {id} missing from assets/sfx/manifest.ron");
        }
    }

    #[test]
    fn voices_and_sizes() {
        assert_eq!(Voice::of("jet", "afterburner"), Voice::Jet);
        assert_eq!(Voice::of("bike", "tyre_smoke"), Voice::Bike);
        assert_eq!(Voice::of("car", "tyre_smoke"), Voice::Car);
        assert_eq!(Voice::of("truck", "exhaust"), Voice::Diesel);
        assert_eq!(Voice::of("dino", "dino"), Voice::Beast);
        assert_eq!(Voice::of("new_row", "something_new"), Voice::Car);
        assert!(size_pitch(2.4) > 1.3 && (size_pitch(6.0) - 1.0).abs() < 1e-4 && size_pitch(20.0) < 0.7);
        assert_eq!((size_class(2.4), size_class(6.0), size_class(12.0)), (0, 1, 2));
        assert!(size_weight(f32::NAN).is_finite() && size_pitch(f32::NAN).is_finite());
    }
}
