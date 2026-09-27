//! LEGENDARY presentation: the visuals of the generic legendary move rules (bd_sim
//! `constructs/legend.rs`, COMBAT.md §5l) — keyed off the same COMBINATIONS, never weapon names.
//! Everything is drawn with bd_fx immediate cubes / segments (hard light, no meshes to build) and a
//! few analytic particles, driven by `HeldFx` events + the replicated construct views, so host,
//! practice and clients look the same. Sounds and hit-stop / shake tiers are NOT here (the juice
//! pass owns them).
//!
//! | HeldFx | look |
//! |---|---|
//! | `Lodge` | dust + a glowing crack ring on the struck surface until it is recalled |
//! | `RecallStart` | a bright call line hand → item; the returning item draws a thick element streak |
//! | `Catch` (2 = snap) | a snap ring + glint in the hand |
//! | `StormRaise` | item held to the sky: element crackle around the head (Electric: arcs + sky bolts); 0 = a grey sputter |
//! | `StormMark` | a ground ring filling to the landing moment; rain drops visibly fall from the sky to land on it |
//! | `StormStrike` | per element: lightning column, meteor impact, icicle spears, tornado funnel, geyser column, crushing disc, rock spikes, shadow maw, nova shell, miasma |
//! | `StormArc` | jagged arcs body to body |
//! | `Ricochet` | spark burst + a glint star |
//! | `Parry` | a flash star on the blade + the reflected bolt's streak along the new heading |
//! | `Wave` | a simulated crescent (element colour, white-hot leading edge, short trail; Shadow = black with a red rim) |
//! | `LeapStart` / `LeapSlam` | launch dust ring / shockwave ring + glowing crater fissure along the heading |
//! | `Armor` | a silver shell flash around the wielder + the item flashes |
//! | `ChainSweep` / `ChainPull` | chained blades whirling a full circle with an element trail / a taut chain hand → victim |
//! | `OneShot` | gold muzzle burst; its `ShotTrace` leaves a gold contrail (the slug is gold in `guns`) |
//! | `NeedleStick` / `Supercombine` | pink crystal needles stuck in the victim (counted) / a pink burst |
//! | `GravGrab` / `GravPunt` | an orange tractor beam muzzle → held thing / a shockwave cone or punt flash |
//! | `StreamLatch` / `StreamRelease` | the proton stream (wavy, crackling) from the muzzle along the aim, bent onto the victim |
//! | `GunCharge` / `ChargeShot` / `BfgZap` | a growing orb at the muzzle / a release burst / green tendrils; the BFG orb is big and green |
//!
//! Idle flourishes (by combination): Electric held melee crackles, a Returning shield glints, Blade +
//! Fast glows with a white core (lightsaber), Whip + Blade chains smoulder in their element; heavy
//! blades / storm items charging glow armored. Poses: [`LegendPose`] (first person via
//! `viewmodel`, third person via the item override here + the arm hook in `players`).
//!
//! Dev: `BD_LEGEND_TEST=<scene>` ([`dev`]) — frame strips in docs/legendary.

pub mod dev;

use super::constructs::SimConstruct;
use super::{bv, corps_rgb, PresentSet};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic};
use bd_core::{ConstructId, PlayerId};
use bd_fx::{BurstFx, BurstKind, ConstructVisual, Emit, FxParticles, PFlags, PKind, ZapFx};
use bd_sim::constructs::legend::{self, StormKind};
use bd_sim::constructs::{AnimState, Construct};
use bd_sim::events::HeldFx;
use bd_sim::GameEvent;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use std::f32::consts::{PI, TAU};

const GOLD: [f32; 3] = [1.0, 0.72, 0.16];
const PINK: [f32; 3] = [1.0, 0.32, 0.78];
const TRACTOR: [f32; 3] = [1.0, 0.55, 0.12];
const BFG_GREEN: [f32; 3] = [0.25, 1.0, 0.3];
const SILVER: [f32; 3] = [0.85, 0.9, 1.0];
/// Rain drops fall from this high, over this long before landing.
const DROP_H: f32 = 32.0;
const DROP_SECS: f32 = 0.45;

// ------------------------------------------------------------------------------------ state

#[derive(Clone, Copy)]
struct WaveFx {
    origin: Vec3,
    dir: Vec3,
    axis: Vec3,
    size: f32,
    speed: f32,
    range: f32,
    t: f32,
    col: [f32; 3],
    dark: bool,
}

#[derive(Clone, Copy)]
struct Mark {
    pos: Vec3,
    t: f32,
    land: f32,
    radius: f32,
    col: [f32; 3],
    kind: StormKind,
}

#[derive(Clone, Copy)]
struct Strike {
    pos: Vec3,
    t: f32,
    dur: f32,
    radius: f32,
    kind: StormKind,
    col: [f32; 3],
    seed: u32,
}

#[derive(Clone, Copy)]
struct Ring {
    pos: Vec3,
    n: Vec3,
    t: f32,
    dur: f32,
    r0: f32,
    r1: f32,
    col: [f32; 3],
    thick: f32,
    em: f32,
}

#[derive(Clone, Copy)]
struct Glint {
    pos: Vec3,
    t: f32,
    dur: f32,
    size: f32,
    col: [f32; 3],
}

#[derive(Clone, Copy)]
struct Streak {
    from: Vec3,
    to: Vec3,
    t: f32,
    dur: f32,
    thick: f32,
    col: [f32; 3],
    em: f32,
}

#[derive(Clone, Copy)]
struct Fissure {
    from: Vec3,
    dir: Vec3,
    len: f32,
    t: f32,
    dur: f32,
    col: [f32; 3],
    seed: u32,
}

#[derive(Clone, Copy)]
struct Sweep {
    owner: PlayerId,
    center: Vec3,
    heading: Vec3,
    radius: f32,
    t: f32,
    dur: f32,
    col: [f32; 3],
    element: Element,
}

#[derive(Clone, Copy)]
struct Pull {
    owner: PlayerId,
    to: Vec3,
    t: f32,
    dur: f32,
    col: [f32; 3],
}

#[derive(Clone, Copy, PartialEq)]
enum Held {
    Player(PlayerId),
    Construct(ConstructId),
}

#[derive(Clone, Copy)]
struct Grav {
    gun: ConstructId,
    what: Option<Held>,
    t: f32,
    max: f32,
}

#[derive(Clone, Copy)]
struct Stream {
    gun: ConstructId,
    victim: Option<PlayerId>,
    t: f32,
}

#[derive(Clone, Copy)]
struct Charge {
    gun: ConstructId,
    owner: PlayerId,
    t: f32,
    full: bool,
    bfg: bool,
    col: [f32; 3],
}

#[derive(Clone, Copy)]
struct Needle {
    gun: ConstructId,
    victim: PlayerId,
    off: Vec3,
    dir: Vec3,
    age: f32,
}

#[derive(Clone, Copy)]
struct Lodged {
    id: ConstructId,
    pos: Vec3,
    n: Vec3,
    t: f32,
    col: [f32; 3],
}

/// A wielder's move in progress (poses + a few continuous visuals).
#[derive(Clone, Copy, PartialEq, Debug)]
enum MoveKind {
    Leap { dove: bool },
    Recall,
    Snap,
    Spin,
    Sputter,
}

#[derive(Clone, Copy)]
struct Move {
    owner: PlayerId,
    id: ConstructId,
    kind: MoveKind,
    t: f32,
    max: f32,
}

#[derive(Resource, Default)]
struct LegendFx {
    waves: Vec<WaveFx>,
    marks: Vec<Mark>,
    strikes: Vec<Strike>,
    rings: Vec<Ring>,
    glints: Vec<Glint>,
    streaks: Vec<Streak>,
    fissures: Vec<Fissure>,
    sweeps: Vec<Sweep>,
    pulls: Vec<Pull>,
    gravs: Vec<Grav>,
    streams: Vec<Stream>,
    charges: Vec<Charge>,
    needles: Vec<Needle>,
    lodged: Vec<Lodged>,
    moves: Vec<Move>,
    /// Recent positions of flying Returning items (trail), newest last.
    trails: Vec<(ConstructId, Vec<Vec3>)>,
    /// Armor / charge flashes to apply to construct visuals this frame.
    flashes: Vec<(ConstructId, f32)>,
    time: f32,
    seed: u32,
}

impl LegendFx {
    fn next_seed(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.seed
    }
    fn ring(&mut self, pos: Vec3, n: Vec3, dur: f32, r0: f32, r1: f32, col: [f32; 3], thick: f32, em: f32) {
        if self.rings.len() < 96 {
            self.rings.push(Ring { pos, n: n.normalize_or(Vec3::Y), t: 0.0, dur, r0, r1, col, thick, em });
        }
    }
    fn glint(&mut self, pos: Vec3, dur: f32, size: f32, col: [f32; 3]) {
        if self.glints.len() < 64 {
            self.glints.push(Glint { pos, t: 0.0, dur, size, col });
        }
    }
    fn streak(&mut self, from: Vec3, to: Vec3, dur: f32, thick: f32, col: [f32; 3], em: f32) {
        if self.streaks.len() < 64 {
            self.streaks.push(Streak { from, to, t: 0.0, dur, thick, col, em });
        }
    }
    fn start_move(&mut self, owner: PlayerId, id: ConstructId, kind: MoveKind, max: f32) {
        self.moves.retain(|m| m.owner != owner || matches!((m.kind, kind), (MoveKind::Sputter, _) | (_, MoveKind::Sputter)));
        self.moves.push(Move { owner, id, kind, t: 0.0, max });
    }
}

// ------------------------------------------------------------------------------------ poses

/// What a wielder's body / item is doing for a legendary move (poses only).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pose {
    /// Storm raise: the item held up to the sky.
    Raise,
    /// Heavy-blade leap: raised behind the head, then cleaving down on the dive.
    LeapUp,
    LeapDive,
    /// Recall: the open hand reaches for the returning item.
    Reach,
    /// The recall catch snaps the arm back.
    Snap,
    /// Chain blades' finisher: a full spin (0..1).
    Spin(f32),
    /// Gravity gun holding: the gun strains.
    Strain,
    /// Stream on: the gun shakes.
    Shake,
    /// Gun charging (0..1): a growing tremble.
    Tremble(f32),
}

/// Poses for legendary moves: first person (`viewmodel::animate_viewmodel` calls [`LegendPose::fp_swing`]),
/// third-person arms (`players` calls [`LegendPose::arm`]) and items (overridden here).
#[derive(Resource, Default)]
pub struct LegendPose {
    fp_pose: Option<Pose>,
    fp_last: Option<Transform>,
    fp_w: f32,
    fp_shake: f32,
    time: f32,
    arms: Vec<(PlayerId, f32, Option<f32>)>,
    /// (item, pose, blend weight).
    items: Vec<(ConstructId, PlayerId, Pose, f32)>,
}

fn noise3(t: f32) -> Vec3 {
    Vec3::new((t * 53.0).sin() * 0.6 + (t * 31.7).sin() * 0.4, (t * 47.3).sin() * 0.6 + (t * 23.1).cos() * 0.4, (t * 61.9).cos() * 0.5 + (t * 17.3).sin() * 0.5)
}

