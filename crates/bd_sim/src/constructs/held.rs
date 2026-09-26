//! **Held constructs** (COMBAT.md §4 Held, §5 mechanics on held items, §8 Guard) — held engineer.
//!
//! Controller [`super::Controller::Held`]: the item rides the owner's right hand; LMB / RMB use it.
//! Guns (and bow / cannon / staff / disc) are driven by [`super::gun`].
//!
//! * **Swings are swept from the animation.** Every melee attack plays a `bd_shapes::SwingCurve`
//!   (the exact curve the client animates, cached once in [`curve`]); during the curve's active window
//!   the item's sample points are swept through `SwingCurve::sweep` substeps (≤ 0.2 rad per step) and
//!   each point's segment is tested against (lag-compensated) enemy hulls and enemy construct OBBs —
//!   nothing tunnels at 120 Hz.
//! * Tap LMB = quick attack; holding past a 0.12 s grace coils into the Heavy curve's `hold_at` pose
//!   and charges (≤ 0.7 s, can move); release = heavy (up to ×2.2 dmg, ×1.8 kb). Swords and gauntlets
//!   chain a 3-hit combo (sword finisher ×1.5); a press during recovery is buffered.
//! * Per form (table in COMBAT.md §4): hammer dive slam (airborne + looking down → crater r = 2.5 m,
//!   radial knockback, momentum from the fall) / ground pound; sword parry stance; axe throw; spear
//!   lunge (18 m/s dash along look, 1.2 s cd — a movement tool); shield block 120° + 150 ms perfect
//!   parry; gauntlet combo + uppercut launcher, Grab = seize (≤ 0.35 s) & throw (thrown players are
//!   projectiles: they hit whoever they fly into and wall-splat); whip 12 m lash that yanks players,
//!   RMB lash on terrain = a third tether (rope constraint in [`post_move`]).
//! * Mechanics layered on held items: Returning (RMB throw, flies back, catch), Thor flight
//!   (Returning + RMB held after the throw drags you, drains willpower), Thrust (swing lunges you),
//!   Explodes (hits detonate), Attract / Repel (swing pulls in / shoves away), Pull (yank), Guard
//!   (RMB blocks on any form), SlowFall / Glide (umbrella: see `worn::movement_mods`), Link / Shrink /
//!   Grow (applied by the world from `HitInfo::construct`).
//! * [`after_update`] (called once per update by the store, after every construct stepped):
//!   swing-vs-swing clashes (sword-lock) and Guard resolution of this tick's melee hits.
//!   [`guard_check`] is the same Guard test for the world's projectile step (reflect / block).
//!
//! State lives in [`HeldState`] (`c.held`), shared by held, worn and pilot controllers.

use std::sync::OnceLock;

use bd_core::input::buttons;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic};
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_core::{ConstructId, PlayerId, Quat, Vec3};
use bd_shapes::{SwingCurve, SwingKind, swing_curve_weighted};
use glam::Affine3A;

use super::{AnimState, Construct, ConstructCtx, ConstructOut, ConstructStore, Driver, Env, Loco, PlayerView, form_spec, player_key};
use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind, TUMBLE_SECS};
use crate::events::{GameEvent, HeldFx};
use crate::geom;
use crate::status::StatusKind;

// ------------------------------------------------------------------------------------ numbers

/// Longest heavy charge (s).
pub const CHARGE_MAX: f32 = 0.7;
/// Holding LMB this long past the quick swing's anticipation turns it into a charge.
pub const TAP_GRACE: f32 = 0.12;
pub const HEAVY_DMG: f32 = 2.2;
pub const HEAVY_KB: f32 = 1.8;
/// Sword third hit.
pub const FINISHER_MULT: f32 = 1.5;
/// After a combo hit ends, the next press within this window continues the combo.
pub const COMBO_WINDOW: f32 = 0.4;
/// Perfect-parry window after raising a guard (s).
pub const PARRY_WINDOW: f32 = 0.15;
/// Block cone: cos(60°) → 120° in front.
pub const BLOCK_COS: f32 = 0.5;
/// Knockback kept through a (non-perfect) block.
pub const BLOCK_KB: f32 = 0.3;
/// Parry stagger on the attacker (s; ≤ 0.35 s control loss).
pub const STAGGER_SECS: f32 = 0.3;
/// Hammer dive: needs to look down at least this much (look.y).
pub const DIVE_LOOK_Y: f32 = -0.55;
pub const DIVE_SPEED: f32 = 28.0;
pub const SLAM_CRATER: f32 = 2.5;
pub const SLAM_RADIUS: f32 = 4.5;
pub const POUND_RADIUS: f32 = 4.0;
pub const LUNGE_SPEED: f32 = 18.0;
pub const LUNGE_CD: f32 = 1.2;
pub const WHIP_RANGE: f32 = 12.0;
pub const WHIP_YANK: f32 = 13.0;
pub const THROW_SPEED: f32 = 30.0;
pub const RETURN_AFTER: f32 = 1.2;
pub const RETURN_SPEED: f32 = 34.0;
pub const CATCH_DIST: f32 = 1.5;
/// A non-returning thrown item that lands lies where it fell this long (s) before it dissolves
/// (owner: summons shouldn't vanish right away). Walking over it picks it back up; the owner's
/// ring is free meanwhile (`World::player_actions`).
pub const LINGER_SECS: f32 = 4.0;
/// Thor flight speed (m/s) and willpower drain (/s).
pub const THOR_SPEED: f32 = 24.0;
pub const THOR_DRAIN: f32 = 14.0;
pub const GRAB_HOLD_MAX: f32 = 0.35;
pub const GRAB_THROW_SPEED: f32 = 26.0;

// ------------------------------------------------------------------------------------ state

/// What the held controller is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Idle,
    /// Playing a swing curve (`swing`, time `t`).
    Swing,
    /// Coiled at the Heavy curve's `hold_at`, charging.
    Charge,
    /// RMB block held (shield / Guard).
    Guard,
    /// Hammer dive toward the ground.
    Dive,
    /// Out of the hand (thrown / returning / Thor flight).
    Thrown,
}

/// Which curve a swing plays (index into the cached table).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Swing {
    #[default]
    Primary = 0,
    Heavy = 1,
    Combo0 = 2,
    Combo1 = 3,
    Combo2 = 4,
    Secondary = 5,
    Throw = 6,
}

impl Swing {
    pub const ALL: [Swing; 7] = [Swing::Primary, Swing::Heavy, Swing::Combo0, Swing::Combo1, Swing::Combo2, Swing::Secondary, Swing::Throw];
    pub fn kind(self) -> SwingKind {
        match self {
            Swing::Primary => SwingKind::Primary,
            Swing::Heavy => SwingKind::Heavy,
            Swing::Combo0 => SwingKind::Combo(0),
            Swing::Combo1 => SwingKind::Combo(1),
            Swing::Combo2 => SwingKind::Combo(2),
            Swing::Secondary => SwingKind::Secondary,
            Swing::Throw => SwingKind::Throw,
        }
    }
    fn tap(self) -> bool {
        matches!(self, Swing::Primary | Swing::Combo0 | Swing::Combo1 | Swing::Combo2)
    }
    fn combo_index(self) -> Option<u8> {
        match self {
            Swing::Combo0 => Some(0),
            Swing::Combo1 => Some(1),
            Swing::Combo2 => Some(2),
            _ => None,
        }
    }
}

