//! **Worn constructs** (COMBAT.md §4 Worn — movement modifiers that stay *below* skilled movement)
//! — held engineer.
//!
//! Controller [`super::Controller::Worn`] plus the two per-player hooks the store forwards:
//! [`movement_mods`] (before movement; also predicted by clients) and [`post_move`] (after movement).
//!
//! * **Wings** (or any worn Glide): hold Jump in the air = glide (fall → forward speed, capped at
//!   [`GLIDE_CAP`]); tap Jump in the air = flap (+[`FLAP_UP`] m/s up, [`MAX_FLAPS`] then land to
//!   recharge).
//! * **Parachute** (or SlowFall): hold Jump = slow fall (terminal [`CHUTE_FALL`] m/s) and it
//!   **cancels tumble** — the ring-out save.
//! * **Jetpack** (or Thrust): hold Jump = thrust up / toward the aim, capped at [`JET_MAX`] m/s,
//!   drains [`JET_DRAIN`] willpower/s.
//! * **Cape**: glide-lite + one mid-air dash (tap Jump in the air; recharges on landing).
//! * Held items routed through these hooks: umbrella (held SlowFall / Glide), Thor flight and the
//!   whip tether (`held::movement_mods` / `held::post_move`), minigun hover (`gun::movement_mods`),
//!   power-suit flight / vehicle mode (`pilot::suit_movement_mods`).
//!
//! Flaps and dashes are velocity impulses from [`step`] (one tick after the press); glide, slow fall
//! and thrust are `MoveModifiers`, so client prediction replays them exactly.

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Form, FormCategory, Mechanic};
use bd_core::{DT, PlayerId, Vec3};

use super::held::{HeldState, fx};
use super::{AnimState, Construct, ConstructOut, ConstructStore, Driver, Env, Loco};
use crate::events::{GameEvent, HeldFx};
use crate::geom;
use crate::movement::{MoveEvents, MoveModifiers, MoveParams, PlayerState};

pub const GLIDE_CAP: f32 = 22.0;
pub const FLAP_UP: f32 = 6.0;
pub const MAX_FLAPS: u8 = 3;
pub const CHUTE_FALL: f32 = 3.0;
pub const JET_MAX: f32 = 16.0;
pub const JET_THRUST: f32 = 26.0;
pub const JET_DRAIN: f32 = 6.0;
pub const CAPE_DASH: f32 = 12.0;

/// What a worn construct does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WornKind {
    Wings,
    Parachute,
    Jetpack,
    Cape,
}

pub fn worn_kind(c: &Construct) -> WornKind {
    match c.recipe.form {
        Form::Wings => WornKind::Wings,
        Form::Parachute => WornKind::Parachute,
        Form::Jetpack => WornKind::Jetpack,
        Form::Cape => WornKind::Cape,
        _ if c.has(Mechanic::Thrust) => WornKind::Jetpack,
        _ if c.has(Mechanic::SlowFall) => WornKind::Parachute,
        _ if c.has(Mechanic::Glide) => WornKind::Wings,
        _ => WornKind::Cape,
    }
}

/// Does this construct (worn or held) deploy a parachute / umbrella?
fn chute(c: &Construct) -> bool {
    match c.category {
        FormCategory::Worn => worn_kind(c) == WornKind::Parachute || c.has(Mechanic::SlowFall),
        FormCategory::Held => c.has(Mechanic::SlowFall),
        _ => false,
    }
}

/// [`super::StepFn`] for [`super::Controller::Worn`].
pub fn step(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    c.body.claim(Driver::Attached);
    c.body.loco = Loco::Kinematic;
    let o = env.owner;
    if !o.alive {
        return;
    }
    let dt = env.dt();
    let fwd = {
        let b = Vec3::new(o.look.x, 0.0, o.look.z).normalize_or_zero();
        if b == Vec3::ZERO { Vec3::NEG_Z } else { b }
    };
    c.pos = o.hull.center() - fwd * 0.35;
    c.rot = geom::yaw_rotation(fwd);
    c.vel = o.vel;
    if !c.active() {
        return;
    }
    let mut s: HeldState = c.held;
    if o.grounded {
        s.air_t = 0.0;
    } else {
        s.air_t += dt;
    }
    let can = !o.busy && !o.input.held(buttons::TYPING);
    let jump_held = can && o.input.held(buttons::JUMP);
    // Wing flaps / the cape dash are applied in the movement path ([`pre_move`]) so the client
    // predicts them; here only the visual state.
    let kind = worn_kind(c);
    let active = match kind {
        WornKind::Wings | WornKind::Cape => jump_held && !o.grounded,
        WornKind::Parachute => {
            let deployed = jump_held && !o.grounded;
            if deployed && !s.chute {
                fx(out, c, HeldFx::ChuteOpen, c.pos, Vec3::Y, 1.0);
            }
            s.chute = deployed;
            deployed
        }
        WornKind::Jetpack => {
            if jump_held {
                out.drain.push((c.owner, JET_DRAIN * dt));
                if !s.thrusting {
                    fx(out, c, HeldFx::Thrust, c.pos, Vec3::NEG_Y, 1.0);
                }
            }
            s.thrusting = jump_held;
            jump_held
        }
    };
    c.set_anim(if active { AnimState::Active } else { AnimState::Idle }, env.tick());
    c.held = s;
}