/// First-person item frame for a pose (right-hand root space, the swing-curve convention:
/// rotation = the item's, translation = the fist's offset × `grip_swing_reach`).
fn fp_frame(p: Pose, rest: Quat) -> (Transform, f32) {
    let t = |x: f32, y: f32, z: f32, r: Quat| Transform { translation: Vec3::new(x, y, z), rotation: r, scale: Vec3::ONE };
    match p {
        Pose::Raise => (t(-0.12, 0.42, 0.0, Quat::from_rotation_x(1.35) * Quat::from_rotation_z(0.2)), 0.004),
        Pose::LeapUp => (t(-0.06, 0.5, 0.22, Quat::from_rotation_x(2.25) * Quat::from_rotation_z(0.15)), 0.0),
        Pose::LeapDive => (t(-0.1, -0.12, -0.3, Quat::from_rotation_x(-0.75)), 0.0),
        Pose::Reach => (t(-0.22, 0.14, -0.32, Quat::from_rotation_x(0.35) * Quat::from_rotation_y(0.25)), 0.0),
        Pose::Snap => (t(0.04, -0.1, 0.24, Quat::from_rotation_x(0.9)), 0.0),
        Pose::Spin(u) => {
            let a = u.clamp(0.0, 1.0) * TAU;
            (t(a.sin() * 0.28, 0.02, -a.cos() * 0.12, Quat::from_rotation_y(-a) * Quat::from_rotation_x(0.05) * Quat::from_rotation_z(1.2)), 0.0)
        }
        Pose::Strain => (Transform { translation: Vec3::new(0.0, -0.03, 0.08), rotation: rest * Quat::from_rotation_x(0.06), scale: Vec3::ONE }, 0.02),
        Pose::Shake => (Transform { translation: Vec3::new(0.0, 0.0, 0.03), rotation: rest, scale: Vec3::ONE }, 0.035),
        Pose::Tremble(k) => (Transform { translation: Vec3::new(0.0, 0.0, 0.02 * k), rotation: rest, scale: Vec3::ONE }, 0.004 + 0.018 * k),
    }
}

impl LegendPose {
    /// First person: the held item's swing frame with the legendary pose blended over it (`cur` = the
    /// active swing curve's transform, `rest` = the item's rest rotation). `None` in → `None` out when
    /// no pose is active.
    pub fn fp_swing(&self, cur: Option<Transform>, rest: Quat) -> Option<Transform> {
        if self.fp_w < 1e-3 {
            return cur;
        }
        let base = cur.unwrap_or(Transform::from_rotation(rest));
        let target = match self.fp_pose {
            Some(p) => fp_frame(p, rest).0,
            None => self.fp_last.unwrap_or(base),
        };
        let w = self.fp_w.clamp(0.0, 1.0);
        let mut out = Transform {
            translation: base.translation.lerp(target.translation, w),
            rotation: base.rotation.slerp(target.rotation, w).normalize(),
            scale: base.scale,
        };
        if self.fp_shake > 0.0 {
            let n = noise3(self.time) * self.fp_shake * w;
            out.rotation = (out.rotation * Quat::from_euler(EulerRot::YXZ, n.x * 2.0, n.y * 2.0, n.z)).normalize();
            out.translation += n * 0.3;
        }
        out.is_finite().then_some(out)
    }

    /// Third person: (arm raise 0..1, pitch override) for `p`'s right arm while a legendary move poses it.
    pub fn arm(&self, p: PlayerId) -> Option<(f32, Option<f32>)> {
        self.arms.iter().find(|a| a.0 == p).map(|a| (a.1, a.2))
    }
}

// ------------------------------------------------------------------------------------ plugin

pub struct LegendPlugin;

impl Plugin for LegendPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GameEvents>()
            .add_message::<ZapFx>()
            .add_message::<BurstFx>()
            .init_resource::<LegendFx>()
            .init_resource::<LegendPose>()
            .add_systems(Update, (legend_events, flashes, poses).chain().in_set(PresentSet::Events))
            .add_systems(
                PostUpdate,
                (pose_items, draw)
                    .chain()
                    .after(crate::viewmodel::HandsPublishSet)
                    .before(TransformSystems::Propagate)
                    .before(bd_fx::FxSet::Emit),
            )
            .add_systems(OnExit(AppState::InGame), clear);
        dev::build(app);
    }
}

fn clear(mut fx: ResMut<LegendFx>, mut pose: ResMut<LegendPose>) {
    *fx = LegendFx::default();
    *pose = LegendPose::default();
}

// ------------------------------------------------------------------------------------ colours

/// Element colour for legendary effects (Light → the wielder's corps; Electric reads blue-white
/// like lightning; Shadow → its red rim).
fn el_rgb(e: Element, corps: [f32; 3]) -> [f32; 3] {
    match e {
        Element::Electric => [0.55, 0.78, 1.0],
        Element::Shadow => [1.0, 0.06, 0.08],
        Element::Earth => [0.85, 0.55, 0.25],
        _ => super::signature::element_rgb(e, corps),
    }
}

fn hot(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] + (1.0 - c[0]) * k, c[1] + (1.0 - c[1]) * k, c[2] + (1.0 - c[2]) * k]
}

