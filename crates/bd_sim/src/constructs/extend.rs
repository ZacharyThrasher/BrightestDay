//! **Telescoping weapons** (`Mechanic::Extends`, bd_core v1.7) — the Ruyi Jingu Bang, the power
//! pole, an extending sword, a telescoping spear, a stretchy fist. Wukong's staff grows from a
//! needle to a pillar that reaches the heavens, weighs 13,500 jin and is twirled constantly; this is
//! that, as generic mechanics on any held melee form (guns ignore it).
//!
//! The length is live state ([`HeldState::ext`], metres beyond the model, and
//! [`HeldState::ext_girth`], a thickness factor), replicated in `ConstructSnap::ext` ([`wire`]) so
//! every client draws the pole continuously (`bd_game::present::extend`). Controls reuse the held
//! grammar (`held.rs`); only what is different lives here:
//!
//! * **LMB** — a 3-hit twirling combo (`bd_shapes::swing_curve_extends`: sweep right→left, sweep
//!   left→right, overhead windmill finisher = a heavy hit). Each swing's pole grows
//!   `swing_reach` m at the peak of its active window: big arcs. Juice (hit-stop, sub thump, camera
//!   punch) comes from the connect tiers: combo hits carry a high impact, the finisher is HeavyMelee.
//! * **Hold LMB, release** — the charge coils the pole back (the Heavy curve's `hold_at`). Release:
//!   * **Thrust** (default): the pole shoots out along the aim at `thrust_speed` m/s up to
//!     `thrust_min..thrust_max` m (by charge) — a line strike: every enemy on the line is hit once,
//!     hard; light constructs are punched through, heavy ones and terrain stop it (the tip chips
//!     rock). It holds a beat, then snaps back.
//!   * **Falling pillar** (aimed down past `pillar_look_y`, or `pillar_air_look_y` in the air): the
//!     pole grows `pillar_girth`× thick and `pillar_len` m long straight up (the telegraph), then
//!     crashes down along the aim's heading like a falling pillar: a line of craters where it meets
//!     the ground, a knock-up band along it and a softer shockwave either side.
//! * **RMB (alt-fire)** — aimed at terrain within `vault_reach`: **pole vault**. The pole plants and
//!   extends; the wielder is launched along the aim mirrored off the surface (off the floor: up and
//!   forward; off a wall: away from it). Velocity is only ever *added* along the launch direction, so
//!   it stacks with bunny-hops, slides, tethers and falls. The movement half runs in [`pre_move`]
//!   (host `step_movement` and client `predict_player` alike: predicted exactly; the cooldown lives on
//!   `PlayerState::extra.vault_cd`). Into open air: a quick **jab** (`jab_range` m).
//!   (RMB is the melee alt-fire — the spear's lunge; an Extends item uses it for the vault instead.)
//! * **Dismiss (X)** — unchanged; clients shrink the pole to a needle into the hand with a tink.
//!
//! Creatures with Extends (a summoned Sun Wukong) strike along their aim out to [`CREATURE_REACH`] m
//! with the same line test ([`line_strike`]) and a `HeldFx::Extend` for the visual.
//!
//! Deterministic: pure functions of the construct, its owner's view, the voxels and the tuning.

use std::sync::OnceLock;

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{FormCategory, Mechanic};
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_core::{DT, PlayerId, Quat, Vec3};
use bd_shapes::SwingCurve;
use serde::{Deserialize, Serialize};

use super::held::{HeldState, Mode, Swing, WEIGHT_BINS, bin_weight, weight_bin};
use super::{AnimState, Construct, ConstructOut, ConstructStore, Env, player_key};
use crate::combat::{CarveRequest, ConstructHit, HitInfo, HitKind};
use crate::events::{GameEvent, HeldFx};
use crate::geom;
use crate::movement::{MoveParams, PlayerState};

/// Melee reach (m) of a creature carrying an extending weapon (approach + strike distance).
pub const CREATURE_REACH: f32 = 10.0;

/// What the pole is doing beyond ordinary swings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ExtMove {
    #[default]
    None = 0,
    /// Charged release: shoot out along the aim, hold, snap back.
    Thrust = 1,
    /// Charged release aimed down / in the air: grow huge, stand up, crash down.
    Pillar = 2,
    /// Alt-fire into a surface: plant (the launch itself is [`pre_move`]).
    Vault = 3,
    /// Alt-fire into open air: a quick poke.
    Jab = 4,
}

impl ExtMove {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => ExtMove::Thrust,
            2 => ExtMove::Pillar,
            3 => ExtMove::Vault,
            4 => ExtMove::Jab,
            _ => ExtMove::None,
        }
    }
}

