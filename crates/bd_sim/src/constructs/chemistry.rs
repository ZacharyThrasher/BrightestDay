//! **Element chemistry** (COMBAT.md §9) — construct engineer.
//!
//! Terrain: fire ignites Leaves / Wood voxels; burning spreads through neighbouring leaves / wood
//! by a deterministic cellular step at 10 Hz (hash-seeded chances, ≤ [`MAX_BURNING`] cells, ≤
//! [`MAX_NEW_PER_STEP`] new cells per step); burnt-out voxels become carves (ember debris on
//! clients); players standing in flames catch Burn; Water / Ice constructs put fires out.
//!
//! Constructs (20 Hz pair pass over the few elemental constructs, fixed-size index lists):
//! * Ice on a fire construct/creature: fire snuffed, it freezes solid and turns brittle (next hit
//!   shatters it — `counters::on_damage`). Ice also freezes Water constructs.
//! * Water on fire: hisses out (snuffed); on a bomb/Explodes object: a dud.
//! * Electric touching Water (or a soaked construct): the water is electrified — current arcs
//!   through everything near it (2× chain reach), *including the water's owner*.
//! * Fire touching a flammable animated-terrain construct (a tree with legs): it catches fire and
//!   burns (integrity damage over time) — and sets leaves it touches alight.
//! * Wind pushes light constructs and swarms; Heavy things ignore it.
//!
//! Cost: nothing when no fire burns and no elemental constructs exist; otherwise bounded by the
//! caps above (a full 192-cell fire step is ~1.5k voxel reads at 10 Hz).

use bd_core::mechanics::{Element, FormCategory, Mechanic};
use bd_core::voxel::VoxelQuery;
use bd_core::{IVec3, PlayerId, VOXEL_SIZE, Vec3};
use bd_voxel::{DebrisClass, VoxelWorld, material_def};

use super::components::{eff_has, on_fire};
use super::{Construct, ConstructCtx, ConstructOut};
use crate::combat::{CarveRequest, ConstructHit, HitInfo, HitKind};
use crate::status::StatusKind;

/// Max simultaneously burning voxels.
pub const MAX_BURNING: usize = 192;
/// Max new ignitions per 10 Hz step (spread) or per ignite call.
pub const MAX_NEW_PER_STEP: usize = 16;
/// Ticks between fire steps (10 Hz at 120 Hz).
const FIRE_TICKS: u64 = 12;
/// Ticks between construct pair passes (20 Hz).
const PAIR_TICKS: u64 = 6;
/// Max elemental constructs considered per pair pass.
const MAX_ELEMENTAL: usize = 48;

/// One burning voxel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burn {
    pub v: IVec3,
    /// Steps burnt so far / steps until it burns away.
    pub age: u8,
    pub life: u8,
    pub by: Option<PlayerId>,
}

#[inline]
fn key(v: IVec3) -> (i32, i32, i32) {
    (v.x, v.y, v.z)
}

/// Terrain chemistry state. Buffers are reserved once (no per-tick allocation).
#[derive(Clone, Debug, Default)]
pub struct Chemistry {
    /// Sorted by voxel coordinate (binary search for "already burning?").
    burning: Vec<Burn>,
    fresh: Vec<Burn>,
}

/// Leaves burn fast, wood slowly; everything else doesn't burn.
fn flammable(m: u8) -> Option<u8> {
    if m == 0 {
        return None;
    }
    match material_def(m).debris {
        DebrisClass::Leaves => Some(5),
        DebrisClass::Wood => Some(18),
        _ => None,
    }
}