fn scale3(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn item_col(active: &ActiveWorld, c: Option<&Construct>, owner: PlayerId) -> [f32; 3] {
    let corps = corps_rgb(active.corps_of(owner));
    match c {
        Some(c) if c.flair & bd_sim::replicate::cflags::LEGENDARY != 0 && c.recipe.element == Element::Light => GOLD,
        Some(c) => el_rgb(c.recipe.element, corps),
        None => corps,
    }
}

fn storm_kind(c: Option<&Construct>) -> StormKind {
    c.and_then(|c| StormKind::of(c.recipe.element)).unwrap_or(StormKind::Bolt)
}

/// Tiny xorshift for cosmetic jitter (the bd_fx pool's own rng can't be borrowed next to it).
struct R(u32);
impl R {
    fn new(seed: u32) -> Self {
        R(seed | 1)
    }
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    fn dir(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }
}

fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

fn hf(a: u32, b: u32) -> f32 {
    (hash(a, b) & 0xFFFF) as f32 / 65535.0
}

fn perp(n: Vec3) -> (Vec3, Vec3) {
    let n = n.normalize_or(Vec3::Y);
    let a = if n.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = n.cross(a).normalize_or(Vec3::X);
    (u, n.cross(u))
}

/// The crescent shape of `legend::wave_point` with a continuous parameter `f` ∈ [−1, 1]
/// (`wave_point(k)` = `crescent(k / 2)`).
fn crescent(center: Vec3, dir: Vec3, axis: Vec3, half_w: f32, f: f32) -> Vec3 {
    center + axis * (f * half_w) - dir * (f * f * half_w * 0.45)
}

// ------------------------------------------------------------------------------------ events

#[allow(clippy::too_many_arguments)]
fn legend_events(
    time: Res<Time>,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut st: ResMut<LegendFx>,
    mut zaps: MessageWriter<ZapFx>,
    mut bursts: MessageWriter<BurstFx>,
    mut particles: ResMut<FxParticles>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let st = &mut *st;
    st.time += time.delta_secs();
    let tuning = active.world.cfg.tuning.legendary;
    let fx = &mut *particles;
    let mut rng = R::new(st.next_seed());
    for ev in events.read() {
        match &ev.event {
            GameEvent::HeldFx { id, owner, fx: kind, pos, dir, strength } => {
                let p = bv(*pos);
                let d = bv(*dir).normalize_or(Vec3::NEG_Z);
                let s = *strength;
                let c = active.world.constructs.get(*id);
                let col = item_col(&active, c, *owner);
                match kind {
                    HeldFx::Lodge => {
                        bursts.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: d, scale: 1.4, color: None });
                        bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: d, scale: 0.8, color: Some(hot(col, 0.3)) });
                        st.ring(p + d * 0.05, d, 0.35, 0.2, 1.6, hot(col, 0.4), 0.07, 5.0);
                        st.lodged.retain(|l| l.id != *id);
                        st.lodged.push(Lodged { id: *id, pos: p, n: d, t: 0.0, col });
                    }
                    HeldFx::RecallStart => {
                        st.lodged.retain(|l| l.id != *id);
                        bursts.write(BurstFx { kind: BurstKind::Element(c.map_or(Element::Light, |c| c.recipe.element)), pos: p, dir: Vec3::Y, scale: 1.2, color: None });
                        st.glint(p, 0.25, 1.2, hot(col, 0.5));
                        if let Some(v) = views.player(*owner) {
                            let hand = v.pos + Vec3::Y * 1.3 * v.size;
                            st.streak(hand, p, 0.14, 0.035, hot(col, 0.7), 9.0);
                        }
                        st.start_move(*owner, *id, MoveKind::Recall, 2.5);
                    }
                    HeldFx::Catch => {
                        let snap = s >= 1.9;
                        st.glint(p, if snap { 0.22 } else { 0.12 }, if snap { 1.0 } else { 0.5 }, hot(col, 0.6));
                        if snap {
                            let n = views.player(*owner).map_or(Vec3::Y, |v| bv(bd_core::input::look_dir(v.yaw, v.pitch)));
                            st.ring(p, n, 0.22, 0.1, 1.3, hot(col, 0.5), 0.05, 7.0);
                            bursts.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 0.8, color: Some(hot(col, 0.5)) });
                            st.flashes.push((*id, 1.0));
                            st.start_move(*owner, *id, MoveKind::Snap, 0.28);
                        } else {
                            st.moves.retain(|m| !(m.owner == *owner && m.kind == MoveKind::Recall));
                        }
                        st.trails.retain(|t| t.0 != *id);
                    }
                    HeldFx::StormRaise => {
                        if s <= 0.0 {
                            // Still recharging: a grey fizzle.
                            for _ in 0..10 {
                                let v = rng.dir() * 2.0 + Vec3::Y * 1.5;
                                spark(fx, p + Vec3::Y * 0.3, v, [0.5, 0.5, 0.55], 0.05, 0.35, 1.5, 1.0);
                            }
                            st.start_move(*owner, *id, MoveKind::Sputter, 0.4);
                        } else {
                            st.glint(p + Vec3::Y * 0.8, 0.3, 1.6, hot(col, 0.6));
                            bursts.write(BurstFx { kind: BurstKind::Element(c.map_or(Element::Electric, |c| c.recipe.element)), pos: p + Vec3::Y * 0.6, dir: Vec3::Y, scale: 1.5, color: None });
                        }
                    }
                    HeldFx::StormMark => {
                        let kind = storm_kind(c);
                        let radius = if kind.rains() { if kind == StormKind::Meteors { 2.6 } else { 1.6 } } else { tuning.storm_radius * if kind == StormKind::Crush { 1.3 } else if kind == StormKind::Nova { 1.5 } else if kind.lingers() { 1.6 } else { 1.0 } };
                        if st.marks.len() < 32 {
                            st.marks.push(Mark { pos: p, t: 0.0, land: s.max(0.05), radius, col, kind });
                        }
                    }
                    HeldFx::StormStrike => {
                        let kind = storm_kind(c);
                        let drop = d.y < -0.5 && kind.rains();
                        let seed = st.next_seed();
                        // A lingering storm's closing burst: a ring, not a second funnel.
                        if kind.lingers() && st.strikes.iter().any(|x| x.kind == kind && x.pos.distance(p) < 1.0 && x.t < x.dur) {
                            st.ring(p + Vec3::Y * 0.2, Vec3::Y, 0.45, 0.5, s * 1.4, hot(col, 0.3), 0.12, 5.0);
                            bursts.write(BurstFx { kind: BurstKind::Element(c.map_or(Element::Wind, |c| c.recipe.element)), pos: p + Vec3::Y, dir: Vec3::Y, scale: 2.5, color: None });
                            continue;
                        }
                        let dur = match kind {
                            StormKind::Bolt => 0.4,
                            StormKind::Meteors => 0.5,
                            StormKind::Icicles => 1.4,
                            StormKind::Tornado | StormKind::Maw | StormKind::Miasma => tuning.linger_secs + 0.25,
                            StormKind::Geyser => 1.0,
                            StormKind::Crush => 0.55,
                            StormKind::Spikes => 1.4,
                            StormKind::Nova => 0.75,
                        };
                        st.marks.retain(|m| m.pos.distance(p) > 0.8 || m.t < m.land - 0.1);
                        if st.strikes.len() < 32 {
                            st.strikes.push(Strike { pos: p, t: 0.0, dur, radius: s.max(0.5), kind, col, seed });
                        }
                        strike_burst(fx, &mut bursts, kind, p, s, col, drop, c.map_or(Element::Electric, |c| c.recipe.element));
                        st.ring(p + Vec3::Y * 0.12, Vec3::Y, 0.4, 0.3, s * 1.25, hot(col, 0.35), 0.1, 6.0);
                    }
                    HeldFx::StormArc => {
                        let to = p + d * s;
                        zaps.write(ZapFx { from: p, to, color: hot(col, 0.4), duration: 0.3 });
                        zaps.write(ZapFx { from: p, to, color: [1.0, 1.0, 1.0], duration: 0.12 });
                        st.glint(to, 0.2, 0.8, hot(col, 0.5));
                    }
                    HeldFx::Ricochet => {
                        bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: d, scale: 1.2, color: Some(hot(col, 0.5)) });
                        bursts.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 0.7, color: None });
                        st.glint(p, 0.25, 1.1, [1.0, 1.0, 1.0]);
                        st.ring(p, d, 0.2, 0.2, 1.1, hot(col, 0.6), 0.04, 7.0);
                    }
                    HeldFx::Parry => {
                        st.glint(p, 0.2, 1.3, [1.0, 1.0, 1.0]);
                        st.flashes.push((*id, 1.0));
                        if c.is_some_and(legend::swing_deflects) {
                            // The reflected bolt heads off along the parrier's aim.
                            st.streak(p, p + d * 14.0, 0.28, 0.06, hot(col, 0.6), 8.0);
                            st.ring(p, d, 0.18, 0.15, 0.9, hot(col, 0.5), 0.04, 8.0);
                        }
                    }
                    HeldFx::Wave => {
                        let heavy = c.is_some_and(|c| c.has(Mechanic::Heavy));
                        let vertical = s < 0.0;
                        let size = s.abs().max(0.1);
                        let axis = bv(legend::wave_axis(*dir, vertical));
                        let dark = c.is_some_and(|c| c.recipe.element == Element::Shadow);
                        if st.waves.len() < 24 {
                            st.waves.push(WaveFx { origin: p, dir: d, axis, size, speed: legend::wave_speed(&tuning, heavy), range: tuning.wave_range, t: 0.0, col, dark });
                        }
                    }
                    HeldFx::LeapStart => {
                        bursts.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::Y, scale: 1.8, color: None });
                        st.ring(p + Vec3::Y * 0.1, Vec3::Y, 0.35, 0.4, 2.8, [0.9, 0.85, 0.75], 0.08, 2.5);
                        st.start_move(*owner, *id, MoveKind::Leap { dove: false }, 2.8);
                    }
                    HeldFx::LeapSlam => {
                        let r = s.max(1.0);
                        st.ring(p + Vec3::Y * 0.15, Vec3::Y, 0.45, 0.4, r, hot(col, 0.5), 0.14, 7.0);
                        st.ring(p + Vec3::Y * 0.1, Vec3::Y, 0.7, 0.2, r * 1.35, [0.9, 0.8, 0.6], 0.07, 2.0);
                        let heading = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::NEG_Z);
                        let seed = st.next_seed();
                        st.fissures.push(Fissure { from: p - heading * 1.2, dir: heading, len: r * 1.5, t: 0.0, dur: 1.4, col: hot(col, 0.2), seed });
                        bursts.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::Y, scale: 3.0, color: None });
                        for _ in 0..40 {
                            let a = rng.range(0.0, TAU);
                            let v = Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(4.0, 10.0) + Vec3::Y * rng.range(3.0, 9.0);
                            spark(fx, p + Vec3::Y * 0.2, v, hot(col, 0.3), 0.08, 0.7, 4.0, 1.2);
                        }
                        st.moves.retain(|m| !(m.owner == *owner && matches!(m.kind, MoveKind::Leap { .. })));
                    }
                    HeldFx::Armor => {
                        for k in 0..3 {
                            st.ring(p + Vec3::Y * (0.3 + 0.6 * k as f32), Vec3::Y, 0.22, 0.6, 1.0, SILVER, 0.035, 6.0);
                        }
                        st.glint(p + Vec3::Y * 0.9, 0.18, 1.0, SILVER);
                        st.flashes.push((*id, 0.8));
                    }
                    HeldFx::ChainSweep => {
                        let heading = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::NEG_Z);
                        st.sweeps.push(Sweep { owner: *owner, center: p, heading, radius: s.max(1.0), t: 0.0, dur: 0.38, col, element: c.map_or(Element::Fire, |c| c.recipe.element) });
                        st.start_move(*owner, *id, MoveKind::Spin, 0.42);
                    }
                    HeldFx::ChainPull => {
                        st.pulls.push(Pull { owner: *owner, to: p, t: 0.0, dur: 0.32, col });
                        st.glint(p, 0.2, 0.9, hot(col, 0.5));
                        bursts.write(BurstFx { kind: BurstKind::Element(c.map_or(Element::Fire, |c| c.recipe.element)), pos: p, dir: d, scale: 1.0, color: None });
                    }
                    HeldFx::OneShot => {
                        bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: d, scale: 1.6, color: Some(GOLD) });
                        st.glint(p, 0.2, 1.4, hot(GOLD, 0.4));
                        st.ring(p, d, 0.25, 0.1, 1.2, GOLD, 0.05, 8.0);
                    }
                    HeldFx::NeedleStick => {
                        let victim = nearest_player(&views, p, Some(*owner));
                        if let Some((vid, vpos)) = victim {
                            if st.needles.len() < 96 {
                                st.needles.push(Needle { gun: *id, victim: vid, off: p - vpos, dir: d, age: 0.0 });
                            }
                            for n in st.needles.iter_mut().filter(|n| n.victim == vid) {
                                n.age = 0.0;
                            }
                        }
                        for _ in 0..5 {
                            spark(fx, p, rng.dir() * 2.5 - d * 1.5, PINK, 0.04, 0.3, 5.0, 0.5);
                        }
                    }
                    HeldFx::Supercombine => {
                        if let Some((vid, _)) = nearest_player(&views, p, Some(*owner)) {
                            st.needles.retain(|n| n.victim != vid);
                        }
                        let r = s.max(1.0);
                        st.ring(p, Vec3::Y, 0.35, 0.2, r, PINK, 0.12, 9.0);
                        st.ring(p, Vec3::X, 0.35, 0.2, r, PINK, 0.08, 7.0);
                        st.ring(p, Vec3::Z, 0.35, 0.2, r, PINK, 0.08, 7.0);
                        st.glint(p, 0.3, 2.2, hot(PINK, 0.5));
                        bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::ZERO, scale: 2.5, color: Some(PINK) });
                        for _ in 0..40 {
                            spark(fx, p, rng.dir() * rng.range(4.0, 12.0), hot(PINK, rng.f() * 0.5), 0.07, 0.5, 6.0, 0.4);
                        }
                    }
                    HeldFx::GravGrab => {
                        let what = if s >= 1.5 { nearest_construct(&active, &views, p).map(Held::Construct) } else { nearest_player(&views, p, Some(*owner)).map(|x| Held::Player(x.0)) };
                        let max = if s >= 1.5 { tuning.grav_construct_secs } else { tuning.grav_player_secs } + 0.3;
                        st.gravs.retain(|g| g.gun != *id);
                        st.gravs.push(Grav { gun: *id, what, t: 0.0, max });
                        st.glint(p, 0.2, 1.2, TRACTOR);
                        st.ring(p, Vec3::Y, 0.25, 1.5, 0.3, TRACTOR, 0.05, 6.0);
                    }
                    HeldFx::GravPunt => {
                        st.gravs.retain(|g| g.gun != *id);
                        if s < 0.0 {
                            // Empty-handed punt: a shockwave cone.
                            let range = tuning.punt_blast_range;
                            let half = tuning.punt_blast_cone_deg.to_radians();
                            for k in 0..5 {
                                let f = (k as f32 + 1.0) / 5.0;
                                let r = (range * f * half.tan()).max(0.2);
                                st.rings.push(Ring { pos: p + d * range * f, n: d, t: -0.03 * k as f32, dur: 0.25, r0: r * 0.6, r1: r, col: TRACTOR, thick: 0.05, em: 6.0 * (1.0 - f * 0.5) });
                            }
                        } else {
                            st.ring(p + d * 0.6, d, 0.2, 0.2, 1.4, hot(TRACTOR, 0.4), 0.07, 9.0);
                            st.streak(p, p + d * 6.0, 0.18, 0.08, hot(TRACTOR, 0.5), 8.0);
                        }
                        st.glint(p, 0.18, 1.3, hot(TRACTOR, 0.5));
                        bursts.write(BurstFx { kind: BurstKind::Flash, pos: p + d * 0.4, dir: d, scale: 0.9, color: Some(TRACTOR) });
                    }
                    HeldFx::StreamLatch => {
                        let v = nearest_player(&views, p, Some(*owner)).map(|x| x.0);
                        st.streams.retain(|x| x.gun != *id);
                        st.streams.push(Stream { gun: *id, victim: v, t: 0.0 });
                        st.glint(p, 0.2, 1.0, hot(col, 0.5));
                    }
                    HeldFx::StreamRelease => {
                        st.streams.retain(|x| x.gun != *id);
                        if s > 0.0 {
                            st.streak(p, p + d * (s * 0.12).clamp(1.0, 6.0), 0.2, 0.06, hot(col, 0.4), 6.0);
                        }
                        bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: d, scale: 0.8, color: Some(hot(col, 0.3)) });
                    }
                    HeldFx::GunCharge => {
                        let bfg = c.is_some_and(legend::bfg);
                        if s <= 0.0 || bfg {
                            st.charges.retain(|x| x.gun != *id);
                            st.charges.push(Charge { gun: *id, owner: *owner, t: 0.0, full: false, bfg, col: if bfg { BFG_GREEN } else { col } });
                        } else if let Some(ch) = st.charges.iter_mut().find(|x| x.gun == *id) {
                            ch.full = true;
                            st.glint(p, 0.2, 1.0, [1.0; 3]);
                        }
                    }
                    HeldFx::ChargeShot => {
                        st.charges.retain(|x| x.gun != *id);
                        let k = s.clamp(0.0, 1.0);
                        st.ring(p + d * 0.3, d, 0.2 + 0.15 * k, 0.1, 0.5 + 1.3 * k, hot(col, 0.4), 0.04 + 0.06 * k, 8.0);
                        st.glint(p, 0.15 + 0.1 * k, 0.6 + 1.2 * k, hot(col, 0.5));
                        bursts.write(BurstFx { kind: BurstKind::Flash, pos: p + d * 0.3, dir: d, scale: 0.5 + k, color: Some(col) });
                    }
                    HeldFx::BfgZap => {
                        let to = p + d * s;
                        zaps.write(ZapFx { from: p, to, color: BFG_GREEN, duration: 0.2 });
                        zaps.write(ZapFx { from: p, to, color: hot(BFG_GREEN, 0.6), duration: 0.1 });
                        st.glint(to, 0.15, 0.7, BFG_GREEN);
                    }
                    HeldFx::SwingStart | HeldFx::ChargeStart => {}
                    _ => {}
                }
            }
            GameEvent::ShotTrace { owner, from, to, .. } => {
                let held = active.world.constructs.held_by(*owner);
                if held.is_some_and(|c| legend::one_shot_recipe(&c.recipe, c.flair & bd_sim::replicate::cflags::LEGENDARY != 0)) {
                    let (a, b) = (bv(*from), bv(*to));
                    if a.is_finite() && b.is_finite() {
                        st.streak(a, b, 0.7, 0.07, GOLD, 5.0);
                        st.streak(a, b, 0.25, 0.025, hot(GOLD, 0.7), 12.0);
                        st.glint(b, 0.3, 1.0, GOLD);
                    }
                }
            }
            GameEvent::ShotFired { owner, .. } => {
                st.charges.retain(|c| c.owner != *owner || c.bfg && c.t < 0.05);
            }
            GameEvent::ConstructGone { id, .. } | GameEvent::ConstructShattered { id, .. } => {
                st.lodged.retain(|l| l.id != *id);
                st.gravs.retain(|g| g.gun != *id && g.what != Some(Held::Construct(*id)));
                st.streams.retain(|x| x.gun != *id);
                st.charges.retain(|x| x.gun != *id);
                st.needles.retain(|n| n.gun != *id);
                st.trails.retain(|t| t.0 != *id);
                st.moves.retain(|m| m.id != *id);
            }
            _ => {}
        }
    }
}

