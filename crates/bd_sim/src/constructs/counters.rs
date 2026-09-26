//! **Clash** (COMBAT.md §3) and **qualitative counters** (§8) — construct engineer.
//!
//! The §8 table is data ([`COUNTERS`]) — each row names *what happens*, and the code below makes
//! it happen. Outcomes are qualitative, never multipliers:
//!
//! | your construct | against | what happens | where |
//! |---|---|---|---|
//! | Blade | Structure / Barrier / Cage | sliced in two along the cut; both halves fall and topple | [`on_damage`] → [`slice`] (any Blade hit: held swings, thrown axes, blade constructs) |
//! | Charge / Heavy (moving) | Wall / Barrier | smashes through: the wall bursts into debris along the ram | [`clash`] |
//! | Cage / Trap | Creature / Kaiju | rooted in place, thrashing; a kaiju cage topples it | `components` contact → `Interaction::Root` |
//! | Heavy / DropFromSky | Flies | slammed out of the sky and pinned | `components` contact + [`clash`] |
//! | Ice | Fire construct | snuffed, frozen solid, brittle: the next hit shatters it | `chemistry` + [`on_damage`] |
//! | Water | Fire / Explodes | hisses out / a dud | `chemistry` |
//! | Electric | Water | arcs through everything wet, including the water's owner | `chemistry` |
//! | Guard / Barrier | Beam | bent off along the shield's facing (redirected) | `components::fire_beam` |
//! | Grab / Constrict | Creature | carried and steered, then thrown | `components` holds |
//! | anything | Structure (big blow) | knocked loose: falls, topples onto whoever is below | [`on_damage`] |
//!
//! Clash: two opposing live constructs in contact exchange integrity damage by relative momentum
//! (the heavier / faster side hurts the other more), both flash (`GameEvent::Clash`); two
//! swinging held weapons = sword-lock (both wielders knocked back a little).

use bd_core::mechanics::{Form, FormCategory, Mechanic};
use bd_core::{ConstructId, PlayerId, Vec3};

use super::components::{self, eff_has, is_blade};
use super::{AnimState, Construct, ConstructCtx, ConstructOut, ConstructStore, Loco, SpawnParams, body};
use crate::combat::ConstructHit;
use crate::events::{GameEvent, GoneReason};
use crate::geom;

/// A §8 row: (attacker trait, defender kind, outcome). Documentation-as-data; the table above is
/// the human view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Slice,
    SmashThrough,
    Root,
    Pin,
    FreezeBrittle,
    Snuff,
    Arc,
    Redirect,
    Steer,
    KnockLoose,
}

pub const COUNTERS: &[(&str, &str, Outcome)] = &[
    ("Blade", "Structure/Barrier/Cage", Outcome::Slice),
    ("Charge/Heavy", "Wall/Barrier", Outcome::SmashThrough),
    ("Cage/Trap", "Creature/Kaiju", Outcome::Root),
    ("Heavy/DropFromSky", "Flies", Outcome::Pin),
    ("Ice", "Fire", Outcome::FreezeBrittle),
    ("Water", "Fire/Explodes", Outcome::Snuff),
    ("Electric", "Water", Outcome::Arc),
    ("Guard/Barrier", "Beam", Outcome::Redirect),
    ("Grab/Constrict", "Creature", Outcome::Steer),
    ("any big blow", "Structure", Outcome::KnockLoose),
];

/// Impulse (m/s·kg-ish, as carried by `ConstructHit::impulse`) that knocks a structure loose.
const LOOSEN_IMPULSE: f32 = 14.0;

fn live_for_clash(c: &Construct) -> bool {
    match c.category {
        FormCategory::Worn => false,
        FormCategory::Held => c.anim_state == AnimState::Active,
        _ => c.comp.attached_to.is_none(),
    }
}

fn wall_like(c: &Construct) -> bool {
    c.category == FormCategory::Structure && (matches!(c.recipe.form, Form::Wall | Form::Dome | Form::Cage) || eff_has(c, Mechanic::Barrier))
}

fn rams(c: &Construct) -> bool {
    let speed = c.vel.length();
    (c.comp.charge == components::ChargePhase::Dash && speed > 6.0) || (eff_has(c, Mechanic::Heavy) && speed > 8.0)
}