/// Per-construct state for held / worn / piloted controllers (fixed-size, no heap).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldState {
    pub mode: Mode,
    pub swing: Swing,
    /// Time along the current curve (s).
    pub t: f32,
    /// Heavy charge held (s) / heavy fraction of the playing swing.
    pub charge: f32,
    pub heavy_k: f32,
    /// LMB time since the press (tap grace) and whether it was released since.
    pub tap_hold: f32,
    pub lmb_up: bool,
    /// Last combo step played and time left to chain the next one.
    pub combo: u8,
    pub combo_timer: f32,
    /// A buffered LMB press (next combo hit).
    pub queued: bool,
    /// Secondary (RMB) cooldown.
    pub cd2: f32,
    /// Explodes-on-hit cooldown.
    pub explode_cd: f32,
    /// Guard raised (block and/or parry live) and for how long.
    pub guarding: bool,
    pub guard_t: f32,
    /// Sword-style guard: only the perfect window counts (no plain block).
    pub parry_only: bool,
    /// The current swing already struck terrain.
    pub terrain_hit: bool,
    /// The current swing's hitbox window has opened (one-shot effects fired).
    pub opened: bool,
    /// Max fall speed seen during a dive / armed transform slam.
    pub fall_speed: f32,
    /// Player held by Grab, and for how long.
    pub grabbed: Option<PlayerId>,
    pub grab_t: f32,
    /// A player we threw (a human projectile) and how long it stays dangerous.
    pub victim: Option<PlayerId>,
    pub victim_t: f32,
    /// Out of the hand (not attached; see `Construct::attached`).
    pub thrown: bool,
    pub returning: bool,
    /// Thor flight: the thrown item drags its owner.
    pub thor: bool,
    pub fly_t: f32,
    /// A non-returning throw came to rest (lying / stuck where it landed) and for how long; `dud`:
    /// it already struck someone and just drops now (no second hit).
    pub landed: bool,
    pub rest_t: f32,
    pub dud: bool,
    /// Thrown flight velocity (kept here so generic pushes can't hijack the throw).
    pub fly_vel: Vec3,
    /// Whip tether anchor + rope length.
    pub anchor: Option<Vec3>,
    pub rope: f32,
    /// This tick's swing segment (world) for swing-vs-swing clashes.
    pub blade_live: bool,
    pub blade0: Vec3,
    pub blade1: Vec3,
    pub blade_r: f32,
    // ---- worn ----
    pub air_t: f32,
    pub flaps: u8,
    pub dashed: bool,
    pub chute: bool,
    pub thrusting: bool,
    // ---- guns ----
    /// Hand-cannon fan-the-hammer burst: shots left and timer.
    pub burst: u8,
    pub burst_t: f32,
    pub beam_t: f32,
    // ---- pilot / suit ----
    pub cd1: f32,
    /// The client should use a third-person camera (cockpit, vehicle mode).
    pub third_person: bool,
    /// Transforms: vehicle (alt) mode; transform sequence time left; a pending toggle.
    pub vehicle: bool,
    pub transform_t: f32,
    pub transform_req: bool,
    pub mode_init: bool,
    /// Vehicle→robot mid-air: slam on landing.
    pub slam_armed: bool,
    pub ram_cd: f32,
    pub dash_t: f32,
}

// ------------------------------------------------------------------------------------ curves

const HELD_FORMS: [Form; 12] = [
    Form::Hammer,
    Form::Sword,
    Form::Axe,
    Form::Spear,
    Form::Shield,
    Form::Gauntlet,
    Form::Whip,
    Form::Gun,
    Form::Bow,
    Form::Cannon,
    Form::Staff,
    Form::Disc,
];

/// Swing curves are mass-scaled (`bd_shapes::swing_curve_weighted`, DESIGN.md "Weight & power
/// fantasy"): the construct's `bd_shapes::weight_of(recipe)` is quantised to 0.1 steps over
/// 0.6..=2.2 so the curves can be built once per bin. Host, client prediction and the client's
/// animation all derive the bin from the same replicated recipe ([`swing_weight`]), so the hitbox
/// sweeps exactly the curve that is drawn.
pub const WEIGHT_BINS: usize = 17;

/// Bin of a weight factor (`weight_of`); non-finite → the authored bin (weight 1).
pub fn weight_bin(w: f32) -> usize {
    if !w.is_finite() {
        return 4;
    }
    (((w.clamp(0.6, 2.2) - 0.6) * 10.0).round() as usize).min(WEIGHT_BINS - 1)
}

/// Weight factor of a bin (bin 4 = exactly 1.0 = the authored curves).
pub fn bin_weight(bin: usize) -> f32 {
    (6 + bin.min(WEIGHT_BINS - 1)) as f32 / 10.0
}

/// The (quantised) weight the swing curves of `recipe` are built for — use it on the client too.
pub fn swing_weight(recipe: &bd_core::mechanics::Recipe) -> f32 {
    bin_weight(weight_bin(bd_shapes::weight_of(recipe)))
}

static CURVES: [OnceLock<Vec<SwingCurve>>; WEIGHT_BINS] = [const { OnceLock::new() }; WEIGHT_BINS];

/// The authored (weight 1) swing curve for `form` / `swing` (built once, then shared).
pub fn curve(form: Form, swing: Swing) -> Option<&'static SwingCurve> {
    curve_weighted(form, swing, 1.0)
}

/// The swing curve the client animates for `form` / `swing` at weight factor `weight` (quantised
/// with [`weight_bin`]; each bin's table is built once on first use, then shared — no per-tick
/// allocation).
pub fn curve_weighted(form: Form, swing: Swing, weight: f32) -> Option<&'static SwingCurve> {
    let bin = weight_bin(weight);
    let table = CURVES[bin].get_or_init(|| {
        let w = bin_weight(bin);
        let mut v = Vec::with_capacity(HELD_FORMS.len() * Swing::ALL.len());
        for f in HELD_FORMS {
            for s in Swing::ALL {
                v.push(swing_curve_weighted(f, s.kind(), w));
            }
        }
        v
    });
    let fi = HELD_FORMS.iter().position(|f| *f == form).unwrap_or(0);
    table.get(fi * Swing::ALL.len() + swing as usize)
}

/// The curve construct `c` swings along (its recipe's weight bin).
fn curve_of(c: &Construct, swing: Swing) -> Option<&'static SwingCurve> {
    curve_weighted(c.recipe.form, swing, bd_shapes::weight_of(&c.recipe))
}

/// Hand frame: origin at the hand, −Z along the look (bd_shapes swing convention).
#[inline]
pub fn hand_frame(o: &PlayerView) -> Affine3A {
    Affine3A::from_rotation_translation(geom::look_rotation(o.look), o.hand)
}

/// Most sample points an item's sweep uses (a Grow whip: 16.8 m / [`WHIP_SAMPLE_SPACING`] = 23).
const MAX_ITEM_POINTS: usize = 24;
/// Spacing of the whip's sample points along its length. Each point's swept path is tested as a
/// segment inflated by the lash radius (0.3 m) against hulls inflated the same way, so ≤ 0.75 m
/// leaves no gap a player's hull can sit in (the old 3 m spacing missed targets between samples).
pub const WHIP_SAMPLE_SPACING: f32 = 0.75;

/// Item sample points in the item's model space (grip at the origin, pointing −Z) + radius.
fn item_points(c: &Construct, o: &PlayerView) -> ([Vec3; MAX_ITEM_POINTS], usize, f32) {
    // Grow: melee reach ×1.4 (never more); Shrink: shorter.
    let reach = if o.size > 1.0 { o.size.min(1.4) } else { o.size.max(0.25) };
    let len = (c.half.z * 2.0).max(0.3) * reach;
    let thick = c.half.x.max(c.half.y);
    let mut p = [Vec3::ZERO; MAX_ITEM_POINTS];
    match c.recipe.form {
        Form::Shield => {
            p[0] = Vec3::new(0.0, 0.0, -c.half.z) * reach;
            (p, 1, (thick * 0.9).clamp(0.3, 1.2) * reach)
        }
        Form::Whip => {
            let l = WHIP_RANGE * reach;
            let n = ((l / WHIP_SAMPLE_SPACING).ceil() as usize).clamp(4, MAX_ITEM_POINTS);
            for (k, q) in p.iter_mut().enumerate().take(n) {
                *q = Vec3::new(0.0, 0.0, -l * (k + 1) as f32 / n as f32);
            }
            (p, n, 0.3)
        }
        _ => {
            for (k, f) in [0.35f32, 0.7, 1.0].iter().enumerate() {
                p[k] = Vec3::new(0.0, 0.0, -len * f);
            }
            (p, 3, (thick + 0.1).clamp(0.25, 1.0) * reach)
        }
    }
}

// ------------------------------------------------------------------------------------ helpers