fn nearest_player(views: &WorldViews, p: Vec3, not: Option<PlayerId>) -> Option<(PlayerId, Vec3)> {
    views
        .players
        .iter()
        .filter(|v| Some(v.id) != not && v.alive())
        .map(|v| (v.id, v.pos, (v.pos + Vec3::Y * 0.9 * v.size).distance(p)))
        .filter(|x| x.2 < 4.0)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|x| (x.0, x.1))
}

fn nearest_construct(active: &ActiveWorld, views: &WorldViews, p: Vec3) -> Option<ConstructId> {
    views
        .constructs
        .iter()
        .filter(|c| !c.attached())
        .map(|c| (c.id, active.world.constructs.get(c.id).map_or(c.pos, |k| bv(k.center())).distance(p)))
        .filter(|x| x.1 < 6.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
}

#[allow(clippy::too_many_arguments)]
fn spark(fx: &mut FxParticles, pos: Vec3, vel: Vec3, col: [f32; 3], size: f32, life: f32, em: f32, gravity: f32) {
    fx.emit(Emit { kind: PKind::Ballistic, flags: PFlags::STRETCH | PFlags::PLAIN, pos, vel, size, color: col, emissive: em, life, gravity, drag: 1.2, spin: 2.0, ..default() });
}

fn puff(fx: &mut FxParticles, pos: Vec3, vel: Vec3, col: [f32; 3], size: f32, life: f32, em: f32) {
    fx.emit(Emit { kind: PKind::Rise, flags: PFlags::GROW | PFlags::PLAIN, pos, vel, size, color: col, emissive: em, life, gravity: 0.6, spin: 0.3, ..default() });
}

/// The one-shot particles of a strike (the continuous shapes are drawn every frame in `draw`).
#[allow(clippy::too_many_arguments)]
fn strike_burst(fx: &mut FxParticles, bursts: &mut MessageWriter<BurstFx>, kind: StormKind, p: Vec3, r: f32, col: [f32; 3], drop: bool, element: Element) {
    let _ = drop;
    let mut rng = R::new(p.x.to_bits() ^ p.z.to_bits().rotate_left(16));
    bursts.write(BurstFx { kind: BurstKind::Element(element), pos: p + Vec3::Y * 0.3, dir: Vec3::Y, scale: (r * 0.8).clamp(1.0, 4.0), color: None });
    match kind {
        StormKind::Bolt => {
            bursts.write(BurstFx { kind: BurstKind::Flash, pos: p + Vec3::Y * 0.5, dir: Vec3::ZERO, scale: 2.0, color: Some(hot(col, 0.6)) });
            for _ in 0..36 {
                let a = rng.range(0.0, TAU);
                let v = Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(3.0, 11.0) + Vec3::Y * rng.range(2.0, 8.0);
                spark(fx, p + Vec3::Y * 0.2, v, hot(col, rng.f() * 0.7), 0.05, 0.5, 8.0, 1.0);
            }
        }
        StormKind::Meteors => {
            for _ in 0..24 {
                let v = rng.dir() * rng.range(3.0, 9.0) + Vec3::Y * 5.0;
                spark(fx, p + Vec3::Y * 0.3, v, if rng.f() < 0.4 { [1.0, 0.8, 0.3] } else { col }, 0.08, 0.8, 5.0, 1.0);
            }
            bursts.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::Y, scale: 1.5, color: None });
        }
        StormKind::Icicles => {
            for _ in 0..16 {
                let v = rng.dir() * rng.range(2.0, 7.0) + Vec3::Y * 3.0;
                spark(fx, p + Vec3::Y * 0.3, v, hot(col, 0.5), 0.06, 0.6, 4.0, 1.0);
            }
        }
        StormKind::Geyser => {
            for _ in 0..60 {
                let a = rng.range(0.0, TAU);
                let v = Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(0.5, 3.0) + Vec3::Y * rng.range(10.0, 20.0);
                fx.emit(Emit { kind: PKind::Ballistic, flags: PFlags::PLAIN, pos: p + Vec3::Y * 0.3, vel: v, size: rng.range(0.08, 0.2), color: hot(col, rng.f() * 0.5), emissive: 2.0, life: 1.3, gravity: 1.0, drag: 0.3, ..default() });
            }
        }
        StormKind::Spikes => bursts.write_dust(p, 2.5),
        StormKind::Crush => bursts.write_dust(p, 2.0),
        StormKind::Nova => {
            bursts.write(BurstFx { kind: BurstKind::Flash, pos: p + Vec3::Y, dir: Vec3::ZERO, scale: 3.0, color: Some(hot(col, 0.6)) });
            for _ in 0..50 {
                spark(fx, p + Vec3::Y, rng.dir() * rng.range(6.0, 16.0), hot(col, rng.f()), 0.07, 0.6, 8.0, 0.3);
            }
        }
        StormKind::Tornado | StormKind::Maw | StormKind::Miasma => {}
    }
}

trait DustBurst {
    fn write_dust(&mut self, p: Vec3, scale: f32);
}
impl DustBurst for MessageWriter<'_, BurstFx> {
    fn write_dust(&mut self, p: Vec3, scale: f32) {
        self.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::Y, scale, color: None });
    }
}

// ------------------------------------------------------------------------------------ flashes + poses

fn flashes(mut st: ResMut<LegendFx>, map: Option<Res<super::constructs::ConstructEntities>>, active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, mut q: Query<&mut SimConstruct>) {
    let (Some(map), Some(active)) = (map, active) else {
        st.flashes.clear();
        return;
    };
    // Heavy blades / storm items charging look armored: a slow, subtle glow pulse.
    let now = st.time;
    for cv in &views.constructs {
        if cv.anim != AnimState::Charging as u8 {
            continue;
        }
        let Some(c) = active.world.constructs.get(cv.id) else { continue };
        if legend::long_charge(c) {
            let k = 0.1 + 0.08 * (now * 7.0).sin().abs() + (cv.anim_time * 0.15).min(0.12);
            st.flashes.push((cv.id, k));
        }
    }
    for (id, f) in st.flashes.drain(..) {
        if let Some(&e) = map.0.get(&id)
            && let Ok(mut sc) = q.get_mut(e)
        {
            sc.flash_at_least(f);
        }
    }
}

/// The pose each wielder's legendary move asks for this frame.
fn pose_of(st: &LegendFx, active: &ActiveWorld, views: &WorldViews, owner: PlayerId, c: &Construct) -> Option<Pose> {
    for m in st.moves.iter().filter(|m| m.owner == owner) {
        return Some(match m.kind {
            MoveKind::Leap { dove } => {
                if dove {
                    Pose::LeapDive
                } else {
                    Pose::LeapUp
                }
            }
            MoveKind::Recall => Pose::Reach,
            MoveKind::Snap => Pose::Snap,
            MoveKind::Spin => Pose::Spin(m.t / 0.38),
            MoveKind::Sputter => continue,
        });
    }
    let view = views.construct(c.id);
    let anim = view.map_or(c.anim_state as u8, |v| v.anim);
    if anim == AnimState::Telegraph as u8 && legend::storms(c) {
        return Some(Pose::Raise);
    }
    if st.gravs.iter().any(|g| g.gun == c.id && g.what.is_some()) {
        return Some(Pose::Strain);
    }
    if legend::stream_gun(c) && anim == AnimState::Active as u8 {
        return Some(Pose::Shake);
    }
    if let Some(ch) = st.charges.iter().find(|x| x.gun == c.id) {
        let full = if ch.bfg { active.world.cfg.tuning.legendary.bfg_spinup } else { active.world.cfg.tuning.legendary.charge_full };
        return Some(Pose::Tremble((ch.t / full.max(0.1)).clamp(0.0, 1.0)));
    }
    None
}

fn poses(time: Res<Time>, active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, mut st: ResMut<LegendFx>, mut pose: ResMut<LegendPose>) {
    let dt = time.delta_secs().min(0.05);
    let Some(active) = active else {
        *pose = LegendPose::default();
        return;
    };
    let st = &mut *st;
    // Advance moves (the leap turns into its dive once the wielder falls).
    for m in st.moves.iter_mut() {
        m.t += dt;
        if let MoveKind::Leap { dove } = &mut m.kind
            && !*dove
            && m.t > 0.2
            && views.player(m.owner).is_some_and(|v| v.vel.y < -1.0)
        {
            *dove = true;
        }
    }
    st.moves.retain(|m| m.t < m.max);
    pose.time += dt;
    pose.arms.clear();
    let mut items = std::mem::take(&mut pose.items);
    for it in items.iter_mut() {
        it.3 = -it.3.abs(); // mark unseen (negative), refreshed below
    }
    let mut fp: Option<Pose> = None;
    for v in &views.players {
        let Some(c) = active.world.constructs.held_by(v.id).filter(|c| c.category == FormCategory::Held) else { continue };
        let Some(p) = pose_of(st, &active, &views, v.id, c) else { continue };
        if active.local == Some(v.id) {
            fp = Some(p);
        }
        let arm = match p {
            Pose::Raise | Pose::LeapUp => Some((1.0, Some(1.1))),
            Pose::LeapDive => Some((1.0, Some(-0.6))),
            Pose::Reach => Some((1.0, None)),
            Pose::Snap => Some((0.7, Some(0.5))),
            Pose::Spin(_) => Some((1.0, Some(0.0))),
            _ => None,
        };
        if let Some((a, pitch)) = arm {
            pose.arms.push((v.id, a, pitch));
        }
        match items.iter_mut().find(|i| i.0 == c.id) {
            Some(i) => {
                i.2 = p;
                i.3 = i.3.abs().max(1e-3);
            }
            None => items.push((c.id, v.id, p, 1e-3)),
        }
    }
    // Blend weights: toward 1 while posed, back to 0 after (then dropped).
    let k = 1.0 - (-16.0 * dt).exp();
    for it in items.iter_mut() {
        if it.3 > 0.0 {
            it.3 += (1.0 - it.3) * k;
        } else {
            it.3 = -(it.3.abs() * (1.0 - k));
        }
    }
    items.retain(|i| i.3.abs() > 1e-3);
    pose.items = items;
    // First person.
    let rest = Quat::IDENTITY;
    match fp {
        Some(p) => {
            pose.fp_pose = Some(p);
            pose.fp_last = Some(fp_frame(p, rest).0);
            pose.fp_shake = fp_frame(p, rest).1;
            // The snap / spin / dive are fast; the rest ease in.
            let rate = if matches!(p, Pose::Snap | Pose::Spin(_) | Pose::LeapDive) { 40.0 } else { 14.0 };
            pose.fp_w += (1.0 - pose.fp_w) * (1.0 - (-rate * dt).exp());
        }
        None => {
            // Rest-relative poses keep their target while fading out; absolute ones fade from theirs.
            if matches!(pose.fp_pose, Some(Pose::Strain | Pose::Shake | Pose::Tremble(_))) {
                pose.fp_pose = None;
                pose.fp_last = None;
            } else if let Some(p) = pose.fp_pose.take() {
                pose.fp_last = Some(fp_frame(p, rest).0);
            }
            pose.fp_shake = 0.0;
            pose.fp_w *= (-12.0 * dt).exp();
            if pose.fp_w < 1e-3 {
                pose.fp_w = 0.0;
            }
        }
    }
}