/// `assets/tuning/combat.ron` → `extend` (hot reload; host-synced so prediction uses the same).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtendTuning {
    /// Combo swings: metres the pole grows at the peak of each swing's active window.
    pub swing_reach: f32,
    /// Combo hits 1–2: knockback ×, connect impact.
    pub combo_kb: f32,
    pub combo_impact: f32,
    /// Combo finisher (overhead windmill, a heavy hit): damage ×, knockback ×.
    pub finisher_dmg: f32,
    pub finisher_kb: f32,
    /// Thrust range at the shortest / full charge (m).
    pub thrust_min: f32,
    pub thrust_max: f32,
    /// Tip speed while extending (m/s), hold at full length and snap-back (s).
    pub thrust_speed: f32,
    pub thrust_hold: f32,
    pub thrust_retract: f32,
    /// Thrust line: damage × the form's damage, knockback (m/s, before the Smash % scaling), line
    /// radius (m), connect impact.
    pub thrust_dmg: f32,
    pub thrust_kb: f32,
    pub thrust_radius: f32,
    pub thrust_impact: f32,
    /// Constructs lighter than this (kg) are punched through; heavier ones stop the pole.
    pub thrust_pierce_mass: f32,
    /// Damage to each construct on the line.
    pub thrust_construct_dmg: f32,
    /// Aim at least this far down (look.y) to slam a pillar on the ground / in the air.
    pub pillar_look_y: f32,
    pub pillar_air_look_y: f32,
    /// Pillar length at full charge (m; ×0.7 at the shortest), thickness × the pole's radius.
    pub pillar_len: f32,
    pub pillar_girth: f32,
    /// Stand-up telegraph, the fall, the shrink back (s).
    pub pillar_rise: f32,
    pub pillar_fall: f32,
    pub pillar_recover: f32,
    /// Impact band: half-width (m), damage × the form's damage, knock-up (m/s), connect impact;
    /// the shockwave either side reaches `pillar_shock` × the width at half strength.
    pub pillar_width: f32,
    pub pillar_dmg: f32,
    pub pillar_knockup: f32,
    pub pillar_impact: f32,
    pub pillar_shock: f32,
    /// Craters along the line: radius, spacing (m), carve power.
    pub pillar_crater: f32,
    pub pillar_crater_step: f32,
    pub pillar_crater_power: f32,
    /// Pole vault: reach (m), launch speed along the mirrored aim (m/s), least upward share of a
    /// launch off walkable ground, cooldown (s).
    pub vault_reach: f32,
    pub vault_speed: f32,
    pub vault_min_up: f32,
    pub vault_cd: f32,
    /// Alt-fire into open air: jab length (m), damage × form damage, knockback (m/s), cooldown (s).
    pub jab_range: f32,
    pub jab_dmg: f32,
    pub jab_kb: f32,
    pub jab_cd: f32,
    /// Creature strike (sun wukong): line radius (m), damage ×, knockback (m/s).
    pub creature_radius: f32,
    pub creature_dmg: f32,
    pub creature_kb: f32,
}

impl Default for ExtendTuning {
    fn default() -> Self {
        Self {
            swing_reach: 1.3,
            combo_kb: 1.15,
            combo_impact: 1.35,
            finisher_dmg: 1.35,
            finisher_kb: 1.5,
            thrust_min: 14.0,
            thrust_max: 34.0,
            thrust_speed: 300.0,
            thrust_hold: 0.12,
            thrust_retract: 0.16,
            thrust_dmg: 1.9,
            thrust_kb: 34.0,
            thrust_radius: 0.5,
            thrust_impact: 2.2,
            thrust_pierce_mass: 400.0,
            thrust_construct_dmg: 45.0,
            pillar_look_y: -0.3,
            pillar_air_look_y: -0.1,
            pillar_len: 24.0,
            pillar_girth: 7.0,
            pillar_rise: 0.3,
            pillar_fall: 0.18,
            pillar_recover: 0.45,
            pillar_width: 2.4,
            pillar_dmg: 2.2,
            pillar_knockup: 30.0,
            pillar_impact: 2.6,
            pillar_shock: 1.9,
            pillar_crater: 1.7,
            pillar_crater_step: 2.8,
            pillar_crater_power: 1.0,
            vault_reach: 7.5,
            vault_speed: 21.0,
            vault_min_up: 0.5,
            vault_cd: 0.8,
            jab_range: 9.0,
            jab_dmg: 0.8,
            jab_kb: 16.0,
            jab_cd: 0.55,
            creature_radius: 0.7,
            creature_dmg: 1.2,
            creature_kb: 22.0,
        }
    }
}