pub(super) fn mech_mult(c: &Construct) -> (f32, f32) {
    let mut dmg = 1.0;
    let mut kb = 1.0;
    if c.has(Mechanic::Heavy) {
        dmg *= 1.5;
        kb *= 1.5;
    }
    if c.has(Mechanic::Fast) {
        dmg *= 0.8;
    }
    if c.has(Mechanic::Blunt) {
        kb *= 1.3;
    }
    (dmg, kb)
}

pub(super) fn is_blade(c: &Construct) -> bool {
    c.has(Mechanic::Blade) || matches!(c.recipe.form, Form::Sword | Form::Axe)
}

fn is_sharp(c: &Construct) -> bool {
    is_blade(c) || c.has(Mechanic::Pierce) || c.recipe.form == Form::Spear
}

fn pace(c: &Construct) -> f32 {
    if c.has(Mechanic::Fast) {
        0.75
    } else if c.has(Mechanic::Heavy) {
        1.25
    } else {
        1.0
    }
}

pub(super) fn fx(out: &mut ConstructOut, c: &Construct, fx: HeldFx, pos: Vec3, dir: Vec3, strength: f32) {
    out.events.push(GameEvent::HeldFx { id: c.id, owner: c.owner, fx, pos, dir, strength });
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or_zero()
}

/// Does RMB block on this construct (shield or anything with Guard)?
fn guards(c: &Construct) -> bool {
    c.recipe.form == Form::Shield || c.has(Mechanic::Guard)
}

fn uses_combo(form: Form) -> bool {
    matches!(form, Form::Sword | Form::Gauntlet)
}

/// Returning items are thrown with RMB (shields throw with a charged LMB instead: RMB blocks).
fn throws_on_rmb(c: &Construct) -> bool {
    (c.has(Mechanic::Returning) && !guards(c)) || c.recipe.form == Form::Axe && !c.has(Mechanic::Guard)
}

// ------------------------------------------------------------------------------------ step

/// [`super::StepFn`] for [`super::Controller::Held`].
pub fn step(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    c.body.claim(Driver::Attached);
    c.body.loco = Loco::Kinematic;
    let o = env.owner;
    let dt = env.dt();
    let mut s = c.held;
    s.blade_live = false;
    s.cd2 = (s.cd2 - dt).max(0.0);
    s.cd1 = (s.cd1 - dt).max(0.0);
    s.explode_cd = (s.explode_cd - dt).max(0.0);
    s.combo_timer = (s.combo_timer - dt).max(0.0);
    let prev = c.pos;
    if !o.alive {
        // Held items wait on their (dead) owner; anything in flight snaps back.
        s = HeldState { combo: s.combo, ..HeldState::default() };
        c.held = s;
        hand_pose(c, &o);
        c.vel = Vec3::ZERO;
        return;
    }
    if s.mode == Mode::Thrown {
        thrown_step(c, &mut s, env, out);
        c.held = s;
        return;
    }
    hand_pose(c, &o);
    if c.active() {
        if super::gun::is_gun(c) {
            super::gun::step(c, &mut s, env, out);
        } else {
            melee(c, &mut s, env, out);
        }
        grab_step(c, &mut s, env, out);
        victim_step(c, &mut s, env, out);
        whip_step(c, &mut s, env, out);
    }
    c.vel = (c.pos - prev) / dt.max(1e-6);
    c.held = s;
}

fn hand_pose(c: &mut Construct, o: &PlayerView) {
    c.pos = o.hand;
    // Idle: angled up-forward; swings override the rotation from the curve.
    c.rot = geom::look_rotation((o.look + Vec3::Y * 0.6).normalize_or_zero());
}

fn melee(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let dt = env.dt();
    let ok = !o.busy;
    let p1 = ok && o.pressed & buttons::PRIMARY != 0;
    let h1 = ok && o.input.held(buttons::PRIMARY);
    let p2 = ok && o.pressed & buttons::SECONDARY != 0;
    let h2 = ok && o.input.held(buttons::SECONDARY);
    if !h1 {
        s.lmb_up = true;
    }
    // Grabbing: RMB throws the victim (handled in grab_step); LMB still punches.
    let grabbing = s.grabbed.is_some();
    match s.mode {
        Mode::Idle | Mode::Thrown => {
            s.guarding = false;
            c.set_anim(AnimState::Idle, env.tick());
            if p1 {
                start_primary(c, s, env, out);
            } else if p2 && !grabbing {
                start_secondary(c, s, env, out);
            } else if h2 && guards(c) && !grabbing {
                raise_guard(c, s, env, out);
            }
        }
        Mode::Guard => {
            s.guard_t += dt;
            c.set_anim(AnimState::Active, env.tick());
            c.rot = geom::look_rotation(o.look);
            if !h2 {
                s.mode = Mode::Idle;
                s.guarding = false;
            } else if p1 {
                // Bash out of the guard.
                s.guarding = false;
                start_primary(c, s, env, out);
            }
        }
        Mode::Charge => {
            s.charge += dt;
            s.guarding = false;
            if let Some(cv) = curve_of(c, Swing::Heavy) {
                pose_on_curve(c, &o, cv, cv.hold_at);
            }
            if !h1 || s.charge >= CHARGE_MAX {
                s.heavy_k = (0.5 + 0.5 * (s.charge / CHARGE_MAX)).min(1.0);
                if c.recipe.form == Form::Shield && c.has(Mechanic::Returning) {
                    // Cap's shield: a charged release throws it (ricochets toward enemies).
                    start_throw(c, s, env, out);
                    return;
                }
                s.mode = Mode::Swing;
                s.swing = Swing::Heavy;
                s.t = curve_of(c, Swing::Heavy).map_or(0.0, |cv| cv.hold_at);
                s.opened = false;
                c.hits.clear();
                s.terrain_hit = false;
                c.set_anim(AnimState::Windup, env.tick());
                // The release IS the heavy's swing start (strength 2 = heavy): the first-person
                // viewmodel holds the coiled curve until this and 3P / construct animation start
                // the heavy from it. Without it the swing only showed on the NEXT click.
                fx(out, c, HeldFx::SwingStart, o.hand, o.look, 2.0);
            }
        }
        Mode::Dive => dive_step(c, s, env, out),
        Mode::Swing => swing_step(c, s, env, out, h1, p1),
    }
}

fn raise_guard(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    s.mode = Mode::Guard;
    s.guarding = true;
    s.parry_only = false;
    s.guard_t = 0.0;
    c.set_anim(AnimState::Active, env.tick());
    fx(out, c, HeldFx::GuardUp, env.owner.hand, env.owner.look, 1.0);
}

fn start_primary(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let form = c.recipe.form;
    // Hammer (or any Blunt/Heavy held item that isn't a blade) airborne + looking down: dive slam.
    let slammer = form == Form::Hammer || (c.has(Mechanic::Blunt) && !is_blade(c));
    if slammer && !o.grounded && o.look.y < DIVE_LOOK_Y {
        s.mode = Mode::Dive;
        s.t = 0.0;
        s.fall_speed = (-o.vel.y).max(0.0);
        c.hits.clear();
        let dv = (-DIVE_SPEED - o.vel.y).min(0.0);
        out.impulses.push((c.owner, Vec3::new(0.0, dv, 0.0), 0.0));
        c.set_anim(AnimState::Active, env.tick());
        fx(out, c, HeldFx::DiveStart, o.hand, Vec3::NEG_Y, 1.0);
        return;
    }
    let swing = if uses_combo(form) {
        if s.combo_timer > 0.0 {
            match s.combo {
                0 => Swing::Combo1,
                1 => Swing::Combo2,
                _ => Swing::Combo0,
            }
        } else {
            Swing::Combo0
        }
    } else {
        Swing::Primary
    };
    begin_swing(c, s, env, out, swing);
    s.tap_hold = 0.0;
    s.lmb_up = false;
    if c.has(Mechanic::Thrust) {
        // Rocket hammer: the swing lunges its wielder.
        let dv = o.look * 14.0 + Vec3::Y * if o.grounded { 3.0 } else { 0.0 };
        out.impulses.push((c.owner, dv, 0.0));
        fx(out, c, HeldFx::Launch, o.hand, o.look, 1.0);
    }
}