/// Third person: legendary poses turn the held item about the hand (storm raise to the sky, the
/// leap's overhead cleave, the chain spin, a straining gun).
fn pose_items(
    pose: Res<LegendPose>,
    views: Res<WorldViews>,
    fp: Option<Res<crate::viewmodel::FirstPersonHeld>>,
    hands: Res<super::players::BodyHands>,
    map: Option<Res<super::constructs::ConstructEntities>>,
    mut q: Query<&mut Transform, With<SimConstruct>>,
) {
    let Some(map) = map else { return };
    for &(id, owner, p, w) in &pose.items {
        let w = w.abs().clamp(0.0, 1.0);
        if w < 1e-3 || fp.as_ref().is_some_and(|f| f.active && f.construct == Some(id)) {
            continue;
        }
        let Some(v) = views.player(owner) else { continue };
        let Some(&e) = map.0.get(&id) else { continue };
        let Ok(mut tf) = q.get_mut(e) else { continue };
        let yaw = Quat::from_rotation_y(v.yaw);
        let look = Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0);
        let target = match p {
            Pose::Raise => yaw * Quat::from_rotation_x(1.45),
            Pose::LeapUp => yaw * Quat::from_rotation_x(2.3),
            Pose::LeapDive => yaw * Quat::from_rotation_x(-0.7),
            Pose::Spin(u) => Quat::from_rotation_y(v.yaw - u.clamp(0.0, 1.0) * TAU) * Quat::from_rotation_y(-PI * 0.5),
            Pose::Strain | Pose::Shake | Pose::Tremble(_) => {
                let amp = match p {
                    Pose::Strain => 0.05,
                    Pose::Shake => 0.07,
                    Pose::Tremble(k) => 0.01 + 0.03 * k,
                    _ => 0.0,
                };
                let n = noise3(pose.time + id.0 as f32) * amp;
                look * Quat::from_euler(EulerRot::YXZ, n.x, n.y, n.z)
            }
            Pose::Reach | Pose::Snap => continue,
        };
        let hand = hands.0.get(&owner).copied().unwrap_or(tf.translation);
        let old = tf.rotation;
        let new = old.slerp(target, w).normalize();
        let local = old.inverse() * (hand - tf.translation);
        let t = hand - new * local;
        if t.is_finite() && new.is_finite() {
            tf.translation = t;
            tf.rotation = new;
        }
    }
}

// ------------------------------------------------------------------------------------ drawing

fn seg(fx: &mut FxParticles, a: Vec3, b: Vec3, th: f32, col: [f32; 3], em: f32) {
    if a.is_finite() && b.is_finite() && em > 0.01 && (b - a).length_squared() > 1e-6 {
        fx.immediate_segment(a, b, th.max(0.004), col, em, PFlags::PLAIN);
    }
}

fn cube(fx: &mut FxParticles, c: Vec3, rot: Quat, size: Vec3, col: [f32; 3], em: f32) {
    if c.is_finite() {
        fx.immediate_box(c, rot, size, col, em, PFlags::PLAIN);
    }
}

fn ring(fx: &mut FxParticles, c: Vec3, n: Vec3, r: f32, th: f32, col: [f32; 3], em: f32, phase: f32) {
    let (u, v) = perp(n);
    let segs = ((r * 7.0) as usize).clamp(12, 48);
    let mut prev = c + u * r;
    for i in 1..=segs {
        let a = phase + i as f32 / segs as f32 * TAU;
        let p = c + (u * (a - phase).cos() + v * (a - phase).sin()) * r;
        seg(fx, prev, p, th, col, em);
        prev = p;
    }
}

/// A jagged lightning line (re-rolled ~24 times a second).
#[allow(clippy::too_many_arguments)]
fn bolt(fx: &mut FxParticles, a: Vec3, b: Vec3, th: f32, col: [f32; 3], em: f32, seed: u32, now: f32) {
    let d = b - a;
    let len = d.length();
    if len < 0.05 {
        return;
    }
    let (u, v) = perp(d / len);
    let phase = (now * 24.0) as u32;
    let n = ((len / 0.9) as usize).clamp(4, 40);
    let amp = (len * 0.04 + 0.1).min(1.4);
    let mut prev = a;
    for i in 1..=n {
        let f = i as f32 / n as f32;
        let j = if i == n { Vec3::ZERO } else { (u * (hf(seed ^ phase, i as u32 * 2) - 0.5) + v * (hf(seed ^ phase, i as u32 * 2 + 1) - 0.5)) * 2.0 * amp };
        let p = a + d * f + j;
        seg(fx, prev, p, th, col, em);
        prev = p;
    }
}

/// A chain of links from `a` to `b` (alternating flat links).
fn chain(fx: &mut FxParticles, a: Vec3, b: Vec3, col: [f32; 3], em: f32) {
    let d = b - a;
    let len = d.length();
    if len < 0.1 {
        return;
    }
    let dir = d / len;
    let n = ((len / 0.22) as usize).clamp(2, 80);
    let base = Quat::from_rotation_arc(Vec3::Z, dir);
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32;
        let rot = base * Quat::from_rotation_z(if i % 2 == 0 { 0.0 } else { PI * 0.5 });
        cube(fx, a + d * f, rot, Vec3::new(0.06, 0.02, 0.17), col, em);
    }
}