impl ExtendTuning {
    pub(crate) fn validated(mut self) -> Self {
        let d = Self::default();
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.swing_reach = fin(self.swing_reach, d.swing_reach, 0.0, 6.0);
        self.combo_kb = fin(self.combo_kb, d.combo_kb, 0.0, 5.0);
        self.combo_impact = fin(self.combo_impact, d.combo_impact, 0.0, 5.0);
        self.finisher_dmg = fin(self.finisher_dmg, d.finisher_dmg, 0.0, 5.0);
        self.finisher_kb = fin(self.finisher_kb, d.finisher_kb, 0.0, 5.0);
        self.thrust_min = fin(self.thrust_min, d.thrust_min, 1.0, 80.0);
        self.thrust_max = fin(self.thrust_max, d.thrust_max, self.thrust_min, 80.0);
        self.thrust_speed = fin(self.thrust_speed, d.thrust_speed, 20.0, 2000.0);
        self.thrust_hold = fin(self.thrust_hold, d.thrust_hold, 0.0, 2.0);
        self.thrust_retract = fin(self.thrust_retract, d.thrust_retract, 0.02, 2.0);
        self.thrust_dmg = fin(self.thrust_dmg, d.thrust_dmg, 0.0, 10.0);
        self.thrust_kb = fin(self.thrust_kb, d.thrust_kb, 0.0, 120.0);
        self.thrust_radius = fin(self.thrust_radius, d.thrust_radius, 0.05, 3.0);
        self.thrust_impact = fin(self.thrust_impact, d.thrust_impact, 0.0, 5.0);
        self.thrust_pierce_mass = fin(self.thrust_pierce_mass, d.thrust_pierce_mass, 0.0, 1.0e7);
        self.thrust_construct_dmg = fin(self.thrust_construct_dmg, d.thrust_construct_dmg, 0.0, 1000.0);
        self.pillar_look_y = fin(self.pillar_look_y, d.pillar_look_y, -1.0, 1.0);
        self.pillar_air_look_y = fin(self.pillar_air_look_y, d.pillar_air_look_y, -1.0, 1.0);
        self.pillar_len = fin(self.pillar_len, d.pillar_len, 2.0, 80.0);
        self.pillar_girth = fin(self.pillar_girth, d.pillar_girth, 1.0, 20.0);
        self.pillar_rise = fin(self.pillar_rise, d.pillar_rise, 0.02, 2.0);
        self.pillar_fall = fin(self.pillar_fall, d.pillar_fall, 0.02, 2.0);
        self.pillar_recover = fin(self.pillar_recover, d.pillar_recover, 0.02, 3.0);
        self.pillar_width = fin(self.pillar_width, d.pillar_width, 0.2, 10.0);
        self.pillar_dmg = fin(self.pillar_dmg, d.pillar_dmg, 0.0, 10.0);
        self.pillar_knockup = fin(self.pillar_knockup, d.pillar_knockup, 0.0, 120.0);
        self.pillar_impact = fin(self.pillar_impact, d.pillar_impact, 0.0, 5.0);
        self.pillar_shock = fin(self.pillar_shock, d.pillar_shock, 1.0, 5.0);
        self.pillar_crater = fin(self.pillar_crater, d.pillar_crater, 0.0, 5.0);
        self.pillar_crater_step = fin(self.pillar_crater_step, d.pillar_crater_step, 0.5, 20.0);
        self.pillar_crater_power = fin(self.pillar_crater_power, d.pillar_crater_power, 0.0, 4.0);
        self.vault_reach = fin(self.vault_reach, d.vault_reach, 0.5, 30.0);
        self.vault_speed = fin(self.vault_speed, d.vault_speed, 0.0, 60.0);
        self.vault_min_up = fin(self.vault_min_up, d.vault_min_up, 0.0, 1.0);
        self.vault_cd = fin(self.vault_cd, d.vault_cd, 0.05, 10.0);
        self.jab_range = fin(self.jab_range, d.jab_range, 0.5, 40.0);
        self.jab_dmg = fin(self.jab_dmg, d.jab_dmg, 0.0, 5.0);
        self.jab_kb = fin(self.jab_kb, d.jab_kb, 0.0, 80.0);
        self.jab_cd = fin(self.jab_cd, d.jab_cd, 0.05, 10.0);
        self.creature_radius = fin(self.creature_radius, d.creature_radius, 0.1, 3.0);
        self.creature_dmg = fin(self.creature_dmg, d.creature_dmg, 0.0, 5.0);
        self.creature_kb = fin(self.creature_kb, d.creature_kb, 0.0, 80.0);
        self
    }
}

// ------------------------------------------------------------------------------------ helpers

/// Does this held construct telescope? (Guns ignore Extends.)
#[inline]
pub fn is_extending(c: &Construct) -> bool {
    c.has(Mechanic::Extends) && c.category == FormCategory::Held && !super::gun::is_gun(c)
}

/// The pole's rest length (m): the model's length along its axis (grip at the origin, −Z).
#[inline]
pub fn base_len(c: &Construct) -> f32 {
    (c.half.z * 2.0).max(0.3)
}

/// The pole's rest radius (m).
#[inline]
pub fn base_radius(c: &Construct) -> f32 {
    c.half.x.max(c.half.y).clamp(0.04, 0.6)
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or_zero()
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn fx(out: &mut ConstructOut, c: &Construct, kind: HeldFx, pos: Vec3, dir: Vec3, strength: f32) {
    out.events.push(GameEvent::HeldFx { id: c.id, owner: c.owner, fx: kind, pos, dir, strength });
}

static CURVES: [OnceLock<Vec<SwingCurve>>; WEIGHT_BINS] = [const { OnceLock::new() }; WEIGHT_BINS];

/// The telescoping swing curve for `swing` at `weight` (cached per weight bin, like `held::curve_weighted`).
pub fn curve_weighted(swing: Swing, weight: f32) -> Option<&'static SwingCurve> {
    let bin = weight_bin(weight);
    let table = CURVES[bin].get_or_init(|| {
        let w = bin_weight(bin);
        Swing::ALL.iter().map(|s| bd_shapes::swing_curve_extends_weighted(s.kind(), w)).collect()
    });
    table.get(swing as usize)
}