#[inline]
fn hash01(v: IVec3, salt: u64) -> f32 {
    let mut z = (v.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (v.y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ (v.z as u64).wrapping_mul(0x94D0_49BB_1331_11EB) ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

impl Chemistry {
    pub fn clear(&mut self) {
        self.burning.clear();
        self.fresh.clear();
    }

    /// Burning voxels (sorted).
    pub fn burning(&self) -> &[Burn] {
        &self.burning
    }

    pub fn is_burning(&self, v: IVec3) -> bool {
        self.burning.binary_search_by_key(&key(v), |b| key(b.v)).is_ok()
    }

    fn reserve(&mut self) {
        if self.burning.capacity() < MAX_BURNING {
            self.burning.reserve_exact(MAX_BURNING - self.burning.len());
        }
        if self.fresh.capacity() < MAX_NEW_PER_STEP * 2 {
            self.fresh.reserve_exact(MAX_NEW_PER_STEP * 2);
        }
    }

    fn insert(&mut self, b: Burn) -> bool {
        if self.burning.len() >= MAX_BURNING {
            return false;
        }
        match self.burning.binary_search_by_key(&key(b.v), |x| key(x.v)) {
            Ok(_) => false,
            Err(i) => {
                self.burning.insert(i, b);
                true
            }
        }
    }

    /// Set Leaves / Wood alight within `radius` of `pos` (≤ 2.5 m, ≤ [`MAX_NEW_PER_STEP`] voxels).
    pub fn ignite_area(&mut self, voxels: &VoxelWorld, pos: Vec3, radius: f32, by: Option<PlayerId>) {
        if !pos.is_finite() || !radius.is_finite() {
            return;
        }
        self.reserve();
        let r = radius.clamp(0.25, 2.5);
        let c = (pos / VOXEL_SIZE).floor().as_ivec3();
        let n = (r / VOXEL_SIZE).ceil() as i32;
        let mut added = 0;
        for y in -n..=n {
            for z in -n..=n {
                for x in -n..=n {
                    if added >= MAX_NEW_PER_STEP {
                        return;
                    }
                    let v = c + IVec3::new(x, y, z);
                    let d = (v.as_vec3() + Vec3::splat(0.5)) * VOXEL_SIZE - pos;
                    if d.length() > r + VOXEL_SIZE * 0.5 {
                        continue;
                    }
                    if let Some(life) = flammable(voxels.material(v)) {
                        if self.insert(Burn { v, age: 0, life, by }) {
                            added += 1;
                        }
                    }
                }
            }
        }
    }

    /// Put out fires within `radius` of `pos`.
    pub fn douse(&mut self, pos: Vec3, radius: f32) {
        let r2 = radius * radius;
        self.burning.retain(|b| ((b.v.as_vec3() + Vec3::splat(0.5)) * VOXEL_SIZE - pos).length_squared() > r2);
    }

    /// One 10 Hz cellular step: age, spread, burn away.
    fn fire_step(&mut self, voxels: &VoxelWorld, tick: u64, out: &mut ConstructOut) {
        self.fresh.clear();
        let mut i = 0;
        while i < self.burning.len() {
            let b = self.burning[i];
            let life = flammable(voxels.material(b.v));
            if life.is_none() {
                // Carved away by something else.
                self.burning.remove(i);
                continue;
            }
            let age = b.age.saturating_add(1);
            if age >= b.life {
                let center = (b.v.as_vec3() + Vec3::splat(0.5)) * VOXEL_SIZE;
                out.carves.push(CarveRequest { center, radius: 0.3, power: 1.0 });
                self.burning.remove(i);
                continue;
            }
            self.burning[i].age = age;
            // Spread (upward spreads best, like real fire).
            if self.fresh.len() < MAX_NEW_PER_STEP {
                for (k, d) in [IVec3::Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::NEG_Y].iter().enumerate() {
                    let n = b.v + *d;
                    let Some(nl) = flammable(voxels.material(n)) else { continue };
                    let chance = match (k, nl) {
                        (0, _) => 0.55,
                        (_, l) if l <= 6 => 0.35,
                        _ => 0.12,
                    };
                    if hash01(n, tick ^ (k as u64) << 8) < chance && !self.is_burning(n) && !self.fresh.iter().any(|f| f.v == n) {
                        self.fresh.push(Burn { v: n, age: 0, life: nl, by: b.by });
                        if self.fresh.len() >= MAX_NEW_PER_STEP {
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
        for k in 0..self.fresh.len() {
            let b = self.fresh[k];
            self.insert(b);
        }
        self.fresh.clear();
    }
}

/// One chemistry step (after all constructs moved). Bounded; see module docs.
pub fn step(chem: &mut Chemistry, items: &mut [Construct], ctx: &ConstructCtx, out: &mut ConstructOut) {
    let tick = ctx.tick;
    if !chem.burning.is_empty() && tick % FIRE_TICKS == 0 {
        chem.fire_step(ctx.voxels, tick, out);
        // Players standing in flames catch fire.
        for p in ctx.players {
            if !p.alive || p.shielded {
                continue;
            }
            let hull = crate::geom::inflate(&p.hull, 0.6);
            let (lo, hi) = hull.voxel_range();
            if let Some(b) = chem.burning.iter().find(|b| b.v.cmpge(lo).all() && b.v.cmple(hi).all()) {
                out.statuses.push((p.id, StatusKind::Burn, 1.0, b.by));
            }
        }
    }
    if tick % PAIR_TICKS != 0 || items.is_empty() {
        return;
    }
    // Elemental constructs (fixed-size index list).
    let mut idx = [0u16; MAX_ELEMENTAL];
    let mut n = 0;
    for (i, c) in items.iter().enumerate() {
        if n >= MAX_ELEMENTAL {
            break;
        }
        if !c.active() || c.attached() {
            continue;
        }
        let e = c.recipe.element;
        let relevant = matches!(e, Element::Fire | Element::Ice | Element::Water | Element::Electric | Element::Wind | Element::Atomic)
            || c.comp.burning > 0.0
            || c.comp.wet > 0.0
            || c.comp.electrified > 0.0
            || c.model.is_some()
            || eff_has(c, Mechanic::Explodes);
        if relevant {
            idx[n] = i as u16;
            n += 1;
        }
    }
    let dt = ctx.dt * PAIR_TICKS as f32;
    // Water / ice put out terrain fires around them.
    if !chem.burning.is_empty() {
        for &i in &idx[..n] {
            let c = &items[i as usize];
            if matches!(c.recipe.element, Element::Water | Element::Ice) {
                chem.douse(c.center(), c.half.max_element() + 2.0);
            }
        }
    }
    for a in 0..n {
        for b in 0..n {
            if a == b {
                continue;
            }
            let (ia, ib) = (idx[a] as usize, idx[b] as usize);
            let (ca, cb) = (&items[ia], &items[ib]);
            if !crate::geom::inflate(&ca.bounds(), 0.4).intersects(&cb.bounds()) {
                continue;
            }
            react_pair(items, ia, ib, dt);
        }
    }
    // Electrified water arcs; burning constructs smoulder; wind pushes.
    for &i in &idx[..n] {
        let i = i as usize;
        let c = &items[i];
        if c.comp.electrified > 0.0 && c.comp.shock_cd <= 0.0 {
            let reach = crate::geom::inflate(&c.bounds(), 4.0);
            let by = c.comp.shock_by;
            for p in ctx.players {
                if !p.alive || p.shielded || Some(p.id) == by || !reach.intersects(&p.hull) {
                    continue;
                }
                out.player_hits.push(
                    HitInfo::new(p.id, by, HitKind::Chain, 5.0, 3.0, p.hull.center() - c.center(), p.hull.center()).with_element(Element::Electric).with_construct(c.id).with_impact(0.6),
                );
            }
            items[i].comp.shock_cd = 0.3;
        }
        let c = &items[i];
        if c.comp.burning > 0.0 && c.recipe.element != Element::Fire {
            out.construct_hits.push(ConstructHit { target: c.id, source: None, damage: 4.0 * dt, impulse: Vec3::ZERO, blade: false, point: c.center() });
        }
        if c.recipe.element == Element::Wind && c.vel.length_squared() > 4.0 {
            let (pos, dir, owner) = (c.center(), c.vel.normalize_or_zero(), c.owner);
            for o in items.iter_mut() {
                if o.owner == owner || !o.active() || o.attached() || eff_has(o, Mechanic::Heavy) || o.mass() > 40.0 || o.category == FormCategory::Structure && !o.body.loose {
                    continue;
                }
                if (o.center() - pos).length_squared() < 25.0 {
                    o.vel += dir * (12.0 * dt);
                }
            }
        }
    }
}

/// Is this animated-terrain construct made of leaves / wood?
fn flammable_model(c: &Construct) -> bool {
    c.model.as_ref().is_some_and(|m| m.voxels.iter().take(64).filter(|(_, mat)| flammable(*mat).is_some()).count() >= 8)
}

/// `a` acts on `b` (called for both orders).
fn react_pair(items: &mut [Construct], a: usize, b: usize, dt: f32) {
    let _ = dt;
    let ea = items[a].recipe.element;
    let a_fire = on_fire(&items[a]);
    let b_fire = on_fire(&items[b]);
    let a_owner = items[a].owner;
    let b_wet = items[b].recipe.element == Element::Water || items[b].comp.wet > 0.0;
    let cb = &mut items[b];
    match ea {
        Element::Ice => {
            if b_fire || cb.recipe.element == Element::Water {
                // Fire snuffed; frozen solid and brittle.
                cb.comp.frozen = cb.comp.frozen.max(if b_fire { 4.0 } else { 3.0 });
                cb.comp.snuffed = true;
                cb.comp.burning = 0.0;
            }
        }
        Element::Water => {
            if b_fire {
                cb.comp.snuffed = true;
                cb.comp.burning = 0.0;
                cb.comp.wet = 3.0;
            }
            if eff_has(cb, Mechanic::Explodes) && cb.category == FormCategory::Object {
                cb.comp.dud = true;
            }
        }
        Element::Electric => {
            if b_wet && cb.comp.electrified <= 0.0 {
                cb.comp.electrified = 1.5;
                cb.comp.shock_by = Some(a_owner);
            }
        }
        Element::Fire | Element::Atomic if a_fire => {
            if flammable_model(cb) && cb.comp.frozen <= 0.0 && cb.comp.wet <= 0.0 {
                cb.comp.burning = cb.comp.burning.max(5.0);
            }
        }
        _ => {}
    }
}