/// Pairwise clash between opposing constructs (after all moved). O(n²) AABB rejects, n ≤ 256.
pub fn clash(items: &mut [Construct], ctx: &ConstructCtx, out: &mut ConstructOut) {
    let n = items.len();
    for i in 0..n {
        if !items[i].active() || items[i].clash_cd > 0.0 || !live_for_clash(&items[i]) {
            continue;
        }
        let bi = items[i].bounds();
        for j in (i + 1)..n {
            let (lo, hi) = items.split_at_mut(j);
            let a = &mut lo[i];
            let b = &mut hi[0];
            if a.owner == b.owner || !b.active() || b.clash_cd > 0.0 || a.clash_cd > 0.0 || !live_for_clash(b) {
                continue;
            }
            if !bi.intersects(&b.bounds()) {
                continue;
            }
            let rel = (a.vel - b.vel).length();
            let swinging = a.category == FormCategory::Held || b.category == FormCategory::Held;
            if rel < 3.0 && !swinging {
                continue;
            }
            // Narrow phase: either centre within the other's inflated box.
            let ra = a.half.min_element().max(0.2);
            let rb = b.half.min_element().max(0.2);
            if !geom::sphere_obb(a.center(), ra + b.half.max_element() * 0.2, b.center(), b.half, b.rot)
                && !geom::sphere_obb(b.center(), rb + a.half.max_element() * 0.2, a.center(), a.half, a.rot)
            {
                continue;
            }
            let point = (a.center() + b.center()) * 0.5;
            let nrm = (a.center() - b.center()).normalize_or(Vec3::Y);
            let (ma, mb) = (body::body_mass(a).max(1.0), body::body_mass(b).max(1.0));
            // Relative momentum split: the heavier side hurts the lighter one more.
            let base = (rel * 1.2 + 4.0).min(60.0);
            let to_a = (base * (2.0 * mb / (ma + mb))).clamp(2.0, 80.0);
            let to_b = (base * (2.0 * ma / (ma + mb))).clamp(2.0, 80.0);
            a.clash_cd = 0.3;
            b.clash_cd = 0.3;
            // Qualitative: a ram smashes clean through a wall.
            let smash_b = rams(a) && wall_like(b);
            let smash_a = rams(b) && wall_like(a);
            let dmg_b = if smash_b { b.integrity + b.max_integrity + 1.0 } else { to_b };
            let dmg_a = if smash_a { a.integrity + a.max_integrity + 1.0 } else { to_a };
            let imp_b = if smash_b { a.vel } else { -nrm * rel * 0.5 };
            let imp_a = if smash_a { b.vel } else { nrm * rel * 0.5 };
            out.construct_hits.push(ConstructHit { pierce: components::is_pierce(b), blunt: components::is_blunt(b), target: a.id, source: Some(b.owner), damage: dmg_a, impulse: imp_a, blade: is_blade(b), point });
            out.construct_hits.push(ConstructHit { pierce: components::is_pierce(a), blunt: components::is_blunt(a), target: b.id, source: Some(a.owner), damage: dmg_b, impulse: imp_b, blade: is_blade(a), point });
            // Heavy / DropFromSky on a flier: pinned.
            if (eff_has(a, Mechanic::Heavy) || eff_has(a, Mechanic::DropFromSky)) && eff_has(b, Mechanic::Flies) && a.vel.y < -4.0 {
                b.comp.pinned = b.comp.pinned.max(1.5);
            }
            if (eff_has(b, Mechanic::Heavy) || eff_has(b, Mechanic::DropFromSky)) && eff_has(a, Mechanic::Flies) && b.vel.y < -4.0 {
                a.comp.pinned = a.comp.pinned.max(1.5);
            }
            if a.category == FormCategory::Held && b.category == FormCategory::Held {
                // Sword-lock: both wielders knocked back a little.
                out.impulses.push((a.owner, nrm * 6.0 + Vec3::Y * 2.0, 0.0));
                out.impulses.push((b.owner, -nrm * 6.0 + Vec3::Y * 2.0, 0.0));
            }
            out.events.push(GameEvent::Clash { a: a.id, b: b.id, point, strength: base });
        }
    }
    let _ = ctx;
}

/// Weak points (joints, cockpit, nape) take this much from Blade (COMBAT.md §5d).
pub const WEAK_POINT_MULT: f32 = 3.0;