/// Extra length at time `t` of a combo swing: a bump peaking mid active window.
pub fn swing_ext(cv: &SwingCurve, t: f32, reach: f32) -> f32 {
    let (a, b) = cv.active_window();
    let lead = cv.anticipation * 0.5;
    let tail = cv.recovery * 0.5;
    let up = ((t - (a - lead)) / (lead + (b - a) * 0.5).max(1e-3)).clamp(0.0, 1.0);
    let down = (((b + tail) - t) / (tail + (b - a) * 0.5).max(1e-3)).clamp(0.0, 1.0);
    reach * smooth(up.min(down))
}

/// Which move a charged release plays.
pub fn release_move(look: Vec3, grounded: bool, t: &ExtendTuning) -> ExtMove {
    let lim = if grounded { t.pillar_look_y } else { t.pillar_air_look_y };
    if look.y < lim { ExtMove::Pillar } else { ExtMove::Thrust }
}

/// Where the pole points during a move at time `xt` (world direction from the hand). The client
/// poses the pole with the same function.
pub fn move_dir(kind: ExtMove, xt: f32, xdir: Vec3, t: &ExtendTuning) -> Vec3 {
    match kind {
        ExtMove::Pillar => {
            if xt < t.pillar_rise {
                // Stands up out of the hand toward the heavens.
                let up = (flat(xdir) * 0.25 + Vec3::Y).normalize_or(Vec3::Y);
                up.lerp(Vec3::Y, smooth(xt / t.pillar_rise)).normalize_or(Vec3::Y)
            } else {
                // Crashes down: accelerating like something toppling.
                let f = ((xt - t.pillar_rise) / t.pillar_fall).clamp(0.0, 1.0);
                let f = f * f;
                slerp_dir(Vec3::Y, xdir, f)
            }
        }
        _ => xdir,
    }
}

fn slerp_dir(a: Vec3, b: Vec3, f: f32) -> Vec3 {
    let q = Quat::from_rotation_arc(a.normalize_or(Vec3::Y), b.normalize_or(Vec3::NEG_Z));
    (Quat::IDENTITY.slerp(q, f) * a).normalize_or(b)
}

/// Pole length (m beyond the model) and thickness factor during a move at time `xt`, and whether
/// the move is over. `xrange` = the move's target length.
pub fn move_shape(kind: ExtMove, xt: f32, xrange: f32, t: &ExtendTuning) -> (f32, f32, bool) {
    match kind {
        ExtMove::None => (0.0, 1.0, true),
        ExtMove::Thrust | ExtMove::Jab | ExtMove::Vault => {
            let speed = if kind == ExtMove::Vault { t.thrust_speed * 0.4 } else { t.thrust_speed };
            let out_t = (xrange / speed).max(DT);
            let hold = match kind {
                ExtMove::Thrust => t.thrust_hold,
                _ => 0.06,
            };
            let back = match kind {
                ExtMove::Thrust => t.thrust_retract,
                _ => t.thrust_retract * 0.8,
            };
            if xt < out_t {
                (xrange * (xt / out_t), 1.0, false)
            } else if xt < out_t + hold {
                (xrange, 1.0, false)
            } else {
                let f = ((xt - out_t - hold) / back).clamp(0.0, 1.0);
                (xrange * (1.0 - f) * (1.0 - f), 1.0, f >= 1.0)
            }
        }
        ExtMove::Pillar => {
            let (r, fl, rec) = (t.pillar_rise, t.pillar_fall, t.pillar_recover);
            if xt < r {
                let k = smooth(xt / r);
                (xrange * k, 1.0 + (t.pillar_girth - 1.0) * k, false)
            } else if xt < r + fl {
                (xrange, t.pillar_girth, false)
            } else {
                let f = ((xt - r - fl) / rec).clamp(0.0, 1.0);
                let k = 1.0 - smooth(f);
                (xrange * k, 1.0 + (t.pillar_girth - 1.0) * k, f >= 1.0)
            }
        }
    }
}

// ------------------------------------------------------------------------------------ wire

/// `ConstructSnap::ext`: ext (0.05 m, 12 bits) | girth (×4, 6 bits) | move (3 bits) | move time
/// (ticks, 8 bits) | move yaw (12 bits) | move pitch (11 bits). 0 for anything else.
pub fn wire(c: &Construct) -> u64 {
    if !c.has(Mechanic::Extends) || c.category != FormCategory::Held {
        return 0;
    }
    wire_state(&c.held)
}