fn begin_swing(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, swing: Swing) {
    s.mode = Mode::Swing;
    s.swing = swing;
    s.t = 0.0;
    s.charge = 0.0;
    s.heavy_k = 0.0;
    s.queued = false;
    s.terrain_hit = false;
    s.opened = false;
    c.hits.clear();
    c.set_anim(AnimState::Windup, env.tick());
    let strength = match swing.combo_index() {
        Some(k) => 1.0 + k as f32 * 0.25,
        None => 1.0,
    };
    fx(out, c, HeldFx::SwingStart, env.owner.hand, env.owner.look, strength);
}

fn start_secondary(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    if throws_on_rmb(c) {
        start_throw(c, s, env, out);
        return;
    }
    if guards(c) {
        raise_guard(c, s, env, out);
        return;
    }
    if s.cd2 > 0.0 {
        return;
    }
    match c.recipe.form {
        Form::Sword => {
            // Parry stance: the curve's active window is the perfect-parry window.
            begin_swing(c, s, env, out, Swing::Secondary);
            s.cd2 = 0.45;
        }
        Form::Spear => {
            begin_swing(c, s, env, out, Swing::Secondary);
            let along = o.vel.dot(o.look);
            let dv = o.look * (LUNGE_SPEED - along.max(0.0)).max(0.0);
            out.impulses.push((c.owner, dv, 0.0));
            s.cd2 = LUNGE_CD;
            fx(out, c, HeldFx::Lunge, o.hand, o.look, 1.0);
        }
        Form::Gauntlet => {
            begin_swing(c, s, env, out, Swing::Secondary);
            s.cd2 = 0.8;
        }
        Form::Hammer => {
            begin_swing(c, s, env, out, Swing::Secondary);
            s.cd2 = 1.2;
        }
        Form::Whip => {
            begin_swing(c, s, env, out, Swing::Secondary);
            s.cd2 = 0.3;
            // Lash at the aim: terrain = a third tether.
            if let Some(h) = raycast(env.ctx.voxels, o.eye, o.look, WHIP_RANGE) {
                s.anchor = Some(h.point);
                s.rope = (h.point - o.eye).length().max(1.5);
                fx(out, c, HeldFx::WhipLatch, h.point, o.look, 1.0);
            }
        }
        _ => {}
    }
}

/// Place the item on its curve at time `t` (world rotation) for the client-matching pose.
fn pose_on_curve(c: &mut Construct, o: &PlayerView, cv: &SwingCurve, t: f32) {
    let m = hand_frame(o) * cv.sample(t);
    let (_, rot, _) = m.to_scale_rotation_translation();
    c.rot = rot.normalize();
}

fn swing_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, h1: bool, p1: bool) {
    let o = env.owner;
    let form = c.recipe.form;
    let Some(cv) = curve_of(c, s.swing) else {
        s.mode = Mode::Idle;
        return;
    };
    let dt = env.dt() / pace(c);
    let (a, b) = cv.active_window();
    let t0 = s.t;
    let mut t1 = t0 + dt;
    let mut clamped = false;
    // Tap grace: a held press coils into a heavy charge.
    if s.swing.tap() && !s.lmb_up && h1 && t1 >= a {
        s.tap_hold += env.dt();
        t1 = a;
        clamped = true;
        if s.tap_hold >= TAP_GRACE {
            s.mode = Mode::Charge;
            s.swing = Swing::Heavy;
            s.charge = s.tap_hold;
            c.set_anim(AnimState::Charging, env.tick());
            fx(out, c, HeldFx::ChargeStart, o.hand, o.look, 1.0);
            return;
        }
    }
    if p1 && t0 >= a {
        s.queued = true;
    }
    s.t = t1;
    pose_on_curve(c, &o, cv, t1);
    if clamped {
        return;
    }
    // Entering the active window.
    if !s.opened && t1 >= a {
        s.opened = true;
        c.set_anim(AnimState::Active, env.tick());
        fx(out, c, HeldFx::SwingActive, o.hand, o.look, if s.swing == Swing::Heavy { 2.0 } else { 1.0 });
        on_active(c, s, env, out);
    }
    if t1 > a && t0 < b {
        sweep(c, s, env, out, cv, t0.max(a), t1.min(b));
    }
    // Sword parry stance: guard live only during the active window.
    if form == Form::Sword && s.swing == Swing::Secondary {
        s.guarding = t1 >= a && t1 < b;
        s.parry_only = true;
        s.guard_t = (t1 - a).max(0.0);
    } else {
        s.guarding = false;
    }
    if t1 >= b && t0 < b {
        c.set_anim(AnimState::Recover, env.tick());
    }
    let dur = cv.duration();
    // A buffered press cancels the back half of the recovery into the next combo hit.
    let cancel = s.queued && uses_combo(form) && t1 >= b + cv.recovery * 0.4;
    if t1 >= dur || cancel {
        if let Some(k) = s.swing.combo_index() {
            s.combo = k;
            s.combo_timer = COMBO_WINDOW;
        } else {
            s.combo_timer = 0.0;
        }
        s.mode = Mode::Idle;
        s.guarding = false;
        c.set_anim(AnimState::Idle, env.tick());
        if s.queued {
            s.queued = false;
            start_primary(c, s, env, out);
        }
    }
}

/// One-shot effects when a swing's hitbox opens (Attract / Repel waves, ground pound).
fn on_active(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let ctx = env.ctx;
    let (dm, km) = mech_mult(c);
    if c.has(Mechanic::Attract) || c.has(Mechanic::Repel) {
        let attract = c.has(Mechanic::Attract);
        for t in ctx.enemies_of(c.owner) {
            let d = t.hull.center() - o.eye;
            let len = d.length();
            if len > 7.0 || len < 1e-3 || d.dot(o.look) / len < 0.3 {
                continue;
            }
            let n = d / len;
            let dv = if attract { -n * 9.0 + Vec3::Y * 2.0 } else { n * 14.0 + Vec3::Y * 3.0 };
            out.impulses.push((t.id, dv, 0.0));
        }
        fx(out, c, HeldFx::Launch, o.hand, o.look, if attract { -1.0 } else { 1.0 });
    }
    if c.recipe.form == Form::Hammer && s.swing == Swing::Secondary {
        // Ground pound: ring shockwave around the feet.
        let spec = ctx.tuning.form_spec(Form::Hammer); // TUNING HOOK
        for t in ctx.enemies_of(c.owner) {
            let d = geom::dist_sq_point_aabb(&t.hull, o.pos).sqrt();
            if d > POUND_RADIUS || (t.pos.y - o.pos.y).abs() > 2.5 {
                continue;
            }
            let fall = 1.0 - 0.6 * d / POUND_RADIUS;
            let dir = flat(t.pos - o.pos) + Vec3::Y;
            out.player_hits.push(
                HitInfo::new(t.id, Some(c.owner), HitKind::Melee, spec.damage * 0.6 * dm * fall, spec.knockback * km * fall, dir, t.pos)
                    .with_element(c.recipe.element)
                    .with_construct(c.id)
                    .with_impact(1.3),
            );
        }
        if o.grounded {
            out.carves.push(CarveRequest { center: o.pos, radius: 1.2, power: 0.6 });
        }
        fx(out, c, HeldFx::GroundPound, o.pos, Vec3::NEG_Y, 1.3);
    }
    if c.recipe.form == Form::Gauntlet && s.swing == Swing::Secondary {
        fx(out, c, HeldFx::Uppercut, o.hand, Vec3::Y, 1.2);
    }
}

/// Numbers for the swing playing now.
struct HitSpec {
    dmg: f32,
    kb: f32,
    kind: HitKind,
    impact: f32,
}

