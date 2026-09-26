//! World side of the dodge and the air jump (the movement itself: `movement/dodge.rs`,
//! `movement/air_jump.rs`).
//!
//! **What a dodge passes through** (for `dodge_iframes` s from its start, `assets/tuning/move.ron`):
//! attacks aimed at you — ring blast and shove, melee (light, heavy, lunge, rams), shots
//! (projectiles and hitscan), thrown objects, creature attacks, electric chain arcs. **Not**:
//! explosions (area damage — dodge *out* of them), crushes (something heavy landing on you) and
//! wall splats (your own impact). Hook-and-strike: a dodging player can't be hooked and any rope on
//! them breaks ([`crate::hook::hook_breaks_on_dodge`] reads [`crate::movement::ExtraMoves::dodging`];
//! the start tick is also `MoveEvents::dodged` / `move_flags::DODGE`).
//!
//! **Lag compensation**, the same rule as hit detection: a hit credited to another player who has a
//! view tick is judged at that (clamped) view tick — the dodge must already have started on the
//! attacker's screen, just as their rewound hull is the one that counts ("favour the shooter");
//! everything else is judged now.
//!
//! **Hits give back** the air jump and the air dodge (Smash recovery), [`on_hit`].

use bd_core::input::buttons;
use bd_core::voxel::raycast;
use bd_core::{DT, Vec3};

use crate::combat::{HitInfo, HitKind};
use crate::movement::PlayerState;
use crate::rng::SimRng;
use crate::weapons::WeaponPhase;
use crate::world::{Player, World};

/// Does a dodge pass through this kind of hit? (Explosion, Crush, WallSplat and anything new: no.)
pub fn ignores(kind: HitKind) -> bool {
    matches!(
        kind,
        HitKind::RingBlast | HitKind::RingShove | HitKind::Melee | HitKind::HeavyMelee | HitKind::Shot | HitKind::Thrown | HitKind::Creature | HitKind::Chain
    )
}

/// Is player index `ti` intangible to `hit` (see the module docs)?
pub fn intangible(w: &World, ti: usize, hit: &HitInfo) -> bool {
    let Some(p) = w.players.get(ti) else { return false };
    let iframes = w.cfg.move_params.dodge_iframes;
    if !(p.mv.extra.since_dodge < iframes) || !ignores(hit.kind) {
        return false;
    }
    let now = w.tick as f64;
    let at = hit
        .source
        .filter(|s| *s != hit.target)
        .and_then(|s| w.player(s))
        .and_then(|a| a.view_tick)
        .and_then(|v| w.history.clamp_view(v))
        .unwrap_or(now);
    let ago = ((now - at).max(0.0) * DT as f64) as f32;
    p.mv.extra.intangible_at(ago, iframes)
}

/// A hit landed on `mv`: the air jump and the air dodge come back.
#[inline]
pub fn on_hit(mv: &mut PlayerState) {
    mv.extra.restore();
}

// ------------------------------------------------------------------------------------------- bots

/// Per-bot state for [`bot_moves`].
#[derive(Clone, Copy, Debug)]
pub struct BotMoves {
    /// Seconds before reacting to the current threat (< 0: no threat being tracked; huge: ignoring it).
    react: f32,
}

impl Default for BotMoves {
    fn default() -> Self {
        Self { react: -1.0 }
    }
}