/// Is `p` on one of `c`'s weak points? Creatures (incl. kaiju) and piloted constructs have them:
/// the nape (top rear), the cockpit/seat (top centre) and the knee joints (low, at the sides).
/// Approximated from the collision box (the sim has no bd_shapes rig), in the body's local frame
/// (−Z forward).
pub fn weak_point(c: &Construct, p: Vec3) -> bool {
    if !(c.category == FormCategory::Creature || c.pilot.is_some() || c.suit) || !p.is_finite() {
        return false;
    }
    let l = c.rot.inverse() * (p - c.center());
    let h = c.half;
    let nape_or_seat = l.y > h.y * 0.55 && l.z > -h.z * 0.25;
    let knees = l.y < -h.y * 0.3 && l.y > -h.y * 0.85 && l.x.abs() > h.x * 0.35;
    nape_or_seat || knees
}

/// Integrity damage + counters. Returns true if the target shattered (or was sliced away).
pub fn on_damage(store: &mut ConstructStore, hit: &ConstructHit, events: &mut Vec<GameEvent>) -> bool {
    let Some(c) = store.get_mut(hit.target) else { return false };
    if c.gone.is_some() || !hit.damage.is_finite() {
        return false;
    }
    // Blade and Pierce find weak points (joints, cockpit, nape). GIANTS: their own anatomy decides
    // (weak points for every hit, limbs, stagger / topple — `giant::damage`).
    let dmg = if c.ai.giant.on {
        let t = store.giants;
        let Some(c) = store.get_mut(hit.target) else { return false };
        super::giant::damage(c, hit, hit.damage.max(0.0) * if hit.blade { 2.0 } else { 1.0 }, &t, events)
    } else {
        let weak = (hit.blade || hit.pierce) && weak_point(c, hit.point);
        hit.damage.max(0.0) * if hit.blade { 2.0 } else { 1.0 } * if weak { WEAK_POINT_MULT } else { 1.0 }
    };
    let Some(c) = store.get_mut(hit.target) else { return false };
    events.push(GameEvent::ConstructHit { id: c.id, source: hit.source, damage: dmg, point: hit.point });
    let impulse = if hit.impulse.is_finite() { hit.impulse } else { Vec3::ZERO };
    // Frozen = brittle: the next hit shatters it.
    if components::is_frozen(c) && dmg >= 1.0 {
        c.integrity = 0.0;
        c.gone = Some(GoneReason::Spent);
        c.shatter = Some(impulse * 2.0);
        return true;
    }
    // Blade through a structure: two halves fall.
    let sliceable = c.category == FormCategory::Structure && !matches!(c.recipe.form, Form::Trampoline | Form::Dome) && c.half.x.max(c.half.z) >= 0.6 && c.parent.is_none();
    if hit.blade && sliceable && dmg >= 4.0 && c.integrity - dmg > 0.0 {
        let n = c.rot * Vec3::X;
        let id = c.id;
        return slice(store, id, hit.point, n, hit.source, events);
    }
    c.integrity -= dmg;
    if !c.attached() {
        if c.category == FormCategory::Structure {
            // A big blow knocks a structure loose (build a wall, knock it onto someone).
            if impulse.length() >= LOOSEN_IMPULSE && c.integrity > 0.0 {
                let axis = Vec3::Y.cross(impulse).normalize_or_zero();
                components::loosen(c, axis * 1.5);
                if c.body.loose {
                    c.vel += impulse / c.mass().max(1.0).sqrt().max(1.0) * 0.3;
                }
            }
        } else {
            c.vel += impulse / c.mass().max(1.0).sqrt().max(1.0);
        }
    }
    if c.integrity <= 0.0 {
        c.integrity = 0.0;
        c.gone = Some(GoneReason::Spent);
        c.shatter = Some(impulse);
        return true;
    }
    false
}