/// Movement modifiers from `owner`'s worn / held / suit constructs. `mods` already holds statuses.
/// Before movement (host `step_movement` and client `predict_player` alike): worn air moves — a
/// wing flap (≤ [`MAX_FLAPS`] per air time) or the cape's one dash — on a fresh Jump press in the
/// air. The counters live on the [`PlayerState`] (reset on the ground) so prediction replays them
/// exactly. `fx` receives the `HeldFx` events (the host forwards them; prediction ignores them).
pub fn pre_move(store: &ConstructStore, owner: PlayerId, input: &PlayerInput, st: &mut PlayerState, busy: bool, mut fx: impl FnMut(GameEvent)) {
    if st.grounded {
        st.air_flaps = 0;
        st.air_dashed = false;
        return;
    }
    let can = !busy && !input.held(buttons::TYPING);
    let pressed = input.buttons & !st.prev_buttons & buttons::JUMP != 0;
    if !(can && pressed && st.air_time > 0.1) {
        return;
    }
    let fwd = {
        let l = input.look_dir();
        let b = Vec3::new(l.x, 0.0, l.z).normalize_or_zero();
        if b == Vec3::ZERO { Vec3::NEG_Z } else { b }
    };
    for c in store.owned_by(owner).filter(|c| c.active() && !c.suit && c.category == FormCategory::Worn) {
        match worn_kind(c) {
            WornKind::Wings if st.air_flaps < MAX_FLAPS => {
                st.air_flaps += 1;
                st.extra.spend_air_jumps(); // AIR JUMP: the flap is this press's air move
                let dv = Vec3::new(0.0, st.vel.y.max(0.0) + FLAP_UP - st.vel.y, 0.0);
                st.apply_impulse(dv, 0.0);
                let strength = 1.0 - 0.2 * (st.air_flaps - 1) as f32;
                fx(GameEvent::HeldFx { id: c.id, owner: c.owner, fx: HeldFx::Flap, pos: c.pos, dir: Vec3::Y, strength });
            }
            WornKind::Cape if !st.air_dashed => {
                st.air_dashed = true;
                st.extra.spend_air_jumps(); // AIR JUMP: the cape dash replaces it
                let dv = fwd * CAPE_DASH + Vec3::Y * 3.0 - Vec3::new(0.0, st.vel.y.min(0.0), 0.0);
                st.apply_impulse(dv, 0.0);
                fx(GameEvent::HeldFx { id: c.id, owner: c.owner, fx: HeldFx::CapeDash, pos: c.pos, dir: fwd, strength: 1.0 });
            }
            // AIR JUMP: Jump in the air belongs to the canopy / jetpack (no hop on the deploy press).
            WornKind::Parachute | WornKind::Jetpack => st.extra.spend_air_jumps(),
            _ => {}
        }
    }
}

pub fn movement_mods(store: &ConstructStore, owner: PlayerId, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
    let jump = input.held(buttons::JUMP) && !input.held(buttons::TYPING);
    let air = !st.grounded;
    let fwd = {
        let l = input.look_dir();
        Vec3::new(l.x, 0.0, l.z).normalize_or_zero()
    };
    let under_glide_cap = st.vel.dot(fwd) < GLIDE_CAP;
    for c in store.owned_by(owner).filter(|c| c.active()) {
        if c.suit {
            super::pilot::suit_movement_mods(c, input, st, mods);
            continue;
        }
        match c.category {
            FormCategory::Worn => match worn_kind(c) {
                WornKind::Wings if jump && air && under_glide_cap => mods.glide = mods.glide.max(1.0),
                WornKind::Cape if jump && air && under_glide_cap => mods.glide = mods.glide.max(0.5),
                WornKind::Parachute if jump && air => slow_fall(mods, CHUTE_FALL),
                WornKind::Jetpack if jump => jet(mods, st),
                _ => {}
            },
            FormCategory::Held => {
                // Umbrella / glider / rocket held items.
                if jump && air && c.has(Mechanic::SlowFall) {
                    slow_fall(mods, CHUTE_FALL);
                }
                if jump && air && c.has(Mechanic::Glide) && under_glide_cap {
                    mods.glide = mods.glide.max(0.8);
                }
                super::held::movement_mods(c, st, mods);
                super::gun::movement_mods(c, input, st, mods);
            }
            _ => {}
        }
    }
}

fn slow_fall(mods: &mut MoveModifiers, v: f32) {
    mods.slow_fall = if mods.slow_fall > 0.0 { mods.slow_fall.min(v) } else { v };
}

/// Jetpack: rise to ~6 m/s, thrust toward the aim / WASD, capped at [`JET_MAX`].
pub(super) fn jet(mods: &mut MoveModifiers, st: &PlayerState) {
    mods.thrust = mods.thrust.max(JET_THRUST);
    mods.thrust_max_speed = JET_MAX;
    let g = if st.vel.y < 6.0 { -0.3 } else { 0.0 };
    mods.gravity_scale = mods.gravity_scale.min(g);
}

/// After `pid` moved: parachute / umbrella tumble-cancel, whip tether, Thor drag.
pub fn post_move(store: &ConstructStore, pid: PlayerId, input: &PlayerInput, st: &mut PlayerState, _pre_vel: Vec3, _ev: &MoveEvents, p: &MoveParams) {
    let jump = input.held(buttons::JUMP) && !input.held(buttons::TYPING);
    for c in store.owned_by(pid).filter(|c| c.active()) {
        if jump && st.tumble > 0.0 && !st.grounded && chute(c) {
            // The ring-out save: the canopy catches you, control returns.
            st.tumble = 0.0;
        }
        if c.category == FormCategory::Held {
            super::held::post_move(c, input, st, p, DT);
        }
    }
}