/// [`wire`] of a held state.
pub fn wire_state(s: &HeldState) -> u64 {
    let q = |v: f32, k: f32, max: u64| if v.is_finite() { ((v * k).round().max(0.0) as u64).min(max) } else { 0 };
    let ext = q(s.ext, 20.0, 4095);
    let girth = q(s.ext_girth.max(1.0), 4.0, 63);
    let kind = s.xmove as u64 & 7;
    let ticks = q(s.xt, bd_core::TICK_HZ as f32, 255);
    let d = s.xdir.normalize_or(Vec3::NEG_Z);
    let yaw = d.x.atan2(-d.z); // 0 = −Z
    let pitch = d.y.clamp(-1.0, 1.0).asin();
    let yq = q((yaw / std::f32::consts::TAU).rem_euclid(1.0), 4096.0, 4095) & 4095;
    let pq = q(pitch / std::f32::consts::PI + 0.5, 2047.0, 2047);
    ext | girth << 12 | kind << 18 | ticks << 21 | yq << 29 | pq << 41
}

/// Apply a replicated [`wire`] value to a replica's held state (clients).
pub fn apply_wire(s: &mut HeldState, w: u64) {
    s.ext = (w & 4095) as f32 / 20.0;
    s.ext_girth = ((w >> 12) & 63) as f32 / 4.0;
    s.xmove = ExtMove::from_u8(((w >> 18) & 7) as u8);
    s.xt = ((w >> 21) & 255) as f32 / bd_core::TICK_HZ as f32;
    let yaw = ((w >> 29) & 4095) as f32 / 4096.0 * std::f32::consts::TAU;
    let pitch = (((w >> 41) & 2047) as f32 / 2047.0 - 0.5) * std::f32::consts::PI;
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    s.xdir = Vec3::new(sy * cp, sp, -cy * cp);
}

// ------------------------------------------------------------------------------------ held hooks

/// Held step, after the melee state machine: keep the pole's length in step with the swing / move
/// (called by `held::step` for [`is_extending`] items).
pub fn after_melee(c: &Construct, s: &mut HeldState, env: &Env<'_>) {
    let t = &env.ctx.tuning.extend;
    match s.mode {
        Mode::Swing => {
            if let Some(cv) = curve_weighted(s.swing, bd_shapes::weight_of(&c.recipe)) {
                s.ext = if s.swing == Swing::Secondary { 0.0 } else { swing_ext(cv, s.t, t.swing_reach) };
            }
            s.ext_girth = 1.0;
        }
        Mode::Extend => {}
        _ => {
            s.ext = 0.0;
            s.ext_girth = 1.0;
            s.xmove = ExtMove::None;
        }
    }
}

/// Charged release (from `held::melee`'s Charge branch; `s.heavy_k` is set): thrust or pillar.
pub fn release(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let t = &env.ctx.tuning.extend;
    let k = ((s.heavy_k - 0.5) * 2.0).clamp(0.0, 1.0);
    let kind = release_move(o.look, o.grounded, t);
    s.mode = Mode::Extend;
    s.xmove = kind;
    s.xt = 0.0;
    s.xdone = false;
    s.guarding = false;
    s.terrain_hit = false;
    c.hits.clear();
    c.set_anim(AnimState::Active, env.tick());
    match kind {
        ExtMove::Pillar => {
            let len = t.pillar_len * (0.7 + 0.3 * k);
            let heading = {
                let f = flat(o.look);
                if f == Vec3::ZERO { flat(Vec3::new(-o.yaw.sin(), 0.0, -o.yaw.cos())).normalize_or(Vec3::NEG_Z) } else { f }
            };
            // Where it lands: the ground under its far end (or level with the feet over the void).
            let total = base_len(c) + len;
            let far = o.hand + heading * total;
            let ground = raycast(env.ctx.voxels, far + Vec3::Y * 8.0, Vec3::NEG_Y, 80.0).map_or(o.pos.y, |h| h.point.y);
            let drop = (ground + 0.3 - o.hand.y).min(total * 0.2).max(-total * 0.95);
            s.xdir = (heading * total + Vec3::Y * drop).normalize_or(heading);
            s.xrange = len;
            fx(out, c, HeldFx::PillarRise, o.hand, s.xdir, total);
        }
        _ => {
            s.xrange = t.thrust_min + (t.thrust_max - t.thrust_min) * k;
            s.xdir = o.look.normalize_or(Vec3::NEG_Z);
            fx(out, c, HeldFx::Extend, o.hand, s.xdir, s.xrange);
        }
    }
}