/// Slice structure `id` in two along the plane through `point` with normal `normal` (its long
/// axis): the original dissolves (`Spent`) and two half-width loose halves (children) fall apart,
/// toppling away from the cut. Returns true (the original is gone).
pub fn slice(store: &mut ConstructStore, id: ConstructId, point: Vec3, normal: Vec3, by: Option<PlayerId>, events: &mut Vec<GameEvent>) -> bool {
    let _ = by;
    let Some(src) = store.get(id).cloned() else { return false };
    if src.gone.is_some() || src.parent.is_some() {
        return false;
    }
    // Cut along the local axis closest to `normal` among X / Z (walls: across their width).
    let inv = src.rot.inverse();
    let ln = inv * normal;
    let axis = if ln.x.abs() >= ln.z.abs() { Vec3::X } else { Vec3::Z };
    let half_len = if axis == Vec3::X { src.half.x } else { src.half.z };
    if half_len < 0.3 {
        return false;
    }
    // Where along the axis the cut is (clamped so neither half is a sliver).
    let lp = inv * (point - src.center());
    let cut = (if axis == Vec3::X { lp.x } else { lp.z }).clamp(-half_len * 0.5, half_len * 0.5);
    let tick = src.anim_since;
    let mut spawned = 0;
    for side in [-1.0f32, 1.0] {
        let (lo, hi) = if side < 0.0 { (-half_len, cut) } else { (cut, half_len) };
        let len = (hi - lo).max(0.2);
        let mid = (lo + hi) * 0.5;
        let mut half = src.half;
        if axis == Vec3::X {
            half.x = len * 0.5;
        } else {
            half.z = len * 0.5;
        }
        let center = src.center() + src.rot * (axis * mid);
        let pivot = center - src.rot * Vec3::new(0.0, src.half.y, 0.0);
        let away = (src.rot * axis) * side;
        let mut recipe = src.recipe.clone();
        recipe.scale *= (len / (2.0 * half_len)).clamp(0.2, 1.0);
        let params = SpawnParams {
            owner: src.owner,
            recipe,
            shape: None,
            pos: pivot,
            rot: src.rot,
            vel: src.vel + away * 1.5,
            target: src.target,
            cost: 0.0,
            integrity: (src.integrity * 0.5).max(5.0),
            forming: 0.0,
            life: 4.0,
            tick,
            colossal: false,
            model: src.model.clone(),
        };
        // Terrain-model halves carry exactly their own voxels.
        let model = src.model.as_ref().map(|m| {
            let vs = bd_core::VOXEL_SIZE;
            let (pm_half, pm_lc) = m.body_box();
            let _ = pm_half;
            let mut vox: Vec<(bd_core::IVec3, bd_core::voxel::Material)> = Vec::new();
            for (v, mat) in &m.voxels {
                // Voxel centre relative to the parent's collider centre (model local frame).
                let lp = ((*v - m.min).as_vec3() + Vec3::splat(0.5)) * vs - (m.max - m.min + bd_core::IVec3::ONE).as_vec3() * vs * 0.5;
                let coord = if axis == Vec3::X { lp.x } else { lp.z };
                if (side < 0.0 && coord <= cut) || (side > 0.0 && coord > cut) {
                    vox.push((*v, *mat));
                }
            }
            let _ = pm_lc;
            let mut lo = bd_core::IVec3::MAX;
            let mut hi = bd_core::IVec3::MIN;
            for (v, _) in &vox {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
            if vox.is_empty() {
                lo = m.min;
                hi = m.min;
            }
            std::sync::Arc::new(super::VoxelModel { origin: m.origin, voxels: vox, min: lo, max: hi })
        });
        let cut_info = super::SliceCut {
            parent: id,
            axis: if axis == Vec3::X { 0 } else { 2 },
            side: if side < 0.0 { -1 } else { 1 },
            cut,
            parent_half: src.half,
            parent_scale: src.recipe.scale,
            offset: axis * mid,
        };
        if let Some(cid) = store.spawn_child(id, params, events) {
            spawned += 1;
            events.push(GameEvent::ConstructSliced { id: cid, parent: id, cut: cut_info });
            if let Some(c) = store.get_mut(cid) {
                c.slice = Some(cut_info);
                if model.is_some() {
                    c.model = model;
                }
                c.half = half;
                c.local_center = Vec3::new(0.0, half.y, 0.0);
                c.life = c.age + 4.0;
                c.body.loose = true;
                c.body.loco = Loco::Ballistic;
                // Topple away from the cut.
                c.body.spin = Vec3::Y.cross(away).normalize_or_zero() * 1.6;
                c.anim_state = AnimState::Move;
            }
        }
    }
    if spawned == 0 {
        return false;
    }
    if let Some(c) = store.get_mut(id) {
        c.gone = Some(GoneReason::Spent);
        c.integrity = 0.0;
    }
    true
}