fn hit_spec(c: &Construct, s: &HeldState, t: &crate::tuning::CombatTuning) -> HitSpec {
    let spec = t.form_spec(c.recipe.form); // TUNING HOOK
    let (dm, km) = mech_mult(c);
    let mut dmg = spec.damage * dm;
    let mut kb = spec.knockback * km;
    let mut kind = HitKind::Melee;
    let mut impact = 1.0;
    match s.swing {
        Swing::Heavy => {
            let k = s.heavy_k.clamp(0.0, 1.0);
            dmg *= 1.0 + (t.hit.heavy_damage - 1.0) * k;
            kb *= 1.0 + (t.hit.heavy_knockback - 1.0) * k;
            kind = HitKind::HeavyMelee;
            impact = 1.5;
        }
        Swing::Combo2 if c.recipe.form == Form::Sword => {
            dmg *= t.hit.finisher_mult;
            kb *= 1.3;
            impact = 1.3;
        }
        Swing::Secondary => match c.recipe.form {
            Form::Spear => {
                dmg *= 1.2;
                impact = 1.2;
            }
            Form::Gauntlet => {
                kb *= 1.3;
                impact = 1.3;
            }
            _ => {}
        },
        _ => {}
    }
    HitSpec { dmg, kb, kind, impact }
}

/// Sweep the item along `cv` over `(t0, t1]` (inside the active window) and hit what it crosses.
fn sweep(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, cv: &SwingCurve, t0: f32, t1: f32) {
    let o = env.owner;
    let ctx = env.ctx;
    let frame = hand_frame(&o);
    let (local, n, r) = item_points(c, &o);
    let mut prev = [Vec3::ZERO; MAX_ITEM_POINTS];
    let m0 = frame * cv.sample(t0);
    for k in 0..n {
        prev[k] = m0.transform_point3(local[k]);
    }
    let hs = hit_spec(c, s, env.ctx.tuning);
    // The whip's secondary is a terrain lash (tether), not an attack; the sword's is a parry stance.
    let form = c.recipe.form;
    if (form == Form::Whip || form == Form::Sword) && s.swing == Swing::Secondary {
        return;
    }
    let heavy_terrain = s.swing == Swing::Heavy && (form == Form::Hammer || c.has(Mechanic::Blunt) || c.has(Mechanic::Heavy) || form == Form::Axe);
    for (_, m) in cv.sweep(t0, t1, 0.2) {
        let m = frame * m; // curve samples are hand-local (like `m0` above); the hitbox is world space
        let mut cur = [Vec3::ZERO; MAX_ITEM_POINTS];
        for k in 0..n {
            cur[k] = m.transform_point3(local[k]);
        }
        s.blade_live = true;
        s.blade0 = m.transform_point3(Vec3::ZERO);
        s.blade1 = cur[n - 1];
        s.blade_r = r;
        for k in 0..n {
            let (p0, p1) = (prev[k], cur[k]);
            let tangent = (p1 - p0).normalize_or_zero();
            // Players.
            for t in ctx.enemies_of(c.owner) {
                if c.hits.contains(player_key(t.id)) {
                    continue;
                }
                let hull = ctx.hull_for(c.owner, t);
                let Some((f, _)) = geom::segment_aabb(p0, p1, &geom::inflate(&hull, r)) else { continue };
                // The whip only lashes forward.
                if form == Form::Whip && (hull.center() - o.eye).dot(o.look) <= 0.0 {
                    continue;
                }
                c.hits.insert(player_key(t.id));
                let point = p0 + (p1 - p0) * f;
                hit_player(c, s, env, out, &hs, t, point, tangent);
            }
            // Enemy constructs (free bodies; attached weapons clash instead).
            for px in env.others {
                if px.owner == c.owner || !px.active || px.attached || c.hits.contains(px.id.0) {
                    continue;
                }
                let Some((f, _)) = geom::segment_obb(p0, p1, px.center, px.half, px.rot, r) else { continue };
                c.hits.insert(px.id.0);
                let point = p0 + (p1 - p0) * f;
                let weak = super::pilot::weak_mult(px, point, is_sharp(c));
                let vs = if form == Form::Axe { 1.5 } else { 1.0 };
                out.construct_hits.push(ConstructHit { pierce: super::components::is_pierce(c), blunt: super::components::is_blunt(c),
                    target: px.id,
                    source: Some(c.owner),
                    damage: hs.dmg * vs * weak,
                    impulse: (o.look + tangent * 0.4).normalize_or_zero() * hs.kb,
                    blade: is_blade(c),
                    point,
                });
                if weak > 1.0 {
                    fx(out, c, HeldFx::WeakPoint, point, tangent, weak);
                }
            }
            // Heavy blunt swings bite into terrain.
            if heavy_terrain && !s.terrain_hit && k == n - 1 && ctx.voxels.is_solid(world_to_voxel(p1)) {
                s.terrain_hit = true;
                let rad = if c.recipe.element == Element::Earth { 1.5 } else { 1.1 };
                out.carves.push(CarveRequest { center: p1, radius: rad, power: 0.8 });
                fx(out, c, HeldFx::SwingImpact, p1, tangent, 1.5);
            }
        }
        prev = cur;
    }
}

#[allow(clippy::too_many_arguments)]
fn hit_player(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, hs: &HitSpec, t: &PlayerView, point: Vec3, tangent: Vec3) {
    let o = env.owner;
    let form = c.recipe.form;
    let mut dir = o.look + tangent * 0.4;
    let mut kb = hs.kb;
    let mut dmg = hs.dmg;
    if form == Form::Gauntlet && s.swing == Swing::Secondary {
        // Uppercut launcher.
        dir = flat(o.look) * 0.25 + Vec3::Y;
    }
    if form == Form::Whip || c.has(Mechanic::Pull) {
        // Yank toward the wielder.
        dir = (o.pos - t.pos).normalize_or_zero() + Vec3::Y * 0.35;
        kb = kb.max(WHIP_YANK);
        fx(out, c, HeldFx::WhipYank, point, dir, 1.0);
    }
    if c.has(Mechanic::Repel) {
        kb *= 1.5;
    }
    let grab = c.has(Mechanic::Grab) && s.grabbed.is_none() && s.swing != Swing::Secondary;
    if grab {
        kb = 0.0;
        dmg *= 0.6;
        s.grabbed = Some(t.id);
        s.grab_t = 0.0;
        fx(out, c, HeldFx::Grab, point, o.look, 1.0);
    }
    out.player_hits.push(
        HitInfo::new(t.id, Some(c.owner), hs.kind, dmg, kb, dir, point)
            .with_element(c.recipe.element)
            .with_construct(c.id)
            .with_impact(hs.impact),
    );
    if c.has(Mechanic::Explodes) && s.explode_cd <= 0.0 {
        s.explode_cd = 0.5;
        out.explosions.push(Explosion {
            pos: point,
            radius: 2.5,
            damage: hs.dmg * 0.6,
            knockback: 14.0,
            source: Some(c.owner),
            construct: Some(c.id),
            element: c.recipe.element,
            carve_radius: 1.0,
            self_damage: false,
        });
    }
}

// ------------------------------------------------------------------------------------ dive slam

fn dive_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    s.t += env.dt();
    s.fall_speed = s.fall_speed.max(-o.vel.y);
    // Head down.
    c.rot = geom::look_rotation(Vec3::new(o.look.x * 0.2, -1.0, o.look.z * 0.2));
    let near_ground = raycast(env.ctx.voxels, o.pos + Vec3::Y * 0.1, Vec3::NEG_Y, 0.6).is_some();
    if o.grounded || near_ground || s.t > 2.5 {
        slam(c, env, out, o.pos, s.fall_speed, HeldFx::DiveSlam);
        s.mode = Mode::Idle;
        s.cd2 = s.cd2.max(0.3);
        c.set_anim(AnimState::Recover, env.tick());
    }
}

/// Landing shockwave (hammer dive slam, transformer landing slam): crater + radial knockback, scaled
/// by the fall speed (the fall also counts as closing speed for the bounded momentum bonus).
pub(super) fn slam(c: &Construct, env: &Env<'_>, out: &mut ConstructOut, at: Vec3, fall_speed: f32, kind: HeldFx) {
    let spec = env.ctx.tuning.form_spec(if c.category == FormCategory::Held { c.recipe.form } else { Form::Hammer }); // TUNING HOOK
    let (dm, km) = mech_mult(c);
    let strength = (1.0 + (fall_speed - 15.0).max(0.0) / 30.0).clamp(1.0, 2.0);
    let crater = SLAM_CRATER * if c.recipe.element == Element::Earth { 1.26 } else { 1.0 } * strength.sqrt();
    out.carves.push(CarveRequest { center: at, radius: crater, power: 1.0 });
    let radius = SLAM_RADIUS * strength.sqrt();
    for t in env.ctx.enemies_of(c.owner) {
        let d = geom::dist_sq_point_aabb(&t.hull, at).sqrt();
        if d > radius {
            continue;
        }
        let fall = 1.0 - 0.6 * (d / radius);
        let dir = flat(t.pos - at) + Vec3::Y * 1.2;
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::HeavyMelee, spec.damage * 1.6 * dm * fall, spec.knockback * 1.4 * km * fall * strength.min(1.5), dir, t.hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_closing(fall_speed)
                .with_impact(2.0 * strength),
        );
    }
    fx(out, c, kind, at, Vec3::NEG_Y, 2.0 * strength);
}