fn item_axis(tf: &Transform, vis: &ConstructVisual) -> (Vec3, Vec3) {
    let b = &vis.model.bounds;
    let (cx, cy) = ((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5);
    (tf.transform_point(Vec3::new(cx, cy, b.max.z)), tf.transform_point(Vec3::new(cx, cy, b.min.z)))
}

#[allow(clippy::too_many_arguments)]
fn draw(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut st: ResMut<LegendFx>,
    mut particles: ResMut<FxParticles>,
    items: Query<(&SimConstruct, &Transform, &ConstructVisual)>,
    hands: Res<super::players::BodyHands>,
    vm_hands: Res<crate::viewmodel::ViewmodelHands>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.05);
    let st = &mut *st;
    let fx = &mut *particles;
    let now = st.time;
    let tuning = active.world.cfg.tuning.legendary;
    let mut rng = R::new(st.next_seed());
    let hand_of = |p: PlayerId| -> Option<Vec3> {
        if active.local == Some(p) && vm_hands.active {
            return Some(vm_hands.world[1]);
        }
        hands.0.get(&p).copied().or_else(|| views.player(p).map(|v| v.pos + Vec3::Y * 1.2 * v.size))
    };

    // ---- crescents (clients simulate them from the launch event)
    for w in st.waves.iter_mut() {
        w.t += dt;
        let travelled = w.speed * w.t;
        let d = travelled.min(w.range);
        let fade_out = if travelled > w.range { 1.0 - (travelled - w.range) / (w.speed * 0.12).max(1.0) } else { 1.0 };
        let fade = fade_out.clamp(0.0, 1.0) * (w.t / 0.03).clamp(0.3, 1.0);
        if fade <= 0.0 {
            continue;
        }
        let center = w.origin + w.dir * d;
        let hw = tuning.wave_half_width * w.size;
        let th = tuning.wave_thick * w.size.sqrt();
        let (body, body_em, edge, edge_em) = if w.dark { ([0.03, 0.0, 0.02], 0.25, [1.0, 0.05, 0.05], 7.0) } else { (w.col, 3.5, hot(w.col, 0.8), 10.0) };
        const N: usize = 14;
        // Trail: fading copies behind.
        for k in (1..=4).rev() {
            let back = (k as f32 * 0.55 * w.size.sqrt()).min(d);
            let c = center - w.dir * back;
            let kf = 1.0 - k as f32 / 5.0;
            let mut prev = crescent(c, w.dir, w.axis, hw * (0.9 + 0.1 * kf), -1.0);
            for i in 1..=N {
                let f = -1.0 + 2.0 * i as f32 / N as f32;
                let p = crescent(c, w.dir, w.axis, hw * (0.9 + 0.1 * kf), f);
                seg(fx, prev, p, th * 0.5 * kf * (1.0 - f * f * 0.6), if w.dark { edge } else { body }, (if w.dark { 3.0 } else { body_em }) * kf * fade * 0.6);
                prev = p;
            }
        }
        // Body + white-hot leading edge (Shadow: black body, red rim both sides).
        let mut prev = crescent(center, w.dir, w.axis, hw, -1.0);
        for i in 1..=N {
            let f = -1.0 + 2.0 * i as f32 / N as f32;
            let p = crescent(center, w.dir, w.axis, hw, f);
            let taper = 1.0 - f * f * 0.7;
            seg(fx, prev, p, th * taper, body, body_em * fade);
            let lead = w.dir * th * 0.45 * taper;
            seg(fx, prev + lead, p + lead, th * 0.28 * taper, edge, edge_em * fade);
            if w.dark {
                seg(fx, prev - lead, p - lead, th * 0.2 * taper, edge, edge_em * 0.7 * fade);
            }
            prev = p;
        }
        if fx.remaining() > 400 && hf(w.origin.x.to_bits(), (w.t * 60.0) as u32) < 0.6 {
            let f = rng.range(-1.0, 1.0);
            let p = crescent(center, w.dir, w.axis, hw, f);
            spark(fx, p, -w.dir * 3.0 + rng.dir(), edge, 0.05, 0.25, 5.0, 0.2);
        }
    }
    st.waves.retain(|w| w.speed * w.t < w.range + w.speed * 0.12);

    // ---- storm telegraphs (+ rain drops falling onto their marks)
    for m in st.marks.iter_mut() {
        m.t += dt;
        let u = (m.t / m.land).clamp(0.0, 1.0);
        let pulse = 0.6 + 0.4 * (m.t * (8.0 + 20.0 * u)).sin().abs();
        let g = m.pos + Vec3::Y * 0.1;
        ring(fx, g, Vec3::Y, m.radius, 0.05, m.col, 3.0 * pulse, now);
        ring(fx, g, Vec3::Y, m.radius * u.max(0.05), 0.08, hot(m.col, 0.4), 5.0 * pulse, -now);
        let rem = m.land - m.t;
        if m.kind.rains() {
            if rem < DROP_SECS && rem > -0.02 {
                let k = (rem / DROP_SECS).max(0.0);
                let from = m.pos + Vec3::new(3.0, DROP_H, 2.0);
                let at = m.pos + (from - m.pos) * k;
                let fall = (m.pos - from).normalize();
                if m.kind == StormKind::Meteors {
                    let rot = Quat::from_rotation_arc(Vec3::Y, -fall) * Quat::from_rotation_y(now * 7.0);
                    cube(fx, at, rot, Vec3::splat(0.9), [1.0, 0.45, 0.08], 6.0);
                    cube(fx, at, rot, Vec3::splat(0.6), [1.0, 0.9, 0.5], 10.0);
                    seg(fx, at, at - fall * 5.0, 0.45, [1.0, 0.35, 0.05], 3.0);
                    if fx.remaining() > 300 {
                        spark(fx, at, -fall * 4.0 + rng.dir() * 2.0, [1.0, 0.6, 0.15], 0.12, 0.35, 5.0, -0.2);
                    }
                } else {
                    seg(fx, at - fall * 1.2, at + fall * 1.2, 0.18, hot(m.col, 0.5), 5.0);
                    seg(fx, at - fall * 3.5, at - fall * 1.2, 0.05, hot(m.col, 0.7), 2.5);
                }
            }
        } else if m.kind == StormKind::Bolt && rem < 0.25 {
            // The sky charges: a thin flicker column.
            let k = 1.0 - rem / 0.25;
            if hf(m.pos.x.to_bits(), (now * 30.0) as u32) < 0.5 + 0.5 * k {
                bolt(fx, m.pos + Vec3::Y * 40.0, m.pos + Vec3::Y * (40.0 - 30.0 * k), 0.03, hot(m.col, 0.6), 4.0, m.pos.z.to_bits(), now);
            }
        }
    }
    st.marks.retain(|m| m.t < m.land + 0.08);

    // ---- strikes
    for s in st.strikes.iter_mut() {
        s.t += dt;
        let u = (s.t / s.dur).clamp(0.0, 1.0);
        draw_strike(fx, s, u, now, &tuning);
    }
    st.strikes.retain(|s| s.t < s.dur);

    // ---- generic rings, glints, streaks
    for r in st.rings.iter_mut() {
        r.t += dt;
        if r.t < 0.0 {
            continue;
        }
        let u = (r.t / r.dur).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - u) * (1.0 - u);
        ring(fx, r.pos, r.n, r.r0 + (r.r1 - r.r0) * e, r.thick * (1.0 - 0.6 * u), r.col, r.em * (1.0 - u), 0.0);
    }
    st.rings.retain(|r| r.t < r.dur);
    for g in st.glints.iter_mut() {
        g.t += dt;
        let u = (g.t / g.dur).clamp(0.0, 1.0);
        let s = g.size * (1.0 - u) * (0.4 + 0.6 * (u * PI).sin().max(0.3));
        let rot = Quat::from_rotation_y(now * 3.0);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let a = rot * axis * s * 0.5;
            seg(fx, g.pos - a, g.pos + a, 0.03 * g.size.sqrt(), g.col, 10.0 * (1.0 - u));
        }
    }
    st.glints.retain(|g| g.t < g.dur);
    for s in st.streaks.iter_mut() {
        s.t += dt;
        let u = (s.t / s.dur).clamp(0.0, 1.0);
        seg(fx, s.from, s.to, s.thick * (1.0 - 0.5 * u), s.col, s.em * (1.0 - u) * (1.0 - u));
    }
    st.streaks.retain(|s| s.t < s.dur);

    // ---- leap crater fissures
    for f in st.fissures.iter_mut() {
        f.t += dt;
        let u = (f.t / f.dur).clamp(0.0, 1.0);
        let grow = (f.t / 0.1).clamp(0.0, 1.0);
        let em = 5.0 * (1.0 - u) * (1.0 - u);
        let side = f.dir.cross(Vec3::Y).normalize_or(Vec3::X);
        let n = 9;
        let mut prev = f.from + Vec3::Y * 0.08;
        for i in 1..=n {
            let k = i as f32 / n as f32 * grow;
            let jitter = side * (hf(f.seed, i) - 0.5) * 0.7;
            let p = f.from + f.dir * f.len * k + jitter + Vec3::Y * 0.08;
            seg(fx, prev, p, 0.12 * (1.0 - 0.5 * k), f.col, em);
            if i % 3 == 0 {
                let s = if hash(f.seed, i) & 1 == 0 { side } else { -side };
                seg(fx, p, p + (s + f.dir * 0.3).normalize() * 1.3 * grow, 0.06, f.col, em * 0.7);
            }
            prev = p;
        }
    }
    st.fissures.retain(|f| f.t < f.dur);

    // ---- chain blades: the 360° sweep and the pull
    for s in st.sweeps.iter_mut() {
        s.t += dt;
        let u = (s.t / s.dur).clamp(0.0, 1.0);
        let a = u * TAU;
        let rot = |ang: f32| Quat::from_rotation_y(-ang);
        let hand = views.player(s.owner).map_or(s.center + Vec3::Y * 1.1, |v| v.pos + Vec3::Y * 1.1 * v.size);
        let tip = hand + rot(a) * s.heading * s.radius;
        chain(fx, hand, tip, hot(s.col, 0.2), 3.0);
        let blade_dir = (rot(a) * s.heading).normalize_or(Vec3::NEG_Z);
        let tan = Vec3::Y.cross(blade_dir);
        cube(fx, tip + blade_dir * 0.35, Quat::from_rotation_arc(Vec3::Z, blade_dir), Vec3::new(0.14, 0.05, 0.8), hot(s.col, 0.5), 7.0);
        // The element trail: the arc the blade just swept.
        let steps = 16;
        let mut prev = tip;
        for i in 1..=steps {
            let back = a - i as f32 / steps as f32 * 1.6;
            if back < 0.0 {
                break;
            }
            let p = hand + rot(back) * s.heading * s.radius;
            let k = 1.0 - i as f32 / steps as f32;
            seg(fx, prev, p, 0.25 * k + 0.03, s.col, 5.0 * k * (1.0 - u * 0.5));
            prev = p;
        }
        if fx.remaining() > 300 {
            for _ in 0..4 {
                let v = -tan * 3.0 + rng.dir() * 1.5 + Vec3::Y * 1.0;
                let c = if s.element == Element::Fire && rng.f() < 0.5 { [1.0, 0.75, 0.2] } else { s.col };
                spark(fx, tip, v, c, 0.09, 0.45, 5.0, -0.3);
            }
        }
    }
    st.sweeps.retain(|s| s.t < s.dur);
    for p in st.pulls.iter_mut() {
        p.t += dt;
        let u = (p.t / p.dur).clamp(0.0, 1.0);
        if let Some(h) = hand_of(p.owner) {
            // Out fast, taut, then released.
            let reach = (p.t / 0.06).clamp(0.0, 1.0);
            chain(fx, h, h + (p.to - h) * reach, hot(p.col, 0.25), 3.5 * (1.0 - u * 0.7));
        }
    }
    st.pulls.retain(|p| p.t < p.dur);

    // ---- lodged items: a glowing crack ring where they bit
    for l in st.lodged.iter_mut() {
        l.t += dt;
        let k = (1.0 - l.t / (tuning.lodge_secs + 1.0)).clamp(0.0, 1.0);
        let pulse = 0.6 + 0.4 * (now * 5.0).sin().abs();
        ring(fx, l.pos + l.n * 0.04, l.n, 0.7, 0.05, l.col, 3.0 * k * pulse, now * 0.5);
        let (u, v) = perp(l.n);
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + hf(l.id.0, i) * 0.6;
            let dir = u * a.cos() + v * a.sin();
            seg(fx, l.pos + l.n * 0.04 + dir * 0.2, l.pos + l.n * 0.04 + dir * (0.9 + hf(l.id.0, i + 9) * 0.8), 0.04, hot(l.col, 0.3), 2.5 * k);
        }
    }
    st.lodged.retain(|l| l.t < tuning.lodge_secs + 1.0 && views.construct(l.id).is_none_or(|v| v.anim == AnimState::Stuck as u8));

    // ---- per held construct: flying trails, storm raise crackle, idle flourishes, guns
    for (sc, tf, vis) in &items {
        if sc.category != FormCategory::Held {
            continue;
        }
        let Some(c) = active.world.constructs.get(sc.id) else { continue };
        let Some(cv) = views.construct(sc.id) else { continue };
        let col = item_col(&active, Some(c), sc.owner);
        let (grip, tip) = item_axis(tf, vis);
        let anim = cv.anim;
        let melee = legend::melee_item(c);
        // Flying Returning items leave an element streak (thicker and brighter while recalled).
        if melee && c.has(Mechanic::Returning) && !cv.attached() && anim != AnimState::Stuck as u8 {
            let centre = (grip + tip) * 0.5;
            let recalled = st.moves.iter().any(|m| m.id == sc.id && m.kind == MoveKind::Recall);
            let trail = match st.trails.iter_mut().position(|t| t.0 == sc.id) {
                Some(i) => &mut st.trails[i].1,
                None => {
                    st.trails.push((sc.id, Vec::with_capacity(12)));
                    &mut st.trails.last_mut().unwrap().1
                }
            };
            trail.push(centre);
            if trail.len() > 10 {
                trail.remove(0);
            }
            let n = trail.len();
            for i in 1..n {
                let k = i as f32 / n as f32;
                let (th, em) = if recalled { (0.22 * k, 7.0 * k) } else { (0.1 * k, 3.5 * k) };
                seg(fx, trail[i - 1], trail[i], th, col, em);
                if recalled {
                    seg(fx, trail[i - 1], trail[i], th * 0.35, hot(col, 0.8), em * 1.6);
                }
            }
            if c.recipe.element == Element::Electric && fx.remaining() > 300 && hf(sc.id.0, (now * 20.0) as u32) < 0.5 {
                let a = centre + rng.dir() * 0.6;
                bolt(fx, centre, a, 0.02, hot(col, 0.6), 6.0, sc.id.0 ^ 77, now);
            }
        } else if let Some(i) = st.trails.iter().position(|t| t.0 == sc.id) {
            st.trails.swap_remove(i);
        }
        // Storm raise: the element gathers on the raised item.
        if anim == AnimState::Telegraph as u8 && legend::storms(c) {
            let head = tip;
            let kind = StormKind::of(c.recipe.element).unwrap_or(StormKind::Bolt);
            if kind == StormKind::Bolt {
                for k in 0..3u32 {
                    if hf(sc.id.0 + k, (now * 18.0) as u32) < 0.7 {
                        let off = Vec3::new(hf(k, (now * 18.0) as u32) - 0.5, hf(k + 5, (now * 18.0) as u32), hf(k + 9, (now * 18.0) as u32) - 0.5) * 1.4;
                        bolt(fx, head, head + off, 0.025, hot(col, 0.5), 7.0, sc.id.0 * 31 + k, now);
                    }
                }
                // The sky answers: a bolt drops onto the item now and then.
                if hf(sc.id.0 ^ 0xABC, (now * 5.0) as u32) < 0.45 && (now * 5.0).fract() < 0.35 {
                    bolt(fx, head + Vec3::new(1.5, 30.0, -1.0), head, 0.06, hot(col, 0.7), 9.0, sc.id.0 ^ (now * 5.0) as u32, now);
                }
            } else if fx.remaining() > 300 {
                for _ in 0..3 {
                    let a = rng.range(0.0, TAU);
                    let p = head + Vec3::new(a.cos(), -0.3, a.sin()) * 0.9;
                    let v = (head - p) * 2.0 + Vec3::Y * 1.5;
                    spark(fx, p, v, hot(col, rng.f() * 0.4), 0.07, 0.4, 5.0, -0.3);
                }
            }
            ring(fx, head, Vec3::Y, 0.5 + 0.1 * (now * 9.0).sin(), 0.03, hot(col, 0.4), 4.0, now * 3.0);
        }
        // Idle flourishes (generic by combination).
        if melee {
            let len = grip.distance(tip);
            if c.recipe.element == Element::Electric && anim != AnimState::Telegraph as u8 {
                let w = (now * 9.0) as u32;
                if hf(sc.id.0, w) < 0.45 {
                    let a = grip.lerp(tip, 0.3 + 0.7 * hf(sc.id.0 + 1, w));
                    let b = grip.lerp(tip, 0.3 + 0.7 * hf(sc.id.0 + 2, w)) + Vec3::new(hf(w, 3) - 0.5, hf(w, 4) - 0.5, hf(w, 5) - 0.5) * len * 0.5;
                    bolt(fx, a, b, 0.012 + 0.01 * len, hot(col, 0.6), 7.0, sc.id.0 ^ w, now);
                }
            }
            if c.recipe.form == Form::Shield && c.has(Mechanic::Returning) {
                // A glint sweeps across the face every few seconds.
                let cyc = (now + sc.id.0 as f32 * 0.37) % 2.6;
                if cyc < 0.3 {
                    let b = &vis.model.bounds;
                    let x = b.min.x + (b.max.x - b.min.x) * (cyc / 0.3);
                    let p = tf.transform_point(Vec3::new(x, b.max.y * 0.3 + b.min.y * 0.7 + (b.max.y - b.min.y) * (cyc / 0.3) * 0.5, b.min.z));
                    let s = (tf.scale.x * (b.max.x - b.min.x) * 0.25).clamp(0.08, 0.4) * ((cyc / 0.3) * PI).sin();
                    for axis in [Vec3::X, Vec3::Y] {
                        let a = tf.rotation * axis * s;
                        seg(fx, p - a, p + a, 0.015, [1.0, 1.0, 1.0], 12.0);
                    }
                }
            }
            if held_blade(c) && c.has(Mechanic::Fast) && c.recipe.form != Form::Whip {
                // Lightsaber: a white-hot core down the blade inside a coloured glow.
                let from = grip.lerp(tip, 0.28);
                let flick = 0.9 + 0.1 * (now * 41.0).sin();
                let th = (len * 0.035).clamp(0.006, 0.06);
                seg(fx, from, tip, th, [1.0, 1.0, 1.0], 8.0 * flick);
                seg(fx, from, tip, th * 2.4, col, 2.5 * flick);
            }
            if c.recipe.form == Form::Whip && c.has(Mechanic::Blade) && fx.remaining() > 400 && hf(sc.id.0, (now * 30.0) as u32) < 0.5 {
                let b = &vis.model.bounds;
                let lp = Vec3::new(rng.range(b.min.x, b.max.x), rng.range(b.min.y, b.max.y), rng.range(b.min.z, b.max.z));
                let p = tf.transform_point(lp);
                let hotc = if c.recipe.element == Element::Fire && rng.f() < 0.5 { [1.0, 0.75, 0.2] } else { col };
                spark(fx, p, Vec3::Y * 0.8 + rng.dir() * 0.3, hotc, 0.05 * tf.scale.x.max(0.3), 0.4, 5.0, -0.4);
            }
            if anim == AnimState::Charging as u8 && legend::heavy_blade(c) && fx.remaining() > 400 && hf(sc.id.0, (now * 20.0) as u32) < 0.5 {
                // Armored charge: silver motes drawn to the blade.
                let p = grip.lerp(tip, rng.f()) + rng.dir() * 0.7;
                spark(fx, p, (grip.lerp(tip, 0.6) - p) * 3.0, SILVER, 0.04, 0.25, 4.0, 0.0);
            }
        }
        // ---- guns
        if bd_sim::constructs::gun::is_gun(c) {
            let muzzle = tip;
            if legend::stream_gun(c) && anim == AnimState::Active as u8 {
                let victim = st.streams.iter().find(|s| s.gun == sc.id).and_then(|s| s.victim).and_then(|v| views.player(v)).map(|v| v.pos + Vec3::Y * 0.9 * v.size);
                let end = victim.unwrap_or_else(|| {
                    let look = views.player(sc.owner).map_or(Vec3::NEG_Z, |v| bv(bd_core::input::look_dir(v.yaw, v.pitch)));
                    let from = super::gv(muzzle);
                    let hit = bd_core::voxel::raycast(&active.world.voxels, from, super::gv(look), tuning.stream_range).map_or(tuning.stream_range, |h| h.t);
                    muzzle + look * hit
                });
                draw_stream(fx, muzzle, end, col, now, sc.id.0, victim.is_some());
            }
            if let Some(g) = st.gravs.iter().find(|g| g.gun == sc.id) {
                let target = match g.what {
                    Some(Held::Player(p)) => views.player(p).map(|v| v.pos + Vec3::Y * 0.9 * v.size),
                    Some(Held::Construct(k)) => active.world.constructs.get(k).map(|k| bv(k.center())),
                    None => None,
                };
                if let Some(t) = target {
                    draw_tractor(fx, muzzle, t, now);
                }
            }
            if let Some(ch) = st.charges.iter().find(|x| x.gun == sc.id) {
                let full_t = if ch.bfg { tuning.bfg_spinup } else { tuning.charge_full };
                let k = (ch.t / full_t.max(0.05)).clamp(0.0, 1.0);
                let r = if ch.bfg { 0.08 + 0.45 * k } else { 0.05 + 0.22 * k * (0.5 + 0.5 * tuning.charge_size_max.min(3.0) / 2.4) };
                let r = r * (tf.scale.x / 1.0).clamp(0.2, 1.0).max(0.35);
                draw_orb(fx, muzzle, r, ch.col, now, ch.full || k >= 1.0);
                if fx.remaining() > 400 {
                    let p = muzzle + rng.dir() * (r * 5.0);
                    spark(fx, p, (muzzle - p) * 4.0, hot(ch.col, 0.4), 0.03, 0.22, 5.0, 0.0);
                }
            }
        }
    }
    for g in st.gravs.iter_mut() {
        g.t += dt;
    }
    st.gravs.retain(|g| g.t < g.max);
    for s in st.streams.iter_mut() {
        s.t += dt;
    }
    st.streams.retain(|s| s.t < tuning.stream_secs + 0.5 && views.construct(s.gun).is_some_and(|v| v.anim == AnimState::Active as u8 || s.t < 0.15));
    for ch in st.charges.iter_mut() {
        ch.t += dt;
    }
    st.charges.retain(|c| c.t < 4.0 && views.construct(c.gun).is_some());

    // ---- needles stuck in their victims
    for n in st.needles.iter_mut() {
        n.age += dt;
    }
    st.needles.retain(|n| n.age < tuning.needle_secs && views.player(n.victim).is_some_and(|v| v.alive()));
    for (i, n) in st.needles.iter().enumerate() {
        let Some(v) = views.player(n.victim) else { continue };
        let at = v.pos + n.off;
        let rot = Quat::from_rotation_arc(Vec3::Z, n.dir);
        let wob = 0.85 + 0.15 * (now * 12.0 + i as f32).sin();
        cube(fx, at - n.dir * 0.12, rot, Vec3::new(0.05, 0.05, 0.34), PINK, 5.0 * wob);
        cube(fx, at - n.dir * 0.2, rot, Vec3::new(0.022, 0.022, 0.24), hot(PINK, 0.7), 9.0 * wob);
    }

    // ---- BFG orbs in flight: big, green, alive
    for pr in &active.world.projectiles {
        if pr.dead {
            continue;
        }
        let bfg = pr.spec.construct.and_then(|id| active.world.constructs.get(id)).is_some_and(legend::bfg);
        if !bfg {
            continue;
        }
        let p = bv(pr.pos);
        draw_orb(fx, p, tuning.bfg_radius.max(0.4), BFG_GREEN, now + pr.id as f32, true);
        if fx.remaining() > 300 {
            for _ in 0..3 {
                spark(fx, p + rng.dir() * tuning.bfg_radius, rng.dir() * 2.0 - bv(pr.vel) * 0.05, hot(BFG_GREEN, rng.f() * 0.5), 0.08, 0.4, 6.0, 0.0);
            }
        }
    }
}