/// RMB on an Extends item (instead of the form's own alt-fire): plant + vault visual, or a jab.
pub fn secondary(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let t = &env.ctx.tuning.extend;
    // The launch itself happens in `pre_move` (same press, same ray): this is the pole's half.
    let plant = raycast(env.ctx.voxels, o.eye, o.look, t.vault_reach + 0.75).filter(|_| s.cd2 <= 0.0);
    if let Some(h) = plant {
        let to = h.point - o.hand;
        s.xmove = ExtMove::Vault;
        s.xrange = (to.length() - base_len(c)).max(0.0);
        s.xdir = to.normalize_or(o.look);
        s.cd2 = t.vault_cd;
    } else if s.cd1 <= 0.0 {
        s.xmove = ExtMove::Jab;
        s.xrange = t.jab_range;
        s.xdir = o.look.normalize_or(Vec3::NEG_Z);
        s.cd1 = t.jab_cd;
        fx(out, c, HeldFx::Extend, o.hand, s.xdir, t.jab_range);
    } else {
        return;
    }
    s.mode = Mode::Extend;
    s.xt = 0.0;
    s.xdone = false;
    c.hits.clear();
    c.set_anim(AnimState::Active, env.tick());
}

/// One tick of a move (`Mode::Extend`).
pub fn step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let t = env.ctx.tuning.extend;
    let dt = env.dt();
    s.xt += dt;
    let (want, girth, done) = move_shape(s.xmove, s.xt, s.xrange, &t);
    s.ext_girth = girth;
    // Pose: the pole along the move's direction (the client draws the same).
    c.rot = geom::look_rotation(move_dir(s.xmove, s.xt, s.xdir, &t));
    match s.xmove {
        ExtMove::Thrust | ExtMove::Jab => {
            let growing = want > s.ext + 1e-4;
            s.ext = want.min(s.xrange);
            if growing || s.xt <= dt {
                thrust_line(c, s, env, out, &t);
            }
        }
        ExtMove::Pillar => {
            s.ext = want;
            if !s.xdone && s.xt >= t.pillar_rise + t.pillar_fall {
                s.xdone = true;
                pillar_impact(c, s, env, out, &t);
            }
        }
        ExtMove::Vault => s.ext = want.min(s.xrange),
        ExtMove::None => s.ext = 0.0,
    }
    if done {
        s.mode = Mode::Idle;
        s.xmove = ExtMove::None;
        s.ext = 0.0;
        s.ext_girth = 1.0;
        s.xt = 0.0;
        c.set_anim(AnimState::Recover, env.tick());
    }
}

/// The thrust / jab line: from the hand along `xdir` out to the current length; terrain and heavy
/// constructs stop it, light constructs are punched through, every enemy on it is hit once.
fn thrust_line(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, t: &ExtendTuning) {
    let o = env.owner;
    let ctx = env.ctx;
    let dir = s.xdir;
    let start = o.hand;
    let base = base_len(c);
    let mut reach = base + s.ext;
    let jab = s.xmove == ExtMove::Jab;
    // Terrain stops the tip (it chips the rock once).
    if let Some(h) = raycast(ctx.voxels, start, dir, reach) {
        reach = h.t.max(0.1);
        s.xrange = (reach - base).max(0.0);
        s.ext = s.ext.min(s.xrange);
        if !s.terrain_hit {
            s.terrain_hit = true;
            if !jab {
                out.carves.push(CarveRequest { center: h.point, radius: 0.9, power: 0.8 });
            }
            fx(out, c, HeldFx::SwingImpact, h.point, dir, if jab { 1.0 } else { 1.8 });
        }
    }
    let spec = ctx.tuning.form_spec(c.recipe.form); // TUNING HOOK
    let (dm, km) = super::held::mech_mult(c);
    let r = t.thrust_radius;
    // Constructs in order along the line: light ones break through, the first heavy one stops it.
    let mut stops: Option<f32> = None;
    let mut hits: [(f32, usize); 16] = [(0.0, 0); 16];
    let mut n = 0;
    for (i, px) in env.others.iter().enumerate() {
        if px.owner == c.owner || !px.active || px.attached || n >= hits.len() {
            continue;
        }
        if let Some((f, _)) = geom::segment_obb(start, start + dir * reach, px.center, px.half, px.rot, r) {
            hits[n] = (f, i);
            n += 1;
        }
    }
    hits[..n].sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for &(f, i) in &hits[..n] {
        let px = &env.others[i];
        let at = f * reach;
        if !c.hits.contains(px.id.0) {
            c.hits.insert(px.id.0);
            let point = start + dir * at;
            let dmg = if jab { t.thrust_construct_dmg * 0.3 } else { t.thrust_construct_dmg };
            out.construct_hits.push(ConstructHit {
                pierce: true,
                blunt: true,
                target: px.id,
                source: Some(c.owner),
                damage: dmg * dm,
                impulse: dir * (if jab { t.jab_kb } else { t.thrust_kb }) * km,
                blade: false,
                point,
            });
            fx(out, c, HeldFx::SwingImpact, point, dir, if jab { 0.8 } else { 1.5 });
        }
        if px.mass > t.thrust_pierce_mass || !px.free {
            stops = Some(at);
            break;
        }
    }
    if let Some(at) = stops {
        reach = reach.min(at.max(0.1));
        s.xrange = s.xrange.min((reach - base).max(0.0));
        s.ext = s.ext.min(s.xrange);
    }
    let end = start + dir * reach;
    for v in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(v.id)) {
            continue;
        }
        let hull = ctx.hull_for(c.owner, v);
        let Some((f, _)) = geom::segment_aabb(start, end, &geom::inflate(&hull, r)) else { continue };
        c.hits.insert(player_key(v.id));
        let point = start + (end - start) * f;
        let (dmg, kb, kind, impact) = if jab {
            (spec.damage * t.jab_dmg * dm, t.jab_kb * km, HitKind::Melee, 1.0)
        } else {
            (spec.damage * t.thrust_dmg * dm, t.thrust_kb * km, HitKind::HeavyMelee, t.thrust_impact)
        };
        out.player_hits.push(
            HitInfo::new(v.id, Some(c.owner), kind, dmg, kb, dir + Vec3::Y * 0.18, point)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact),
        );
    }
}