// ------------------------------------------------------------------------------------ throw

fn start_throw(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    s.mode = Mode::Thrown;
    s.thrown = true;
    s.returning = false;
    s.thor = false;
    s.fly_t = 0.0;
    s.landed = false;
    s.rest_t = 0.0;
    s.dud = false;
    s.guarding = false;
    c.bounces = 0;
    c.hits.clear();
    c.pos = o.hand + o.look * 0.6;
    c.vel = o.look * THROW_SPEED + o.vel;
    s.fly_vel = c.vel;
    c.set_anim(AnimState::Move, env.tick());
    fx(out, c, HeldFx::Throw, c.pos, o.look, 1.0);
}

fn catch(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    s.mode = Mode::Idle;
    s.thrown = false;
    s.returning = false;
    s.thor = false;
    s.landed = false;
    s.rest_t = 0.0;
    hand_pose(c, &env.owner);
    c.vel = Vec3::ZERO;
    c.set_anim(AnimState::Idle, env.tick());
    fx(out, c, HeldFx::Catch, c.pos, env.owner.look, 1.0);
}

fn thrown_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    // Flight state lives in HeldState: queued generic pushes / contacts can't hijack a throw.
    c.vel = s.fly_vel;
    thrown_inner(c, s, env, out);
    s.fly_vel = c.vel;
}

fn thrown_inner(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let ctx = env.ctx;
    let dt = env.dt();
    s.fly_t += dt;
    let returning_item = c.has(Mechanic::Returning);
    let h2 = !o.busy && o.input.held(buttons::SECONDARY);
    // Thor flight: Returning + RMB still held after the throw → the item leads, you follow.
    if returning_item && h2 && !s.returning && s.fly_t > 0.15 && o.willpower > 0.5 {
        if !s.thor {
            fx(out, c, HeldFx::ThorFlight, c.pos, o.look, 1.0);
        }
        s.thor = true;
        let want = o.eye + o.look * 2.5;
        let v = ((want - c.pos) / dt.max(1e-6)).clamp_length_max(80.0);
        let p0 = c.pos;
        c.pos += v * dt;
        c.vel = v;
        c.rot = geom::look_rotation(o.look);
        out.drain.push((c.owner, THOR_DRAIN * dt));
        c.set_anim(AnimState::Move, env.tick());
        thrown_hits(c, env, out, p0, c.pos);
        return;
    }
    if s.thor {
        s.thor = false;
        s.returning = true;
    }
    if returning_item && (s.fly_t > RETURN_AFTER || s.returning) {
        s.returning = true;
        c.set_anim(AnimState::Returning, env.tick());
        let to = o.hand - c.pos;
        if to.length() < CATCH_DIST.max(RETURN_SPEED * dt) {
            catch(c, s, env, out);
            return;
        }
        let p0 = c.pos;
        c.vel = to.normalize_or_zero() * RETURN_SPEED;
        c.pos += c.vel * dt;
        spin(c, s);
        thrown_hits(c, env, out, p0, c.pos);
        return;
    }
    if s.landed {
        // Lying where it landed: walk over it to pick it up, else it dissolves after a while.
        s.rest_t += dt;
        c.vel = Vec3::ZERO;
        let near = o.alive && o.seat.is_none() && (o.hull.center() - c.center()).length() < CATCH_DIST + c.half.max_element();
        if near && s.rest_t > 0.25 {
            catch(c, s, env, out);
        } else if s.rest_t >= LINGER_SECS {
            c.gone = Some(crate::events::GoneReason::Spent);
        }
        return;
    }
    if s.fly_t > 4.0 {
        // Lost in flight (never landed): the construct dissolves.
        c.gone = Some(crate::events::GoneReason::Spent);
        return;
    }
    c.vel.y -= 14.0 * dt;
    let p0 = c.pos;
    let p1 = p0 + c.vel * dt;
    spin(c, s);
    if !s.dud && thrown_hits(c, env, out, p0, p1) {
        if returning_item {
            s.returning = true;
            c.pos = p1;
        } else {
            // It struck someone: it glances off and drops (lands and lingers; no second hit).
            s.dud = true;
            c.vel = c.vel * -0.15 + Vec3::Y * 3.0;
            c.pos = p0;
        }
        return;
    }
    let seg = p1 - p0;
    let len = seg.length();
    let r = c.half.min_element().max(0.15);
    if let Some(h) = (len > 1e-5).then(|| raycast(ctx.voxels, p0, seg, len + r)).flatten() {
        let nrm = if h.normal == bd_core::IVec3::ZERO { -seg / len } else { h.normal.as_vec3() };
        if c.has(Mechanic::Explodes) {
            out.explosions.push(Explosion { pos: h.point, radius: 2.5, damage: 12.0, knockback: 14.0, source: Some(c.owner), construct: Some(c.id), element: c.recipe.element, carve_radius: 1.2, self_damage: false });
        }
        if returning_item {
            // Ricochet (Cap's shield) re-aims at the nearest enemy; otherwise it just comes back.
            if (c.has(Mechanic::Ricochet) || c.recipe.form == Form::Shield) && c.bounces < 2 {
                c.bounces += 1;
                let mut v = c.vel - nrm * (2.0 * c.vel.dot(nrm));
                if let Some(t) = ctx.nearest_enemy(c.owner, h.point) {
                    let want = (t.hull.center() - h.point).normalize_or_zero();
                    if want.dot(nrm) > 0.0 {
                        v = want * v.length().max(THROW_SPEED);
                    }
                }
                c.vel = v;
                c.pos = h.point + nrm * (r + 0.05);
                c.hits.clear();
            } else {
                s.returning = true;
                c.pos = h.point + nrm * (r + 0.05);
            }
        } else {
            // It lands (stuck in / lying on what it hit) and lingers — see `LINGER_SECS`.
            s.landed = true;
            s.rest_t = 0.0;
            c.pos = h.point + nrm * (r * 0.5);
            c.vel = Vec3::ZERO;
            c.set_anim(AnimState::Idle, env.tick());
            fx(out, c, HeldFx::SwingImpact, h.point, nrm, 0.6);
        }
        return;
    }
    c.pos = p1;
}

fn spin(c: &mut Construct, s: &HeldState) {
    let spin = if matches!(c.recipe.form, Form::Axe | Form::Hammer | Form::Disc | Form::Shield) { s.fly_t * 18.0 } else { 0.0 };
    c.rot = (geom::look_rotation(c.vel) * Quat::from_rotation_x(-spin)).normalize();
}

/// Swept thrown-item contact with enemies. Returns true if it hit someone.
fn thrown_hits(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, p0: Vec3, p1: Vec3) -> bool {
    let spec = env.ctx.tuning.form_spec(c.recipe.form); // TUNING HOOK
    let (dm, km) = mech_mult(c);
    let r = c.half.max_element().clamp(0.2, 0.8);
    let mut any = false;
    for t in env.ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(t.id)) {
            continue;
        }
        let hull = env.ctx.hull_for(c.owner, t);
        let Some((f, _)) = geom::segment_aabb(p0, p1, &geom::inflate(&hull, r)) else { continue };
        c.hits.insert(player_key(t.id));
        let point = p0 + (p1 - p0) * f;
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Thrown, spec.damage * 1.2 * dm, spec.knockback * km, c.vel, point)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(1.3),
        );
        if c.has(Mechanic::Explodes) {
            out.explosions.push(Explosion { pos: point, radius: 2.5, damage: spec.damage * 0.6, knockback: 14.0, source: Some(c.owner), construct: Some(c.id), element: c.recipe.element, carve_radius: 1.0, self_damage: false });
        }
        any = true;
    }
    any
}