fn held_blade(c: &Construct) -> bool {
    c.has(Mechanic::Blade) || matches!(c.recipe.form, Form::Sword | Form::Axe)
}

/// A crackling proton stream: two strands winding around the line + a white core, arcs off it.
fn draw_stream(fx: &mut FxParticles, a: Vec3, b: Vec3, col: [f32; 3], now: f32, seed: u32, latched: bool) {
    let d = b - a;
    let len = d.length();
    if len < 0.2 {
        return;
    }
    let dir = d / len;
    let (u, v) = perp(dir);
    let n = ((len / 0.4) as usize).clamp(6, 70);
    for strand in 0..2 {
        let ph = strand as f32 * PI;
        let mut prev = a;
        for i in 1..=n {
            let f = i as f32 / n as f32;
            let amp = 0.18 * (f * PI).sin().max(0.15) * (1.0 + 0.4 * (now * 23.0 + f * 9.0).sin());
            let ang = f * len * 1.6 - now * 18.0 + ph;
            let p = a + d * f + (u * ang.cos() + v * ang.sin()) * amp;
            seg(fx, prev, p, 0.05, if strand == 0 { col } else { hot(col, 0.3) }, 5.0);
            prev = p;
        }
    }
    seg(fx, a, b, 0.025, [1.0, 1.0, 1.0], 9.0);
    let w = (now * 20.0) as u32;
    for k in 0..2u32 {
        if hf(seed + k, w) < 0.6 {
            let f = hf(seed + k + 10, w);
            let p = a + d * f;
            let off = (u * (hf(seed + k, w + 1) - 0.5) + v * (hf(seed + k, w + 2) - 0.5)) * 2.0;
            bolt(fx, p, p + off, 0.015, [0.6, 0.8, 1.0], 8.0, seed ^ w ^ k, now);
        }
    }
    if latched {
        ring(fx, b, dir, 0.5 + 0.15 * (now * 14.0).sin(), 0.04, hot(col, 0.4), 6.0, now * 4.0);
    }
}

/// The gravity gun's tractor beam: strands spiralling in to the held thing.
fn draw_tractor(fx: &mut FxParticles, a: Vec3, b: Vec3, now: f32) {
    let d = b - a;
    let len = d.length();
    if len < 0.2 {
        return;
    }
    let dir = d / len;
    let (u, v) = perp(dir);
    let n = ((len / 0.3) as usize).clamp(6, 40);
    for strand in 0..3 {
        let ph = strand as f32 * TAU / 3.0;
        let mut prev = a;
        for i in 1..=n {
            let f = i as f32 / n as f32;
            let ang = ph + f * 5.0 - now * 9.0;
            let amp = 0.08 + 0.35 * f;
            let p = a + d * f + (u * ang.cos() + v * ang.sin()) * amp;
            seg(fx, prev, p, 0.03, TRACTOR, 4.0 * (0.5 + 0.5 * f));
            prev = p;
        }
    }
    seg(fx, a, b, 0.015, hot(TRACTOR, 0.6), 5.0);
    ring(fx, b, dir, 0.9 + 0.1 * (now * 10.0).sin(), 0.035, TRACTOR, 5.0, now * 2.0);
}

/// A glowing orb of rotated cubes (charge orb, BFG round).
fn draw_orb(fx: &mut FxParticles, c: Vec3, r: f32, col: [f32; 3], now: f32, full: bool) {
    let pulse = if full { 1.0 + 0.2 * (now * 16.0).sin() } else { 1.0 };
    let r = r * pulse;
    for k in 0..3 {
        let rot = Quat::from_euler(EulerRot::XYZ, now * (1.3 + k as f32), now * (2.1 - k as f32 * 0.5), k as f32 * 0.7);
        cube(fx, c, rot, Vec3::splat(r * (1.4 - k as f32 * 0.3)), col, 2.0 + k as f32 * 1.5);
    }
    cube(fx, c, Quat::from_rotation_y(now * 4.0), Vec3::splat(r * 0.7), hot(col, 0.8), if full { 12.0 } else { 7.0 });
}