/// Bots use the air jump (recovery back toward `last_ground` when falling over a drop and the
/// rescue rope found nothing — `roping`: the bot's own tether rescue is under way; chasing a target
/// above) and the dodge (a telegraphed melee lunge at them, or a charging railgun / bow aimed at
/// them — sometimes, after a reaction time). Adjusts this tick's buttons / WASD; `yaw` is the
/// input's yaw.
#[allow(clippy::too_many_arguments)]
pub fn bot_moves(
    st: &mut BotMoves,
    w: &World,
    me: &Player,
    target: Option<&Player>,
    last_ground: Vec3,
    roping: bool,
    difficulty: f32,
    yaw: f32,
    rng: &mut SimRng,
    b: &mut u16,
    mx: &mut i8,
    mz: &mut i8,
) {
    let mv = &me.mv;
    let (sy, cy) = yaw.sin_cos();
    let (fwd, right) = (Vec3::new(-sy, 0.0, -cy), Vec3::new(cy, 0.0, -sy));
    let keys = |d: Vec3| {
        let (f, r) = (d.dot(fwd), d.dot(right));
        let k = |v: f32| if v > 0.38 { 1 } else if v < -0.38 { -1 } else { 0 };
        (k(r), k(f))
    };
    // A fresh Jump press in the air is the air jump: never start one by accident (auto-hop holds).
    let held_before = me.input.held(buttons::JUMP);
    if !mv.grounded && !held_before {
        *b &= !buttons::JUMP;
    }
    let pos = mv.pos;
    if !mv.grounded && !mv.extra.air_jump_used && !mv.tethered() && mv.tumble <= 0.0 && mv.air_time > 0.2 {
        let over_void = raycast(&w.voxels, pos + Vec3::Y * 0.5, Vec3::NEG_Y, 20.0).is_none();
        // The rope first (bots.rs looks for an anchor once falling faster than 6 m/s), then this.
        let recover = over_void && mv.vel.y < -6.0 && !roping;
        let chase = difficulty > 0.3
            && mv.vel.y < 2.0
            && target.is_some_and(|t| t.mv.pos.y > pos.y + 2.0 && Vec3::new(t.mv.pos.x - pos.x, 0.0, t.mv.pos.z - pos.z).length() < 12.0);
        if recover || chase {
            if held_before {
                *b &= !buttons::JUMP; // release first; press next tick
            } else {
                *b |= buttons::JUMP;
                if recover {
                    (*mx, *mz) = keys(Vec3::new(last_ground.x - pos.x, 0.0, last_ground.z - pos.z).normalize_or_zero());
                }
            }
        }
    }
    // Dodge a telegraphed attack (once per threat, if the roll says this bot notices).
    let Some(from) = threat(w, me) else {
        st.react = -1.0;
        return;
    };
    if st.react < 0.0 {
        st.react = if rng.chance(0.3 + 0.5 * difficulty.clamp(0.0, 1.0)) { rng.range(0.12, 0.3) } else { 99.0 };
    }
    st.react -= DT;
    let air = !mv.grounded;
    if st.react > 0.0 || mv.extra.dodge_cd > 0.0 || (air && mv.extra.air_dodge_used) || mv.tumble > 0.0 {
        return;
    }
    let side = Vec3::new(-from.z, 0.0, from.x).normalize_or_zero();
    let safe = |d: Vec3| air || [2.0f32, 4.5].iter().all(|k| raycast(&w.voxels, pos + d * *k + Vec3::Y * 0.5, Vec3::NEG_Y, 4.0).is_some());
    let first = if rng.chance(0.5) { side } else { -side };
    let Some(d) = [first, -first].into_iter().find(|d| *d != Vec3::ZERO && safe(*d)) else {
        st.react = 99.0;
        return;
    };
    (*mx, *mz) = keys(d);
    if (*mx, *mz) != (0, 0) {
        *b |= buttons::DODGE;
    }
    st.react = 99.0;
}

/// Direction from an attack being telegraphed at `me` toward `me` (a lunge closing on them, or a
/// charging railgun / bow aimed at them), if any.
fn threat(w: &World, me: &Player) -> Option<Vec3> {
    let params = w.params();
    let chest = me.hull(params).center();
    for q in w.players.iter().filter(|q| q.alive && q.id != me.id) {
        if q.mv.lunge.active() && (q.mv.lunge.to - chest).length() < 3.5 {
            return Some(Vec3::new(chest.x - q.mv.pos.x, 0.0, chest.z - q.mv.pos.z).normalize_or_zero());
        }
        let charging = w.constructs.held_by(q.id).and_then(|c| c.weapon).is_some_and(|ws| ws.phase == WeaponPhase::Charging && ws.charge > 0.35);
        if charging {
            let to = chest - q.eye(params);
            let d = to.length();
            if d > 0.5 && d < 90.0 && q.input.look_dir().dot(to / d) > 0.99 {
                return Some(Vec3::new(to.x, 0.0, to.z).normalize_or_zero());
            }
        }
    }
    None
}