// ------------------------------------------------------------------------------------ grab

fn grab_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let Some(pid) = s.grabbed else { return };
    let o = env.owner;
    let Some(v) = env.ctx.player(pid).filter(|v| v.alive && v.seat.is_none()).copied() else {
        s.grabbed = None;
        return;
    };
    s.grab_t += env.dt();
    let throw = (!o.busy && o.pressed & buttons::SECONDARY != 0) || s.grab_t >= GRAB_HOLD_MAX;
    if throw {
        let dir = if o.look.y < -0.3 { o.look } else { (o.look + Vec3::Y * 0.15).normalize_or_zero() };
        let dv = dir * GRAB_THROW_SPEED + o.vel * 0.5 - v.vel;
        out.impulses.push((pid, dv, TUMBLE_SECS));
        s.grabbed = None;
        s.victim = Some(pid);
        s.victim_t = 1.0;
        fx(out, c, HeldFx::GrabThrow, v.hull.center(), dir, 1.4);
        return;
    }
    // Carry: steer the victim's centre to the fist (velocity correction, no teleport).
    let hold = o.hand + o.look * 1.1 - Vec3::Y * 0.6;
    let dv = ((hold - v.pos) * 14.0 + o.vel - v.vel).clamp_length_max(40.0);
    out.impulses.push((pid, dv, 0.0));
}

/// A thrown player is a projectile: whoever they fly into gets hit (both take it).
fn victim_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let Some(pid) = s.victim else { return };
    s.victim_t -= env.dt();
    let Some(v) = env.ctx.player(pid).filter(|v| v.alive).copied() else {
        s.victim = None;
        return;
    };
    if s.victim_t <= 0.0 || v.vel.length() < 8.0 {
        if s.victim_t <= 0.0 {
            s.victim = None;
        }
        return;
    }
    let body = geom::inflate(&v.hull, 0.3);
    for t in env.ctx.enemies_of(c.owner) {
        if t.id == pid || !t.hull.intersects(&body) {
            continue;
        }
        let speed = v.vel.length();
        let point = (t.hull.center() + v.hull.center()) * 0.5;
        out.player_hits.push(HitInfo::new(t.id, Some(c.owner), HitKind::Thrown, 6.0 + speed * 0.3, 8.0 + speed * 0.4, v.vel, point).with_construct(c.id).with_impact(1.5));
        out.player_hits.push(HitInfo::new(pid, Some(c.owner), HitKind::Thrown, 5.0, 4.0, -v.vel, point).with_construct(c.id));
        s.victim = None;
        break;
    }
}

// ------------------------------------------------------------------------------------ whip tether

fn whip_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let Some(a) = s.anchor else { return };
    let o = env.owner;
    let chest = o.pos + Vec3::Y * (o.hull.max.y - o.hull.min.y) * 0.6;
    let dist = (chest - a).length();
    // Anchor carved away, released, or stretched past breaking.
    let solid = raycast(env.ctx.voxels, chest, a - chest, dist + 0.3).is_some();
    if o.busy || !o.input.held(buttons::SECONDARY) || !solid || dist > WHIP_RANGE + 3.0 {
        s.anchor = None;
        fx(out, c, HeldFx::WhipRelease, a, Vec3::ZERO, 1.0);
        return;
    }
    // Ratchet like a tether rope: only ever shortens.
    s.rope = s.rope.min(dist).max(1.5);
    c.rot = geom::look_rotation(a - o.hand);
}

// ------------------------------------------------------------------------------------ owner hooks

/// Before movement (called by `worn::movement_mods` for held items): Thor flight hangs the wielder
/// on the item; held SlowFall/Glide (umbrella) is handled with the worn items there.
pub fn movement_mods(c: &Construct, mods: &mut crate::movement::MoveModifiers) {
    if c.held.thor {
        mods.gravity_scale = 0.0;
    }
}

/// After movement (called by `worn::post_move`): whip rope constraint, Thor drag.
pub fn post_move(c: &Construct, input: &bd_core::input::PlayerInput, st: &mut crate::movement::PlayerState, p: &crate::movement::MoveParams, dt: f32) {
    let s = &c.held;
    if let Some(a) = s.anchor {
        let chest = st.chest(p);
        let d = chest - a;
        let dist = d.length();
        if dist > 1e-3 {
            let n = d / dist;
            // Gentle pull toward the anchor (like a tether), plus the inelastic rope.
            st.vel -= n * (p.rope_pull * dt);
            if dist >= s.rope {
                let vr = st.vel.dot(n);
                if vr > 0.0 {
                    st.vel -= n * vr;
                }
                if dist > s.rope + 0.05 {
                    st.vel -= n * ((dist - s.rope) * 8.0).min(10.0);
                }
                if n.y < -0.2 {
                    st.grounded = false;
                }
            }
            st.tumble = 0.0;
        }
    }
    if s.thor {
        let look = input.look_dir();
        let want = look * THOR_SPEED;
        st.vel += (want - st.vel) * (5.0 * dt).min(1.0);
        st.tumble = 0.0;
        if want.y > 0.0 {
            st.grounded = false;
        }
    }
}

// ------------------------------------------------------------------------------------ guard

/// Outcome of a Guard test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Guarded {
    /// Perfect parry: projectiles are reflected to their owner, melee attackers staggered.
    Parry,
    /// Blocked: absorbed (the guard takes integrity damage), a little pushback.
    Block,
}

fn guard_verdict(s: &HeldState, eye: Vec3, look: Vec3, from: Vec3, pierce: bool) -> Option<Guarded> {
    if !s.guarding {
        return None;
    }
    let d = (from - eye).normalize_or_zero();
    if d == Vec3::ZERO || d.dot(look.normalize_or_zero()) < BLOCK_COS {
        return None;
    }
    let parry = s.guard_t <= PARRY_WINDOW;
    if parry {
        Some(Guarded::Parry)
    } else if s.parry_only || pierce {
        // Swords only parry; Pierce goes through a plain block (COMBAT.md §8).
        None
    } else {
        Some(Guarded::Block)
    }
}

/// Does `target`'s raised guard stop a hit arriving from `from`? (`eye`/`look` = the target's view.)
/// Used by the world's projectile step (reflect on parry, absorb on block) and by [`after_update`].
pub fn guard_check(store: &ConstructStore, target: PlayerId, eye: Vec3, look: Vec3, from: Vec3, pierce: bool) -> Option<(Guarded, ConstructId)> {
    store
        .owned_by(target)
        .filter(|c| c.active() && c.held.guarding && (c.category == FormCategory::Held || c.suit))
        .find_map(|c| guard_verdict(&c.held, eye, look, from, pierce).map(|g| (g, c.id)))
}

/// Swing clashes + Guard resolution for this tick's melee hits. Called by the store once per
/// update after every construct stepped (before the generic clash pass).
pub fn after_update(items: &mut [Construct], ctx: &ConstructCtx, out: &mut ConstructOut) {
    clash_swings(items, out);
    resolve_guards(items, ctx, out);
}