fn draw_strike(fx: &mut FxParticles, s: &Strike, u: f32, now: f32, t: &legend::LegendTuning) {
    let p = s.pos;
    let r = s.radius;
    let col = s.col;
    let mut rng = R::new(s.seed ^ (now * 1000.0) as u32);
    match s.kind {
        StormKind::Bolt => {
            let k = 1.0 - u;
            let top = p + Vec3::new(2.0, 45.0, -1.5);
            let flick = if (now * 30.0).fract() < 0.7 { 1.0 } else { 0.35 };
            bolt(fx, top, p, 0.35 * k + 0.05, hot(col, 0.4), 6.0 * k * flick, s.seed, now);
            bolt(fx, top, p, 0.12 * k + 0.03, [1.0, 1.0, 1.0], 14.0 * k * flick, s.seed, now);
            for b in 0..3u32 {
                let f = 0.35 + 0.18 * b as f32;
                let from = top.lerp(p, f);
                let off = Vec3::new(hf(s.seed, b) - 0.5, -0.6, hf(s.seed, b + 7) - 0.5) * 10.0;
                bolt(fx, from, from + off, 0.05 * k, hot(col, 0.5), 5.0 * k * flick, s.seed ^ (b + 1), now);
            }
            ring(fx, p + Vec3::Y * 0.1, Vec3::Y, r * (0.4 + 0.8 * u), 0.12 * k, hot(col, 0.5), 7.0 * k, 0.0);
        }
        StormKind::Meteors => {
            let k = 1.0 - u;
            cube(fx, p + Vec3::Y * 0.3, Quat::IDENTITY, Vec3::new(r * 0.5, 0.2, r * 0.5) * (1.0 - 0.5 * u), [1.0, 0.5, 0.1], 5.0 * k);
        }
        StormKind::Icicles => {
            let grow = (u * s.dur / 0.07).clamp(0.0, 1.0);
            let shrink = ((1.0 - u) * s.dur / 0.3).clamp(0.0, 1.0);
            for i in 0..5u32 {
                let a = hf(s.seed, i) * TAU;
                let tilt = Vec3::new(a.cos() * 0.5, 1.0, a.sin() * 0.5).normalize();
                let base = p + Vec3::new(a.cos(), 0.0, a.sin()) * hf(s.seed, i + 5) * r * 0.5;
                let h = (1.0 + hf(s.seed, i + 11) * 1.4) * grow * shrink;
                let rot = Quat::from_rotation_arc(Vec3::Y, tilt);
                cube(fx, base + tilt * h * 0.5, rot, Vec3::new(0.18, h, 0.18), hot(col, 0.3), 2.5);
                cube(fx, base + tilt * h * 0.6, rot, Vec3::new(0.07, h * 0.8, 0.07), hot(col, 0.8), 5.0);
            }
        }
        StormKind::Tornado => {
            let fade = (u * s.dur / 0.2).clamp(0.0, 1.0) * ((1.0 - u) * s.dur / 0.25).clamp(0.0, 1.0);
            let rr = t.storm_radius;
            for h in 0..14 {
                let y = h as f32 * 0.75;
                let rad = (0.35 + y * 0.22) * rr / 3.5 * (0.9 + 0.1 * (now * 7.0 + h as f32).sin());
                let sway = Vec3::new((now * 2.0 + y * 0.3).sin(), 0.0, (now * 1.7 + y * 0.25).cos()) * y * 0.08;
                ring(fx, p + Vec3::Y * y + sway, Vec3::Y, rad, 0.06, if h % 2 == 0 { col } else { hot(col, 0.4) }, 2.5 * fade, now * 7.0 + h as f32 * 0.4);
            }
            if fx.remaining() > 300 {
                for _ in 0..4 {
                    let a = rng.range(0.0, TAU);
                    let y = rng.range(0.0, 9.0);
                    let rad = (0.35 + y * 0.22) * rr / 3.5;
                    let q = p + Vec3::new(a.cos() * rad, y, a.sin() * rad);
                    let tan = Vec3::new(-a.sin(), 0.3, a.cos()) * 9.0;
                    fx.emit(Emit { kind: PKind::Ballistic, flags: PFlags::PLAIN, pos: q, vel: tan, size: 0.1, color: [0.6, 0.55, 0.45], emissive: 0.6, life: 0.35, gravity: -0.5, drag: 0.5, ..default() });
                }
            }
        }
        StormKind::Geyser => {
            let rise = (u * s.dur / 0.15).clamp(0.0, 1.0);
            let fall = ((1.0 - u) * s.dur / 0.5).clamp(0.0, 1.0);
            let h = 9.0 * rise * (0.4 + 0.6 * fall);
            let w = 1.2 * r / 3.5;
            for i in 0..6 {
                let f = i as f32 / 6.0;
                let y = h * f;
                let wob = Vec3::new((now * 13.0 + f * 7.0).sin(), 0.0, (now * 11.0 + f * 5.0).cos()) * 0.12;
                cube(fx, p + Vec3::Y * (y + h / 12.0) + wob, Quat::from_rotation_y(now * 3.0 + f), Vec3::new(w * (1.0 - f * 0.3), h / 6.0 + 0.1, w * (1.0 - f * 0.3)), hot(col, f * 0.5), 2.2 * fall);
            }
            cube(fx, p + Vec3::Y * h, Quat::from_rotation_y(now * 5.0), Vec3::splat(w * 1.3 * fall), hot(col, 0.6), 4.0 * fall);
        }
        StormKind::Crush => {
            let drop = (u * s.dur / 0.12).clamp(0.0, 1.0);
            let y = 6.0 * (1.0 - drop * drop);
            let k = 1.0 - u;
            for i in 0..3 {
                ring(fx, p + Vec3::Y * (y + 0.1 + i as f32 * 0.08), Vec3::Y, r * (1.0 - 0.15 * i as f32), 0.12, hot(col, 0.2 * i as f32), 6.0 * k, 0.0);
            }
            cube(fx, p + Vec3::Y * (y + 0.05), Quat::IDENTITY, Vec3::new(r * 1.3, 0.06, r * 1.3), col, 1.5 * k);
        }
        StormKind::Spikes => {
            let grow = (u * s.dur / 0.08).clamp(0.0, 1.0);
            let sink = ((1.0 - u) * s.dur / 0.35).clamp(0.0, 1.0);
            for i in 0..8u32 {
                let a = hf(s.seed, i) * TAU;
                let d = if i == 0 { 0.0 } else { (0.3 + 0.7 * hf(s.seed, i + 3)) * r * 0.8 };
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                let tilt = (Vec3::Y + out * (0.15 + 0.35 * d / r)).normalize();
                let h = (1.5 + 2.0 * hf(s.seed, i + 17)) * grow * sink;
                let rot = Quat::from_rotation_arc(Vec3::Y, tilt) * Quat::from_rotation_y(a);
                let base = p + out * d;
                cube(fx, base + tilt * h * 0.45, rot, Vec3::new(0.55, h, 0.55), [0.45, 0.3, 0.16], 0.35);
                cube(fx, base + tilt * h * 0.85, rot, Vec3::new(0.3, h * 0.3, 0.3), hot(col, 0.1), 1.8);
            }
        }
        StormKind::Maw => {
            let open = (u * s.dur / 0.2).clamp(0.0, 1.0) * ((1.0 - u) * s.dur / 0.25).clamp(0.0, 1.0);
            let rr = r;
            for k in 0..5 {
                ring(fx, p + Vec3::Y * 0.06, Vec3::Y, rr * (0.2 + 0.18 * k as f32) * open, 0.2, [0.02, 0.0, 0.03], 0.3, now * (k as f32 - 2.0));
            }
            ring(fx, p + Vec3::Y * 0.08, Vec3::Y, rr * open, 0.08, col, 6.0 * (0.7 + 0.3 * (now * 8.0).sin()), 0.0);
            let bite = 0.5 + 0.5 * (now * 6.0).sin();
            for i in 0..12 {
                let a = i as f32 / 12.0 * TAU;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                let base = p + out * rr * open;
                let tipd = (Vec3::Y * 1.6 - out * (0.4 + 0.9 * bite)).normalize();
                let h = 1.4 * open;
                let rot = Quat::from_rotation_arc(Vec3::Y, tipd);
                cube(fx, base + tipd * h * 0.5, rot, Vec3::new(0.22, h, 0.22), [0.03, 0.0, 0.03], 0.3);
                cube(fx, base + tipd * h, rot, Vec3::splat(0.14), col, 7.0);
            }
            if fx.remaining() > 300 {
                let a = rng.range(0.0, TAU);
                let q = p + Vec3::new(a.cos(), 0.2, a.sin()) * rr * 1.4;
                spark(fx, q, (p - q) * 2.0 + Vec3::Y, [0.05, 0.0, 0.05], 0.12, 0.5, 0.2, 0.0);
            }
        }
        StormKind::Nova => {
            let e = 1.0 - (1.0 - u) * (1.0 - u);
            let rr = r * (0.2 + 1.1 * e);
            let k = (1.0 - u) * 9.0;
            let c = p + Vec3::Y * 0.5;
            ring(fx, c, Vec3::Y, rr, 0.15, hot(col, 0.5), k, 0.0);
            ring(fx, c, Vec3::X, rr, 0.1, col, k * 0.7, 0.0);
            ring(fx, c, Vec3::Z, rr, 0.1, col, k * 0.7, 0.0);
            cube(fx, c, Quat::from_rotation_y(now), Vec3::splat(rr * 0.5 * (1.0 - u)), [1.0, 1.0, 0.9], 12.0 * (1.0 - u));
        }
        StormKind::Miasma => {
            let fade = ((1.0 - u) * s.dur / 0.4).clamp(0.0, 1.0);
            ring(fx, p + Vec3::Y * 0.08, Vec3::Y, r, 0.06, col, 2.0 * fade, now * 0.5);
            if fx.remaining() > 300 && hf(s.seed, (now * 40.0) as u32) < 0.7 {
                let a = rng.range(0.0, TAU);
                let d = rng.f().sqrt() * r;
                let q = p + Vec3::new(a.cos() * d, 0.3, a.sin() * d);
                puff(fx, q, Vec3::Y * 0.4 + rng.dir() * 0.3, scale3(col, 0.8 + 0.4 * rng.f()), 0.5 + rng.f() * 0.6, 1.2, 0.9);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crescent_matches_the_sim_samples() {
        let (c, d) = (bd_core::Vec3::new(1.0, 2.0, 3.0), bd_core::Vec3::new(0.0, 0.0, -1.0));
        for vertical in [false, true] {
            let axis = legend::wave_axis(d, vertical);
            for k in -2..=2 {
                let a = bv(legend::wave_point(c, d, axis, 1.3, k));
                let b = crescent(bv(c), bv(d), bv(axis), 1.3, k as f32 / 2.0);
                assert!(a.distance(b) < 1e-4, "{a} vs {b}");
            }
        }
    }

    #[test]
    fn fp_pose_blends_and_is_finite() {
        let mut p = LegendPose { fp_pose: Some(Pose::Raise), fp_w: 0.5, ..Default::default() };
        let rest = Quat::from_euler(EulerRot::YXZ, -0.85, 0.55, -0.45);
        let t = p.fp_swing(None, rest).unwrap();
        assert!(t.is_finite());
        assert!(t.translation.y > 0.1, "raised");
        p.fp_w = 0.0;
        assert!(p.fp_swing(None, rest).is_none(), "no pose, no override");
        for pose in [Pose::LeapUp, Pose::LeapDive, Pose::Reach, Pose::Snap, Pose::Spin(0.3), Pose::Strain, Pose::Shake, Pose::Tremble(0.5)] {
            let q = LegendPose { fp_pose: Some(pose), fp_w: 1.0, fp_shake: 0.02, time: 1.3, ..Default::default() };
            assert!(q.fp_swing(Some(Transform::IDENTITY), rest).unwrap().is_finite(), "{pose:?}");
        }
    }
}