/// The pillar lands: craters where it meets the ground, a knock-up band along it, a shockwave either side.
fn pillar_impact(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, t: &ExtendTuning) {
    let o = env.owner;
    let ctx = env.ctx;
    let start = o.hand;
    let total = base_len(c) + s.xrange;
    let dir = s.xdir;
    let end = start + dir * total;
    // Craters.
    let n = ((total / t.pillar_crater_step).floor() as usize).clamp(1, 24);
    for i in 1..=n {
        let q = start + dir * (i as f32 * total / n as f32);
        if let Some(h) = raycast(ctx.voxels, q + Vec3::Y * 1.5, Vec3::NEG_Y, 1.5 + t.pillar_width + 2.0) {
            out.carves.push(CarveRequest { center: h.point, radius: t.pillar_crater, power: t.pillar_crater_power });
        } else if ctx.voxels.is_solid(world_to_voxel(q)) {
            out.carves.push(CarveRequest { center: q, radius: t.pillar_crater, power: t.pillar_crater_power });
        }
    }
    let spec = ctx.tuning.form_spec(c.recipe.form); // TUNING HOOK
    let (dm, km) = super::held::mech_mult(c);
    let heading = flat(dir);
    for v in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(v.id)) {
            continue;
        }
        let hull = ctx.hull_for(c.owner, v);
        let band = |w: f32| geom::segment_aabb(start, end, &geom::inflate(&hull, w));
        let k = if band(t.pillar_width).is_some() {
            1.0
        } else if band(t.pillar_width * t.pillar_shock).is_some() {
            0.5
        } else {
            continue;
        };
        c.hits.insert(player_key(v.id));
        let point = geom::closest_point(&hull, start + dir * (hull.center() - start).dot(dir).clamp(0.0, total));
        let up = Vec3::Y + heading * 0.3;
        let kind = if k >= 1.0 { HitKind::HeavyMelee } else { HitKind::Melee };
        out.player_hits.push(
            HitInfo::new(v.id, Some(c.owner), kind, spec.damage * t.pillar_dmg * dm * k, t.pillar_knockup * km * k, up, point)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(t.pillar_impact * k),
        );
    }
    for px in env.others {
        if px.owner == c.owner || !px.active || px.attached || c.hits.contains(px.id.0) {
            continue;
        }
        let Some((f, _)) = geom::segment_obb(start, end, px.center, px.half, px.rot, t.pillar_width) else { continue };
        c.hits.insert(px.id.0);
        out.construct_hits.push(ConstructHit {
            pierce: false,
            blunt: true,
            target: px.id,
            source: Some(c.owner),
            damage: t.thrust_construct_dmg * 1.5 * dm,
            impulse: (Vec3::Y + heading * 0.3) * t.pillar_knockup * km,
            blade: false,
            point: start + (end - start) * f,
        });
    }
    fx(out, c, HeldFx::PillarSlam, start, dir, total);
}

// ------------------------------------------------------------------------------------ vault

/// Before movement (host `step_movement` and client `predict_player`, same place): RMB into a
/// surface within `vault_reach` with an Extends item in hand launches the wielder along the aim
/// mirrored off that surface. Adds velocity only along the launch direction (never removes any), so
/// it composes with hops, slides, tethers and falls. Cooldown on `st.extra.vault_cd`.
#[allow(clippy::too_many_arguments)]
pub fn pre_move(
    store: &ConstructStore,
    voxels: &impl VoxelQuery,
    t: &ExtendTuning,
    owner: PlayerId,
    input: &PlayerInput,
    st: &mut PlayerState,
    p: &MoveParams,
    busy: bool,
    mut emit: impl FnMut(GameEvent),
) {
    if st.extra.vault_cd > 0.0 {
        st.extra.vault_cd = (st.extra.vault_cd - DT).max(0.0);
    }
    let pressed = input.buttons & !st.prev_buttons & buttons::SECONDARY != 0;
    if !pressed || busy || input.held(buttons::TYPING) || st.extra.vault_cd > 0.0 {
        return;
    }
    let Some(c) = store.owned_by(owner).find(|c| c.active() && c.attached() && is_extending(c) && c.pilot.is_none()) else { return };
    let look = input.look_dir();
    let eye = st.eye(p);
    let Some(h) = raycast(voxels, eye, look, t.vault_reach) else { return };
    let n = if h.normal == bd_core::IVec3::ZERO { -look } else { h.normal.as_vec3() };
    let mut dir = look - n * (2.0 * look.dot(n));
    if n.y > 0.5 {
        dir.y = dir.y.max(t.vault_min_up);
    }
    let dir = dir.normalize_or(Vec3::Y);
    let add = (t.vault_speed - st.vel.dot(dir)).max(0.0);
    st.apply_impulse(dir * add, 0.0);
    st.extra.vault_cd = t.vault_cd;
    emit(GameEvent::HeldFx { id: c.id, owner, fx: HeldFx::Vault, pos: h.point, dir, strength: add.max(1.0) });
}