/// Closest distance between segments `a0a1` and `b0b1`, with the midpoint of the closest pair.
fn segment_distance(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> (f32, Vec3) {
    let d1 = a1 - a0;
    let d2 = b1 - b0;
    let r = a0 - b0;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t) = if a <= 1e-8 && e <= 1e-8 {
        (0.0, 0.0)
    } else if a <= 1e-8 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-8 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let den = a * e - b * b;
            let mut s = if den > 1e-8 { ((b * f - c * e) / den).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    let pa = a0 + d1 * s;
    let pb = b0 + d2 * t;
    ((pa - pb).length(), (pa + pb) * 0.5)
}

/// Two live swings of different owners crossing: sword-lock (both knocked back, both interrupted).
fn clash_swings(items: &mut [Construct], out: &mut ConstructOut) {
    let n = items.len();
    for i in 0..n {
        if !items[i].held.blade_live || items[i].clash_cd > 0.0 {
            continue;
        }
        for j in (i + 1)..n {
            let (lo, hi) = items.split_at_mut(j);
            let a = &mut lo[i];
            let b = &mut hi[0];
            if !b.held.blade_live || b.clash_cd > 0.0 || a.owner == b.owner || a.clash_cd > 0.0 {
                continue;
            }
            let (d, point) = segment_distance(a.held.blade0, a.held.blade1, b.held.blade0, b.held.blade1);
            if d > a.held.blade_r + b.held.blade_r {
                continue;
            }
            let nrm = flat(a.held.blade0 - b.held.blade0);
            let nrm = if nrm == Vec3::ZERO { Vec3::X } else { nrm };
            let strength = 8.0 + (form_spec(a.recipe.form).damage + form_spec(b.recipe.form).damage) * 0.5;
            out.impulses.push((a.owner, nrm * 6.0 + Vec3::Y * 2.0, 0.0));
            out.impulses.push((b.owner, -nrm * 6.0 + Vec3::Y * 2.0, 0.0));
            out.construct_hits.push(ConstructHit { pierce: false, blunt: false, target: a.id, source: Some(b.owner), damage: strength * 0.5, impulse: Vec3::ZERO, blade: is_blade(b), point });
            out.construct_hits.push(ConstructHit { pierce: false, blunt: false, target: b.id, source: Some(a.owner), damage: strength * 0.5, impulse: Vec3::ZERO, blade: is_blade(a), point });
            out.events.push(GameEvent::Clash { a: a.id, b: b.id, point, strength });
            fx(out, a, HeldFx::SwordLock, point, nrm, 1.5);
            for c in [a, b] {
                interrupt(c);
                c.clash_cd = 0.3;
            }
        }
    }
}

/// Knock a swing into its recovery (parried / clashed).
fn interrupt(c: &mut Construct) {
    let s = &mut c.held;
    if s.mode == Mode::Swing || s.mode == Mode::Charge {
        s.mode = Mode::Idle;
        s.queued = false;
        s.combo_timer = 0.0;
    }
    s.blade_live = false;
    c.hits.clear();
    c.anim_state = AnimState::Recover;
}

fn resolve_guards(items: &mut [Construct], ctx: &ConstructCtx, out: &mut ConstructOut) {
    if !items.iter().any(|c| c.held.guarding && c.active()) {
        return;
    }
    let ConstructOut { player_hits, construct_hits, statuses, impulses, events, .. } = out;
    player_hits.retain_mut(|h| {
        if !matches!(h.kind, HitKind::Melee | HitKind::HeavyMelee | HitKind::Creature | HitKind::Thrown) {
            return true;
        }
        let Some(gi) = items.iter().position(|c| c.owner == h.target && c.active() && c.held.guarding && (c.category == FormCategory::Held || c.suit)) else {
            return true;
        };
        let Some(def) = ctx.player(h.target) else { return true };
        let attacker = h.construct.and_then(|id| items.binary_search_by_key(&id.0, |c| c.id.0).ok());
        let from = match attacker {
            Some(ai) if !items[ai].attached() => items[ai].center(),
            _ => h.source.and_then(|s| ctx.player(s)).map_or(h.point - h.impulse_dir.normalize_or_zero(), |a| a.eye),
        };
        let pierce = attacker.is_some_and(|ai| items[ai].has(Mechanic::Pierce) || items[ai].recipe.form == Form::Spear);
        let verdict = guard_verdict(&items[gi].held, def.eye, def.look, from, pierce);
        let (gid, gowner) = (items[gi].id, items[gi].owner);
        match verdict {
            Some(Guarded::Parry) => {
                if let Some(src) = h.source.filter(|s| *s != h.target) {
                    statuses.push((src, StatusKind::Stun, STAGGER_SECS, Some(h.target)));
                    let push = flat(from - def.eye);
                    impulses.push((src, push * 9.0 + Vec3::Y * 2.5, 0.0));
                }
                if let Some(ai) = attacker.filter(|ai| items[*ai].category == FormCategory::Held) {
                    interrupt(&mut items[ai]);
                }
                events.push(GameEvent::HeldFx { id: gid, owner: gowner, fx: HeldFx::Parry, pos: h.point, dir: (from - def.eye).normalize_or_zero(), strength: 1.5 });
                false
            }
            Some(Guarded::Block) => {
                construct_hits.push(ConstructHit { pierce: false, blunt: false, target: gid, source: h.source, damage: h.damage * 0.5, impulse: Vec3::ZERO, blade: false, point: h.point });
                events.push(GameEvent::HeldFx { id: gid, owner: gowner, fx: HeldFx::Block, pos: h.point, dir: (from - def.eye).normalize_or_zero(), strength: h.damage / 10.0 });
                h.damage = 0.0;
                h.knockback *= BLOCK_KB;
                true
            }
            None => {
                if pierce && items[gi].held.guarding {
                    // Pierce vs Guard: goes through, the guard cracks.
                    construct_hits.push(ConstructHit { pierce: false, blunt: false, target: gid, source: h.source, damage: h.damage * 1.5, impulse: Vec3::ZERO, blade: false, point: h.point });
                }
                true
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_held_form_has_cached_curves() {
        for f in HELD_FORMS {
            for s in Swing::ALL {
                let cv = curve(f, s).expect("curve");
                assert!(cv.duration() > 0.0);
            }
        }
        // The cache returns the same allocation every time (no per-tick allocation).
        let a = curve(Form::Sword, Swing::Combo2).map(|c| c as *const SwingCurve);
        let b = curve(Form::Sword, Swing::Combo2).map(|c| c as *const SwingCurve);
        assert_eq!(a, b);
        assert_eq!(curve(Form::Sword, Swing::Combo2).map(|c| c.name.as_str()), Some("sword_overhead_finisher"));
    }

    #[test]
    fn weighted_curves_are_deterministic_and_mass_scaled() {
        // Weight 1 is exactly the authored curve (bit-identical: no change for nominal items).
        assert_eq!(bin_weight(weight_bin(1.0)), 1.0);
        for f in HELD_FORMS {
            for s in Swing::ALL {
                assert_eq!(curve_weighted(f, s, 1.0).cloned(), Some(bd_shapes::swing_curve(f, s.kind())));
            }
        }
        // Quantisation is a pure function of the weight (host and client pick the same bin).
        for k in 0..400 {
            let w = 0.5 + k as f32 * 0.0047;
            assert_eq!(weight_bin(w), weight_bin(w));
            assert!(weight_bin(w) < WEIGHT_BINS);
        }
        assert_eq!(weight_bin(f32::NAN), weight_bin(1.0));
        // Heavy hammers wind up longer than light swords; the same (form, swing, bin) is shared.
        let heavy = curve_weighted(Form::Hammer, Swing::Primary, 2.2).expect("curve");
        let light = curve_weighted(Form::Hammer, Swing::Primary, 0.6).expect("curve");
        assert!(heavy.anticipation > light.anticipation);
        let a = curve_weighted(Form::Hammer, Swing::Primary, 2.21).map(|c| c as *const SwingCurve);
        assert_eq!(a, Some(heavy as *const SwingCurve));
    }

    #[test]
    fn segment_distance_basics() {
        let (d, _) = segment_distance(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, -1.0), Vec3::new(0.0, 1.0, 1.0));
        assert!((d - 1.0).abs() < 1e-5);
    }

    #[test]
    fn guard_cone_and_parry_window() {
        let mut s = HeldState { guarding: true, guard_t: 0.05, ..Default::default() };
        let eye = Vec3::ZERO;
        let look = Vec3::NEG_Z;
        assert_eq!(guard_verdict(&s, eye, look, Vec3::new(0.0, 0.0, -5.0), false), Some(Guarded::Parry));
        assert_eq!(guard_verdict(&s, eye, look, Vec3::new(0.0, 0.0, 5.0), false), None, "behind");
        s.guard_t = 0.5;
        assert_eq!(guard_verdict(&s, eye, look, Vec3::new(2.0, 0.0, -5.0), false), Some(Guarded::Block));
        assert_eq!(guard_verdict(&s, eye, look, Vec3::new(2.0, 0.0, -5.0), true), None, "pierce goes through");
        s.parry_only = true;
        assert_eq!(guard_verdict(&s, eye, look, Vec3::new(0.0, 0.0, -5.0), false), None);
    }
}