// ------------------------------------------------------------------------------------ creatures

/// A creature's extending strike: a line from `from` along `dir` for `len` m (terrain stops it);
/// every enemy player on it is hit once. Returns the length actually reached.
#[allow(clippy::too_many_arguments)]
pub fn line_strike(c: &mut Construct, env: &Env<'_>, from: Vec3, dir: Vec3, len: f32, dmg: f32, kb: f32, impact: f32, out: &mut ConstructOut) -> f32 {
    let ctx = env.ctx;
    let t = &ctx.tuning.extend;
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let reach = raycast(ctx.voxels, from, dir, len).map_or(len, |h| h.t.max(0.1));
    let end = from + dir * reach;
    for v in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(v.id)) {
            continue;
        }
        let hull = ctx.hull_for(c.owner, v);
        let Some((f, _)) = geom::segment_aabb(from, end, &geom::inflate(&hull, t.creature_radius)) else { continue };
        c.hits.insert(player_key(v.id));
        out.player_hits.push(
            HitInfo::new(v.id, Some(c.owner), HitKind::Creature, dmg * t.creature_dmg, kb.max(t.creature_kb), dir + Vec3::Y * 0.25, from + (end - from) * f)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact),
        );
    }
    out.events.push(GameEvent::HeldFx { id: c.id, owner: c.owner, fx: HeldFx::Extend, pos: from, dir, strength: reach });
    reach
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_round_trips() {
        let mut s = HeldState { ext: 23.45, ext_girth: 6.5, xmove: ExtMove::Pillar, xt: 0.4, xdir: Vec3::new(0.6, -0.2, -0.77).normalize(), ..Default::default() };
        s.mode = Mode::Extend;
        let mut c = HeldState::default();
        let w = wire_state(&s);
        apply_wire(&mut c, w);
        assert!((c.ext - s.ext).abs() < 0.03);
        assert!((c.ext_girth - s.ext_girth).abs() < 0.13);
        assert_eq!(c.xmove, ExtMove::Pillar);
        assert!((c.xt - s.xt).abs() < 0.01);
        assert!(c.xdir.dot(s.xdir) > 0.9995, "{:?} vs {:?}", c.xdir, s.xdir);
    }

    #[test]
    fn shapes_grow_and_end() {
        let t = ExtendTuning::default();
        let (e0, _, d0) = move_shape(ExtMove::Thrust, 0.0, 30.0, &t);
        assert!(e0 < 1e-3 && !d0);
        let (e1, _, _) = move_shape(ExtMove::Thrust, 30.0 / t.thrust_speed, 30.0, &t);
        assert!((e1 - 30.0).abs() < 1e-3);
        let (_, _, d2) = move_shape(ExtMove::Thrust, 5.0, 30.0, &t);
        assert!(d2);
        let (_, g, _) = move_shape(ExtMove::Pillar, t.pillar_rise + 0.01, 24.0, &t);
        assert!((g - t.pillar_girth).abs() < 1e-4);
        // The pillar stands up, then ends pointing along its landing direction.
        let x = Vec3::new(0.0, -0.1, -1.0).normalize();
        assert!(move_dir(ExtMove::Pillar, t.pillar_rise, x, &t).y > 0.99);
        assert!(move_dir(ExtMove::Pillar, t.pillar_rise + t.pillar_fall, x, &t).dot(x) > 0.999);
        assert_eq!(release_move(Vec3::new(0.0, -0.6, -0.8), true, &t), ExtMove::Pillar);
        assert_eq!(release_move(Vec3::NEG_Z, true, &t), ExtMove::Thrust);
        assert_eq!(release_move(Vec3::new(0.0, -0.2, -0.98), false, &t), ExtMove::Pillar);
    }

    #[test]
    fn every_extends_curve_exists() {
        for s in Swing::ALL {
            let cv = curve_weighted(s, 1.5).expect("curve");
            assert!(cv.duration() > 0.0);
        }
        let a = curve_weighted(Swing::Combo0, 1.5).map(|c| c as *const SwingCurve);
        assert_eq!(a, curve_weighted(Swing::Combo0, 1.5).map(|c| c as *const SwingCurve));
    }
}
